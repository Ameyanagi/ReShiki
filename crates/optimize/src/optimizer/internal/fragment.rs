//! Fragment translations and exponential-map rotations for TRIC.
//!
//! L.-P. Wang and C. Song, J. Chem. Phys. 144, 214108 (2016),
//! doi:10.1063/1.4952956, Eqs. (2), (6)–(8), and (12), and B. K. P. Horn,
//! J. Opt. Soc. Am. A 4, 629–642 (1987), doi:10.1364/JOSAA.4.000629
//! \[Wang2016TRIC\] \[Horn1987Quaternion\]. Full references: docs/references/xtb.md.
//!
//! The quaternion fits the reference **to the current geometry**. Its principal
//! logarithm is multiplied by the reference radius of gyration, so all six
//! coordinates have units of length. The logarithm is smooth at the origin.
//! Recenter before π/2 and reject values beyond that hard chart boundary,
//! keeping local differences away from the principal logarithm's branch cut.
//! All analytic rotation derivatives share one eigensystem per fragment.

use std::array::from_fn;
use std::f64::consts::PI;

use crate::linalg::symmetric_eigen;

use super::geometry::{Vector, cross, dot, length, subtract};
type Quaternion = [f64; 4];

// A transverse RMS size below 10^-4 of the total size cannot define a useful
// axial twist. Keep a linear chart until the size doubles to avoid oscillating
// between charts at this numerical threshold.
const LINEAR_FRACTION: f64 = 1e-8;
const MAX_CHART_ANGLE: f64 = 0.5 * PI;
const REBUILD_ANGLE: f64 = 0.45 * PI;

/// One fragment, with a fixed reference geometry and rotation scale.
pub(super) struct Fragment {
    atoms: Vec<usize>,
    ncart: usize,
    // Centered reference positions divided by radius. Retain their mean so
    // derivatives include centering even after roundoff.
    reference: Vec<Vector>,
    reference_mean: Vector,
    radius: f64,
    line: Option<Line>,
}

struct Line {
    axis: Vector,
    // Longitudinal reference coordinates, divided by atom count.
    weights: Vec<f64>,
    mean_weight: f64,
}

enum Rotation {
    Full {
        q: Quaternion,
        values: [f64; 4],
        vectors: [Quaternion; 4],
    },
    Line {
        q: Quaternion,
        target: Vector,
        length: f64,
    },
}

impl Rotation {
    fn quaternion(&self) -> Quaternion {
        match self {
            Self::Full { q, .. } | Self::Line { q, .. } => *q,
        }
    }
}

impl Fragment {
    pub(super) fn new(atoms: &[usize], x: &[f64]) -> Option<Self> {
        if atoms.is_empty() || !x.len().is_multiple_of(3) {
            return None;
        }
        let mut unique = atoms.to_vec();
        unique.sort_unstable();
        if unique.windows(2).any(|pair| pair[0] == pair[1]) {
            return None;
        }
        let (_, mut reference) = centered(atoms, x)?;
        if atoms.len() == 1 {
            return Some(Self {
                atoms: atoms.to_vec(),
                ncart: x.len(),
                reference,
                reference_mean: [0.0; 3],
                radius: 0.0,
                line: None,
            });
        }
        let radius = rms_radius(&reference)?;
        for point in &mut reference {
            *point = point.map(|value| value / radius);
        }
        let reference_mean = mean(&reference);
        let (fraction, axis) = linearity(&reference)?;
        let line = if fraction <= LINEAR_FRACTION {
            let weights: Vec<f64> = reference
                .iter()
                .map(|&point| dot(subtract(point, reference_mean), axis) / atoms.len() as f64)
                .collect();
            let mean_weight = weights.iter().sum::<f64>() / atoms.len() as f64;
            Some(Line {
                axis,
                weights,
                mean_weight,
            })
        } else {
            None
        };
        Some(Self {
            atoms: atoms.to_vec(),
            ncart: x.len(),
            reference,
            reference_mean,
            radius,
            line,
        })
    }

