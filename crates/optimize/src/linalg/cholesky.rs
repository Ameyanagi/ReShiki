//! Scaled Cholesky factorization and triangular solves.

use super::{dot, dot_four};

/// A + shift I = scale L Lᵀ. The normalized lower triangle is stored row-major;
/// entries above the diagonal are zero. Golub & Van Loan, Matrix Computations.
#[derive(Clone)]
pub(crate) struct Cholesky {
    lower: Vec<f64>,
    dimension: usize,
    scale: f64,
}

impl Cholesky {
    /// Factor a symmetric matrix with an optional signed diagonal shift.
    /// `minimum_pivot` is the strict lower bound on a squared diagonal pivot
    /// after scaling. Zero accepts every positive pivot; 64*EPSILON retains
    /// the numerical rank threshold used by DIIS.
    pub(crate) fn new(
        matrix: &[f64],
        dimension: usize,
        shift: f64,
        minimum_pivot: f64,
    ) -> Option<Self> {
        if matrix.len() != dimension.checked_mul(dimension)?
            || !shift.is_finite()
            || !minimum_pivot.is_finite()
            || minimum_pivot < 0.
        {
            return None;
        }
        let scale = matrix
            .iter()
            .fold(shift.abs(), |largest, value| largest.max(value.abs()));
        if !(scale.is_finite() && scale > 0.) || matrix.iter().any(|value| !value.is_finite()) {
            return None;
        }
        let mut lower = vec![0.; matrix.len()];
        for j in 0..dimension {
            let diagonal = j * dimension + j;
            let prefix = &lower[j * dimension..diagonal];
            let mut value = matrix[diagonal] / scale - dot(prefix, prefix);
            if shift != 0. {
                value += shift / scale;
            }
            if !value.is_finite() || value <= minimum_pivot {
                return None;
            }
            let pivot = value.sqrt();
            lower[diagonal] = pivot;
            let mut i = j + 1;
            if dimension > 256 {
                // Independent rows share this pivot's prefix while retaining
                // each dot product's four accumulators and summation order.
                while i + 4 <= dimension {
                    let rows = std::array::from_fn(|r| {
                        &lower[(i + r) * dimension..(i + r) * dimension + j]
                    });
                    let products = dot_four(rows, &lower[j * dimension..diagonal]);
                    for (r, product) in products.into_iter().enumerate() {
                        let index = (i + r) * dimension + j;
                        lower[index] = (matrix[index] / scale - product) / pivot;
                    }
                    i += 4;
                }
            }
            for i in i..dimension {
                let value = matrix[i * dimension + j] / scale
                    - dot(
                        &lower[i * dimension..i * dimension + j],
                        &lower[j * dimension..diagonal],
                    );
                lower[i * dimension + j] = value / pivot;
            }
        }
        Some(Self {
            lower,
            dimension,
            scale,
        })
    }

    pub(crate) fn lower(&self) -> &[f64] {
        &self.lower
    }

    pub(crate) fn scale(&self) -> f64 {
        self.scale
    }

    /// Preserve the triangular arithmetic used by the original DIIS solve.
    pub(crate) fn solve(&self, rhs: &[f64]) -> Option<Vec<f64>> {
        let n = self.dimension;
        if rhs.len() != n {
            return None;
        }
        let mut result = vec![0.; n];
        for i in 0..n {
            result[i] = (rhs[i] / self.scale - dot(&self.lower[i * n..i * n + i], &result[..i]))
                / self.lower[i * n + i];
        }
        for i in (0..n).rev() {
            for j in i + 1..n {
                result[i] -= self.lower[j * n + i] * result[j];
            }
            result[i] /= self.lower[i * n + i];
        }
        result
            .iter()
            .all(|value| value.is_finite())
            .then_some(result)
    }

    /// Reuse a right-hand-side buffer. Contiguous backward updates avoid
    /// repeatedly gathering a factor column for new performance-sensitive
    /// callers; the result can differ from `solve` by floating-point roundoff.
    pub(crate) fn solve_in_place(&self, rhs: &mut [f64]) -> Option<()> {
        let n = self.dimension;
        if rhs.len() != n {
            return None;
        }
        for i in 0..n {
            rhs[i] = (rhs[i] / self.scale - dot(&self.lower[i * n..i * n + i], &rhs[..i]))
                / self.lower[i * n + i];
        }
        for i in (0..n).rev() {
            rhs[i] /= self.lower[i * n + i];
            let xi = rhs[i];
            for (value, lij) in rhs[..i].iter_mut().zip(&self.lower[i * n..i * n + i]) {
                *value -= lij * xi;
            }
        }
        rhs.iter().all(|value| value.is_finite()).then_some(())
    }
}

