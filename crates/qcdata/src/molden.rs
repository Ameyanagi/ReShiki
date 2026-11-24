//! Conversion between Molden and canonical atomic-orbital order and normalization.

use serde::{Deserialize, Serialize};

use crate::{
    BasisSet, Error, Shell,
    basis::{canonical_cartesian_order, odd_double_factorial},
};

/// The normalization used by a Molden producer for Cartesian atomic orbitals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CartesianNorm {
    /// Each Cartesian atomic orbital has unit self-overlap.
    Unit,
    /// All Cartesian components share the normalization of the axial component.
    Axial,
}

/// Returns the Cartesian powers of x, y, z in Molden order for an s through g shell.
pub fn molden_cartesian_order(l: u8) -> Result<&'static [[u8; 3]], Error> {
    match l {
        0 => Ok(&[[0, 0, 0]]),
        1 => Ok(&[[1, 0, 0], [0, 1, 0], [0, 0, 1]]),
        2 => Ok(&[
            [2, 0, 0],
            [0, 2, 0],
            [0, 0, 2],
            [1, 1, 0],
            [1, 0, 1],
            [0, 1, 1],
        ]),
        3 => Ok(&[
            [3, 0, 0],
            [0, 3, 0],
            [0, 0, 3],
            [1, 2, 0],
            [2, 1, 0],
            [2, 0, 1],
            [1, 0, 2],
            [0, 1, 2],
            [0, 2, 1],
            [1, 1, 1],
        ]),
        4 => Ok(&[
            [4, 0, 0],
            [0, 4, 0],
            [0, 0, 4],
            [3, 1, 0],
            [3, 0, 1],
            [1, 3, 0],
            [0, 3, 1],
            [1, 0, 3],
            [0, 1, 3],
            [2, 2, 0],
            [2, 0, 2],
            [0, 2, 2],
            [2, 1, 1],
            [1, 2, 1],
            [1, 1, 2],
        ]),
        _ => Err(Error::Unsupported(format!(
            "Molden does not support l = {l}"
        ))),
    }
}

/// Returns the signed m indices in Molden order for a pure d, f, or g shell.
pub fn molden_pure_order(l: u8) -> Result<Vec<i8>, Error> {
    if !(2..=4).contains(&l) {
        return Err(Error::Unsupported(format!(
            "Molden does not support a pure shell with l = {l}"
        )));
    }
    Ok(std::iter::once(0)
        .chain((1..=l as i8).flat_map(|m| [m, -m]))
        .collect())
}

/// Returns the dimensionless squared norm of xᵃyᵇzᶜ relative to the axial component.
pub fn axial_norm_ratio(a: u8, b: u8, c: u8) -> f64 {
    let (a, b, c) = (u16::from(a), u16::from(b), u16::from(c));
    odd_double_factorial(a) * odd_double_factorial(b) * odd_double_factorial(c)
        / odd_double_factorial(a + b + c)
}

/// Converts one shell's dimensionless MO coefficients from Molden to canonical order and norm.
pub fn shell_from_molden(
    shell: &Shell,
    coefficients: &[f64],
    norm: CartesianNorm,
) -> Result<Vec<f64>, Error> {
    convert_shell(shell, coefficients, norm, true)
}

/// Converts one shell's dimensionless MO coefficients from canonical to Molden order and norm.
pub fn shell_to_molden(
    shell: &Shell,
    coefficients: &[f64],
    norm: CartesianNorm,
) -> Result<Vec<f64>, Error> {
    convert_shell(shell, coefficients, norm, false)
}

/// Converts one full dimensionless MO row from Molden to canonical order and norm.
pub fn mo_from_molden(
    basis: &BasisSet,
    coefficients: &[f64],
    norm: CartesianNorm,
) -> Result<Vec<f64>, Error> {
    convert_mo(basis, coefficients, norm, true)
}

/// Converts one full dimensionless MO row from canonical to Molden order and norm.
pub fn mo_to_molden(
    basis: &BasisSet,
    coefficients: &[f64],
    norm: CartesianNorm,
) -> Result<Vec<f64>, Error> {
    convert_mo(basis, coefficients, norm, false)
}

