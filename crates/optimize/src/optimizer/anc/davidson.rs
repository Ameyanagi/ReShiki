//! Lowest eigenvector of the symmetric RFO matrix [H g; gᵀ 0].

use crate::linalg::{dot, norm, symmetric_eigen};

const MAX_SUBSPACE: usize = 20;
pub(super) const MAX_ITERATIONS: usize = 100;

pub(super) struct AugmentedHessian<'a> {
    pub hessian: &'a [f64],
    pub gradient: &'a [f64],
}

impl AugmentedHessian<'_> {
    pub fn lowest_eigenvector(&self, max_iterations: usize) -> Option<Vec<f64>> {
        self.davidson(MAX_SUBSPACE, max_iterations)
            .or_else(|| self.dense_lowest_eigenvector())
    }

    fn product(&self, vector: &[f64]) -> Vec<f64> {
        let n = self.gradient.len();
        let v = &vector[..n];
        let mut product = Vec::with_capacity(vector.len());
        for (row, g) in self.hessian.chunks_exact(v.len()).zip(self.gradient) {
            product.push(dot(row, v) + g * vector[n]);
        }
        product.push(dot(self.gradient, v));
        product
    }

    // E. R. Davidson, J. Comput. Phys. 17, 87–94 (1975).
    // Store each trial vector and its matrix product. Only the small projected
    // matrix needs to be diagonalized as the subspace grows.
    fn davidson(&self, max_subspace: usize, max_iterations: usize) -> Option<Vec<f64>> {
        let n = self.gradient.len();
        let gradient_norm = norm(self.gradient);
        if gradient_norm == 0. || !gradient_norm.is_finite() || max_subspace < 3 {
            return None;
        }
        let diagonal: Vec<_> = (0..n).map(|i| self.hessian[i * n + i]).collect();
        // Always include the homogeneous axis. Its Ritz equation g·s = λ
        // then holds throughout, and corrections live in the physical block.
        let mut axis = vec![0.; n + 1];
        axis[n] = 1.;
        let mut axis_image = self.gradient.to_vec();
        axis_image.push(0.);
        let mut basis = vec![axis];
        let mut products = vec![axis_image];

        // Start with a scaled gradient to get a negative shift before CG.
        let mut trial: Vec<_> = self
            .gradient
            .iter()
            .zip(&diagonal)
            .map(|(g, d)| -g / d.abs().max(gradient_norm))
            .collect();
        let length = norm(&trial);
        trial.iter_mut().for_each(|v| *v /= length);
        trial.push(0.);

        let lowest_diagonal = diagonal.iter().copied().fold(0., f64::min);
        // The step solves (H − λI)s = −g. Scale the residual by its right-hand
        // side since |λ| becomes quadratic in the gradient near convergence.
        let tolerance = 1e-12 * gradient_norm;
        for _ in 1..max_iterations {
            products.push(self.product(&trial));
            basis.push(trial);
            let m = basis.len();
            let mut projected = vec![0.; m * m];
            for (i, v) in basis.iter().enumerate() {
                for (j, product) in products[..=i].iter().enumerate() {
                    let entry = dot(v, product);
                    projected[i * m + j] = entry;
                    projected[j * m + i] = entry;
                }
            }
            let (values, coefficients) = symmetric_eigen(projected, m).ok()?;
            let value = values[0];
            let ritz = combine(&basis, &coefficients[..m]);
            let image = combine(&products, &coefficients[..m]);
            let residual: Vec<_> = image
                .iter()
                .zip(&ritz)
                .map(|(av, v)| av - value * v)
                .collect();
            if norm(&residual) <= tolerance {
                // The model floor and accepted BFGS updates give H > 0, so
                // the negative root is unique. A Ritz value above any
                // diagonal entry cannot be the lowest eigenvalue.
                return (value < 0. && value <= lowest_diagonal + tolerance).then_some(ritz);
            }
            if m == n + 1 {
                return None;
            }
            if m == max_subspace {
                // Keep the homogeneous axis and the physical part of the Ritz vector.
                let length = norm(&ritz[..n]);
                let mut direction = ritz[..n].to_vec();
                direction.iter_mut().for_each(|x| *x /= length);
                direction.push(0.);
                basis.truncate(1);
                products.truncate(1);
                products.push(self.product(&direction));
                basis.push(direction);
            }

            let mut correction = self.correction(&residual[..n], &diagonal, value)?;
            correction.push(0.);
            let correction_norm = norm(&correction);
            // Two modified Gram-Schmidt passes keep the small projected
            // eigenproblem orthonormal even for nearly dependent corrections.
            for _ in 0..2 {
                for v in &basis {
                    let overlap = dot(&correction, v);
                    for (r, vi) in correction.iter_mut().zip(v) {
                        *r -= overlap * vi;
                    }
                }
            }
            let length = norm(&correction);
            if !length.is_finite() || length <= 32. * f64::EPSILON * correction_norm {
                return None;
            }
            correction.iter_mut().for_each(|v| *v /= length);
            trial = correction;
        }
        None
    }

    // BFGS and ANC frame changes couple the original diagonal modes. A few
    // diagonally preconditioned CG iterations approximate (H − λI)⁻¹r,
    // using those couplings without factoring H. Davidson only needs an
    // independent expansion direction, so the inner solve can be inexact.
    fn correction(&self, residual: &[f64], diagonal: &[f64], value: f64) -> Option<Vec<f64>> {
        let n = residual.len();
        let mut r = residual.to_vec();
        let mut direction: Vec<_> = r
            .iter()
            .zip(diagonal)
            .map(|(ri, d)| ri / (d - value))
            .collect();
        let mut rz = dot(&r, &direction);
        if !rz.is_finite() || rz <= 0. {
            return None;
        }
        let threshold = 1e-4 * rz; // Reduce the preconditioned residual norm by 100.
        let mut correction = vec![0.; n];
        let mut work = vec![0.; n];
        for _ in 0..8 {
            for ((wi, row), pi) in work
                .iter_mut()
                .zip(self.hessian.chunks_exact(n))
                .zip(&direction)
            {
                *wi = dot(row, &direction) - value * pi;
            }
            let curvature = dot(&direction, &work);
            if curvature <= 0. || !curvature.is_finite() {
                return None;
            }
            let alpha = rz / curvature;
            for i in 0..n {
                correction[i] += alpha * direction[i];
                r[i] -= alpha * work[i];
                work[i] = r[i] / (diagonal[i] - value);
            }
            let next_rz = dot(&r, &work);
            if !next_rz.is_finite() || next_rz < 0. {
                return None;
            }
            if next_rz <= threshold {
                break;
            }
            let beta = next_rz / rz;
            for (pi, zi) in direction.iter_mut().zip(&work) {
                *pi = zi + beta * *pi;
            }
            rz = next_rz;
        }
        Some(correction)
    }

    fn dense_lowest_eigenvector(&self) -> Option<Vec<f64>> {
        let n = self.gradient.len();
        let stride = n + 1;
        let mut matrix = vec![0.; stride * stride];
        for (i, row) in self.hessian.chunks_exact(n).enumerate() {
            matrix[i * stride..i * stride + n].copy_from_slice(row);
            matrix[i * stride + n] = self.gradient[i];
            matrix[n * stride + i] = self.gradient[i];
        }
        let (_, mut vectors) = symmetric_eigen(matrix, stride).ok()?;
        vectors.truncate(stride);
        Some(vectors)
    }
}

