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
        let (point, converged) = saddle::refine(
            &mut MullerBrown,
            &guess,
            &tangent,
            false,
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
            let (end, energies) = descent
                .trace(&mut MullerBrown, forward, &mut proceed)
                .unwrap();
            assert!(energies.len() > 2 && energies.windows(2).all(|e| e[1] < e[0]));
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
    for branch in &one.irc {
        assert_eq!(branch.energies[0], one.ts.energy);
        assert!(branch.energies[1] < one.ts.energy);
        assert_eq!(branch.energies.last(), Some(&branch.end.energy));
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
    let report = triangle_inputs(&[1; 3], &options, false, 1, proceed).unwrap();
    assert!(!report.neb_converged);
    assert!(report.saddle_converged && report.connects);
    assert!(report.hessians > 2);
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
    let x = [0.; 6];
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
        bonds(&[6; 3], &report.reactant.positions),
        bonds(&[6; 3], &report.product.positions)
    );
    assert!(report.connects);
    for branch in report.irc {
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
fn stationary_minimum_returns_no_saddle() {
    let x = triangle(1.1);
    let input = Search {
        numbers: &[1; 3],
        masses_amu: &[1.; 3],
        reactant: &x,
        product: &x,
        options: &TsOptions::default(),
    };
    assert_eq!(
        search(&mut [Triangle::default()], &input, proceed).unwrap_err(),
        TsError::NoSaddle
    );
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
    assert_eq!(
        report
            .frequencies_cm1
            .iter()
            .filter(|&&f| f < -IMAGINARY_CM1)
            .count(),
        2
    );
}
