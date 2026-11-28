use std::collections::VecDeque;

use super::*;
use crate::ts::{IrcBranch, IrcReason, SaddleReason};

const NUMBERS: [u8; 2] = [6, 8];

fn point(q: f64, kcal: f64) -> Minimum {
    Minimum {
        positions: vec![0., 0., 0., 1. + q, 0., 0.],
        energy: kcal / KCAL_PER_HARTREE,
    }
}

fn coordinate(point: &Minimum) -> f64 {
    point.positions[3] - point.positions[0] - 1.
}

fn candidate(
    reactant: Minimum,
    product: Minimum,
    path: Vec<Minimum>,
    ends: Option<[Minimum; 2]>,
) -> TsReport {
    let climbing_image = (1..path.len() - 1)
        .max_by(|&a, &b| path[a].energy.total_cmp(&path[b].energy))
        .unwrap();
    let ts = path[climbing_image].clone();
    let irc = ends.map(|ends| {
        ends.map(|end| IrcBranch {
            energies: vec![ts.energy, ts.energy - 0.001, end.energy],
            path: vec![Minimum {
                positions: end.positions.clone(),
                energy: ts.energy - 0.001,
            }],
            reason: IrcReason::Converged,
            gradient_calls: 1,
            path_length: 1.,
            end,
            // Deliberately unreliable global endpoint classification: the
            // planner must determine local identity from mapped geometries.
            reached: Reached::Other,
        })
    });
    TsReport {
        reactant,
        product,
        path,
        neb_converged: true,
        climbing_image,
        ts,
        saddle_converged: true,
        saddle_reason: SaddleReason::Stationary,
        saddle_diagnostics: Vec::new(),
        hessian: Vec::new(),
        frequencies_cm1: vec![-100.],
        imaginary: 1,
        transition_mode: vec![1.; 6],
        irc,
        irc_error: None,
        connects: false,
        evaluations: 0,
        hessians: 0,
    }
}

fn connected(a: &Minimum, b: &Minimum) -> TsReport {
    candidate(
        a.clone(),
        b.clone(),
        vec![
            a.clone(),
            point((coordinate(a) + coordinate(b)) / 2., 10.),
            b.clone(),
        ],
        Some([a.clone(), b.clone()]),
    )
}

#[derive(Default)]
struct Mock {
    searches: Vec<(f64, f64)>,
    relaxations: Vec<f64>,
    reports: VecDeque<TsReport>,
    rejected: Vec<f64>,
    collapse: Option<Minimum>,
    cancel_search: Option<usize>,
    cancel_relaxation: Option<usize>,
    cancel_started: bool,
    search_error: Option<TsError>,
    relaxation_error: Option<TsError>,
    normalizations: Vec<f64>,
    normalization_unresolved: bool,
}

impl Backend for Mock {
    type Verification = usize;

    fn canonicalize(&mut self, seed: &Minimum) -> Result<Option<Minimum>, TsError> {
        self.normalizations.push(coordinate(seed));
        Ok((!self.normalization_unresolved).then(|| seed.clone()))
    }

    fn relax(&mut self, seed: &Minimum) -> Result<Option<Minimum>, TsError> {
        self.relaxations.push(coordinate(seed));
        if let Some(error) = self.relaxation_error.take() {
            return Err(error);
        }
        if self.cancel_relaxation == Some(self.relaxations.len()) {
            return Err(TsError::Cancelled);
        }
        if self.rejected.contains(&coordinate(seed)) {
            return Ok(None);
        }
        Ok(Some(self.collapse.clone().unwrap_or_else(|| seed.clone())))
    }

    fn search(&mut self, a: &Minimum, b: &Minimum) -> Result<(TsReport, usize), TsError> {
        self.searches.push((coordinate(a), coordinate(b)));
        if let Some(error) = self.search_error.take() {
            return Err(error);
        }
        if self.cancel_search == Some(self.searches.len()) {
            return Err(TsError::Cancelled);
        }
        Ok((
            self.reports.pop_front().unwrap_or_else(|| connected(a, b)),
            self.searches.len(),
        ))
    }