    /// Linear fragments retain three redundant components of a rank-two
    /// rotation chart. A single atom has translations only.
    pub(super) fn len(&self) -> usize {
        if self.atoms.len() == 1 { 3 } else { 6 }
    }

    /// Centroid xyz, followed by the scaled principal rotation vector xyz.
    pub(super) fn values(&self, x: &[f64]) -> Option<Vec<f64>> {
        self.values_in_chart(x, true)
    }

    /// Unrestricted principal values for independent Wilson-row checks.
    #[cfg(test)]
    pub(super) fn raw_values(&self, x: &[f64]) -> Option<Vec<f64>> {
        self.values_in_chart(x, false)
    }

    fn values_in_chart(&self, x: &[f64], local: bool) -> Option<Vec<f64>> {
        let (centroid, current) = self.current(x)?;
        let mut result = centroid.to_vec();
        if self.len() == 6 {
            let q = self.rotation(&current)?.quaternion();
            // A displacement and its endpoint Wilson rows must use the same
            // logarithm chart. Component-wise wrapping cannot ensure this.
            if local && rotation_angle(q) >= MAX_CHART_ANGLE {
                return None;
            }
            result.extend(rotation_vector(q).map(|value| self.radius * value));
        }
        result
            .iter()
            .all(|value| value.is_finite())
            .then_some(result)
    }

    /// Full Cartesian Wilson B rows, in the same order as `values`.
    ///
    /// The full fit differentiates a simple eigenvector as
    /// `dq = sum_k e_k (e_k^T dF q)/(lambda - lambda_k)`. The small-angle
    /// logarithm uses Taylor series, avoiding subtraction of nearly equal
    /// numbers. Away from a vanishing eigenvalue gap or the pi branch, accuracy
    /// is limited by floating-point arithmetic and eigenproblem conditioning,
    /// with no finite-difference truncation error.
    pub(super) fn rows(&self, x: &[f64]) -> Option<Vec<Vec<f64>>> {
        let (_, current) = self.current(x)?;
        let mut rows = vec![vec![0.0; self.ncart]; self.len()];
        let inverse_count = 1.0 / self.atoms.len() as f64;
        for &atom in &self.atoms {
            for axis in 0..3 {
                rows[axis][3 * atom + axis] = inverse_count;
            }
        }
        if self.len() == 3 {
            return Some(rows);
        }
        let fit = self.rotation(&current)?;
        let q = fit.quaternion();
        // There is no unique derivative of the principal logarithm at pi.
        if q[0] <= 1e-8 {
            return None;
        }
        for (local, &atom) in self.atoms.iter().enumerate() {
            for axis in 0..3 {
                let dq = self.quaternion_derivative(&fit, local, axis);
                let derivative = logarithm_derivative(q, dq);
                for k in 0..3 {
                    rows[3 + k][3 * atom + axis] = self.radius * derivative[k];
                }
            }
        }
        rows.iter()
            .flatten()
            .all(|value| value.is_finite())
            .then_some(rows)
    }

    /// Whether this fixed reference no longer provides a reliable local chart.
    ///
    /// Near-linear references use the minimum rotation mapping the reference
    /// axis onto its current least-squares image. Rotation about that axis is
    /// intentionally absent. A substantial bend restores the full quaternion
    /// chart on rebuild. A newly linear fragment loses its axial twist.
    pub(super) fn needs_rebuild(&self, x: &[f64]) -> bool {
        let Some((_, current)) = self.current(x) else {
            return true;
        };
        if self.len() == 3 {
            return false;
        }
        let Some((fraction, _)) = linearity(&current) else {
            return true;
        };
        if match &self.line {
            Some(_) => fraction > 4.0 * LINEAR_FRACTION,
            None => fraction <= LINEAR_FRACTION,
        } {
            return true;
        }
        let Some(fit) = self.rotation(&current) else {
            return true;
        };
        rotation_angle(fit.quaternion()) >= REBUILD_ANGLE
    }

