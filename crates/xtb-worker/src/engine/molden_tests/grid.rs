//! Independent signed-grid evaluation of the AO contract in xtb-data.md, rules 5–8.

use reshiki_qcdata::BasisSet;
use std::f64::consts::PI;

type Polynomial = Vec<(f64, [u8; 3])>;

struct Ao {
    atom: usize,
    polynomial: Polynomial,
    // Exponent and the complete coefficient multiplying exp(-exponent * r²).
    primitives: Vec<(f64, f64)>,
}

/// All positions, including those in the parsed Molden reference, are in bohr.
pub(super) fn relative_errors(
    basis: &BasisSet,
    positions: &[[f64; 3]],
    rows: &[f64],
    molden: &super::Molden,
    nmo: usize,
) -> (usize, Vec<f64>) {
    let canonical = canonical_aos(basis);
    let nao = canonical.len();
    assert!(nmo > 0 && nmo <= molden.mos.len());
    assert_eq!(rows.len(), nao * molden.mos.len());
    assert_eq!(positions.len(), molden.positions.len());
    let points = grid(positions);
    let mut maxima = vec![(0.0_f64, 0.0_f64); nmo];
    for point in &points {
        let actual_aos = canonical_values(&canonical, positions, *point);
        let reference_aos = molden_cartesian_values(molden, *point);
        for (mo, (difference, scale)) in maxima.iter_mut().enumerate() {
            let actual = dot(&rows[mo * nao..(mo + 1) * nao], &actual_aos);
            let reference = dot(&molden.mos[mo].coefficients, &reference_aos);
            assert!(actual.is_finite() && reference.is_finite(), "MO {mo}");
            *difference = difference.max((actual - reference).abs());
            *scale = scale.max(reference.abs());
        }
    }
    let errors = maxima
        .into_iter()
        .enumerate()
        .map(|(mo, (difference, scale))| {
            assert!(scale > 0.0, "zero grid reference for MO {mo}");
            difference / scale
        })
        .collect();
    (points.len(), errors)
}

fn canonical_aos(basis: &BasisSet) -> Vec<Ao> {
    let mut aos = Vec::new();
    for shell in &basis.shells {
        assert_eq!(shell.exponents_per_bohr2.len(), shell.coefficients.len());
        // Rule 7: contraction coefficients multiply normalized axial primitives.
        let primitives = shell
            .exponents_per_bohr2
            .iter()
            .zip(&shell.coefficients)
            .map(|(&alpha, &coefficient)| {
                let axial = moment(2 * shell.l, 2.0 * alpha) * moment(0, 2.0 * alpha).powi(2);
                (alpha, coefficient / axial.sqrt())
            })
            .collect::<Vec<_>>();
        let polynomials = if shell.pure {
            harmonics(shell.l)
        } else {
            // Rule 5, deliberately independent of the production order helper.
            let mut polynomials = Vec::new();
            for x in (0..=shell.l).rev() {
                for y in (0..=shell.l - x).rev() {
                    polynomials.push(vec![(1.0, [x, y, shell.l - x - y])]);
                }
            }
            polynomials
        };
        for polynomial in polynomials {
            // Rule 8: integrate the actual contracted polynomial, including every
            // primitive pair and every cross term, rather than assuming unit scale.
            let mut overlap = 0.0;
            for &(alpha, a) in &primitives {
                for &(beta, b) in &primitives {
                    overlap += a * b * polynomial_overlap(&polynomial, alpha + beta);
                }
            }
            assert!(overlap.is_finite() && overlap > 0.0);
            let normalization = overlap.sqrt().recip();
            aos.push(Ao {
                atom: shell.atom as usize,
                polynomial,
                primitives: primitives
                    .iter()
                    .map(|&(alpha, coefficient)| (alpha, coefficient * normalization))
                    .collect(),
            });
        }
    }
    aos
}

fn molden_cartesian_values(molden: &super::Molden, point: [f64; 3]) -> Vec<f64> {
    let mut values = Vec::new();
    for shell in &molden.basis.shells {
        assert_eq!(shell.exponents_per_bohr2.len(), shell.coefficients.len());
        assert!(
            !shell.pure,
            "the independent reference must be Cartesian Molden"
        );
        // Molden order from the format specification; no canonical permutation.
        let order: &[[u8; 3]] = match shell.l {
            0 => &[[0, 0, 0]],
            1 => &[[1, 0, 0], [0, 1, 0], [0, 0, 1]],
            2 => &[
                [2, 0, 0],
                [0, 2, 0],
                [0, 0, 2],
                [1, 1, 0],
                [1, 0, 1],
                [0, 1, 1],
            ],
            3 => &[
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
            ],
            l => panic!("unsupported reference angular momentum {l}"),
        };
        let center = molden.positions[shell.atom as usize];
        let [x, y, z] = std::array::from_fn(|i| point[i] - center[i]);
        let r2 = x * x + y * y + z * z;
        for &[a, b, c] in order {
            let monomial = x.powi(a.into()) * y.powi(b.into()) * z.powi(c.into());
            // The file's coefficients already normalize the contraction. Each
            // primitive multiplies its own unit Cartesian monomial.
            let radial = shell
                .exponents_per_bohr2
                .iter()
                .zip(&shell.coefficients)
                .map(|(&alpha, d)| {
                    let norm2 = [a, b, c]
                        .map(|p| moment(2 * p, 2. * alpha))
                        .iter()
                        .product::<f64>();
                    d * (-alpha * r2).exp() / norm2.sqrt()
                })
                .sum::<f64>();
            values.push(monomial * radial);
        }
    }
    values
}