fn combine(basis: &[Vec<f64>], weights: &[f64]) -> Vec<f64> {
    let mut result = vec![0.; basis[0].len()];
    for (v, weight) in basis.iter().zip(weights) {
        for (r, vi) in result.iter_mut().zip(v) {
            *r += weight * vi;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restarted_davidson_recovers_lowest_dense_root() {
        let n = 12;
        let mut hessian = vec![0.; n * n];
        for i in 0..n {
            hessian[i * n + i] = 1. + 0.1 * i as f64;
            if i > 0 {
                hessian[i * n + i - 1] = 0.4;
                hessian[(i - 1) * n + i] = 0.4;
            }
        }
        let g: Vec<_> = (0..n).map(|i| 0.01 * (i as f64 + 1.).sin()).collect();
        let matrix = AugmentedHessian {
            hessian: &hessian,
            gradient: &g,
        };
        // A three-vector subspace forces restarts before reaching tolerance.
        assert!(matrix.davidson(3, 3).is_none());
        let v = matrix.davidson(3, MAX_ITERATIONS).unwrap();
        let av = matrix.product(&v);
        let value = dot(&v, &av);
        let residual: Vec<_> = av.iter().zip(&v).map(|(a, x)| a - value * x).collect();
        assert!(norm(&residual) <= 1.01e-8 * value.abs());
        let exact = matrix.dense_lowest_eigenvector().unwrap();
        assert!((value - dot(&exact, &matrix.product(&exact))).abs() < 1e-12);
    }

    #[test]
    fn iteration_limit_uses_the_dense_lowest_eigenvector() {
        let matrix = AugmentedHessian {
            hessian: &[1.5],
            gradient: &[1.],
        };
        assert!(matrix.davidson(MAX_SUBSPACE, 1).is_none());
        let v = matrix.lowest_eigenvector(1).unwrap();
        // [1.5 1; 1 0] has roots −0.5 and 2. The final row gives v₀/v₁ = λ.
        assert!((v[0] / v[1] + 0.5).abs() < 1e-14);
    }
}

#[cfg(test)]
mod regression;
