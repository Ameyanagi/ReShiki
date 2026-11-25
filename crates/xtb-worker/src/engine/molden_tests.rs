//! Signed grid checks against axtb's independent, public Molden writer.
//! This evaluator uses polynomial Gaussian moments, not the engine's AO transforms.

use super::*;
use reshiki_qcdata::{
    CartesianNorm, Shell, axial_norm_ratio, contraction_overlap, mo_to_molden,
    molden_cartesian_order, primitive_norm,
};

type Polynomial = Vec<(f64, [u8; 3])>;

#[test]
fn canonical_orbitals_match_cartesian_and_spherical_molden_references() {
    for (numbers, positions, charge) in [
        (
            vec![8, 1, 1],
            vec![
                [0., 0., 0.119262],
                [0., 0.763239, -0.477047],
                [0., -0.763239, -0.477047],
            ],
            0,
        ),
        // Cerium's q-vSZP basis includes f functions, even in this closed-shell ion.
        (vec![58], vec![[0., 0., 0.]], 4),
    ] {
        let open: Open = serde_json::from_value(serde_json::json!({
            "numbers": numbers, "positions": positions, "charge": charge,
            "units": "angstrom", "method": "gxtb", "uhf": 0,
            "scf": {"electronic_temperature_k": 1000.0},
            "heap_bytes": 2_147_483_648_u64
        }))
        .unwrap();
        let (mut session, _) = Session::open(open.clone(), open.heap_budget().unwrap()).unwrap();
        let actual = session
            .compute(Compute {
                orbitals: OrbitalOutput::All,
                ..Compute::default()
            })
            .unwrap()
            .orbitals
            .unwrap();
        let basis = actual.basis.unwrap();
        let nao = actual.alpha.energies_hartree.len();
        let source = Request {
            numbers: numbers.into_iter().map(|z| z as usize).collect(),
            positions: session.positions.clone(),
            charge: charge as f64,
            uhf: Some(0),
            method: Some("gxtb-v2.0.1".into()),
            restart: Some("previous".into()),
            etemp: Some(1000.),
            molden: true,
            ..Request::default()
        };
        for cartesian in [false, true] {
            let reference = axtb_driver::run(&Request {
                molden_cartesian: cartesian,
                ..source.clone()
            })
            .unwrap();
            let (reference_basis, reference_rows) =
                parse_molden(reference.molden.as_ref().unwrap(), !cartesian);
            assert_eq!(reference_rows.len(), nao);
            assert_eq!(reference_basis.shells.len(), basis.shells.len());
            for (a, b) in basis.shells.iter().zip(&reference_basis.shells) {
                assert_eq!((a.atom, a.l), (b.atom, b.l));
                for (&a, &b) in a
                    .exponents_per_bohr2
                    .iter()
                    .chain(&a.coefficients)
                    .zip(b.exponents_per_bohr2.iter().chain(&b.coefficients))
                {
                    assert!((a - b).abs() < 2e-12 * a.abs().max(1.));
                }
            }
            for (row, reference_row) in actual
                .alpha
                .coefficients
                .chunks_exact(nao)
                .zip(reference_rows)
            {
                if !cartesian {
                    let molden = mo_to_molden(&basis, row, CartesianNorm::Unit).unwrap();
                    for (actual, reference) in molden.into_iter().zip(reference_row) {
                        assert!(
                            (actual - reference).abs() < 2e-11,
                            "spherical Molden coefficient {actual} != {reference}"
                        );
                    }
                } else {
                    for point in [
                        [0.31, -0.47, 0.59],
                        [-1.13, 0.23, -0.77],
                        [0.07, 1.61, -0.38],
                        [2.2, -1.7, 0.6],
                    ] {
                        let expected = evaluate(
                            &reference_basis,
                            &reference_row,
                            &session.positions,
                            point,
                            true,
                        );
                        let actual = evaluate(&basis, row, &session.positions, point, false);
                        assert!(
                            (actual - expected).abs() < 2e-11 * expected.abs().max(1.),
                            "signed grid value {actual} != {expected}"
                        );
                    }
                }
            }
        }
    }
}

