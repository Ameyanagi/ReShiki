//! Read and write Cartesian Hessians in ORCA `.hess` files.
use std::fmt::Write;

use crate::qcdata::ANGSTROM_PER_BOHR;

use super::{Error, Frame, atoms, check_atoms};

/// Read the `$hessian` matrix in Hartree/Bohr², in row-major order.
///
/// Its dimension must be three times `expected_atom_count`. Other sections in
/// the file, including `$atoms`, are ignored; the caller supplies the geometry
/// whose Cartesian coordinate order the matrix must follow.
pub fn read_orca_hess(text: &str, expected_atom_count: usize) -> Result<Vec<f64>, Error> {
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
        let mut columns = Vec::with_capacity(5);
        for field in header.split_whitespace() {
            if columns.len() == 5 {
                return Err(hessian_error(
                    line_number,
                    "A column block must contain at most five column indices.",
                ));
            }
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
                let value = if field.contains(['D', 'd']) {
                    field.replace(['D', 'd'], "E").parse::<f64>()
                } else {
                    field.parse::<f64>()
                }
                .map_err(|_| {
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
    Ok(hessian)
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

    const MATRIX: &str =
        "$hessian\n3\n0 1 2\n0 1.0E+00 2.0e-01 -3\n1 4 5 6\n2 7 8 9\n$atoms\nignored\n$end\n";

    fn assert_error(text: &str, atoms: usize, expected: &str) {
        let error = read_orca_hess(text, atoms).unwrap_err();
        assert!(matches!(error, Error::Invalid(_)));
        assert!(
            error.to_string().contains(expected),
            "expected {expected:?} in {error}"
        );
    }

    #[test]
    fn reader_roundtrips_the_writer_across_column_blocks() {
        let frame = Frame {
            numbers: vec![1, 1],
            positions_angstrom: vec![[0.0, 0.0, 0.0], [0.74, 0.0, 0.0]],
            isotopes: vec![],
            masses_amu: vec![],
            charge: 0,
            multiplicity: 1,
            drawing_atoms: vec![],
        };
        let mut expected: Vec<_> = (0..36)
            .map(|index| (f64::from(index) - 17.25) / 7.0)
            .collect();
        expected[0] = 0.0;
        expected[1] = -1.234567890123e-100;
        expected[2] = 1.234567890123e100;
        let text = write_orca_hess(&frame, &expected).unwrap();
        let actual = read_orca_hess(&text, frame.numbers.len()).unwrap();
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
        let text = "\n$orca_hessian_file\n$normal_modes\nnot a Hessian matrix\n  $HESSIAN  \n\n 3 \n2 0\n2 9 7\n0 -3D+00 1D+00\n1 6d+00 4E+00\n\n1\n1 5\n2 8\n0 2e-01\n$vibrational_frequencies\n3\n0 -100\n$end\n";
        for text in [text.to_owned(), text.replace('\n', "\r\n")] {
            assert_eq!(
                read_orca_hess(&text, 1).unwrap(),
                vec![1.0, 0.2, -3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]
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
        for atoms in [0, usize::MAX] {
            assert_error(MATRIX, atoms, "atom count");
        }
    }

    #[test]
    fn reader_rejects_truncated_matrices_before_the_next_section() {
        assert_error("$hessian\n$atoms\n1\n", 1, "missing the dimension");
        assert_error("$hessian\n3\n", 1, "truncated");
        assert_error(&MATRIX.replace("2 7 8 9\n", ""), 1, "missing a matrix row");
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
            ("0 4 5 6", "Repeated row index 0"),
            ("3 4 5 6", "Row index 3 is outside dimension 3"),
            ("nope 4 5 6", "Invalid row index"),
            ("1 4 5", "Row 1 has 2 values; expected 3"),
            ("1 4 5 6 7", "Row 1 has 4 values; expected 3"),
        ] {
            assert_error(&MATRIX.replace("1 4 5 6", row), 1, expected);
        }
        assert_error(
            "$hessian\n3\n0 1\n0 1 2\n1 4 5\n2 7 8\n1 2\n",
            1,
            "Repeated column index 1",
        );
        assert_error(
            &format!("{MATRIX}\n").replace("$atoms\nignored\n$end", "0 1 2"),
            1,
            "Repeated column index 0",
        );
        let oversized_block = format!(
            "$hessian\n6\n0 1 2 3 4 5\n{}",
            (0..6)
                .map(|row| format!("{row} 1 2 3 4 5 6\n"))
                .collect::<String>()
        );
        assert_error(&oversized_block, 2, "at most five column indices");
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
}
