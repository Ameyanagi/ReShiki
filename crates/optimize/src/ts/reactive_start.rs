//! Graph-preserving preparation of a reactive starting conformer.

use std::ops::ControlFlow;

use crate::model_hessian::covalent_radius;
use crate::{ANGSTROM_PER_BOHR, Accuracy, EvalError, KCAL_PER_HARTREE, Options, Potential};

use super::endpoint::{
    DistanceLimit, Restrained, Restraint, bonds_prevalidated, canonical_pairs, distance,
    seed_linear_bend, validate_nuclei_separation,
};
use super::{TsError, evaluate, validate_geometry};

const TARGET_RADIUS_FACTOR: f64 = 2.0;
const STAGES: usize = 3;
const STIFFNESS: f64 = 0.02; // Eh/bohr², E = k max(r - target, 0)² / 2.
const MIN_DISTANCE_GAIN: f64 = 0.05 / ANGSTROM_PER_BOHR; // bohr.
const MAX_STEPS: usize = 80; // Per guided stage and for the released minimization.

/// An accepted, unrestrained reactive starting conformer. Distances describe
/// the same canonical, zero-based forming pairs before and after preparation.
#[derive(Debug, Clone, PartialEq)]
pub struct ReactiveStart {
    pub pairs: Vec<(usize, usize)>,
    pub before_angstrom: Vec<f64>,
    pub after_angstrom: Vec<f64>,
    /// Physical potential-energy change, excluding all guiding restraints.
    pub energy_change_kcal: f64,
}

impl ReactiveStart {
    /// Format the accepted change using caller-supplied atom labels, for example
    /// `C1···C6 4.72 → 3.05 Å (+2.1 kcal/mol)`.
    pub fn description(&self, label: impl Fn(usize) -> String) -> String {
        format!(
            "{} ({:+.1} kcal/mol)",
            self.distance_description(label),
            self.energy_change_kcal
        )
    }

