//! Accepted-point histories and coordinate transforms for shared DIIS proposals.

use std::collections::VecDeque;

use crate::linalg::norm;
use crate::optimizer::internal::{InternalDirection, InternalModel};
use crate::optimizer::{Acceleration, Coordinates, Point, rfo};

use super::{Anc, RfoDirection};

#[cfg(test)]
pub(super) use rfo::{energy_coefficients, residual_coefficients};

const HISTORY: usize = 5;

struct Sample {
    // Subtract actual positions before projection to avoid cancellation of a
    // large common offset. ANC stores free positions; TRIC stores full ones.
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

    fn push_sample(&mut self, position: Vec<f64>, gradient: Vec<f64>, energy: f64) {
        if self.samples.len() == HISTORY {
            self.samples.pop_front();
        }
        self.samples.push_back(Sample {
            position,
            gradient,
            energy,
        });
    }

    pub fn push(&mut self, anc: &Anc, point: &Point, coords: &Coordinates) {
        self.push_sample(
            coords.indices.iter().map(|&i| point.positions[i]).collect(),
            anc.to_anc(&point.active_gradient),
            point.energy,
        );
        self.gradient.clone_from(&point.active_gradient);
    }

    pub fn push_internal(
        &mut self,
        model: &InternalModel,
        positions: &[f64],
        gradient: &[f64],
        energy: f64,
    ) {
        let Some(internal) = model.gradient(positions, gradient) else {
            self.clear();
            return;
        };
        self.push_sample(positions.to_vec(), internal, energy);
        self.gradient = gradient.to_vec();
    }

    pub fn direction(
        &self,
        anc: &Anc,
        acceleration: Acceleration,
        radius: f64,
    ) -> Option<RfoDirection> {
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
        let gradients: Vec<_> = self.samples.iter().map(|s| s.gradient.as_slice()).collect();
        let energies: Vec<_> = self
            .samples
            .iter()
            .map(|s| s.energy - current.energy)
            .collect();
        let denominator = (anc.nfree as f64).sqrt();
        let rms_step = offsets
            .iter()
            .rev()
            .nth(1)
            .map_or(0., |s| norm(s) / denominator);
        rfo::diis_direction(
            &anc.hessian,
            anc.positive_definite,
            &offsets,
            &gradients,
            &energies,
            norm(&self.gradient) / denominator,
            rms_step,
            acceleration,
            radius,
            |g| anc.rfo_step(g),
            |step| anc.model_direction(&self.gradient, &current.gradient, &step),
        )
    }

