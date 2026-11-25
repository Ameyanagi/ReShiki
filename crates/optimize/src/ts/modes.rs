//! Eckart translation/rotation projection and harmonic normal modes.
//!
//! The mass-weighted rigid subspace follows Wilson, Decius & Cross,
//! *Molecular Vibrations* (1955), and Miller, Handy & Adams, J. Chem. Phys.
//! 72, 99 (1980). Diagonalizing its orthogonal complement retains physical
//! zero-curvature modes.

use crate::linalg::{dot, norm, projected_eigen};

use super::TsError;

// CODATA 2022 values in joules, metres, kilograms and metres per second.
const HARTREE_J: f64 = 4.359_744_722_206e-18;
const BOHR_M: f64 = 5.291_772_105_44e-11;
const AMU_KG: f64 = 1.660_539_068_92e-27;
const LIGHT_M_S: f64 = 299_792_458.0;

fn wavenumber_factor() -> f64 {
    (HARTREE_J / (AMU_KG * BOHR_M * BOHR_M)).sqrt()
        / (2.0 * std::f64::consts::PI * LIGHT_M_S * 100.0)
}

/// Orthonormal translations and rotations. `None` uses ordinary Cartesian
/// coordinates. Supplied masses use q_i = sqrt(m_i) x_i and the center of mass.
/// A relative rotational-rank tolerance of 1e-10 recognizes linear molecules.
pub(super) fn rigid_basis(x: &[f64], masses: Option<&[f64]>) -> Vec<Vec<f64>> {
    let n = x.len();
    let atoms = n / 3;
    let mass = |i: usize| masses.map_or(1.0, |m| m[i]);
    let total: f64 = (0..atoms).map(mass).sum();
    let mut basis = Vec::with_capacity(6);
    for axis in 0..3 {
        let mut translation = vec![0.0; n];
        for (i, xyz) in translation.as_chunks_mut::<3>().0.iter_mut().enumerate() {
            xyz[axis] = (mass(i) / total).sqrt();
        }
        basis.push(translation);
    }
    // Subtract an atom first so that the frame does not affect rank decisions.
    let relative: Vec<_> = x.iter().enumerate().map(|(i, v)| v - x[i % 3]).collect();
    let span = relative.iter().fold(0.0_f64, |a, b| a.max(b.abs()));
    if span == 0.0 {
        return basis;
    }
    let mut center = [0.0; 3];
    for (i, xyz) in relative.as_chunks::<3>().0.iter().enumerate() {
        for k in 0..3 {
            center[k] += (mass(i) / total) * (xyz[k] / span);
        }
    }
    let mut rotations = [
        Vec::with_capacity(n),
        Vec::with_capacity(n),
        Vec::with_capacity(n),
    ];
    for (i, xyz) in relative.as_chunks::<3>().0.iter().enumerate() {
        let m = mass(i).sqrt();
        let [a, b, c] = std::array::from_fn(|k| m * (xyz[k] / span - center[k]));
        rotations[0].extend([0.0, -c, b]);
        rotations[1].extend([c, 0.0, -a]);
        rotations[2].extend([-b, a, 0.0]);
    }
    // Size pivoting and reorthogonalization make the rank test independent of
    // which laboratory axis happens to be closest to the molecular axis.
    let mut rotations = rotations.map(|v| (norm(&v), v));
    rotations.sort_by(|a, b| b.0.total_cmp(&a.0));
    let tolerance = 1e-10 * rotations[0].0;
    for (_, mut axis) in rotations {
        project(&mut axis, &basis);
        project(&mut axis, &basis);
        let length = norm(&axis);
        if length > tolerance {
            axis.iter_mut().for_each(|v| *v /= length);
            basis.push(axis);
        }
    }
    basis
}

pub(super) fn project(vector: &mut [f64], basis: &[Vec<f64>]) {
    for axis in basis {
        let weight = dot(vector, axis);
        for (v, u) in vector.iter_mut().zip(axis) {
            *v -= weight * u;
        }
    }
}

/// Eigenvalues and orthonormal physical mode rows of an already mass-weighted
/// Hessian, expressed in the full mass-weighted Cartesian coordinates.
pub(super) fn spectrum(
    x: &[f64],
    h_weighted: &[f64],
    masses: &[f64],
) -> Result<(Vec<f64>, Vec<f64>), TsError> {
    projected_eigen(h_weighted.to_vec(), x.len(), &rigid_basis(x, Some(masses)))
        .map_err(TsError::Failed)
}

/// Weight one triangle of the symmetric Cartesian Hessian and mirror each pair.
/// Independent multiplications can round differently and lose symmetry under projection.
pub(super) fn mass_weight(h: &[f64], inverse_sqrt: &[f64]) -> Vec<f64> {
    let n = inverse_sqrt.len();
    let mut weighted = vec![0.; h.len()];
    for i in 0..n {
        for j in 0..=i {
            let value = h[i * n + j] * inverse_sqrt[i] * inverse_sqrt[j];
            weighted[i * n + j] = value;
            weighted[j * n + i] = value;
        }
    }
    weighted
}

