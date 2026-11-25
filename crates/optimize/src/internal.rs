//! Translation-rotation-internal coordinates for molecular RFO.
//!
//! Each covalent fragment contributes valence coordinates, a centroid and an
//! orientation relative to its reference geometry [Wang & Song, J. Chem.
//! Phys. **144**, 214108 (2016)]. The nonzero eigenvectors of the free-column
//! Wilson metric define the delocalized coordinates [Baker, Kessi & Delley,
//! J. Chem. Phys. **105**, 192 (1996)]. A basis remains fixed across accepted
//! steps until a rebuild. The Wilson matrix follows the instantaneous geometry.

mod back_transform;
mod chart;
mod fragment;
mod geometry;
mod metric;
mod primitive;
mod topology;

use std::borrow::Cow;

use crate::linalg::{dot, norm, symmetric_eigen};
use chart::Chart;
use metric::{Tangent, apply, congruence};
use topology::Topology;

pub(crate) struct InternalModel {
    numbers: Vec<u8>,
    free: Vec<usize>,
    hbonds: bool,
    chart: Chart,
    /// Row i is the primitive-space eigenvector defining DLC i.
    basis: Vec<f64>,
    hessian: Vec<f64>,
    /// The last accepted point with a nonsingular Wilson matrix in this chart.
    linearized_at: Vec<f64>,
    tangent: Tangent,
}

impl InternalModel {
    /// Positions are in bohr. Free indices are unique Cartesian components, in
    /// the order expected by the caller. An optional row-major Cartesian Hessian
    /// can be full (atom order) or restricted (free order). Its transformation
    /// (B_u⁺)ᵀ H_x B_u⁺ omits the gradient-weighted coordinate second derivatives
    /// (the K term), exact at a stationary point [Peng, Ayala, Schlegel &
    /// Frisch, J. Comput. Chem. **17**, 49 (1996)].
    pub(crate) fn build(
        numbers: &[u8],
        free: &[usize],
        positions: &[f64],
        hbonds: bool,
        cartesian_hessian: Option<&[f64]>,
    ) -> Result<Self, String> {
        if free.is_empty() {
            return Err("TRIC requires free Cartesian components".into());
        }
        let mut seen = vec![false; positions.len()];
        for &i in free {
            if i >= seen.len() || seen[i] {
                return Err("invalid or repeated free Cartesian index".into());
            }
            seen[i] = true;
        }
        let chart = Chart::new(numbers, positions, hbonds)?;
        let rows = chart
            .rows(positions, free)
            .ok_or("undefined primitive Wilson matrix")?;
        let (basis, tangent) = Tangent::delocalize(&rows, free.len())?;
        let hessian = if let Some(h) = cartesian_hessian {
            let restricted = restrict_hessian(h, positions.len(), free)?;
            congruence(&tangent.dual, &restricted, free.len())
                .ok_or("nonfinite transformed Cartesian Hessian")?
        } else {
            chart.lindh_hessian(&basis)
        };
        if !hessian.iter().all(|v| v.is_finite()) {
            return Err("nonfinite internal Hessian".into());
        }
        Ok(Self {
            numbers: numbers.to_vec(),
            free: free.to_vec(),
            hbonds,
            chart,
            basis,
            hessian,
            linearized_at: positions.to_vec(),
            tangent,
        })
    }

    fn dimension(&self) -> usize {
        self.tangent.b.len() / self.free.len()
    }

