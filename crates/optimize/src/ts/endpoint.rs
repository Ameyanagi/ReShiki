//! Distance-restrained endpoint construction and covalent connectivity.
//!
//! Covalent radii are Pyykkö and Atsumi, Chem. Eur. J. 15, 186 (2009) [Pyykko2009Radii],
//! as tabulated by the crate's model Hessian. The TS graph uses a 1.2 cutoff.
//! Staged harmonic distance restraints preserve rigid-motion invariance and
//! use the existing molecular minimizer (Wang and Song, J. Chem. Phys. 144,
//! 214108, 2016 [Wang2016TRIC], and Banerjee et al., J. Phys. Chem. 89, 52,
//! 1985 [Banerjee1985PRFO]).

use std::{collections::BTreeSet, ops::ControlFlow};

use crate::{
    ANGSTROM_PER_BOHR, Accuracy, EvalError, Potential,
    linalg::{dot, norm, symmetric_eigen},
    model_hessian::covalent_radius,
};

use super::{Minimum, TsError, minimize, validate_geometry};

const TS_BOND_FACTOR: f64 = 1.2;
const RESTRAINT_STIFFNESS: f64 = 0.5; // Eh/bohr², E = k (r - r_target)² / 2.
const STAGES: usize = 5;
const BREAK_DISTANCE: f64 = 3.0 / ANGSTROM_PER_BOHR; // bohr.
const MIN_NUCLEAR_SEPARATION: f64 = 0.1; // bohr.
const LINEAR_TOLERANCE: f64 = 1e-3; // Maximum distance from the best-fit line, bohr.
const TRANSVERSE_SEED: f64 = 0.1; // Relative transverse displacement, bohr.

/// Return bonded pairs in index order. Two atoms bond at or below 1.2 times
/// their covalent-radius sum. Reject invalid atomic numbers, a coordinate count
/// other than 3N, and nonfinite coordinates. Empty and single-atom graphs are valid.
pub fn bonds(numbers: &[u8], positions: &[f64]) -> Result<Vec<(usize, usize)>, TsError> {
    if numbers.iter().any(|z| !(1..=118).contains(z)) {
        return Err(TsError::Invalid("atomic numbers must be in 1..=118".into()));
    }
    if numbers.len().checked_mul(3) != Some(positions.len())
        || !positions.iter().all(|v| v.is_finite())
    {
        return Err(TsError::Invalid(
            "expected three finite coordinates per atom".into(),
        ));
    }
    Ok(bonds_prevalidated(numbers, positions))
}

/// Internal graph construction after validating atomic numbers and coordinates.
pub(super) fn bonds_prevalidated(numbers: &[u8], positions: &[f64]) -> Vec<(usize, usize)> {
    let mut result = Vec::new();
    for a in 0..numbers.len() {
        for b in a + 1..numbers.len() {
            let cutoff =
                TS_BOND_FACTOR * (covalent_radius(numbers[a]) + covalent_radius(numbers[b]));
            if distance(positions, a, b) <= cutoff {
                result.push((a, b));
            }
        }
    }
    result
}

/// Reject colliding nuclei before a potential sees a molecular-search input.
/// The caller has already checked that coordinates are finite and have length 3N.
pub(super) fn validate_nuclei_separation(positions: &[f64]) -> Result<(), TsError> {
    for a in 0..positions.len() / 3 {
        for b in a + 1..positions.len() / 3 {
            if distance(positions, a, b) < MIN_NUCLEAR_SEPARATION {
                return Err(TsError::Invalid(format!(
                    "atoms {a} and {b} are separated by less than 0.1 bohr"
                )));
            }
        }
    }
    Ok(())
}

