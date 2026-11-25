//! Partitioned rational-function saddle refinement.
//!
//! Written from Banerjee, Adams, Simons & Shepard, J. Phys. Chem. 89, 52
//! (1985), Baker, J. Comput. Chem. 7, 385 (1986), and Bofill, J. Comput.
//! Chem. 15, 1 (1994). P-RFO maximizes one Hessian eigenmode and minimizes
//! the orthogonal physical block. A Fletcher-style model ratio adjusts the trust
//! radius. Mode following uses maximum absolute overlap, initially with the
//! NEB tangent among the six lowest physical modes, then with the prior mode.

use std::ops::ControlFlow;

use crate::{
    Level, Potential,
    linalg::{dot, norm, projected_eigen, symmetric_eigen},
};

use super::{Event, Minimum, TsError, TsOptions, emit, evaluate, modes};

const DIFFERENCE_STEP: f64 = 0.005;
const MIN_RADIUS: f64 = 0.01;
const MAX_RADIUS: f64 = 0.3;

/// Analytic Hessian when available, otherwise central gradient differences
/// with a 0.005 bohr displacement. Both paths explicitly symmetrize the result.
pub(super) fn hessian(p: &mut impl Potential, x: &[f64]) -> Result<Vec<f64>, TsError> {
    let n = x.len();
    let mut h = vec![0.0; n * n];
    if let Some(result) = p.hessian(x, &mut h) {
        result.map_err(TsError::from)?;
    } else {
        let mut displaced = x.to_vec();
        for j in 0..n {
            displaced[j] = x[j] + DIFFERENCE_STEP;
            let (_, plus) = evaluate(p, &displaced)?;
            displaced[j] = x[j] - DIFFERENCE_STEP;
            let (_, minus) = evaluate(p, &displaced)?;
            displaced[j] = x[j];
            for i in 0..n {
                h[i * n + j] = (plus[i] - minus[i]) / (2.0 * DIFFERENCE_STEP);
            }
        }
    }
    if h.iter().any(|v| !v.is_finite()) {
        return Err(TsError::Failed(
            "non-finite transition-state Hessian".into(),
        ));
    }
    for i in 0..n {
        for j in 0..i {
            let average = 0.5 * h[i * n + j] + 0.5 * h[j * n + i];
            h[i * n + j] = average;
            h[j * n + i] = average;
        }
    }
    Ok(h)
}

/// Bofill's SR1/PSB mixture. With r = y - Hs, the SR1 weight is
/// cos²(r,s). Unit directions avoid products of squared norms and division
/// by r.s, so orthogonal residuals give the finite PSB limit.
/// This update accepts either curvature sign and satisfies H_new s = y.
pub(super) fn bofill(h: &mut [f64], s: &[f64], y: &[f64]) {
    let n = s.len();
    let step_length = norm(s);
    if step_length <= 1e-12 {
        return;
    }
    let mut residual: Vec<_> = h
        .chunks_exact(n)
        .zip(y)
        .map(|(row, yi)| yi - dot(row, s))
        .collect();
    let residual_length = norm(&residual);
    if residual_length <= 1e-15 {
        return;
    }
    residual.iter_mut().for_each(|v| *v /= residual_length);
    let direction: Vec<_> = s.iter().map(|v| v / step_length).collect();
    let cosine = dot(&residual, &direction).clamp(-1.0, 1.0);
    let psb = 1.0 - cosine * cosine;
    let scale = residual_length / step_length;
    for i in 0..n {
        for j in 0..=i {
            let delta = scale
                * (cosine * residual[i] * residual[j]
                    + psb
                        * (residual[i] * direction[j] + direction[i] * residual[j]
                            - cosine * direction[i] * direction[j]));
            h[i * n + j] += delta;
            h[j * n + i] = h[i * n + j];
        }
    }
}

fn selected_mode(vectors: &[f64], n: usize, reference: &[f64], candidates: usize) -> usize {
    let mut best = 0;
    let mut overlap = -1.0;
    for (i, vector) in vectors.chunks_exact(n).take(candidates).enumerate() {
        let current = dot(vector, reference).abs();
        if current > overlap {
            best = i;
            overlap = current;
        }
    }
    best
}

