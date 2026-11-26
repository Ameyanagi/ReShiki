//! Dense symmetric eigendecomposition and vector operations for geometry optimization.

pub(crate) mod cholesky;
mod projected;

pub(crate) use projected::projected_eigen;

// A fused operation is fast on these targets. Other targets (including WASM
// and baseline x86-64) can vectorize separate multiplies and additions.
#[inline]
pub(crate) fn multiply_add(x: f64, y: f64, z: f64) -> f64 {
    if cfg!(any(target_arch = "aarch64", target_feature = "fma")) {
        x.mul_add(y, z)
    } else {
        x * y + z
    }
}

/// The Euclidean inner product, with four independent sums to limit the depth
/// of rounding error and allow the compiler to execute the sums in parallel.
pub(crate) fn dot(x: &[f64], y: &[f64]) -> f64 {
    assert_eq!(x.len(), y.len(), "inner product dimensions differ");
    let mut sums = [0.0; 4];
    let (xs, x_remainder) = x.as_chunks::<4>();
    let (ys, y_remainder) = y.as_chunks::<4>();
    for (a, b) in xs.iter().zip(ys) {
        sums[0] = multiply_add(a[0], b[0], sums[0]);
        sums[1] = multiply_add(a[1], b[1], sums[1]);
        sums[2] = multiply_add(a[2], b[2], sums[2]);
        sums[3] = multiply_add(a[3], b[3], sums[3]);
    }
    let mut result = (sums[0] + sums[1]) + (sums[2] + sums[3]);
    for (a, b) in x_remainder.iter().zip(y_remainder) {
        result += a * b;
    }
    result
}

/// A scaled Euclidean norm, avoiding overflow or underflow in the squares.
pub(crate) fn norm(x: &[f64]) -> f64 {
    let mut scale: f64 = 0.0;
    for &value in x {
        if value.is_nan() {
            return f64::NAN;
        }
        scale = scale.max(value.abs());
    }
    if scale == 0.0 || scale.is_infinite() {
        return scale;
    }
    let mut sum = 0.0;
    for &value in x {
        let scaled = value / scale;
        sum += scaled * scaled;
    }
    scale * sum.sqrt()
}

