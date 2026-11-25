//! Basis sets, molecular orbitals and the canonical atomic-orbital contract.

use std::{f64::consts::PI, mem::size_of_val};

use serde::{Deserialize, Serialize};

use crate::{Error, Frame, MAX_BASIS_FUNCTIONS, MAX_COEFFICIENTS, MAX_L, MAX_PRIMITIVES};

/// An atomic-orbital basis in canonical shell order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BasisSet {
    /// Shells in atomic-orbital order.
    pub shells: Vec<Shell>,
}

/// A contracted Gaussian shell on one atom.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shell {
    /// Zero-based index of the atom in the frame.
    pub atom: u32,
    /// Angular momentum.
    pub l: u8,
    /// Whether the shell uses real solid harmonics.
    pub pure: bool,
    /// Primitive exponents in bohr⁻².
    pub exponents_per_bohr2: Vec<f64>,
    /// Dimensionless contraction coefficients of normalized axial primitives.
    pub coefficients: Vec<f64>,
}

impl Shell {
    /// Returns the number of atomic orbitals in the shell.
    pub fn n_functions(&self) -> usize {
        let l = usize::from(self.l);
        if self.pure {
            2 * l + 1
        } else {
            (l + 1) * (l + 2) / 2
        }
    }

    /// Checks the atom index, representation, and primitive contraction.
    pub fn validate(&self, n_atoms: usize) -> Result<(), Error> {
        if self.atom as usize >= n_atoms {
            return Err(Error::Invalid("shell atom index is out of range".into()));
        }
        if self.l > MAX_L {
            return Err(Error::TooLarge {
                what: "shell angular momentum",
                count: usize::from(self.l),
                max: usize::from(MAX_L),
            });
        }
        if self.pure && self.l < 2 {
            return Err(Error::Invalid("s and p shells must be Cartesian".into()));
        }

        let count = self.exponents_per_bohr2.len();
        if count > MAX_PRIMITIVES {
            return Err(Error::TooLarge {
                what: "primitives per shell",
                count,
                max: MAX_PRIMITIVES,
            });
        }
        if count == 0 || self.coefficients.len() != count {
            return Err(Error::Invalid(
                "a shell needs matching, nonempty exponent and coefficient arrays".into(),
            ));
        }
        if self
            .exponents_per_bohr2
            .iter()
            .any(|&value| !value.is_finite() || value <= 0.0)
            || self.coefficients.iter().any(|value| !value.is_finite())
        {
            return Err(Error::Invalid(
                "a shell needs finite positive exponents and finite coefficients".into(),
            ));
        }
        let overlap = contraction_overlap(self);
        if !overlap.is_finite() || overlap <= 0.0 {
            return Err(Error::Invalid(
                "shell contraction must have positive finite self-overlap".into(),
            ));
        }
        Ok(())
    }
}

/// The orbitals and core electron counts for a frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Orbitals {
    /// Gaussian basis, present when any coefficient rows are stored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub basis: Option<BasisSet>,
    /// Optional counts of electrons excluded from the basis, one per atom.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub core_electrons: Vec<u32>,
    /// Alpha spectrum and available coefficient rows.
    pub alpha: OrbitalSet,
    /// Beta spectrum or its relation to the alpha spectrum.
    pub beta: Beta,
}

/// An ordered molecular-orbital spectrum and its available coefficient rows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrbitalSet {
    /// Orbital energies in ascending order, in hartree.
    pub energies_hartree: Vec<f64>,
    /// Optional occupations in electrons, one per orbital.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub occupations: Vec<f64>,
    /// Optional symmetry labels, one per orbital.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub symmetries: Vec<String>,
    /// Strictly increasing zero-based indices of the stored MO rows.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rows: Vec<u32>,
    /// Dimensionless MO coefficients, one canonical AO block per stored row.
    #[serde(
        with = "crate::compact::f32",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub coefficients: Vec<f64>,
}

