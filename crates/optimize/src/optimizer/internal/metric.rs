//! Dense Wilson matrices. Eigenvectors and Cartesian derivatives occupy rows.

use crate::linalg::cholesky::Cholesky;
use crate::linalg::{norm, symmetric_eigen};

/// Retain λ > 10⁻¹⁰ λmax in the Wilson metric (singular values above 10⁻⁵ σmax).
/// Use the same cutoff when inverting a trial-geometry metric in the frozen DLC basis.
const RELATIVE_CUTOFF: f64 = 1e-10;

pub(super) fn apply(rows: &[f64], x: &[f64]) -> Vec<f64> {
    crate::linalg::row_dots(rows, x)
}

/// Weighted sum of matrix rows, i.e. Mᵀ w.
pub(super) fn transpose_apply(rows: &[f64], width: usize, weights: &[f64]) -> Vec<f64> {
    let mut result = vec![0.; width];
    for (&weight, row) in weights.iter().zip(rows.chunks_exact(width)) {
        for (entry, x) in result.iter_mut().zip(row) {
            *entry += weight * x;
        }
    }
    result
}

fn row_gram(rows: &[f64], width: usize) -> Vec<f64> {
    super::product::row_products(rows, rows, width, true)
}

/// A H Aᵀ for a symmetric H and rectangular A with `width` columns.
pub(super) fn congruence(a: &[f64], h: &[f64], width: usize) -> Option<Vec<f64>> {
    let product = super::product::row_products(a, h, width, false);
    let result = super::product::row_products(&product, a, width, true);
    result.iter().all(|v| v.is_finite()).then_some(result)
}

/// B_u and its reciprocal at one geometry, with free Cartesian columns only.
#[derive(Clone)]
pub(super) struct Tangent {
    pub(super) b: Vec<f64>,
    reciprocal: Reciprocal,
    dimension: usize,
}

#[derive(Clone)]
enum Reciprocal {
    /// Already available from the spectral construction at chart creation.
    Dense(Vec<f64>),
    /// Reuse the rank certificate's factor for residual-refined solves.
    Metric {
        factor: Cholesky,
        b_norm: f64,
        shift: f64,
    },
}

impl Tangent {
    /// Baker, Kessi & Delley, J. Chem. Phys. 105, 192 (1996) \[Baker1996DLC\].
    /// Diagonalizing BᵀB gives the nonzero eigenpairs of G = BBᵀ.
    pub(super) fn delocalize(rows: &[f64], nfree: usize) -> Result<(Vec<f64>, Self), String> {
        profile_phase!("delocalization");
        let sparse = super::delocalize::WilsonRows::new(rows, nfree);
        let gram = sparse.gram(nfree);
        if nfree >= 32
            && let Some(build) = sparse.cholesky_basis(&gram, nfree)
        {
            return Ok((
                build.modes,
                Self {
                    b: build.b,
                    reciprocal: Reciprocal::Dense(build.dual),
                    dimension: nfree,
                },
            ));
        }
        let (values, vectors) = symmetric_eigen(gram, nfree)?;
        let cutoff = values[nfree - 1] * RELATIVE_CUTOFF;
        let rank = values.iter().filter(|&&v| v > cutoff).count();
        if rank != nfree {
            return Err(format!(
                "TRIC primitives span {rank} of {nfree} free Cartesian directions"
            ));
        }
        let mut basis = Vec::with_capacity(rows.len());
        let mut b = Vec::with_capacity(nfree * nfree);
        let mut dual = Vec::with_capacity(nfree * nfree);
        for (&lambda, v) in values.iter().zip(vectors.chunks_exact(nfree)) {
            let sigma = lambda.sqrt();
            sparse.project_mode(v, sigma, &mut basis);
            b.extend(v.iter().map(|x| x * sigma));
            dual.extend(v.iter().map(|x| x / sigma));
        }
        Ok((
            basis,
            Self {
                b,
                reciprocal: Reciprocal::Dense(dual),
                dimension: nfree,
            },
        ))
    }

    /// The metric factor also certifies rank. Residual refinement in B or Bᵀ
    /// corrects the forward error of the normal-equation solve.
    pub(super) fn from_projected(b: Vec<f64>, nfree: usize) -> Option<Self> {
        profile_phase!("tangent metric");
        if nfree == 0 || b.len() != nfree.checked_mul(nfree)? || b.iter().any(|x| !x.is_finite()) {
            return None;
        }
        let gram = {
            profile_phase!("tangent Gram");
            row_gram(&b, nfree)
        };
        if gram.iter().any(|x| !x.is_finite()) {
            return None;
        }
        let upper = gram
            .chunks_exact(nfree)
            .map(|row| row.iter().map(|x| x.abs()).sum::<f64>())
            .fold(0., f64::max);
        let trace = (0..nfree).map(|i| gram[i * nfree + i]).sum::<f64>();
        // Both Gram construction and Cholesky have O(n ε) backward error.
        // The trace bounds the Frobenius-square terms in those estimates.
        let rounding = 16. * nfree as f64 * f64::EPSILON * trace;
        let mut shift = RELATIVE_CUTOFF * upper + rounding;
        let mut factor = {
            profile_phase!("tangent Cholesky");
            Cholesky::new(&gram, nfree, -shift, 0.)
        };
        if factor.is_none() {
            profile_phase!("tangent rank spectral");
            let (values, vectors) = symmetric_eigen(gram.clone(), nfree).ok()?;
            if values[0] <= RELATIVE_CUTOFF * values[nfree - 1] {
                return None;
            }
            factor = Cholesky::new(&gram, nfree, 0., 0.);
            shift = 0.;
            if factor.is_none() {
                let dual = spectral_dual(&b, nfree, &values, &vectors);
                if dual.iter().any(|x| !x.is_finite()) {
                    return None;
                }
                return Some(Self {
                    b,
                    reciprocal: Reciprocal::Dense(dual),
                    dimension: nfree,
                });
            }
        }
        let b_norm = trace.sqrt();
        Some(Self {
            b,
            reciprocal: Reciprocal::Metric {
                factor: factor?,
                b_norm,
                shift,
            },
            dimension: nfree,
        })
    }