// Rule 6, m = -l..=l, without the Condon–Shortley phase. Positive scale factors
// are immaterial because canonical_aos normalizes each complete contraction.
fn harmonics(l: u8) -> Vec<Polynomial> {
    match l {
        2 => vec![
            vec![(1.0, [1, 1, 0])],
            vec![(1.0, [0, 1, 1])],
            vec![(2.0, [0, 0, 2]), (-1.0, [2, 0, 0]), (-1.0, [0, 2, 0])],
            vec![(1.0, [1, 0, 1])],
            vec![(1.0, [2, 0, 0]), (-1.0, [0, 2, 0])],
        ],
        3 => vec![
            vec![(3.0, [2, 1, 0]), (-1.0, [0, 3, 0])],
            vec![(1.0, [1, 1, 1])],
            vec![(4.0, [0, 1, 2]), (-1.0, [2, 1, 0]), (-1.0, [0, 3, 0])],
            vec![(2.0, [0, 0, 3]), (-3.0, [2, 0, 1]), (-3.0, [0, 2, 1])],
            vec![(4.0, [1, 0, 2]), (-1.0, [3, 0, 0]), (-1.0, [1, 2, 0])],
            vec![(1.0, [2, 0, 1]), (-1.0, [0, 2, 1])],
            vec![(1.0, [3, 0, 0]), (-3.0, [1, 2, 0])],
        ],
        _ => panic!("unsupported pure angular momentum {l}"),
    }
}

fn canonical_values(aos: &[Ao], positions: &[[f64; 3]], point: [f64; 3]) -> Vec<f64> {
    aos.iter()
        .map(|ao| {
            let displacement =
                std::array::from_fn::<_, 3, _>(|axis| point[axis] - positions[ao.atom][axis]);
            let r2 = displacement.iter().map(|x| x * x).sum::<f64>();
            let angular = ao
                .polynomial
                .iter()
                .map(|(factor, powers)| {
                    factor
                        * (0..3)
                            .map(|axis| displacement[axis].powi(i32::from(powers[axis])))
                            .product::<f64>()
                })
                .sum::<f64>();
            let radial = ao
                .primitives
                .iter()
                .map(|(alpha, coefficient)| coefficient * (-alpha * r2).exp())
                .sum::<f64>();
            angular * radial
        })
        .collect()
}

fn dot(coefficients: &[f64], values: &[f64]) -> f64 {
    assert_eq!(coefficients.len(), values.len());
    coefficients.iter().zip(values).map(|(a, b)| a * b).sum()
}

fn polynomial_overlap(polynomial: &Polynomial, exponent: f64) -> f64 {
    polynomial
        .iter()
        .flat_map(|(a, p)| {
            polynomial.iter().map(move |(b, q)| {
                a * b
                    * (0..3)
                        .map(|axis| moment(p[axis] + q[axis], exponent))
                        .product::<f64>()
            })
        })
        .sum()
}

// Integral over the real line of x^power exp(-exponent * x²).
fn moment(power: u8, exponent: f64) -> f64 {
    if !power.is_multiple_of(2) {
        return 0.0;
    }
    (0..power / 2).fold((PI / exponent).sqrt(), |integral, k| {
        integral * f64::from(2 * k + 1) / (2.0 * exponent)
    })
}

fn grid(positions: &[[f64; 3]]) -> Vec<[f64; 3]> {
    const POINTS: usize = 60;
    assert!((1..=POINTS).contains(&positions.len()));
    let golden_angle = PI * (3. - 5_f64.sqrt());
    (0..POINTS)
        .map(|i| {
            let center = positions[i % positions.len()];
            // Visit every center at 0.08 bohr, then sample out to 3 bohr. Cycle
            // latitudes and radii independently of atom order on the golden spiral.
            let fraction = |stride| ((i * stride % POINTS) as f64 + 0.5) / POINTS as f64;
            let radius = if i < positions.len() {
                0.08
            } else {
                0.2 + 2.8 * fraction(17)
            };
            let z = 2. * fraction(31) - 1.;
            let xy = (1. - z * z).sqrt();
            let (sin, cos) = ((i as f64 + 0.5) * golden_angle).sin_cos();
            let direction = [xy * cos, xy * sin, z];
            let point = std::array::from_fn(|axis| center[axis] + radius * direction[axis]);
            assert!(!positions.contains(&point));
            point
        })
        .collect()
}
