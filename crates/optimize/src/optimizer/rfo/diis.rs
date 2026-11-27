//! DIIS interpolation in the current RFO coordinate frame.
//!
//! Full references: docs/references/xtb.md.

use crate::linalg::{cholesky::Cholesky, dot, norm};
use crate::optimizer::Acceleration;

const MIN_POINTS: usize = 3;
const SMALL_STEP: f64 = 2.5e-3;
const LARGE_GRADIENT: f64 = 1e-3;

pub(crate) fn is_positive_definite(hessian: &[f64], dimension: usize) -> bool {
    Cholesky::new(hessian, dimension, 0., 64. * f64::EPSILON).is_some()
}

/// Safeguarded DIIS in a single coordinate chart. RMS controls use free
/// Cartesian components, while interpolation and errors use model coordinates.
#[allow(clippy::too_many_arguments)]
pub(crate) fn diis_direction<T>(
    hessian: &[f64],
    positive_definite: bool,
    offsets: &[Vec<f64>],
    gradients: &[&[f64]],
    energies: &[f64],
    rms_gradient: f64,
    rms_step: f64,
    acceleration: Acceleration,
    radius: f64,
    rfo_step: impl Fn(&[f64]) -> Option<Vec<f64>>,
    proposal: impl Fn(Vec<f64>) -> Option<T>,
) -> Option<T> {
    let n = gradients.last()?.len();
    let count = offsets.len();
    if count < MIN_POINTS || (acceleration == Acceleration::Gediis && rms_gradient > LARGE_GRADIENT)
    {
        return None;
    }
    if acceleration == Acceleration::Gediis && rms_step >= SMALL_STEP {
        let coefficients = energy_coefficients(offsets, gradients, energies)?;
        let mut step = combination(offsets, &coefficients);
        if norm(&step) > radius {
            return None;
        }
        let gradient = combination(gradients, &coefficients);
        // A vanishing weighted gradient needs no positive-model correction.
        if !positive_definite || gradient.iter().any(|&v| v != 0.) {
            for (value, correction) in step.iter_mut().zip(rfo_step(&gradient)?) {
                *value += correction;
            }
        }
        return proposal(step);
    }
    // Use the same current H^-1 for every accepted-point residual.
    let factor = Cholesky::new(hessian, n, 0., 64. * f64::EPSILON)?;
    let errors: Vec<_> = gradients
        .iter()
        .map(|g| factor.solve(g))
        .collect::<Option<_>>()?;
    for count in (MIN_POINTS..=count).rev() {
        let first = errors.len() - count;
        let Some(coefficients) = residual_coefficients(&errors[first..]) else {
            continue;
        };
        let reference = errors.last()?;
        let reference_length = norm(reference);
        let interpolation = combination(&offsets[first..], &coefficients);
        let mut step = combination(&errors[first..], &coefficients);
        for (value, position) in step.iter_mut().zip(&interpolation) {
            *value = position - *value;
        }
        let length = norm(&step);
        // Farkas & Schlegel, PCCP 4, 11 (2002), controls (a)-(c) [Farkas2002GDIIS].
        let cosine = -dot(&step, reference) / (length * reference_length);
        let minimum_cosine = [0.84, 0.71, 0.67][count - MIN_POINTS];
        if length.is_finite()
            && length > 0.
            && length <= 10. * reference_length.min(radius)
            && norm(&interpolation) <= radius
            && cosine >= minimum_cosine
            && let Some(direction) = proposal(step)
        {
            return Some(direction);
        }
    }
    None
}

fn combination(vectors: &[impl AsRef<[f64]>], coefficients: &[f64]) -> Vec<f64> {
    let mut result = vec![0.; vectors[0].as_ref().len()];
    for (vector, coefficient) in vectors.iter().zip(coefficients) {
        for (value, component) in result.iter_mut().zip(vector.as_ref()) {
            *value += coefficient * component;
        }
    }
    result
}