/// Build an endpoint with five linearly staged, distance-only restrained
/// minimizations, followed by an unrestrained minimization. Forming pairs have
/// a harmonic target of one covalent-radius sum. Breaking pairs have a
/// one-sided, flat-bottomed lower limit of 3 Å. Each spring is 0.5 Eh/bohr².
///
/// The boolean reports whether the newly present and newly absent bonds match
/// `form` and `break_` exactly. Pair order and duplicates have no effect.
/// A pair requested for both formation and breaking is invalid. Callback step
/// indices count continuously across all six minimizations, whose intermediate
/// energies include the active restraints.
///
/// A geometry within 1e-3 bohr of its best-fit line receives a deterministic
/// 0.1-bohr relative transverse displacement when an actual bond change is requested.
/// Distance restraints on an exactly collinear chain have only axial forces.
/// The bend lets atoms change order without colliding and preserves the geometric
/// center. Input nuclei must be separated by at least 0.1 bohr.
pub fn build_endpoint<P: Potential>(
    potential: &mut P,
    numbers: &[u8],
    start: &[f64],
    form: &[(usize, usize)],
    break_: &[(usize, usize)],
    mut on_step: impl FnMut(usize, f64) -> ControlFlow<()>,
) -> Result<(Minimum, bool), TsError> {
    validate_geometry(numbers, start)?;
    validate_nuclei_separation(start)?;
    let form = canonical_pairs(form, numbers.len())?;
    let break_ = canonical_pairs(break_, numbers.len())?;
    if !form.is_disjoint(&break_) {
        return Err(TsError::Invalid(
            "a pair cannot be both formed and broken".into(),
        ));
    }
    let initial_graph: BTreeSet<_> = bonds_prevalidated(numbers, start).into_iter().collect();
    let restraints: Vec<Restraint> = form
        .iter()
        .chain(&break_)
        .map(|&(a, b)| {
            let initial = distance(start, a, b);
            let breaking = break_.contains(&(a, b));
            Restraint {
                a,
                b,
                initial,
                target: if breaking {
                    BREAK_DISTANCE
                } else {
                    covalent_radius(numbers[a]) + covalent_radius(numbers[b])
                },
                breaking,
            }
        })
        .collect();
    let mut positions = start.to_vec();
    let changing_atom = form
        .difference(&initial_graph)
        .chain(initial_graph.intersection(&break_))
        .map(|&(a, _)| a)
        .min();
    if let Some(atom) = changing_atom {
        seed_linear_bend(&mut positions, atom)?;
    }
    let mut offset = 0;
    for stage in 1..=STAGES {
        let mut restrained = Restrained {
            potential,
            restraints: &restraints,
            fraction: stage as f64 / STAGES as f64,
        };
        let mut last_step = 0;
        positions = minimize(&mut restrained, numbers, &positions, |step, energy| {
            last_step = step;
            on_step(offset + step, energy)
        })?
        .positions;
        offset += last_step + 1;
    }
    let endpoint = minimize(potential, numbers, &positions, |step, energy| {
        on_step(offset + step, energy)
    })?;
    let final_graph: BTreeSet<_> = bonds(numbers, &endpoint.positions)?.into_iter().collect();
    let correct = final_graph.difference(&initial_graph).eq(&form)
        && initial_graph.difference(&final_graph).eq(&break_);
    Ok((endpoint, correct))
}