/// Extreme eigenvector of the augmented RFO block, in intermediate
/// normalization. A vanishing last component is an uphill escape direction
/// at a stationary point with the wrong Hessian index. Cap it at the radius.
fn partition_step(
    values: &[f64],
    gradient: &[f64],
    maximize: bool,
    radius: f64,
) -> Result<Vec<f64>, TsError> {
    let dim = values.len();
    if dim == 0 {
        return Ok(Vec::new());
    }
    let size = dim + 1;
    let mut augmented = vec![0.0; size * size];
    for i in 0..dim {
        augmented[i * size + i] = values[i];
        augmented[i * size + dim] = gradient[i];
        augmented[dim * size + i] = gradient[i];
    }
    let (_, vectors) = symmetric_eigen(augmented, size).map_err(TsError::Failed)?;
    let row = if maximize {
        &vectors[dim * size..]
    } else {
        &vectors[..size]
    };
    let mut step = row[..dim].to_vec();
    let scale = if row[dim].abs() > 1e-10 {
        row[dim].recip()
    } else {
        radius / norm(&step)
    };
    step.iter_mut().for_each(|v| *v *= scale);
    Ok(step)
}

fn rfo_step(
    values: &[f64],
    vectors: &[f64],
    g: &[f64],
    followed: usize,
    radius: f64,
) -> Result<Vec<f64>, TsError> {
    let n = g.len();
    let mode_gradient: Vec<_> = vectors.chunks_exact(n).map(|v| dot(v, g)).collect();
    let uphill = partition_step(
        &[values[followed]],
        &[mode_gradient[followed]],
        true,
        radius,
    )?;
    let other: Vec<_> = (0..values.len()).filter(|&i| i != followed).collect();
    let downhill_values: Vec<_> = other.iter().map(|&i| values[i]).collect();
    let downhill_gradient: Vec<_> = other.iter().map(|&i| mode_gradient[i]).collect();
    let downhill = partition_step(&downhill_values, &downhill_gradient, false, radius)?;
    let mut step: Vec<_> = vectors[followed * n..(followed + 1) * n]
        .iter()
        .map(|v| uphill[0] * v)
        .collect();
    for (&i, amplitude) in other.iter().zip(downhill) {
        for (s, v) in step.iter_mut().zip(&vectors[i * n..(i + 1) * n]) {
            *s += amplitude * v;
        }
    }
    let length = norm(&step);
    if length > radius {
        step.iter_mut().for_each(|v| *v *= radius / length);
    }
    Ok(step)
}

fn gradient_metrics(g: &[f64]) -> (f64, f64) {
    (
        g.iter().fold(0.0_f64, |a, v| a.max(v.abs())),
        norm(g) / (g.len() as f64).sqrt(),
    )
}

