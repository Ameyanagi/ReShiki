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
    let result = neb::run(&mut [MullerBrown], band, ends, &[], &options, &mut proceed).unwrap();
    let highest = &result.path[result.climbing];
    assert!(result.converged);
    assert!(
        distance(&highest.positions, &UPPER_SADDLE) < 0.005,
        "{highest:?}"
    );
    assert!((highest.energy + 40.665).abs() < 0.005, "{highest:?}");
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
        assert!(branch.completed);
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
