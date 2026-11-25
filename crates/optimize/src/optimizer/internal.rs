//! Translation-rotation-internal coordinates for molecular RFO.
//!
//! Each covalent fragment contributes valence coordinates, a centroid and an
//! orientation relative to its reference geometry [Wang & Song, J. Chem.
//! Phys. **144**, 214108 (2016)]. An orthonormal basis for the range of the
//! free-column Wilson matrix defines delocalized coordinates [Baker, Kessi &
//! Delley, J. Chem. Phys. **105**, 192 (1996)]. A basis remains fixed across accepted
//! steps until a rebuild. The Wilson matrix follows the instantaneous geometry.

#[cfg(test)]
mod profile;

// Compile the timer and its clock reads only into the unit-test binary.
macro_rules! profile_phase {
    ($phase:literal) => {
        #[cfg(test)]
        let _phase = crate::optimizer::internal::profile::Span::start($phase);
    };
}

mod back_transform;
mod chart;
mod delocalize;
mod fragment;
mod geometry;
mod metric;
mod primitive;
mod product;
mod topology;

use std::borrow::Cow;

use super::{Options, TrialStep, Update, coordinates::rigid, rfo};
use crate::linalg::{dot, norm, projected_eigen};
use crate::model_hessian::contact_stretches;
use chart::Chart;
use metric::{Tangent, apply, congruence};

pub(crate) struct InternalModel {
    numbers: Vec<u8>,
    free: Vec<usize>,
    hbonds: bool,
    parameters: ModelParameters,
    chart: Chart,
    /// Each row defines one orthonormal primitive-space direction.
    basis: Vec<f64>,
    hessian: Vec<f64>,
    positive_definite: bool,
    /// The last accepted point with a nonsingular Wilson matrix in this chart.
    linearized_at: Vec<f64>,
    primitive_values: Vec<f64>,
    tangent: Tangent,
}

pub(crate) enum Curvature<'a> {
    Initial(Option<&'a [f64]>, &'a Options),
    Transport(&'a InternalModel),
}

#[derive(Clone, Copy)]
struct ModelParameters {
    contacts: bool,
    curvature_floor: f64,
}