/// Fit the principal Cartesian axis and leave its invariant linear subspace.
/// For three or more atoms, shift one reacting atom relative to the rest and
/// subtract the mean displacement to preserve the center.
fn seed_linear_bend(positions: &mut [f64], changing_atom: usize) -> Result<(), TsError> {
    let atoms = positions.len() / 3;
    if atoms < 3 {
        return Ok(());
    }
    let center: [f64; 3] = std::array::from_fn(|k| {
        positions
            .as_chunks::<3>()
            .0
            .iter()
            .map(|p| p[k] / atoms as f64)
            .sum()
    });
    let offsets: Vec<[f64; 3]> = positions
        .as_chunks::<3>()
        .0
        .iter()
        .map(|p| std::array::from_fn(|k| p[k] - center[k]))
        .collect();
    let scale = offsets.iter().map(|p| norm(p)).fold(0.0_f64, f64::max);
    if !scale.is_finite() {
        return Err(TsError::Invalid(
            "endpoint coordinates exceed the line-fit range".into(),
        ));
    }
    if scale == 0.0 {
        return Ok(());
    }
    let mut covariance = vec![0.0; 9];
    for point in &offsets {
        let scaled: [f64; 3] = point.map(|v| v / scale);
        for i in 0..3 {
            for j in i..3 {
                covariance[3 * i + j] += scaled[i] * scaled[j];
            }
        }
    }
    for i in 0..3 {
        for j in 0..i {
            covariance[3 * i + j] = covariance[3 * j + i];
        }
    }
    let (_, vectors) = symmetric_eigen(covariance, 3).map_err(TsError::Failed)?;
    let line = &vectors[6..9];
    for point in &offsets {
        let projection = dot(point, line);
        let residual: [f64; 3] = std::array::from_fn(|k| point[k] - projection * line[k]);
        if norm(&residual) > LINEAR_TOLERANCE {
            return Ok(());
        }
    }
    // Choose the Cartesian axis least parallel to the fitted line. Projection
    // makes the result independent of the eigensolver's eigenvector sign.
    let axis = (0..3)
        .min_by(|&a, &b| line[a].abs().total_cmp(&line[b].abs()))
        .unwrap();
    let mut transverse: [f64; 3] =
        std::array::from_fn(|k| f64::from(k == axis) - line[k] * line[axis]);
    let length = norm(&transverse);
    transverse.iter_mut().for_each(|v| *v /= length);
    for (atom, point) in positions.as_chunks_mut::<3>().0.iter_mut().enumerate() {
        let displacement =
            TRANSVERSE_SEED * (f64::from(atom == changing_atom) - 1.0 / atoms as f64);
        for k in 0..3 {
            point[k] += displacement * transverse[k];
        }
    }
    Ok(())
}

fn canonical_pairs(
    pairs: &[(usize, usize)],
    atoms: usize,
) -> Result<BTreeSet<(usize, usize)>, TsError> {
    pairs
        .iter()
        .map(|&(a, b)| {
            if a >= atoms || b >= atoms || a == b {
                Err(TsError::Invalid(
                    "restrained pairs require two distinct valid atom indices".into(),
                ))
            } else {
                Ok((a.min(b), a.max(b)))
            }
        })
        .collect()
}

struct Restraint {
    a: usize,
    b: usize,
    initial: f64,
    target: f64,
    breaking: bool,
}

struct Restrained<'a, P> {
    potential: &'a mut P,
    restraints: &'a [Restraint],
    fraction: f64,
}

impl<P: Potential> Potential for Restrained<'_, P> {
    fn evaluate(
        &mut self,
        positions: &[f64],
        gradient: &mut [f64],
        accuracy: Accuracy,
    ) -> Result<f64, EvalError> {
        let mut energy = self.potential.evaluate(positions, gradient, accuracy)?;
        for restraint in self.restraints {
            let r = distance(positions, restraint.a, restraint.b);
            let target = restraint.initial + self.fraction * (restraint.target - restraint.initial);
            let delta = r - target;
            if restraint.breaking && delta >= 0.0 {
                continue;
            }
            if r == 0.0 {
                return Err(EvalError::Failed(
                    "a distance restraint cannot separate coincident atoms".into(),
                ));
            }
            energy += 0.5 * RESTRAINT_STIFFNESS * delta * delta;
            for k in 0..3 {
                let force = RESTRAINT_STIFFNESS * delta / r
                    * (positions[3 * restraint.a + k] - positions[3 * restraint.b + k]);
                gradient[3 * restraint.a + k] += force;
                gradient[3 * restraint.b + k] -= force;
            }
        }
        Ok(energy)
    }
}

