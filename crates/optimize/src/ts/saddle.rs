//! Partitioned rational-function saddle refinement.
//!
//! Written from Banerjee, Adams, Simons & Shepard, J. Phys. Chem. 89, 52
//! (1985), Baker, J. Comput. Chem. 7, 385 (1986), and Bofill, J. Comput.
//! Chem. 15, 1 (1994). P-RFO maximizes one Hessian eigenmode and minimizes
//! the orthogonal physical block. A Fletcher-style model ratio adjusts the trust
//! radius. Mode following uses maximum absolute overlap across the physical
//! spectrum, initially with the normalized NEB tangent, then with the prior
//! mode. Initial overlap below 0.3 falls back to the lowest physical mode.

use std::ops::ControlFlow;

use crate::{
    Level, Potential,
    linalg::{dot, norm, projected_eigen, symmetric_eigen},
};

use super::{Event, Minimum, TsError, TsOptions, emit, evaluate, modes};

const DIFFERENCE_STEP: f64 = 0.005;
const MIN_RADIUS: f64 = 0.01;
const MAX_RADIUS: f64 = 0.3;
const ENERGY_CANCELLATION_RELATIVE: f64 = 1e-3;

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

fn selected_mode(vectors: &[f64], n: usize, reference: &[f64]) -> usize {
    let mut best = 0;
    let mut overlap = -1.0;
    for (i, vector) in vectors.chunks_exact(n).enumerate() {
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

pub(super) struct Refinement {
    pub(super) point: Minimum,
    pub(super) converged: bool,
    /// A Hessian rebuilt at `point`, suitable for its frequency analysis.
    /// Absent if the last accepted step has only a Bofill Hessian model.
    pub(super) hessian: Option<Vec<f64>>,
}

/// Refine until both the projected gradient and the followed-mode Newton
/// displacement pass. For a Cartesian unit mode v the displacement in the IRC
/// metric is |g·v| / |λ| times ‖√M v‖. It must lie below 10% of the launch
/// length. Empty masses select the Euclidean metric for generic potentials.
pub(super) fn refine(
    p: &mut impl Potential,
    start: &Minimum,
    tangent: &[f64],
    masses: &[f64],
    options: &TsOptions,
    on_event: &mut impl FnMut(Event) -> ControlFlow<()>,
) -> Result<Refinement, TsError> {
    let mut x = start.positions.clone();
    let (mut energy, mut gradient) = evaluate(p, &x)?;
    let mut h = hessian(p, &x)?;
    let mut h_is_exact = true;
    let mut reference = tangent.to_vec();
    let scale: Vec<_> = if masses.is_empty() {
        vec![1.0; x.len()]
    } else {
        masses.iter().flat_map(|m| [m.sqrt(); 3]).collect()
    };
    let mut radius = 0.1;
    let criteria = Level::Normal.criteria();
    let mut first = true;
    let mut accepted = 0;

    while accepted <= options.saddle_max_steps {
        let rigid = if masses.is_empty() {
            Vec::new()
        } else {
            modes::rigid_basis(&x, None)
        };
        let mut g = gradient.clone();
        modes::project(&mut g, &rigid);
        let (max_gradient, rms_gradient) = gradient_metrics(&g);
        let gradient_passes =
            max_gradient <= criteria.max_gradient && rms_gradient <= criteria.rms_gradient;
        if gradient_passes && !h_is_exact {
            // A secant curvature can overestimate a soft mode and therefore
            // underestimate its residual displacement. Certify convergence
            // against a Hessian rebuilt at this accepted geometry.
            h = hessian(p, &x)?;
            h_is_exact = true;
        }
        let (values, vectors) =
            projected_eigen(h.clone(), x.len(), &rigid).map_err(TsError::Failed)?;
        if values.is_empty() {
            return Err(TsError::Failed(
                "no internal coordinates for saddle refinement".into(),
            ));
        }
        let mut followed = selected_mode(&vectors, x.len(), &reference);
        if first {
            let reference_length = norm(&reference);
            let overlap = if reference_length > 0.0 {
                dot(
                    &vectors[followed * x.len()..(followed + 1) * x.len()],
                    &reference,
                )
                .abs()
                    / reference_length
            } else {
                0.0
            };
            if overlap < 0.3 {
                followed = 0;
            }
        }
        reference.copy_from_slice(&vectors[followed * x.len()..(followed + 1) * x.len()]);
        first = false;
        emit(
            on_event,
            Event::Saddle {
                step: accepted,
                energy,
                max_gradient,
                eigenvalue: values[followed],
            },
        )?;
        let slope = dot(&g, &reference).abs();
        let amplitude = if values[followed] != 0.0 {
            slope / values[followed].abs()
        } else if slope == 0.0 {
            0.0
        } else {
            f64::INFINITY
        };
        let weighted_mode: Vec<_> = reference.iter().zip(&scale).map(|(v, m)| v * m).collect();
        let mode_displacement = amplitude * norm(&weighted_mode);
        if gradient_passes && mode_displacement < 0.1 * options.irc_step {
            return Ok(Refinement {
                point: Minimum {
                    positions: x,
                    energy,
                },
                converged: true,
                hessian: Some(h),
            });
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
        let mut ascent = 0.0;
        let mut descent = 0.0;
        for (i, (&value, vector)) in values.iter().zip(vectors.chunks_exact(x.len())).enumerate() {
            let amplitude = dot(vector, &step);
            let contribution = amplitude * (dot(vector, &g) + 0.5 * value * amplitude);
            if i == followed {
                ascent = contribution;
            } else {
                descent += contribution;
            }
        }
        let predicted = ascent + descent;
        let trial: Vec<_> = x.iter().zip(&step).map(|(xi, si)| xi + si).collect();
        let (trial_energy, trial_gradient) = evaluate(p, &trial)?;
        let actual = trial_energy - energy;
        // Opposing ascent/descent contributions can cancel even when their
        // sum exceeds an absolute roundoff threshold. Below 0.1% of their
        // separate magnitudes, use the squared-gradient model ratio instead.
        let cancellation_scale = ascent.abs() + descent.abs();
        let cancels = predicted.abs() <= 1e-12
            || predicted.abs() <= ENERGY_CANCELLATION_RELATIVE * cancellation_scale;
        let ratio = if !cancels {
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
            let rejected_length = radius.min(norm(&step));
            if rejected_length <= MIN_RADIUS {
                // A bound at or above this length cannot shorten the rejected step.
                break;
            }
            radius = (0.5 * rejected_length).max(MIN_RADIUS);
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
        h_is_exact = false;
        x = trial;
        energy = trial_energy;
        gradient = trial_gradient;
        accepted += 1;
        if options.hessian_every > 0 && accepted.is_multiple_of(options.hessian_every) {
            h = hessian(p, &x)?;
            h_is_exact = true;
        }
    }
    Ok(Refinement {
        point: Minimum {
            positions: x,
            energy,
        },
        converged: false,
        hessian: h_is_exact.then_some(h),
    })
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
        assert_eq!(selected_mode(&[1.0, 0.0, 0.0, 1.0], 2, &[0.0, 0.0]), 0);
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
        let Refinement {
            point: result,
            converged,
            ..
        } = refine(
            &mut Quadratic {
                analytic: true,
                balanced: true,
            },
            &start,
            &[1.0, 0.0],
            &[],
            &TsOptions::default(),
            &mut |_| ControlFlow::Continue(()),
        )
        .unwrap();
        assert!(converged);
        assert!(norm(&result.positions) < 1e-4);
    }

    struct SoftCubic {
        cubic: f64,
    }

    impl Potential for SoftCubic {
        fn evaluate(&mut self, x: &[f64], g: &mut [f64], _: Accuracy) -> Result<f64, EvalError> {
            g[0] = -0.001 * x[0] - self.cubic * x[0] * x[0];
            Ok(-0.0005 * x[0] * x[0] - self.cubic * x[0].powi(3) / 3.0)
        }

        fn hessian(&mut self, x: &[f64], h: &mut [f64]) -> Option<Result<(), EvalError>> {
            h[0] = -0.001 - 2.0 * self.cubic * x[0];
            Some(Ok(()))
        }
    }

    #[test]
    fn soft_mode_requires_small_displacement_even_when_gradient_passes() {
        let start = Minimum {
            positions: vec![0.06],
            energy: 0.0,
        };
        let options = TsOptions {
            saddle_max_steps: 0,
            ..TsOptions::default()
        };
        let Refinement { converged, .. } = refine(
            &mut SoftCubic { cubic: 0.0 },
            &start,
            &[1.0],
            &[],
            &options,
            &mut |_| ControlFlow::Continue(()),
        )
        .unwrap();
        assert!(!converged, "the residual mode displacement is 0.06 bohr");
        let options = TsOptions::default();
        let Refinement {
            point, converged, ..
        } = refine(
            &mut SoftCubic { cubic: 0.0 },
            &start,
            &[1.0],
            &[],
            &options,
            &mut |_| ControlFlow::Continue(()),
        )
        .unwrap();
        assert!(converged);
        assert!(point.positions[0].abs() < 0.1 * options.irc_step);
    }

    #[test]
    fn soft_mode_convergence_uses_rebuilt_curvature() {
        let options = TsOptions {
            irc_step: 0.2,
            hessian_every: 0,
            ..TsOptions::default()
        };
        let Refinement {
            point,
            converged,
            hessian,
        } = refine(
            &mut SoftCubic { cubic: 0.1 },
            &Minimum {
                positions: vec![0.1],
                energy: 0.0,
            },
            &[1.0],
            &[],
            &options,
            &mut |_| ControlFlow::Continue(()),
        )
        .unwrap();
        assert!(converged);
        let x = point.positions[0];
        assert_eq!(hessian.unwrap(), [-0.001 - 0.2 * x]);
        let displacement = (-0.001 * x - 0.1 * x * x).abs() / (-0.001 - 0.2 * x).abs();
        assert!(
            displacement < 0.1 * options.irc_step,
            "rebuilt-curvature displacement {displacement} at x={x}"
        );
    }

    struct CancellingCubic;

    impl Potential for CancellingCubic {
        fn evaluate(&mut self, x: &[f64], g: &mut [f64], _: Accuracy) -> Result<f64, EvalError> {
            g[0] = -2.0 * x[0] - 3.0 * (x[0] - 0.1).powi(2);
            g[1] = 2.0 * x[1] + 1e-6;
            Ok(-x[0] * x[0] + x[1] * x[1] + 1e-6 * x[1] - (x[0] - 0.1).powi(3))
        }

        fn hessian(&mut self, x: &[f64], h: &mut [f64]) -> Option<Result<(), EvalError>> {
            h.copy_from_slice(&[-2.0 - 6.0 * (x[0] - 0.1), 0.0, 0.0, 2.0]);
            Some(Ok(()))
        }
    }

    #[test]
    fn near_cancelling_energy_model_accepts_gradient_improvement() {
        let start = Minimum {
            positions: vec![0.1, 0.1],
            energy: 0.0,
        };
        let mut potential = CancellingCubic;
        let old_norm = norm(&evaluate(&mut potential, &start.positions).unwrap().1);
        let Refinement {
            point,
            converged,
            hessian,
        } = refine(
            &mut potential,
            &start,
            &[1.0, 0.0],
            &[],
            &TsOptions {
                saddle_max_steps: 1,
                ..TsOptions::default()
            },
            &mut |_| ControlFlow::Continue(()),
        )
        .unwrap();
        assert!(!converged);
        assert!(
            hessian.is_none(),
            "an updated model is not a rebuilt Hessian"
        );
        assert!(
            point.positions[0] < 0.099,
            "no useful P-RFO step was accepted"
        );
        let new_norm = norm(&evaluate(&mut potential, &point.positions).unwrap().1);
        assert!(new_norm < 0.95 * old_norm, "{old_norm} -> {new_norm}");
    }

    struct SteepCubic {
        curvature: f64,
        cubic: f64,
        evaluated: Vec<f64>,
    }

    impl Potential for SteepCubic {
        fn evaluate(&mut self, x: &[f64], g: &mut [f64], _: Accuracy) -> Result<f64, EvalError> {
            self.evaluated.push(x[0]);
            g[0] = 1.0 - self.curvature * x[0] - 3.0 * self.cubic * x[0].powi(2);
            Ok(x[0] - 0.5 * self.curvature * x[0].powi(2) - self.cubic * x[0].powi(3))
        }

        fn hessian(&mut self, x: &[f64], h: &mut [f64]) -> Option<Result<(), EvalError>> {
            h[0] = -self.curvature - 6.0 * self.cubic * x[0];
            Some(Ok(()))
        }
    }

    #[test]
    fn rejected_minimum_radius_trial_is_not_evaluated_again() {
        // The stronger initial curvature gives an unbounded RFO trial that already
        // fits inside the minimum radius. Tightening to the floor would repeat it.
        for (curvature, cubic) in [(2.0, 20_000.0), (400.0, 200_000.0)] {
            let mut potential = SteepCubic {
                curvature,
                cubic,
                evaluated: Vec::new(),
            };
            let Refinement {
                point, converged, ..
            } = refine(
                &mut potential,
                &Minimum {
                    positions: vec![0.0],
                    energy: 0.0,
                },
                &[1.0],
                &[],
                &TsOptions::default(),
                &mut |_| ControlFlow::Continue(()),
            )
            .unwrap();
            assert!(!converged);
            assert_eq!(point.positions, [0.0]);
            if curvature == 2.0 {
                assert!(
                    potential
                        .evaluated
                        .iter()
                        .any(|x| (*x - MIN_RADIUS).abs() < 1e-14)
                );
            } else {
                assert_eq!(potential.evaluated.len(), 2);
                assert!(potential.evaluated[1] < MIN_RADIUS);
            }
            assert!(
                potential
                    .evaluated
                    .windows(2)
                    .all(|pair| pair[0] != pair[1]),
                "repeated rejected trial: {:?}",
                potential.evaluated
            );
        }
    }

    struct SpectatorDoubleWell;

    impl Potential for SpectatorDoubleWell {
        fn evaluate(&mut self, x: &[f64], g: &mut [f64], _: Accuracy) -> Result<f64, EvalError> {
            let mut energy = 0.25 * (x[6] * x[6] - 1.0).powi(2);
            for i in 0..6 {
                let curvature = 0.01 * (i + 1) as f64;
                g[i] = curvature * x[i];
                energy += 0.5 * curvature * x[i] * x[i];
            }
            g[6] = x[6] * (x[6] * x[6] - 1.0);
            Ok(energy)
        }

        fn hessian(&mut self, x: &[f64], h: &mut [f64]) -> Option<Result<(), EvalError>> {
            h.fill(0.0);
            for i in 0..6 {
                h[i * 7 + i] = 0.01 * (i + 1) as f64;
            }
            h[48] = 3.0 * x[6] * x[6] - 1.0;
            Some(Ok(()))
        }
    }

    #[test]
    fn initial_mode_can_follow_reaction_above_six_spectators() {
        let mut eigenvalue = None;
        let Refinement { point, .. } = refine(
            &mut SpectatorDoubleWell,
            &Minimum {
                positions: vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.9],
                energy: 0.0,
            },
            &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1e-8],
            &[],
            &TsOptions {
                saddle_max_steps: 1,
                ..TsOptions::default()
            },
            &mut |event| {
                if let Event::Saddle {
                    eigenvalue: value, ..
                } = event
                {
                    eigenvalue.get_or_insert(value);
                }
                ControlFlow::Continue(())
            },
        )
        .unwrap();
        assert!((eigenvalue.unwrap() - 1.43).abs() < 1e-12);
        assert!(
            point.positions[6] < 0.9,
            "reaction coordinate moved toward its well"
        );
        assert!(norm(&point.positions[..6]) < 1e-12);
    }

    struct DiagonalQuadratic;

    impl Potential for DiagonalQuadratic {
        fn evaluate(&mut self, x: &[f64], g: &mut [f64], _: Accuracy) -> Result<f64, EvalError> {
            for (i, (g, x)) in g.iter_mut().zip(x).enumerate() {
                *g = (i + 1) as f64 * x;
            }
            Ok(0.5 * dot(x, g))
        }

        fn hessian(&mut self, x: &[f64], h: &mut [f64]) -> Option<Result<(), EvalError>> {
            h.fill(0.0);
            for i in 0..x.len() {
                h[i * x.len() + i] = (i + 1) as f64;
            }
            Some(Ok(()))
        }
    }

    #[test]
    fn diffuse_normalized_initial_overlap_falls_back_to_lowest_mode() {
        for scale in [1e-8, 1e8] {
            let mut tangent = vec![scale; 13];
            tangent[5] *= 1.05;
            let mut eigenvalue = None;
            refine(
                &mut DiagonalQuadratic,
                &Minimum {
                    positions: vec![0.0; 13],
                    energy: 0.0,
                },
                &tangent,
                &[],
                &TsOptions {
                    saddle_max_steps: 0,
                    ..TsOptions::default()
                },
                &mut |event| {
                    if let Event::Saddle {
                        eigenvalue: value, ..
                    } = event
                    {
                        eigenvalue = Some(value);
                    }
                    ControlFlow::Continue(())
                },
            )
            .unwrap();
            assert_eq!(eigenvalue, Some(1.0));
        }
    }

    #[test]
    fn bofill_matches_numerical_matrix_at_intermediate_mixing_angle() {
        let mut h = [2.0, 0.5, 0.5, -1.0];
        // With r = (-3, 9/2) and s = (1, 2), the SR1 weight is 16/65.
        bofill(&mut h, &[1.0, 2.0], &[0.0, 3.0]);
        let expected = [
            2086.0 / 1625.0,
            -1043.0 / 1625.0,
            -1043.0 / 1625.0,
            2959.0 / 1625.0,
        ];
        for (actual, expected) in h.iter().zip(expected) {
            assert!((actual - expected).abs() < 1e-13, "{h:?}");
        }
    }

    struct SoftBond;

    impl Potential for SoftBond {
        fn evaluate(&mut self, x: &[f64], g: &mut [f64], _: Accuracy) -> Result<f64, EvalError> {
            let delta: Vec<_> = x[..3].iter().zip(&x[3..]).map(|(a, b)| b - a).collect();
            let r = norm(&delta);
            let slope = -0.002 * (r - 2.0);
            for axis in 0..3 {
                g[axis] = -slope * delta[axis] / r;
                g[axis + 3] = -g[axis];
            }
            Ok(-0.001 * (r - 2.0).powi(2))
        }
    }

    #[test]
    fn soft_mode_displacement_uses_the_irc_mass_metric() {
        let options = TsOptions {
            saddle_max_steps: 0,
            ..TsOptions::default()
        };
        for (mass, expected) in [(1.0, true), (100.0, false)] {
            let Refinement { converged, .. } = refine(
                &mut SoftBond,
                &Minimum {
                    positions: vec![-1.005, 0.0, 0.0, 1.005, 0.0, 0.0],
                    energy: 0.0,
                },
                &[-1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                &[mass, mass],
                &options,
                &mut |_| ControlFlow::Continue(()),
            )
            .unwrap();
            assert_eq!(converged, expected, "atomic mass {mass}");
        }
    }
}