impl InternalModel {
    /// Positions are in bohr. Free indices are unique Cartesian components, in
    /// the order expected by the caller. An optional row-major Cartesian Hessian
    /// can be full (atom order) or restricted (free order). Its physical free
    /// Cartesian eigenvalues are reflected and floored before transformation.
    /// Unanchored global rigid directions receive the same curvature floor.
    /// The transformation (B_u⁺)ᵀ H_x B_u⁺ omits the gradient-weighted coordinate
    /// second derivatives (the K term), and is exact at a stationary point
    /// [Peng, Ayala, Schlegel & Frisch, J. Comput. Chem. **17**, 49 (1996)].
    pub(crate) fn build(
        numbers: &[u8],
        free: &[usize],
        positions: &[f64],
        hbonds: bool,
        curvature: Curvature<'_>,
    ) -> Result<Self, String> {
        profile_phase!("model build");
        let parameters = match curvature {
            Curvature::Initial(_, options) => ModelParameters {
                contacts: options.contacts,
                curvature_floor: options.curvature_floor,
            },
            Curvature::Transport(old) => old.parameters,
        };
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
        let mut chart = Chart::new(numbers, positions, hbonds)?;
        let primitive_values = chart.values(positions).ok_or("undefined primitive value")?;
        let rows = chart
            .rows(positions, free)
            .ok_or("undefined primitive Wilson matrix")?;
        let (basis, tangent) = Tangent::delocalize(&rows, free.len())?;
        chart
            .set_basis(&basis, free.len())
            .ok_or("invalid DLC basis dimensions")?;
        let dual = tangent.dense_dual().expect("new DLC basis");
        let (hessian, positive_definite) = match curvature {
            Curvature::Transport(old) => (old.transport(positions, dual)?, old.positive_definite),
            Curvature::Initial(seed, _) => {
                let h = if let Some(h) = seed {
                    let restricted = restrict_hessian(h, positions.len(), free)?;
                    let seeded =
                        potential_seed(restricted, positions, free, parameters.curvature_floor)?;
                    congruence(dual, &seeded, free.len())
                        .ok_or("nonfinite transformed Cartesian Hessian")?
                } else {
                    let mut h = chart.lindh_hessian(&basis, parameters.curvature_floor);
                    if parameters.contacts {
                        for contact in contact_stretches(numbers, positions) {
                            let row = apply(dual, &contact.free_row(free));
                            for (i, &ri) in row.iter().enumerate() {
                                for (j, &rj) in row[..=i].iter().enumerate() {
                                    let k = h[i * row.len() + j] + contact.force * ri * rj;
                                    h[i * row.len() + j] = k;
                                    h[j * row.len() + i] = k;
                                }
                            }
                        }
                    }
                    h
                };
                if !h.iter().all(|v| v.is_finite()) {
                    return Err("nonfinite internal Hessian".into());
                }
                let pd = rfo::is_positive_definite(&h, free.len());
                (h, pd)
            }
        };
        Ok(Self {
            numbers: numbers.to_vec(),
            free: free.to_vec(),
            hbonds,
            parameters,
            chart,
            basis,
            hessian,
            positive_definite,
            linearized_at: positions.to_vec(),
            primitive_values,
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
        let b = self.chart.projected_rows(x, &self.free)?;
        Tangent::from_projected(b, self.free.len()).map(Cow::Owned)
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

    pub(crate) fn gradient(&self, positions: &[f64], g: &[f64]) -> Option<Vec<f64>> {
        self.tangent_at(positions)?
            .gradient(&self.free_gradient(g)?)
    }

    fn values_at(&self, x: &[f64]) -> Option<Cow<'_, [f64]>> {
        if x == self.linearized_at {
            Some(Cow::Borrowed(&self.primitive_values))
        } else {
            self.chart.values(x).map(Cow::Owned)
        }
    }

    pub(crate) fn displacement(&self, old: &[f64], new: &[f64]) -> Option<Vec<f64>> {
        let delta = self
            .chart
            .differences(&self.values_at(new)?, &self.values_at(old)?);
        Some(apply(&self.basis, &delta))
    }

    pub(crate) fn hessian(&self) -> &[f64] {
        &self.hessian
    }

    pub(crate) fn positive_definite(&self) -> bool {
        self.positive_definite
    }

    pub(crate) fn cartesian_distance(&self, old: &[f64], new: &[f64]) -> f64 {
        norm(
            &self
                .free
                .iter()
                .map(|&i| new[i] - old[i])
                .collect::<Vec<_>>(),
        )
    }

    /// Lowest augmented-Hessian eigenvector, following Banerjee et al.,
    /// J. Phys. Chem. **89**, 52 (1985). The caller supplies the projected
    /// gradient in full atom order or, when shorter, free-component order.
    pub(crate) fn direction<'a>(
        &'a self,
        positions: &'a [f64],
        cartesian_gradient: &[f64],
    ) -> Option<InternalDirection<'a>> {
        profile_phase!("RFO direction");
        let g = self.gradient(positions, cartesian_gradient)?;
        let step = rfo::step(&self.hessian, &g, self.positive_definite)?;
        self.model_direction(positions, g, step)
    }

