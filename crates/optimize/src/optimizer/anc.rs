//! Approximate normal coordinates, rational-function steps and Hessian updates.
//!
//! As in xtb's ANCopt (Grimme et al. 2017) \[Grimme2017ANCopt\]. Full references:
//! docs/references/xtb.md.

mod davidson;
mod diis;

pub(super) use diis::History;

#[cfg(test)]
use crate::linalg::symmetric_eigen;
use crate::linalg::{dot, norm, projected_eigen};
use crate::model_hessian::model_hessian;

use super::{Coordinates, Options, Point, TrialStep, Update};

#[cfg(test)]
const CURVATURE_FLOOR: f64 = 1e-3;

pub(super) struct Anc {
    // Mode i occupies row i, in the free Cartesian coordinate system.
    modes: Vec<f64>,
    hessian: Vec<f64>,
    nfree: usize,
    curvature_floor: f64,
    // The SR1/BFGS blend can lose positive definiteness, invalidating Davidson's unique
    // negative-root certificate. Keep the fast solve while it is guaranteed.
    positive_definite: bool,
    pub built_at: usize,
}

pub(super) struct RfoDirection {
    cartesian: Vec<f64>,
    length: f64,
    linear: f64,
    quadratic: f64,
}

impl RfoDirection {
    /// Scale the same eigenvector on all retries from this accepted point.
    pub fn restricted(&self, radius: f64) -> Option<TrialStep> {
        let scale = (radius / self.length).min(1.);
        let length = self.length.min(radius);
        let predicted = scale * (self.linear + scale * self.quadratic) / (1. + length * length);
        if !predicted.is_finite() || predicted >= 0. {
            return None;
        }
        let cart = self.cartesian.iter().map(|x| scale * x).collect();
        Some(TrialStep {
            cartesian: cart,
            predicted,
            length,
            converged_transform: true,
        })
    }
}

impl Anc {
    pub fn build(
        numbers: &[u8],
        coords: &Coordinates,
        point: &Point,
        built_at: usize,
        previous: Option<&Self>,
        exact: Option<Vec<f64>>,
        options: &Options,
    ) -> Result<Self, String> {
        let is_exact = exact.is_some();
        let full = match exact {
            Some(full) => full,
            None => model_hessian(numbers, &point.positions, options.contacts)?,
        };
        let nfree = coords.indices.len();
        let h = if nfree == point.positions.len() {
            full
        } else {
            let mut restricted = Vec::with_capacity(nfree * nfree);
            for &i in &coords.indices {
                let row = &full[i * point.positions.len()..];
                restricted.extend(coords.indices.iter().map(|&j| row[j]));
            }
            restricted
        };

        let (values, modes) = projected_eigen(h, nfree, &point.rigid)?;
        let dim = values.len();
        if dim == 0 {
            return Err("no internal search directions".into());
        }
        let mut hessian = vec![0.; dim * dim];
        for (i, &value) in values.iter().enumerate() {
            // Soft and disconnected-fragment directions still need a finite
            // restoring curvature for the initial RFO step.
            hessian[i * dim + i] =
                if is_exact { value.abs() } else { value }.max(options.curvature_floor);
        }
        let mut model = Self {
            modes,
            hessian,
            nfree,
            curvature_floor: options.curvature_floor,
            positive_definite: true,
            built_at,
        };
        if let Some(old) = previous.filter(|old| old.hessian.len() == model.hessian.len()) {
            model.carry_curvature(old)?;
        }
        Ok(model)
    }

