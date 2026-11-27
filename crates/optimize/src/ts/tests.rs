//! Analytic benchmarks for the elastic band, saddle search, and downhill paths.

use super::*;
use crate::linalg::{dot, norm, symmetric_eigen};

fn proceed(_: Event) -> ControlFlow<()> {
    ControlFlow::Continue(())
}

/// Four-exponential test surface (Müller & Brown, Theor. Chim. Acta 53, 75, 1979).
struct MullerBrown;

impl MullerBrown {
    fn derivatives(&self, x: &[f64], g: &mut [f64], h: &mut [f64]) -> f64 {
        let mut energy = 0.;
        g.fill(0.);
        h.fill(0.);
        for (amplitude, a, b, c, x0, y0) in [
            (-200., -1., 0., -10., 1., 0.),
            (-100., -1., 0., -10., 0., 0.5),
            (-170., -6.5, 11., -6.5, -0.5, 1.5),
            (15., 0.7, 0.6, 0.7, -1., 1.),
        ] {
            let (u, v) = (x[0] - x0, x[1] - y0);
            let term = amplitude * (a * u * u + b * u * v + c * v * v).exp();
            let d = [2. * a * u + b * v, b * u + 2. * c * v];
            energy += term;
            for k in 0..2 {
                g[k] += term * d[k];
            }
            for (entry, value) in h.iter_mut().zip([
                2. * a + d[0] * d[0],
                b + d[0] * d[1],
                b + d[0] * d[1],
                2. * c + d[1] * d[1],
            ]) {
                *entry += term * value;
            }
        }
        energy
    }
}

impl Potential for MullerBrown {
    fn evaluate(&mut self, x: &[f64], g: &mut [f64], _: Accuracy) -> Result<f64, EvalError> {
        Ok(self.derivatives(x, g, &mut [0.; 4]))
    }
    fn hessian(&mut self, x: &[f64], h: &mut [f64]) -> Option<Result<(), EvalError>> {
        self.derivatives(x, &mut [0.; 2], h);
        Some(Ok(()))
    }
}

const A: [f64; 2] = [-0.558224, 1.441726];
const B: [f64; 2] = [0.623499, 0.028038];
const INTERMEDIATE: [f64; 2] = [-0.050011, 0.466694];
const UPPER_SADDLE: [f64; 2] = [-0.822002, 0.624313];

fn distance(a: &[f64], b: &[f64]) -> f64 {
    norm(&a.iter().zip(b).map(|(x, y)| x - y).collect::<Vec<_>>())
}

#[test]
fn muller_brown_ci_neb_between_outer_minima() {
    let options = TsOptions {
        images: 16,
        neb_max_iterations: 1500,
        ..TsOptions::default()
    };
    let band = (0..options.images)
        .map(|i| {
            let t = i as f64 / (options.images - 1) as f64;
            A.iter().zip(B).map(|(a, b)| a + t * (b - a)).collect()
        })
        .collect();
    let ends = [A, B].map(|x| evaluate(&mut MullerBrown, &x).unwrap().0);
    let mut final_band = Vec::new();
    let mut terminal_events = 0;
    let result = neb::run(
        &mut [MullerBrown],
        band,
        ends,
        &[],
        &options,
        &mut |event| {
            if let Event::Neb {
                finished: true,
                energies,
                ..
            } = event
            {
                terminal_events += 1;
                final_band = energies.to_vec();
            }
            ControlFlow::Continue(())
        },
    )
    .unwrap();
    assert_eq!(terminal_events, 1);
    assert_eq!(
        final_band,
        result.path.iter().map(|p| p.energy).collect::<Vec<_>>()
    );
    let highest = &result.path[result.climbing];
    assert!(result.converged);
    assert!(
        distance(&highest.positions, &UPPER_SADDLE) < 0.005,
        "{highest:?}"
    );
    assert!((highest.energy + 40.665).abs() < 0.005, "{highest:?}");
}

#[test]
fn exhausted_neb_publishes_the_last_evaluated_band() {
    let options = TsOptions {
        images: 5,
        neb_max_iterations: 0,
        ..Default::default()
    };
    let band: Vec<Vec<_>> = (0..options.images)
        .map(|i| {
            let t = i as f64 / (options.images - 1) as f64;
            A.iter().zip(B).map(|(a, b)| a + t * (b - a)).collect()
        })
        .collect();
    let ends = [A, B].map(|x| evaluate(&mut MullerBrown, &x).unwrap().0);
    let mut events = Vec::new();
    let result = neb::run(
        &mut [MullerBrown],
        band,
        ends,
        &[],
        &options,
        &mut |event| {
            if let Event::Neb {
                finished, energies, ..
            } = event
            {
                events.push((finished, energies.to_vec()));
            }
            ControlFlow::Continue(())
        },
    )
    .unwrap();
    assert!(!result.converged);
    assert_eq!(
        events,
        [(true, result.path.iter().map(|p| p.energy).collect())]
    );
}

#[test]
fn muller_brown_refinement_and_downhill_paths_from_both_saddles() {
    // The outer minima A and B are separated by *two* passes. Each downhill
    // pair connects its outer well to the same shallow intermediate.
    for (start, tangent, expected, outside) in [
        ([-0.79, 0.65], [1., -1.], UPPER_SADDLE, A),
        ([0.19, 0.30], [0.5, -1.], [0.212487, 0.292988], B),
    ] {
        let options = TsOptions {
            irc_step: 0.015,
            irc_max_steps: 200,
            ..TsOptions::default()
        };
        let guess = Minimum {
            positions: start.into(),
            energy: evaluate(&mut MullerBrown, &start).unwrap().0,
        };
        let saddle::Refinement {
            point, converged, ..
        } = saddle::refine(
            &mut MullerBrown,
            &guess,
            &tangent,
            &[],
            &options,
            &mut proceed,
        )
        .unwrap();
        assert!(
            converged && distance(&point.positions, &expected) < 1e-5,
            "{point:?}"
        );
        let h = saddle::hessian(&mut MullerBrown, &point.positions).unwrap();
        let (_, modes) = symmetric_eigen(h.clone(), 2).unwrap();
        let descent = irc::Descent {
            ts: &point,
            mode: &modes[..2],
            hessian: &h,
            masses: &[],
            options: &options,
        };
        let ends = [true, false].map(|forward| {
            let (end, path, _) = descent
                .trace(&mut MullerBrown, forward, &mut proceed)
                .unwrap();
            assert!(path.len() > 1 && path.windows(2).all(|p| p[1].energy < p[0].energy));
            end.positions
        });
        for well in [outside, INTERMEDIATE] {
            assert!(
                ends.iter().any(|x| distance(x, &well) < 2e-4),
                "ends {ends:?}, expected {well:?}"
            );
        }
    }
}

/// One bistable pair, and two harmonic sides. Both triangles have zero energy.
/// At the saddle r₀₁ = 1.7 bohr and the barrier is 0.6⁴/4 hartree.
#[derive(Clone, Default)]
struct Triangle {
    analytic: bool,
    second_bistable_side: bool,
    evaluations: usize,
    hessians: usize,
}

impl Triangle {
    fn derivatives(&self, x: &[f64], g: &mut [f64], h: &mut [f64]) -> f64 {
        g.fill(0.);
        h.fill(0.);
        let mut energy = 0.;
        for (a, b) in [(0, 1), (0, 2), (1, 2)] {
            let d: [f64; 3] = std::array::from_fn(|k| x[3 * a + k] - x[3 * b + k]);
            let r = norm(&d);
            let u = d.map(|v| v / r);
            let (e, first, second) = if b == 1 || (a == 0 && self.second_bistable_side) {
                let t = r - if b == 1 { 1.7 } else { 2.3 };
                let q = t * t - 0.36;
                (q * q / 4., t * q, 3. * t * t - 0.36)
            } else {
                let t = r - 2.3;
                (t * t / 2., t, 1.)
            };
            energy += e;
            for i in 0..3 {
                g[3 * a + i] += first * u[i];
                g[3 * b + i] -= first * u[i];
                for j in 0..3 {
                    let hij = (second - first / r) * u[i] * u[j] + f64::from(i == j) * first / r;
                    for (m, n, sign) in [(a, a, 1.), (a, b, -1.), (b, a, -1.), (b, b, 1.)] {
                        h[(3 * m + i) * 9 + 3 * n + j] += sign * hij;
                    }
                }
            }
        }
        energy
    }
}

impl Potential for Triangle {
    fn evaluate(&mut self, x: &[f64], g: &mut [f64], _: Accuracy) -> Result<f64, EvalError> {
        self.evaluations += 1;
        Ok(self.derivatives(x, g, &mut [0.; 81]))
    }
    fn hessian(&mut self, x: &[f64], h: &mut [f64]) -> Option<Result<(), EvalError>> {
        self.hessians += 1;
        self.analytic.then(|| {
            self.derivatives(x, &mut [0.; 9], h);
            Ok(())
        })
    }
}

