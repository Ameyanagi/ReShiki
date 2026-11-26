//! Read and write Cartesian Hessians in ORCA `.hess` files.
use std::fmt::Write;

use crate::qcdata::ANGSTROM_PER_BOHR;

use super::{Error, Frame, atoms, check_atoms};

const GEOMETRY_TOLERANCE_ANGSTROM: f64 = 1e-3;
const SYMMETRY_ABSOLUTE_TOLERANCE: f64 = 1e-6;
const SYMMETRY_RELATIVE_TOLERANCE: f64 = 1e-6;

/// Read the `$hessian` matrix in Hartree/Bohr², in row-major order.
///
/// Its dimension must be three times the atom count in `expected_frame`. The
/// required `$atoms` section must have the same ordered elements and Cartesian
/// coordinates, compared in the original frame after converting Bohr to Å with
/// an absolute tolerance of 1e-3 Å per coordinate. Masses must be positive and
/// finite, but may differ from the expected frame's isotopic masses.
///
/// Matrix entries must satisfy `|H_ij - H_ji| <= 1e-6 +
/// 1e-6 * max(|H_ij|, |H_ji|)`; accepted numerical noise is preserved.
pub fn read_orca_hess(text: &str, expected_frame: &Frame) -> Result<Vec<f64>, Error> {
    check_atoms(expected_frame)?;
    let expected_atom_count = expected_frame.numbers.len();
    let expected_dimension = expected_atom_count
        .checked_mul(3)
        .filter(|&dimension| dimension != 0)
        .ok_or_else(|| Error::Invalid("The expected Hessian atom count is invalid.".into()))?;
    let mut sections = text
        .lines()
        .enumerate()
        .filter(|(_, line)| line.trim().eq_ignore_ascii_case("$hessian"));
    let Some((section_index, _)) = sections.next() else {
        return Err(Error::Invalid(
            "The ORCA file has no $hessian section.".into(),
        ));
    };
    if let Some((index, _)) = sections.next() {
        return Err(hessian_error(index + 1, "Repeated $hessian section."));
    }
    let mut lines = text
        .lines()
        .enumerate()
        .skip(section_index + 1)
        .map(|(index, line)| (index + 1, line.trim()))
        .take_while(|(_, line)| !line.starts_with('$'))
        .filter(|(_, line)| !line.is_empty());
    let (line_number, line) = next_hessian_line(&mut lines, "the dimension")?;
    let dimension = line
        .parse::<usize>()
        .ok()
        .filter(|&dimension| dimension != 0)
        .ok_or_else(|| hessian_error(line_number, "Expected one positive matrix dimension."))?;
    if dimension != expected_dimension {
        return Err(hessian_error(
            line_number,
            format!(
                "The dimension is {dimension}; expected {expected_dimension} for {expected_atom_count} atoms."
            ),
        ));
    }
    let cell_count = dimension
        .checked_pow(2)
        .ok_or_else(|| hessian_error(line_number, "The Hessian matrix dimension is too large."))?;
    // Every matrix value needs at least one byte. Check this lower bound before
    // allocating, so a large dimension in a short, truncated file stays cheap.
    let section_bytes = lines.clone().map(|(_, line)| line.len()).sum::<usize>();
    if cell_count > section_bytes {
        return Err(Error::Invalid(format!(
            "The ORCA $hessian section is truncated: expected {cell_count} matrix values."
        )));
    }
    let mut hessian = Vec::new();
    hessian
        .try_reserve_exact(cell_count)
        .map_err(|_| Error::Invalid("The ORCA Hessian matrix is too large to read.".into()))?;
    hessian.resize(cell_count, 0.0);
    let mut seen_columns = vec![false; dimension];
    let mut seen_rows = vec![false; dimension];
    let mut column_count = 0;
    while let Some((line_number, header)) = lines.next() {
        let mut columns = Vec::new();
        for field in header.split_whitespace() {
            let column = field.parse::<usize>().map_err(|_| {
                hessian_error(line_number, format!("Invalid column index {field:?}."))
            })?;
            let seen = seen_columns.get_mut(column).ok_or_else(|| {
                hessian_error(
                    line_number,
                    format!("Column index {column} is outside dimension {dimension}."),
                )
            })?;
            if *seen {
                return Err(hessian_error(
                    line_number,
                    format!("Repeated column index {column}."),
                ));
            }
            *seen = true;
            columns.push(column);
        }
        seen_rows.fill(false);
        for _ in 0..dimension {
            let (line_number, line) = next_hessian_line(&mut lines, "a matrix row")?;
            let mut fields = line.split_whitespace();
            let row_field = fields
                .next()
                .ok_or_else(|| hessian_error(line_number, "Expected a matrix row index."))?;
            let row = row_field.parse::<usize>().map_err(|_| {
                hessian_error(line_number, format!("Invalid row index {row_field:?}."))
            })?;
            let seen = seen_rows.get_mut(row).ok_or_else(|| {
                hessian_error(
                    line_number,
                    format!("Row index {row} is outside dimension {dimension}."),
                )
            })?;
            if *seen {
                return Err(hessian_error(
                    line_number,
                    format!("Repeated row index {row}."),
                ));
            }
            *seen = true;
            let value_count = fields.clone().count();
            if value_count != columns.len() {
                return Err(hessian_error(
                    line_number,
                    format!(
                        "Row {row} has {value_count} values; expected {} for this column block.",
                        columns.len()
                    ),
                ));
            }
            for (&column, field) in columns.iter().zip(fields) {
                let value = parse_orca_float(field).map_err(|_| {
                    hessian_error(
                        line_number,
                        format!("Invalid value {field:?} at row {row}, column {column}."),
                    )
                })?;
                if !value.is_finite() {
                    return Err(hessian_error(
                        line_number,
                        format!("The value at row {row}, column {column} must be finite."),
                    ));
                }
                let cell = hessian.get_mut(row * dimension + column).ok_or_else(|| {
                    hessian_error(line_number, "The Hessian matrix index is out of range.")
                })?;
                *cell = value;
            }
        }
        column_count += columns.len();
    }
    if column_count != dimension {
        return Err(Error::Invalid(format!(
            "The ORCA $hessian section is truncated: read {column_count} of {dimension} columns."
        )));
    }
    for (row, values) in hessian.chunks_exact(dimension).enumerate() {
        for (column, &forward) in values.iter().enumerate().skip(row + 1) {
            let &reverse = hessian.get(column * dimension + row).ok_or_else(|| {
                Error::Invalid("The ORCA Hessian matrix index is out of range.".into())
            })?;
            let tolerance = SYMMETRY_ABSOLUTE_TOLERANCE
                + SYMMETRY_RELATIVE_TOLERANCE * forward.abs().max(reverse.abs());
            if (forward - reverse).abs() > tolerance {
                return Err(Error::Invalid(format!(
                    "The ORCA $hessian matrix is asymmetric at row {row}, column {column}: {forward} differs from {reverse} at row {column}, column {row} (tolerance {tolerance})."
                )));
            }
        }
    }
    validate_orca_atoms(text, expected_frame)?;
    Ok(hessian)
}