    fn current(&self, x: &[f64]) -> Option<(Vector, Vec<Vector>)> {
        if x.len() != self.ncart {
            return None;
        }
        centered(&self.atoms, x)
    }

    fn rotation(&self, current: &[Vector]) -> Option<Rotation> {
        if let Some(line) = &self.line {
            let mut image = [0.0; 3];
            for (&point, &weight) in current.iter().zip(&line.weights) {
                for k in 0..3 {
                    image[k] += weight * (point[k] / self.radius);
                }
            }
            let magnitude = length(image);
            if !(magnitude.is_finite() && magnitude > 1e-14) {
                return None;
            }
            let target = image.map(|value| value / magnitude);
            let cosine = dot(line.axis, target).clamp(-1.0, 1.0);
            let w = ((1.0 + cosine) * 0.5).sqrt();
            // The minimum line rotation has no unique limit at an antipode.
            if w <= 1e-8 {
                return None;
            }
            let v = cross(line.axis, target).map(|value| value / (2.0 * w));
            return Some(Rotation::Line {
                q: [w, v[0], v[1], v[2]],
                target,
                length: magnitude,
            });
        }
        let mut covariance = [[0.0; 3]; 3];
        let inverse_count = 1.0 / self.atoms.len() as f64;
        for (&reference, &point) in self.reference.iter().zip(current) {
            for i in 0..3 {
                for j in 0..3 {
                    covariance[i][j] += reference[i] * (point[j] / self.radius) * inverse_count;
                }
            }
        }
        let matrix = horn(covariance);
        let (eigenvalues, eigenvectors) =
            symmetric_eigen(matrix.into_iter().flatten().collect(), 4).ok()?;
        let values: [f64; 4] = eigenvalues.try_into().ok()?;
        let vectors: [Quaternion; 4] = from_fn(|i| from_fn(|j| eigenvectors[4 * i + j]));
        let spectral_scale = values.iter().fold(0.0_f64, |a, &b| a.max(b.abs()));
        if values[3] - values[2] <= 256.0 * f64::EPSILON * spectral_scale {
            return None;
        }
        let sign = if vectors[3][0] < 0.0 { -1.0 } else { 1.0 };
        let q = vectors[3].map(|value| sign * value);
        Some(Rotation::Full { q, values, vectors })
    }

    fn quaternion_derivative(&self, fit: &Rotation, atom: usize, axis: usize) -> Quaternion {
        match fit {
            Rotation::Full { q, values, vectors } => {
                let mut covariance = [[0.0; 3]; 3];
                for (i, row) in covariance.iter_mut().enumerate() {
                    row[axis] = (self.reference[atom][i] - self.reference_mean[i])
                        / (self.radius * self.atoms.len() as f64);
                }
                let differential = horn(covariance);
                let product: Quaternion = from_fn(|i| dot4(differential[i], *q));
                let mut dq = [0.0; 4];
                for k in 0..3 {
                    let coefficient = dot4(vectors[k], product) / (values[3] - values[k]);
                    for (entry, &component) in dq.iter_mut().zip(&vectors[k]) {
                        *entry += coefficient * component;
                    }
                }
                dq
            }
            Rotation::Line { q, target, length } => {
                let line = self.line.as_ref().expect("linear fit requires a line");
                // Centering of the target contributes the mean reference weight.
                let factor = (line.weights[atom] - line.mean_weight) / (self.radius * length);
                let db = from_fn(|k| factor * (f64::from(k == axis) - target[k] * target[axis]));
                let dw = dot(line.axis, db) / (4.0 * q[0]);
                let change = cross(line.axis, db);
                [
                    dw,
                    change[0] / (2.0 * q[0]) - q[1] * dw / q[0],
                    change[1] / (2.0 * q[0]) - q[2] * dw / q[0],
                    change[2] / (2.0 * q[0]) - q[3] * dw / q[0],
                ]
            }
        }
    }
}