/// How the beta spin channel is represented.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Beta {
    /// Shares alpha orbitals and uses spin-summed occupations (0–2 electrons).
    Restricted,
    /// Shares alpha orbitals, with separate occupations (0–1 electron per spin).
    SameOrbitals {
        /// Optional beta occupations in electrons for the shared spectrum.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        occupations: Vec<f64>,
    },
    /// Has its own orbital spectrum and occupations (0–1 electron per spin).
    Unrestricted(OrbitalSet),
}

/// Spin channel of a molecular orbital.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Spin {
    /// Alpha spin channel.
    Alpha,
    /// Beta spin channel.
    Beta,
}

impl Orbitals {
    /// Checks the basis, core electron counts, and spin channels against the frame.
    pub fn validate(&self, frame: &Frame) -> Result<(), Error> {
        let n_atoms = frame.numbers.len();
        if (!self.core_electrons.is_empty() && self.core_electrons.len() != n_atoms)
            || self
                .core_electrons
                .iter()
                .zip(&frame.numbers)
                .any(|(&core, &z)| core > u32::from(z))
            || self
                .core_electrons
                .iter()
                .map(|&core| i64::from(core))
                .sum::<i64>()
                > frame.electrons()
        {
            return Err(Error::Invalid(
                "invalid orbital core electron counts".into(),
            ));
        }

        let has_rows = !self.alpha.rows.is_empty()
            || matches!(&self.beta, Beta::Unrestricted(set) if !set.rows.is_empty());
        if self.basis.is_some() != has_rows {
            return Err(Error::Invalid(
                "an orbital basis must be present exactly when coefficient rows are stored".into(),
            ));
        }

        let mut n_ao = 0;
        if let Some(basis) = &self.basis {
            for shell in &basis.shells {
                shell.validate(n_atoms)?;
                n_ao += shell.n_functions();
                if n_ao > MAX_BASIS_FUNCTIONS {
                    return Err(Error::TooLarge {
                        what: "basis functions",
                        count: n_ao,
                        max: MAX_BASIS_FUNCTIONS,
                    });
                }
            }
        }
        if has_rows && n_ao == 0 {
            return Err(Error::Invalid(
                "orbital rows require a nonempty basis".into(),
            ));
        }

        let alpha_max_occupation = if matches!(self.beta, Beta::Restricted) {
            if frame.multiplicity != 1 {
                return Err(Error::Invalid(
                    "restricted orbitals require a singlet".into(),
                ));
            }
            2.0
        } else {
            1.0
        };
        self.alpha.validate(n_ao, alpha_max_occupation)?;
        match &self.beta {
            Beta::Restricted => Ok(()),
            Beta::SameOrbitals { occupations } => {
                validate_occupations(occupations, self.alpha.energies_hartree.len(), 1.0)
            }
            Beta::Unrestricted(set) => set.validate(n_ao, 1.0),
        }
    }

    /// Estimates the owned heap storage in bytes.
    pub fn heap_bytes(&self) -> usize {
        self.basis.as_ref().map_or(0, |basis| {
            size_of_val(basis.shells.as_slice())
                + basis
                    .shells
                    .iter()
                    .map(|shell| {
                        size_of_val(shell.exponents_per_bohr2.as_slice())
                            + size_of_val(shell.coefficients.as_slice())
                    })
                    .sum::<usize>()
        }) + size_of_val(self.core_electrons.as_slice())
            + self.alpha.heap_bytes()
            + match &self.beta {
                Beta::Restricted => 0,
                Beta::SameOrbitals { occupations } => size_of_val(occupations.as_slice()),
                Beta::Unrestricted(set) => set.heap_bytes(),
            }
    }
}