    fn on_event(&mut self, event: Event) -> ControlFlow<()> {
        if self.cancel_started && event == Event::Started {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    }
}

#[test]
fn double_barrier_relaxes_valley_and_reuses_first_elementary_saddle() {
    let [a, middle, b] = [point(0., 0.), point(1., -1.), point(2., 0.)];
    let initial = candidate(
        a.clone(),
        b.clone(),
        vec![
            a.clone(),
            point(0.5, 8.),
            middle.clone(),
            point(1.5, 7.),
            b.clone(),
        ],
        Some([a.clone(), middle.clone()]),
    );
    let mut backend = Mock::default();
    let report = discover(&NUMBERS, (initial, 42), &Options::default(), &mut backend).unwrap();
    assert!(report.complete());
    assert_eq!(report.order, [0, 2, 1]);
    assert_eq!(report.edges.len(), 2);
    assert_eq!(backend.searches, [(1., 2.)]);
    assert_eq!(backend.relaxations, [1.]);
    let first = &report.attempts[report.edges[0].attempt];
    assert_eq!(first.verification, 42);
    assert_eq!(first.report.product, middle);
    assert_ne!(first.report.path.last(), Some(&first.report.product));
    assert!(first.report.connects);
}

#[test]
fn ordinary_connected_single_step_needs_no_additional_calculations() {
    let initial = connected(&point(0., 0.), &point(2., 0.));
    let mut backend = Mock::default();
    let report = discover(&NUMBERS, (initial, 7), &Options::default(), &mut backend).unwrap();
    assert!(report.complete());
    assert_eq!(report.edges.len(), 1);
    assert_eq!(report.order, [0, 1]);
    assert!(backend.relaxations.is_empty());
    assert!(backend.searches.is_empty());
    assert!(backend.normalizations.is_empty());
}

#[test]
fn failed_initial_saddle_still_supplies_a_valley_to_search() {
    let [a, b] = [point(0., 0.), point(2., 0.)];
    let mut initial = candidate(
        a.clone(),
        b.clone(),
        vec![a, point(0.5, 8.), point(1., 0.), point(1.5, 7.), b],
        None,
    );
    initial.saddle_converged = false;
    let mut backend = Mock::default();
    let report = discover(&NUMBERS, (initial, 0), &Options::default(), &mut backend).unwrap();
    assert!(report.complete());
    assert_eq!(report.edges.len(), 2);
    assert_eq!(report.attempts.len(), 3);
    assert_eq!(backend.searches, [(0., 1.), (1., 2.)]);
}

#[test]
fn later_segment_can_discover_another_valley_recursively() {
    let [a, middle, next, b] = [point(0., 0.), point(1., 0.), point(2., 0.), point(3., 0.)];
    let initial = candidate(
        a.clone(),
        b.clone(),
        vec![a.clone(), point(1.5, 10.), b.clone()],
        Some([a, middle.clone()]),
    );
    let recursive = candidate(
        middle.clone(),
        b.clone(),
        vec![
            middle.clone(),
            point(1.5, 8.),
            next.clone(),
            point(2.5, 6.),
            b,
        ],
        Some([next, middle]),
    );
    let mut backend = Mock {
        reports: VecDeque::from([recursive]),
        ..Mock::default()
    };
    let report = discover(&NUMBERS, (initial, 0), &Options::default(), &mut backend).unwrap();
    assert!(report.complete());
    assert_eq!(report.order, [0, 2, 3, 1]);
    assert_eq!(report.edges.len(), 3);
    assert_eq!(backend.searches, [(1., 3.), (2., 3.)]);
}

#[test]
fn reversed_misclassified_irc_ends_are_rebound_to_the_local_edge() {
    let [a, middle, b] = [point(0., 0.), point(1., 0.), point(2., 0.)];
    let mut initial = candidate(
        a.clone(),
        b.clone(),
        vec![a.clone(), point(1., 10.), b],
        Some([middle, a]),
    );
    for branch in initial.irc.as_mut().unwrap() {
        branch.reached = Reached::Product;
    }
    let mut backend = Mock::default();
    let report = discover(&NUMBERS, (initial, 0), &Options::default(), &mut backend).unwrap();
    assert!(report.complete());
    let branches = report.attempts[0].report.irc.as_ref().unwrap();
    assert_eq!(branches[0].reached, Reached::Product);
    assert_eq!(branches[1].reached, Reached::Reactant);
    assert_eq!(backend.searches, [(1., 2.)]);
}

#[test]
fn two_unknown_irc_ends_preserve_the_edge_and_search_both_outer_gaps() {
    let [a, left, right, b] = [point(0., 0.), point(1., 0.), point(2., 0.), point(3., 0.)];
    let initial = candidate(
        a.clone(),
        b.clone(),
        vec![a, point(1.5, 10.), b],
        Some([right, left]),
    );
    let mut backend = Mock::default();
    let report = discover(&NUMBERS, (initial, 5), &Options::default(), &mut backend).unwrap();
    assert!(report.complete());
    assert_eq!(
        report
            .order
            .iter()
            .map(|&id| coordinate(&report.minima[id]))
            .collect::<Vec<_>>(),
        [0., 1., 2., 3.]
    );
    assert_eq!(report.edges.len(), 3);
    assert_eq!(backend.searches, [(0., 1.), (2., 3.)]);
    assert_eq!(report.edges[1].attempt, 0);
}

#[test]
fn same_basin_irc_does_not_create_an_edge_or_repeat_a_search() {
    let [a, b] = [point(0., 0.), point(2., 0.)];
    let initial = candidate(
        a.clone(),
        b.clone(),
        vec![a.clone(), point(1., 10.), b],
        Some([a.clone(), a]),
    );
    let mut backend = Mock::default();
    let report = discover(&NUMBERS, (initial, 0), &Options::default(), &mut backend).unwrap();
    assert!(!report.complete());
    assert!(report.edges.is_empty());
    assert_eq!(report.gaps.len(), 1);
    assert!(report.gaps[0].reason.contains("same minimum"));
    assert!(backend.searches.is_empty());
}

#[test]
fn valley_collapsing_to_an_endpoint_is_not_an_intermediate() {
    let [a, b] = [point(0., 0.), point(2., 0.)];
    let initial = candidate(
        a.clone(),
        b.clone(),
        vec![a.clone(), point(0.5, 8.), point(1., 0.), point(1.5, 8.), b],
        None,
    );
    let mut backend = Mock {
        collapse: Some(a),
        ..Mock::default()
    };
    let report = discover(&NUMBERS, (initial, 0), &Options::default(), &mut backend).unwrap();
    assert_eq!(report.minima.len(), 2);
    assert_eq!(report.order, [0, 1]);
    assert_eq!(report.gaps.len(), 1);
    assert!(backend.searches.is_empty());
}

#[test]
fn graph_agreement_does_not_merge_distinct_mapped_geometries() {
    let a = point(0., 0.);
    let b = point(0.5, 0.);
    assert_eq!(
        endpoint::bonds_prevalidated(&NUMBERS, &a.positions),
        endpoint::bonds_prevalidated(&NUMBERS, &b.positions)
    );
    assert!(!same_minimum(&NUMBERS, &a, &b));
    let mut moved = a.clone();
    for atom in moved.positions.chunks_mut(3) {
        atom[0] += 10.;
        atom[1] -= 3.;
    }
    assert!(same_minimum(&NUMBERS, &a, &moved));
}

#[test]
fn close_degenerate_same_graph_intermediate_stays_distinct_after_normalization() {
    let [a, middle, b] = [point(0., 0.), point(0.02, 0.), point(0.04, 0.)];
    let initial = candidate(
        a.clone(),
        b.clone(),
        vec![
            a.clone(),
            point(0.01, 8.),
            middle.clone(),
            point(0.03, 7.),
            b,
        ],
        Some([a, middle]),
    );
    let mut backend = Mock::default();
    let report = discover(&NUMBERS, (initial, 0), &Options::default(), &mut backend).unwrap();
    assert!(report.complete());
    assert_eq!(report.minima.len(), 3);
    assert_eq!(report.order, [0, 2, 1]);
    assert_eq!(report.edges.len(), 2);
    assert!(backend.normalizations.len() >= 3);
}

#[test]
fn flat_shallow_and_monotonic_profiles_do_not_create_valleys() {
    for energies in [
        vec![0., 0., 0., 0., 0.],
        vec![0., 0.1, 0., 0.2, 0.],
        vec![4., 3., 2., 3., 4.],
        vec![0., 1., 2., 3., 4.],
    ] {
        let path: Vec<_> = energies
            .into_iter()
            .enumerate()
            .map(|(i, e)| point(i as f64, e))
            .collect();
        assert!(valley_candidates(&path, 0.5).is_empty());
    }
    let plateau: Vec<_> = [0., 5., 0., 0., 0., 5., 0.]
        .into_iter()
        .enumerate()
        .map(|(i, e)| point(i as f64, e))
        .collect();
    assert_eq!(valley_candidates(&plateau, 0.5).len(), 1);
    let noisy: Vec<_> = [0., 5., 0.1, 0.2, 0., 0.1, 0.05, 5., 0.]
        .into_iter()
        .enumerate()
        .map(|(i, e)| point(i as f64, e))
        .collect();
    assert_eq!(valley_candidates(&noisy, 0.5), [4]);
}

#[test]
fn search_limit_preserves_verified_edge_and_explicit_outer_gaps() {
    let [a, left, right, b] = [point(0., 0.), point(1., 0.), point(2., 0.), point(3., 0.)];
    let initial = candidate(
        a.clone(),
        b.clone(),
        vec![a, point(1.5, 10.), b],
        Some([left, right]),
    );
    let mut backend = Mock::default();
    let options = Options {
        max_searches: 1,
        ..Options::default()
    };
    let report = discover(&NUMBERS, (initial, 0), &options, &mut backend).unwrap();
    assert_eq!(report.edges.len(), 1);
    assert_eq!(report.gaps.len(), 2);
    assert!(report.gaps.iter().all(|gap| gap.reason.contains("limit")));
    assert!(backend.searches.is_empty());
}

#[test]
fn fatal_backend_error_keeps_the_verified_edge_and_stops_other_gaps() {
    let [a, left, right, b] = [point(0., 0.), point(1., 0.), point(2., 0.), point(3., 0.)];
    let initial = candidate(
        a.clone(),
        b.clone(),
        vec![a, point(1.5, 10.), b],
        Some([left, right]),
    );
    for error in [
        TsError::Failed("worker disconnected".into()),
        TsError::Invalid("bad backend input".into()),
    ] {
        let mut backend = Mock {
            search_error: Some(error.clone()),
            ..Mock::default()
        };
        let report = discover(
            &NUMBERS,
            (initial.clone(), 0),
            &Options::default(),
            &mut backend,
        )
        .unwrap();
        assert_eq!(report.error, Some(error));
        assert!(!report.cancelled);
        assert_eq!(report.edges.len(), 1);
        assert_eq!(report.gaps.len(), 2);
        assert_eq!(backend.searches.len(), 1);
        assert!(report.gaps[1].reason.contains("stopped"));
    }
}

#[test]
fn local_scf_failure_leaves_one_gap_and_allows_the_other_gap_to_finish() {
    let [a, left, right, b] = [point(0., 0.), point(1., 0.), point(2., 0.), point(3., 0.)];
    let initial = candidate(
        a.clone(),
        b.clone(),
        vec![a, point(1.5, 10.), b],
        Some([left, right]),
    );
    let mut backend = Mock {
        search_error: Some(TsError::Scf("local geometry".into())),
        ..Mock::default()
    };
    let report = discover(&NUMBERS, (initial, 0), &Options::default(), &mut backend).unwrap();
    assert_eq!(report.error, None);
    assert_eq!(report.edges.len(), 2);
    assert_eq!(report.gaps.len(), 1);
    assert!(report.gaps[0].reason.contains("SCF"));
    assert_eq!(backend.searches.len(), 2);
}

#[test]
fn fatal_relaxation_error_retains_the_attempt_without_more_calculations() {
    let [a, b] = [point(0., 0.), point(2., 0.)];
    let initial = candidate(
        a.clone(),
        b.clone(),
        vec![
            a.clone(),
            point(0.5, 8.),
            point(1., 0.),
            point(1.5, 7.),
            b.clone(),
        ],
        Some([a, b]),
    );
    let mut backend = Mock {
        relaxation_error: Some(TsError::Failed("worker disconnected".into())),
        ..Mock::default()
    };
    let report = discover(&NUMBERS, (initial, 99), &Options::default(), &mut backend).unwrap();
    assert!(report.error.is_some());
    assert_eq!(report.attempts.len(), 1);
    assert_eq!(report.attempts[0].verification, 99);
    assert_eq!(backend.relaxations.len(), 1);
    assert!(backend.searches.is_empty());
}

#[test]
fn cancellation_retains_the_initial_verified_edge_without_more_work() {
    let [a, middle, b] = [point(0., 0.), point(1., 0.), point(2., 0.)];
    let initial = candidate(
        a.clone(),
        b.clone(),
        vec![a.clone(), point(1., 10.), b],
        Some([a, middle]),
    );
    let mut backend = Mock {
        cancel_search: Some(1),
        ..Mock::default()
    };
    let report = discover(&NUMBERS, (initial, 99), &Options::default(), &mut backend).unwrap();
    assert!(report.cancelled);
    assert_eq!(report.edges.len(), 1);
    assert_eq!(report.attempts[0].verification, 99);
    assert_eq!(report.gaps.len(), 1);
    assert!(report.gaps[0].reason.contains("cancelled"));
    assert_eq!(backend.searches.len(), 1);
}

#[test]
fn cancellation_during_relaxation_or_at_start_keeps_the_attempt() {
    let [a, b] = [point(0., 0.), point(2., 0.)];
    let initial = candidate(
        a.clone(),
        b.clone(),
        vec![a, point(1., 10.), b],
        Some([point(0.5, 0.), point(1.5, 0.)]),
    );
    for mut backend in [
        Mock {
            cancel_started: true,
            ..Mock::default()
        },
        Mock {
            cancel_relaxation: Some(1),
            ..Mock::default()
        },
    ] {
        let report = discover(
            &NUMBERS,
            (initial.clone(), 9),
            &Options::default(),
            &mut backend,
        )
        .unwrap();
        assert!(report.cancelled);
        assert_eq!(report.attempts.len(), 1);
        assert_eq!(report.gaps.len(), 1);
        assert!(backend.searches.is_empty());
        assert!(backend.relaxations.len() <= 1);
    }
}

#[test]
fn empty_irc_launches_cannot_certify_an_elementary_edge() {
    let [a, b] = [point(0., 0.), point(2., 0.)];
    let mut initial = connected(&a, &b);
    initial.irc.as_mut().unwrap()[0].path.clear();
    initial.connects = true;
    let report = discover(
        &NUMBERS,
        (initial, 0),
        &Options::default(),
        &mut Mock::default(),
    )
    .unwrap();
    assert!(report.edges.is_empty());
    assert_eq!(report.gaps.len(), 1);
}

/// A rigid-motion-invariant molecular distance potential with three minima at
/// r=1,2,3 bohr and two saddles at r=2±1/√3. Both barriers are 4k/27 Eh.
#[derive(Clone, Copy)]
struct TripleWell;

impl crate::Potential for TripleWell {
    fn evaluate(
        &mut self,
        x: &[f64],
        g: &mut [f64],
        _: crate::Accuracy,
    ) -> Result<f64, crate::EvalError> {
        let axis: [f64; 3] = std::array::from_fn(|i| x[i + 3] - x[i]);
        let r = axis.iter().map(|v| v * v).sum::<f64>().sqrt();
        let q = r - 2.;
        let derivative = 0.02 * (6. * q.powi(5) - 8. * q.powi(3) + 2. * q);
        for i in 0..3 {
            g[i + 3] = derivative * axis[i] / r;
            g[i] = -g[i + 3];
        }
        Ok(0.02 * q * q * (q * q - 1.).powi(2))
    }

