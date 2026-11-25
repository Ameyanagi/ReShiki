//! Geometry DIIS from accepted points in one approximate-normal-coordinate frame.

use std::collections::VecDeque;

use crate::linalg::{dot, norm};
use crate::optimizer::{Acceleration, Coordinates, Point};

use super::{Anc, RfoDirection};

const HISTORY: usize = 5;
const MIN_POINTS: usize = 3;
const SMALL_STEP: f64 = 2.5e-3;
const LARGE_GRADIENT: f64 = 1e-3;

struct Sample {
    // Keep free Cartesian positions: subtract before projection, avoiding
    // cancellation of a large common translation in absolute ANC positions.
    position: Vec<f64>,
    gradient: Vec<f64>,
    energy: f64,
}

#[derive(Default)]
pub(in crate::optimizer) struct History {
    samples: VecDeque<Sample>,
    gradient: Vec<f64>,
}

impl History {
    pub fn clear(&mut self) {
        self.samples.clear();
        self.gradient.clear();
    }

    pub fn push(&mut self, anc: &Anc, point: &Point, coords: &Coordinates) {
        if self.samples.len() == HISTORY {
            self.samples.pop_front();
        }
        self.samples.push_back(Sample {
            position: coords.indices.iter().map(|&i| point.positions[i]).collect(),
            gradient: anc.to_anc(&point.active_gradient),
            energy: point.energy,
        });
        self.gradient.clone_from(&point.active_gradient);
    }

    pub fn direction(
        &self,
        anc: &Anc,
        acceleration: Acceleration,
        radius: f64,
    ) -> Option<RfoDirection> {
        if self.samples.len() < MIN_POINTS {
            return None;
        }
        let rms_denominator = (anc.nfree as f64).sqrt();
        if acceleration == Acceleration::Gediis
            && norm(&self.gradient) / rms_denominator > LARGE_GRADIENT
        {
            return None;
        }
        let current = self.samples.back()?;
        let offsets: Vec<_> = self
            .samples
            .iter()
            .map(|sample| {
                let difference: Vec<_> = sample
                    .position
                    .iter()
                    .zip(&current.position)
                    .map(|(old, new)| old - new)
                    .collect();
                anc.to_anc(&difference)
            })
            .collect();
        if acceleration == Acceleration::Gediis
            // Measure the last accepted displacement, projected into this
            // ANC frame, using the optimizer's free Cartesian RMS convention.
            && norm(&offsets[offsets.len() - 2]) / rms_denominator >= SMALL_STEP
        {
            let energies: Vec<_> = self
                .samples
                .iter()
                .map(|sample| sample.energy - current.energy)
                .collect();
            let gradients: Vec<_> = self
                .samples
                .iter()
                .map(|sample| sample.gradient.as_slice())
                .collect();
            let coefficients = energy_coefficients(&offsets, &gradients, &energies)?;
            let mut step = combination(&offsets, &coefficients);
            if norm(&step) > radius {
                return None;
            }
            let gradient = combination(&gradients, &coefficients);
            // For a positive model, a zero interpolated gradient needs no
            // correction; the interpolation itself can still give descent.
            if !anc.positive_definite || gradient.iter().any(|&value| value != 0.) {
                for (value, correction) in step.iter_mut().zip(anc.rfo_step(&gradient)?) {
                    *value += correction;
                }
            }
            return anc.model_direction(&self.gradient, &current.gradient, &step);
        }
        // Factor the current Hessian once, and use that same H^-1 for every
        // GDIIS error. An indefinite or ill-conditioned model uses plain RFO.
        let factor = Cholesky::new(&anc.hessian, current.gradient.len())?;
        let errors: Vec<_> = self
            .samples
            .iter()
            .map(|sample| factor.solve(&sample.gradient))
            .collect::<Option<_>>()?;
        // Remove older points from this proposal when they give unstable or
        // excessive extrapolation; accepted history itself remains available.
        for count in (MIN_POINTS..=errors.len()).rev() {
            let first = errors.len() - count;
            if let Some(coefficients) = residual_coefficients(&errors[first..])
                && let Some(direction) = self.proposal(
                    anc,
                    &offsets[first..],
                    &errors[first..],
                    &coefficients,
                    radius,
                )
            {
                return Some(direction);
            }
        }
        None
    }