/// Diagonalize a row-major real symmetric matrix.
///
/// Eigenvalues are ascending; row `i` of the returned row-major matrix is the
/// unit eigenvector belonging to eigenvalue `i`. The implementation follows the
/// Householder similarity reduction and implicit Wilkinson-shift QR iteration
/// described in the symmetric eigenvalue chapter of Golub and Van Loan,
/// *Matrix Computations* \[Golub2013Matrix\].
/// All arithmetic and storage are implemented here using safe, standard Rust.
pub(crate) fn symmetric_eigen(
    mut matrix: Vec<f64>,
    n: usize,
) -> Result<(Vec<f64>, Vec<f64>), String> {
    let size = n
        .checked_mul(n)
        .ok_or_else(|| "eigenproblem dimensions overflow".to_owned())?;
    if matrix.len() != size {
        return Err(format!(
            "expected {size} matrix entries for dimension {n}, got {}",
            matrix.len()
        ));
    }
    if matrix.iter().any(|entry| !entry.is_finite()) {
        return Err("eigenproblem contains a nonfinite matrix entry".to_owned());
    }
    if n == 0 {
        return Ok((Vec::new(), Vec::new()));
    }
    let scale = matrix.iter().fold(0.0_f64, |a, b| a.max(b.abs()));
    for i in 0..n {
        for j in 0..i {
            let lower = matrix[i * n + j];
            let upper = matrix[j * n + i];
            let tolerance = 64.0 * f64::EPSILON * lower.abs().max(upper.abs());
            if (lower - upper).abs() > tolerance {
                return Err(format!("matrix is not symmetric at ({i}, {j})"));
            }
            if scale != 0.0 {
                // Average after scaling so that two large finite entries do
                // not overflow, and use only this lower triangle from here.
                matrix[i * n + j] = 0.5 * (lower / scale + upper / scale);
            }
        }
        if scale != 0.0 {
            matrix[i * n + i] /= scale;
        }
    }
    let mut vectors = vec![0.0; size];
    for i in 0..n {
        vectors[i * n + i] = 1.0;
    }
    if scale == 0.0 {
        return Ok((vec![0.0; n], vectors));
    }
    if n == 1 {
        return Ok((vec![matrix[0] * scale], vectors));
    }

    let mut diagonal = vec![0.0; n];
    let mut off_diagonal = vec![0.0; n - 1];
    let mut tau = vec![0.0; n - 1];
    let mut v = vec![0.0; n];
    let mut w = vec![0.0; n];

    // H = I - tau v v^T, with v[0] = 1. The rank-two update
    // H A H = A - v w^T - w v^T uses
    // w = tau A v - (tau^2 / 2)(v^T A v) v.
    // Reflectors remain in the eliminated part of the lower triangle.
    for k in 0..n - 1 {
        let first = k + 1;
        let m = n - first;
        for i in 0..m {
            v[i] = matrix[(first + i) * n + k];
        }
        let leading = v[0];
        if norm(&v[1..m]) == 0.0 {
            off_diagonal[k] = leading;
            continue;
        }
        let alpha = -norm(&v[..m]).copysign(leading);
        off_diagonal[k] = alpha;
        let denominator = leading - alpha;
        for value in &mut v[1..m] {
            *value /= denominator;
        }
        v[0] = 1.0;
        tau[k] = (alpha - leading) / alpha;
        w[..m].fill(0.0);

        // Symmetric matrix-vector multiplication from the lower triangle.
        // The dot and scaled addition are separate contiguous loops.
        for i in 0..m {
            let start = (first + i) * n + first;
            let row = &matrix[start..start + i + 1];
            w[i] += dot(row, &v[..i + 1]);
            let vi = v[i];
            for (wj, aij) in w[..i].iter_mut().zip(&row[..i]) {
                *wj = multiply_add(vi, *aij, *wj);
            }
        }
        let correction = -0.5 * tau[k] * dot(&v[..m], &w[..m]);
        for (wi, vi) in w[..m].iter_mut().zip(&v[..m]) {
            *wi = tau[k] * (*wi + correction * vi);
        }
        for i in 0..m {
            let start = (first + i) * n + first;
            let row = &mut matrix[start..start + i + 1];
            let vi = v[i];
            let wi = w[i];
            for ((aij, vj), wj) in row.iter_mut().zip(&v[..i + 1]).zip(&w[..i + 1]) {
                *aij -= multiply_add(vi, *wj, wi * vj);
            }
            matrix[(first + i) * n + k] = v[i];
        }
    }
    for i in 0..n {
        diagonal[i] = matrix[i * n + i];
    }

    // Build Q^T = H_last ... H_first. Apply small batches of reflectors
    // to one row at a time so that the working row stays in the first-level
    // cache throughout the batch. Leading identity rows remain untouched.
    const REFLECTOR_BATCH: usize = 32;
    let mut reflectors = vec![0.0; REFLECTOR_BATCH * n];
    let mut high = n - 1;
    while high > 0 {
        let low = high.saturating_sub(REFLECTOR_BATCH);
        for k in low..high {
            let reflector = &mut reflectors[(k - low) * n..(k - low + 1) * n];
            for i in k + 1..n {
                reflector[i] = matrix[i * n + k];
            }
        }
        independent_chunks(&mut vectors[(low + 1) * n..], n, n, |rows, offset| {
            for (j, row) in rows.chunks_exact_mut(n).enumerate() {
                let i = low + 1 + offset + j;
                for k in (low..high.min(i)).rev() {
                    if tau[k] == 0.0 {
                        continue;
                    }
                    let first = k + 1;
                    let reflector = &reflectors[(k - low) * n + first..(k - low + 1) * n];
                    let active = &mut row[first..];
                    let weight = -tau[k] * dot(active, reflector);
                    for (entry, vi) in active.iter_mut().zip(reflector) {
                        *entry = multiply_add(weight, *vi, *entry);
                    }
                }
            }
        });
        high = low;
    }

    diagonalize_tridiagonal(&mut diagonal, &mut off_diagonal, &mut vectors, n)?;

    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&i, &j| diagonal[i].total_cmp(&diagonal[j]));
    let mut eigenvalues = Vec::with_capacity(n);
    let mut eigenvectors = Vec::with_capacity(size);
    for index in order {
        let value = diagonal[index] * scale;
        if !value.is_finite() {
            return Err("eigenvalue is outside the finite f64 range".to_owned());
        }
        eigenvalues.push(value);
        eigenvectors.extend_from_slice(&vectors[index * n..(index + 1) * n]);
    }
    Ok((eigenvalues, eigenvectors))
}