fn triangle(r: f64) -> [f64; 9] {
    [
        -r / 2.,
        0.,
        0.,
        r / 2.,
        0.,
        0.,
        0.,
        (2.3_f64.powi(2) - (r / 2.).powi(2)).sqrt(),
        0.,
    ]
}

fn triangle_inputs(
    numbers: &[u8],
    options: &TsOptions,
    analytic: bool,
    workers: usize,
    mut on_event: impl FnMut(Event) -> ControlFlow<()>,
) -> Result<TsReport, TsError> {
    let a = triangle(1.08);
    let b: Vec<_> = triangle(2.32)
        .as_chunks::<3>()
        .0
        .iter()
        .flat_map(|p| [3. + p[2], -2. + p[0], 0.8 + p[1]])
        .collect();
    let mut pool = vec![
        Triangle {
            analytic,
            ..Default::default()
        };
        workers
    ];
    let report = search(
        &mut pool,
        &Search {
            numbers,
            masses_amu: &[1., 2., 4.],
            reactant: &a,
            product: &b,
            options,
        },
        &mut on_event,
    )?;
    assert_eq!(
        report.evaluations,
        pool.iter().map(|p| p.evaluations).sum::<usize>()
    );
    assert_eq!(
        report.hessians,
        pool.iter().map(|p| p.hessians).sum::<usize>()
    );
    Ok(report)
}

fn saved_triangle_band() -> Vec<Minimum> {
    let potential = Triangle::default();
    [1.1, 1.4, 1.7, 2.0, 2.3]
        .into_iter()
        .map(|r| {
            let positions = triangle(r).to_vec();
            let energy = potential.derivatives(&positions, &mut [0.; 9], &mut [0.; 81]);
            Minimum { positions, energy }
        })
        .collect()
}

/// A distinct target surface with uniformly longer bonds, half the source
/// barrier, and an arbitrary energy zero. Source geometries are not stationary.
struct TargetTriangle {
    inner: Triangle,
    first_hessian: Option<Vec<f64>>,
}

impl TargetTriangle {
    const LENGTH: f64 = 1.08;
    const ENERGY: f64 = 0.5;
    const OFFSET: f64 = 7.0;

    fn new() -> Self {
        Self {
            inner: Triangle {
                analytic: true,
                ..Default::default()
            },
            first_hessian: None,
        }
    }
}

impl Potential for TargetTriangle {
    fn evaluate(&mut self, x: &[f64], g: &mut [f64], a: Accuracy) -> Result<f64, EvalError> {
        let scaled: Vec<_> = x.iter().map(|v| v / Self::LENGTH).collect();
        let energy = self.inner.evaluate(&scaled, g, a)?;
        g.iter_mut().for_each(|v| *v *= Self::ENERGY / Self::LENGTH);
        Ok(Self::OFFSET + Self::ENERGY * energy)
    }

    fn hessian(&mut self, x: &[f64], h: &mut [f64]) -> Option<Result<(), EvalError>> {
        if self.first_hessian.is_none() {
            self.first_hessian = Some(x.to_vec());
        }
        let scaled: Vec<_> = x.iter().map(|v| v / Self::LENGTH).collect();
        self.inner.hessian(&scaled, h).map(|result| {
            result.map(|()| {
                h.iter_mut()
                    .for_each(|v| *v *= Self::ENERGY / Self::LENGTH.powi(2));
            })
        })
    }
}

#[test]
fn geometry_refinement_reoptimizes_on_target_and_verifies_its_own_barrier() {
    let reactant = triangle(1.1);
    let product = triangle(2.3);
    let seed = triangle(1.7);
    let options = TsOptions {
        // Cross-method refinement must ignore this same-method shortcut.
        optimize_endpoints: false,
        ..Default::default()
    };
    let input = Search {
        numbers: &[1; 3],
        masses_amu: &[1., 2., 4.],
        reactant: &reactant,
        product: &product,
        options: &options,
    };
    let mut potential = TargetTriangle::new();
    let source_geometry_energy = evaluate(&mut potential, &seed).unwrap().0;
    potential.inner.evaluations = 0;
    let mut stages = Vec::new();
    let (report, checked_geometry) = refine_with_verification(
        &mut potential,
        &input,
        &seed,
        |event| {
            if let Event::Stage(stage) = event {
                stages.push(stage);
            }
            ControlFlow::Continue(())
        },
        |_, x, imaginary| {
            assert_eq!(imaginary, 1);
            Ok((true, x.to_vec()))
        },
    )
    .unwrap();
    assert_eq!(
        stages,
        [
            Stage::Endpoints,
            Stage::Saddle,
            Stage::Frequencies,
            Stage::Irc
        ]
    );
    assert!(report.saddle_converged && report.connects, "{report:?}");
    assert_eq!(report.imaginary, 1);
    assert_eq!(potential.first_hessian.as_deref(), Some(seed.as_slice()));
    assert_eq!(checked_geometry, report.ts.positions);
    // The production operation uses Normal stationarity tolerances; it does
    // not promise the analytic saddle to four decimal places in bohr.
    for (point, expected) in [
        (&report.ts, 1.7),
        (&report.reactant, 1.1),
        (&report.product, 2.3),
    ] {
        assert!(
            (endpoint::distance(&point.positions, 0, 1) - expected * TargetTriangle::LENGTH).abs()
                < 0.005,
            "{point:?}"
        );
    }
    let target_barrier = TargetTriangle::ENERGY * 0.6_f64.powi(4) / 4.;
    assert!((report.ts.energy - TargetTriangle::OFFSET - target_barrier).abs() < 1e-5);
    assert!((report.reactant.energy - TargetTriangle::OFFSET).abs() < 1e-5);
    assert!((report.forward_barrier_kcal() - target_barrier * KCAL_PER_HARTREE).abs() < 0.01);
    assert!((report.ts.energy - source_geometry_energy).abs() > 1e-3);
    assert_eq!(
        report.path,
        [
            report.reactant.clone(),
            report.ts.clone(),
            report.product.clone()
        ]
    );
    assert_eq!(report.climbing_image, 1);
    assert!(!report.neb_converged);
    assert_eq!(report.evaluations, potential.inner.evaluations);
    assert_eq!(report.hessians, potential.inner.hessians);
    let mut independent = TargetTriangle::new();
    for point in &report.path {
        let (energy, gradient) = evaluate(&mut independent, &point.positions).unwrap();
        assert!((point.energy - energy).abs() < 1e-12);
        let normal = crate::Level::Normal.criteria();
        assert!(gradient.iter().all(|g| g.abs() <= normal.max_gradient));
        assert!(norm(&gradient) / (gradient.len() as f64).sqrt() <= normal.rms_gradient);
    }
    for branch in report.irc.unwrap() {
        assert_ne!(branch.reached, Reached::Other);
        for point in branch.path.iter().chain(std::iter::once(&branch.end)) {
            assert!(point.energy >= TargetTriangle::OFFSET - 1e-10);
        }
    }
}

#[test]
fn geometry_refinement_failure_keeps_target_candidate_unverified() {
    let reactant = triangle(1.1);
    let product = triangle(2.3);
    let seed = triangle(1.7);
    for (steps, agrees) in [(0, true), (100, false)] {
        let options = TsOptions {
            saddle_max_steps: steps,
            ..Default::default()
        };
        let mut potential = TargetTriangle::new();
        let (report, checked) = refine_with_verification(
            &mut potential,
            &Search {
                numbers: &[1; 3],
                masses_amu: &[1., 2., 4.],
                reactant: &reactant,
                product: &product,
                options: &options,
            },
            &seed,
            |event| {
                assert!(!matches!(event, Event::Stage(Stage::Irc)));
                ControlFlow::Continue(())
            },
            |_, positions, _| Ok((agrees, positions.to_vec())),
        )
        .unwrap();
        assert_eq!(report.saddle_converged, steps > 0);
        assert!(report.irc.is_none() && !report.connects);
        assert_eq!(checked, report.ts.positions);
        assert!(report.ts.energy > TargetTriangle::OFFSET);
        assert!((report.reactant.energy - TargetTriangle::OFFSET).abs() < 1e-7);
        assert_eq!(report.path[1], report.ts);
    }
}

struct TargetSingleWell(f64);

impl Potential for TargetSingleWell {
    fn evaluate(&mut self, x: &[f64], g: &mut [f64], _: Accuracy) -> Result<f64, EvalError> {
        let r = endpoint::distance(x, 0, 1);
        let delta = r - self.0;
        for k in 0..3 {
            g[k] = delta * (x[k] - x[3 + k]) / r;
            g[3 + k] = -g[k];
        }
        Ok(9.0 + 0.5 * delta * delta)
    }