fn convert_shell(
    shell: &Shell,
    coefficients: &[f64],
    norm: CartesianNorm,
    from_molden: bool,
) -> Result<Vec<f64>, Error> {
    if coefficients.len() != shell.n_functions() {
        return Err(Error::Invalid(
            "MO block length does not match the shell".into(),
        ));
    }
    let mut converted = vec![0.0; coefficients.len()];
    let mut assign = |molden: usize, canonical: usize, axial_to_unit: f64| {
        if from_molden {
            converted[canonical] = coefficients[molden] * axial_to_unit;
        } else {
            converted[molden] = coefficients[canonical] / axial_to_unit;
        }
    };
    if shell.pure {
        for (molden, m) in molden_pure_order(shell.l)?.into_iter().enumerate() {
            let canonical = (i16::from(m) + i16::from(shell.l)) as usize;
            assign(molden, canonical, 1.0);
        }
    } else {
        let order = molden_cartesian_order(shell.l)?;
        let canonical_order = canonical_cartesian_order(shell.l);
        for (molden, &[a, b, c]) in order.iter().enumerate() {
            let canonical = canonical_order
                .iter()
                .position(|&powers| powers == [a, b, c])
                .expect("Molden and canonical components are permutations");
            let axial_to_unit = match norm {
                CartesianNorm::Unit => 1.0,
                CartesianNorm::Axial => axial_norm_ratio(a, b, c).sqrt(),
            };
            assign(molden, canonical, axial_to_unit);
        }
    }
    if converted.iter().any(|value| !value.is_finite()) {
        return Err(Error::Invalid("MO coefficients must be finite".into()));
    }
    Ok(converted)
}