    fn proposal(
        &self,
        anc: &Anc,
        offsets: &[Vec<f64>],
        errors: &[Vec<f64>],
        coefficients: &[f64],
        radius: f64,
    ) -> Option<RfoDirection> {
        let reference = errors.last()?;
        let reference_length = norm(reference);
        let limit = 10. * reference_length.min(radius);
        let interpolation = combination(offsets, coefficients);
        let mut step = combination(errors, coefficients);
        for (value, position) in step.iter_mut().zip(&interpolation) {
            *value = position - *value;
        }
        let length = norm(&step);
        // Farkas & Schlegel, PCCP 4, 11 (2002), controls (a)-(c).
        // Limiting the interpolation separately also catches cancellation
        // between a distant extrapolated geometry and its Newton correction.
        let cosine = -dot(&step, reference) / (length * reference_length);
        let minimum_cosine = [0.84, 0.71, 0.67][coefficients.len() - MIN_POINTS];
        if !(length.is_finite()
            && length > 0.
            && length <= limit
            && norm(&interpolation) <= radius
            && cosine >= minimum_cosine)
        {
            return None;
        }
        anc.model_direction(&self.gradient, &self.samples.back()?.gradient, &step)
    }
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

pub(super) fn residual_coefficients(errors: &[Vec<f64>]) -> Option<Vec<f64>> {
    // Csaszar & Pulay, J. Mol. Struct. 114, 31 (1984): minimize
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
    let weights = Cholesky::new(&gram, count)?.solve(&vec![1.; count])?;
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

pub(super) fn energy_coefficients(
    offsets: &[Vec<f64>],
    gradients: &[&[f64]],
    energies: &[f64],
) -> Option<Vec<f64>> {
    // Li & Frisch, JCTC 2, 835 (2006): the interpolated first-order
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
            let Some(solution) =
                Cholesky::new(&reduced, dimension).and_then(|factor| factor.solve(&rhs))
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

// Golub & Van Loan, Matrix Computations: a scaled Cholesky factorization
// followed by triangular solves. Scaling also keeps the small DIIS systems
// insensitive to the absolute gradient and energy units.
struct Cholesky {
    lower: Vec<f64>,
    dimension: usize,
    scale: f64,
}

impl Cholesky {
    fn new(matrix: &[f64], dimension: usize) -> Option<Self> {
        let scale = matrix
            .iter()
            .fold(0_f64, |largest, value| largest.max(value.abs()));
        if !(scale.is_finite() && scale > 0.) || matrix.iter().any(|value| !value.is_finite()) {
            return None;
        }
        let mut lower = vec![0.; matrix.len()];
        for i in 0..dimension {
            for j in 0..=i {
                let value = matrix[i * dimension + j] / scale
                    - dot(
                        &lower[i * dimension..i * dimension + j],
                        &lower[j * dimension..j * dimension + j],
                    );
                if i == j {
                    if !value.is_finite() || value <= 64. * f64::EPSILON {
                        return None;
                    }
                    lower[i * dimension + j] = value.sqrt();
                } else {
                    lower[i * dimension + j] = value / lower[j * dimension + j];
                }
            }
        }
        Some(Self {
            lower,
            dimension,
            scale,
        })
    }

    fn solve(&self, rhs: &[f64]) -> Option<Vec<f64>> {
        let n = self.dimension;
        let mut result = vec![0.; n];
        for i in 0..n {
            result[i] = (rhs[i] / self.scale - dot(&self.lower[i * n..i * n + i], &result[..i]))
                / self.lower[i * n + i];
        }
        for i in (0..n).rev() {
            for j in i + 1..n {
                result[i] -= self.lower[j * n + i] * result[j];
            }
            result[i] /= self.lower[i * n + i];
        }
        result
            .iter()
            .all(|value| value.is_finite())
            .then_some(result)
    }
}