impl OrbitalSet {
    fn validate(&self, n_ao: usize, max_occupation: f64) -> Result<(), Error> {
        let n_levels = self.energies_hartree.len();
        if self
            .energies_hartree
            .iter()
            .any(|energy| !energy.is_finite())
            || self
                .energies_hartree
                .windows(2)
                .any(|pair| pair[0] > pair[1])
        {
            return Err(Error::Invalid(
                "orbital energies must be finite and in ascending order".into(),
            ));
        }
        validate_occupations(&self.occupations, n_levels, max_occupation)?;
        if !self.symmetries.is_empty() && self.symmetries.len() != n_levels {
            return Err(Error::Invalid(
                "orbital symmetry labels must match the number of levels".into(),
            ));
        }
        if self.rows.iter().any(|&row| row as usize >= n_levels)
            || self.rows.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(Error::Invalid(
                "orbital coefficient row indices must increase strictly within the spectrum".into(),
            ));
        }
        let count = self.coefficients.len();
        if count > MAX_COEFFICIENTS {
            return Err(Error::TooLarge {
                what: "orbital coefficients per spin",
                count,
                max: MAX_COEFFICIENTS,
            });
        }
        if n_ao.checked_mul(self.rows.len()) != Some(count)
            || self
                .coefficients
                .iter()
                .any(|&value| !(value as f32).is_finite())
        {
            return Err(Error::Invalid(
                "orbital coefficient rows must contain one finite value per basis function".into(),
            ));
        }
        Ok(())
    }

    fn heap_bytes(&self) -> usize {
        size_of_val(self.energies_hartree.as_slice())
            + size_of_val(self.occupations.as_slice())
            + size_of_val(self.symmetries.as_slice())
            + self.symmetries.iter().map(String::len).sum::<usize>()
            + size_of_val(self.rows.as_slice())
            + size_of_val(self.coefficients.as_slice())
    }
}

fn validate_occupations(occupations: &[f64], n_levels: usize, max: f64) -> Result<(), Error> {
    let range = -1e-6..=max + 1e-6;
    if !occupations.is_empty()
        && (occupations.len() != n_levels
            || occupations
                .iter()
                .any(|occupation| !range.contains(occupation)))
    {
        return Err(Error::Invalid("invalid orbital occupations".into()));
    }
    Ok(())
}

/// Lists the powers of x, y, z in canonical Cartesian order.
pub fn canonical_cartesian_order(l: u8) -> Vec<[u8; 3]> {
    let mut order = Vec::new();
    for a in (0..=l).rev() {
        for b in (0..=l - a).rev() {
            order.push([a, b, l - a - b]);
        }
    }
    order
}

/// Returns the normalization of an axial Gaussian primitive with exponent `alpha` in bohr⁻².
pub fn primitive_norm(alpha: f64, l: u8) -> f64 {
    let log_alpha = alpha.ln();
    (0.75 * (log_alpha + (2.0 / PI).ln()) + 0.5 * f64::from(l) * (log_alpha + 4.0_f64.ln())
        - 0.5 * odd_double_factorial(l.into()).ln())
    .exp()
}

/// Returns the dimensionless self-overlap of the axial contraction.
pub fn contraction_overlap(shell: &Shell) -> f64 {
    let mut overlap = 0.0;
    let primitives = shell.exponents_per_bohr2.iter().zip(&shell.coefficients);
    for (&alpha, &d) in primitives.clone() {
        for (&beta, &e) in primitives.clone() {
            // The normalized primitive overlap is (2√(αβ)/(α+β))^(l+3/2).
            let ratio = alpha.min(beta).sqrt() / alpha.max(beta).sqrt();
            let pair = (2.0 * ratio / (1.0 + ratio * ratio)).powf(f64::from(shell.l) + 1.5);
            overlap += (d * pair) * e;
        }
    }
    overlap
}

pub(crate) fn odd_double_factorial(n: u16) -> f64 {
    (0..n).map(|index| f64::from(2 * index + 1)).product()
}