    fn tangent_at(&self, x: &[f64]) -> Option<Cow<'_, Tangent>> {
        if x == self.linearized_at {
            return Some(Cow::Borrowed(&self.tangent));
        }
        let rows = self.chart.rows(x, &self.free)?;
        Tangent::at(&rows, &self.basis, self.free.len()).map(Cow::Owned)
    }

    fn free_gradient(&self, g: &[f64]) -> Option<Vec<f64>> {
        let restricted: Vec<_> = if g.len() == self.linearized_at.len() {
            self.free.iter().map(|&i| g[i]).collect()
        } else if g.len() == self.free.len() {
            g.to_vec()
        } else {
            return None;
        };
        restricted
            .iter()
            .all(|x| x.is_finite())
            .then_some(restricted)
    }

    fn gradient(&self, positions: &[f64], g: &[f64]) -> Option<Vec<f64>> {
        Some(apply(
            &self.tangent_at(positions)?.dual,
            &self.free_gradient(g)?,
        ))
    }

    fn displacement(&self, old: &[f64], new: &[f64]) -> Option<Vec<f64>> {
        let delta = self
            .chart
            .differences(&self.chart.values(new)?, &self.chart.values(old)?);
        Some(apply(&self.basis, &delta))
    }

    /// Dense lowest augmented-Hessian eigenvector, following Banerjee et al.,
    /// J. Phys. Chem. **89**, 52 (1985). The caller supplies the projected
    /// gradient in full atom order or, when shorter, free-component order.
    pub(crate) fn direction<'a>(
        &'a self,
        positions: &'a [f64],
        cartesian_gradient: &[f64],
    ) -> Option<InternalDirection<'a>> {
        let g = self.gradient(positions, cartesian_gradient)?;
        if g.iter().all(|&x| x == 0.) {
            return None;
        }
        let n = g.len();
        let mut augmented = vec![0.; (n + 1) * (n + 1)];
        for (i, row) in self.hessian.chunks_exact(n).enumerate() {
            let start = i * (n + 1);
            augmented[start..start + n].copy_from_slice(row);
            augmented[start + n] = g[i];
            augmented[n * (n + 1) + i] = g[i];
        }
        let (_, vectors) = symmetric_eigen(augmented, n + 1).ok()?;
        // With an indefinite supplied Hessian, a negative mode perpendicular
        // to the gradient can give a lowest root without a finite RFO step.
        if vectors[n] == 0. {
            return None;
        }
        let step: Vec<_> = vectors[..n].iter().map(|v| v / vectors[n]).collect();
        let length = norm(&step);
        let slope = dot(&g, &step);
        if !length.is_finite() || !slope.is_finite() || slope >= 0. {
            return None;
        }
        Some(InternalDirection {
            model: self,
            origin: positions,
            gradient: g,
            step,
            length,
        })
    }

    /// BFGS at an accepted step. Both endpoint gradients are expressed in the
    /// same DLC chart using the reciprocal Wilson rows at their own geometry.
    pub(crate) fn update(
        &mut self,
        old_positions: &[f64],
        new_positions: &[f64],
        old_gradient: &[f64],
        new_gradient: &[f64],
    ) {
        let Some(next) = self.tangent_at(new_positions).map(Cow::into_owned) else {
            return;
        };
        if let (Some(old), Some(new), Some(s)) = (
            self.gradient(old_positions, old_gradient),
            self.free_gradient(new_gradient),
            self.displacement(old_positions, new_positions),
        ) {
            let mut y = apply(&next.dual, &new);
            for (v, previous) in y.iter_mut().zip(old) {
                *v -= previous;
            }
            self.update_curvature(&s, &y);
        }
        self.tangent = next;
        self.linearized_at.copy_from_slice(new_positions);
    }

    fn update_curvature(&mut self, s: &[f64], y: &[f64]) {
        let ys = dot(y, s);
        if !ys.is_finite() || ys <= 1e-12 * norm(s) * norm(y) {
            return;
        }
        let hs = apply(&self.hessian, s);
        let shs = dot(s, &hs);
        if !shs.is_finite() || shs <= 0. {
            return;
        }
        let n = s.len();
        let mut updated = self.hessian.clone();
        for i in 0..n {
            for j in 0..=i {
                let h = self.hessian[i * n + j] + y[i] * y[j] / ys - hs[i] * hs[j] / shs;
                if !h.is_finite() {
                    return;
                }
                updated[i * n + j] = h;
                updated[j * n + i] = h;
            }
        }
        self.hessian = updated;
    }

    /// Recenter after a change of primitives or fragment orientation chart, or
    /// when the instantaneous delocalized metric fails the relative rank test.
    pub(crate) fn needs_rebuild(&self, positions: &[f64]) -> bool {
        !self.chart.valid_positions(positions)
            || self
                .chart
                .fragments
                .iter()
                .any(|f| f.needs_rebuild(positions))
            || !Topology::generate(&self.numbers, positions, self.hbonds)
                .is_ok_and(|graph| self.chart.topology.same_chart(&graph))
            || self.tangent_at(positions).is_none()
    }

    /// Carry curvature through free Cartesians into a fresh DLC chart at x.
    /// If the old chart is already singular, its last usable tangent supplies
    /// the Cartesian curvature. A failed rebuild leaves the model usable.
    pub(crate) fn rebuild(&mut self, x: &[f64]) -> Result<(), String> {
        let mut next = Self::build(&self.numbers, &self.free, x, self.hbonds, None)?;
        let old = self.tangent_at(x).unwrap_or(Cow::Borrowed(&self.tangent));
        // A = (B_new⁺)ᵀ B_oldᵀ, hence A H_old Aᵀ is the Cartesian transfer
        // without materializing a second full Cartesian Hessian.
        let map: Vec<_> = next
            .tangent
            .dual
            .chunks_exact(self.free.len())
            .flat_map(|r| apply(&old.b, r))
            .collect();
        next.hessian = congruence(&map, &self.hessian, self.dimension())
            .ok_or("nonfinite curvature after TRIC rebuild")?;
        *self = next;
        Ok(())
    }
}