    fn hessian(&mut self, _: &[f64], _: &mut [f64]) -> Option<Result<(), EvalError>> {
        panic!("an altered reaction must fail before TS refinement");
    }
}

#[test]
fn geometry_refinement_rejects_target_endpoint_collapse_before_saddle_work() {
    let options = TsOptions::default();
    let result = refine(
        &mut TargetSingleWell(1.2),
        &Search {
            numbers: &[1; 2],
            masses_amu: &[1.; 2],
            reactant: &[0., 0., 0., 1.1, 0., 0.],
            product: &[0., 0., 0., 2.3, 0., 0.],
            options: &options,
        },
        &[0., 0., 0., 1.7, 0., 0.],
        proceed,
    );
    assert!(
        matches!(result, Err(TsError::Failed(message)) if message.contains("source reaction endpoint"))
    );
}

#[test]
fn selected_search_preserves_endpoint_identity_before_interpolation_or_neb() {
    fn prepared(
        potential: &mut (impl Potential + Send),
        distances: [f64; 2],
        preserve: bool,
    ) -> (Result<TsReport, TsError>, Vec<Stage>) {
        let options = TsOptions {
            images: 5,
            preserve_endpoint_identity: preserve,
            ..Default::default()
        };
        let reactant = [0., 0., 0., distances[0], 0., 0.];
        let product = [0., 0., 0., distances[1], 0., 0.];
        let mut stages = Vec::new();
        let report = search(
            std::slice::from_mut(potential),
            &Search {
                numbers: &[1; 2],
                masses_amu: &[1.; 2],
                reactant: &reactant,
                product: &product,
                options: &options,
            },
            |event| {
                if let Event::Stage(stage) = event {
                    stages.push(stage);
                    if stage == Stage::Interpolation {
                        return ControlFlow::Break(());
                    }
                }
                ControlFlow::Continue(())
            },
        );
        (report, stages)
    }

    // Both input and target graphs are unbonded. The geometry guard must catch
    // their collapse without depending on a changed graph or an IRC result.
    let (collapsed, stages) = prepared(&mut TargetSingleWell(5.5), [5.0, 6.0], true);
    assert_eq!(stages, [Stage::Endpoints]);
    assert!(matches!(collapsed, Err(TsError::Failed(message)) if message.contains("collapsed")));

    // These unbonded seeds relax into distinct 1- and 3-bohr wells, changing
    // the mapped reactant bond graph while retaining two distinct minima.
    let (changed, stages) = prepared(&mut CarbonStretch(0.02), [1.8, 3.1], true);
    assert_eq!(stages, [Stage::Endpoints]);
    assert!(
        matches!(changed, Err(TsError::Failed(message)) if message.contains("reactant relaxation"))
    );

    // Existing callers preserve their behavior until they opt into the guard.
    assert!(!TsOptions::default().preserve_endpoint_identity);
    let (unguarded, stages) = prepared(&mut TargetSingleWell(5.5), [5.0, 6.0], false);
    assert_eq!(stages, [Stage::Endpoints, Stage::Interpolation]);
    assert_eq!(unguarded, Err(TsError::Cancelled));
}

#[test]
fn geometry_refinement_propagates_endpoint_failure_and_cancellation() {
    struct EndpointFailure;
    impl Potential for EndpointFailure {
        fn evaluate(&mut self, _: &[f64], _: &mut [f64], _: Accuracy) -> Result<f64, EvalError> {
            Err(EvalError::Scf("target endpoint failed".into()))
        }
    }
    struct NoMinimum;
    impl Potential for NoMinimum {
        fn evaluate(&mut self, x: &[f64], g: &mut [f64], _: Accuracy) -> Result<f64, EvalError> {
            let r = endpoint::distance(x, 0, 1);
            for k in 0..3 {
                g[k] = 0.01 * (x[3 + k] - x[k]) / r;
                g[3 + k] = -g[k];
            }
            Ok(-0.01 * r)
        }
    }
    let options = TsOptions::default();
    let input = Search {
        numbers: &[1; 2],
        masses_amu: &[1.; 2],
        reactant: &[0., 0., 0., 1.1, 0., 0.],
        product: &[0., 0., 0., 2.3, 0., 0.],
        options: &options,
    };
    let seed = [0., 0., 0., 1.7, 0., 0.];
    assert_eq!(
        refine(&mut EndpointFailure, &input, &seed, proceed),
        Err(TsError::Scf("target endpoint failed".into()))
    );
    assert_eq!(
        refine(&mut EndpointFailure, &input, &seed, |_| ControlFlow::Break(
            ()
        )),
        Err(TsError::Cancelled)
    );
    let unconverged = refine(&mut NoMinimum, &input, &seed, |event| {
        assert!(!matches!(event, Event::Stage(Stage::Saddle)));
        ControlFlow::Continue(())
    });
    assert!(matches!(unconverged, Err(TsError::Failed(_))));
}

#[test]
fn saved_band_refines_directly_and_verifies_both_irc_destinations() {
    let path = saved_triangle_band();
    let options = TsOptions {
        images: path.len(),
        ..Default::default()
    };
    let search = Search {
        numbers: &[1; 3],
        masses_amu: &[1., 2., 4.],
        reactant: &path[0].positions,
        product: &path[path.len() - 1].positions,
        options: &options,
    };
    for neb_converged in [false, true] {
        let mut potential = Triangle {
            analytic: true,
            ..Default::default()
        };
        let mut stages = Vec::new();
        let report = refine_from_band(&mut potential, &search, &path, neb_converged, |event| {
            if let Event::Stage(stage) = event {
                stages.push(stage);
            }
            ControlFlow::Continue(())
        })
        .unwrap();
        assert_eq!(stages, [Stage::Saddle, Stage::Frequencies, Stage::Irc]);
        assert_eq!(report.path, path);
        assert_eq!(report.climbing_image, 2);
        assert_eq!(report.neb_converged, neb_converged);
        assert!(report.saddle_converged && report.connects);
        assert_eq!(report.imaginary, 1);
        assert!((report.ts.energy - 0.6_f64.powi(4) / 4.).abs() < 1e-8);
        let reached = report.irc.unwrap().map(|branch| branch.reached);
        assert!(reached.contains(&Reached::Reactant));
        assert!(reached.contains(&Reached::Product));
        assert_eq!(report.evaluations, potential.evaluations);
        assert_eq!(report.hessians, potential.hessians);
    }
}

#[test]
fn independent_frequency_check_precedes_and_can_veto_irc() {
    use std::cell::Cell;
    let checked = Cell::new(false);
    let irc_started = Cell::new(false);
    let mut pool = [Triangle {
        analytic: true,
        ..Default::default()
    }];
    let options = TsOptions::default();
    let a = triangle(1.1);
    let b = triangle(2.3);
    let (report, ()) = search_with_verification(
        &mut pool,
        &Search {
            numbers: &[1; 3],
            masses_amu: &[1., 2., 4.],
            reactant: &a,
            product: &b,
            options: &options,
        },
        |event| {
            if matches!(event, Event::Stage(Stage::Irc)) {
                assert!(checked.get(), "IRC started before independent frequencies");
                irc_started.set(true);
            }
            ControlFlow::Continue(())
        },
        || ControlFlow::Continue(()),
        |potential, positions, imaginary| {
            assert!(!irc_started.get());
            let exact = saddle::hessian(potential, positions)?;
            let (frequencies, _) = modes::frequencies(positions, &exact, &[1., 2., 4.])?;
            assert_eq!(
                imaginary,
                frequencies.iter().filter(|f| **f < -IMAGINARY_CM1).count()
            );
            checked.set(true);
            Ok((false, ()))
        },
    )
    .unwrap();
    assert!(checked.get() && !irc_started.get());
    assert!(report.saddle_converged && report.imaginary == 1);
    assert!(report.irc.is_none() && !report.connects);
}

/// Triangle specified by all three side lengths, with atom order preserved.
fn triangle_sides(a: f64, b: f64, c: f64) -> Vec<f64> {
    let x = (a * a + b * b - c * c) / (2.0 * a);
    vec![0., 0., 0., a, 0., 0., x, (b * b - x * x).sqrt(), 0.]
}