fn parse_molden(text: &str, spherical: bool) -> (BasisSet, Vec<Vec<f64>>) {
    let lines = text.lines().collect::<Vec<_>>();
    let mut index = lines.iter().position(|line| *line == "[GTO]").unwrap() + 1;
    let mut atom = 0;
    let mut shells = Vec::new();
    while !lines[index].starts_with('[') {
        let fields = lines[index].split_whitespace().collect::<Vec<_>>();
        index += 1;
        if fields.is_empty() {
            continue;
        }
        let l = match fields[0] {
            "s" => 0,
            "p" => 1,
            "d" => 2,
            "f" => 3,
            _ => {
                atom = fields[0].parse::<u32>().unwrap() - 1;
                continue;
            }
        };
        let n = fields[1].parse::<usize>().unwrap();
        let mut exponents = Vec::new();
        let mut coefficients = Vec::new();
        for line in &lines[index..index + n] {
            let mut fields = line.split_whitespace();
            exponents.push(fields.next().unwrap().parse().unwrap());
            coefficients.push(fields.next().unwrap().parse().unwrap());
        }
        index += n;
        shells.push(Shell {
            atom,
            l,
            pure: spherical && l >= 2,
            exponents_per_bohr2: exponents,
            coefficients,
        });
    }
    let start = lines.iter().position(|line| *line == "[MO]").unwrap() + 1;
    let mut rows: Vec<Vec<f64>> = Vec::new();
    for line in &lines[start..] {
        if line.starts_with("Sym=") {
            rows.push(Vec::new());
        } else {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            if fields.len() == 2 && fields[0].parse::<usize>().is_ok() {
                rows.last_mut().unwrap().push(fields[1].parse().unwrap());
            }
        }
    }
    (BasisSet { shells }, rows)
}

fn evaluate(
    basis: &BasisSet,
    row: &[f64],
    positions: &[[f64; 3]],
    point: [f64; 3],
    molden: bool,
) -> f64 {
    let mut offset = 0;
    let mut value = 0.;
    for shell in &basis.shells {
        let displacement = std::array::from_fn::<_, 3, _>(|axis| {
            point[axis] - positions[shell.atom as usize][axis]
        });
        let r2 = displacement.iter().map(|x| x * x).sum::<f64>();
        let radial = shell
            .exponents_per_bohr2
            .iter()
            .zip(&shell.coefficients)
            .map(|(&a, &d)| d * primitive_norm(a, shell.l) * (-a * r2).exp())
            .sum::<f64>()
            / contraction_overlap(shell).sqrt();
        let polynomials = if shell.pure {
            (-(shell.l as i8)..=shell.l as i8)
                .map(|m| harmonic(shell.l, m))
                .collect::<Vec<_>>()
        } else {
            let order = if molden {
                molden_cartesian_order(shell.l).unwrap().to_vec()
            } else {
                reshiki_qcdata::canonical_cartesian_order(shell.l)
            };
            order
                .into_iter()
                .map(|powers| {
                    vec![(
                        1. / axial_norm_ratio(powers[0], powers[1], powers[2]).sqrt(),
                        powers,
                    )]
                })
                .collect()
        };
        for polynomial in polynomials {
            let angular = polynomial
                .iter()
                .map(|(factor, powers)| {
                    factor
                        * displacement
                            .iter()
                            .zip(powers)
                            .map(|(x, &power)| x.powi(i32::from(power)))
                            .product::<f64>()
                })
                .sum::<f64>();
            value += row[offset] * radial * angular;
            offset += 1;
        }
    }
    assert_eq!(offset, row.len());
    value
}

fn harmonic(l: u8, m: i8) -> Polynomial {
    let terms: &[(f64, [u8; 3])] = match (l, m) {
        (2, -2) => &[(1., [1, 1, 0])],
        (2, -1) => &[(1., [0, 1, 1])],
        (2, 0) => &[(2., [0, 0, 2]), (-1., [2, 0, 0]), (-1., [0, 2, 0])],
        (2, 1) => &[(1., [1, 0, 1])],
        (2, 2) => &[(1., [2, 0, 0]), (-1., [0, 2, 0])],
        (3, -3) => &[(3., [2, 1, 0]), (-1., [0, 3, 0])],
        (3, -2) => &[(1., [1, 1, 1])],
        (3, -1) => &[(4., [0, 1, 2]), (-1., [2, 1, 0]), (-1., [0, 3, 0])],
        (3, 0) => &[(2., [0, 0, 3]), (-3., [2, 0, 1]), (-3., [0, 2, 1])],
        (3, 1) => &[(4., [1, 0, 2]), (-1., [3, 0, 0]), (-1., [1, 2, 0])],
        (3, 2) => &[(1., [2, 0, 1]), (-1., [0, 2, 1])],
        (3, 3) => &[(1., [3, 0, 0]), (-3., [1, 2, 0])],
        _ => unreachable!(),
    };
    let overlap = terms
        .iter()
        .flat_map(|(a, p)| {
            terms.iter().map(move |(b, q)| {
                a * b
                    * (0..3)
                        .map(|axis| moment(p[axis] + q[axis]))
                        .product::<f64>()
            })
        })
        .sum::<f64>();
    let axial = moment(2 * l) * moment(0).powi(2);
    terms
        .iter()
        .map(|(coefficient, powers)| (coefficient * (axial / overlap).sqrt(), *powers))
        .collect()
}

fn moment(power: u8) -> f64 {
    if !power.is_multiple_of(2) {
        return 0.;
    }
    (0..power / 2).fold(std::f64::consts::PI.sqrt(), |moment, k| {
        moment * f64::from(2 * k + 1) / 2.
    })
}