    fn hessian(&mut self, x: &[f64], h: &mut [f64]) -> Option<Result<(), crate::EvalError>> {
        let axis: [f64; 3] = std::array::from_fn(|i| x[i + 3] - x[i]);
        let r = axis.iter().map(|v| v * v).sum::<f64>().sqrt();
        let q = r - 2.;
        let first = 0.02 * (6. * q.powi(5) - 8. * q.powi(3) + 2. * q);
        let second = 0.02 * (30. * q.powi(4) - 24. * q * q + 2.);
        h.fill(0.);
        for i in 0..3 {
            for j in 0..3 {
                let value = (second - first / r) * axis[i] * axis[j] / (r * r)
                    + if i == j { first / r } else { 0. };
                h[6 * i + j] = value;
                h[6 * (i + 3) + j + 3] = value;
                h[6 * i + j + 3] = -value;
                h[6 * (i + 3) + j] = -value;
            }
        }
        Some(Ok(()))
    }
}

struct NumericalBackend<P> {
    potential: P,
    options: crate::ts::TsOptions,
    searches: usize,
    relaxations: usize,
}

impl<P: crate::Potential + Send> Backend for NumericalBackend<P> {
    type Verification = ();

    fn canonicalize(&mut self, seed: &Minimum) -> Result<Option<Minimum>, TsError> {
        let outcome = crate::optimize(
            &mut self.potential,
            &[6; 2],
            &seed.positions,
            &crate::Options {
                level: crate::Level::Tight,
                convergence: crate::Convergence::EnergyGradient {
                    gradient_norm: 1e-10,
                },
                initial_hessian: crate::InitialHessian::Potential,
                max_steps: 100,
                ..crate::Options::default()
            },
            |_| ControlFlow::Continue(()),
        );
        match outcome.status {
            crate::Status::Converged => Ok(Some(Minimum {
                positions: outcome.positions,
                energy: outcome.energy,
            })),
            crate::Status::MaxSteps => Ok(None),
            crate::Status::ScfFailure(reason) => Err(TsError::Scf(reason)),
            crate::Status::Failed(reason) => Err(TsError::Failed(reason)),
            crate::Status::Cancelled => Err(TsError::Cancelled),
        }
    }