#[cfg(test)]
// Numerically integrate x^(2k) exp(-a x²) over a fine 1D grid.
pub(crate) fn gaussian_moment(k: u8, a: f64) -> f64 {
    let n = 20_000;
    let end = 12.0 / a.sqrt();
    let step = 2.0 * end / f64::from(n);
    (0..=n)
        .map(|i| {
            let x = -end + f64::from(i) * step;
            let weight = if i == 0 || i == n { 0.5 } else { 1.0 };
            weight * x.powi(2 * i32::from(k)) * (-a * x * x).exp()
        })
        .sum::<f64>()
        * step
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shell(l: u8) -> Shell {
        Shell {
            atom: 0,
            l,
            pure: false,
            exponents_per_bohr2: vec![0.45, 1.2],
            coefficients: vec![0.7, -0.2],
        }
    }

    fn orbitals() -> (Frame, Orbitals) {
        let frame = Frame {
            numbers: vec![8],
            positions_angstrom: vec![[0.0; 3]],
            isotopes: vec![],
            masses_amu: vec![],
            charge: 0,
            multiplicity: 1,
            drawing_atoms: vec![],
        };
        let orbitals = Orbitals {
            basis: Some(BasisSet {
                shells: vec![shell(0)],
            }),
            core_electrons: vec![2],
            alpha: OrbitalSet {
                energies_hartree: vec![-0.4, -0.4, 0.25],
                occupations: vec![1.75, 0.25, 0.0],
                symmetries: vec!["a".into(), "b".into(), "c".into()],
                rows: vec![0, 2],
                coefficients: vec![0.3, 0.6],
            },
            beta: Beta::Restricted,
        };
        (frame, orbitals)
    }

    #[test]
    fn canonical_cartesian_components_and_shell_sizes() {
        assert_eq!(
            canonical_cartesian_order(2),
            [
                [2, 0, 0],
                [1, 1, 0],
                [1, 0, 1],
                [0, 2, 0],
                [0, 1, 1],
                [0, 0, 2]
            ]
        );
        for l in 0..=MAX_L {
            let mut shell = shell(l);
            let degree = usize::from(l);
            assert_eq!(canonical_cartesian_order(l).len(), shell.n_functions());
            assert_eq!(shell.n_functions(), (degree + 1) * (degree + 2) / 2);
            if l >= 2 {
                shell.pure = true;
                assert_eq!(shell.n_functions(), 2 * degree + 1);
            }
        }
    }

    #[test]
    fn primitives_have_unit_overlap_by_quadrature() {
        for l in 0..=4 {
            for alpha in [0.12, 0.8, 2.75] {
                let norm = primitive_norm(alpha, l);
                let integral = norm.powi(2)
                    * gaussian_moment(l, 2.0 * alpha)
                    * gaussian_moment(0, 2.0 * alpha).powi(2);
                assert!((integral - 1.0).abs() < 1e-12, "l={l}, alpha={alpha}");
            }
        }
    }

    #[test]
    fn two_primitive_contraction_overlap_matches_quadrature() {
        for l in 0..=2 {
            let shell = shell(l);
            let [a, b] = shell.exponents_per_bohr2[..] else {
                unreachable!();
            };
            let [d, e] = shell.coefficients[..] else {
                unreachable!();
            };
            let integrate = |alpha, beta, da: f64, db: f64| {
                da * db
                    * primitive_norm(alpha, l)
                    * primitive_norm(beta, l)
                    * gaussian_moment(l, alpha + beta)
                    * gaussian_moment(0, alpha + beta).powi(2)
            };
            let integral =
                integrate(a, a, d, d) + 2.0 * integrate(a, b, d, e) + integrate(b, b, e, e);
            assert!(
                (contraction_overlap(&shell) - integral).abs() < 1e-12,
                "l={l}"
            );
        }
    }

    #[test]
    fn shell_validation_rejects_invalid_representation_and_atom() {
        let p = shell(1);
        p.validate(1).unwrap();
        for invalid in [
            Shell {
                pure: true,
                ..p.clone()
            },
            Shell {
                l: MAX_L + 1,
                ..p.clone()
            },
            Shell { atom: 1, ..p },
        ] {
            assert!(invalid.validate(1).is_err(), "{invalid:?}");
        }
    }

    #[test]
    fn shell_validation_rejects_invalid_contractions() {
        let s = shell(0);
        for invalid in [
            Shell {
                exponents_per_bohr2: vec![],
                coefficients: vec![],
                ..s.clone()
            },
            Shell {
                exponents_per_bohr2: vec![0.0, 1.0],
                ..s.clone()
            },
            Shell {
                exponents_per_bohr2: vec![-1.0, 1.0],
                ..s.clone()
            },
            Shell {
                exponents_per_bohr2: vec![f64::INFINITY, 1.0],
                ..s.clone()
            },
            Shell {
                exponents_per_bohr2: vec![1.0],
                ..s.clone()
            },
            Shell {
                exponents_per_bohr2: vec![1.0; MAX_PRIMITIVES + 1],
                coefficients: vec![1.0; MAX_PRIMITIVES + 1],
                ..s.clone()
            },
            Shell {
                coefficients: vec![f64::NAN, 1.0],
                ..s.clone()
            },
            Shell {
                coefficients: vec![0.0, 0.0],
                ..s
            },
        ] {
            assert!(invalid.validate(1).is_err(), "{invalid:?}");
        }
    }

    #[test]
    fn orbital_validation_checks_core_electrons_and_basis_presence() {
        let (frame, valid) = orbitals();
        valid.validate(&frame).unwrap();
        for invalid in [
            Orbitals {
                core_electrons: vec![9],
                ..valid.clone()
            },
            Orbitals {
                basis: None,
                ..valid.clone()
            },
            Orbitals {
                alpha: OrbitalSet {
                    rows: vec![],
                    coefficients: vec![],
                    ..valid.alpha.clone()
                },
                ..valid.clone()
            },
        ] {
            assert!(invalid.validate(&frame).is_err(), "{invalid:?}");
        }
    }

    #[test]
    fn orbital_validation_checks_rows_and_spectrum() {
        let (frame, valid) = orbitals();
        for alpha in [
            OrbitalSet {
                rows: vec![2, 0],
                ..valid.alpha.clone()
            },
            OrbitalSet {
                rows: vec![0, 0],
                ..valid.alpha.clone()
            },
            OrbitalSet {
                rows: vec![0, 3],
                ..valid.alpha.clone()
            },
            OrbitalSet {
                coefficients: vec![1.0],
                ..valid.alpha.clone()
            },
            OrbitalSet {
                energies_hartree: vec![0.0, -1.0, 1.0],
                ..valid.alpha.clone()
            },
            OrbitalSet {
                symmetries: vec!["a".into()],
                ..valid.alpha.clone()
            },
        ] {
            assert!(
                Orbitals {
                    alpha,
                    ..valid.clone()
                }
                .validate(&frame)
                .is_err()
            );
        }
    }

    #[test]
    fn orbital_validation_checks_each_spin_occupation_range() {
        let (frame, mut valid) = orbitals();
        valid.alpha.occupations = vec![2.0 + 0.5e-6, 0.3, -0.5e-6];
        valid.validate(&frame).unwrap();
        valid.alpha.occupations[0] = 2.0 + 2e-6;
        assert!(valid.validate(&frame).is_err());

        valid.alpha.occupations = vec![1.0 + 0.5e-6, 0.3, 0.0];
        valid.beta = Beta::SameOrbitals {
            occupations: vec![0.7, 1.0 + 0.5e-6, -0.5e-6],
        };
        valid.validate(&frame).unwrap();
        valid.beta = Beta::SameOrbitals {
            occupations: vec![0.7, 1.0 + 2e-6, 0.0],
        };
        assert!(valid.validate(&frame).is_err());

        let mut beta = valid.alpha.clone();
        beta.occupations = vec![0.25, 0.75, -0.5e-6];
        valid.beta = Beta::Unrestricted(beta.clone());
        valid.validate(&frame).unwrap();
        valid.alpha.occupations[0] = 1.0 + 2e-6;
        assert!(valid.validate(&frame).is_err());
        valid.alpha.occupations[0] = 1.0;
        beta.occupations[2] = -2e-6;
        valid.beta = Beta::Unrestricted(beta);
        assert!(valid.validate(&frame).is_err());
    }
}