fn distance(x: &[f64], a: usize, b: usize) -> f64 {
    (x[3 * a] - x[3 * b])
        .hypot(x[3 * a + 1] - x[3 * b + 1])
        .hypot(x[3 * a + 2] - x[3 * b + 2])
}

#[cfg(test)]
mod tests {
    use super::*;

    struct RadialDoubleWell;

    impl Potential for RadialDoubleWell {
        fn evaluate(
            &mut self,
            positions: &[f64],
            gradient: &mut [f64],
            _accuracy: Accuracy,
        ) -> Result<f64, EvalError> {
            let r = distance(positions, 0, 1);
            let q = (r - 1.2) * (r - 6.0);
            let derivative = 0.004 * q * (2.0 * r - 7.2);
            for k in 0..3 {
                let value = derivative * (positions[k] - positions[3 + k]) / r;
                gradient[k] = value;
                gradient[3 + k] = -value;
            }
            Ok(0.002 * q * q)
        }
    }

    struct Zero;

    impl Potential for Zero {
        fn evaluate(
            &mut self,
            _positions: &[f64],
            gradient: &mut [f64],
            _accuracy: Accuracy,
        ) -> Result<f64, EvalError> {
            gradient.fill(0.0);
            Ok(0.0)
        }
    }

    #[test]
    fn graph_uses_the_ts_cutoff_and_sorted_pairs() {
        assert_eq!(bonds(&[], &[]).unwrap(), []);
        assert_eq!(bonds(&[1], &[0.0; 3]).unwrap(), []);
        let cutoff = TS_BOND_FACTOR * (covalent_radius(1) + covalent_radius(1));
        for (r, expected) in [(cutoff, vec![(0, 1)]), (cutoff + 1e-8, vec![])] {
            assert_eq!(
                bonds(&[1, 1], &[0.0, 0.0, 0.0, r, 0.0, 0.0]).unwrap(),
                expected
            );
        }
        assert_eq!(
            bonds(&[1, 1, 1], &[0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 1.0, 0.0, 0.0]).unwrap(),
            [(0, 1), (0, 2), (1, 2)]
        );
    }

    #[test]
    fn endpoint_restraints_change_the_unrestrained_bond_graph() {
        for forming in [false, true] {
            let start = [0.0, 0.0, 0.0, if forming { 6.0 } else { 1.2 }, 0.0, 0.0];
            let (form, break_) = if forming {
                (&[(1, 0), (0, 1)][..], &[][..])
            } else {
                (&[][..], &[(0, 1)][..])
            };
            let mut steps = Vec::new();
            let (minimum, correct) = build_endpoint(
                &mut RadialDoubleWell,
                &[1, 1],
                &start,
                form,
                break_,
                |step, _| {
                    steps.push(step);
                    ControlFlow::Continue(())
                },
            )
            .unwrap();
            assert!(correct, "forming={forming}");
            assert_eq!(
                bonds(&[1, 1], &minimum.positions).unwrap().len(),
                usize::from(forming)
            );
            assert!(minimum.energy < 1e-8, "energy={}", minimum.energy);
            assert!(steps.len() > STAGES);
            assert!(steps.windows(2).all(|pair| pair[0] < pair[1]));
        }
    }

    #[test]
    fn breaking_restraint_is_one_sided_and_has_the_correct_gradient() {
        let restraint = [Restraint {
            a: 0,
            b: 1,
            initial: 2.0,
            target: 4.0,
            breaking: true,
        }];
        let mut potential = Zero;
        let mut restrained = Restrained {
            potential: &mut potential,
            restraints: &restraint,
            fraction: 0.5,
        };
        for r in [2.0, 4.0] {
            let mut x = [0.0, 0.0, 0.0, r, 0.4, -0.2];
            let mut gradient = [0.0; 6];
            let energy = restrained
                .evaluate(&x, &mut gradient, Accuracy::Normal)
                .unwrap();
            for k in 0..6 {
                x[k] += 1e-5;
                let plus = restrained
                    .evaluate(&x, &mut [0.0; 6], Accuracy::Normal)
                    .unwrap();
                x[k] -= 2e-5;
                let minus = restrained
                    .evaluate(&x, &mut [0.0; 6], Accuracy::Normal)
                    .unwrap();
                x[k] += 1e-5;
                assert!((gradient[k] - (plus - minus) / 2e-5).abs() < 1e-10);
            }
            if r > 3.0 {
                assert_eq!(energy, 0.0);
                assert_eq!(gradient, [0.0; 6]);
            }
        }
    }

