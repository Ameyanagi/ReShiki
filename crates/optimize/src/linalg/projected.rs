//! Symmetric eigendecomposition on the complement of known orthonormal axes.

use super::{dot, norm, symmetric_eigen};

fn reflect(v: &mut [f64], u: &[f64]) {
    let alpha = 2. * dot(v, u);
    for (x, &ui) in v.iter_mut().zip(u) {
        *x -= alpha * ui;
    }
}

/// Diagonalize P H P on range(P), where P removes `orthonormal` axes.
/// Returned eigenvectors occupy rows in the original Cartesian coordinates.
/// Additional null directions of H remain in this physical block.
pub(crate) fn projected_eigen(
    mut h: Vec<f64>,
    n: usize,
    orthonormal: &[Vec<f64>],
) -> Result<(Vec<f64>, Vec<f64>), String> {
    if n.checked_mul(n) != Some(h.len())
        || orthonormal.len() > n
        || orthonormal.iter().any(|axis| axis.len() != n)
    {
        return Err("invalid projected eigenproblem dimensions".into());
    }
    if orthonormal.is_empty() {
        return symmetric_eigen(h, n);
    }
    // Q is a product of reflectors taking the orthonormal rigid vectors
    // onto the first coordinate axes. The trailing block of Q^T H Q is
    // exactly P H P restricted to range(P), even when H has extra nulls.
    let mut axes = orthonormal.to_vec();
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
            .chunks_exact(n)
            .skip(k)
            .map(|row| dot(&row[k..], &u))
            .collect();
        let uhu = dot(&u, &w);
        for (wi, ui) in w.iter_mut().zip(&u) {
            *wi = 2. * (*wi - uhu * ui);
        }
        for (i, row) in h.chunks_exact_mut(n).skip(k).enumerate() {
            for (j, x) in row[k..].iter_mut().enumerate() {
                *x -= u[i] * w[j] + w[i] * u[j];
            }
        }
        reflectors.push(u);
    }
    let rank = reflectors.len();
    let dim = n - rank;
    if dim == 0 {
        return Ok((Vec::new(), Vec::new()));
    }
    let block = h
        .chunks_exact(n)
        .skip(rank)
        .flat_map(|r| r[rank..].iter().copied())
        .collect();
    let (values, vectors) = symmetric_eigen(block, dim)?;
    let mut modes = vec![0.; dim * n];
    for (i, mode) in modes.chunks_exact_mut(n).enumerate() {
        mode[rank..].copy_from_slice(&vectors[i * dim..(i + 1) * dim]);
        for (k, u) in reflectors.iter().enumerate().rev() {
            reflect(&mut mode[k..], u);
        }
    }
    Ok((values, modes))
}
