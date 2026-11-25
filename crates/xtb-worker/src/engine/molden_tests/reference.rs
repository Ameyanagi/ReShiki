// SPDX-License-Identifier: MIT OR Apache-2.0
//! A deliberately small reader for the closed-shell Molden files emitted by axtb.

use reshiki_qcdata::{BasisSet, CartesianNorm, OrbitalSet, Shell, mo_to_molden};

pub(super) struct Molden {
    pub(super) positions: Vec<[f64; 3]>,
    pub(super) basis: BasisSet,
    pub(super) mos: Vec<MoldenMo>,
}

pub(super) struct MoldenMo {
    energy_line: String,
    pub(super) coefficients: Vec<f64>,
}

pub(super) fn parse(text: &str, spherical: bool) -> Molden {
    let mut lines = text.lines();
    assert!(lines.any(|line| line == "[Atoms] AU"));
    let mut positions = Vec::new();
    for line in lines.by_ref().take_while(|line| *line != "[GTO]") {
        let [_, index, _, x, y, z] = fields(line);
        assert_eq!(index.parse::<usize>().unwrap(), positions.len() + 1);
        positions.push([x, y, z].map(number));
    }
    let mut shells = Vec::new();
    for atom in 0..positions.len() {
        let [index, _] = fields(lines.next().expect("GTO atom"));
        assert_eq!(index.parse::<usize>().unwrap(), atom + 1);
        while let Some(line) = lines.next().filter(|line| !line.trim().is_empty()) {
            let [label, count, scale] = fields(line);
            let l = match label {
                "s" => 0,
                "p" => 1,
                "d" => 2,
                "f" => 3,
                _ => panic!("unexpected GTO shell {label}"),
            };
            assert_eq!(number(scale), 1.0);
            let (exponents_per_bohr2, coefficients) = (0..count.parse::<usize>().unwrap())
                .map(|_| {
                    let [alpha, coefficient] = fields(lines.next().expect("GTO primitive"));
                    (number(alpha), number(coefficient))
                })
                .unzip();
            shells.push(Shell {
                atom: atom as u32,
                l,
                pure: spherical && l >= 2,
                exponents_per_bohr2,
                coefficients,
            });
        }
    }
    // axtb emits flags according to the greatest l, with [7F] before [5D].
    let max_l = shells.iter().map(|shell| shell.l).max().unwrap();
    let flags: &[(u8, &str)] = if spherical {
        &[(3, "[7F]"), (2, "[5D]")]
    } else {
        &[(2, "[6D]"), (3, "[10F]")]
    };
    for &(l, flag) in flags {
        if max_l >= l {
            assert_eq!(lines.next(), Some(flag));
        }
    }
    assert_eq!(lines.next(), Some("[MO]"));
    let basis = BasisSet { shells };
    let n_ao = basis.shells.iter().map(Shell::n_functions).sum::<usize>();
    let mut mos = Vec::new();
    while let Some(symmetry) = lines.next() {
        assert!(symmetry.starts_with("Sym="));
        let energy_line = lines.next().expect("MO energy").to_owned();
        assert!(energy_line.starts_with("Ene="));
        assert_eq!(lines.next(), Some("Spin= Alpha"));
        assert!(lines.next().unwrap().starts_with("Occup="));
        let coefficients = (1..=n_ao)
            .map(|i| {
                let [index, value] = fields(lines.next().expect("MO coefficient"));
                assert_eq!(index.parse::<usize>().unwrap(), i);
                number(value)
            })
            .collect();
        mos.push(MoldenMo {
            energy_line,
            coefficients,
        });
    }
    assert!(!mos.is_empty());
    Molden {
        positions,
        basis,
        mos,
    }
}

/// Rebuild all spherical GTO/MO data and return (values, bit-identical values, max error).
/// Error is |actual-reference| / max(1, |reference|). Energies are checked as exact
/// formatted lines, and are excluded from these floating-point comparison counts.
pub(super) fn check_spherical(
    basis: &BasisSet,
    orbitals: &OrbitalSet,
    molden: &Molden,
) -> (usize, usize, f64) {
    let mut report = (0, 0, 0.0_f64);
    assert_eq!(basis.shells.len(), molden.basis.shells.len());
    for (index, (actual, reference)) in basis.shells.iter().zip(&molden.basis.shells).enumerate() {
        assert_eq!((actual.atom, actual.l), (reference.atom, reference.l));
        assert_eq!(actual.pure, actual.l >= 2);
        assert_eq!(actual.pure, reference.pure);
        assert_eq!(actual.exponents_per_bohr2.len(), actual.coefficients.len());
        for (label, values, expected) in [
            (
                "exponent",
                &actual.exponents_per_bohr2,
                &reference.exponents_per_bohr2,
            ),
            ("contraction", &actual.coefficients, &reference.coefficients),
        ] {
            assert_eq!(values.len(), expected.len(), "shell {index} {label} count");
            for (primitive, (&value, &expected)) in values.iter().zip(expected).enumerate() {
                compare(
                    &mut report,
                    value,
                    expected,
                    format_args!("shell {index} {label} {primitive}"),
                );
            }
        }
    }
    let n_ao = basis.shells.iter().map(Shell::n_functions).sum::<usize>();
    assert!(n_ao > 0);
    assert_eq!(orbitals.energies_hartree.len(), molden.mos.len());
    assert_eq!(
        orbitals.rows,
        (0..molden.mos.len() as u32).collect::<Vec<_>>()
    );
    assert_eq!(orbitals.coefficients.len(), molden.mos.len() * n_ao);
    for (index, (row, reference)) in orbitals
        .coefficients
        .chunks_exact(n_ao)
        .zip(&molden.mos)
        .enumerate()
    {
        assert_eq!(
            energy_line(orbitals.energies_hartree[index]),
            reference.energy_line,
            "MO {index} energy"
        );
        let rebuilt = mo_to_molden(basis, row, CartesianNorm::Unit).unwrap();
        assert_eq!(rebuilt.len(), reference.coefficients.len());
        for (ao, (&value, &expected)) in rebuilt.iter().zip(&reference.coefficients).enumerate() {
            compare(
                &mut report,
                value,
                expected,
                format_args!("MO {index} AO {ao}"),
            );
        }
    }
    report
}

fn fields<const N: usize>(line: &str) -> [&str; N] {
    let mut fields = line.split_whitespace();
    let values = std::array::from_fn(|_| fields.next().expect("missing Molden field"));
    assert!(fields.next().is_none(), "extra fields: {line}");
    values
}

fn number(text: &str) -> f64 {
    let value = text.parse::<f64>().unwrap();
    assert!(value.is_finite(), "nonfinite Molden value: {text}");
    value
}

fn energy_line(energy: f64) -> String {
    assert!(energy.is_finite());
    let scientific = format!("{energy:.14E}");
    let (mantissa, exponent) = scientific.split_once('E').unwrap();
    format!("Ene= {mantissa}E{:+03}", exponent.parse::<i32>().unwrap())
}

fn compare(
    report: &mut (usize, usize, f64),
    actual: f64,
    reference: f64,
    label: std::fmt::Arguments<'_>,
) {
    assert!(actual.is_finite() && reference.is_finite(), "{label}");
    let ratio = (actual - reference).abs() / reference.abs().max(1.0);
    assert!(
        ratio <= 1e-14,
        "{label}: {actual} != {reference} ({ratio:e})"
    );
    report.0 += 1;
    report.1 += usize::from(actual.to_bits() == reference.to_bits());
    report.2 = report.2.max(ratio);
}