    #[test]
    fn endpoint_rejects_conflicting_pairs_and_honors_cancellation() {
        let x = [0.0, 0.0, 0.0, 1.2, 0.0, 0.0];
        for (form, break_) in [
            (&[(0, 0)][..], &[][..]),
            (&[][..], &[(0, 2)][..]),
            (&[(0, 1)][..], &[(1, 0)][..]),
        ] {
            assert!(matches!(
                build_endpoint(&mut Zero, &[1, 1], &x, form, break_, |_, _| {
                    ControlFlow::Continue(())
                }),
                Err(TsError::Invalid(_))
            ));
        }
        assert!(matches!(
            build_endpoint(&mut Zero, &[1, 1], &x, &[], &[(0, 1)], |_, _| {
                ControlFlow::Break(())
            }),
            Err(TsError::Cancelled)
        ));
    }

    #[test]
    fn forming_an_existing_bond_is_no_graph_change() {
        let x = [0., 0., 0., 1.2, 0., 0.];
        let (_, changed) = build_endpoint(&mut Zero, &[1, 1], &x, &[(0, 1)], &[], |_, _| {
            ControlFlow::Continue(())
        })
        .unwrap();
        assert!(!changed);
    }

    /// Rotation-invariant analytic HCN/HNC wells. The zero-energy minima have
    /// r_CN = 2.2, r_HC + r_HN = 6.2, and |r_HC - r_HN| = 2.2 bohr, so only
    /// the two linear atom orders minimize every term. A short-range core
    /// prevents a minimizer step from exchanging atoms through a collision.
    struct LinearIsomers;

    impl Potential for LinearIsomers {
        fn evaluate(
            &mut self,
            positions: &[f64],
            gradient: &mut [f64],
            _accuracy: Accuracy,
        ) -> Result<f64, EvalError> {
            let hc = distance(positions, 0, 1);
            let hn = distance(positions, 0, 2);
            let cn = distance(positions, 1, 2);
            let sum = hc + hn - 6.2;
            let difference = hc - hn;
            let order = difference * difference - 2.2_f64.powi(2);
            let mut energy = 0.005 * sum * sum + 0.0005 * order * order + 0.05 * (cn - 2.2).powi(2);
            let order_derivative = 0.002 * difference * order;
            gradient.fill(0.0);
            for (a, b, r, mut derivative) in [
                (0, 1, hc, 0.01 * sum + order_derivative),
                (0, 2, hn, 0.01 * sum - order_derivative),
                (1, 2, cn, 0.1 * (cn - 2.2)),
            ] {
                if r == 0.0 {
                    return Err(EvalError::Failed("coincident analytic nuclei".into()));
                }
                if r < 1.2 {
                    let core = 1.2 / r - 1.0;
                    energy += 0.1 * core.powi(4);
                    derivative -= 0.48 * core.powi(3) / (r * r);
                }
                for k in 0..3 {
                    let value = derivative * (positions[3 * a + k] - positions[3 * b + k]) / r;
                    gradient[3 * a + k] += value;
                    gradient[3 * b + k] -= value;
                }
            }
            Ok(energy)
        }
    }

