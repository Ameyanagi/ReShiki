//! Dense Wilson matrices. Eigenvectors and Cartesian derivatives occupy rows.

use crate::linalg::{dot, symmetric_eigen};

/// Retain λ > 10⁻¹⁰ λmax in the Wilson metric (singular values above 10⁻⁵ σmax).
/// Use the same cutoff when inverting a trial-geometry metric in the frozen DLC basis.
const RELATIVE_CUTOFF: f64 = 1e-10;

pub(super) fn apply(rows: &[f64], x: &[f64]) -> Vec<f64> {
    rows.chunks_exact(x.len()).map(|r| dot(r, x)).collect()
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
    let n = rows.len() / width;
    let mut gram = vec![0.; n * n];
    for (i, a) in rows.chunks_exact(width).enumerate() {
        for (j, b) in rows.chunks_exact(width).take(i + 1).enumerate() {
            let value = dot(a, b);
            gram[i * n + j] = value;
            gram[j * n + i] = value;
        }
    }
    gram
}

/// A H Aᵀ for a symmetric H and rectangular A with `width` columns.
pub(super) fn congruence(a: &[f64], h: &[f64], width: usize) -> Option<Vec<f64>> {
    let n = a.len() / width;
    let mut result = vec![0.; n * n];
    for (i, row) in a.chunks_exact(width).enumerate() {
        let product = apply(h, row);
        for (j, other) in a.chunks_exact(width).take(i + 1).enumerate() {
            let value = dot(&product, other);
            result[i * n + j] = value;
            result[j * n + i] = value;
        }
    }
    result.iter().all(|v| v.is_finite()).then_some(result)
}

/// B_u and (B_u⁺)ᵀ at a single geometry, with free Cartesian columns only.
#[derive(Clone)]
pub(super) struct Tangent {
    pub(super) b: Vec<f64>,
    pub(super) dual: Vec<f64>,
}

impl Tangent {
    /// Baker, Kessi & Delley, J. Chem. Phys. 105, 192 (1996).
    /// Diagonalizing the smaller BᵀB gives the nonzero eigenpairs of G = BBᵀ.
    /// For a unit right eigenvector v, U = Bv/√λ and UᵀB = √λ vᵀ.
    pub(super) fn delocalize(rows: &[f64], nfree: usize) -> Result<(Vec<f64>, Self), String> {
        let nprim = rows.len() / nfree;
        let mut columns = vec![0.; rows.len()];
        for (i, row) in rows.chunks_exact(nfree).enumerate() {
            for (j, x) in row.iter().enumerate() {
                columns[j * nprim + i] = *x;
            }
        }
        let (values, vectors) = symmetric_eigen(row_gram(&columns, nprim), nfree)?;
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
            basis.extend(rows.chunks_exact(nfree).map(|p| dot(p, v) / sigma));
            b.extend(v.iter().map(|x| x * sigma));
            dual.extend(v.iter().map(|x| x / sigma));
        }
        Ok((basis, Self { b, dual }))
    }

    /// Hold U fixed and recompute both the Wilson rows and their metric.
    pub(super) fn at(rows: &[f64], basis: &[f64], nfree: usize) -> Option<Self> {
        let nprim = rows.len() / nfree;
        let b: Vec<_> = basis
            .chunks_exact(nprim)
            .flat_map(|u| transpose_apply(rows, nfree, u))
            .collect();
        let dim = b.len() / nfree;
        let (lambda, vectors) = symmetric_eigen(row_gram(&b, nfree), dim).ok()?;
        if lambda[0] <= RELATIVE_CUTOFF * lambda[dim - 1] {
            return None;
        }
        // (BBᵀ)⁻¹ B = Σ v (vᵀ B)/λ, for the rows of the delocalized B.
        let mut dual = vec![0.; b.len()];
        for (v, value) in vectors.chunks_exact(dim).zip(lambda) {
            let mut mode = transpose_apply(&b, nfree, v);
            mode.iter_mut().for_each(|x| *x /= value);
            for (&weight, row) in v.iter().zip(dual.chunks_exact_mut(nfree)) {
                for (entry, x) in row.iter_mut().zip(&mode) {
                    *entry += weight * x;
                }
            }
        }
        dual.iter()
            .all(|x| x.is_finite())
            .then_some(Self { b, dual })
    }
}
