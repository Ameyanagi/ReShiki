//! Products of the sparse primitive Wilson rows at a DLC build.

use crate::linalg::dot;

pub(super) struct WilsonRows {
    offsets: Vec<usize>,
    columns: Vec<usize>,
    derivatives: Vec<f64>,
}

impl WilsonRows {
    pub(super) fn new(dense: &[f64], width: usize) -> Self {
        let mut offsets = vec![0];
        let mut columns = Vec::new();
        let mut derivatives = Vec::new();
        for row in dense.chunks_exact(width) {
            for (j, &x) in row.iter().enumerate() {
                if x != 0.0 {
                    columns.push(j);
                    derivatives.push(x);
                }
            }
            offsets.push(columns.len());
        }
        Self {
            offsets,
            columns,
            derivatives,
        }
    }

    /// BᵀB is the sum of primitive row outer products. Only the fragment
    /// rotation rows touch more than four atoms.
    pub(super) fn gram(&self, width: usize) -> Vec<f64> {
        let mut gram = vec![0.; width * width];
        for span in self.offsets.windows(2) {
            for i in span[0]..span[1] {
                let row = &mut gram[self.columns[i] * width..][..width];
                let a = self.derivatives[i];
                for j in span[0]..=i {
                    row[self.columns[j]] += a * self.derivatives[j];
                }
            }
        }
        for i in 0..width {
            for j in 0..i {
                gram[j * width + i] = gram[i * width + j];
            }
        }
        gram
    }

    pub(super) fn project_mode(&self, v: &[f64], sigma: f64, basis: &mut Vec<f64>) {
        for span in self.offsets.windows(2) {
            let range = span[0]..span[1];
            let values = &self.derivatives[range.clone()];
            // Centroid/rotation rows can include every Cartesian component.
            let product = if values.len() == v.len() {
                dot(values, v)
            } else {
                self.columns[range]
                    .iter()
                    .zip(values)
                    .map(|(&j, &x)| x * v[j])
                    .sum()
            };
            basis.push(product / sigma);
        }
    }
}

/// An orthonormal basis for range(P) is unique up to a rotation. If PᵀP = L Lᵀ,
/// U = P L⁻ᵀ gives the same DLC subspace, norms and rational model as the
/// spectral basis. Its Wilson matrix UᵀP is Lᵀ and its reciprocal is L⁻¹.
/// Cholesky QR is used only well inside the full-rank regime, where forming
/// the Gram matrix is accurate (Golub & Van Loan, Matrix Computations, §5.2).
pub(super) struct OrthonormalBasis {
    pub(super) modes: Vec<f64>,
    pub(super) b: Vec<f64>,
    pub(super) dual: Vec<f64>,
}

impl WilsonRows {
    pub(super) fn cholesky_basis(&self, gram: &[f64], n: usize) -> Option<OrthonormalBasis> {
        let factor = crate::linalg::cholesky::Cholesky::new(gram, n, 0., 0.)?;
        let lower = factor.lower();
        let scale = factor.scale().sqrt();
        let mut inverse = vec![0.; n * n];
        for i in 0..n {
            // Forward substitution for row i of L⁻¹; previous rows are triangular.
            let (above, below) = inverse.split_at_mut(i * n);
            let row = &mut below[..=i];
            row[i] = scale.recip();
            for (j, previous) in above.chunks_exact(n).enumerate() {
                let weight = lower[i * n + j];
                for (entry, &value) in row[..=j].iter_mut().zip(previous) {
                    *entry -= weight * value;
                }
            }
            let scale = lower[i * n + i].recip();
            row.iter_mut().for_each(|v| *v *= scale);
        }

        // κ₂(P)² ≤ ||PᵀP||∞ ||L⁻¹||₁ ||L⁻¹||∞. Use the spectral
        // construction for marginal conditioning and its original rank test.
        let max_row_sum = |m: &[f64]| {
            m.chunks_exact(n)
                .map(|r| r.iter().map(|v| v.abs()).sum::<f64>())
                .fold(0., f64::max)
        };
        let mut column_sums = vec![0.; n];
        for row in inverse.chunks_exact(n) {
            for (sum, &value) in column_sums.iter_mut().zip(row) {
                *sum += value.abs();
            }
        }
        let bound =
            max_row_sum(gram) * max_row_sum(&inverse) * column_sums.into_iter().fold(0., f64::max);
        // Keep three orders of magnitude of headroom below the rank cutoff
        // (κ² = 10¹⁰). Also limit amplification of Gram rounding for wide rows.
        if !bound.is_finite() || bound > 1e7 || bound * n as f64 * f64::EPSILON > 1e-6 {
            return None;
        }
        let mut modes = Vec::with_capacity(n * (self.offsets.len() - 1));
        let mut b = vec![0.; n * n];
        for (i, row) in inverse.chunks_exact(n).enumerate() {
            self.project_mode(row, 1., &mut modes);
            for j in i..n {
                b[i * n + j] = scale * lower[j * n + i];
            }
        }
        Some(OrthonormalBasis {
            modes,
            b,
            dual: inverse,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::WilsonRows;
    use crate::linalg::dot;

    #[test]
    fn triangular_chart_is_orthonormal_and_reproduces_the_wilson_matrix() {
        let n = 35;
        let mut rows = vec![0.0; (n + 17) * n];
        for (i, row) in rows.chunks_exact_mut(n).enumerate() {
            row[i % n] = 1. + 0.07 * (i % 7) as f64;
            row[(i + 3) % n] = 0.2;
        }
        let sparse = WilsonRows::new(&rows, n);
        let build = sparse.cholesky_basis(&sparse.gram(n), n).unwrap();
        let modes: Vec<_> = build.modes.chunks_exact(n + 17).collect();
        for (i, mode) in modes.iter().enumerate() {
            for (j, other) in modes.iter().enumerate() {
                let identity = f64::from(i == j);
                assert!((dot(mode, other) - identity).abs() < 1e-12);
                let projected: f64 = mode
                    .iter()
                    .zip(rows.chunks_exact(n))
                    .map(|(u, row)| u * row[j])
                    .sum();
                assert!((projected - build.b[i * n + j]).abs() < 1e-12);
                assert!(
                    (dot(
                        &build.b[i * n..(i + 1) * n],
                        &build.dual[j * n..(j + 1) * n]
                    ) - identity)
                        .abs()
                        < 1e-12
                );
            }
        }
        // A nearly dependent Cartesian column uses the original spectral rank test.
        for row in rows.chunks_exact_mut(n) {
            row[0] *= 1e-5;
        }
        let sparse = WilsonRows::new(&rows, n);
        assert!(sparse.cholesky_basis(&sparse.gram(n), n).is_none());
    }
}