    fn relax(&mut self, seed: &Minimum) -> Result<Option<Minimum>, TsError> {
        self.relaxations += 1;
        let minimum =
            crate::ts::minimize(&mut self.potential, &[6; 2], &seed.positions, |_, _| {
                ControlFlow::Continue(())
            })?;
        let mut hessian = vec![0.; 36];
        self.potential
            .hessian(&minimum.positions, &mut hessian)
            .unwrap()?;
        let (frequencies, _) =
            crate::ts::modes::frequencies(&minimum.positions, &hessian, &[12.; 2])?;
        Ok(frequencies
            .iter()
            .all(|&f| f >= -crate::ts::IMAGINARY_CM1)
            .then_some(minimum))
    }

    fn search(&mut self, a: &Minimum, b: &Minimum) -> Result<(TsReport, ()), TsError> {
        self.searches += 1;
        crate::ts::search(
            std::slice::from_mut(&mut self.potential),
            &crate::ts::Search {
                numbers: &[6; 2],
                masses_amu: &[12.; 2],
                reactant: &a.positions,
                product: &b.positions,
                options: &self.options,
            },
            |_| ControlFlow::Continue(()),
        )
        .map(|report| (report, ()))
    }
}

#[test]
fn numerical_neb_minimization_saddles_and_ircs_recover_both_triple_well_barriers() {
    let a = point(0., 0.);
    let b = point(2., 0.);
    let mut backend = NumericalBackend {
        potential: TripleWell,
        options: crate::ts::TsOptions {
            optimize_endpoints: false,
            preserve_endpoint_identity: true,
            neb_max_iterations: 60,
            ..crate::ts::TsOptions::default()
        },
        searches: 0,
        relaxations: 0,
    };
    let initial = backend.search(&a, &b).unwrap();
    assert!(valley_candidates(&initial.0.path, 0.5).len() == 1);
    let report = discover(&[6; 2], initial, &Options::default(), &mut backend).unwrap();
    assert!(
        report.complete(),
        "gaps={:?}; diagnostics={:?}",
        report.gaps,
        report.diagnostics
    );
    assert!(!report.cancelled);
    assert_eq!(report.minima.len(), 3);
    assert_eq!(report.edges.len(), 2);
    assert!(backend.searches <= 3);
    assert!(backend.relaxations >= 1);
    let midpoint = &report.minima[report.order[1]];
    assert!((coordinate(midpoint) - 1.).abs() < 0.005);
    let expected = 0.02 * 4. / 27. * KCAL_PER_HARTREE;
    for edge in &report.edges {
        let candidate = &report.attempts[edge.attempt].report;
        assert!(candidate.saddle_converged && candidate.connects);
        assert_eq!(candidate.imaginary, 1);
        assert!(
            candidate
                .irc
                .as_ref()
                .unwrap()
                .iter()
                .all(|b| !b.path.is_empty())
        );
        assert!((candidate.forward_barrier_kcal() - expected).abs() < 0.01);
        assert!((candidate.reverse_barrier_kcal() - expected).abs() < 0.01);
    }
}

#[derive(Clone, Copy)]
struct SoftDoubleWell {
    stiffness: f64,
    epsilon: f64,
}

impl SoftDoubleWell {
    fn radial(&self, radius: f64) -> (f64, f64, f64) {
        let q = radius - 2.;
        let t = q * q - 1.;
        let a = 4. * t.powi(3) + 2. * self.epsilon * t;
        let b = 12. * t * t + 2. * self.epsilon;
        (
            self.stiffness * (t.powi(4) + self.epsilon * t * t),
            self.stiffness * a * 2. * q,
            self.stiffness * (b * 4. * q * q + a * 2.),
        )
    }
}

impl crate::Potential for SoftDoubleWell {
    fn evaluate(
        &mut self,
        x: &[f64],
        g: &mut [f64],
        _: crate::Accuracy,
    ) -> Result<f64, crate::EvalError> {
        let axis: [f64; 3] = std::array::from_fn(|i| x[i + 3] - x[i]);
        let radius = axis.iter().map(|v| v * v).sum::<f64>().sqrt();
        let (energy, first, _) = self.radial(radius);
        for i in 0..3 {
            g[i + 3] = first * axis[i] / radius;
            g[i] = -g[i + 3];
        }
        Ok(energy)
    }