    /// The chart build already supplies a dense reciprocal for Hessian maps.
    pub(super) fn dense_dual(&self) -> Option<&[f64]> {
        match &self.reciprocal {
            Reciprocal::Dense(dual) => Some(dual),
            Reciprocal::Metric { .. } => None,
        }
    }

    /// (B⁺)ᵀ g = B⁻ᵀ g for this full-rank, square delocalized matrix.
    pub(super) fn gradient(&self, cartesian: &[f64]) -> Option<Vec<f64>> {
        if cartesian.len() != self.dimension {
            return None;
        }
        match &self.reciprocal {
            Reciprocal::Dense(dual) => Some(apply(dual, cartesian)),
            Reciprocal::Metric {
                factor,
                b_norm,
                shift,
            } => self.refined_solve(factor, *b_norm, cartesian, true, *shift),
        }
    }

    /// B⁺ dq = B⁻¹ dq, without materializing an instantaneous inverse.
    pub(super) fn cartesian_step(&self, internal: &[f64]) -> Option<Vec<f64>> {
        if internal.len() != self.dimension {
            return None;
        }
        match &self.reciprocal {
            Reciprocal::Dense(dual) => Some(transpose_apply(dual, internal.len(), internal)),
            Reciprocal::Metric {
                factor,
                b_norm,
                shift,
            } => self.refined_solve(factor, *b_norm, internal, false, *shift),
        }
    }

    /// One inexact chord correction, guarded by a relative linear error bound.
    /// The nonlinear back-transform independently checks its own residual.
    pub(super) fn cartesian_correction(&self, internal: &[f64]) -> Option<Vec<f64>> {
        if internal.len() != self.dimension {
            return None;
        }
        match &self.reciprocal {
            Reciprocal::Dense(dual) => Some(transpose_apply(dual, self.dimension, internal)),
            Reciprocal::Metric {
                factor,
                b_norm,
                shift,
            } => {
                let mut y = internal.to_vec();
                factor.solve_in_place(&mut y)?;
                let dx = transpose_apply(&self.b, self.dimension, &y);
                // G > sI gives ||dx - B⁻¹r||² = s² yᵀG⁻¹y ≤ s ||y||².
                // Avoid an oversized first chord step close to the rank cutoff.
                if shift.sqrt() * norm(&y) > 0.01 * norm(&dx) {
                    self.refined_solve(factor, *b_norm, internal, false, *shift)
                } else {
                    Some(dx)
                }
            }
        }
    }

    fn reciprocal_apply(
        &self,
        factor: &Cholesky,
        rhs: &[f64],
        transposed: bool,
    ) -> Option<Vec<f64>> {
        let mut product = if transposed {
            apply(&self.b, rhs)
        } else {
            rhs.to_vec()
        };
        factor.solve_in_place(&mut product)?;
        if transposed {
            Some(product)
        } else {
            Some(transpose_apply(&self.b, self.dimension, &product))
        }
    }

    fn refined_solve(
        &self,
        factor: &Cholesky,
        b_norm: f64,
        rhs: &[f64],
        transposed: bool,
        shift: f64,
    ) -> Option<Vec<f64>> {
        self.refine(factor, b_norm, rhs, transposed).or_else(|| {
            // Close to the rank cutoff a negative shift can make the
            // stationary refinement noncontracting. Revert to the unshifted
            // metric; its full-rank decision was already established above.
            if shift == 0. {
                return None;
            }
            let gram = row_gram(&self.b, self.dimension);
            let exact = Cholesky::new(&gram, self.dimension, 0., 0.)?;
            self.refine(&exact, b_norm, rhs, transposed)
        })
    }

    fn refine(
        &self,
        factor: &Cholesky,
        b_norm: f64,
        rhs: &[f64],
        transposed: bool,
    ) -> Option<Vec<f64>> {
        let solve = |r: &[f64]| self.reciprocal_apply(factor, r, transposed);
        let mut x = solve(rhs)?;
        let rhs_norm = norm(rhs);
        let mut previous_error = f64::INFINITY;
        for iteration in 0..4 {
            let mut residual = if transposed {
                transpose_apply(&self.b, self.dimension, &x)
            } else {
                apply(&self.b, &x)
            };
            for (r, &b) in residual.iter_mut().zip(rhs) {
                *r = b - *r;
            }
            let tolerance = 64. * f64::EPSILON * (rhs_norm + b_norm * norm(&x));
            let error = norm(&residual);
            if error == 0. || iteration > 0 && error <= tolerance {
                return Some(x);
            }
            if iteration == 3 || error >= previous_error {
                return None;
            }
            previous_error = error;
            for (entry, correction) in x.iter_mut().zip(solve(&residual)?) {
                *entry += correction;
            }
        }
        None
    }
}

fn spectral_dual(b: &[f64], n: usize, values: &[f64], vectors: &[f64]) -> Vec<f64> {
    let mut dual = vec![0.; b.len()];
    for (v, &value) in vectors.chunks_exact(n).zip(values) {
        let mode = transpose_apply(b, n, v);
        for (&weight, row) in v.iter().zip(dual.chunks_exact_mut(n)) {
            for (entry, &x) in row.iter_mut().zip(&mode) {
                *entry += weight * x / value;
            }
        }
    }
    dual
}

#[cfg(test)]
mod tests;