fn parse_orca_float(field: &str) -> Result<f64, std::num::ParseFloatError> {
    if field.contains(['D', 'd']) {
        field.replace(['D', 'd'], "E").parse()
    } else {
        field.parse()
    }
}

fn validate_orca_atoms(text: &str, expected_frame: &Frame) -> Result<(), Error> {
    let mut sections = text
        .lines()
        .enumerate()
        .filter(|(_, line)| line.trim().eq_ignore_ascii_case("$atoms"));
    let Some((section_index, _)) = sections.next() else {
        return Err(Error::Invalid(
            "The ORCA file has no $atoms section; the Hessian geometry is required.".into(),
        ));
    };
    if let Some((index, _)) = sections.next() {
        return Err(atoms_error(index + 1, "Repeated $atoms section."));
    }
    let mut lines = text
        .lines()
        .enumerate()
        .skip(section_index + 1)
        .map(|(index, line)| (index + 1, line.trim()))
        .take_while(|(_, line)| !line.starts_with('$'))
        .filter(|(_, line)| !line.is_empty());
    let (line_number, line) = lines
        .next()
        .ok_or_else(|| Error::Invalid("The ORCA $atoms section has no atom count.".into()))?;
    let atom_count = line
        .parse::<usize>()
        .ok()
        .filter(|&count| count != 0)
        .ok_or_else(|| atoms_error(line_number, "Expected one positive atom count."))?;
    if atom_count != expected_frame.numbers.len() {
        return Err(atoms_error(
            line_number,
            format!(
                "Atom count mismatch: $atoms has {atom_count}; INPUT has {}.",
                expected_frame.numbers.len()
            ),
        ));
    }
    for (index, (_, expected_element, expected_position)) in atoms(expected_frame).enumerate() {
        let atom = index + 1;
        let (line_number, line) = lines.next().ok_or_else(|| {
            Error::Invalid(format!(
                "The ORCA $atoms section is truncated: missing atom {atom}."
            ))
        })?;
        let fields: Vec<_> = line.split_whitespace().collect();
        let &[symbol, mass, x, y, z] = fields.as_slice() else {
            return Err(atoms_error(
                line_number,
                format!("Atom {atom} needs an element, mass, and three Bohr coordinates."),
            ));
        };
        if symbol != expected_element.symbol {
            return Err(atoms_error(
                line_number,
                format!(
                    "Atom {atom} element mismatch: $atoms has {symbol}; INPUT has {}.",
                    expected_element.symbol
                ),
            ));
        }
        if !parse_orca_float(mass).is_ok_and(|mass| mass.is_finite() && mass > 0.0) {
            return Err(atoms_error(
                line_number,
                format!("Atom {atom} needs a positive finite mass."),
            ));
        }
        for ((field, axis), expected_coordinate) in [x, y, z]
            .into_iter()
            .zip(["x", "y", "z"])
            .zip(expected_position)
        {
            let coordinate = parse_orca_float(field)
                .ok()
                .filter(|value| value.is_finite())
                .ok_or_else(|| {
                    atoms_error(
                        line_number,
                        format!("Atom {atom} needs finite Bohr coordinates."),
                    )
                })?
                * ANGSTROM_PER_BOHR;
            if (coordinate - expected_coordinate).abs() > GEOMETRY_TOLERANCE_ANGSTROM {
                return Err(atoms_error(
                    line_number,
                    format!(
                        "Atom {atom} coordinate {axis} mismatch: $atoms has {coordinate:.10} Å; INPUT has {expected_coordinate:.10} Å (tolerance 1e-3 Å)."
                    ),
                ));
            }
        }
    }
    if let Some((line_number, _)) = lines.next() {
        return Err(atoms_error(
            line_number,
            format!("Unexpected atom data after the declared count of {atom_count}."),
        ));
    }
    Ok(())
}

