//! Conversion from axtb's spherical AOs to qcdata's unit-norm canonical AOs.
//!
//! The adapter only changes representation and normalization. It never changes
//! d/f phases: both contracts use the same real harmonics, in m = -l .. l order.

use crate::protocol::{ErrorKind, ResponseError};
use reshiki_qcdata::{BasisSet, Shell, contraction_overlap, primitive_norm};

/// Convert one engine shell, whose coefficients include axial primitive norms.
/// Returns the canonical shell and the original contraction's positive norm.
pub(crate) fn shell(
    atom: usize,
    l: usize,
    exponents: &[f64],
    coefficients: &[f64],
) -> Result<(Shell, f64), ResponseError> {
    if l > 3 {
        return Err(ResponseError::new(
            ErrorKind::Unsupported,
            format!("axtb orbital conversion does not support l = {l}"),
        ));
    }
    let mut shell = Shell {
        atom: atom as u32,
        l: l as u8,
        pure: l >= 2,
        exponents_per_bohr2: exponents.to_vec(),
        coefficients: exponents
            .iter()
            .zip(coefficients)
            .map(|(&alpha, &coefficient)| coefficient / primitive_norm(alpha, l as u8))
            .collect(),
    };
    if exponents.len() != coefficients.len() {
        return Err(invalid("axtb returned inconsistent shell primitives"));
    }
    shell
        .validate(atom + 1)
        .map_err(|error| invalid(error.to_string()))?;
    let norm = contraction_overlap(&shell).sqrt();
    for coefficient in &mut shell.coefficients {
        *coefficient /= norm;
    }
    Ok((shell, norm))
}

/// Select MO rows, reorder p from y,z,x to x,y,z, and scale each shell by its norm.
pub(crate) fn rows(
    basis: &BasisSet,
    norms: &[f64],
    coefficients: &[f64],
    selected: &[u32],
) -> Result<Vec<f64>, ResponseError> {
    let nao = basis.shells.iter().map(Shell::n_functions).sum::<usize>();
    if nao == 0 || norms.len() != basis.shells.len() || !coefficients.len().is_multiple_of(nao) {
        return Err(invalid("axtb returned inconsistent orbital coefficients"));
    }
    let mut converted = Vec::with_capacity(selected.len() * nao);
    for &row in selected {
        let offset = row as usize * nao;
        let source = coefficients
            .get(offset..offset + nao)
            .ok_or_else(|| invalid("axtb returned too few orbital rows"))?;
        let mut start = 0;
        for (shell, &norm) in basis.shells.iter().zip(norms) {
            let block = &source[start..start + shell.n_functions()];
            if shell.l == 1 {
                converted.extend([block[2] * norm, block[0] * norm, block[1] * norm]);
            } else {
                converted.extend(block.iter().map(|coefficient| coefficient * norm));
            }
            start += shell.n_functions();
        }
    }
    if converted.iter().any(|coefficient| !coefficient.is_finite()) {
        return Err(invalid("axtb returned non-finite orbital coefficients"));
    }
    Ok(converted)
}

fn invalid(message: impl Into<String>) -> ResponseError {
    ResponseError::new(ErrorKind::Numerical, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_normalized_s_primitives_have_the_expected_contraction_norm() {
        // For exponents 1/2 and 2, the normalized primitive overlap is (4/5)^(3/2).
        let axial = |a: f64| (2. * a / std::f64::consts::PI).powf(0.75);
        let (shell, norm) = shell(0, 0, &[0.5, 2.], &[2. * axial(0.5), -axial(2.)]).unwrap();
        let expected_norm = (5. - 4. * 0.8_f64.powf(1.5)).sqrt();
        assert!((norm - expected_norm).abs() < 1e-14);
        assert!((shell.coefficients[0] - 2. / expected_norm).abs() < 1e-14);
        assert!((shell.coefficients[1] + 1. / expected_norm).abs() < 1e-14);
        let basis = BasisSet {
            shells: vec![shell],
        };
        let mo = rows(&basis, &[norm], &[-0.7], &[0]).unwrap();
        assert!((mo[0] + 0.7 * expected_norm).abs() < 1e-14);
    }

    #[test]
    fn p_reorders_and_d_f_keep_their_m_order_and_phases() {
        let mut shells = Vec::new();
        let mut norms = Vec::new();
        for l in 0..=3 {
            let (shell, norm) = shell(0, l, &[0.75], &[primitive_norm(0.75, l as u8)]).unwrap();
            assert_eq!(shell.pure, l >= 2);
            shells.push(shell);
            norms.push(norm);
        }
        let basis = BasisSet { shells };
        let source = (1..=16).map(f64::from).collect::<Vec<_>>();
        let actual = rows(&basis, &norms, &source, &[0]).unwrap();
        let expected = [
            1., 4., 2., 3., 5., 6., 7., 8., 9., 10., 11., 12., 13., 14., 15., 16.,
        ];
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert!((actual - expected).abs() < 1e-13);
        }
    }

    #[test]
    fn contraction_and_mo_scaling_preserve_the_signed_radial_function() {
        for l in 0..=3 {
            let exponents = [0.4, 1.3];
            let input = [0.8, -0.17];
            let (shell, norm) = shell(0, l, &exponents, &input).unwrap();
            assert!((contraction_overlap(&shell) - 1.0).abs() < 1e-13);
            let basis = BasisSet {
                shells: vec![shell.clone()],
            };
            let nao = shell.n_functions();
            let original = vec![-0.7; 2 * nao];
            let converted = rows(&basis, &[norm], &original, &[1]).unwrap();
            for r2 in [0., 0.1, 0.75, 3.2] {
                let old_radial = exponents
                    .iter()
                    .zip(input)
                    .map(|(&a, c)| c * (-a * r2).exp())
                    .sum::<f64>();
                let new_radial = shell
                    .exponents_per_bohr2
                    .iter()
                    .zip(&shell.coefficients)
                    .map(|(&a, &d)| d * primitive_norm(a, l as u8) * (-a * r2).exp())
                    .sum::<f64>();
                for coefficient in &converted {
                    assert!(
                        (coefficient * new_radial - -0.7 * old_radial).abs() < 1e-13,
                        "l={l}, r²={r2}"
                    );
                }
            }
        }
    }

    #[test]
    fn unsupported_g_shell_and_invalid_engine_arrays_are_refused() {
        assert_eq!(
            shell(0, 4, &[1.], &[1.]).unwrap_err().kind,
            ErrorKind::Unsupported
        );
        assert!(shell(0, 0, &[1.], &[]).is_err());
        assert!(shell(0, 0, &[f64::NAN], &[1.]).is_err());
        assert!(shell(0, 0, &[1.], &[0.]).is_err());
    }
}