// The original traversal supplies an independent arithmetic reference.
#[cfg(test)]
fn factor_rows(
    matrix: &[f64],
    lower: &mut [f64],
    dimension: usize,
    scale: f64,
    shift: f64,
    minimum_pivot: f64,
) -> Option<()> {
    for i in 0..dimension {
        for j in 0..=i {
            let mut value = matrix[i * dimension + j] / scale
                - dot(
                    &lower[i * dimension..i * dimension + j],
                    &lower[j * dimension..j * dimension + j],
                );
            if i == j {
                if shift != 0. {
                    value += shift / scale;
                }
                if !value.is_finite() || value <= minimum_pivot {
                    return None;
                }
                lower[i * dimension + j] = value.sqrt();
            } else {
                lower[i * dimension + j] = value / lower[j * dimension + j];
            }
        }
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_traversal_matches_original_factor_bits_and_pivot_decisions() {
        // The original row traversal is a reference for entry-wise arithmetic,
        // independent of the order in which columns become available.
        let reference = |matrix: &[f64], n: usize, shift: f64, minimum_pivot: f64| {
            let scale = matrix.iter().fold(shift.abs(), |a, b| a.max(b.abs()));
            let mut lower = vec![0.; n * n];
            factor_rows(matrix, &mut lower, n, scale, shift, minimum_pivot).map(|()| lower)
        };
        for n in [
            1_usize, 2, 3, 9, 24, 63, 120, 204, 255, 256, 257, 294, 295, 300,
        ] {
            for scale in [1e-250, 1., 1e250] {
                let mut matrix = vec![0.; n * n];
                for i in 0..n {
                    for j in 0..=i {
                        let value = scale * (((i * 19 + j * 43 + 7) % 101) as f64 * 1e-6);
                        matrix[i * n + j] = value;
                        matrix[j * n + i] = value;
                    }
                    matrix[i * n + i] = scale * (1. + 0.03 * (i % 7) as f64);
                }
                for shift in [-0.25 * scale, 0., 0.5 * scale] {
                    let expected = reference(&matrix, n, shift, 0.).unwrap();
                    let factor = Cholesky::new(&matrix, n, shift, 0.).unwrap();
                    assert!(
                        factor
                            .lower()
                            .iter()
                            .zip(expected)
                            .all(|(actual, expected)| actual.to_bits() == expected.to_bits())
                    );
                }
            }
        }
        for (matrix, shift, minimum_pivot) in [
            ([1., 1., 1., 1.], 0., 0.),
            ([1., 2., 2., 1.], 0., 0.),
            ([1., 0., 0., 32. * f64::EPSILON], 0., 64. * f64::EPSILON),
            ([1., 0., 0., 1.], -1., 0.),
        ] {
            assert_eq!(
                Cholesky::new(&matrix, 2, shift, minimum_pivot).map(|factor| factor.lower),
                reference(&matrix, 2, shift, minimum_pivot),
            );
        }
        for n in [257, 300] {
            for (diagonal, value, minimum_pivot) in [
                (1, -1., 0.),
                (n - 1, 0., 0.),
                (n - 1, 32. * f64::EPSILON, 64. * f64::EPSILON),
            ] {
                let mut matrix = vec![0.; n * n];
                for i in 0..n {
                    matrix[i * n + i] = 1.;
                }
                matrix[diagonal * n + diagonal] = value;
                assert_eq!(
                    Cholesky::new(&matrix, n, 0., minimum_pivot).map(|factor| factor.lower),
                    reference(&matrix, n, 0., minimum_pivot),
                );
            }
        }
    }

    #[test]
    fn signed_shift_and_scaled_triangle_solve_the_requested_matrix() {
        let matrix = [4., 1., 1., 3.];
        for shift in [-1., 0., 2.] {
            let factor = Cholesky::new(&matrix, 2, shift, 0.).unwrap();
            let rhs = [1., -2.];
            let direct = factor.solve(&rhs).unwrap();
            let mut in_place = rhs;
            factor.solve_in_place(&mut in_place).unwrap();
            for solution in [direct.as_slice(), in_place.as_slice()] {
                assert!(((4. + shift) * solution[0] + solution[1] - rhs[0]).abs() < 1e-14);
                assert!((solution[0] + (3. + shift) * solution[1] - rhs[1]).abs() < 1e-14);
            }
            let l = factor.lower();
            assert!((factor.scale() * (l[2] * l[2] + l[3] * l[3]) - 3. - shift).abs() < 1e-14);
        }
    }

    #[test]
    fn minimum_pivot_preserves_the_diis_rank_threshold() {
        let matrix = [1., 0., 0., 32. * f64::EPSILON];
        assert!(Cholesky::new(&matrix, 2, 0., 0.).is_some());
        assert!(Cholesky::new(&matrix, 2, 0., 64. * f64::EPSILON).is_none());
    }
}
