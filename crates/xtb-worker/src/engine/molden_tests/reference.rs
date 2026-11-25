// SPDX-License-Identifier: MIT OR Apache-2.0
//! Read axtb's Molden reference and compare it with the worker's canonical orbitals.

use reshiki_qcdata::{
    BasisSet, Beta, CartesianNorm, OrbitalSet, Orbitals, Shell, Spin, mo_to_molden,
};
use std::fmt;

pub(super) struct Molden {
    pub(super) positions: Vec<[f64; 3]>,
    pub(super) basis: BasisSet,
    pub(super) alpha: Vec<MoldenMo>,
    pub(super) beta: Vec<MoldenMo>,
}

pub(super) struct MoldenMo {
    energy_line: String,
    occupation: f64,
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
    let mut alpha = Vec::new();
    let mut beta = Vec::new();
    while let Some(symmetry) = lines.next() {
        let energy_line = lines.next().expect("MO energy").to_owned();
        assert!(energy_line.starts_with("Ene="));
        let mos = match lines.next().expect("MO spin") {
            "Spin= Alpha" => &mut alpha,
            "Spin= Beta" => &mut beta,
            spin => panic!("unexpected Molden spin {spin}"),
        };
        assert_eq!(symmetry, format!("Sym= {}a", mos.len() + 1));
        let occupation = number(lines.next().unwrap().strip_prefix("Occup= ").unwrap());
        let coefficients = (1..=n_ao)
            .map(|i| {
                let [index, value] = fields(lines.next().expect("MO coefficient"));
                assert_eq!(index.parse::<usize>().unwrap(), i);
                number(value)
            })
            .collect();
        mos.push(MoldenMo {
            energy_line,
            occupation,
            coefficients,
        });
    }
    assert!(!alpha.is_empty());
    Molden {
        positions,
        basis,
        alpha,
        beta,
    }
}

/// Errors cover primitive exponents, contractions, and signed MO coefficients.
/// Energies and occupations are checked separately at the precision of the file.
#[derive(Default)]
pub(super) struct Comparison {
    compared: usize,
    identical: usize,
    pub(super) max_relative: f64,
    worst: String,
}

impl Comparison {
    fn compare(&mut self, actual: f64, reference: f64, label: fmt::Arguments<'_>) {
        assert!(actual.is_finite() && reference.is_finite(), "{label}");
        let relative = (actual - reference).abs() / reference.abs().max(1.0);
        if relative > self.max_relative {
            self.max_relative = relative;
            self.worst = format!("{label}: {actual} vs {reference}");
        }
        self.compared += 1;
        self.identical += usize::from(actual.to_bits() == reference.to_bits());
    }

    pub(super) fn assert_matches(&self) {
        assert!(self.max_relative <= 1e-14, "{}: {self}", self.worst);
    }
}

impl fmt::Display for Comparison {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "compared={} bit_identical={} max_rel={:.3e}",
            self.compared, self.identical, self.max_relative
        )
    }
}

pub(super) fn check_spherical(
    orbitals: &Orbitals,
    molden: &Molden,
    expected_rows: &[u32],
) -> Comparison {
    let basis = orbitals.basis.as_ref().unwrap();
    let mut report = Comparison::default();
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
                report.compare(
                    value,
                    expected,
                    format_args!("shell {index} {label} {primitive}"),
                );
            }
        }
    }
    let n_ao = basis.shells.iter().map(Shell::n_functions).sum::<usize>();
    let mut check_spin = |spin, set: &OrbitalSet, occupations: &[f64], mos: &[MoldenMo]| {
        assert_eq!(mos.len(), n_ao, "{spin:?} Molden MO count");
        assert_eq!(set.energies_hartree.len(), mos.len(), "{spin:?} energies");
        assert_eq!(occupations.len(), mos.len(), "{spin:?} occupations");
        for (index, mo) in mos.iter().enumerate() {
            assert_eq!(
                energy_line(set.energies_hartree[index]),
                mo.energy_line,
                "{spin:?} MO {index} energy"
            );
            // The writer rounds occupations to ten places after the decimal.
            assert_eq!(
                number(&format!("{:.10}", occupations[index])),
                mo.occupation,
                "{spin:?} MO {index} occupation"
            );
        }
        assert_eq!(set.rows, expected_rows, "{spin:?} selected rows");
        assert_eq!(
            set.coefficients.len(),
            expected_rows.len() * n_ao,
            "{spin:?} coefficient count"
        );
        for (&index, row) in expected_rows
            .iter()
            .zip(set.coefficients.chunks_exact(n_ao))
        {
            let rebuilt = mo_to_molden(basis, row, CartesianNorm::Unit).unwrap();
            let reference = &mos[index as usize].coefficients;
            assert_eq!(rebuilt.len(), reference.len());
            for (ao, (&value, &expected)) in rebuilt.iter().zip(reference).enumerate() {
                report.compare(value, expected, format_args!("{spin:?} MO {index} AO {ao}"));
            }
        }
    };
    check_spin(
        Spin::Alpha,
        &orbitals.alpha,
        &orbitals.alpha.occupations,
        &molden.alpha,
    );
    match &orbitals.beta {
        Beta::Restricted => assert!(molden.beta.is_empty()),
        Beta::SameOrbitals { occupations } => {
            check_spin(Spin::Beta, &orbitals.alpha, occupations, &molden.beta);
        }
        Beta::Unrestricted(beta) => {
            check_spin(Spin::Beta, beta, &beta.occupations, &molden.beta);
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
