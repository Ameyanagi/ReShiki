//! Row-major A Bᵀ, optionally computing a symmetric result from its lower triangle.

pub(super) fn row_products(a: &[f64], b: &[f64], n: usize, symmetric: bool) -> Vec<f64> {
    // Reuse an eight-column panel of the transposed right operand across
    // four independent left rows. Contiguous lanes vectorize on ARM/x86.
    const COLS: usize = 8;
    let cols = b.len() / n;
    let rows = a.len() / n;
    let mut out = vec![0.; rows * cols];
    // Direct dots also preserve the existing operation order on tiny charts.
    if n < 32 {
        for (i, row) in a.chunks_exact(n).enumerate() {
            for (j, other) in
                b.chunks_exact(n)
                    .enumerate()
                    .take(if symmetric { i + 1 } else { cols })
            {
                let value = crate::linalg::dot(row, other);
                out[i * cols + j] = value;
                if symmetric {
                    out[j * cols + i] = value;
                }
            }
        }
        return out;
    }
    let mut panel = vec![[0.; COLS]; n];
    for j in (0..cols).step_by(COLS) {
        let w = (cols - j).min(COLS);
        for (k, v) in panel.iter_mut().enumerate() {
            for (lane, row) in v.iter_mut().zip(b[j * n..(j + w) * n].chunks_exact(n)) {
                *lane = row[k];
            }
        }
        let start = if symmetric { j } else { 0 };
        for i in (start..rows).step_by(4) {
            if i + 4 > rows {
                for p in i..rows {
                    for k in 0..w {
                        out[p * cols + j + k] = crate::linalg::dot(
                            &a[p * n..(p + 1) * n],
                            &b[(j + k) * n..(j + k + 1) * n],
                        );
                    }
                }
                break;
            }
            let (row0, rest) = a[i * n..].split_at(n);
            let (row1, rest) = rest.split_at(n);
            let (row2, rest) = rest.split_at(n);
            let row3 = &rest[..n];
            let mut s0 = [0.; COLS];
            let mut s1 = [0.; COLS];
            let mut s2 = [0.; COLS];
            let mut s3 = [0.; COLS];
            for k in 0..n {
                let p = &panel[k];
                for c in 0..COLS {
                    s0[c] = crate::linalg::multiply_add(row0[k], p[c], s0[c]);
                    s1[c] = crate::linalg::multiply_add(row1[k], p[c], s1[c]);
                    s2[c] = crate::linalg::multiply_add(row2[k], p[c], s2[c]);
                    s3[c] = crate::linalg::multiply_add(row3[k], p[c], s3[c]);
                }
            }
            for (r, sums) in [s0, s1, s2, s3].iter().enumerate() {
                out[(i + r) * cols + j..(i + r) * cols + j + w].copy_from_slice(&sums[..w]);
            }
        }
    }
    if symmetric {
        for i in 0..rows {
            for j in 0..i {
                out[j * cols + i] = out[i * cols + j]
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::row_products;
    use crate::linalg::dot;

    #[test]
    fn partial_panels_and_rectangular_products_match_direct_dots() {
        let width = 37;
        let a: Vec<_> = (0..11 * width).map(|i| (i % 17) as f64 - 6.).collect();
        let b: Vec<_> = (0..13 * width).map(|i| (i % 23) as f64 - 9.).collect();
        for (right, symmetric) in [(&a[..], true), (&b[..], false)] {
            let product = row_products(&a, right, width, symmetric);
            let expected: Vec<_> = a
                .chunks_exact(width)
                .flat_map(|row| right.chunks_exact(width).map(move |other| dot(row, other)))
                .collect();
            assert_eq!(product, expected);
        }
    }
}