pub(super) fn refine(
    p: &mut impl Potential,
    start: &Minimum,
    tangent: &[f64],
    molecular: bool,
    options: &TsOptions,
    on_event: &mut impl FnMut(Event) -> ControlFlow<()>,
) -> Result<(Minimum, bool), TsError> {
    let mut x = start.positions.clone();
    let (mut energy, mut gradient) = evaluate(p, &x)?;
    let mut h = hessian(p, &x)?;
    let mut reference = tangent.to_vec();
    let mut radius = 0.1;
    let criteria = Level::Normal.criteria();
    let mut first = true;
    let mut accepted = 0;
    let mut stalled = 0;

    while accepted <= options.saddle_max_steps {
        let rigid = if molecular {
            modes::rigid_basis(&x, None)
        } else {
            Vec::new()
        };
        let mut g = gradient.clone();
        modes::project(&mut g, &rigid);
        let (values, vectors) =
            projected_eigen(h.clone(), x.len(), &rigid).map_err(TsError::Failed)?;
        if values.is_empty() {
            return Err(TsError::Failed(
                "no internal coordinates for saddle refinement".into(),
            ));
        }
        let candidates = if first {
            values.len().min(6)
        } else {
            values.len()
        };
        let followed = selected_mode(&vectors, x.len(), &reference, candidates);
        reference.copy_from_slice(&vectors[followed * x.len()..(followed + 1) * x.len()]);
        first = false;
        let (max_gradient, rms_gradient) = gradient_metrics(&g);
        emit(
            on_event,
            Event::Saddle {
                step: accepted,
                energy,
                max_gradient,
                eigenvalue: values[followed],
            },
        )?;
        if max_gradient <= criteria.max_gradient && rms_gradient <= criteria.rms_gradient {
            // The following frequency stage rebuilds the final Hessian here.
            return Ok((
                Minimum {
                    positions: x,
                    energy,
                },
                true,
            ));
        }
        if accepted == options.saddle_max_steps {
            break;
        }
        let step = rfo_step(&values, &vectors, &g, followed, radius)?;
        let mut predicted_gradient: Vec<_> = h
            .chunks_exact(x.len())
            .zip(&g)
            .map(|(row, gi)| gi + dot(row, &step))
            .collect();
        modes::project(&mut predicted_gradient, &rigid);
        let predicted = dot(&g, &step) + 0.5 * (dot(&step, &predicted_gradient) - dot(&step, &g));
        let trial: Vec<_> = x.iter().zip(&step).map(|(xi, si)| xi + si).collect();
        let (trial_energy, trial_gradient) = evaluate(p, &trial)?;
        let actual = trial_energy - energy;
        // Opposing ascent/descent contributions can cancel in the energy.
        // In that limit use the squared-gradient model ratio instead.
        let ratio = if predicted.abs() > 1e-12 {
            actual / predicted
        } else {
            let old = dot(&g, &g);
            let mut trial_g = trial_gradient.clone();
            modes::project(&mut trial_g, &rigid);
            let expected = old - dot(&predicted_gradient, &predicted_gradient);
            if expected.abs() > 1e-24 {
                (old - dot(&trial_g, &trial_g)) / expected
            } else if (actual - predicted).abs() < 1e-12 {
                1.0
            } else {
                0.0
            }
        };
        if !ratio.is_finite() || ratio < 0.1 {
            radius = (0.5 * radius).max(MIN_RADIUS);
            stalled += 1;
            if stalled >= 8 {
                break;
            }
            continue;
        }
        if !(0.25..=1.75).contains(&ratio) {
            radius = (0.5 * radius).max(MIN_RADIUS);
        } else if (0.75..=1.25).contains(&ratio) && norm(&step) >= 0.8 * radius {
            radius = (1.5 * radius).min(MAX_RADIUS);
        }
        let y: Vec<_> = trial_gradient
            .iter()
            .zip(&gradient)
            .map(|(new, old)| new - old)
            .collect();
        bofill(&mut h, &step, &y);
        x = trial;
        energy = trial_energy;
        gradient = trial_gradient;
        accepted += 1;
        stalled = 0;
        if options.hessian_every > 0 && accepted.is_multiple_of(options.hessian_every) {
            h = hessian(p, &x)?;
        }
    }
    Ok((
        Minimum {
            positions: x,
            energy,
        },
        false,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Accuracy, EvalError};

    #[test]
    fn bofill_is_symmetric_and_satisfies_indefinite_secant() {
        for y in [[-2.0, 3.0], [0.0, 2.0], [1.0, 1.0]] {
            for s in [[1.0, 0.0], [0.3, -0.4], [1e-6, -2e-6]] {
                let mut h = [1.0, 0.0, 0.0, 1.0];
                bofill(&mut h, &s, &y);
                assert_eq!(h[1], h[2]);
                for (row, target) in h.as_chunks::<2>().0.iter().zip(y) {
                    assert!((dot(row, &s) - target).abs() < 1e-12);
                }
            }
        }
    }

    #[test]
    fn zero_tangent_follows_lowest_mode() {
        assert_eq!(selected_mode(&[1.0, 0.0, 0.0, 1.0], 2, &[0.0, 0.0], 2), 0);
    }

    struct Quadratic {
        analytic: bool,
        balanced: bool,
    }

    impl Potential for Quadratic {
        fn evaluate(&mut self, x: &[f64], g: &mut [f64], _: Accuracy) -> Result<f64, EvalError> {
            let stable = if self.balanced { 2.0 } else { 3.0 };
            g[0] = -2.0 * x[0];
            g[1] = stable * x[1];
            Ok(-x[0] * x[0] + 0.5 * stable * x[1] * x[1])
        }

        fn hessian(&mut self, _: &[f64], h: &mut [f64]) -> Option<Result<(), EvalError>> {
            if !self.analytic {
                return None;
            }
            let stable = if self.balanced { 2.0 } else { 3.0 };
            h.copy_from_slice(&[-2.0, 0.0, 0.0, stable]);
            Some(Ok(()))
        }
    }

    #[test]
    fn finite_difference_matches_analytic_hessian() {
        assert!(
            hessian(
                &mut Quadratic {
                    analytic: false,
                    balanced: false
                },
                &[0.2, -0.4]
            )
            .unwrap()
            .iter()
            .zip([-2.0, 0.0, 0.0, 3.0])
            .all(|(a, b)| (a - b).abs() < 1e-12)
        );
    }

    #[test]
    fn rfo_converges_to_quadratic_saddle_with_cancelling_energy_change() {
        let start = Minimum {
            positions: vec![0.3, 0.3],
            energy: 0.0,
        };
        let (result, converged) = refine(
            &mut Quadratic {
                analytic: true,
                balanced: true,
            },
            &start,
            &[1.0, 0.0],
            false,
            &TsOptions::default(),
            &mut |_| ControlFlow::Continue(()),
        )
        .unwrap();
        assert!(converged);
        assert!(norm(&result.positions) < 1e-4);
    }
}
