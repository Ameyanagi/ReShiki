//! Distance-restrained endpoint construction and covalent connectivity.
//!
//! Covalent radii are Pyykkö and Atsumi, Chem. Eur. J. 15, 186 (2009),
//! as tabulated by the crate's model Hessian. The TS graph uses a 1.2 cutoff.
//! Staged harmonic distance restraints preserve rigid-motion invariance and
//! use the existing molecular minimizer (Wang and Song, J. Chem. Phys. 144,
//! 214108, 2016, and Banerjee et al., J. Phys. Chem. 89, 52, 1985).

use std::{collections::BTreeSet, ops::ControlFlow};

use crate::{ANGSTROM_PER_BOHR, Accuracy, EvalError, Potential, model_hessian::covalent_radius};

use super::{Minimum, TsError, minimize, validate_geometry};

const TS_BOND_FACTOR: f64 = 1.2;
const RESTRAINT_STIFFNESS: f64 = 0.5; // Eh/bohr², E = k (r - r_target)² / 2.
const STAGES: usize = 5;
const BREAK_DISTANCE: f64 = 3.0 / ANGSTROM_PER_BOHR; // bohr.

/// Return bonded pairs in index order. Two atoms bond at or below 1.2 times
/// their covalent-radius sum. Inputs have valid atomic numbers and 3N coordinates.
pub fn bonds(numbers: &[u8], positions: &[f64]) -> Vec<(usize, usize)> {
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
pub fn build_endpoint<P: Potential>(
    potential: &mut P,
    numbers: &[u8],
    start: &[f64],
    form: &[(usize, usize)],
    break_: &[(usize, usize)],
    mut on_step: impl FnMut(usize, f64) -> ControlFlow<()>,
) -> Result<(Minimum, bool), TsError> {
    validate_geometry(numbers, start)?;
    let form = canonical_pairs(form, numbers.len())?;
    let break_ = canonical_pairs(break_, numbers.len())?;
    if !form.is_disjoint(&break_) {
        return Err(TsError::Invalid(
            "a pair cannot be both formed and broken".into(),
        ));
    }
    let initial_graph: BTreeSet<_> = bonds(numbers, start).into_iter().collect();
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
    let final_graph: BTreeSet<_> = bonds(numbers, &endpoint.positions).into_iter().collect();
    let correct = final_graph.difference(&initial_graph).eq(&form)
        && initial_graph.difference(&final_graph).eq(&break_);
    Ok((endpoint, correct))
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
        let cutoff = TS_BOND_FACTOR * (covalent_radius(1) + covalent_radius(1));
        for (r, expected) in [(cutoff, vec![(0, 1)]), (cutoff + 1e-8, vec![])] {
            assert_eq!(bonds(&[1, 1], &[0.0, 0.0, 0.0, r, 0.0, 0.0]), expected);
        }
        assert_eq!(
            bonds(&[1, 1, 1], &[0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 1.0, 0.0, 0.0]),
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
                bonds(&[1, 1], &minimum.positions).len(),
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
}