fn centered(atoms: &[usize], x: &[f64]) -> Option<(Vector, Vec<Vector>)> {
    let anchor = super::geometry::point(x, *atoms.first()?)?;
    let mut points: Vec<Vector> = atoms
        .iter()
        .map(|&atom| super::geometry::point(x, atom).map(|point| subtract(point, anchor)))
        .collect::<Option<_>>()?;
    let offset = mean(&points);
    let centroid = from_fn(|k| anchor[k] + offset[k]);
    for point in &mut points {
        *point = subtract(*point, offset);
    }
    if centroid
        .iter()
        .chain(points.iter().flatten())
        .all(|value| value.is_finite())
    {
        Some((centroid, points))
    } else {
        None
    }
}

fn mean(points: &[Vector]) -> Vector {
    from_fn(|k| {
        points
            .iter()
            .map(|point| point[k] / points.len() as f64)
            .sum()
    })
}

fn rms_radius(points: &[Vector]) -> Option<f64> {
    let scale = points
        .iter()
        .flatten()
        .fold(0.0_f64, |a, &b| a.max(b.abs()));
    if !(scale.is_finite() && scale > 0.0) {
        return None;
    }
    let square = points
        .iter()
        .map(|point| {
            point
                .iter()
                .map(|&value| (value / scale).powi(2))
                .sum::<f64>()
                / points.len() as f64
        })
        .sum::<f64>();
    let radius = scale * square.sqrt();
    (radius.is_finite() && radius > 0.0).then_some(radius)
}

/// Fraction of squared size transverse to the principal axis, and that axis.
fn linearity(points: &[Vector]) -> Option<(f64, Vector)> {
    let radius = rms_radius(points)?;
    let mut covariance = [[0.0; 3]; 3];
    for point in points {
        for i in 0..3 {
            for j in 0..3 {
                covariance[i][j] += (point[i] / radius) * (point[j] / radius) / points.len() as f64;
            }
        }
    }
    let (values, vectors) = symmetric_eigen(covariance.into_iter().flatten().collect(), 3).ok()?;
    let fraction = ((values[0] + values[1]) / values.iter().sum::<f64>()).max(0.0);
    Some((fraction, [vectors[6], vectors[7], vectors[8]]))
}

fn horn([[a, b, c], [d, e, f], [g, h, i]]: [Vector; 3]) -> [Quaternion; 4] {
    [
        [a + e + i, f - h, g - c, b - d],
        [f - h, a - e - i, b + d, c + g],
        [g - c, b + d, e - a - i, f + h],
        [b - d, c + g, f + h, i - a - e],
    ]
}

fn rotation_angle(q: Quaternion) -> f64 {
    2.0 * length([q[1], q[2], q[3]]).atan2(q[0])
}

fn rotation_vector(q: Quaternion) -> Vector {
    let vector = [q[1], q[2], q[3]];
    let sine = length(vector);
    let factor = if sine < 1e-4 {
        let square = sine * sine;
        2.0 + square * (1.0 / 3.0 + square * (3.0 / 20.0))
    } else {
        2.0 * sine.atan2(q[0]) / sine
    };
    vector.map(|value| factor * value)
}

fn logarithm_derivative(q: Quaternion, dq: Quaternion) -> Vector {
    let vector = [q[1], q[2], q[3]];
    let change = [dq[1], dq[2], dq[3]];
    let sine = length(vector);
    let square = sine * sine;
    let (factor, radial) = if sine < 1e-4 {
        (
            2.0 + square * (1.0 / 3.0 + square * (3.0 / 20.0)),
            -4.0 / 3.0 - square * (2.0 / 5.0 + square * (3.0 / 14.0)),
        )
    } else {
        let angle = 2.0 * sine.atan2(q[0]);
        (angle / sine, (2.0 * q[0] * sine - angle) / (square * sine))
    };
    let coefficient = radial * dot(vector, change) - 2.0 * dq[0];
    from_fn(|k| factor * change[k] + vector[k] * coefficient)
}

fn dot4(a: Quaternion, b: Quaternion) -> f64 {
    (a[0] * b[0] + a[1] * b[1]) + (a[2] * b[2] + a[3] * b[3])
}