    fn hessian(&mut self, x: &[f64], h: &mut [f64]) -> Option<Result<(), crate::EvalError>> {
        let axis: [f64; 3] = std::array::from_fn(|i| x[i + 3] - x[i]);
        let radius = axis.iter().map(|v| v * v).sum::<f64>().sqrt();
        let (_, first, second) = self.radial(radius);
        h.fill(0.);
        for i in 0..3 {
            for j in 0..3 {
                let value = (second - first / radius) * axis[i] * axis[j] / radius.powi(2)
                    + if i == j { first / radius } else { 0. };
                h[6 * i + j] = value;
                h[6 * (i + 3) + j + 3] = value;
                h[6 * i + j + 3] = -value;
                h[6 * (i + 3) + j] = -value;
            }
        }
        Some(Ok(()))
    }
}

#[test]
fn numerical_soft_single_step_does_not_invent_intermediates_from_force_convergence() {
    for epsilon in [1e-4, 1.] {
        for stiffness in [0.02, 0.002, 0.0002] {
            let a = point(0., 0.);
            let b = point(2., 0.);
            let mut backend = NumericalBackend {
                potential: SoftDoubleWell { stiffness, epsilon },
                options: crate::ts::TsOptions {
                    optimize_endpoints: false,
                    preserve_endpoint_identity: true,
                    neb_max_iterations: 60,
                    ..crate::ts::TsOptions::default()
                },
                searches: 0,
                relaxations: 0,
            };
            let initial = backend.search(&a, &b).unwrap();
            let report = discover(&[6; 2], initial, &Options::default(), &mut backend).unwrap();
            assert!(
                report.complete(),
                "k={stiffness}, epsilon={epsilon}: gaps={:?}, diagnostics={:?}",
                report.gaps,
                report.diagnostics
            );
            assert_eq!(report.order, [0, 1]);
            assert_eq!(report.minima.len(), 2);
            assert_eq!(report.edges.len(), 1);
            assert_eq!(backend.searches, 1);
        }
    }
}

#[test]
fn an_irc_endpoint_cannot_be_reassigned_to_a_different_basin_after_relaxation() {
    let [a, b] = [point(0., 0.), point(2., 0.)];
    let initial = candidate(
        a.clone(),
        b.clone(),
        vec![a.clone(), point(1., 10.), b],
        Some([a, point(0.5, 0.)]),
    );
    let mut backend = Mock {
        collapse: Some(point(1.5, 0.)),
        ..Mock::default()
    };
    let report = discover(&NUMBERS, (initial, 0), &Options::default(), &mut backend).unwrap();
    assert!(!report.complete());
    assert!(report.edges.is_empty());
    assert_eq!(report.minima.len(), 2);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|text| text.contains("changed basin"))
    );
    assert!(backend.searches.is_empty());
}

#[test]
fn undecided_soft_geometry_is_a_gap_and_not_a_certified_intermediate() {
    let [a, b] = [point(0., 0.), point(2., 0.)];
    let initial = candidate(
        a.clone(),
        b.clone(),
        vec![a.clone(), point(1., 10.), b],
        Some([a, point(0.5, 0.)]),
    );
    let mut backend = Mock {
        normalization_unresolved: true,
        ..Mock::default()
    };
    let report = discover(&NUMBERS, (initial, 0), &Options::default(), &mut backend).unwrap();
    assert!(!report.complete());
    assert_eq!(report.minima.len(), 2);
    assert!(report.edges.is_empty());
    assert_eq!(report.gaps.len(), 1);
    assert!(backend.searches.is_empty());
}
