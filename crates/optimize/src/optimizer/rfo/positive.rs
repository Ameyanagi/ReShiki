//! The unique negative RFO root for a positive-definite physical Hessian.

use crate::linalg::{cholesky::Cholesky, dot, norm};

const KRYLOV_DIMENSION: usize = 8;
const MAX_NEWTON_ITERATIONS: usize = 24;
const MAX_CG_ITERATIONS: usize = 100;
const ROOT_TOLERANCE: f64 = 2e-14;
const STEP_TOLERANCE: f64 = 1e-10;

/// Banerjee et al., J. Phys. Chem. 89, 52 (1985) \[Banerjee1985PRFO\]: writing the negative
/// augmented eigenvalue as -t gives (H+tI)x=g, s=-x, g·x=t. With H>0,
/// f(t)=g·(H+tI)^-1g-t is decreasing, with f'=-1-||x||², and its unique
/// positive root lies below ||g||. This scalar equation avoids forming all
/// eigenvectors of the augmented matrix.
pub(super) fn step(hessian: &[f64], gradient: &[f64]) -> Option<Vec<f64>> {
    let n = gradient.len();
    if hessian.len() != n.checked_mul(n)? {
        return None;
    }
    let gradient_norm = norm(gradient);
    if !(gradient_norm.is_finite() && gradient_norm > 0.) {
        return None;
    }
    // A small Krylov model estimates only the shift. Its step need not be
    // accurate: the full Hessian is factored once, and all subsequent solves
    // and the final error check use the full matrix.
    let mut shift = estimate_shift(hessian, gradient, gradient_norm)?;
    let factor = Cholesky::new(hessian, n, shift, 0.)?;
    let mut x = gradient.to_vec();
    factor.solve_in_place(&mut x)?;
    let mut lower = 0.;
    let mut upper = gradient_norm;
    for _ in 0..MAX_NEWTON_ITERATIONS {
        let value = dot(gradient, &x) - shift;
        if !value.is_finite() {
            return None;
        }
        if value.abs() <= ROOT_TOLERANCE * shift {
            let residual: Vec<_> = gradient
                .iter()
                .zip(product(hessian, &x, shift))
                .map(|(g, ax)| g - ax)
                .collect();
            let length = norm(&x);
            // H+tI >= tI. Solving the tiny residual separately makes the
            // remaining error bound ||r-A delta||/t useful even when a
            // small force in a flat mode is hidden by ||r||/||g||.
            let tolerance = 0.01 * STEP_TOLERANCE * length * shift;
            let correction = solve(
                hessian,
                &residual,
                shift,
                &factor,
                &vec![0.; n],
                Tolerance::Absolute(tolerance),
            )?;
            let remainder: Vec<_> = residual
                .iter()
                .zip(product(hessian, &correction, shift))
                .map(|(r, ad)| r - ad)
                .collect();
            let remainder_bound = norm(&remainder) / shift;
            let solve_error = norm(&correction) + remainder_bound;
            // Since |f'|>=1, |f(t)| bounds the error in the root. The
            // resolvent identity then bounds the change in the step at
            // that root; this checks displacement accuracy, not just the
            // backward residual of the augmented eigenvector.
            let root_error =
                (value + dot(gradient, &correction)).abs() + gradient_norm * remainder_bound;
            let minimum_shift = shift - root_error;
            let step_error = solve_error + root_error / minimum_shift * (length + solve_error);
            if minimum_shift > 0. && step_error.is_finite() && step_error <= STEP_TOLERANCE * length
            {
                x.iter_mut().for_each(|v| *v = -*v);
                return Some(x);
            }
            for (xi, delta) in x.iter_mut().zip(&correction) {
                *xi += delta;
            }
            continue;
        }
        if value > 0. {
            lower = shift;
        } else {
            upper = shift;
        }
        let mut next = shift + value / (1. + dot(&x, &x));
        if !next.is_finite() || next <= lower || next >= upper {
            next = lower + 0.5 * (upper - lower);
        }
        if !(next.is_finite() && next > 0.) || next == shift {
            return None;
        }
        // Inexact Newton solves can be loose while the shift is moving.
        // Their tolerance tightens quadratically with the scalar residual.
        let tolerance = (0.005 * (value / shift).powi(2)).clamp(1e-14, 1e-4);
        x = solve(
            hessian,
            gradient,
            next,
            &factor,
            &x,
            Tolerance::Relative(tolerance),
        )?;
        shift = next;
    }
    None
}