    pub(crate) fn model_direction<'a>(
        &'a self,
        positions: &'a [f64],
        gradient: Vec<f64>,
        step: Vec<f64>,
    ) -> Option<InternalDirection<'a>> {
        self.values_at(positions)?;
        let length = norm(&step);
        let slope = dot(&gradient, &step);
        if !length.is_finite() || !slope.is_finite() || slope >= 0. {
            return None;
        }
        Some(InternalDirection {
            model: self,
            origin: positions,
            gradient,
            step,
            length,
        })
    }

    /// Selected secant update at an accepted step. Endpoint gradients use the
    /// same DLC chart using the reciprocal Wilson rows at their own geometry.
    pub(crate) fn update(
        &mut self,
        old_positions: &[f64],
        new_positions: &[f64],
        old_gradient: &[f64],
        new_gradient: &[f64],
        update: Update,
    ) {
        // The next search rebuilds at the accepted point. Do not form a
        // secant across an expired principal-log chart in the meantime.
        let Some(new_values) = self.chart.values(new_positions) else {
            return;
        };
        let Some(old_values) = self.values_at(old_positions) else {
            return;
        };
        let s = apply(
            &self.basis,
            &self.chart.differences(&new_values, &old_values),
        );
        let Some(next) = self.tangent_at(new_positions).map(Cow::into_owned) else {
            return;
        };
        if let (Some(old), Some(mut y)) = (
            self.gradient(old_positions, old_gradient),
            self.free_gradient(new_gradient)
                .and_then(|g| next.gradient(&g)),
        ) {
            for (v, previous) in y.iter_mut().zip(old) {
                *v -= previous;
            }
            rfo::update(
                &mut self.hessian,
                &mut self.positive_definite,
                &s,
                &mut y,
                update,
            );
        }
        self.tangent = next;
        self.primitive_values = new_values;
        self.linearized_at.copy_from_slice(new_positions);
    }

    /// Recenter after a change of primitives or fragment orientation chart, or
    /// when the instantaneous delocalized metric fails the relative rank test.
    pub(crate) fn needs_rebuild(&self, positions: &[f64]) -> bool {
        profile_phase!("chart check");
        !self.chart.valid_positions(positions)
            || self
                .chart
                .fragments
                .iter()
                .any(|f| f.needs_rebuild(positions))
            || !self
                .chart
                .topology
                .same_chart_at(&self.numbers, positions, self.hbonds)
            || self.tangent_at(positions).is_none()
    }

    /// Carry curvature through free Cartesians into a fresh DLC chart at x.
    /// If the old chart is already singular, its last usable tangent supplies
    /// the Cartesian curvature. A failed rebuild leaves the model usable.
    pub(crate) fn rebuild(&mut self, x: &[f64]) -> Result<(), String> {
        profile_phase!("rebuild");
        *self = Self::build(
            &self.numbers,
            &self.free,
            x,
            self.hbonds,
            Curvature::Transport(self),
        )?;
        Ok(())
    }

    fn transport(&self, x: &[f64], new_dual: &[f64]) -> Result<Vec<f64>, String> {
        profile_phase!("curvature transport");
        let old = self.tangent_at(x).unwrap_or(Cow::Borrowed(&self.tangent));
        // A = (B_new⁺)ᵀ B_oldᵀ, hence A H_old Aᵀ is the Cartesian transfer.
        let map = product::row_products(new_dual, &old.b, self.free.len(), false);
        congruence(&map, &self.hessian, self.dimension())
            .ok_or_else(|| "nonfinite curvature after TRIC rebuild".to_owned())
    }
}

/// Reflect/floor the physical free-Cartesian block before the nonorthogonal
/// DLC change of basis. Only global rigid motion is removed, and any fixed
/// component anchors the system. Relative fragment motion remains physical.
fn potential_seed(
    restricted: Vec<f64>,
    positions: &[f64],
    free: &[usize],
    floor: f64,
) -> Result<Vec<f64>, String> {
    let n = free.len();
    let axes: Vec<_> = if n == positions.len() {
        rigid(positions)
            .into_iter()
            .map(|axis| free.iter().map(|&i| axis[i]).collect())
            .collect()
    } else {
        Vec::new()
    };
    let (values, modes) = projected_eigen(restricted, n, &axes)?;
    // floor I gives each removed global rigid direction curvature_floor
    // hartree/bohr². Physical eigenmodes replace that prior with max(|λ|, floor).
    let mut seed = vec![0.; n * n];
    for i in 0..n {
        seed[i * n + i] = floor;
    }
    for (&value, mode) in values.iter().zip(modes.chunks_exact(n)) {
        let extra = value.abs().max(floor) - floor;
        for (i, row) in seed.chunks_exact_mut(n).enumerate() {
            let weight = extra * mode[i];
            for (entry, &v) in row[..=i].iter_mut().zip(mode) {
                *entry += weight * v;
            }
        }
    }
    for i in 0..n {
        for j in 0..i {
            seed[j * n + i] = seed[i * n + j];
        }
    }
    Ok(seed)
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
    pub(crate) fn restricted(&self, positions: &[f64], radius: f64) -> Option<TrialStep> {
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
        let prediction = rfo::prediction(&self.model.hessian, &self.gradient, &step)?;
        let cartesian = self
            .model
            .free
            .iter()
            .map(|&i| trial.positions[i] - positions[i])
            .collect();
        Some(TrialStep {
            cartesian,
            predicted: prediction,
            length,
            converged_transform: trial.converged,
        })
    }
}

#[cfg(test)]
mod tests;