const STRIP: usize = 64;
const ROTATION_BATCH: usize = 32_768;

/// Chase a QR bulge through each unreduced tridiagonal block.
fn diagonalize_tridiagonal(
    diagonal: &mut [f64],
    off_diagonal: &mut [f64],
    vectors: &mut [f64],
    n: usize,
) -> Result<(), String> {
    let strips = n.div_ceil(STRIP);
    let mut packed = vec![0.0; strips * n * STRIP];
    for (strip, tile) in packed.chunks_exact_mut(n * STRIP).enumerate() {
        let first = strip * STRIP;
        let width = STRIP.min(n - first);
        for (i, row) in tile.as_chunks_mut::<STRIP>().0.iter_mut().enumerate() {
            row[..width].copy_from_slice(&vectors[i * n + first..i * n + first + width]);
        }
    }
    let mut end = n - 1;
    let mut sweeps = 0_usize;
    let mut rotations = Vec::with_capacity((n * n).min(ROTATION_BATCH));
    let limit = n.saturating_mul(128);
    while end > 0 {
        let mut start = end;
        while start > 0 {
            let threshold = (f64::EPSILON * (diagonal[start - 1].abs() + diagonal[start].abs()))
                .max(f64::MIN_POSITIVE);
            if off_diagonal[start - 1].abs() <= threshold {
                off_diagonal[start - 1] = 0.0;
                break;
            }
            start -= 1;
        }
        if start == end {
            end -= 1;
            continue;
        }
        sweeps += 1;
        if sweeps > limit {
            return Err("symmetric QR iteration did not converge".to_owned());
        }

        // The trailing 2-by-2 eigenvalue nearer its last diagonal entry is
        // Wilkinson's shift; this form avoids squaring the off-diagonal.
        let half_gap = 0.5 * (diagonal[end - 1] - diagonal[end]);
        let edge = off_diagonal[end - 1];
        let denominator = half_gap + half_gap.hypot(edge).copysign(half_gap);
        let shift = diagonal[end] - edge * (edge / denominator);
        let mut x = diagonal[start] - shift;
        let mut bulge = off_diagonal[start];
        for k in start..end {
            let radius = x.hypot(bulge);
            let (cosine, sine) = if radius == 0.0 {
                (1.0, 0.0)
            } else {
                (x / radius, bulge / radius)
            };
            if k > start {
                off_diagonal[k - 1] = radius;
            }

            // G^T [a b; b d] G, for G = [c -s; s c].
            let a = diagonal[k];
            let b = off_diagonal[k];
            let d = diagonal[k + 1];
            let cc = cosine * cosine;
            let ss = sine * sine;
            let cs = cosine * sine;
            diagonal[k] = cc * a + 2.0 * cs * b + ss * d;
            diagonal[k + 1] = ss * a - 2.0 * cs * b + cc * d;
            off_diagonal[k] = (cc - ss) * b + cs * (d - a);
            if k + 1 < end {
                let next = off_diagonal[k + 1];
                bulge = sine * next;
                off_diagonal[k + 1] = cosine * next;
                x = off_diagonal[k];
            }

            rotations.push((k, cosine, sine));
        }
        if rotations.len() >= ROTATION_BATCH {
            apply_rotations(&mut packed, n, &rotations);
            rotations.clear();
        }
    }
    apply_rotations(&mut packed, n, &rotations);
    for (strip, tile) in packed.chunks_exact(n * STRIP).enumerate() {
        let first = strip * STRIP;
        let width = STRIP.min(n - first);
        for (i, row) in tile.as_chunks::<STRIP>().0.iter().enumerate() {
            vectors[i * n + first..i * n + first + width].copy_from_slice(&row[..width]);
        }
    }
    if diagonal.iter().any(|entry| !entry.is_finite())
        || vectors.iter().any(|entry| !entry.is_finite())
    {
        return Err("symmetric QR iteration produced a nonfinite result".to_owned());
    }
    Ok(())
}