#[test]
fn saved_second_order_candidate_recovers_a_nearby_first_order_connection() {
    // The saved band already passes a first-order saddle (spectator side in
    // its lower well), while the actual failed candidate has both sides at
    // their barriers. Recovery must begin at the latter, not rerun that band.
    let mut potential = Triangle {
        analytic: true,
        second_bistable_side: true,
        ..Default::default()
    };
    let path: Vec<_> = [1.1, 1.4, 1.7, 2.0, 2.3]
        .into_iter()
        .map(|r| {
            let positions = triangle_sides(r, 1.7, 2.3);
            let energy = evaluate(&mut potential, &positions).unwrap().0;
            Minimum { positions, energy }
        })
        .collect();
    let positions = triangle_sides(1.7, 2.3, 2.3);
    let hessian = saddle::hessian(&mut potential, &positions).unwrap();
    let (frequencies, _) = modes::frequencies(&positions, &hessian, &[1.; 3]).unwrap();
    assert_eq!(
        frequencies.iter().filter(|f| **f < -IMAGINARY_CM1).count(),
        2
    );
    let options = TsOptions {
        images: path.len(),
        ..Default::default()
    };
    let search = Search {
        numbers: &[1; 3],
        masses_amu: &[1.; 3],
        reactant: &path[0].positions,
        product: &path[path.len() - 1].positions,
        options: &options,
    };
    struct Tracked {
        inner: Triangle,
        hessian_points: Vec<Vec<f64>>,
        origin: Vec<f64>,
        seed_starts: usize,
        fault: Option<(usize, EvalError)>,
    }
    impl Potential for Tracked {
        fn evaluate(&mut self, x: &[f64], g: &mut [f64], a: Accuracy) -> Result<f64, EvalError> {
            self.inner.evaluate(x, g, a)
        }
        fn hessian(&mut self, x: &[f64], h: &mut [f64]) -> Option<Result<(), EvalError>> {
            self.hessian_points.push(x.to_vec());
            if (distance(x, &self.origin) - 0.1).abs() < 1e-10 {
                self.seed_starts += 1;
                if let Some((seed, error)) = &self.fault
                    && self.seed_starts == *seed
                {
                    return Some(Err(error.clone()));
                }
            }
            self.inner.hessian(x, h)
        }
    }
    for (veto, fault) in [
        (false, None),
        (true, None),
        (
            false,
            Some((1, EvalError::Failed("first seed unavailable".into()))),
        ),
        (
            false,
            Some((2, EvalError::Failed("alternate seed unavailable".into()))),
        ),
        (false, Some((2, EvalError::Cancelled))),
    ] {
        potential.evaluations = 0;
        potential.hessians = 0;
        let mut potential = Tracked {
            inner: potential.clone(),
            hessian_points: Vec::new(),
            origin: positions.clone(),
            seed_starts: 0,
            fault: fault.clone(),
        };
        let mut checks = Vec::new();
        let result = refine_from_candidate(
            &mut potential,
            &search,
            &path,
            false,
            &SaddleCandidate {
                positions: positions.clone(),
                hessian: hessian.clone(),
            },
            proceed,
            |_, x, imaginary| {
                checks.push((x.to_vec(), imaginary));
                Ok((!veto, x.to_vec()))
            },
        );
        if matches!(fault, Some((_, EvalError::Cancelled))) {
            assert_eq!(result.unwrap_err(), TsError::Cancelled);
            continue;
        }
        let (report, checked_positions) = result.unwrap();
        assert_eq!(checks.len(), if fault.is_some() { 1 } else { 2 });
        assert!(
            report.saddle_converged && report.imaginary == 1,
            "{report:?}"
        );
        assert_eq!(report.connects, !veto, "{report:?}");
        assert_eq!(checked_positions, report.ts.positions);
        if veto {
            assert!(report.irc.is_none());
        }
        assert_eq!(report.path, path);
        assert!(!report.neb_converged);
        assert_eq!(report.evaluations, potential.inner.evaluations);
        assert_eq!(report.hessians, potential.hessian_points.len());
        let seeds: Vec<_> = potential
            .hessian_points
            .iter()
            .filter(|x| (distance(x, &positions) - 0.1).abs() < 1e-10)
            .collect();
        assert_eq!(
            seeds.len(),
            2,
            "each sign starts 0.1 bohr from saved candidate"
        );
        for i in 0..positions.len() {
            assert!((seeds[0][i] + seeds[1][i] - 2.0 * positions[i]).abs() < 1e-12);
        }
    }
}

#[test]
fn saved_band_validation_and_initial_cancellation_do_not_evaluate_the_potential() {
    let path = saved_triangle_band();
    let options = TsOptions {
        images: path.len(),
        ..Default::default()
    };
    let search = Search {
        numbers: &[1; 3],
        masses_amu: &[1., 2., 4.],
        reactant: &path[0].positions,
        product: &path[path.len() - 1].positions,
        options: &options,
    };
    let mut invalid = Vec::new();
    invalid.push(path[..path.len() - 1].to_vec());
    let mut nonfinite = path.clone();
    nonfinite[2].energy = f64::NAN;
    invalid.push(nonfinite);
    let mut incomplete = path.clone();
    incomplete[2].positions.pop();
    invalid.push(incomplete);
    let mut nonfinite = path.clone();
    nonfinite[2].positions[0] = f64::INFINITY;
    invalid.push(nonfinite);
    let mut coincident = path.clone();
    coincident[2].positions[..3].copy_from_slice(&path[2].positions[3..6]);
    invalid.push(coincident);
    let mut wrong_endpoint = path.clone();
    wrong_endpoint[0].positions = triangle(1.2).to_vec();
    invalid.push(wrong_endpoint);
    let mut potential = Triangle::default();
    for invalid in invalid {
        assert!(matches!(
            refine_from_band(&mut potential, &search, &invalid, false, |_| {
                panic!("invalid saved band emitted progress")
            }),
            Err(TsError::Invalid(_))
        ));
    }
    assert_eq!(
        refine_from_band(&mut potential, &search, &path, false, |event| {
            assert!(matches!(event, Event::Stage(Stage::Saddle)));
            ControlFlow::Break(())
        }),
        Err(TsError::Cancelled)
    );
    assert_eq!(potential.evaluations, 0);
    assert_eq!(potential.hessians, 0);
}

#[test]
fn molecular_barrier_irc_and_parallel_images() {
    let options = TsOptions::default();
    let mut stages = Vec::new();
    let one = triangle_inputs(&[1; 3], &options, true, 1, |event| {
        if let Event::Stage(s) = event {
            stages.push(s);
        }
        ControlFlow::Continue(())
    })
    .unwrap();
    let three = triangle_inputs(&[1; 3], &options, true, 3, proceed).unwrap();
    assert_eq!(one, three);
    assert_eq!(
        stages,
        [
            Stage::Endpoints,
            Stage::Interpolation,
            Stage::Neb,
            Stage::Saddle,
            Stage::Frequencies,
            Stage::Irc
        ]
    );
    assert!(
        one.connects && one.saddle_converged && one.neb_converged,
        "saddle={}, band={}, connects={}",
        one.saddle_converged,
        one.neb_converged,
        one.connects
    );
    assert_eq!(one.frequencies_cm1.len(), 3);
    assert_eq!(one.imaginary, 1);
    assert_eq!(
        one.frequencies_cm1
            .iter()
            .filter(|&&f| f < -IMAGINARY_CM1)
            .count(),
        1
    );
    for barrier in [one.forward_barrier_kcal(), one.reverse_barrier_kcal()] {
        assert!((barrier / KCAL_PER_HARTREE - 0.0324).abs() < 1e-4);
    }
    assert_eq!(one.path.first(), Some(&one.reactant));
    assert_eq!(one.path.last(), Some(&one.product));
    assert!((dot(&one.transition_mode, &one.transition_mode) - 1.).abs() < 1e-12);
    assert_eq!(one.hessians, 2);
    for branch in one.irc.as_ref().unwrap() {
        assert_eq!(branch.energies.len(), branch.path.len() + 2);
        assert_eq!(branch.energies[0], one.ts.energy);
        assert!(branch.energies[1] < one.ts.energy);
        assert_eq!(branch.energies.last(), Some(&branch.end.energy));
        for (point, &energy) in branch.path.iter().zip(&branch.energies[1..]) {
            assert_eq!(point.energy, energy);
            assert_eq!(point.positions.len(), one.ts.positions.len());
        }
    }
}

#[test]
fn finite_difference_hessian_and_unconverged_band() {
    let options = TsOptions {
        neb_max_iterations: 0,
        hessian_every: 1,
        optimize_endpoints: false,
        ..TsOptions::default()
    };
    let mut accepted_steps = 0;
    let report = triangle_inputs(&[1; 3], &options, false, 1, |event| {
        if let Event::Saddle { step, .. } = event {
            accepted_steps = step;
        }
        proceed(event)
    })
    .unwrap();
    assert!(!report.neb_converged);
    assert!(report.saddle_converged && report.connects);
    assert!(accepted_steps > 0);
    // With an interval of one, frequencies reuse the rebuild at the last step.
    assert_eq!(report.hessians, 1 + accepted_steps);
    assert!(report.evaluations >= report.hessians * 18);
    assert!((report.ts.energy - 0.0324).abs() < 1e-4);
}

