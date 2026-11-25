use super::*;

type HessAtom = (String, f64, [f64; 3]);

fn parse(text: &str) -> (Vec<f64>, Vec<HessAtom>) {
    let lines: Vec<_> = text.lines().collect();
    let mut index = lines.iter().position(|line| *line == "$hessian").unwrap() + 1;
    let dimension = lines[index].parse::<usize>().unwrap();
    index += 1;
    let mut values = vec![f64::NAN; dimension * dimension];
    for first in (0..dimension).step_by(5) {
        let columns: Vec<_> = lines[index]
            .split_whitespace()
            .map(|value| value.parse::<usize>().unwrap())
            .collect();
        assert_eq!(
            columns,
            (first..dimension.min(first + 5)).collect::<Vec<_>>()
        );
        index += 1;
        for row in 0..dimension {
            let fields: Vec<_> = lines[index].split_whitespace().collect();
            assert_eq!(fields[0].parse::<usize>().unwrap(), row);
            assert_eq!(fields.len(), columns.len() + 1);
            for (&column, field) in columns.iter().zip(&fields[1..]) {
                let (_, exponent) = field.split_once('E').unwrap();
                assert_eq!(exponent.len(), 3, "Fortran exponent: {field}");
                assert!(exponent.starts_with('+') || exponent.starts_with('-'));
                assert!(exponent[1..].bytes().all(|byte| byte.is_ascii_digit()));
                values[row * dimension + column] = field.parse::<f64>().unwrap();
            }
            index += 1;
        }
    }
    assert!(values.iter().all(|value| value.is_finite()));
    let index = lines.iter().position(|line| *line == "$atoms").unwrap();
    let count = lines[index + 1].parse::<usize>().unwrap();
    assert_eq!(count * 3, dimension);
    let atoms = lines[index + 2..index + 2 + count]
        .iter()
        .map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            assert_eq!(fields.len(), 5);
            (
                fields[0].to_owned(),
                fields[1].parse().unwrap(),
                [
                    fields[2].parse().unwrap(),
                    fields[3].parse().unwrap(),
                    fields[4].parse().unwrap(),
                ],
            )
        })
        .collect();
    (values, atoms)
}

#[test]
fn hessian_roundtrips_matrix_masses_and_bohr_coordinates() {
    for custom_masses in [false, true] {
        let mut frame = water();
        frame.positions_angstrom[0][2] = -0.125;
        if custom_masses {
            frame.masses_amu = vec![17.99916, 1.00783, 2.01410];
        }
        let mut input = hessian(&frame);
        input[0] = -1.2e-9;
        input[1] = 1.2e9;
        let text = write_orca_hess(&frame, &input).unwrap();
        assert!(text.starts_with("\n$orca_hessian_file\n"));
        assert!(text.contains("$act_atom\n  0\n"));
        assert!(text.contains("$act_coord\n  0\n"));
        assert!(text.contains("$act_energy\n"));
        assert!(text.ends_with("\n\n$end\n"));
        let (actual, atoms) = parse(&text);
        assert_eq!(actual.len(), input.len());
        for (actual, expected) in actual.into_iter().zip(input) {
            assert!(
                (actual - expected).abs() / expected.abs() <= 1e-10,
                "{actual} != {expected}"
            );
        }
        for (index, (symbol, mass, coordinates)) in atoms.into_iter().enumerate() {
            let element = &crate::chemistry::ELEMENTS[usize::from(frame.numbers[index])];
            assert_eq!(symbol, element.symbol);
            let expected_mass = if custom_masses {
                frame.masses_amu[index]
            } else {
                element.average_mass_amu()
            };
            assert!((mass - expected_mass).abs() <= 5.1e-6);
            for (actual, angstrom) in coordinates.into_iter().zip(frame.positions_angstrom[index]) {
                assert!((actual - angstrom / ANGSTROM_PER_BOHR).abs() <= 5.1e-11);
            }
        }
    }
}

#[test]
fn invalid_hessians_are_rejected_with_the_public_error() {
    let frame = water();
    for invalid in [
        vec![],
        vec![0.0; 80],
        vec![0.0; 82],
        vec![f64::NAN; 81],
        vec![f64::INFINITY; 81],
        vec![f64::NEG_INFINITY; 81],
    ] {
        let error = write_orca_hess(&frame, &invalid).unwrap_err();
        assert!(matches!(error, Error::Invalid(_)));
        assert_eq!(
            error.to_string(),
            "The Hessian does not match the structure."
        );
    }
}
