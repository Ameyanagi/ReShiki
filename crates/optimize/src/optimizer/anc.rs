//! Approximate normal coordinates, rational-function steps and direct BFGS updates.

mod davidson;

use crate::linalg::{dot, norm, symmetric_eigen};
use crate::model_hessian::model_hessian;

use super::{Coordinates, Point};

const CURVATURE_FLOOR: f64 = 1e-3;

pub(super) struct Anc {
    // Mode i occupies row i, in the free Cartesian coordinate system.
    modes: Vec<f64>,
    hessian: Vec<f64>,
    nfree: usize,
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
    pub fn restricted(&self, radius: f64) -> Option<(Vec<f64>, f64, bool)> {
        let scale = (radius / self.length).min(1.);
        let length = self.length.min(radius);
        let predicted = scale * (self.linear + scale * self.quadratic) / (1. + length * length);
        if !predicted.is_finite() || predicted >= 0. {
            return None;
        }
        let cart = self.cartesian.iter().map(|x| scale * x).collect();
        Some((cart, predicted, self.length >= radius))
    }
}

fn reflect(v: &mut [f64], u: &[f64]) {
    let alpha = 2. * dot(v, u);
    for (x, &ui) in v.iter_mut().zip(u) {
        *x -= alpha * ui;
    }
}

impl Anc {
    pub fn build(
        numbers: &[u8],
        coords: &Coordinates,
        point: &Point,
        built_at: usize,
        previous: Option<&Self>,
    ) -> Result<Self, String> {
        let full = model_hessian(numbers, &point.positions)?;
        let nfree = coords.indices.len();
        let mut h = if nfree == point.positions.len() {
            full
        } else {
            let mut restricted = Vec::with_capacity(nfree * nfree);
            for &i in &coords.indices {
                let row = &full[i * point.positions.len()..];
                restricted.extend(coords.indices.iter().map(|&j| row[j]));
            }
            restricted
        };

        // Q is a product of reflectors taking the orthonormal rigid vectors
        // onto the first coordinate axes. The trailing block of Q^T H Q is
        // exactly P H P restricted to range(P), even when H has extra nulls.
        let mut axes = point.rigid.clone();
        let mut reflectors = Vec::with_capacity(axes.len());
        for k in 0..axes.len() {
            let mut u = axes[k][k..].to_vec();
            u[0] += norm(&u).copysign(u[0]);
            let length = norm(&u);
            if !(length.is_finite() && length > 0.) {
                return Err("singular rigid-body basis".into());
            }
            u.iter_mut().for_each(|x| *x /= length);
            for axis in &mut axes[k + 1..] {
                reflect(&mut axis[k..], &u);
            }
            let mut w: Vec<_> = h
                .chunks_exact(nfree)
                .skip(k)
                .map(|row| dot(&row[k..], &u))
                .collect();
            let uhu = dot(&u, &w);
            for (wi, ui) in w.iter_mut().zip(&u) {
                *wi = 2. * (*wi - uhu * ui);
            }
            for (i, row) in h.chunks_exact_mut(nfree).skip(k).enumerate() {
                for (j, x) in row[k..].iter_mut().enumerate() {
                    *x -= u[i] * w[j] + w[i] * u[j];
                }
            }
            reflectors.push(u);
        }
        let rank = reflectors.len();
        let dim = nfree - rank;
        if dim == 0 {
            return Err("no internal search directions".into());
        }
        let block = if rank == 0 {
            h
        } else {
            h.chunks_exact(nfree)
                .skip(rank)
                .flat_map(|r| r[rank..].iter().copied())
                .collect()
        };
        let (values, vectors) = symmetric_eigen(block, dim)?;
        let mut modes = vec![0.; dim * nfree];
        let mut hessian = vec![0.; dim * dim];
        for (i, mode) in modes.chunks_exact_mut(nfree).enumerate() {
            mode[rank..].copy_from_slice(&vectors[i * dim..(i + 1) * dim]);
            for (k, u) in reflectors.iter().enumerate().rev() {
                reflect(&mut mode[k..], u);
            }
            // Soft and disconnected-fragment directions still need a finite
            // restoring curvature for the initial RFO step.
            hessian[i * dim + i] = values[i].max(CURVATURE_FLOOR);
        }
        let mut model = Self {
            modes,
            hessian,
            nfree,
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
                row_product[j] = dot(t, row) - CURVATURE_FLOOR * t[j];
            }
            for (j, u) in overlap.chunks_exact(dim).take(i + 1).enumerate() {
                let mut value = dot(&row_product, u);
                if i == j {
                    value += CURVATURE_FLOOR;
                }
                if !value.is_finite() {
                    return Err("non-finite curvature after ANC rebuild".into());
                }
                self.hessian[i * dim + j] = value;
                self.hessian[j * dim + i] = value;
            }
        }
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
        let n = g.len();
        if g.iter().all(|&v| v == 0.) {
            return None;
        }
        let matrix = davidson::AugmentedHessian {
            hessian: &self.hessian,
            gradient: &g,
        };
        let v = matrix.lowest_eigenvector(davidson::MAX_ITERATIONS)?;
        // Banerjee et al., J. Phys. Chem. 89, 52 (1985). Scaling the last
        // coordinate to one makes the last eigen-equation g·s = λ < 0.
        let s: Vec<_> = v[..n].iter().map(|x| x / v[n]).collect();
        let length = norm(&s);
        let linear = dot(&g, &s);
        if !length.is_finite() || !linear.is_finite() || linear >= 0. {
            return None;
        }
        let hs: Vec<_> = self
            .hessian
            .chunks_exact(n)
            .map(|row| dot(row, &s))
            .collect();
        let quadratic = 0.5 * dot(&s, &hs);
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

    pub fn update(&mut self, displacement: &[f64], gradient_change: &[f64]) {
        let s = self.to_anc(displacement);
        let y = self.to_anc(gradient_change);
        let n = s.len();
        let ys = dot(&y, &s);
        if !ys.is_finite() || ys <= 1e-12 * norm(&y) * norm(&s) {
            return;
        }
        let hs: Vec<_> = self
            .hessian
            .chunks_exact(n)
            .map(|row| dot(row, &s))
            .collect();
        let shs = dot(&s, &hs);
        if !shs.is_finite() || shs <= 0. {
            return;
        }
        for (i, row) in self.hessian.chunks_exact_mut(n).enumerate() {
            for (j, x) in row.iter_mut().enumerate() {
                *x += y[i] * y[j] / ys - hs[i] * hs[j] / shs;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let model = Anc::build(&z, &coords, &point, 0, None).unwrap();
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
                built_at: 0,
            };
            let direction = anc.direction(&gradient).unwrap();
            let slope = dot(&gradient, &direction.cartesian);
            assert!(slope < 0.);
            assert!((slope - root).abs() < 1e-12);
            let (step, predicted, boundary) = direction.restricted(0.1).unwrap();
            assert!(boundary && predicted < 0. && dot(&gradient, &step) < 0.);
        }
    }

    #[test]
    fn curvature_transport_restores_directions_outside_the_old_internal_space() {
        let old = Anc {
            modes: vec![1., 0., 0., 0., 1., 0.],
            hessian: vec![2., 1., 1., 4.],
            nfree: 3,
            built_at: 0,
        };
        let mut new = Anc {
            modes: vec![0., 1., 0., 0., 0., 1.],
            hessian: vec![0.; 4],
            nfree: 3,
            built_at: 30,
        };
        new.carry_curvature(&old).unwrap();
        assert_eq!(new.hessian, [4., 0., 0., CURVATURE_FLOOR]);
    }
}