    fn carry_curvature(&mut self, old: &Self) -> Result<(), String> {
        // T maps between ANC frames. An ambient floor in the old rigid
        // directions supplies curvature if the two tangent spaces have tilted:
        // H' = T H Tᵀ + floor (I − T Tᵀ).
        let mut overlap = Vec::with_capacity(old.hessian.len());
        for mode in self.modes.chunks_exact(self.nfree) {
            overlap.extend(old.to_anc(mode));
        }
        let dim = self.modes.len() / self.nfree;
        let mut row_product = vec![0.; dim];
        for (i, t) in overlap.chunks_exact(dim).enumerate() {
            for (j, row) in old.hessian.chunks_exact(dim).enumerate() {
                row_product[j] = dot(t, row) - self.curvature_floor * t[j];
            }
            for (j, u) in overlap.chunks_exact(dim).take(i + 1).enumerate() {
                let mut value = dot(&row_product, u);
                if i == j {
                    value += self.curvature_floor;
                }
                if !value.is_finite() {
                    return Err("non-finite curvature after ANC rebuild".into());
                }
                self.hessian[i * dim + j] = value;
                self.hessian[j * dim + i] = value;
            }
        }
        self.positive_definite = old.positive_definite;
        Ok(())
    }

    fn to_anc(&self, x: &[f64]) -> Vec<f64> {
        self.modes
            .chunks_exact(self.nfree)
            .map(|mode| dot(mode, x))
            .collect()
    }

    pub fn direction(&self, gradient: &[f64]) -> Option<RfoDirection> {
        let g = self.to_anc(gradient);
        let s = self.rfo_step(&g)?;
        self.model_direction(gradient, &g, &s)
    }

    /// Solve the augmented model for an ANC gradient.
    pub fn rfo_step(&self, g: &[f64]) -> Option<Vec<f64>> {
        if g.iter().all(|&v| v == 0.) {
            return None;
        }
        let matrix = davidson::AugmentedHessian {
            hessian: &self.hessian,
            gradient: g,
        };
        let v = if self.positive_definite {
            matrix.lowest_eigenvector(davidson::MAX_ITERATIONS)
        } else {
            matrix.dense_lowest_eigenvector()
        }?;
        // Banerjee et al., J. Phys. Chem. 89, 52 (1985) [Banerjee1985PRFO]. Scaling the last
        // coordinate to one makes the last eigen-equation g·s = λ < 0.
        super::rfo::eigenvector_step(&v, g)
    }

    /// Convert an ANC displacement using the current rational model.
    pub fn model_direction(&self, gradient: &[f64], g: &[f64], s: &[f64]) -> Option<RfoDirection> {
        let n = g.len();
        if s.len() != n {
            return None;
        }
        let length = norm(s);
        let linear = dot(g, s);
        if !length.is_finite() || !linear.is_finite() || linear >= 0. {
            return None;
        }
        let hs: Vec<_> = self
            .hessian
            .chunks_exact(n)
            .map(|row| dot(row, s))
            .collect();
        let quadratic = 0.5 * dot(s, &hs);
        let mut cart = vec![0.; self.nfree];
        for (mode, si) in self.modes.chunks_exact(self.nfree).zip(s) {
            for (x, vi) in cart.iter_mut().zip(mode) {
                *x += vi * si;
            }
        }
        // A change of basis can seed roundoff-sized displacements along a
        // symmetry axis with exactly zero force. Soft modes can amplify that
        // noise on later steps. Removing it here preserves the descent slope.
        for (x, g) in cart.iter_mut().zip(gradient) {
            if *g == 0. && x.abs() <= f64::EPSILON * length {
                *x = 0.;
            }
        }
        Some(RfoDirection {
            cartesian: cart,
            length,
            linear,
            quadratic,
        })
    }