/// Harmonic wavenumbers [cm^-1], ascending with imaginary values negative,
/// and the lowest mass-weighted eigenvector converted to a unit Cartesian
/// displacement. The Hessian is in Eh/bohr² and masses are in amu.
pub(super) fn frequencies(
    x: &[f64],
    h: &[f64],
    masses: &[f64],
) -> Result<(Vec<f64>, Vec<f64>), TsError> {
    let n = x.len();
    let inverse_sqrt: Vec<_> = masses.iter().flat_map(|m| [m.sqrt().recip(); 3]).collect();
    let weighted = mass_weight(h, &inverse_sqrt);
    let (eigenvalues, vectors) = spectrum(x, &weighted, masses)?;
    let factor = wavenumber_factor();
    let frequencies = eigenvalues
        .iter()
        .map(|v| factor * v.abs().sqrt().copysign(*v))
        .collect();
    let mut mode: Vec<_> = vectors
        .iter()
        .take(n)
        .zip(&inverse_sqrt)
        .map(|(v, m)| v * m)
        .collect();
    let length = norm(&mode);
    if length > 0.0 {
        mode.iter_mut().for_each(|v| *v /= length);
    }
    Ok((frequencies, mode))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_and_bent_rigid_rank_is_frame_invariant() {
        for (x, expected) in [
            (vec![-1.0, -1.0, -1.0, 0.0, 0.0, 0.0, 2.0, 2.0, 2.0], 5),
            (vec![-1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0], 6),
        ] {
            let basis = rigid_basis(&x, Some(&[1.0, 12.0, 2.0]));
            assert_eq!(basis.len(), expected);
            for (i, v) in basis.iter().enumerate() {
                for (j, w) in basis.iter().enumerate() {
                    assert!((dot(v, w) - f64::from(i == j)).abs() < 1e-12);
                }
            }
        }
    }

    #[test]
    fn unequal_mass_diatomic_frequency_and_cartesian_mode() {
        let x = [-0.5, 0.0, 0.0, 0.5, 0.0, 0.0];
        let mut h = vec![0.0; 36];
        for (i, j, value) in [(0, 0, 0.25), (0, 3, -0.25), (3, 0, -0.25), (3, 3, 0.25)] {
            h[i * 6 + j] = value;
        }
        let (frequencies, mode) = frequencies(&x, &h, &[1.0, 4.0]).unwrap();
        assert_eq!(frequencies.len(), 1);
        assert!((frequencies[0] - wavenumber_factor() * 0.3125_f64.sqrt()).abs() < 1e-8);
        assert!((wavenumber_factor() - 5_140.487_14).abs() < 1e-4);
        assert!((norm(&mode) - 1.0).abs() < 1e-12);
        assert!((mode[0] / mode[3] + 4.0).abs() < 1e-12);
    }

    #[test]
    fn physical_zero_modes_are_retained() {
        let x = [-1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0];
        let (frequencies, mode) = frequencies(&x, &[0.0; 81], &[1.0; 3]).unwrap();
        assert_eq!(frequencies, vec![0.0; 3]);
        assert!((norm(&mode) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn mixed_mass_radial_springs_preserve_symmetry_and_projected_index() {
        let x = [0., 0., 0., 2., 0., 0., 1., 4., 0., 2., 1., 3.];
        let masses = [16., 12., 14., 35.45];
        let mut h = [0.; 144];
        // At its reference length, each spring contributes k u uᵀ.
        for a in 0..4 {
            for b in a + 1..4 {
                let axis: [f64; 3] = std::array::from_fn(|i| x[3 * a + i] - x[3 * b + i]);
                let length = norm(&axis);
                let unit = axis.map(|d| d / length);
                let k = if (a, b) == (0, 1) { -0.1 } else { 0.1 };
                for i in 0..3 {
                    for j in 0..3 {
                        let hij = k * (unit[i] * unit[j]);
                        for (m, n, sign) in [(a, a, 1.), (b, b, 1.), (a, b, -1.), (b, a, -1.)] {
                            h[(3 * m + i) * 12 + 3 * n + j] += sign * hij;
                        }
                    }
                }
            }
        }
        let (freq, _) = frequencies(&x, &h, &masses).unwrap();
        assert_eq!(freq.len(), 6);
        assert!(freq[0] < -super::super::IMAGINARY_CM1 && freq[1] > 0.);
        let inv: Vec<_> = masses.iter().flat_map(|m| [m.sqrt().recip(); 3]).collect();
        let weighted = mass_weight(&h, &inv);
        for i in 0..12 {
            for j in 0..i {
                assert_eq!(weighted[i * 12 + j], weighted[j * 12 + i]);
            }
        }
    }
}