fn product(hessian: &[f64], x: &[f64], shift: f64) -> Vec<f64> {
    hessian
        .chunks_exact(x.len())
        .zip(x)
        .map(|(row, xi)| dot(row, x) + shift * xi)
        .collect()
}

// Golub & Van Loan, Matrix Computations [Golub2013Matrix]: symmetric Krylov projection with
// reorthogonalization. Eight products suffice for a useful shift even when
// they would be far from sufficient for an accurate lowest eigenvector.
fn estimate_shift(hessian: &[f64], gradient: &[f64], gradient_norm: f64) -> Option<f64> {
    let n = gradient.len();
    let dimension = KRYLOV_DIMENSION.min(n);
    let mut basis = vec![
        gradient
            .iter()
            .map(|g| g / gradient_norm)
            .collect::<Vec<_>>(),
    ];
    let mut images = Vec::with_capacity(dimension);
    for k in 0..dimension {
        let image = product(hessian, &basis[k], 0.);
        let image_norm = norm(&image);
        let mut trial = image.clone();
        images.push(image);
        if k + 1 == dimension {
            break;
        }
        for _ in 0..2 {
            for q in &basis {
                let overlap = dot(q, &trial);
                for (x, qi) in trial.iter_mut().zip(q) {
                    *x -= overlap * qi;
                }
            }
        }
        let length = norm(&trial);
        if !length.is_finite() {
            return None;
        }
        if length <= 32. * f64::EPSILON * image_norm {
            break;
        }
        trial.iter_mut().for_each(|x| *x /= length);
        basis.push(trial);
    }
    let m = basis.len();
    let mut projected = vec![0.; m * m];
    for i in 0..m {
        for j in 0..=i {
            let value = dot(&basis[i], &images[j]);
            projected[i * m + j] = value;
            projected[j * m + i] = value;
        }
    }
    let mut g = vec![0.; m];
    g[0] = gradient_norm;
    let step = super::dense_step(&projected, &g)?;
    let shift = -dot(&g, &step);
    (shift.is_finite() && shift > 0.).then_some(shift.min(gradient_norm))
}

enum Tolerance {
    Relative(f64),
    Absolute(f64),
}

// Diagonally shifted systems reuse one Cholesky factor at the estimated
// root. Preconditioned conjugate gradients therefore normally need only a
// few iterations. A failed or inaccurate solve falls back to the dense RFO.
fn solve(
    hessian: &[f64],
    rhs: &[f64],
    shift: f64,
    factor: &Cholesky,
    initial: &[f64],
    tolerance: Tolerance,
) -> Option<Vec<f64>> {
    let n = rhs.len();
    let rhs_norm = norm(rhs);
    let mut x = initial.to_vec();
    let mut residual: Vec<_> = rhs
        .iter()
        .zip(product(hessian, &x, shift))
        .map(|(r, ax)| r - ax)
        .collect();
    let mut z = residual.clone();
    factor.solve_in_place(&mut z)?;
    let mut direction = z.clone();
    let mut rz = dot(&residual, &z);
    for _ in 0..=n.min(MAX_CG_ITERATIONS) {
        let converged = match tolerance {
            Tolerance::Relative(tolerance) => {
                norm(&z) <= tolerance * norm(&x) && norm(&residual) <= tolerance * rhs_norm
            }
            Tolerance::Absolute(tolerance) => norm(&residual) <= tolerance,
        };
        if converged {
            return Some(x);
        }
        let image = product(hessian, &direction, shift);
        let curvature = dot(&direction, &image);
        if !(curvature.is_finite() && curvature > 0. && rz.is_finite() && rz > 0.) {
            return None;
        }
        let alpha = rz / curvature;
        for i in 0..n {
            x[i] += alpha * direction[i];
            residual[i] -= alpha * image[i];
        }
        z.copy_from_slice(&residual);
        factor.solve_in_place(&mut z)?;
        let next_rz = dot(&residual, &z);
        let beta = next_rz / rz;
        for (p, zi) in direction.iter_mut().zip(&z) {
            *p = zi + beta * *p;
        }
        rz = next_rz;
    }
    None
}

#[cfg(test)]
mod tests;
