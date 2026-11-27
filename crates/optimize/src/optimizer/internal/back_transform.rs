//! Iterative back-transformation after Pulay & Fogarasi, J. Chem. Phys.
//! **96**, 2856 (1992), and Peng et al., J. Comput. Chem. **17**, 49 (1996)
//! \[Pulay1992BackTransform\] \[Peng1996Redundant\].
//!
//! Full references: docs/references/xtb.md.

use super::{InternalModel, metric::apply};
use crate::linalg::norm;

pub(crate) struct BackTransform {
    pub(crate) positions: Vec<f64>,
    pub(crate) converged: bool,
}

impl InternalModel {
    /// Solve Uᵀ(q(x) − q(x₀)) = target in the local chart. Wrap each proper
    /// torsion before applying Uᵀ. Reuse the starting Wilson matrix (Peng et al.)
    /// while the residual contracts, refreshing it for slow or stalled trials.
    pub(crate) fn back_transform(&self, origin: &[f64], target: &[f64]) -> Option<BackTransform> {
        if target.len() != self.dimension() {
            return None;
        }
        let initial_norm = norm(target);
        if !initial_norm.is_finite() {
            return None;
        }
        let q0 = self.values_at(origin)?;
        let mut x = origin.to_vec();
        let mut residual = target.to_vec();
        let mut previous_norm = initial_norm;
        let mut increases = 0;
        let mut first = None;
        let mut best = None;
        let mut best_norm = f64::INFINITY;
        let mut tangent = self.tangent_at(origin)?;
        let mut refresh = false;
        for iteration in 0..25 {
            profile_phase!("back-transform iter");
            if refresh {
                let Some(next) = self.tangent_at(&x) else {
                    break;
                };
                tangent = next;
            }
            // The outer residual verifies an ordinary chord correction. Use
            // the refined linear solve when contraction required a fresh tangent.
            let dx = if refresh {
                tangent.cartesian_step(&residual)
            } else {
                tangent.cartesian_correction(&residual)
            };
            let Some(dx) = dx else {
                break;
            };
            let rms = norm(&dx) / (self.free.len() as f64).sqrt();
            if !rms.is_finite() {
                break;
            }
            for (&i, d) in self.free.iter().zip(dx) {
                x[i] += d;
            }
            // Guarded values keep residuals and Newton rows in this same
            // chart. Reject an invalid iterate before recording a fallback.
            let Some(q) = self.chart.values(&x) else {
                break;
            };
            if first.is_none() {
                first = Some(x.clone());
            }
            let achieved = apply(&self.basis, &self.chart.differences(&q, &q0));
            for ((r, &t), a) in residual.iter_mut().zip(target).zip(achieved) {
                *r = t - a;
            }
            let error = norm(&residual);
            if !error.is_finite() {
                break;
            }
            // The residual check detects corrections lost to rounding at a
            // large absolute Cartesian position.
            if rms < 1e-7 && error <= 1e-10 * (1. + initial_norm) {
                return Some(BackTransform {
                    positions: x,
                    converged: true,
                });
            }
            if error < best_norm {
                best_norm = error;
                best = Some(x.clone());
            }
            increases = if error > previous_norm {
                increases + 1
            } else {
                0
            };
            if increases == 2 {
                break;
            }
            // Allow a contracting chord iteration time to finish. Switch to
            // full Newton near the iteration limit to recover slow trials.
            refresh = error > 0.75 * previous_norm || iteration >= 15;
            previous_norm = error;
        }
        // Recover an iterate that realized at least 90% of the requested
        // displacement in residual norm. Otherwise use the first linear step.
        let positions = if best_norm <= 0.1 * initial_norm {
            best?
        } else {
            first?
        };
        Some(BackTransform {
            positions,
            converged: false,
        })
    }
}