fn atoms_error(line_number: usize, message: impl std::fmt::Display) -> Error {
    Error::Invalid(format!("ORCA atoms, line {line_number}: {message}"))
}

fn hessian_error(line_number: usize, message: impl std::fmt::Display) -> Error {
    Error::Invalid(format!("ORCA Hessian, line {line_number}: {message}"))
}

fn next_hessian_line<'a>(
    lines: &mut impl Iterator<Item = (usize, &'a str)>,
    expected: &str,
) -> Result<(usize, &'a str), Error> {
    lines.next().ok_or_else(|| {
        Error::Invalid(format!(
            "The ORCA $hessian section is truncated: missing {expected}."
        ))
    })
}

pub fn write_orca_hess(frame: &Frame, hessian_hartree_per_bohr2: &[f64]) -> Result<String, Error> {
    check_atoms(frame)?;
    let dimension = frame.numbers.len() * 3;
    let hessian = hessian_hartree_per_bohr2;
    if dimension.checked_pow(2) != Some(hessian.len()) || hessian.iter().any(|x| !x.is_finite()) {
        return Err(Error::Invalid(
            "The Hessian does not match the structure.".into(),
        ));
    }

    let mut text = String::from(
        "\n$orca_hessian_file\n\n$act_atom\n  0\n\n$act_coord\n  0\n\n$act_energy\n        0.000000\n\n$hessian\n",
    );
    let _ = writeln!(text, "{dimension}");
    for start in (0..dimension).step_by(5) {
        let end = (start + 5).min(dimension);
        for column in start..end {
            let _ = write!(text, "{column:>19}");
        }
        text.push('\n');
        for (row, values) in hessian.chunks_exact(dimension).enumerate() {
            let _ = write!(text, "{row:>7}");
            for &value in values.iter().skip(start).take(end - start) {
                let _ = write!(text, "{:>19}", fortran_scientific(value));
            }
            text.push('\n');
        }
    }

    let _ = writeln!(text, "$atoms\n{}", frame.numbers.len());
    for (index, (_, element, pos)) in atoms(frame).enumerate() {
        let sym = element.symbol;
        let mass = frame
            .masses_amu
            .get(index)
            .copied()
            .unwrap_or_else(|| element.average_mass_amu());
        let [x, y, z] = pos.map(|coordinate| coordinate / ANGSTROM_PER_BOHR);
        let _ = writeln!(
            text,
            " {sym:<2} {mass:>12.5} {x:>17.10} {y:>17.10} {z:>17.10}"
        );
    }
    text.push_str("\n$end\n");
    Ok(text)
}