#[test]
fn callbacks_cancel_every_stage() {
    for stage in [
        Stage::Endpoints,
        Stage::Interpolation,
        Stage::Neb,
        Stage::Saddle,
        Stage::Frequencies,
        Stage::Irc,
    ] {
        let result = triangle_inputs(&[1; 3], &TsOptions::default(), true, 1, |e| {
            if matches!(e, Event::Stage(s) if s==stage) {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        });
        assert_eq!(result.unwrap_err(), TsError::Cancelled);
    }
}

#[derive(Clone, Copy)]
enum IrcFault {
    Scf,
    StepLimit,
    Cancelled,
}

fn branch_failure(fault: IrcFault, fail_forward: bool) -> Result<TsReport, TsError> {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    struct FailingIrc {
        triangle: Triangle,
        fault_active: Arc<AtomicBool>,
        fault: IrcFault,
    }

    impl Potential for FailingIrc {
        fn evaluate(&mut self, x: &[f64], g: &mut [f64], a: Accuracy) -> Result<f64, EvalError> {
            if !self.fault_active.load(Ordering::Relaxed) {
                return self.triangle.evaluate(x, g, a);
            }
            match self.fault {
                IrcFault::Scf => Err(EvalError::Scf("injected second-branch SCF failure".into())),
                IrcFault::Cancelled => Err(EvalError::Cancelled),
                IrcFault::StepLimit => {
                    // An unbounded internal stretch cannot meet the minimizer's
                    // gradient criterion and exhausts its accepted-step budget.
                    let axis: [f64; 3] = std::array::from_fn(|k| x[3 + k] - x[k]);
                    let r = norm(&axis);
                    g.fill(0.);
                    for k in 0..3 {
                        g[k] = axis[k] / r;
                        g[3 + k] = -g[k];
                    }
                    Ok(-r)
                }
            }
        }

        fn hessian(&mut self, x: &[f64], h: &mut [f64]) -> Option<Result<(), EvalError>> {
            self.triangle.hessian(x, h)
        }
    }

    let fault_active = Arc::new(AtomicBool::new(false));
    let mut pool = [FailingIrc {
        triangle: Triangle {
            analytic: true,
            ..Default::default()
        },
        fault_active: Arc::clone(&fault_active),
        fault,
    }];
    let options = TsOptions {
        irc_max_length: if matches!(fault, IrcFault::StepLimit) {
            0.05
        } else {
            TsOptions::default().irc_max_length
        },
        ..TsOptions::default()
    };
    search(
        &mut pool,
        &Search {
            numbers: &[1; 3],
            masses_amu: &[1., 2., 4.],
            reactant: &triangle(1.08),
            product: &triangle(2.32),
            options: &options,
        },
        |event| {
            if matches!(
                event,
                Event::Irc {
                    forward,
                    step,
                    ..
                } if forward == fail_forward && step == usize::from(matches!(fault, IrcFault::StepLimit))
            ) {
                fault_active.store(true, Ordering::Relaxed);
            }
            ControlFlow::Continue(())
        },
    )
}

#[test]
fn second_irc_branch_scf_failure_keeps_characterized_report() {
    let report = branch_failure(IrcFault::Scf, false)
        .expect("an IRC SCF failure discarded the characterized candidate");
    assert!(report.saddle_converged);
    assert_eq!(report.imaginary, 1);
    assert_eq!(report.frequencies_cm1.len(), 3);
    assert!((norm(&report.transition_mode) - 1.).abs() < 1e-12);
    assert!((report.ts.energy - 0.0324).abs() < 1e-4);
    assert!(report.irc.is_none() && !report.connects);
    assert_eq!(report.hessian.len(), report.ts.positions.len().pow(2));
    assert!(report.hessian.iter().all(|v| v.is_finite()));
    assert_eq!(
        report.irc_error.unwrap().cause,
        TsError::Scf("injected second-branch SCF failure".into())
    );
    assert_eq!(
        branch_failure(IrcFault::Cancelled, false).unwrap_err(),
        TsError::Cancelled
    );
}

#[test]
fn irc_minimizer_step_limit_keeps_every_finished_trace() {
    for fail_forward in [true, false] {
        let report = branch_failure(IrcFault::StepLimit, fail_forward).unwrap();
        assert!(report.saddle_converged);
        assert_eq!(report.imaginary, 1);
        assert!((report.ts.energy - 0.0324).abs() < 1e-4);
        assert!(report.irc.is_none() && !report.connects);
        let [plus, minus] = report.irc_diagnostics();
        assert_eq!(plus.unwrap().reached.is_none(), fail_forward);
        assert_eq!(minus.is_none(), fail_forward);
        for trace in [plus, minus].into_iter().flatten() {
            assert_eq!(trace.reason, IrcReason::LengthCap);
            assert_eq!(trace.steps, 1);
            assert_eq!(trace.gradient_calls, 5);
            assert_eq!(trace.path_length, 0.05);
        }
        assert!(minus.is_none_or(|trace| trace.reached.is_none()));
        assert_eq!(
            report.irc_error.unwrap().cause,
            TsError::Failed("endpoint minimization reached its step limit".into())
        );
    }
}

#[test]
fn invalid_inputs_skip_evaluations_and_events() {
    let x = [0., 0., 0., 1., 0., 0.];
    let base = Search {
        numbers: &[1, 1],
        masses_amu: &[1., 1.],
        reactant: &x,
        product: &x,
        options: &TsOptions::default(),
    };
    let many = vec![1; MAX_ATOMS + 1];
    let mut p = [Triangle::default()];
    for input in [
        Search {
            numbers: &many,
            ..base
        },
        Search {
            numbers: &[1],
            ..base
        },
        Search {
            numbers: &[0, 1],
            ..base
        },
        Search {
            reactant: &[0.],
            ..base
        },
        Search {
            product: &[f64::NAN; 6],
            ..base
        },
        Search {
            masses_amu: &[1.],
            ..base
        },
        Search {
            masses_amu: &[0., 1.],
            ..base
        },
        Search {
            masses_amu: &[1., f64::INFINITY],
            ..base
        },
        Search {
            options: &TsOptions {
                images: 4,
                ..Default::default()
            },
            ..base
        },
        Search {
            options: &TsOptions {
                images: 25,
                ..Default::default()
            },
            ..base
        },
        Search {
            options: &TsOptions {
                irc_step: f64::NAN,
                ..Default::default()
            },
            ..base
        },
        Search {
            options: &TsOptions {
                irc_max_length: 0.,
                ..Default::default()
            },
            ..base
        },
        Search {
            options: &TsOptions {
                irc_max_length: f64::NAN,
                ..Default::default()
            },
            ..base
        },
        Search {
            options: &TsOptions {
                irc_max_length: f64::INFINITY,
                ..Default::default()
            },
            ..base
        },
    ] {
        assert!(matches!(
            search(&mut p, &input, |_| panic!("invalid input emitted progress")),
            Err(TsError::Invalid(_))
        ));
    }
    assert_eq!(p[0].evaluations, 0);
    assert!(matches!(
        search::<Triangle>(&mut [], &base, proceed),
        Err(TsError::Invalid(_))
    ));
}

#[test]
fn same_graph_irc_ends_are_distinguished_by_superposed_rmsd() {
    let report = triangle_inputs(&[6; 3], &TsOptions::default(), true, 1, proceed).unwrap();
    assert_eq!(
        bonds(&[6; 3], &report.reactant.positions).unwrap(),
        bonds(&[6; 3], &report.product.positions).unwrap()
    );
    assert!(report.connects);
    for branch in report.irc.unwrap() {
        let reference = match branch.reached {
            Reached::Reactant => &report.reactant,
            Reached::Product => &report.product,
            Reached::Other => panic!("branch did not reach an endpoint"),
        };
        let aligned =
            mapping::superpose(&reference.positions, &branch.end.positions, &[1., 2., 4.]);
        assert!(distance(&aligned, &reference.positions) < 0.003);
    }
}

#[test]
fn stationary_minimum_is_reported_without_irc() {
    let x = triangle(1.1);
    let input = Search {
        numbers: &[1; 3],
        masses_amu: &[1.; 3],
        reactant: &x,
        product: &x,
        options: &TsOptions::default(),
    };
    let report = search(&mut [Triangle::default()], &input, proceed).unwrap();
    assert_eq!(report.imaginary, 0);
    assert!(report.frequencies_cm1.iter().all(|&f| f >= -IMAGINARY_CM1));
    assert_eq!(report.irc, None);
    assert!(!report.connects);
}

#[test]
fn neb_assigns_physical_image_indices_and_propagates_worker_errors() {
    #[derive(Default, Clone)]
    struct Probe {
        visits: Vec<usize>,
        error: Option<EvalError>,
    }
    impl Potential for Probe {
        fn evaluate(&mut self, x: &[f64], g: &mut [f64], _: Accuracy) -> Result<f64, EvalError> {
            self.visits.push(x[0] as usize);
            g.fill(0.);
            self.error.clone().map_or(Ok(0.), Err)
        }
    }
    let options = TsOptions {
        neb_max_iterations: 0,
        ..Default::default()
    };
    let band: Vec<_> = (0..6).map(|i| vec![f64::from(i), 0.]).collect();
    let mut probes = vec![Probe::default(); 3];
    neb::run(
        &mut probes,
        band.clone(),
        [0.; 2],
        &[],
        &options,
        &mut proceed,
    )
    .unwrap();
    assert_eq!(
        probes
            .iter()
            .map(|p| p.visits.as_slice())
            .collect::<Vec<_>>(),
        [&[3][..], &[1, 4], &[2]]
    );
    for error in [
        EvalError::Scf("scf".into()),
        EvalError::Failed("worker".into()),
        EvalError::Cancelled,
    ] {
        probes[2].error = Some(error.clone());
        assert!(
            matches!(neb::run(&mut probes, band.clone(), [0.;2], &[], &options, &mut proceed), Err(e) if e==TsError::from(error))
        );
    }
}

#[test]
fn higher_order_saddle_reports_both_imaginary_frequencies() {
    // Two bistable distances are simultaneously at their barriers.
    let x = triangle(1.7);
    let options = TsOptions {
        optimize_endpoints: false,
        ..Default::default()
    };
    let input = Search {
        numbers: &[1; 3],
        masses_amu: &[1., 2., 4.],
        reactant: &x,
        product: &x,
        options: &options,
    };
    let mut pool = [Triangle {
        analytic: true,
        second_bistable_side: true,
        ..Default::default()
    }];
    let report = search(&mut pool, &input, proceed).unwrap();
    assert!(report.saddle_converged);
    assert_eq!(report.imaginary, 2);
    assert!(report.irc.is_none() && !report.connects);
    assert_eq!(
        report
            .frequencies_cm1
            .iter()
            .filter(|&&f| f < -IMAGINARY_CM1)
            .count(),
        2
    );
}

#[test]
fn fitted_cartesian_hessian_matches_independent_rebuild() {
    let reference = triangle(1.45);
    let mobile: Vec<_> = reference
        .as_chunks::<3>()
        .0
        .iter()
        .flat_map(|r| {
            [
                8. + 0.6 * r[0] - 0.8 * r[1],
                -7. + r[2],
                2. - 0.8 * r[0] - 0.6 * r[1],
            ]
        })
        .collect();
    let fit = mapping::Fit::new(&reference, &mobile, &[1., 2., 4.]);
    let fitted = fit.apply(&mobile);
    assert!(distance(&reference, &fitted) < 1e-12);
    let mut p = Triangle {
        analytic: true,
        ..Default::default()
    };
    let h = saddle::hessian(&mut p, &mobile).unwrap();
    let rotated = fit.rotate_hessian(&h, mobile.len());
    let rebuilt = saddle::hessian(&mut p, &fitted).unwrap();
    assert!(distance(&rotated, &rebuilt) < 1e-12);
}

/// Radial carbon double well with minima at 1 and 3 bohr and a saddle at 2 bohr.
struct CarbonStretch(f64);

impl Potential for CarbonStretch {
    fn evaluate(&mut self, x: &[f64], g: &mut [f64], _: Accuracy) -> Result<f64, EvalError> {
        let axis: [f64; 3] = std::array::from_fn(|i| x[i + 3] - x[i]);
        let r = norm(&axis);
        let t = r - 2.;
        let q = t * t - 1.;
        for i in 0..3 {
            g[i + 3] = self.0 * t * q * axis[i] / r;
            g[i] = -g[i + 3];
        }
        Ok(self.0 * q * q / 4.)
    }
}

#[test]
fn sparse_band_restart_recovers_a_saddle_hidden_after_a_positive_mode() {
    let mut surface = CarbonStretch(0.02);
    let minimum = Minimum {
        positions: vec![0., 0., 0., 1., 0., 0.],
        energy: 0.,
    };
    let tangent = vec![-1., 0., 0., 1., 0., 0.];
    let options = TsOptions::default();
    let bare = saddle::refine(
        &mut surface,
        &minimum,
        &tangent,
        &[12.; 2],
        &options,
        &mut proceed,
    )
    .unwrap();
    assert!(bare.converged && bare.point.energy.abs() < 1e-12);
    let (before, _) =
        modes::frequencies(&bare.point.positions, &bare.hessian.unwrap(), &[12.; 2]).unwrap();
    assert!(before[0] > 0.);
    // The CI has collapsed into the first well, but its next link spans the
    // barrier. The exact midpoint has r=2 and fits within the energy ceiling.
    let band = neb::Band {
        path: vec![
            minimum.clone(),
            minimum,
            Minimum {
                positions: vec![0., 0., 0., 3., 0., 0.],
                energy: 0.,
            },
        ],
        converged: false,
        climbing: 1,
        tangent,
    };
    let recovered =
        saddle::refine_band(&mut surface, &band, &[12.; 2], &options, &mut proceed).unwrap();
    assert!(recovered.converged);
    assert!((recovered.point.energy - 0.005).abs() < 1e-10);
    let x = &recovered.point.positions;
    assert!((distance(&x[..3], &x[3..]) - 2.).abs() < 1e-8);
    let (frequencies, _) = modes::frequencies(x, &recovered.hessian.unwrap(), &[12.; 2]).unwrap();
    assert_eq!(
        frequencies.iter().filter(|f| **f < -IMAGINARY_CM1).count(),
        1
    );
}

/// The band tangent can select a stable mode at the correct stationary saddle.
/// Its neighboring link midpoints are higher stationary index-two points.
struct TangentialSaddle;

impl Potential for TangentialSaddle {
    fn evaluate(&mut self, x: &[f64], g: &mut [f64], _: Accuracy) -> Result<f64, EvalError> {
        let (a, b) = (x[0], x[1]);
        g[0] = a.powi(5) - 1.25 * a.powi(3) + 0.25 * a;
        g[1] = -2. * b + 4. * b.powi(3);
        Ok(a.powi(6) / 6. - 5. * a.powi(4) / 16. + a * a / 8. - b * b + b.powi(4))
    }

    fn hessian(&mut self, x: &[f64], h: &mut [f64]) -> Option<Result<(), EvalError>> {
        h.copy_from_slice(&[
            5. * x[0].powi(4) - 3.75 * x[0] * x[0] + 0.25,
            0.,
            0.,
            -2. + 12. * x[1] * x[1],
        ]);
        Some(Ok(()))
    }
}

#[test]
fn exact_index_one_stationarity_precedes_positive_tangent_recovery() {
    let mut surface = TangentialSaddle;
    let v = 0.5_f64.sqrt();
    let band = neb::Band {
        path: [[0., -v], [-1., 0.], [0., 0.], [1., 0.], [0., v]]
            .into_iter()
            .map(|x| Minimum {
                positions: x.to_vec(),
                energy: evaluate(&mut surface, &x).unwrap().0,
            })
            .collect(),
        converged: false,
        climbing: 2,
        tangent: vec![1., 0.],
    };
    let result = saddle::refine_band(
        &mut surface,
        &band,
        &[],
        &TsOptions::default(),
        &mut proceed,
    )
    .unwrap();
    assert!(result.converged);
    assert_eq!(result.point.positions, [0., 0.]);
    let (values, _) = symmetric_eigen(result.hessian.unwrap(), 2).unwrap();
    assert_eq!(values.iter().filter(|v| **v < 0.).count(), 1);
}

#[test]
fn bracketed_hidden_peak_above_default_headroom_recovers_saddle() {
    let minimum = Minimum {
        positions: vec![0., 0., 0., 1., 0., 0.],
        energy: 0.,
    };
    let band = neb::Band {
        path: vec![
            minimum.clone(),
            minimum,
            Minimum {
                positions: vec![0., 0., 0., 3., 0., 0.],
                energy: 0.,
            },
        ],
        converged: false,
        climbing: 1,
        tangent: vec![-1., 0., 0., 1., 0., 0.],
    };
    let result = saddle::refine_band(
        &mut CarbonStretch(0.08),
        &band,
        &[12.; 2],
        &TsOptions::default(),
        &mut proceed,
    )
    .unwrap();
    assert!(result.converged);
    assert!((result.point.energy - 0.020).abs() < 1e-10);
    let x = &result.point.positions;
    assert!((distance(&x[..3], &x[3..]) - 2.).abs() < 1e-8);
    let (frequencies, _) = modes::frequencies(x, &result.hessian.unwrap(), &[12.; 2]).unwrap();
    assert_eq!(
        frequencies.iter().filter(|f| **f < -IMAGINARY_CM1).count(),
        1
    );
}

struct OptionalMidpointFailure {
    failed: f64,
    error: EvalError,
    calls: Vec<f64>,
}

impl Potential for OptionalMidpointFailure {
    fn evaluate(&mut self, x: &[f64], g: &mut [f64], _: Accuracy) -> Result<f64, EvalError> {
        self.calls.push(x[0]);
        if x[0] == self.failed {
            return Err(self.error.clone());
        }
        g[0] = -4. * x[0] * (x[0] * x[0] - 0.25);
        Ok(-(x[0] * x[0] - 0.25).powi(2))
    }
}

fn optional_midpoint_band() -> neb::Band {
    neb::Band {
        path: [-1.0_f64, 0., 1.]
            .into_iter()
            .map(|x| Minimum {
                positions: vec![x],
                energy: -(x * x - 0.25).powi(2),
            })
            .collect(),
        converged: false,
        climbing: 1,
        tangent: vec![1.],
    }
}

#[test]
fn optional_midpoint_scf_failure_keeps_the_other_link() {
    for failed in [-0.5, 0.5] {
        let mut surface = OptionalMidpointFailure {
            failed,
            error: EvalError::Scf("optional midpoint unavailable".into()),
            calls: Vec::new(),
        };
        let probe = optional_midpoint_band()
            .intermediate_peaks(&mut surface, &[], 1.)
            .expect("an optional midpoint SCF failure discarded the usable link");
        assert_eq!(surface.calls, [-0.5, 0.5]);
        assert_eq!(probe.diagnostics.len(), 1);
        assert!(probe.diagnostics[0].contains("optional midpoint unavailable"));
        let seed = probe.seeds.into_iter().next().unwrap();
        assert_eq!(seed.point.positions, [-failed]);
    }
}

#[test]
fn optional_midpoint_cancellation_and_fatal_failure_propagate() {
    for error in [
        EvalError::Cancelled,
        EvalError::Failed("worker disconnected".into()),
    ] {
        let mut surface = OptionalMidpointFailure {
            failed: -0.5,
            error: error.clone(),
            calls: Vec::new(),
        };
        let result = optional_midpoint_band().intermediate_peaks(&mut surface, &[], 1.);
        assert!(matches!(result, Err(actual) if actual == TsError::from(error)));
        assert_eq!(surface.calls, [-0.5]);
    }
}

struct OptionalSeedHessianFailure {
    failed: f64,
    hessian_calls: Vec<f64>,
}

impl Potential for OptionalSeedHessianFailure {
    fn evaluate(&mut self, x: &[f64], g: &mut [f64], _: Accuracy) -> Result<f64, EvalError> {
        let a = x[0];
        let tilt = self.failed.signum() * 0.001;
        g[0] = a - 4. * a.powi(3) + tilt * (15. * a * a - 60. * a.powi(4));
        Ok(-(a * a - 0.25).powi(2) + tilt * a.powi(3) * (5. - 12. * a * a))
    }

    fn hessian(&mut self, x: &[f64], h: &mut [f64]) -> Option<Result<(), EvalError>> {
        self.hessian_calls.push(x[0]);
        if x[0] == self.failed {
            Some(Err(EvalError::Scf(
                "optional seed Hessian unavailable".into(),
            )))
        } else {
            let a = x[0];
            let tilt = self.failed.signum() * 0.001;
            h[0] = 1. - 12. * a * a + tilt * (30. * a - 240. * a.powi(3));
            Some(Ok(()))
        }
    }
}

#[test]
fn optional_recovery_hessian_scf_failure_tries_the_other_link() {
    for failed in [-0.5, 0.5] {
        let mut surface = OptionalSeedHessianFailure {
            failed,
            hessian_calls: Vec::new(),
        };
        let mut band = optional_midpoint_band();
        for point in &mut band.path {
            point.energy = evaluate(&mut surface, &point.positions).unwrap().0;
        }
        let result = saddle::refine_band(
            &mut surface,
            &band,
            &[],
            &TsOptions::default(),
            &mut proceed,
        )
        .unwrap();
        assert_eq!(surface.hessian_calls, [0., failed, -failed]);
        assert!(result.converged);
        assert_eq!(result.point.positions, [-failed]);
        assert_eq!(result.diagnostics.len(), 1);
        assert!(result.diagnostics[0].contains("optional seed Hessian unavailable"));
    }
}

struct FinalCandidateFailure {
    error: EvalError,
    failed_hessians: usize,
}

impl Potential for FinalCandidateFailure {
    fn evaluate(&mut self, x: &[f64], g: &mut [f64], accuracy: Accuracy) -> Result<f64, EvalError> {
        CarbonStretch(0.02).evaluate(x, g, accuracy)
    }

    fn hessian(&mut self, x: &[f64], _: &mut [f64]) -> Option<Result<(), EvalError>> {
        if distance(&x[..3], &x[3..]) > 1.51 {
            self.failed_hessians += 1;
            Some(Err(self.error.clone()))
        } else {
            None
        }
    }
}

fn final_candidate_failure(
    surface: &mut FinalCandidateFailure,
    masses: &[f64],
) -> Result<saddle::Refinement, TsError> {
    let positions = vec![0., 0., 0., 1.5, 0., 0.];
    let start = Minimum {
        energy: evaluate(surface, &positions).unwrap().0,
        positions,
    };
    let band = neb::Band {
        path: vec![start.clone(), start.clone(), start],
        converged: false,
        climbing: 1,
        tangent: vec![-1., 0., 0., 1., 0., 0.],
    };
    saddle::refine_band(
        surface,
        &band,
        masses,
        &TsOptions {
            saddle_max_steps: 1,
            ..TsOptions::default()
        },
        &mut proceed,
    )
}

#[test]
fn optional_final_candidate_scf_failure_keeps_verified_candidate() {
    let mut surface = FinalCandidateFailure {
        error: EvalError::Scf("provisional Hessian unavailable".into()),
        failed_hessians: 0,
    };
    let result = final_candidate_failure(&mut surface, &[12.; 2])
        .expect("a failed provisional Hessian discarded the earlier verified candidate");
    assert_eq!(
        surface.failed_hessians, 1,
        "the provisional candidate was not checked"
    );
    assert!(!result.converged);
    assert_eq!(result.point.positions, [0., 0., 0., 1.5, 0., 0.]);
    assert!(result.hessian.is_some());
    assert_eq!(result.reason, SaddleReason::AcceptedStepLimit);
    assert_eq!(result.diagnostics.len(), 1);
    assert!(result.diagnostics[0].contains("provisional Hessian unavailable"));
}

#[test]
fn unavailable_final_hessian_without_verified_candidate_propagates_once() {
    let mut surface = FinalCandidateFailure {
        error: EvalError::Scf("provisional Hessian unavailable".into()),
        failed_hessians: 0,
    };
    // These masses put the initial imaginary frequency below the significance
    // threshold, leaving no previously verified index-one fallback.
    let result = final_candidate_failure(&mut surface, &[1e8; 2]);
    assert!(
        matches!(result, Err(TsError::Scf(message)) if message == "provisional Hessian unavailable")
    );
    assert_eq!(surface.failed_hessians, 1);
}

#[test]
fn optional_final_candidate_cancellation_and_fatal_failure_propagate() {
    for error in [
        EvalError::Cancelled,
        EvalError::Failed("worker disconnected".into()),
    ] {
        let mut surface = FinalCandidateFailure {
            error: error.clone(),
            failed_hessians: 0,
        };
        let result = final_candidate_failure(&mut surface, &[12.; 2]);
        assert!(matches!(result, Err(actual) if actual == TsError::from(error)));
        assert_eq!(surface.failed_hessians, 1);
    }
}

fn carbon_recovery_band(surface: &mut impl Potential, neighbors: [f64; 2]) -> neb::Band {
    neb::Band {
        path: [neighbors[0], 1., neighbors[1]]
            .into_iter()
            .map(|r| {
                let positions = vec![0., 0., 0., r, 0., 0.];
                Minimum {
                    energy: evaluate(surface, &positions).unwrap().0,
                    positions,
                }
            })
            .collect(),
        converged: false,
        climbing: 1,
        tangent: vec![-1., 0., 0., 1., 0., 0.],
    }
}

#[test]
fn bracketed_midpoint_handoff_retains_headroom_for_uphill_refinement() {
    let mut surface = CarbonStretch(0.08);
    let band = carbon_recovery_band(&mut surface, [1., 2.8]);
    let midpoint_energy = 0.08 * (0.1_f64.powi(2) - 1.).powi(2) / 4.;
    let options = TsOptions {
        saddle_headroom_kcal: 0.5,
        ..TsOptions::default()
    };
    let original_ceiling = options.saddle_headroom_kcal / KCAL_PER_HARTREE;
    assert!(midpoint_energy > original_ceiling);
    let raised_ceiling = midpoint_energy + original_ceiling;
    let mut accepted_uphill = false;
    let result = saddle::refine_band(&mut surface, &band, &[12.; 2], &options, &mut |event| {
        if let Event::Saddle { step, energy, .. } = event {
            assert!(energy <= raised_ceiling);
            accepted_uphill |= step > 0 && energy > midpoint_energy + 1e-7;
        }
        proceed(event)
    })
    .unwrap();
    assert!(accepted_uphill, "the midpoint must refine further uphill");
    assert!(result.converged);
    assert_eq!(result.reason, SaddleReason::Stationary);
    assert!((result.point.energy - 0.020).abs() < 3e-6);
    assert!(
        (distance(&result.point.positions[..3], &result.point.positions[3..]) - 2.).abs() < 0.007
    );
}

#[test]
fn unbracketed_over_ceiling_handoff_does_not_converge_as_a_minimum() {
    let mut surface = CarbonStretch(0.08);
    // The far endpoint is above the midpoint, so this observed link does not
    // bracket a new peak and cannot raise the original energy reference.
    let band = carbon_recovery_band(&mut surface, [1., 3.5]);
    let result = saddle::refine_band(
        &mut surface,
        &band,
        &[12.; 2],
        &TsOptions::default(),
        &mut proceed,
    )
    .unwrap();
    assert!(!result.converged);
    assert_eq!(result.reason, SaddleReason::EnergyCeiling);
    assert!(
        result
            .diagnostics
            .iter()
            .any(|message| message.contains("right midpoint exceeds"))
    );
    assert_eq!(result.point.positions, band.path[band.climbing].positions);
}

struct MidpointScfStretch {
    failed: f64,
    failures: usize,
}

impl Potential for MidpointScfStretch {
    fn evaluate(&mut self, x: &[f64], g: &mut [f64], accuracy: Accuracy) -> Result<f64, EvalError> {
        if (distance(&x[..3], &x[3..]) - self.failed).abs() < 1e-12 {
            self.failures += 1;
            Err(EvalError::Scf("link midpoint unavailable".into()))
        } else {
            CarbonStretch(0.08).evaluate(x, g, accuracy)
        }
    }
}

#[test]
fn recovery_reports_scf_diagnostics_and_refines_the_other_link() {
    for (failed, side) in [(1.7, "left"), (1.9, "right")] {
        let mut surface = MidpointScfStretch {
            failed,
            failures: 0,
        };
        let band = carbon_recovery_band(&mut surface, [2.4, 2.8]);
        let result = saddle::refine_band(
            &mut surface,
            &band,
            &[12.; 2],
            &TsOptions::default(),
            &mut proceed,
        )
        .unwrap();
        assert_eq!(surface.failures, 1);
        assert!(result.converged);
        assert!((result.point.energy - 0.020).abs() < 3e-6);
        assert_eq!(result.diagnostics.len(), 1);
        assert!(result.diagnostics[0].contains(&format!("{side} midpoint")));
        assert!(result.diagnostics[0].contains("link midpoint unavailable"));
    }
}

#[test]
fn saddle_headroom_configuration_requires_finite_nonnegative_values() {
    let x = [0., 0., 0., 1., 0., 0.];
    for value in [-1., f64::INFINITY, f64::NAN] {
        let options = TsOptions {
            saddle_headroom_kcal: value,
            ..TsOptions::default()
        };
        let input = Search {
            numbers: &[6; 2],
            masses_amu: &[12.; 2],
            reactant: &x,
            product: &x,
            options: &options,
        };
        assert!(
            matches!(input.validate(), Err(TsError::Invalid(message)) if message.contains("headroom"))
        );
    }
    let options = TsOptions {
        saddle_headroom_kcal: 0.,
        ..TsOptions::default()
    };
    assert!(
        Search {
            numbers: &[6; 2],
            masses_amu: &[12.; 2],
            reactant: &x,
            product: &x,
            options: &options,
        }
        .validate()
        .is_ok()
    );
}

#[test]
fn refinement_cannot_leave_the_band_for_a_remote_higher_saddle() {
    // A sparse band hands off a point barely outside the reactant well.
    // The true saddle on this stretch is 0.25 Eh above it.
    let mut surface = CarbonStretch(1.);
    let x = vec![0., 0., 0., 1.03, 0., 0.];
    let (energy, _) = evaluate(&mut surface, &x).unwrap();
    let start = Minimum {
        positions: x,
        energy,
    };
    let limit = energy + 10.0 / KCAL_PER_HARTREE;
    let mut exhausted = false;
    let result = saddle::refine(
        &mut surface,
        &start,
        &[-1., 0., 0., 1., 0., 0.],
        &[12.; 2],
        &TsOptions::default(),
        &mut |event| {
            if let Event::Saddle { energy, .. } = event {
                assert!(energy <= limit, "accepted energy {energy} above {limit}");
            }
            if let Event::SaddleStatus("energy ceiling exhausted") = event {
                exhausted = true;
            }
            proceed(event)
        },
    )
    .unwrap();
    assert!(!result.converged);
    assert!(result.point.energy <= limit);
    assert!(
        exhausted,
        "the unbracketed remote saddle exceeded the energy ceiling"
    );
    assert_eq!(result.reason, SaddleReason::EnergyCeiling);
}

fn carbon_stretch(k: f64, options: TsOptions) -> TsReport {
    search(
        &mut [CarbonStretch(k)],
        &Search {
            numbers: &[6; 2],
            masses_amu: &[12.; 2],
            reactant: &[0., 0., 0., 1., 0., 0.],
            product: &[0., 0., 0., 3., 0., 0.],
            options: &options,
        },
        proceed,
    )
    .unwrap()
}

#[test]
fn below_threshold_imaginary_frequency_keeps_report() {
    // Reduced mass 6 amu. An odd image count puts an image at the exact saddle.
    let k = 6. * (40. / 5_140.487_14_f64).powi(2);
    let report = carbon_stretch(
        k,
        TsOptions {
            neb_max_iterations: 0,
            ..Default::default()
        },
    );
    assert!(report.saddle_converged);
    assert_eq!(report.imaginary, 0);
    assert!((report.frequencies_cm1[0] + 40.).abs() < 0.01);
    assert!(report.irc.is_none() && !report.connects);
}

#[test]
fn exhausted_refinement_with_one_imaginary_mode_skips_irc() {
    let report = carbon_stretch(
        0.2,
        TsOptions {
            images: 16,
            neb_max_iterations: 0,
            saddle_max_steps: 0,
            ..Default::default()
        },
    );
    assert!(!report.saddle_converged);
    assert_eq!(report.imaginary, 1);
    assert!(report.frequencies_cm1[0] < -IMAGINARY_CM1);
    assert!(report.irc.is_none() && !report.connects);
}

#[test]
fn soft_saddle_refines_until_both_irc_launches_descend() {
    let report = carbon_stretch(
        0.002,
        TsOptions {
            images: 16,
            neb_max_iterations: 0,
            ..Default::default()
        },
    );
    assert!(report.saddle_converged);
    let stretch = |p: &Minimum| distance(&p.positions[..3], &p.positions[3..]);
    assert!((stretch(&report.ts) - 2.).abs() * 6_f64.sqrt() < 0.015);
    let first = report.irc.unwrap().map(|branch| {
        assert!(branch.completed());
        let end_offset = stretch(branch.path.last().unwrap()) - 2.;
        assert!(
            3. * end_offset.powi(2) > 1.,
            "trace stopped on the concave side of the well"
        );
        let first = &branch.path[0];
        assert!(first.energy < report.ts.energy);
        stretch(first) - 2.
    });
    assert!(first[0] * first[1] < 0.);
}

#[test]
fn search_rejects_coincident_nuclei_in_either_endpoint() {
    let valid = [0., 0., 0., 0.1, 0., 0.];
    let invalid = [0., 0., 0., 0.099, 0., 0.];
    let base = Search {
        numbers: &[1; 2],
        masses_amu: &[1.; 2],
        reactant: &valid,
        product: &valid,
        options: &TsOptions::default(),
    };
    assert!(base.validate().is_ok());
    for input in [
        Search {
            reactant: &invalid,
            ..base
        },
        Search {
            product: &invalid,
            ..base
        },
    ] {
        assert!(matches!(input.validate(), Err(TsError::Invalid(_))));
    }
}