/// Select H_ff and average roundoff-sized asymmetries before the basis change.
fn restrict_hessian(h: &[f64], ncart: usize, free: &[usize]) -> Result<Vec<f64>, String> {
    let n = free.len();
    let full = ncart.checked_mul(ncart) == Some(h.len());
    if !full && n.checked_mul(n) != Some(h.len()) {
        return Err("Cartesian Hessian must be full or restricted to free components".into());
    }
    if !h.iter().all(|x| x.is_finite()) {
        return Err("nonfinite Cartesian Hessian".into());
    }
    let index = |i: usize, j: usize| {
        if full {
            free[i] * ncart + free[j]
        } else {
            i * n + j
        }
    };
    let scale = h.iter().map(|x| x.abs()).fold(0., f64::max);
    let mut result = vec![0.; n * n];
    for i in 0..n {
        for j in 0..=i {
            let (a, b) = (h[index(i, j)], h[index(j, i)]);
            if (a - b).abs() > 1e-12 * scale {
                return Err("asymmetric Cartesian Hessian".into());
            }
            let value = 0.5 * a + 0.5 * b;
            result[i * n + j] = value;
            result[j * n + i] = value;
        }
    }
    Ok(result)
}

pub(crate) struct InternalDirection<'a> {
    model: &'a InternalModel,
    origin: &'a [f64],
    gradient: Vec<f64>,
    step: Vec<f64>,
    length: f64,
}

impl InternalDirection<'_> {
    /// Restrict a direction from this accepted point to the internal trust
    /// radius. Return free Cartesian displacements, rational energy prediction
    /// and internal step length. A fallback iterate is assessed at its attained
    /// internal displacement and must still satisfy the trust radius.
    pub(crate) fn restricted(
        &self,
        positions: &[f64],
        radius: f64,
    ) -> Option<(Vec<f64>, f64, f64)> {
        if positions != self.origin || !radius.is_finite() || radius <= 0. {
            return None;
        }
        let mut length = radius.min(self.length);
        let mut step: Vec<_> = self
            .step
            .iter()
            .map(|x| x * (length / self.length))
            .collect();
        let trial = self.model.back_transform(positions, &step)?;
        if !trial.converged {
            step = self.model.displacement(positions, &trial.positions)?;
            length = norm(&step);
            if length > radius {
                return None;
            }
        }
        let quadratic = dot(&step, &apply(&self.model.hessian, &step));
        let prediction = (dot(&self.gradient, &step) + 0.5 * quadratic) / (1. + length * length);
        if !prediction.is_finite() || prediction >= 0. {
            return None;
        }
        let cartesian = self
            .model
            .free
            .iter()
            .map(|&i| trial.positions[i] - positions[i])
            .collect();
        Some((cartesian, prediction, length))
    }
}

#[cfg(test)]
mod tests;
