//! Iterative back-transformation after Pulay & Fogarasi, J. Chem. Phys.
//! **96**, 2856 (1992), and Peng et al., J. Comput. Chem. **17**, 49 (1996).

use super::{
    InternalModel,
    metric::{apply, transpose_apply},
};
use crate::linalg::norm;

pub(crate) struct BackTransform {
    pub(crate) positions: Vec<f64>,
    pub(crate) converged: bool,
}

impl InternalModel {
    /// Solve Uᵀ(q(x) − q(x₀)) = target in the local chart. Wrap each proper
    /// torsion before applying Uᵀ. Newton corrections use the current B_u⁺
    /// and are scattered only into free Cartesian components.
    pub(crate) fn back_transform(&self, origin: &[f64], target: &[f64]) -> Option<BackTransform> {
        if target.len() != self.dimension() {
            return None;
        }
        let initial_norm = norm(target);
        if !initial_norm.is_finite() {
            return None;
        }
        let q0 = self.chart.values(origin)?;
        let mut x = origin.to_vec();
        let mut residual = target.to_vec();
        let mut previous_norm = initial_norm;
        let mut increases = 0;
        let mut first = None;
        let mut best = None;
        let mut best_norm = f64::INFINITY;
        for _ in 0..25 {
            let Some(tangent) = self.tangent_at(&x) else {
                break;
            };
            let dx = transpose_apply(&tangent.dual, self.free.len(), &residual);
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