    pub fn update(&mut self, displacement: &[f64], gradient_change: &[f64], update: Update) {
        let s = self.to_anc(displacement);
        let mut y = self.to_anc(gradient_change);
        super::rfo::update(
            &mut self.hessian,
            &mut self.positive_definite,
            &s,
            &mut y,
            update,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity_anc(hessian: Vec<f64>) -> Anc {
        let n = hessian.len().isqrt();
        let mut modes = vec![0.; n * n];
        for i in 0..n {
            modes[i * n + i] = 1.;
        }
        Anc {
            modes,
            hessian,
            nfree: n,
            curvature_floor: CURVATURE_FLOOR,
            positive_definite: true,
            built_at: 0,
        }
    }

    fn close(actual: &[f64], expected: &[f64]) {
        assert_eq!(actual.len(), expected.len());
        for (a, e) in actual.iter().zip(expected) {
            assert!((a - e).abs() < 2e-12, "{actual:?} != {expected:?}");
        }
    }

    fn diis_sample(position: [f64; 3], gradient: [f64; 3]) -> Point {
        Point {
            positions: [vec![0.; 3], position.to_vec()].concat(),
            energy: 0.5 * dot(&position, &gradient),
            gradient: [vec![0.; 3], gradient.to_vec()].concat(),
            active_gradient: gradient.to_vec(),
            rigid: Vec::new(),
        }
    }

    #[test]
    fn gdiis_requires_three_points_and_uses_the_current_hessian() {
        use crate::optimizer::Acceleration;
        let mut anc = identity_anc(vec![1., 0., 0., 0., 1., 0., 0., 0., 1.]);
        let coords = Coordinates::new(&[1, 1], &[0.; 6], &[0]).unwrap();
        let mut history = diis::History::default();
        for (position, gradient) in [
            ([0.001, 0., -0.001], [0.001, 0., 0.]),
            ([0., 0.001, -0.001], [0., 0.001, 0.]),
        ] {
            history.push(&anc, &diis_sample(position, gradient), &coords);
        }
        assert!(history.direction(&anc, Acceleration::Gdiis, 0.01).is_none());
        history.push(&anc, &diis_sample([0.; 3], [0., 0., 0.001]), &coords);
        let direction = history.direction(&anc, Acceleration::Gdiis, 0.01).unwrap();
        close(&direction.cartesian, &[0., 0., -0.001]);
        assert!(direction.restricted(0.01).unwrap().predicted < 0.);
        for i in 0..3 {
            anc.hessian[3 * i + i] = 2.;
        }
        let direction = history.direction(&anc, Acceleration::Gdiis, 0.01).unwrap();
        close(&direction.cartesian, &[1e-3 / 6., 1e-3 / 6., -5e-3 / 6.]);
        assert!(direction.restricted(0.01).unwrap().predicted < 0.);
        history.clear();
        assert!(history.direction(&anc, Acceleration::Gdiis, 0.01).is_none());
    }

    #[test]
    fn gdiis_rejects_interpolation_beyond_the_trust_radius() {
        use crate::optimizer::Acceleration;
        let anc = identity_anc(vec![1., 0., 0., 0., 1., 0., 0., 0., 1.]);
        let coords = Coordinates::new(&[1, 1], &[0.; 6], &[0]).unwrap();
        let mut history = diis::History::default();
        for (position, gradient) in [
            ([0., 0., -0.005], [0.001, 0., 0.]),
            ([0., 0., -0.004], [0., 0.001, 0.]),
            ([0.; 3], [0., 0., 0.001]),
        ] {
            history.push(&anc, &diis_sample(position, gradient), &coords);
        }
        assert!(
            history
                .direction(&anc, Acceleration::Gdiis, 0.005)
                .is_some()
        );
        assert!(
            history
                .direction(&anc, Acceleration::Gdiis, 0.001)
                .is_none()
        );
    }

    #[test]
    fn gdiis_subtracts_large_cartesian_offsets_before_projection() {
        use crate::optimizer::Acceleration;
        let anc = identity_anc(vec![1., 0., 0., 0., 1., 0., 0., 0., 1.]);
        let coords = Coordinates::new(&[1, 1], &[0.; 6], &[0]).unwrap();
        let displacement = 1. / 1024.;
        let origin = 2_f64.powi(40);
        let mut history = diis::History::default();
        for (position, gradient) in [
            ([displacement, 0., -displacement], [displacement, 0., 0.]),
            ([0., displacement, -displacement], [0., displacement, 0.]),
            ([0.; 3], [0., 0., displacement]),
        ] {
            let translated = position.map(|value| value + origin);
            history.push(&anc, &diis_sample(translated, gradient), &coords);
        }
        let direction = history.direction(&anc, Acceleration::Gdiis, 0.01).unwrap();
        close(&direction.cartesian, &[0., 0., -displacement]);
    }

    #[test]
    fn diis_coefficients_respect_the_affine_and_simplex_constraints() {
        let errors = [vec![1., 0., 0.], vec![0., 1., 0.], vec![0., 0., 1.]];
        close(
            &diis::residual_coefficients(&errors).unwrap(),
            &[1. / 3.; 3],
        );
        assert!(diis::residual_coefficients(&vec![vec![1.; 3]; 3]).is_none());
        assert!(
            diis::residual_coefficients(&[
                vec![1., 0., 0.],
                vec![1.01, 0.001, 0.],
                vec![1.02, 0., 0.001],
            ])
            .is_none()
        );
        // Equal-energy vertices of an equilateral triangle have an interior
        // minimum at equal weights in the GEDIIS first-order energy model.
        let positions = [vec![0., 0.], vec![1., 0.], vec![0.5, 3_f64.sqrt() / 2.]];
        let gradients: Vec<_> = positions.iter().map(Vec::as_slice).collect();
        close(
            &diis::energy_coefficients(&positions, &gradients, &[0.; 3]).unwrap(),
            &[1. / 3.; 3],
        );
        // A collinear history has a singular full face; its optimal edge is
        // still found, with a zero coefficient at the middle point.
        let positions = [vec![-1.], vec![0.], vec![1.]];
        let gradients: Vec<_> = positions.iter().map(Vec::as_slice).collect();
        close(
            &diis::energy_coefficients(&positions, &gradients, &[0.5, 0., 0.5]).unwrap(),
            &[0.5, 0., 0.5],
        );
        let gradients = [&[0.][..]; 3];
        close(
            &diis::energy_coefficients(&positions, &gradients, &[1., 0., 2.]).unwrap(),
            &[0., 1., 0.],
        );
    }

    #[test]
    fn gediis_corrects_its_interpolated_gradient_with_rfo() {
        use crate::optimizer::Acceleration;
        let anc = identity_anc(vec![0.01, 0., 0., 0., 0.01, 0., 0., 0., 0.01]);
        let coords = Coordinates::new(&[1, 1], &[0.; 6], &[0]).unwrap();
        let mut history = diis::History::default();
        for (position, gradient) in [
            ([0.01, 0., -0.01], [0.0001, 0., 0.]),
            ([0., 0.01, -0.01], [0., 0.0001, 0.]),
            ([0.; 3], [0., 0., 0.0001]),
        ] {
            let mut point = diis_sample(position, gradient);
            point.energy = 5e-7;
            history.push(&anc, &point, &coords);
        }
        let direction = history.direction(&anc, Acceleration::Gediis, 0.1).unwrap();
        let g: f64 = 1e-4 / 3.;
        let correction = -2. * g / (0.01 + (0.0001 + 12. * g * g).sqrt());
        close(
            &direction.cartesian,
            &[
                0.01 / 3. + correction,
                0.01 / 3. + correction,
                -0.02 / 3. + correction,
            ],
        );
        assert!(direction.restricted(0.1).unwrap().predicted < 0.);
    }

    #[test]
    fn gediis_keeps_descent_interpolation_when_the_weighted_gradient_vanishes() {
        use crate::optimizer::Acceleration;
        let anc = identity_anc(vec![0.01, 0., 0., 0., 0.01, 0., 0., 0., 0.01]);
        let coords = Coordinates::new(&[1, 1], &[0.; 6], &[0]).unwrap();
        let mut history = diis::History::default();
        for (x, gx) in [(-0.01, -1e-4), (0., 0.), (0.01, 1e-4)] {
            let point = diis_sample([x, 0., 0.], [gx, 0., 0.]);
            history.push(&anc, &point, &coords);
        }
        let direction = history.direction(&anc, Acceleration::Gediis, 0.1).unwrap();
        close(&direction.cartesian, &[-0.01, 0., 0.]);
        assert!(direction.restricted(0.1).unwrap().predicted < 0.);
    }

    fn cartesian_hessian(anc: &Anc) -> Vec<f64> {
        let n = anc.nfree;
        let dim = anc.hessian.len().isqrt();
        (0..n * n)
            .map(|index| {
                let (i, j) = (index / n, index % n);
                (0..dim)
                    .flat_map(|a| (0..dim).map(move |b| (a, b)))
                    .map(|(a, b)| {
                        anc.modes[a * n + i] * anc.hessian[a * dim + b] * anc.modes[b * n + j]
                    })
                    .sum()
            })
            .collect()
    }

    #[test]
    fn exact_seed_uses_the_free_block_and_absolute_floored_curvatures() {
        let z = [1, 1];
        let positions = vec![0., 0., 0., 1.4, 0., 0.];
        let coords = Coordinates::new(&z, &positions, &[0]).unwrap();
        let point = Point {
            rigid: coords.rigid(&positions),
            positions,
            energy: 0.,
            gradient: vec![0.; 6],
            active_gradient: vec![0.; 3],
        };
        let mut exact = vec![0.; 36];
        for (i, h) in [100., 200., 300., -4., 0.0002, 2.].into_iter().enumerate() {
            exact[6 * i + i] = h;
        }
        // Fixed/free couplings are excluded, rather than Schur-complemented.
        exact[3] = 10.;
        exact[18] = 10.;
        let anc = Anc::build(
            &z,
            &coords,
            &point,
            0,
            None,
            Some(exact),
            &Options::default(),
        )
        .unwrap();
        close(
            &cartesian_hessian(&anc),
            &[4., 0., 0., 0., CURVATURE_FLOOR, 0., 0., 0., 2.],
        );
        assert!(anc.positive_definite);
        assert_eq!(anc.built_at, 0);
    }

    #[test]
    fn exact_seed_projects_global_rigid_modes_before_taking_absolute_values() {
        let z = [1, 1];
        let positions = vec![0., 0., 0., 1.4, 0., 0.];
        let coords = Coordinates::new(&z, &positions, &[]).unwrap();
        let point = Point {
            rigid: coords.rigid(&positions),
            positions,
            energy: 0.,
            gradient: vec![0.; 6],
            active_gradient: vec![0.; 6],
        };
        let mut exact = vec![0.; 36];
        for (i, h) in [-4., 8., 9., -4., 10., 11.].into_iter().enumerate() {
            exact[6 * i + i] = h;
        }
        let anc = Anc::build(
            &z,
            &coords,
            &point,
            0,
            None,
            Some(exact),
            &Options::default(),
        )
        .unwrap();
        assert_eq!(anc.modes.len(), 6);
        close(&anc.hessian, &[4.]);
        assert!(point.rigid.iter().all(|u| dot(&anc.modes, u).abs() < 1e-12));
    }

    #[test]
    fn bfgs_retains_the_standard_secant_and_curvature_guards() {
        let mut anc = identity_anc(vec![2., 0., 0., 3.]);
        anc.update(&[1., 1.], &[4., 2.], Update::Bfgs);
        close(&anc.hessian, &[58. / 15., 2. / 15., 2. / 15., 28. / 15.]);
        close(
            &[
                anc.hessian[0] + anc.hessian[1],
                anc.hessian[2] + anc.hessian[3],
            ],
            &[4., 2.],
        );
        let before = anc.hessian.clone();
        anc.update(&[1., 0.], &[-1., 2.], Update::Bfgs);
        assert_eq!(anc.hessian, before);
    }

    #[test]
    fn damped_bfgs_modifies_negative_curvature_and_preserves_positive_definiteness() {
        let mut anc = identity_anc(vec![2., 0., 0., 3.]);
        anc.update(&[1., 0.], &[-1., 2.], Update::DampedBfgs);
        // theta = 8/15, r = (2/5, 16/15), so H_new s = r.
        close(&anc.hessian, &[2. / 5., 16. / 15., 16. / 15., 263. / 45.]);
        let (values, _) = symmetric_eigen(anc.hessian, 2).unwrap();
        assert!(values[0] > 0.);

        let mut bfgs = identity_anc(vec![2., 0., 0., 3.]);
        let mut damped = identity_anc(bfgs.hessian.clone());
        bfgs.update(&[1., 1.], &[4., 2.], Update::Bfgs);
        damped.update(&[1., 1.], &[4., 2.], Update::DampedBfgs);
        assert_eq!(bfgs.hessian, damped.hessian);
    }

    #[test]
    fn farkas_schlegel_uses_the_absolute_cosine_minimization_blend() {
        let mut anc = identity_anc(vec![2., 0., 0., 3.]);
        anc.update(&[1., 0.], &[3., 1.], Update::FarkasSchlegel);
        // r=(1,1), phi=1/sqrt(2). Both updates give first column (3,1),
        // while SR1 adds one to H_11 and BFGS adds one third.
        close(&anc.hessian, &[3., 1., 1., 10. / 3. + 2_f64.sqrt() / 3.]);
        assert!(anc.positive_definite);

        let mut negative = identity_anc(vec![2., 0., 0., 3.]);
        negative.update(&[1., 0.], &[-1., 1.], Update::FarkasSchlegel);
        close(&negative.hessian[..2], &[-1., 1.]);
        assert!(!negative.positive_definite);
        assert!(symmetric_eigen(negative.hessian.clone(), 2).unwrap().0[0] < 0.);
        let direction = negative.direction(&[0.1, -0.2]).unwrap();
        assert!(direction.restricted(0.3).unwrap().predicted < 0.);
    }

    #[test]
    fn farkas_schlegel_uses_the_defined_component_and_skips_zero_secants() {
        let mut anc = identity_anc(vec![2., 0., 0., 3.]);
        // r·s = 0 makes SR1 undefined; ordinary BFGS remains usable.
        anc.update(&[1., 0.], &[2., 1.], Update::FarkasSchlegel);
        close(&anc.hessian, &[2., 1., 1., 3.5]);
        let before = anc.hessian.clone();
        anc.update(&[0., 0.], &[1., 2.], Update::FarkasSchlegel);
        assert_eq!(anc.hessian, before);
        anc.update(&[1., 0.], &[2., 1.], Update::FarkasSchlegel);
        assert_eq!(anc.hessian, before);

        let mut sr1 = identity_anc(vec![2., 0., 0., 3.]);
        // y·s = 0 makes BFGS undefined; full SR1 still gives H_new s=y.
        sr1.update(&[1., 0.], &[0., 1.], Update::FarkasSchlegel);
        close(&sr1.hessian, &[0., 1., 1., 2.5]);
        assert!(!sr1.positive_definite);
    }

    #[test]
    fn indefinite_rfo_does_not_accept_a_root_that_misses_an_orthogonal_negative_mode() {
        // All diagonal entries are positive, but (1,−1,0) has curvature −2
        // and is orthogonal to g. A gradient-only Krylov solve misses it.
        let mut anc = identity_anc(vec![1., 3., 0., 3., 1., 0., 0., 0., 2.]);
        anc.positive_definite = false;
        assert!(anc.direction(&[0., 0., 0.1]).is_none());
    }

    #[test]
    fn only_global_rigid_modes_are_removed_from_disconnected_dimers() {
        let z = [1; 4];
        let positions = vec![0., 0., 0., 1.4, 0., 0., 15., 2., 1., 15., 3.4, 1.];
        let coords = Coordinates::new(&z, &positions, &[]).unwrap();
        let point = Point {
            rigid: coords.rigid(&positions),
            positions,
            energy: 0.,
            gradient: vec![0.; 12],
            active_gradient: vec![0.; 12],
        };
        let model = Anc::build(&z, &coords, &point, 0, None, None, &Options::default()).unwrap();
        assert_eq!(point.rigid.len(), 6);
        assert_eq!(model.modes.len(), 6 * 12);
        for (i, mode) in model.modes.as_chunks::<12>().0.iter().enumerate() {
            assert!(point.rigid.iter().all(|u| dot(mode, u).abs() < 1e-12));
            let curvature = model.hessian[i * 6 + i];
            if i < 4 {
                assert_eq!(curvature, CURVATURE_FLOOR);
            } else {
                let stretch = 0.9 * (1.35_f64.powi(2) - 1.4_f64.powi(2)).exp();
                assert!((curvature - stretch).abs() < 1e-12);
            }
        }
    }

    #[test]
    fn lowest_rfo_root_gives_descent_for_positive_and_negative_curvature() {
        // In the gradient mode, [h 1; 1 0] has the exact lowest roots below.
        // Rotating the ANC basis also checks the sign of the Cartesian step.
        let gradient = [0., 1.];
        for (curvature, root) in [(1.5, -0.5), (-1.5, -2.)] {
            let anc = Anc {
                modes: vec![0., 1., -1., 0.],
                hessian: vec![curvature, 0., 0., 2.],
                nfree: 2,
                curvature_floor: CURVATURE_FLOOR,
                positive_definite: curvature > 0.,
                built_at: 0,
            };
            let direction = anc.direction(&gradient).unwrap();
            let slope = dot(&gradient, &direction.cartesian);
            assert!(slope < 0.);
            assert!((slope - root).abs() < 1e-12);
            let TrialStep {
                cartesian: step,
                predicted,
                length,
                ..
            } = direction.restricted(0.1).unwrap();
            assert_eq!(length, 0.1);
            assert!(predicted < 0. && dot(&gradient, &step) < 0.);
        }
    }

    #[test]
    fn curvature_transport_restores_directions_outside_the_old_internal_space() {
        let old = Anc {
            modes: vec![1., 0., 0., 0., 1., 0.],
            hessian: vec![2., 1., 1., 4.],
            nfree: 3,
            curvature_floor: CURVATURE_FLOOR,
            positive_definite: true,
            built_at: 0,
        };
        let mut new = Anc {
            modes: vec![0., 1., 0., 0., 0., 1.],
            hessian: vec![0.; 4],
            nfree: 3,
            curvature_floor: CURVATURE_FLOOR,
            positive_definite: true,
            built_at: 30,
        };
        for floor in [CURVATURE_FLOOR, 0.007] {
            new.curvature_floor = floor;
            new.carry_curvature(&old).unwrap();
            assert_eq!(new.hessian, [4., 0., 0., floor]);
        }
    }
    #[test]
    fn configurable_floor_applies_to_model_and_potential_seeds() {
        let z = [18, 18];
        let positions = vec![0., 0., 0., 15., 0., 0.];
        let coords = Coordinates::new(&z, &positions, &[0]).unwrap();
        let point = Point {
            rigid: coords.rigid(&positions),
            positions,
            energy: 0.,
            gradient: vec![0.; 6],
            active_gradient: vec![0.; 3],
        };
        let options = Options {
            curvature_floor: 0.007,
            ..Options::default()
        };
        let model = Anc::build(&z, &coords, &point, 0, None, None, &options).unwrap();
        close(
            &cartesian_hessian(&model),
            &[0.007, 0., 0., 0., 0.007, 0., 0., 0., 0.007],
        );
        let mut exact = vec![0.; 36];
        exact[3 * 6 + 3] = -0.002;
        exact[4 * 6 + 4] = 0.004;
        exact[5 * 6 + 5] = -2.;
        let model = Anc::build(&z, &coords, &point, 0, None, Some(exact), &options).unwrap();
        close(
            &cartesian_hessian(&model),
            &[0.007, 0., 0., 0., 0.007, 0., 0., 0., 2.],
        );
    }

    #[test]
    fn contact_curvature_reaches_the_projected_anc_model() {
        let z = [18, 18];
        let positions = vec![0., 0., 0., 2. * 1.88 / crate::ANGSTROM_PER_BOHR, 0., 0.];
        let coords = Coordinates::new(&z, &positions, &[]).unwrap();
        let point = Point {
            rigid: coords.rigid(&positions),
            positions,
            energy: 0.,
            gradient: vec![0.; 6],
            active_gradient: vec![0.; 6],
        };
        let base = Anc::build(&z, &coords, &point, 0, None, None, &Options::default()).unwrap();
        let with = Anc::build(
            &z,
            &coords,
            &point,
            0,
            None,
            None,
            &Options {
                contacts: true,
                ..Options::default()
            },
        )
        .unwrap();
        close(&base.hessian, &[CURVATURE_FLOOR]);
        close(&with.hessian, &[0.002]);
    }
}