/// Apply a batch to narrow column strips, retaining the active strip in cache.
fn apply_rotations(vectors: &mut [f64], n: usize, rotations: &[(usize, f64, f64)]) {
    independent_chunks(vectors, n * STRIP, n, |group, _| {
        for tile in group.chunks_exact_mut(n * STRIP) {
            for &(k, cosine, sine) in rotations {
                let (before, after) = tile.split_at_mut((k + 1) * STRIP);
                let left = &mut before[k * STRIP..];
                let right = &mut after[..STRIP];
                for (u, v) in left.iter_mut().zip(right) {
                    let old_u = *u;
                    let old_v = *v;
                    *u = multiply_add(cosine, old_u, sine * old_v);
                    *v = multiply_add(cosine, old_v, -sine * old_u);
                }
            }
        }
    });
}

// Whole rows or packed column strips are independent. The calling thread
// always drains this queue, including when an optional worker cannot start.
fn independent_chunks(
    values: &mut [f64],
    unit: usize,
    n: usize,
    apply: impl Fn(&mut [f64], usize) + Sync,
) {
    let workers = if n < 768 || cfg!(target_family = "wasm") {
        1
    } else {
        std::thread::available_parallelism()
            .map_or(1, |count| count.get().min(4))
            .min(values.len() / unit)
            .max(1)
    };
    if workers == 1 {
        apply(values, 0);
        return;
    }
    let units_per_job = (values.len() / unit).div_ceil(workers * 4).max(1);
    let queue = std::sync::Mutex::new(values.chunks_mut(units_per_job * unit).enumerate());
    let run = || loop {
        let job = queue.lock().expect("an eigensolver worker panicked").next();
        let Some((index, chunk)) = job else { break };
        apply(chunk, index * units_per_job);
    };
    std::thread::scope(|scope| {
        for _ in 1..workers {
            let _ = std::thread::Builder::new().spawn_scoped(scope, run);
        }
        run();
    });
}

#[cfg(test)]
mod tests {
    use super::{dot, independent_chunks, norm, symmetric_eigen};

    fn check_eigenpairs(matrix: &[f64], n: usize, tolerance: f64) {
        let (values, vectors) = symmetric_eigen(matrix.to_vec(), n).unwrap();
        assert!(values.windows(2).all(|pair| pair[0] <= pair[1]));
        let scale = norm(matrix).max(1.0);
        for i in 0..n {
            let vector = &vectors[i * n..(i + 1) * n];
            let residual: Vec<f64> = matrix
                .chunks_exact(n)
                .zip(vector)
                .map(|(row, vi)| dot(row, vector) - values[i] * vi)
                .collect();
            assert!(
                norm(&residual) <= tolerance * scale,
                "eigenpair {i}: {}",
                norm(&residual)
            );
            for j in 0..=i {
                let overlap = dot(vector, &vectors[j * n..(j + 1) * n]);
                let expected = if i == j { 1.0 } else { 0.0 };
                assert!(
                    (overlap - expected).abs() <= tolerance,
                    "overlap ({i}, {j}): {overlap}"
                );
            }
        }
    }

    #[test]
    fn independent_work_queue_covers_serial_and_parallel_chunks() {
        for n in [47, 768] {
            let mut values = vec![f64::NAN; 1_030];
            independent_chunks(&mut values, 5, n, |chunk, first| {
                for (i, value) in chunk.iter_mut().enumerate() {
                    *value = (first * 5 + i + 1) as f64;
                }
            });
            assert!(
                values
                    .iter()
                    .enumerate()
                    .all(|(i, value)| *value == (i + 1) as f64)
            );
        }
    }