    pub fn internal_direction<'a>(
        &self,
        model: &'a InternalModel,
        positions: &'a [f64],
        acceleration: Acceleration,
        radius: f64,
    ) -> Option<InternalDirection<'a>> {
        let current = self.samples.back()?;
        if current.position != positions {
            return None;
        }
        let offsets: Vec<_> = self
            .samples
            .iter()
            .map(|sample| model.displacement(positions, &sample.position))
            .collect::<Option<_>>()?;
        let gradients: Vec<_> = self.samples.iter().map(|s| s.gradient.as_slice()).collect();
        let energies: Vec<_> = self
            .samples
            .iter()
            .map(|s| s.energy - current.energy)
            .collect();
        let denominator = (self.gradient.len() as f64).sqrt();
        // The GEDIIS switching controls retain the optimizer's free Cartesian
        // RMS convention; only its interpolation and correction live in DLCs.
        let rms_step = self.samples.iter().rev().nth(1).map_or(0., |previous| {
            model.cartesian_distance(&previous.position, positions) / denominator
        });
        rfo::diis_direction(
            model.hessian(),
            model.positive_definite(),
            &offsets,
            &gradients,
            &energies,
            norm(&self.gradient) / denominator,
            rms_step,
            acceleration,
            radius,
            |g| rfo::step(model.hessian(), g, model.positive_definite()),
            |step| model.model_direction(positions, current.gradient.clone(), step),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linalg::dot;
    use crate::optimizer::Options;
    use crate::optimizer::internal::Curvature;

    fn held_water() -> (InternalModel, Vec<f64>, Vec<usize>) {
        let angle = 104.5_f64.to_radians();
        let positions = vec![
            0.,
            0.,
            0.,
            1.81,
            0.,
            0.,
            1.81 * angle.cos(),
            1.81 * angle.sin(),
            0.,
        ];
        let free: Vec<_> = (3..9).collect();
        let model = InternalModel::build(
            &[8, 1, 1],
            &free,
            &positions,
            false,
            Curvature::Initial(None, &Options::default()),
        )
        .unwrap();
        (model, positions, free)
    }

    // An independent central-difference chain rule converts a prescribed DLC
    // gradient to free Cartesians at each sample's own nonlinear geometry.
    fn cartesian_gradient(
        model: &InternalModel,
        positions: &[f64],
        free: &[usize],
        gradient: &[f64],
    ) -> Vec<f64> {
        let epsilon = 1e-5;
        free.iter()
            .map(|&i| {
                let mut plus = positions.to_vec();
                let mut minus = positions.to_vec();
                plus[i] += epsilon;
                minus[i] -= epsilon;
                dot(gradient, &model.displacement(&minus, &plus).unwrap()) / (2. * epsilon)
            })
            .collect()
    }

    fn attained(
        model: &InternalModel,
        positions: &[f64],
        free: &[usize],
        direction: &InternalDirection<'_>,
    ) -> Vec<f64> {
        let trial = direction.restricted(positions, 0.1).unwrap();
        assert!(trial.predicted < 0. && trial.converged_transform);
        let mut next = positions.to_vec();
        for (&i, value) in free.iter().zip(trial.cartesian) {
            next[i] += value;
        }
        assert_eq!(&next[..3], &positions[..3]);
        model.displacement(positions, &next).unwrap()
    }

    #[test]
    fn internal_gdiis_uses_chart_offsets_and_endpoint_wilson_gradients() {
        let (model, origin, free) = held_water();
        let n = free.len();
        let delta = 1e-3;
        let mut history = History::default();
        for sample in 0..3 {
            let mut offset = vec![0.; n];
            if sample < 2 {
                offset[sample] = delta;
                offset[2] = -delta;
            }
            let point = model.back_transform(&origin, &offset).unwrap();
            assert!(point.converged);
            // H^-1 g_i = delta e_i, so the equal residual weights give
            // (q_0+q_1+q_2)/3 - delta(e_0+e_1+e_2)/3 = -delta e_2.
            let gradient: Vec<_> = model
                .hessian()
                .chunks_exact(n)
                .map(|row| delta * row[sample])
                .collect();
            let cartesian = cartesian_gradient(&model, &point.positions, &free, &gradient);
            history.push_internal(&model, &point.positions, &cartesian, 0.);
        }
        let direction = history
            .internal_direction(&model, &origin, Acceleration::Gdiis, 0.01)
            .unwrap();
        let actual = attained(&model, &origin, &free, &direction);
        for (i, value) in actual.iter().enumerate() {
            assert!(
                (value - if i == 2 { -delta } else { 0. }).abs() < 1e-8,
                "{actual:?}"
            );
        }
        history.clear();
        assert!(
            history
                .internal_direction(&model, &origin, Acceleration::Gdiis, 0.01)
                .is_none()
        );
    }

    #[test]
    fn internal_gediis_retains_descent_when_the_interpolated_gradient_vanishes() {
        let (model, origin, free) = held_water();
        let mut history = History::default();
        let delta = 0.03;
        let mut current = Vec::new();
        for sign in [-1., 0., 1.] {
            let mut offset = vec![0.; free.len()];
            offset[0] = sign * delta;
            let point = model.back_transform(&origin, &offset).unwrap();
            assert!(point.converged);
            let mut gradient = vec![0.; free.len()];
            gradient[0] = sign * 1e-4;
            let cartesian = cartesian_gradient(&model, &point.positions, &free, &gradient);
            history.push_internal(
                &model,
                &point.positions,
                &cartesian,
                0.5 * delta * 1e-4 * sign * sign,
            );
            current = point.positions;
        }
        let direction = history
            .internal_direction(&model, &current, Acceleration::Gediis, 0.1)
            .unwrap();
        let actual = attained(&model, &current, &free, &direction);
        for (i, value) in actual.iter().enumerate() {
            assert!(
                (value - if i == 0 { -delta } else { 0. }).abs() < 1e-8,
                "{actual:?}"
            );
        }
    }
}