pub(crate) fn residual_coefficients(errors: &[Vec<f64>]) -> Option<Vec<f64>> {
    // Csaszar & Pulay, J. Mol. Struct. 114, 31 (1984) [Csaszar1984GDIIS]: minimize
    // ||sum c_i H^-1 g_i||^2 subject to sum c_i = 1.
    let scale = errors
        .iter()
        .map(|error| norm(error))
        .fold(f64::INFINITY, f64::min);
    if !(scale.is_finite() && scale > 0.) {
        return None;
    }
    let scaled: Vec<Vec<_>> = errors
        .iter()
        .map(|error| error.iter().map(|value| value / scale).collect())
        .collect();
    let count = errors.len();
    let gram: Vec<_> = scaled
        .iter()
        .flat_map(|left| scaled.iter().map(move |right| dot(left, right)))
        .collect();
    let weights = Cholesky::new(&gram, count, 0., 64. * f64::EPSILON)?.solve(&vec![1.; count])?;
    // Eq. (9) and control (d) of Farkas & Schlegel: these unnormalized
    // weights are c_i/||r||^2, with the shortest error scaled to unit norm.
    if weights.iter().any(|value| value.abs() > 1e8) {
        return None;
    }
    let sum: f64 = weights.iter().sum();
    if !(sum.is_finite() && sum > 0.) {
        return None;
    }
    let mut coefficients: Vec<_> = weights.iter().map(|value| value / sum).collect();
    coefficients[count - 1] = 1. - coefficients[..count - 1].iter().sum::<f64>();
    (coefficients
        .iter()
        .filter(|&&value| value > 0.)
        .sum::<f64>()
        <= 15.)
        .then_some(coefficients)
}

pub(crate) fn energy_coefficients(
    offsets: &[Vec<f64>],
    gradients: &[&[f64]],
    energies: &[f64],
) -> Option<Vec<f64>> {
    // Li & Frisch, JCTC 2, 835 (2006) [Li2006GEDIIS]: the interpolated first-order
    // energies give E(c) = sum c_i E_i - 1/2 sum_ij c_i c_j A_ij,
    // A_ij = (g_i-g_j).(q_i-q_j), on c_i >= 0, sum c_i = 1.
    let count = energies.len();
    let mut quadratic = vec![0.; count * count];
    for i in 0..count {
        for j in 0..i {
            let value: f64 = gradients[i]
                .iter()
                .zip(gradients[j])
                .zip(offsets[i].iter().zip(&offsets[j]))
                .map(|((gi, gj), (qi, qj))| -(gi - gj) * (qi - qj))
                .sum();
            quadratic[i * count + j] = value;
            quadratic[j * count + i] = value;
        }
    }
    if energies
        .iter()
        .chain(&quadratic)
        .any(|value| !value.is_finite())
    {
        return None;
    }
    let mut best = None;
    let mut lowest = f64::INFINITY;
    // At most 31 faces. A constrained minimum lies at a stationary point
    // of a convex face or at a smaller face. Singular flat faces also have
    // a boundary minimizer, so skipping their singular systems is safe.
    for mask in 1_usize..1 << count {
        let face: Vec<_> = (0..count).filter(|&i| mask & (1 << i) != 0).collect();
        let base = *face.last()?;
        let dimension = face.len() - 1;
        let mut coefficients = vec![0.; count];
        coefficients[base] = 1.;
        if dimension > 0 {
            let mut reduced = vec![0.; dimension * dimension];
            let mut rhs = vec![0.; dimension];
            for (i, &left) in face[..dimension].iter().enumerate() {
                rhs[i] = energies[base] - energies[left] - quadratic[left * count + base];
                for (j, &right) in face[..dimension].iter().enumerate() {
                    reduced[i * dimension + j] = quadratic[left * count + right]
                        - quadratic[left * count + base]
                        - quadratic[base * count + right];
                }
            }
            let Some(solution) = Cholesky::new(&reduced, dimension, 0., 64. * f64::EPSILON)
                .and_then(|factor| factor.solve(&rhs))
            else {
                continue;
            };
            for (&index, &value) in face[..dimension].iter().zip(&solution) {
                coefficients[index] = value;
                coefficients[base] -= value;
            }
            if coefficients.iter().any(|&value| value < 0.) {
                continue;
            }
        }
        let energy = dot(energies, &coefficients)
            + 0.5
                * quadratic
                    .chunks_exact(count)
                    .zip(&coefficients)
                    .map(|(row, value)| value * dot(row, &coefficients))
                    .sum::<f64>();
        if energy.is_finite() && energy < lowest {
            lowest = energy;
            best = Some(coefficients);
        }
    }
    best
}