    #[test]
    fn lcg_dense_residuals_and_orthonormality() {
        let n = 47;
        let mut state = 0x7b1d_67c0_14ea_92a5_u64;
        let mut matrix = vec![0.0; n * n];
        for i in 0..n {
            for j in 0..=i {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                let value = ((state >> 11) as f64) * (1.0 / ((1_u64 << 53) as f64)) - 0.5;
                matrix[i * n + j] = value;
                matrix[j * n + i] = value;
            }
        }
        check_eigenpairs(&matrix, n, 2e-12);
    }

    #[test]
    fn repeated_and_clustered_eigenvalues() {
        // I + 2 uu^T has a repeated eigenvalue of one and one eigenvalue
        // of three. A tiny diagonal perturbation also exercises clusters.
        let n = 19;
        let u: Vec<f64> = (0..n).map(|i| (i + 1) as f64).collect();
        let length = norm(&u);
        for perturbation in [0.0, 1e-13] {
            let mut matrix = vec![0.0; n * n];
            for i in 0..n {
                for j in 0..n {
                    matrix[i * n + j] = 2.0 * (u[i] / length) * (u[j] / length);
                }
                matrix[i * n + i] += 1.0 + perturbation * (i as f64);
            }
            check_eigenpairs(&matrix, n, 2e-12);
            let (values, _) = symmetric_eigen(matrix, n).unwrap();
            assert!((values[n - 1] - 3.0).abs() < 3e-12);
            assert!(
                values[..n - 1]
                    .iter()
                    .all(|value| (value - 1.0).abs() < 3e-12)
            );
        }
    }

    #[test]
    fn zero_diagonal_and_tiny_or_large_scales() {
        for scale in [1e-250, 1.0, 1e250] {
            let matrix = vec![0.0, scale, 0.0, scale, 0.0, scale, 0.0, scale, 0.0];
            check_eigenpairs(&matrix, 3, 2e-13);
            let (values, _) = symmetric_eigen(matrix, 3).unwrap();
            assert!((values[0] / scale + 2.0_f64.sqrt()).abs() < 2e-14);
            assert!((values[2] / scale - 2.0_f64.sqrt()).abs() < 2e-14);
        }
    }

    #[test]
    fn diagonal_zero_and_one_dimensional_matrices() {
        assert_eq!(
            symmetric_eigen(Vec::new(), 0).unwrap(),
            (Vec::new(), Vec::new())
        );
        assert_eq!(
            symmetric_eigen(vec![-2.0], 1).unwrap(),
            (vec![-2.0], vec![1.0])
        );
        check_eigenpairs(&[0.0; 16], 4, 0.0);
        check_eigenpairs(&[3.0, 0.0, 0.0, 0.0, -2.0, 0.0, 0.0, 0.0, 3.0], 3, 0.0);
    }

    #[test]
    fn malformed_and_nonfinite_matrices_are_errors() {
        assert!(symmetric_eigen(vec![1.0], 2).is_err());
        assert!(symmetric_eigen(Vec::new(), usize::MAX).is_err());
        assert!(symmetric_eigen(vec![f64::NAN], 1).is_err());
        assert!(symmetric_eigen(vec![f64::INFINITY], 1).is_err());
        assert!(symmetric_eigen(vec![1.0, 2.0, 3.0, 4.0], 2).is_err());
        assert!(symmetric_eigen(vec![f64::MAX; 4], 2).is_err());
    }

    #[test]
    fn norm_avoids_square_overflow_and_underflow() {
        assert!((norm(&[3e250, 4e250]) / 5e250 - 1.0).abs() < 1e-15);
        assert!((norm(&[3e-250, 4e-250]) / 5e-250 - 1.0).abs() < 1e-15);
        assert_eq!(norm(&[]), 0.0);
        assert!(norm(&[f64::NAN]).is_nan());
    }
}