fn fortran_scientific(value: f64) -> String {
    let formatted = format!("{value:.10E}");
    if let Some((mantissa, exponent)) = formatted.split_once('E') {
        let (sign, digits) = exponent
            .strip_prefix('-')
            .map_or(('+', exponent), |digits| ('-', digits));
        format!("{mantissa}E{sign}{digits:0>2}")
    } else {
        formatted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MATRIX: &str = "$hessian\n3\n0 1 2\n0 1.0E+00 2.0e-01 -3\n1 0.2 5 6\n2 -3 6 9\n$atoms\n1\nH 1.00794 0 0 0\n$end\n";

    fn frame(atom_count: usize) -> Frame {
        Frame {
            numbers: vec![1; atom_count],
            positions_angstrom: vec![[0.0; 3]; atom_count],
            isotopes: vec![],
            masses_amu: vec![],
            charge: 0,
            multiplicity: 1,
            drawing_atoms: vec![],
        }
    }

    fn with_atoms(atoms: &str) -> String {
        let (hessian, _) = MATRIX.split_once("$atoms").unwrap();
        format!("{hessian}$atoms\n{atoms}\n$end\n")
    }

    fn assert_error(text: &str, atoms: usize, expected: &str) {
        assert_frame_error(text, &frame(atoms), expected);
    }

    fn assert_frame_error(text: &str, frame: &Frame, expected: &str) {
        let error = read_orca_hess(text, frame).unwrap_err();
        assert!(matches!(error, Error::Invalid(_)));
        assert!(
            error.to_string().contains(expected),
            "expected {expected:?} in {error}"
        );
    }

    #[test]
    fn reader_roundtrips_the_writer_across_column_blocks() {
        let mut frame = frame(2);
        frame.positions_angstrom[1][0] = 0.74;
        let mut expected: Vec<_> = (0_i32..36)
            .map(|index| {
                let row = index / 6;
                let column = index % 6;
                (f64::from(row.min(column) * 6 + row.max(column)) - 17.25) / 7.0
            })
            .collect();
        expected[0] = 0.0;
        expected[1] = -1.234567890123e-100;
        expected[6] = expected[1];
        expected[2] = 1.234567890123e100;
        expected[12] = expected[2];
        let text = write_orca_hess(&frame, &expected).unwrap();
        let actual = read_orca_hess(&text, &frame).unwrap();
        assert_eq!(actual.len(), expected.len());
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert!(
                (actual - expected).abs() <= expected.abs() * 6e-11,
                "{actual} != {expected}"
            );
        }
    }

    #[test]
    fn reader_places_indexed_blocks_and_ignores_other_sections() {
        let text = "\n$orca_hessian_file\n$normal_modes\nnot a Hessian matrix\n  $HESSIAN  \n\n 3 \n2 0\n2 9 -3\n0 -3D+00 1D+00\n1 6d+00 2E-01\n\n1\n1 5\n2 6\n0 2e-01\n$vibrational_frequencies\n3\n0 -100\n  $ATOMS  \n\n 1 \nH 1.00794 0 0 0\n$end\n";
        for text in [text.to_owned(), text.replace('\n', "\r\n")] {
            assert_eq!(
                read_orca_hess(&text, &frame(1)).unwrap(),
                vec![1.0, 0.2, -3.0, 0.2, 5.0, 6.0, -3.0, 6.0, 9.0]
            );
        }
    }

    #[test]
    fn reader_rejects_missing_or_repeated_sections_and_wrong_dimensions() {
        assert_error("$atoms\n1\n$end\n", 1, "no $hessian");
        assert_error(&format!("{MATRIX}{MATRIX}"), 1, "Repeated $hessian");
        assert_error(MATRIX, 2, "expected 6 for 2 atoms");
        for dimension in ["0", "-3", "3 3", "three"] {
            assert_error(
                &MATRIX.replacen("\n3\n", &format!("\n{dimension}\n"), 1),
                1,
                "positive matrix dimension",
            );
        }
    }

    #[test]
    fn reader_rejects_invalid_expected_geometry() {
        assert_error(MATRIX, 0, "structure has no atoms");
        let mut invalid_frame = frame(1);
        invalid_frame.positions_angstrom.clear();
        assert_frame_error(MATRIX, &invalid_frame, "missing position");
    }

    #[test]
    fn reader_rejects_truncated_matrices_before_the_next_section() {
        assert_error("$hessian\n$atoms\n1\n", 1, "missing the dimension");
        assert_error("$hessian\n3\n", 1, "truncated");
        assert_error(&MATRIX.replace("2 -3 6 9\n", ""), 1, "missing a matrix row");
        assert_error(
            "$hessian\n3\n0 1\n0 1 2\n1 4 5\n2 7 8\n$atoms\n1\n",
            1,
            "read 2 of 3 columns",
        );
        assert_error("$hessian\n999999\n0 1 2\n", 333333, "truncated");
    }

    #[test]
    fn reader_rejects_invalid_column_and_row_indices_and_value_counts() {
        for (header, expected) in [
            ("0 0 2", "Repeated column index 0"),
            ("0 1 3", "Column index 3 is outside dimension 3"),
            ("0 1 nope", "Invalid column index"),
        ] {
            assert_error(
                &MATRIX.replacen("\n0 1 2\n", &format!("\n{header}\n"), 1),
                1,
                expected,
            );
        }
        for (row, expected) in [
            ("0 0.2 5 6", "Repeated row index 0"),
            ("3 0.2 5 6", "Row index 3 is outside dimension 3"),
            ("nope 0.2 5 6", "Invalid row index"),
            ("1 0.2 5", "Row 1 has 2 values; expected 3"),
            ("1 0.2 5 6 7", "Row 1 has 4 values; expected 3"),
        ] {
            assert_error(&MATRIX.replace("1 0.2 5 6", row), 1, expected);
        }
        assert_error(
            "$hessian\n3\n0 1\n0 1 2\n1 4 5\n2 7 8\n1 2\n",
            1,
            "Repeated column index 1",
        );
        assert_error(
            &format!("{MATRIX}\n").replace("$atoms\n1\nH 1.00794 0 0 0\n$end", "0 1 2"),
            1,
            "Repeated column index 0",
        );
    }

    #[test]
    fn reader_accepts_six_column_blocks_with_a_short_final_block() {
        let dimension = 15;
        let mut text = format!("$hessian\n{dimension}\n");
        let expected: Vec<f64> = (0..dimension * dimension)
            .map(|index| ((index / dimension + index % dimension) as f64) / 7.0)
            .collect();
        for start in (0..dimension).step_by(6) {
            let end = (start + 6).min(dimension);
            for column in start..end {
                let _ = write!(text, "{column} ");
            }
            text.push('\n');
            for row in 0..dimension {
                let _ = write!(text, "{row}");
                for column in start..end {
                    let _ = write!(text, " {}", expected[row * dimension + column]);
                }
                text.push('\n');
            }
        }
        text.push_str("$atoms\n5\n");
        for _ in 0..5 {
            text.push_str("H 1.00794 0 0 0\n");
        }
        text.push_str("$end\n");
        assert_eq!(read_orca_hess(&text, &frame(5)).unwrap(), expected);
    }

    #[test]
    fn reader_rejects_malformed_and_nonfinite_values() {
        assert_error(
            &MATRIX.replace("2.0e-01", "broken"),
            1,
            "Invalid value \"broken\" at row 0, column 1",
        );
        for invalid in ["NaN", "inf", "-inf", "1E+999", "1D+999"] {
            assert_error(&MATRIX.replace("2.0e-01", invalid), 1, "must be finite");
        }
    }

    #[test]
    fn reader_checks_symmetry_with_absolute_and_relative_tolerances() {
        for (forward, reverse, accepted) in [
            (0.0, 0.9e-6, true),
            (0.0, 1.1e-6, false),
            (1.0, 1.0 + 1.5e-6, true),
            (1.0, 1.0 + 2.1e-6, false),
            (1e8, 1e8 + 90.0, true),
            (1e8, 1e8 + 110.0, false),
            (-1e8, -1e8 - 90.0, true),
            (-1e8, -1e8 - 110.0, false),
        ] {
            let text = format!(
                "$hessian\n3\n0 1 2\n0 1 {forward} 0\n1 {reverse} 1 0\n2 0 0 1\n$atoms\n1\nH 1 0 0 0\n$end\n"
            );
            if accepted {
                let hessian = read_orca_hess(&text, &frame(1)).unwrap();
                assert_eq!(hessian[1], forward);
                assert_eq!(hessian[3], reverse);
            } else {
                assert_error(&text, 1, "asymmetric at row 0, column 1");
            }
        }
    }

    #[test]
    fn reader_validates_atoms_in_the_original_frame_after_bohr_conversion() {
        let text = with_atoms("1\nH 2.014D+00 2D+00 -3d+00 4E+00");
        let mut frame = frame(1);
        frame.positions_angstrom[0] = [2.0, -3.0, 4.0].map(|value| value * ANGSTROM_PER_BOHR);
        frame.masses_amu = vec![1.00794];
        assert!(read_orca_hess(&text, &frame).is_ok());
        frame.positions_angstrom[0][0] += 0.999e-3;
        assert!(read_orca_hess(&text, &frame).is_ok());
        frame.positions_angstrom[0][0] += 0.002e-3;
        assert_frame_error(&text, &frame, "Atom 1 coordinate x mismatch");
    }

    #[test]
    fn reader_rejects_missing_repeated_or_inconsistent_atoms() {
        let (hessian, _) = MATRIX.split_once("$atoms").unwrap();
        assert_error(hessian, 1, "no $atoms section");
        assert_error(
            &format!("{MATRIX}$atoms\n1\nH 1 0 0 0\n"),
            1,
            "Repeated $atoms",
        );
        assert_error(
            &with_atoms("2\nH 1 0 0 0\nH 1 0 0 0"),
            1,
            "Atom count mismatch: $atoms has 2; INPUT has 1",
        );
        assert_error(&with_atoms("1"), 1, "missing atom 1");
        assert_error(
            &with_atoms("1\nH 1 0 0 0\nH 1 0 0 0"),
            1,
            "Unexpected atom data",
        );
    }

    #[test]
    fn reader_rejects_first_element_or_coordinate_mismatch() {
        let mut expected_frame = frame(2);
        expected_frame.numbers[1] = 8;
        let mut reordered_frame = expected_frame.clone();
        reordered_frame.numbers.swap(0, 1);
        let text = write_orca_hess(&reordered_frame, &[0.0; 36]).unwrap();
        assert_frame_error(
            &text,
            &expected_frame,
            "Atom 1 element mismatch: $atoms has O; INPUT has H",
        );
        assert_error(
            &with_atoms("1\nH 1 1 2 3"),
            1,
            "Atom 1 coordinate x mismatch",
        );
    }

    #[test]
    fn reader_rejects_malformed_atoms() {
        assert_error(&with_atoms(""), 1, "no atom count");
        for count in ["0", "-1", "one", "1 1"] {
            assert_error(&with_atoms(count), 1, "positive atom count");
        }
        for atom in ["H", "H 1 0 0", "H 1 0 0 0 extra"] {
            assert_error(
                &with_atoms(&format!("1\n{atom}")),
                1,
                "element, mass, and three Bohr coordinates",
            );
        }
        assert_error(
            &with_atoms("1\nXx 1 0 0 0"),
            1,
            "element mismatch: $atoms has Xx; INPUT has H",
        );
        for mass in ["0", "-1", "NaN", "inf", "broken"] {
            assert_error(
                &with_atoms(&format!("1\nH {mass} 0 0 0")),
                1,
                "positive finite mass",
            );
        }
        for coordinate in ["NaN", "inf", "1D+999", "broken"] {
            assert_error(
                &with_atoms(&format!("1\nH 1 0 {coordinate} 0")),
                1,
                "finite Bohr coordinates",
            );
        }
    }
}