    #[test]
    fn linear_hcn_reaches_unrestrained_hnc_graph() {
        // A rigid translation leaves the potential unchanged. Away from the
        // origin it also keeps roundoff-sized transverse motions from seeding a bend.
        for offset in [0.0, 10_000.0] {
            let start = [
                -2.0, offset, offset, 0.0, offset, offset, 2.2, offset, offset,
            ];
            let (endpoint, changed) = build_endpoint(
                &mut LinearIsomers,
                &[1, 6, 7],
                &start,
                &[(0, 2)],
                &[(0, 1)],
                |_, _| ControlFlow::Continue(()),
            )
            .unwrap();
            assert!(changed, "endpoint={:?}", endpoint.positions);
            // Check the physical minimum after all restraints have been removed.
            assert_eq!(
                bonds(&[1, 6, 7], &endpoint.positions).unwrap(),
                [(0, 2), (1, 2)]
            );
            assert!(distance(&endpoint.positions, 0, 2) < 2.1);
            assert!(distance(&endpoint.positions, 0, 1) > 4.0);
            assert!((distance(&endpoint.positions, 1, 2) - 2.2).abs() < 0.02);
            assert!(endpoint.energy < 1e-5, "energy={}", endpoint.energy);
        }
    }

    #[test]
    fn endpoint_rejects_coincident_nuclei_before_potential_evaluation() {
        struct MustNotEvaluate;
        impl Potential for MustNotEvaluate {
            fn evaluate(
                &mut self,
                _positions: &[f64],
                _gradient: &mut [f64],
                _accuracy: Accuracy,
            ) -> Result<f64, EvalError> {
                panic!("invalid nuclei must be rejected before the potential is called");
            }
        }
        for separation in [0.0, 0.099_999] {
            assert!(matches!(
                build_endpoint(
                    &mut MustNotEvaluate,
                    &[1, 1],
                    &[0.0, 0.0, 0.0, separation, 0.0, 0.0],
                    &[],
                    &[],
                    |_, _| ControlFlow::Continue(()),
                ),
                Err(TsError::Invalid(_))
            ));
        }
    }

    #[test]
    fn bonds_rejects_wrong_coordinate_count() {
        for positions in [&[0.0; 3][..], &[0.0; 7][..]] {
            assert!(matches!(
                bonds(&[1, 1], positions),
                Err(TsError::Invalid(_))
            ));
        }
    }

    #[test]
    fn bonds_rejects_invalid_atomic_numbers() {
        for number in [0, 119, 255] {
            assert!(matches!(
                bonds(&[number, 1], &[0.0; 6]),
                Err(TsError::Invalid(_))
            ));
        }
    }

    #[test]
    fn bonds_rejects_nonfinite_coordinates() {
        for coordinate in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(matches!(
                bonds(&[1, 1], &[0.0, 0.0, 0.0, coordinate, 0.0, 0.0]),
                Err(TsError::Invalid(_))
            ));
        }
    }

    #[test]
    fn transverse_seed_uses_the_best_line_and_preserves_the_center() {
        for (middle_bend, seeded) in [(1.2e-3, true), (3e-3, false)] {
            let direction = 3.0_f64.sqrt().recip();
            let transverse = 2.0_f64.sqrt().recip();
            let mut positions: Vec<f64> = [-2.0, 0.0, 2.0]
                .into_iter()
                .enumerate()
                .flat_map(|(atom, along)| {
                    let bend = if atom == 1 { middle_bend } else { 0.0 };
                    [
                        10.0 + along * direction + bend * transverse,
                        -4.0 + along * direction - bend * transverse,
                        7.0 + along * direction,
                    ]
                })
                .collect();
            let original = positions.clone();
            seed_linear_bend(&mut positions, 0).unwrap();
            assert_eq!(positions != original, seeded);
            let mut repeated = original.clone();
            seed_linear_bend(&mut repeated, 0).unwrap();
            assert_eq!(positions, repeated);
            for k in 0..3 {
                let before: f64 = original.as_chunks::<3>().0.iter().map(|p| p[k]).sum();
                let after: f64 = positions.as_chunks::<3>().0.iter().map(|p| p[k]).sum();
                assert!((before - after).abs() < 1e-13);
            }
        }
    }
}