fn convert_mo(
    basis: &BasisSet,
    coefficients: &[f64],
    norm: CartesianNorm,
    from_molden: bool,
) -> Result<Vec<f64>, Error> {
    let mut representations: [Option<bool>; 5] = [None; 5];
    let mut n_ao = 0;
    for shell in &basis.shells {
        let l = usize::from(shell.l);
        let Some(representation) = representations.get_mut(l) else {
            return Err(Error::Unsupported(format!(
                "Molden does not support l = {l}"
            )));
        };
        if l >= 2
            && representation
                .replace(shell.pure)
                .is_some_and(|pure| pure != shell.pure)
        {
            return Err(Error::Unsupported(format!(
                "Molden cannot mix pure and Cartesian shells with l = {l}"
            )));
        }
        n_ao += shell.n_functions();
    }
    if coefficients.len() != n_ao {
        return Err(Error::Invalid(
            "MO row length does not match the basis".into(),
        ));
    }

    let mut converted = Vec::with_capacity(n_ao);
    for shell in &basis.shells {
        let start = converted.len();
        let end = start + shell.n_functions();
        converted.extend(convert_shell(
            shell,
            &coefficients[start..end],
            norm,
            from_molden,
        )?);
    }
    Ok(converted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basis::gaussian_moment;

    const CARTESIAN_LABELS: [&str; 5] = [
        "s",
        "x y z",
        "xx yy zz xy xz yz",
        "xxx yyy zzz xyy xxy xxz xzz yzz yyz xyz",
        "xxxx yyyy zzzz xxxy xxxz yyyx yyyz zzzx zzzy xxyy xxzz yyzz xxyz yyxz zzxy",
    ];

    fn powers(label: &str) -> [u8; 3] {
        (*b"xyz").map(|axis| label.bytes().filter(|&letter| letter == axis).count() as u8)
    }

    fn shell(l: u8, pure: bool) -> Shell {
        Shell {
            atom: 0,
            l,
            pure,
            exponents_per_bohr2: vec![0.5, 1.25],
            coefficients: vec![0.7, -0.2],
        }
    }

    #[test]
    fn cartesian_orders_match_molden_and_permute_canonical_components() {
        for (l, labels) in (0..=4).zip(CARTESIAN_LABELS) {
            let expected: Vec<_> = labels.split_whitespace().map(powers).collect();
            let molden = molden_cartesian_order(l).unwrap();
            assert_eq!(molden, expected);
            let mut molden_sorted = molden.to_vec();
            molden_sorted.sort_unstable();
            let mut canonical_sorted = canonical_cartesian_order(l);
            canonical_sorted.sort_unstable();
            assert_eq!(molden_sorted, canonical_sorted, "l={l}");
        }
    }

    #[test]
    fn pure_orders_match_molden() {
        assert_eq!(molden_pure_order(2).unwrap(), [0, 1, -1, 2, -2]);
        assert_eq!(molden_pure_order(3).unwrap(), [0, 1, -1, 2, -2, 3, -3]);
        assert_eq!(
            molden_pure_order(4).unwrap(),
            [0, 1, -1, 2, -2, 3, -3, 4, -4]
        );
    }

    #[test]
    fn axial_norm_ratios_match_quadrature() {
        let alpha = 1.3;
        for l in 2..=4 {
            let axial = gaussian_moment(l, alpha) * gaussian_moment(0, alpha).powi(2);
            for [a, b, c] in canonical_cartesian_order(l) {
                let component = gaussian_moment(a, alpha)
                    * gaussian_moment(b, alpha)
                    * gaussian_moment(c, alpha);
                assert!((axial_norm_ratio(a, b, c) - component / axial).abs() < 1e-12);
            }
        }
    }

    #[test]
    fn cartesian_blocks_land_at_the_same_canonical_component() {
        let canonical_labels = [
            "s",
            "x y z",
            "xx xy xz yy yz zz",
            "xxx xxy xxz xyy xyz xzz yyy yyz yzz zzz",
            "xxxx xxxy xxxz xxyy xxyz xxzz xyyy xyyz xyzz xzzz yyyy yyyz yyzz yzzz zzzz",
        ];
        for (l, (molden, canonical)) in
            (0..=4).zip(CARTESIAN_LABELS.into_iter().zip(canonical_labels))
        {
            let molden: Vec<_> = molden.split_whitespace().map(powers).collect();
            let input: Vec<_> = (1..=molden.len()).map(|value| value as f64).collect();
            let actual = shell_from_molden(&shell(l, false), &input, CartesianNorm::Unit).unwrap();
            for (value, label) in actual.into_iter().zip(canonical.split_whitespace()) {
                let index = molden
                    .iter()
                    .position(|&component| component == powers(label))
                    .unwrap();
                assert_eq!(value, input[index], "l={l}, {label}");
            }
        }
    }

    #[test]
    fn pure_blocks_land_at_the_same_canonical_m() {
        let expected: [&[f64]; 3] = [
            &[5.0, 3.0, 1.0, 2.0, 4.0],
            &[7.0, 5.0, 3.0, 1.0, 2.0, 4.0, 6.0],
            &[9.0, 7.0, 5.0, 3.0, 1.0, 2.0, 4.0, 6.0, 8.0],
        ];
        for (l, expected) in (2..=4).zip(expected) {
            let shell = shell(l, true);
            let input = (1..=shell.n_functions())
                .map(|value| value as f64)
                .collect::<Vec<_>>();
            assert_eq!(
                shell_from_molden(&shell, &input, CartesianNorm::Unit).unwrap(),
                expected
            );
        }
    }

    #[test]
    fn axial_d_xy_is_scaled_by_inverse_square_root_of_three() {
        let molden = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0];
        let expected = [0.0, 1.0 / 3.0_f64.sqrt(), 0.0, 0.0, 0.0, 0.0];
        let canonical = shell_from_molden(&shell(2, false), &molden, CartesianNorm::Axial).unwrap();
        for (&actual, expected) in canonical.iter().zip(expected) {
            assert!((actual - expected).abs() < 1e-15);
        }
    }

    #[test]
    fn shell_blocks_round_trip_for_both_normalizations() {
        for l in 0..=4 {
            for pure in [false, true] {
                if pure && l < 2 {
                    continue;
                }
                let shell = shell(l, pure);
                let input = (1..=shell.n_functions())
                    .map(|value| value as f64 * -0.3)
                    .collect::<Vec<_>>();
                for norm in [CartesianNorm::Unit, CartesianNorm::Axial] {
                    let canonical = shell_from_molden(&shell, &input, norm).unwrap();
                    let molden = shell_to_molden(&shell, &canonical, norm).unwrap();
                    for (actual, &expected) in molden.into_iter().zip(&input) {
                        assert!(
                            (actual - expected).abs() < 1e-14,
                            "l={l}, pure={pure}, {norm:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn mo_conversion_uses_shell_offsets_and_split_sp_order() {
        let basis = BasisSet {
            shells: vec![
                shell(0, false),
                Shell {
                    coefficients: vec![0.2, 0.8],
                    ..shell(0, false)
                },
                Shell {
                    coefficients: vec![0.5, 0.6],
                    ..shell(1, false)
                },
                shell(2, true),
                shell(3, false),
            ],
        };
        let molden = (1..=20).map(f64::from).collect::<Vec<_>>();
        let canonical = mo_from_molden(&basis, &molden, CartesianNorm::Unit).unwrap();
        assert_eq!(
            canonical,
            [
                1.0, 2.0, 3.0, 4.0, 5.0, 10.0, 8.0, 6.0, 7.0, 9.0, 11.0, 15.0, 16.0, 14.0, 20.0,
                17.0, 12.0, 19.0, 18.0, 13.0,
            ]
        );
        assert_eq!(
            mo_to_molden(&basis, &canonical, CartesianNorm::Unit).unwrap(),
            molden
        );
    }

    #[test]
    fn converters_reject_h_and_same_l_mixed_representations() {
        for pure in [false, true] {
            let h = shell(5, pure);
            let row = vec![0.0; h.n_functions()];
            assert!(shell_from_molden(&h, &row, CartesianNorm::Unit).is_err());
            assert!(shell_to_molden(&h, &row, CartesianNorm::Unit).is_err());
            let basis = BasisSet { shells: vec![h] };
            assert!(mo_from_molden(&basis, &row, CartesianNorm::Unit).is_err());
            assert!(mo_to_molden(&basis, &row, CartesianNorm::Unit).is_err());
        }
        for l in 2..=4 {
            let basis = BasisSet {
                shells: vec![shell(l, false), shell(l, true)],
            };
            let row = vec![0.0; basis.shells.iter().map(Shell::n_functions).sum()];
            assert!(mo_from_molden(&basis, &row, CartesianNorm::Unit).is_err());
            assert!(mo_to_molden(&basis, &row, CartesianNorm::Unit).is_err());
        }
    }
}