    pub fn distance_description(&self, label: impl Fn(usize) -> String) -> String {
        self.pairs
            .iter()
            .zip(&self.before_angstrom)
            .zip(&self.after_angstrom)
            .map(|((&(a, b), before), after)| {
                format!("{}···{} {before:.2} → {after:.2} Å", label(a), label(b))
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Prepare a graph-preserving starting conformer before [`super::build_endpoint`].
/// Young et al. (2021) \[Young2021autodE\] motivate preparing a pre-reaction
/// conformer. Coordinates are in bohr and callback energies in hartree.
///
/// Forming pairs farther apart than twice their covalent-radius sum receive
/// three staged, one-sided distance restraints, with stiffness 0.02 Eh/bohr².
/// Each guided stage and the final release have an 80-step minimization budget.
/// The original covalent graph must survive every trial, including the final
/// minimization after all restraints have been removed. Breaking pairs are
/// validated but are not stretched during this preparation.
///
/// Accept only a converged unrestrained minimum that shortens every initially
/// long forming pair by more than 0.05 Å and leaves initially close forming pairs
/// within their target. A failed minimization, unsafe trial, or ineffective
/// preparation returns the input coordinates unchanged and `None`. All pairs
/// already within target return without any potential evaluations. Potential
/// errors and cancellation are propagated. Callback step indices are continuous;
/// energies during the guided stages include the active restraints.
pub fn prepare_reactive_start<P: Potential>(
    potential: &mut P,
    numbers: &[u8],
    start: &[f64],
    form: &[(usize, usize)],
    break_: &[(usize, usize)],
    on_step: impl FnMut(usize, f64) -> ControlFlow<()>,
) -> Result<(Vec<f64>, Option<ReactiveStart>), TsError> {
    validate_geometry(numbers, start)?;
    validate_nuclei_separation(start)?;
    let form = canonical_pairs(form, numbers.len())?;
    let break_ = canonical_pairs(break_, numbers.len())?;
    if !form.is_disjoint(&break_) {
        return Err(TsError::Invalid(
            "a pair cannot be both formed and broken".into(),
        ));
    }
    let (before, restraints): (Vec<_>, Vec<_>) = form
        .into_iter()
        .map(|(a, b)| {
            let initial = distance(start, a, b);
            let target =
                TARGET_RADIUS_FACTOR * (covalent_radius(numbers[a]) + covalent_radius(numbers[b]));
            (
                initial,
                Restraint {
                    a,
                    b,
                    initial: initial.max(target),
                    target,
                    limit: DistanceLimit::Upper,
                },
            )
        })
        .unzip();
    let changing_atom = restraints
        .iter()
        .zip(&before)
        .find_map(|(restraint, &before)| (before > restraint.target).then_some(restraint.a));
    let Some(changing_atom) = changing_atom else {
        return Ok((start.to_vec(), None));
    };

    let graph = bonds_prevalidated(numbers, start);
    let mut guarded = GraphPreserving {
        potential,
        numbers,
        graph,
        potential_error: false,
    };
    let (initial_energy, _) = evaluate(&mut guarded, start)?;
    let mut positions = start.to_vec();
    seed_linear_bend(&mut positions, changing_atom)?;
    let options = Options {
        max_steps: MAX_STEPS,
        ..Options::default()
    };
    let minimum = match Restrained::new(&mut guarded, &restraints, STIFFNESS)
        .relax_and_release(numbers, positions, STAGES, &options, on_step)
    {
        Ok(minimum) => minimum,
        Err(error @ TsError::Cancelled) => return Err(error),
        Err(error) if guarded.potential_error => return Err(error),
        Err(_) => return Ok((start.to_vec(), None)),
    };
    let after: Vec<_> = restraints
        .iter()
        .map(|restraint| distance(&minimum.positions, restraint.a, restraint.b))
        .collect();
    if !before
        .iter()
        .zip(&after)
        .zip(&restraints)
        .all(|((&before, &after), restraint)| {
            if before > restraint.target {
                before - after > MIN_DISTANCE_GAIN
            } else {
                after <= restraint.target
            }
        })
    {
        return Ok((start.to_vec(), None));
    }
    let report = ReactiveStart {
        pairs: restraints
            .iter()
            .map(|restraint| (restraint.a, restraint.b))
            .collect(),
        before_angstrom: before.iter().map(|r| r * ANGSTROM_PER_BOHR).collect(),
        after_angstrom: after.iter().map(|r| r * ANGSTROM_PER_BOHR).collect(),
        energy_change_kcal: (minimum.energy - initial_energy) * KCAL_PER_HARTREE,
    };
    Ok((minimum.positions, Some(report)))
}

/// Reject unsafe trials before the physical potential sees them. In particular,
/// a transient product graph cannot be used to reach a nominal reactant minimum.
struct GraphPreserving<'a, P> {
    potential: &'a mut P,
    numbers: &'a [u8],
    graph: Vec<(usize, usize)>,
    potential_error: bool,
}

impl<P: Potential> Potential for GraphPreserving<'_, P> {
    fn evaluate(
        &mut self,
        positions: &[f64],
        gradient: &mut [f64],
        accuracy: Accuracy,
    ) -> Result<f64, EvalError> {
        if !positions.iter().all(|r| r.is_finite())
            || validate_nuclei_separation(positions).is_err()
            || bonds_prevalidated(self.numbers, positions) != self.graph
        {
            return Err(EvalError::Failed(
                "reactive-start trial changed the molecular graph".into(),
            ));
        }
        self.potential
            .evaluate(positions, gradient, accuracy)
            .inspect_err(|_| {
                self.potential_error = true;
            })
    }
}

#[cfg(test)]
mod tests {
    use super::super::minimize;
    use super::*;

    struct MustNotEvaluate;

    impl Potential for MustNotEvaluate {
        fn evaluate(&mut self, _: &[f64], _: &mut [f64], _: Accuracy) -> Result<f64, EvalError> {
            panic!("the unchanged/invalid fast path must not evaluate the potential");
        }
    }

    #[derive(Clone, Copy)]
    enum Well {
        /// The two separated-carbon minima at 5.4 and 10 bohr share one graph.
        Double,
        Harmonic(f64),
    }

    struct PairWells {
        pairs: Vec<(usize, usize, Well)>,
        graph: Vec<(usize, usize)>,
        calls: usize,
        fail_at: Option<(usize, EvalError)>,
    }

    impl PairWells {
        fn one(well: Well) -> Self {
            Self {
                pairs: vec![(0, 1, well)],
                graph: vec![],
                calls: 0,
                fail_at: None,
            }
        }
    }

    impl Potential for PairWells {
        fn evaluate(
            &mut self,
            positions: &[f64],
            gradient: &mut [f64],
            _: Accuracy,
        ) -> Result<f64, EvalError> {
            self.calls += 1;
            if let Some((at, error)) = &self.fail_at
                && self.calls == *at
            {
                return Err(error.clone());
            }
            // Any changed-graph trial must be caught before this backend runs.
            assert_eq!(
                bonds_prevalidated(&vec![6; positions.len() / 3], positions),
                self.graph
            );
            gradient.fill(0.0);
            let mut energy = 0.0;
            for &(a, b, well) in &self.pairs {
                let r = distance(positions, a, b);
                let (e, derivative) = match well {
                    Well::Double => {
                        let q = (r - 5.4) * (r - 10.0);
                        (0.0005 * q * q, 0.001 * q * (2.0 * r - 15.4))
                    }
                    Well::Harmonic(minimum) => (0.01 * (r - minimum).powi(2), 0.02 * (r - minimum)),
                };
                energy += e;
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
    fn close_pairs_and_no_forming_pairs_are_unevaluated_fast_paths() {
        // The iodine contact is close on its own radius scale despite exceeding
        // a fixed 3 Å cutoff. H and C use their smaller element-specific scales.
        for (numbers, angstrom) in [([1, 1], 1.2), ([6, 6], 2.9), ([53, 53], 4.0)] {
            for form in [&[][..], &[(1, 0), (0, 1)][..]] {
                let start = [-0.0, 0.0, 0.0, angstrom / ANGSTROM_PER_BOHR, 0.0, 0.0];
                let (positions, report) = prepare_reactive_start(
                    &mut MustNotEvaluate,
                    &numbers,
                    &start,
                    form,
                    &[],
                    |_, _| panic!("a skipped preparation must not report minimizer progress"),
                )
                .unwrap();
                assert!(
                    positions
                        .iter()
                        .map(|x| x.to_bits())
                        .eq(start.map(f64::to_bits))
                );
                assert_eq!(report, None);
            }
        }
    }

    #[test]
    fn released_same_graph_minimum_has_physical_distances_and_energy() {
        // Atom 2 stays bonded to atom 0 while the unbonded 0···1 contact folds.
        let start = [0.0, 0.0, 0.0, 10.0, 0.0, 0.0, 0.0, 2.8, 0.0];
        let mut potential = PairWells {
            pairs: vec![(0, 1, Well::Double), (0, 2, Well::Harmonic(2.8))],
            graph: vec![(0, 2)],
            calls: 0,
            fail_at: None,
        };
        let mut steps = Vec::new();
        let (positions, report) = prepare_reactive_start(
            &mut potential,
            &[6; 3],
            &start,
            &[(1, 0), (0, 1)],
            &[],
            |step, _| {
                steps.push(step);
                ControlFlow::Continue(())
            },
        )
        .unwrap();
        let report = report.expect("the shorter physical conformer was discarded");
        assert_eq!(report.pairs, [(0, 1)]);
        assert!((distance(&positions, 0, 1) - 5.4).abs() < 0.03);
        assert!(distance(&positions, 0, 1) * ANGSTROM_PER_BOHR < 3.0);
        assert_eq!(bonds_prevalidated(&[6; 3], &positions), [(0, 2)]);
        let (physical_energy, gradient) = evaluate(&mut potential, &positions).unwrap();
        assert!(gradient.iter().all(|value| value.abs() < 4.5e-4));
        assert!(physical_energy < 1e-5);
        assert_eq!(report.before_angstrom, [10.0 * ANGSTROM_PER_BOHR]);
        assert_eq!(
            report.after_angstrom,
            [distance(&positions, 0, 1) * ANGSTROM_PER_BOHR]
        );
        assert!((report.energy_change_kcal - physical_energy * KCAL_PER_HARTREE).abs() < 1e-12);
        assert!(steps.len() > STAGES);
        assert!(steps.windows(2).all(|pair| pair[0] < pair[1]));
        // The fully released result stays in the shorter basin when minimized again.
        let released = minimize(&mut potential, &[6; 3], &positions, |_, _| {
            ControlFlow::Continue(())
        })
        .unwrap();
        assert!((distance(&released.positions, 0, 1) - 5.4).abs() < 0.03);
        assert_eq!(bonds_prevalidated(&[6; 3], &released.positions), [(0, 2)]);
    }

    #[test]
    fn release_to_original_basin_returns_bit_identical_input_without_report() {
        let start = [0.0, 0.0, 0.0, 10.0, 0.0, 0.0];
        let (positions, report) = prepare_reactive_start(
            &mut PairWells::one(Well::Harmonic(10.0)),
            &[6, 6],
            &start,
            &[(0, 1)],
            &[],
            |_, _| ControlFlow::Continue(()),
        )
        .unwrap();
        assert_eq!(positions, start);
        assert_eq!(report, None);
    }

    #[test]
    fn guided_graph_change_is_rejected_before_physical_evaluation() {
        let start = [0.0, 0.0, 0.0, 10.0, 0.0, 0.0];
        let mut potential = PairWells::one(Well::Harmonic(2.5));
        let (positions, report) =
            prepare_reactive_start(&mut potential, &[6, 6], &start, &[(0, 1)], &[], |_, _| {
                ControlFlow::Continue(())
            })
            .unwrap();
        assert!(potential.calls > 1);
        assert_eq!(positions, start);
        assert_eq!(report, None);
    }

    #[test]
    fn graph_guard_preserves_existing_bonds_and_rejects_collisions() {
        let mut potential = MustNotEvaluate;
        let mut guarded = GraphPreserving {
            potential: &mut potential,
            numbers: &[6, 6],
            graph: vec![(0, 1)],
            potential_error: false,
        };
        for r in [4.0, 0.01] {
            assert!(
                guarded
                    .evaluate(
                        &[0.0, 0.0, 0.0, r, 0.0, 0.0],
                        &mut [0.0; 6],
                        Accuracy::Normal
                    )
                    .is_err()
            );
        }
        assert!(!guarded.potential_error);
    }

    #[test]
    fn every_pair_is_reported_and_initially_close_pairs_remain_close() {
        let start = [
            0.0, 0.0, 0.0, 10.0, 0.0, 0.0, 0.0, 20.0, 0.0, 5.0, 20.0, 0.0,
        ];
        let mut potential = PairWells {
            pairs: vec![(0, 1, Well::Double), (2, 3, Well::Harmonic(5.0))],
            graph: vec![],
            calls: 0,
            fail_at: None,
        };
        let (positions, report) = prepare_reactive_start(
            &mut potential,
            &[6; 4],
            &start,
            &[(3, 2), (1, 0)],
            &[],
            |_, _| ControlFlow::Continue(()),
        )
        .unwrap();
        let report = report.expect("a close second pair must not prevent preparation");
        assert_eq!(report.pairs, [(0, 1), (2, 3)]);
        assert!(report.after_angstrom[0] < report.before_angstrom[0] - 0.05);
        assert!(distance(&positions, 2, 3) < 5.1);
        assert!(report.after_angstrom[1] < 3.0);
    }

    #[test]
    fn cancellation_and_backend_failures_are_not_hidden_as_unchanged() {
        let start = [0.0, 0.0, 0.0, 10.0, 0.0, 0.0];
        assert_eq!(
            prepare_reactive_start(
                &mut PairWells::one(Well::Double),
                &[6, 6],
                &start,
                &[(0, 1)],
                &[],
                |_, _| ControlFlow::Break(()),
            ),
            Err(TsError::Cancelled)
        );
        for error in [
            EvalError::Scf("analytic SCF".into()),
            EvalError::Failed("analytic backend".into()),
            EvalError::Cancelled,
        ] {
            let mut potential = PairWells::one(Well::Double);
            potential.fail_at = Some((2, error.clone()));
            assert_eq!(
                prepare_reactive_start(&mut potential, &[6, 6], &start, &[(0, 1)], &[], |_, _| {
                    ControlFlow::Continue(())
                },),
                Err(TsError::from(error))
            );
        }
    }

    #[test]
    fn invalid_inputs_are_rejected_without_evaluating() {
        for (start, form, break_) in [
            (&[0.0, 0.0, 0.0, 10.0, 0.0, 0.0][..], &[(0, 2)][..], &[][..]),
            (
                &[0.0, 0.0, 0.0, 10.0, 0.0, 0.0][..],
                &[(0, 1)][..],
                &[(1, 0)][..],
            ),
            (&[0.0; 6][..], &[(0, 1)][..], &[][..]),
        ] {
            assert!(matches!(
                prepare_reactive_start(
                    &mut MustNotEvaluate,
                    &[6, 6],
                    start,
                    form,
                    break_,
                    |_, _| ControlFlow::Continue(())
                ),
                Err(TsError::Invalid(_))
            ));
        }
    }

    #[test]
    fn report_description_keeps_pair_order_units_and_signed_energy() {
        let report = ReactiveStart {
            pairs: vec![(0, 5), (1, 4)],
            before_angstrom: vec![4.72, 4.11],
            after_angstrom: vec![3.05, 2.95],
            energy_change_kcal: 2.1,
        };
        assert_eq!(
            report.description(|index| format!("C{}", index + 1)),
            "C1···C6 4.72 → 3.05 Å, C2···C5 4.11 → 2.95 Å (+2.1 kcal/mol)"
        );
    }
}
