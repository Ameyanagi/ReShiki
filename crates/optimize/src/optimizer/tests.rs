//! Optimizer state transitions, rigid modes, and convergence contract tests.

use super::*;

struct Surface<F>(F);

impl<F: FnMut(&[f64], &mut [f64], Accuracy) -> Result<f64, EvalError>> Potential for Surface<F> {
    fn evaluate(&mut self, x: &[f64], g: &mut [f64], a: Accuracy) -> Result<f64, EvalError> {
        self.0(x, g, a)
    }
}

pub(super) fn surface(
    f: impl FnMut(&[f64], &mut [f64], Accuracy) -> Result<f64, EvalError>,
) -> impl Potential {
    Surface(f)
}

const INITIAL: [f64; 6] = [-0., 0., 0., 3., -1., 2.];

fn anchored(method: Method) -> Options {
    Options {
        method,
        fixed_atoms: vec![0],
        ..Options::default()
    }
}

fn anchored_tight(method: Method) -> Options {
    Options {
        tight_final: true,
        ..anchored(method)
    }
}

fn well(x: &[f64], g: &mut [f64]) -> f64 {
    g.copy_from_slice(x);
    dot(x, x) * 0.5
}

fn proceed(_: &Step) -> ControlFlow<()> {
    ControlFlow::Continue(())
}

#[test]
fn default_internal_step_preserves_fixed_atoms_and_can_be_cancelled() {
    assert_eq!(Method::default(), Method::Internal);
    let options = Options {
        fixed_atoms: vec![0],
        ..Options::default()
    };
    assert_eq!(options.method, Method::Internal);

    let initial: [f64; 6] = [-0., 0., -0., 1.8, 0.1, 0.];
    let check_fixed = |x: &[f64]| {
        for (actual, expected) in x[..3].iter().zip(&initial[..3]) {
            assert_eq!(actual.to_bits(), expected.to_bits());
        }
    };
    let mut p = surface(|x, g, _| {
        check_fixed(x);
        g.fill(0.);
        g[3] = x[3] - 1.4;
        Ok(0.5 * g[3].powi(2))
    });
    let mut accepted = Vec::new();
    let result = optimize(&mut p, &[1, 1], &initial, &options, |step| {
        check_fixed(step.positions);
        if step.index == 1 {
            accepted.extend_from_slice(step.positions);
            ControlFlow::Break(())
        } else {
            proceed(step)
        }
    });
    assert_eq!(result.status, Status::Cancelled);
    assert_eq!(result.method, Method::Internal);
    assert_eq!(result.steps, 1);
    assert_eq!(result.positions, accepted);
    assert!(result.positions[3] < initial[3]);
    check_fixed(&result.positions);
}

#[test]
fn invalid_input_never_reaches_the_potential_or_callback() {
    for (z, x, fixed) in [
        (vec![], vec![], vec![]),
        (vec![0], vec![0.; 3], vec![]),
        (vec![119], vec![0.; 3], vec![]),
        (vec![1], vec![0.; 2], vec![]),
        (vec![1], vec![0., f64::INFINITY, 0.], vec![]),
        (vec![1, 1], vec![0.; 6], vec![2]),
        (vec![1, 1], vec![0.; 6], vec![0, 0]),
        (vec![1, 1], vec![0.; 6], vec![0, 1]),
    ] {
        let mut p = surface(|_, _, _| panic!("invalid input was evaluated"));
        let opts = Options {
            fixed_atoms: fixed,
            ..Options::default()
        };
        let result = optimize(&mut p, &z, &x, &opts, |_| panic!("invalid input callback"));
        assert!(matches!(result.status, Status::Failed(_)));
        assert_eq!(result.evaluations, 0);
        assert!(result.methods.is_empty());
        assert!(result.energy.is_nan());
    }
}

#[test]
fn callback_can_cancel_at_initial_or_accepted_points() {
    for method in [Method::AncRfo, Method::Internal, Method::Lbfgs] {
        for stop in [0, 3] {
            let mut p = surface(|x, g, _| Ok(well(x, g)));
            let mut accepted = Vec::new();
            let result = optimize(&mut p, &[1, 1], &INITIAL, &anchored(method), |s| {
                assert_eq!(s.index, accepted.len());
                accepted.push((s.positions.to_vec(), s.energy));
                if s.index == stop {
                    ControlFlow::Break(())
                } else {
                    proceed(s)
                }
            });
            assert_eq!(result.status, Status::Cancelled);
            assert_eq!(result.steps, stop);
            assert_eq!(
                result.methods.first().copied(),
                (stop > 0).then_some(method)
            );
            assert_eq!(result.energies.len(), stop + 1);
            assert_eq!(result.positions, accepted[stop].0);
            assert_eq!(result.energy, accepted[stop].1);
            assert_eq!(result.gradient, result.positions);
        }
    }
}

#[test]
fn failed_evaluations_preserve_the_last_accepted_sample() {
    for method in [Method::AncRfo, Method::Internal, Method::Lbfgs] {
        for (error, status) in [
            (
                EvalError::Scf("SCF".into()),
                Status::ScfFailure("SCF".into()),
            ),
            (EvalError::Cancelled, Status::Cancelled),
            (
                EvalError::Failed("worker".into()),
                Status::Failed("worker".into()),
            ),
        ] {
            for fail_at in [1, 2, 4] {
                let mut calls = 0;
                let mut p = surface(|x, g, _| {
                    calls += 1;
                    if calls == fail_at {
                        g.fill(123.);
                        Err(error.clone())
                    } else {
                        Ok(well(x, g))
                    }
                });
                let mut last = Vec::new();
                let result = optimize(&mut p, &[1, 1], &INITIAL, &anchored(method), |s| {
                    last = s.positions.to_vec();
                    proceed(s)
                });
                assert_eq!(result.status, status);
                assert_eq!(result.evaluations, fail_at);
                assert_eq!(
                    result.methods.first().copied(),
                    (fail_at > 1).then_some(method)
                );
                if fail_at == 1 {
                    assert_eq!(result.positions, INITIAL);
                    assert!(result.energy.is_nan());
                    assert!(result.energies.is_empty());
                } else {
                    assert_eq!(result.positions, last);
                    assert_eq!(result.gradient, last);
                    assert_eq!(result.energy, 0.5 * dot(&last, &last));
                }
            }
        }
    }
}

#[test]
fn input_convergence_does_not_record_an_optimizer_algorithm() {
    for method in [Method::Internal, Method::AncRfo, Method::Lbfgs] {
        for adaptive_accuracy in [false, true] {
            for tight_final in [false, true] {
                let mut p = surface(|_, g, _| {
                    g.fill(0.);
                    Ok(0.)
                });
                let options = Options {
                    adaptive_accuracy,
                    tight_final,
                    initial_hessian: InitialHessian::Potential,
                    ..anchored(method)
                };
                let out = optimize(&mut p, &[1, 1], &INITIAL, &options, proceed);
                assert_eq!(out.status, Status::Converged);
                assert_eq!(out.steps, 0);
                assert_eq!(
                    out.evaluations,
                    1 + usize::from(adaptive_accuracy) + usize::from(tight_final)
                );
                assert_eq!(out.hessian_evaluations, 0);
                assert_eq!(out.method, method);
                assert!(out.methods.is_empty());
            }
        }
    }
}

#[test]
fn nonfinite_or_unwritten_results_fail_even_on_fixed_atoms() {
    for case in 0..3 {
        let mut p = surface(|_, g, _| {
            if case == 0 {
                g.fill(0.);
                return Ok(f64::INFINITY);
            }
            g[1..].fill(0.);
            if case == 1 {
                g[0] = f64::NEG_INFINITY;
            }
            Ok(0.)
        });
        let result = optimize(
            &mut p,
            &[1, 1],
            &INITIAL,
            &anchored(Method::AncRfo),
            proceed,
        );
        assert!(matches!(result.status, Status::Failed(_)));
        assert_eq!(result.evaluations, 1);
        assert!(result.energy.is_nan());
    }
}

#[test]
fn tight_final_is_opt_in() {
    let run = |options: &Options| {
        let mut calls = Vec::new();
        let mut p = surface(|x, g, accuracy| {
            calls.push((x.to_vec(), accuracy));
            let bond: [f64; 3] = std::array::from_fn(|i| x[3 + i] - x[i]);
            let length = norm(&bond);
            let displacement = length - 1.4;
            for (i, component) in bond.into_iter().enumerate() {
                g[3 + i] = displacement * component / length;
                g[i] = -g[3 + i];
            }
            Ok(0.5 * displacement.powi(2))
        });
        let initial = [0., 0., 0., 2., 0., 0.];
        let result = optimize(&mut p, &[1, 1], &initial, options, proceed);
        drop(p);
        (result, calls)
    };

    let options = Options::default();
    assert!(!options.tight_final);
    assert_eq!(options.convergence, Convergence::Standard);
    let (normal, mut calls) = run(&options);
    assert_eq!(normal.status, Status::Converged);
    assert!(normal.steps > 0);
    assert!(
        calls
            .iter()
            .all(|(_, accuracy)| *accuracy == Accuracy::Normal)
    );

    let (tight, tight_calls) = run(&Options {
        tight_final: true,
        ..options
    });
    assert_eq!(tight.status, Status::Converged);
    assert_eq!(tight.positions, normal.positions);
    calls.push((normal.positions, Accuracy::Tight));
    assert_eq!(tight_calls, calls);
}

#[test]
fn tight_recheck_can_resume_and_counts_once_at_the_same_point() {
    for method in [Method::AncRfo, Method::Internal, Method::Lbfgs] {
        let mut calls = Vec::new();
        let mut callbacks = Vec::new();
        let mut p = surface(|x, g, a| {
            calls.push((x.to_vec(), a));
            g.fill(0.);
            if a == Accuracy::Normal {
                Ok(2.)
            } else {
                g[3] = x[3] - 2.;
                Ok(0.5 * g[3] * g[3] - 4.)
            }
        });
        let result = optimize(&mut p, &[1, 1], &INITIAL, &anchored_tight(method), |s| {
            callbacks.push(s.accuracy);
            proceed(s)
        });
        drop(p);
        assert_eq!(result.status, Status::Converged);
        assert_eq!(result.evaluations, calls.len());
        assert_eq!(calls[0], (INITIAL.to_vec(), Accuracy::Normal));
        assert_eq!(calls[1], (INITIAL.to_vec(), Accuracy::Tight));
        assert!(calls[1..].iter().all(|(_, a)| *a == Accuracy::Tight));
        assert_eq!(callbacks.len(), result.steps + 2);
        assert!(callbacks[1..].iter().all(|a| *a == Accuracy::Tight));
        assert_eq!(result.gradient[3], result.positions[3] - 2.);
        assert_eq!(result.energy, 0.5 * result.gradient[3].powi(2) - 4.);
        assert!((result.positions[3] - 2.).abs() < 1e-5);
        assert_eq!(result.energies[0], -3.5);
        assert_eq!(result.energies.last(), Some(&result.energy));
    }
}

#[test]
fn tight_recheck_reports_the_replacement_sample_before_convergence() {
    let mut p = surface(|_, g, a| {
        g.fill(0.);
        Ok(if a == Accuracy::Normal { 2. } else { -4. })
    });
    let mut callbacks = Vec::new();
    let options = Options {
        tight_final: true,
        ..Options::default()
    };
    let result = optimize(&mut p, &[1, 1], &INITIAL, &options, |s| {
        callbacks.push((s.index, s.accuracy, s.energy, s.positions.to_vec()));
        assert_eq!(s.delta_energy, 0.);
        assert_eq!(s.max_step, 0.);
        assert_eq!(s.rms_step, 0.);
        proceed(s)
    });
    assert_eq!(result.status, Status::Converged);
    assert_eq!((result.steps, result.evaluations), (0, 2));
    assert_eq!(
        callbacks,
        [
            (0, Accuracy::Normal, 2., INITIAL.to_vec()),
            (0, Accuracy::Tight, -4., INITIAL.to_vec()),
        ]
    );
    assert_eq!(result.energies, [-4.]);
}

#[test]
fn callback_can_cancel_a_tight_recheck_at_initial_or_accepted_points() {
    struct TightProbe {
        normal_gradient: f64,
        tight_gradient: f64,
    }
    impl Potential for TightProbe {
        fn evaluate(&mut self, x: &[f64], g: &mut [f64], a: Accuracy) -> Result<f64, EvalError> {
            g.fill(0.);
            if a == Accuracy::Tight {
                g[3] = self.tight_gradient;
                Ok(self.tight_gradient * x[3] - 4.)
            } else {
                let dx = x[3] - INITIAL[3];
                g[3] = self.normal_gradient + 0.5 * dx;
                Ok(self.normal_gradient * x[3] + 0.25 * dx * dx + 2.)
            }
        }
        fn hessian(&mut self, _: &[f64], h: &mut [f64]) -> Option<Result<(), EvalError>> {
            h.fill(0.);
            h[3 * 6 + 3] = 0.5;
            Some(Ok(()))
        }
    }
    // The exact Normal curvature gives all three methods a small first step
    // with no rejected trials, so the intended Tight callback has index one.
    for method in [Method::AncRfo, Method::Internal, Method::Lbfgs] {
        for (index, normal_gradient) in [(0, 0.), (1, 4e-5)] {
            for tight_gradient in [0., 0.01] {
                let mut p = TightProbe {
                    normal_gradient,
                    tight_gradient,
                };
                let options = Options {
                    initial_hessian: InitialHessian::Potential,
                    ..anchored_tight(method)
                };
                let mut last_normal = Vec::new();
                let result = optimize(&mut p, &[1, 1], &INITIAL, &options, |s| {
                    if s.accuracy == Accuracy::Tight {
                        assert_eq!(s.index, index);
                        assert_eq!(s.positions, last_normal);
                        assert_eq!(s.energy, tight_gradient * s.positions[3] - 4.);
                        ControlFlow::Break(())
                    } else {
                        last_normal = s.positions.to_vec();
                        proceed(s)
                    }
                });
                assert_eq!(
                    result.status,
                    Status::Cancelled,
                    "{method:?}: index {index}, Tight gradient {tight_gradient}"
                );
                assert_eq!((result.steps, result.evaluations), (index, index + 2));
                assert_eq!(result.positions, last_normal);
                assert_eq!(result.energy, tight_gradient * result.positions[3] - 4.);
                assert_eq!(result.gradient[3], tight_gradient);
                assert_eq!(result.energies.len(), index + 1);
                assert_eq!(result.energies[index], result.energy);
            }
        }
    }
}

#[test]
fn tight_resume_preserves_anc_curvature_and_radius_but_clears_lbfgs_history() {
    for method in [Method::AncRfo, Method::Lbfgs] {
        let mut normal_calls = 0;
        let mut tight_calls = 0;
        let mut p = surface(|_, g, a| {
            g.fill(0.);
            let (calls, initial_gradient, offset) = match a {
                Accuracy::Coarse => unreachable!("adaptive accuracy is disabled"),
                Accuracy::Normal => (&mut normal_calls, 0.2, 1.),
                Accuracy::Tight => (&mut tight_calls, 0.01, -1.),
            };
            *calls += 1;
            if *calls == 1 {
                g[3] = initial_gradient;
            }
            Ok(offset - (*calls - 1) as f64)
        });
        let mut callbacks = Vec::new();
        let result = optimize(&mut p, &[1, 1], &INITIAL, &anchored_tight(method), |s| {
            callbacks.push((s.index, s.accuracy, s.positions.to_vec(), s.trust_radius));
            proceed(s)
        });
        assert_eq!(result.status, Status::Converged);
        assert_eq!((result.steps, result.evaluations), (2, 4));
        assert_eq!(result.method, method);
        assert_eq!(callbacks.len(), 4);
        assert_eq!((callbacks[1].0, callbacks[1].1), (1, Accuracy::Normal));
        assert_eq!((callbacks[2].0, callbacks[2].1), (1, Accuracy::Tight));
        assert_eq!(callbacks[1].2, callbacks[2].2);
        let normal_displacement = callbacks[1].2[3] - INITIAL[3];
        assert!((normal_displacement + 0.3).abs() < 1e-12);
        let tight_displacement = result.positions[3] - callbacks[2].2[3];
        let expected = match method {
            Method::AncRfo => {
                // The first accepted step learns this curvature. A fresh Lindh
                // model would instead take a capped, much larger Tight step.
                let curvature = -0.2 / normal_displacement;
                assert_eq!(callbacks[3].3, 0.5);
                -0.02 / (curvature + (curvature * curvature + 0.0004).sqrt())
            }
            Method::Lbfgs => {
                // Retaining the Normal pair would use its learned inverse
                // curvature instead of the initial inverse curvature of two.
                -2. * 0.01
            }
            Method::Internal => unreachable!("this test checks ANC and L-BFGS curvature formulas"),
        };
        assert!((tight_displacement - expected).abs() < 1e-12);
        assert_eq!(result.energies, [1., -1., -2.]);
    }
}

#[test]
fn tight_failure_keeps_the_normal_sample() {
    let mut p = surface(|_, g, a| {
        g.fill(0.);
        match a {
            Accuracy::Coarse => unreachable!("adaptive accuracy is disabled"),
            Accuracy::Normal => Ok(3.),
            Accuracy::Tight => Err(EvalError::Scf("tight".into())),
        }
    });
    let options = Options {
        tight_final: true,
        ..Options::default()
    };
    let result = optimize(&mut p, &[1, 1], &INITIAL, &options, proceed);
    assert_eq!(result.status, Status::ScfFailure("tight".into()));
    assert_eq!(result.steps, 0);
    assert_eq!(result.evaluations, 2);
    assert_eq!(result.energy, 3.);
    assert_eq!(result.energies, [3.]);

    let result = optimize(&mut p, &[1, 1], &INITIAL, &Options::default(), proceed);
    assert_eq!(result.status, Status::Converged);
    assert_eq!(result.evaluations, 1);
}

#[test]
fn anc_rejections_retry_from_the_accepted_point_down_to_the_minimum_radius() {
    let mut calls = Vec::new();
    let mut p = surface(|x, g, _| {
        calls.push(x.to_vec());
        let reject = (2..=6).contains(&calls.len());
        Ok(well(x, g) + if reject { 10. } else { 0. })
    });
    let opts = Options {
        max_steps: 1,
        ..anchored(Method::AncRfo)
    };
    let mut accepted = Vec::new();
    let result = optimize(&mut p, &[1, 1], &INITIAL, &opts, |s| {
        accepted.push(s.positions.to_vec());
        proceed(s)
    });
    drop(p);
    assert_eq!(result.status, Status::MaxSteps);
    // The sixth trial is clamped to the minimum radius and accepted by ANC.
    assert_eq!(result.method, Method::AncRfo);
    assert_eq!(result.evaluations, 7);
    assert_eq!(accepted, [INITIAL.to_vec(), calls[6].clone()]);
    let length = |x: &[f64]| {
        let delta: Vec<_> = x.iter().zip(INITIAL).map(|(v, i)| v - i).collect();
        crate::linalg::norm(&delta)
    };
    for (retry, x) in calls[1..6].iter().enumerate() {
        assert!((length(x) - 0.3 * 0.25_f64.powi(retry as i32)).abs() < 1e-12);
    }
    assert!((length(&calls[6]) - 1e-3).abs() < 1e-12);
}

#[test]
fn anc_retries_at_the_minimum_radius_before_switching_methods() {
    // A stiff bond: the first steps overshoot; a 0.001 bohr step stays within
    // the 1e-6 Eh rejection tolerance and is accepted.
    let mut p = surface(|x, g, _| {
        let (dx, y) = (x[3] - 1.4, x[4]);
        g.fill(0.);
        g[3] = 100_000. * dx + 5e-6;
        g[4] = 0.001 * y + 3.5e-6;
        Ok(50_000. * dx * dx + 5e-6 * dx + 0.0005 * y * y + 3.5e-6 * y)
    });
    let options = Options {
        tight_final: false,
        ..anchored(Method::AncRfo)
    };
    let result = optimize(
        &mut p,
        &[1, 1],
        &[0., 0., 0., 1.4, 0., 0.],
        &options,
        proceed,
    );
    assert_eq!(result.status, Status::Converged);
}

#[test]
fn anc_rejection_does_not_repeat_an_interior_trial() {
    let mut evaluated = Vec::new();
    let mut p = surface(|x, g, _| {
        let positions: [f64; 6] = x.try_into().unwrap();
        assert!(!evaluated.contains(&positions), "repeated trial at {x:?}");
        evaluated.push(positions);
        let bond: [f64; 3] = std::array::from_fn(|i| x[3 + i] - x[i]);
        let r = crate::linalg::norm(&bond);
        let d = r - 1.4;
        for (i, component) in bond.into_iter().enumerate() {
            g[3 + i] = (20. * d + 0.00027) * component / r;
            g[i] = -g[3 + i];
        }
        Ok(10. * d * d + 0.00027 * d)
    });
    let options = Options {
        tight_final: false,
        ..anchored(Method::AncRfo)
    };
    let result = optimize(
        &mut p,
        &[1, 1],
        &[0., 0., 0., 1.4, 0., 0.],
        &options,
        proceed,
    );
    assert_eq!(result.status, Status::Converged);
}

#[test]
fn rigid_gradient_is_removed_even_for_oblique_linear_geometries() {
    for (x, rank) in [
        (vec![1., 2., 3.], 3),
        (vec![1., 2., 3., 2., 4., 2., 4., 8., 0.], 5),
        (vec![1., 2., 3., 2., 4., 2., 4., 8., 1.], 6),
    ] {
        let z = vec![6; x.len() / 3];
        let coords = Coordinates::new(&z, &x, &[]).unwrap();
        assert_eq!(coords.rigid(&x).len(), rank);
        let mut p = surface(|x, g, _| {
            for (r, v) in x.as_chunks::<3>().0.iter().zip(g.as_chunks_mut::<3>().0) {
                v.copy_from_slice(&[
                    1. + r[1] - r[2],
                    2. - r[0] + 2. * r[2],
                    3. + r[0] - 2. * r[1],
                ]);
            }
            Ok(1.)
        });
        let opts = Options {
            max_steps: 0,
            ..Options::default()
        };
        let result = optimize(&mut p, &z, &x, &opts, |s| {
            assert!(s.max_gradient < 1e-13);
            proceed(s)
        });
        assert_eq!(result.status, Status::Converged);
        assert_eq!(result.evaluations, 1);
        assert_eq!(result.steps, 0);
        assert!(result.gradient.iter().any(|v| v.abs() >= 1.));
    }
}

#[test]
fn convergence_requires_both_gradient_tests_and_an_accepted_step_or_small_gradient() {
    let c = Level::Normal.criteria();
    let mut s = Step {
        index: 0,
        energy: 0.,
        delta_energy: c.energy,
        max_gradient: c.max_gradient,
        rms_gradient: c.rms_gradient,
        max_step: c.max_step,
        rms_step: c.rms_step,
        trust_radius: 0.3,
        on_boundary: false,
        ratio: f64::NAN,
        accuracy: Accuracy::Normal,
        positions: &[],
    };
    assert!(!s.converged(c, false));
    assert!(s.converged(c, true));
    assert!(
        !Step {
            rms_gradient: 1.01 * c.rms_gradient,
            ..s
        }
        .converged(c, true)
    );
    assert!(
        !Step {
            max_gradient: 1.01 * c.max_gradient,
            ..s
        }
        .converged(c, true)
    );
    assert!(
        !Step {
            delta_energy: -1.01 * c.energy,
            ..s
        }
        .converged(c, true)
    );
    assert!(
        !Step {
            max_step: 1.01 * c.max_step,
            ..s
        }
        .converged(c, true)
    );
    assert!(
        !Step {
            rms_step: 1.01 * c.rms_step,
            ..s
        }
        .converged(c, true)
    );
    s.max_gradient = c.max_gradient / 100.;
    assert!(!s.converged(c, false));
    s.max_gradient *= 0.99;
    assert!(s.converged(c, false));
}

#[test]
fn singular_model_falls_back_without_evaluating_the_input_twice() {
    let mut p = surface(|x, g, _| {
        g.fill(0.);
        g[3] = x[3] - 1.;
        Ok(0.5 * g[3].powi(2))
    });
    for tight_final in [false, true] {
        let options = Options {
            tight_final,
            ..anchored(Method::AncRfo)
        };
        let mut last = Accuracy::Normal;
        let result = optimize(&mut p, &[1, 1], &[0.; 6], &options, |s| {
            last = s.accuracy;
            proceed(s)
        });
        assert_eq!(result.status, Status::Converged);
        assert_eq!(result.method, Method::Lbfgs);
        assert_eq!(result.methods, [Method::AncRfo, Method::Lbfgs]);
        assert!((result.positions[3] - 1.).abs() < 1e-10);
        // Each capped descent step is accepted, plus the input and the opt-in recheck.
        assert_eq!(
            result.evaluations,
            result.steps + 1 + usize::from(tight_final)
        );
        let expected = if tight_final {
            Accuracy::Tight
        } else {
            Accuracy::Normal
        };
        assert_eq!(last, expected);
    }
}

#[test]
fn failed_first_fallback_trial_records_every_attempted_optimizer() {
    for (method, methods) in [
        (
            Method::Internal,
            vec![Method::Internal, Method::AncRfo, Method::Lbfgs],
        ),
        (Method::AncRfo, vec![Method::AncRfo, Method::Lbfgs]),
        (Method::Lbfgs, vec![Method::Lbfgs]),
    ] {
        let mut calls = 0;
        let mut p = surface(|_, g, _| {
            calls += 1;
            if calls == 2 {
                return Err(EvalError::Scf("trial failed".into()));
            }
            g.fill(0.);
            g[3] = -1.;
            Ok(0.5)
        });
        // Coincident atoms prevent both internal and ANC model construction,
        // so the first evaluated trial belongs to the L-BFGS fallback.
        let out = optimize(&mut p, &[1, 1], &[0.; 6], &anchored(method), proceed);
        assert_eq!(out.status, Status::ScfFailure("trial failed".into()));
        assert_eq!((out.steps, out.evaluations), (0, 2));
        assert_eq!(out.method, Method::Lbfgs);
        assert_eq!(out.methods, methods);
        assert_eq!(out.positions, [0.; 6]);
        assert_eq!(out.energies, [0.5]);
    }
}

#[test]
fn tight_final_reuses_the_normal_displacement_with_a_fresh_tight_gradient() {
    let mut p = surface(|x, g, a| {
        g.fill(0.);
        g[3] = 4e-5;
        Ok(g[3] * x[3] + if a == Accuracy::Tight { 100. } else { 0. })
    });
    let mut callbacks = Vec::new();
    let options = anchored_tight(Method::Lbfgs);
    let result = optimize(&mut p, &[1, 1], &INITIAL, &options, |s| {
        callbacks.push((s.index, s.accuracy, s.rms_gradient, s.delta_energy));
        proceed(s)
    });
    assert_eq!(result.status, Status::Converged);
    assert_eq!((result.steps, result.evaluations), (1, 3));
    assert_eq!(callbacks.len(), 3);
    let (index, accuracy, rms, de) = callbacks[2];
    assert_eq!(index, 1);
    assert_eq!(accuracy, Accuracy::Tight);
    assert_eq!(callbacks[1].0, index);
    assert_eq!(callbacks[1].1, Accuracy::Normal);
    assert_eq!(callbacks[1].2, rms);
    assert_eq!(de, 0.);
    assert!((rms - 4e-5 / 3f64.sqrt()).abs() < 1e-18);
    assert!(callbacks[1].3.abs() < Level::Normal.criteria().energy);
    assert_eq!(result.energy, 100. + result.positions[3] * 4e-5);
}

#[test]
fn energy_gradient_small_gradient_converges_at_a_constant_minimum() {
    let mut p = surface(|_, g, _| {
        g.fill(0.);
        Ok(0.)
    });
    let options = Options {
        convergence: Convergence::EnergyGradient {
            gradient_norm: 1e-3,
        },
        tight_final: true,
        ..Options::default()
    };
    let mut trace = Vec::new();
    let out = optimize(&mut p, &[1], &[0.; 3], &options, |s| {
        trace.push((s.index, s.accuracy));
        proceed(s)
    });
    assert_eq!(out.status, Status::Converged);
    assert_eq!((out.steps, out.evaluations), (0, 2));
    assert!(out.methods.is_empty());
    assert_eq!(out.positions, [0.; 3]);
    assert_eq!(trace, [(0, Accuracy::Normal), (0, Accuracy::Tight)]);
}

#[test]
fn energy_gradient_small_gradient_converges_after_one_harmonic_step() {
    let mut p = surface(|x, g, a| {
        g.fill(0.);
        g[3] = 0.5 * x[3];
        Ok(0.25 * x[3].powi(2) + if a == Accuracy::Tight { 100. } else { 0. })
    });
    let options = Options {
        convergence: Convergence::EnergyGradient {
            gradient_norm: 1e-3,
        },
        max_steps: 1,
        ..anchored_tight(Method::Lbfgs)
    };
    let mut trace = Vec::new();
    let out = optimize(
        &mut p,
        &[1, 1],
        &[0., 0., 0., 0.01, 0., 0.],
        &options,
        |s| {
            trace.push((s.index, s.accuracy, s.delta_energy));
            proceed(s)
        },
    );
    assert_eq!(out.status, Status::Converged);
    assert_eq!((out.steps, out.evaluations), (1, 3));
    assert_eq!(out.positions, [0.; 6]);
    assert_eq!(out.energy, 100.);
    assert!(trace[1].2 < -Level::Normal.criteria().energy);
    assert_eq!(trace[2], (1, Accuracy::Tight, 0.));
}

#[test]
fn energy_gradient_small_gradient_rechecks_the_threshold_and_norm_at_tight() {
    let threshold = Level::Normal.criteria().max_gradient / 100.;
    for (tight_gradient, gradient_norm, status) in [
        (0.99 * threshold, 1e-3, Status::Converged),
        (threshold, 1e-3, Status::MaxSteps),
        (0.99 * threshold, 0.99 * threshold, Status::MaxSteps),
    ] {
        let mut p = surface(|_, g, a| {
            g.fill(0.);
            g[3] = if a == Accuracy::Tight {
                tight_gradient
            } else {
                0.
            };
            Ok(0.)
        });
        let options = Options {
            convergence: Convergence::EnergyGradient { gradient_norm },
            max_steps: 0,
            ..anchored_tight(Method::Lbfgs)
        };
        let out = optimize(&mut p, &[1, 1], &INITIAL, &options, proceed);
        assert_eq!(out.status, status, "{tight_gradient}, {gradient_norm}");
        assert_eq!((out.steps, out.evaluations), (0, 2));
        assert_eq!(out.gradient[3], tight_gradient);
    }
}

#[test]
fn energy_gradient_small_gradient_failed_tight_recheck_requires_a_fresh_pair() {
    let mut p = surface(|x, g, a| {
        g.fill(0.);
        if a == Accuracy::Tight {
            g[3] = 1e-5;
            Ok(100. + 1e-5 * x[3])
        } else {
            g[3] = 0.5 * x[3];
            Ok(0.25 * x[3].powi(2))
        }
    });
    let options = Options {
        convergence: Convergence::EnergyGradient {
            gradient_norm: 1.5e-5,
        },
        max_steps: 3,
        ..anchored_tight(Method::Lbfgs)
    };
    let mut trace = Vec::new();
    let out = optimize(
        &mut p,
        &[1, 1],
        &[0., 0., 0., 0.002, 0., 0.],
        &options,
        |s| {
            trace.push((s.index, s.accuracy));
            proceed(s)
        },
    );
    assert_eq!(out.status, Status::Converged);
    assert_eq!((out.steps, out.evaluations), (3, 5));
    assert_eq!(trace[1..3], [(1, Accuracy::Normal), (1, Accuracy::Tight)]);
    assert_eq!(trace[3..], [(2, Accuracy::Tight), (3, Accuracy::Tight)]);
}

#[test]
fn energy_gradient_requires_two_consecutive_accepted_steps_and_resets_on_failure() {
    let mut calls = 0;
    let mut p = surface(|x, g, _| {
        calls += 1;
        g.fill(0.);
        // Step two fails only the additional norm bound. The level's maximum
        // and RMS gradient tests pass throughout.
        g[3] = if calls == 3 { 2e-5 } else { 1e-5 };
        Ok(1e-5 * x[3])
    });
    let options = Options {
        convergence: Convergence::EnergyGradient {
            gradient_norm: 1.5e-5,
        },
        tight_final: false,
        ..anchored(Method::Lbfgs)
    };
    let mut accepted = Vec::new();
    let result = optimize(&mut p, &[1, 1], &INITIAL, &options, |s| {
        accepted.push(s.index);
        proceed(s)
    });
    assert_eq!(result.status, Status::Converged);
    assert_eq!(result.steps, 4);
    assert_eq!(accepted, [0, 1, 2, 3, 4]);
    assert_eq!(result.evaluations, 5);
}

#[test]
fn energy_gradient_keeps_the_level_gradient_and_energy_thresholds() {
    let c = Level::Normal.criteria();
    let s = Step {
        index: 1,
        energy: 0.,
        delta_energy: -c.energy,
        max_gradient: c.max_gradient,
        rms_gradient: c.rms_gradient,
        max_step: 100.,
        rms_step: 100.,
        trust_radius: 0.3,
        on_boundary: true,
        ratio: 1.,
        accuracy: Accuracy::Normal,
        positions: &[],
    };
    assert!(s.energy_gradient(c, 0.999e-3, 1e-3));
    assert!(!s.energy_gradient(c, 1e-3, 1e-3));
    assert!(!s.energy_gradient(c, 1.01e-3, 1e-3));
    for failure in [
        Step {
            max_gradient: 1.01 * c.max_gradient,
            ..s
        },
        Step {
            rms_gradient: 1.01 * c.rms_gradient,
            ..s
        },
        Step {
            delta_energy: -1.01 * c.energy,
            ..s
        },
        Step {
            accuracy: Accuracy::Coarse,
            ..s
        },
    ] {
        assert!(!failure.energy_gradient(c, 0.999e-3, 1e-3));
    }
}

#[test]
fn energy_gradient_does_not_count_rejected_trials() {
    let mut calls = 0;
    let mut p = surface(|x, g, _| {
        calls += 1;
        g.fill(0.);
        g[3] = 1e-5;
        Ok(1e-5 * x[3] + if (2..=4).contains(&calls) { 1. } else { 0. })
    });
    let options = Options {
        convergence: Convergence::EnergyGradient {
            gradient_norm: 1.5e-5,
        },
        tight_final: false,
        ..anchored(Method::Lbfgs)
    };
    let mut callbacks = Vec::new();
    let result = optimize(&mut p, &[1, 1], &INITIAL, &options, |s| {
        callbacks.push(s.index);
        proceed(s)
    });
    assert_eq!(result.status, Status::Converged);
    assert_eq!(result.steps, 2);
    assert_eq!(result.evaluations, 6);
    assert_eq!(callbacks, [0, 1, 2]);
}

#[test]
fn energy_gradient_tight_recheck_preserves_or_resets_the_normal_pair() {
    for fail_recheck in [false, true] {
        let mut tight_calls = 0;
        let mut p = surface(|x, g, a| {
            g.fill(0.);
            let offset = if a == Accuracy::Tight {
                tight_calls += 1;
                100.
            } else {
                0.
            };
            g[3] = if fail_recheck && a == Accuracy::Tight && tight_calls == 1 {
                2e-5
            } else {
                1e-5
            };
            Ok(1e-5 * x[3] + offset)
        });
        let options = Options {
            convergence: Convergence::EnergyGradient {
                gradient_norm: 1.5e-5,
            },
            ..anchored_tight(Method::Lbfgs)
        };
        let mut trace = Vec::new();
        let result = optimize(&mut p, &[1, 1], &INITIAL, &options, |s| {
            trace.push((s.index, s.accuracy, s.delta_energy));
            if s.accuracy == Accuracy::Tight && s.index == 2 {
                assert_eq!((s.delta_energy, s.max_step, s.rms_step), (0., 0., 0.));
                assert!(s.ratio.is_nan());
                assert!(!s.on_boundary);
            }
            proceed(s)
        });
        assert_eq!(result.status, Status::Converged);
        assert_eq!(result.steps, if fail_recheck { 4 } else { 2 });
        assert_eq!(trace[2].0, 2);
        assert_eq!(trace[2].1, Accuracy::Normal);
        assert_eq!(trace[3], (2, Accuracy::Tight, 0.));
        assert!(trace[4..].iter().all(|s| s.1 == Accuracy::Tight));
        assert_eq!(result.energy, 100. + 1e-5 * result.positions[3]);
    }
}

#[test]
fn energy_gradient_does_not_count_coarse_motion_or_accuracy_rechecks() {
    for max_steps in [2, 3] {
        let mut coarse_calls = 0;
        let mut p = surface(|x, g, a| {
            g.fill(0.);
            let offset = if a == Accuracy::Coarse {
                coarse_calls += 1;
                0.
            } else {
                100.
            };
            g[3] = if a == Accuracy::Coarse && coarse_calls == 1 {
                0.02
            } else {
                1e-5
            };
            Ok(1e-5 * x[3] + offset)
        });
        let options = Options {
            convergence: Convergence::EnergyGradient {
                gradient_norm: 1.5e-5,
            },
            adaptive_accuracy: true,
            tight_final: false,
            max_steps,
            ..anchored(Method::Lbfgs)
        };
        let mut trace = Vec::new();
        let result = optimize(&mut p, &[1, 1], &INITIAL, &options, |s| {
            trace.push((s.index, s.accuracy));
            proceed(s)
        });
        assert_eq!(
            trace[..3],
            [
                (0, Accuracy::Coarse),
                (1, Accuracy::Coarse),
                (1, Accuracy::Normal)
            ]
        );
        assert_eq!(result.steps, max_steps);
        assert_eq!(
            result.status,
            if max_steps == 2 {
                Status::MaxSteps
            } else {
                Status::Converged
            }
        );
    }
}

#[test]
fn energy_gradient_norm_must_be_finite_and_positive_even_for_lbfgs() {
    for method in [Method::AncRfo, Method::Internal, Method::Lbfgs] {
        for gradient_norm in [f64::NAN, f64::INFINITY, 0., -1.] {
            let mut p = surface(|_, _, _| panic!("invalid convergence option was evaluated"));
            let options = Options {
                convergence: Convergence::EnergyGradient { gradient_norm },
                ..anchored(method)
            };
            let result = optimize(&mut p, &[1, 1], &INITIAL, &options, |_| {
                panic!("invalid convergence option callback")
            });
            assert!(matches!(result.status, Status::Failed(_)));
            assert_eq!(result.evaluations, 0);
        }
    }
}

#[test]
fn rfo_diagnostics_describe_the_accepted_trial_after_rejection() {
    let initial = [0., 0., 0., 1., 0., 0.];
    let mut calls = 0;
    let p = surface(|x, g, _| {
        calls += 1;
        Ok(well(x, g) + if calls == 2 { 1. } else { 0. })
    });
    let options = Options {
        max_steps: 1,
        tight_final: false,
        ..anchored(Method::AncRfo)
    };
    // Seed the exact harmonic Hessian so the model prediction can be checked
    // independently of model-Hessian construction.
    struct Harmonic<P>(P);
    impl<P: Potential> Potential for Harmonic<P> {
        fn evaluate(&mut self, x: &[f64], g: &mut [f64], a: Accuracy) -> Result<f64, EvalError> {
            self.0.evaluate(x, g, a)
        }
        fn hessian(&mut self, x: &[f64], h: &mut [f64]) -> Option<Result<(), EvalError>> {
            h.fill(0.);
            for i in 0..x.len() {
                h[i * x.len() + i] = 1.;
            }
            Some(Ok(()))
        }
    }
    let mut callbacks = 0;
    let result = optimize(
        &mut Harmonic(p),
        &[1, 1],
        &initial,
        &Options {
            initial_hessian: InitialHessian::Potential,
            ..options
        },
        |s| {
            callbacks += 1;
            if s.index == 0 {
                assert!(s.ratio.is_nan());
                assert!(!s.on_boundary);
            } else {
                assert_eq!(s.trust_radius, 0.075);
                assert!(s.on_boundary);
                let displacement = s.positions[3] - initial[3];
                let predicted = (initial[3] * displacement + 0.5 * displacement.powi(2))
                    / (1. + displacement.powi(2));
                assert!((s.ratio - s.delta_energy / predicted).abs() < 1e-13);
            }
            proceed(s)
        },
    );
    assert_eq!(result.status, Status::MaxSteps);
    assert_eq!((result.steps, result.evaluations, callbacks), (1, 3, 2));
}

#[test]
fn lbfgs_diagnostics_use_the_backtracked_linear_prediction_and_atom_limit() {
    let mut p = surface(|x, g, _| {
        g.fill(0.);
        g[3] = 64. * x[3];
        Ok(32. * x[3].powi(2))
    });
    let options = Options {
        tight_final: false,
        max_steps: 2,
        ..anchored(Method::Lbfgs)
    };
    let mut trace = Vec::new();
    let result = optimize(&mut p, &[1, 1], &[0., 0., 0., 0.1, 0., 0.], &options, |s| {
        trace.push((s.trust_radius, s.on_boundary, s.ratio));
        proceed(s)
    });
    assert_eq!(result.status, Status::Converged);
    assert_eq!(trace.len(), 3);
    assert_eq!((trace[1].0, trace[1].1), (0.15, true));
    assert!((trace[1].2 - 0.25).abs() < 1e-14);
    assert_eq!((trace[2].0, trace[2].1), (0.3, false));
    assert!((trace[2].2 - 0.5).abs() < 1e-14);
}

#[test]
fn per_atom_boundary_flag_distinguishes_the_radius_from_the_atom_cap() {
    for (atom_cap, on_boundary, next_radius) in [(0.01, false, 0.3), (0.4, true, 0.5)] {
        let mut p = surface(|x, g, _| Ok(well(x, g)));
        let options = Options {
            trust: Trust::PerAtom {
                start: 0.3,
                min: 0.001,
                max: 0.5,
                atom_cap,
            },
            max_steps: 2,
            tight_final: false,
            ..anchored(Method::AncRfo)
        };
        let mut trace = Vec::new();
        let result = optimize(&mut p, &[1, 1], &INITIAL, &options, |s| {
            trace.push((s.trust_radius, s.on_boundary));
            proceed(s)
        });
        assert_eq!(result.status, Status::MaxSteps);
        assert_eq!(result.method, Method::AncRfo);
        assert_eq!(trace[1], (0.3, on_boundary));
        assert_eq!(trace[2].0, next_radius);
    }
}

#[test]
fn unavailable_hessian_reproduces_the_model_run() {
    for method in [Method::AncRfo, Method::Internal] {
        let run = |initial_hessian| {
            let mut p = surface(|x, g, _| Ok(well(x, g)));
            let options = Options {
                initial_hessian,
                ..anchored(method)
            };
            optimize(&mut p, &[1, 1], &INITIAL, &options, proceed)
        };
        let model = run(InitialHessian::Model);
        let requested = run(InitialHessian::Potential);
        assert_eq!(model.hessian_evaluations, 0);
        assert_eq!(
            requested,
            Outcome {
                hessian_evaluations: 1,
                ..model
            }
        );
    }
}

#[test]
fn internal_build_failure_preserves_the_potential_hessian_for_anc() {
    const COINCIDENT: [f64; 6] = [-0., 0., -0., 0., 0., 0.];
    let mut hessian = vec![0.; 36];
    for i in 0..6 {
        hessian[i * 6 + i] = 1.;
    }
    // A coincident pair has no defined internal bond or fragment frame. The
    // exact Cartesian Hessian still gives ANC a usable positive model.
    assert!(
        InternalModel::build(
            &[1, 1],
            &[3, 4, 5],
            &COINCIDENT,
            true,
            InternalCurvature::Initial(Some(&hessian), &Options::default()),
        )
        .is_err()
    );
    struct SeparatedWell {
        calls: Vec<(Vec<f64>, Accuracy)>,
        hessian: Vec<f64>,
    }
    impl Potential for SeparatedWell {
        fn evaluate(&mut self, x: &[f64], g: &mut [f64], a: Accuracy) -> Result<f64, EvalError> {
            for (&actual, &initial) in x[..3].iter().zip(&COINCIDENT[..3]) {
                assert_eq!(actual.to_bits(), initial.to_bits());
            }
            self.calls.push((x.to_vec(), a));
            g.copy_from_slice(x);
            g[3] -= 1.4;
            Ok(0.5 * dot(g, g))
        }
        fn hessian(&mut self, x: &[f64], h: &mut [f64]) -> Option<Result<(), EvalError>> {
            assert_eq!(x, COINCIDENT);
            h.copy_from_slice(&self.hessian);
            Some(Ok(()))
        }
    }
    let run = |method, tight_final| {
        let mut p = SeparatedWell {
            calls: Vec::new(),
            hessian: hessian.clone(),
        };
        let options = Options {
            initial_hessian: InitialHessian::Potential,
            tight_final,
            ..anchored(method)
        };
        let mut callbacks = Vec::new();
        let out = optimize(&mut p, &[1, 1], &COINCIDENT, &options, |s| {
            callbacks.push((s.index, s.positions.to_vec(), s.energy, s.accuracy));
            proceed(s)
        });
        (out, p.calls, callbacks)
    };
    // With the opt-in recheck, the fallback must still end with one Tight call.
    let (tight, tight_calls, _) = run(Method::Internal, true);
    assert_eq!(tight.status, Status::Converged);
    assert_eq!(tight.method, Method::AncRfo);
    assert_eq!(tight.methods, [Method::Internal, Method::AncRfo]);
    assert_eq!(tight.evaluations, tight.steps + 2);
    assert_eq!(tight_calls.last().unwrap().1, Accuracy::Tight);
    let (direct, direct_calls, direct_callbacks) = run(Method::AncRfo, false);
    let (fallback, fallback_calls, fallback_callbacks) = run(Method::Internal, false);
    assert_eq!(fallback.status, Status::Converged);
    assert_eq!(fallback.method, Method::AncRfo);
    assert_eq!(fallback.methods, [Method::Internal, Method::AncRfo]);
    assert_eq!(direct.methods, [Method::AncRfo]);
    assert!(fallback.steps > 0);
    assert_eq!(fallback.hessian_evaluations, 1);
    assert_eq!(fallback.evaluations, fallback.steps + 1);
    assert_eq!(fallback_callbacks.len(), fallback.steps + 1);
    assert!((fallback.positions[3] - 1.4).abs() < 1e-5);
    assert_eq!(
        Outcome {
            methods: direct.methods.clone(),
            ..fallback
        },
        direct
    );
    assert_eq!(fallback_calls, direct_calls);
    assert_eq!(fallback_callbacks, direct_callbacks);
    assert_eq!(
        fallback_calls
            .iter()
            .filter(|(x, _)| x == &COINCIDENT)
            .count(),
        1
    );
}

#[test]
fn rejected_short_diis_step_retries_rfo_at_the_radius_floor() {
    let options = Options {
        acceleration: Acceleration::Gdiis,
        ..anchored(Method::AncRfo)
    };
    let mut engine = Engine::new(&options, 1);
    let mut trials = Vec::new();
    let mut p = surface(|x, g, _| {
        trials.push(x.to_vec());
        g.fill(0.);
        Ok(if trials.len() == 1 { 1. } else { -1. })
    });
    let mut eval = Evaluator {
        potential: &mut p,
        coords: Coordinates::new(&[1, 1], &[0.; 6], &[0]).unwrap(),
        accuracy: Accuracy::Normal,
        count: 0,
        hessian_count: 0,
    };
    let point = |x: [f64; 3], g: [f64; 3]| Point {
        positions: [[0.; 3], x].concat(),
        gradient: [[0.; 3], g].concat(),
        active_gradient: g.to_vec(),
        energy: 0.,
        rigid: Vec::new(),
    };
    let d = 5e-4;
    let current = point([0.; 3], [0., 0., d]);
    let mut h = vec![0.; 36];
    for i in 0..6 {
        h[i * 6 + i] = 1.;
    }
    let anc = Anc::build(&[1, 1], &eval.coords, &current, 0, None, Some(h), &options).unwrap();
    for sample in [
        point([d, 0., -d], [d, 0., 0.]),
        point([0., d, -d], [0., d, 0.]),
    ] {
        engine.history.push(&anc, &sample, &eval.coords);
    }
    engine.anc = Some(anc);
    let (_, diagnostics) = engine
        .search_anc(&mut eval, &[1, 1], &current, 2, false)
        .unwrap()
        .unwrap();
    drop(p);
    assert_eq!(diagnostics.radius, 1e-3);
    assert_eq!(trials.len(), 2);
    assert!((trials[0][5] + d).abs() < 1e-15);
    assert!(trials[1][5] > trials[0][5]); // The ordinary RFO step is shorter.
}

struct BrokenHessian(Option<EvalError>);

impl Potential for BrokenHessian {
    fn evaluate(&mut self, x: &[f64], g: &mut [f64], _: Accuracy) -> Result<f64, EvalError> {
        Ok(well(x, g))
    }
    fn hessian(&mut self, x: &[f64], h: &mut [f64]) -> Option<Result<(), EvalError>> {
        assert_eq!(h.len(), x.len() * x.len());
        // With no error, return success but leave the matrix unwritten.
        Some(self.0.take().map_or(Ok(()), Err))
    }
}

#[test]
fn no_hessian_is_requested_without_a_first_step() {
    for method in [Method::AncRfo, Method::Internal] {
        for (positions, status) in [([0.; 6], Status::Converged), (INITIAL, Status::MaxSteps)] {
            let options = Options {
                adaptive_accuracy: true,
                max_steps: 0,
                initial_hessian: InitialHessian::Potential,
                ..anchored(method)
            };
            let out = optimize(
                &mut BrokenHessian(Some(EvalError::Scf("scf".into()))),
                &[1, 1],
                &positions,
                &options,
                proceed,
            );
            assert_eq!(out.status, status);
            assert_eq!(out.hessian_evaluations, 0);
            assert!(out.methods.is_empty());
        }
    }
}

#[test]
fn unusable_potential_hessian_falls_back_to_the_model() {
    for method in [Method::AncRfo, Method::Internal] {
        let run = |initial_hessian, error| {
            let options = Options {
                initial_hessian,
                ..anchored(method)
            };
            optimize(
                &mut BrokenHessian(error),
                &[1, 1],
                &INITIAL,
                &options,
                proceed,
            )
        };
        let model = run(InitialHessian::Model, None);
        assert_eq!(model.status, Status::Converged);
        for error in [
            Some(EvalError::Scf("scf".into())),
            Some(EvalError::Failed("worker".into())),
            None,
        ] {
            let out = run(InitialHessian::Potential, error);
            assert_eq!(out.hessian_evaluations, 1);
            assert_eq!(
                Outcome {
                    hessian_evaluations: 0,
                    ..out
                },
                model
            );
        }
        let out = run(InitialHessian::Potential, Some(EvalError::Cancelled));
        assert_eq!(out.status, Status::Cancelled);
        assert!(out.methods.is_empty());
        assert_eq!(
            (out.steps, out.evaluations, out.hessian_evaluations),
            (0, 1, 1)
        );
        assert_eq!(out.positions, INITIAL);
        assert_eq!(out.gradient, INITIAL);
        assert_eq!(out.energies, [0.5 * dot(&INITIAL, &INITIAL)]);
    }
}

#[test]
fn adaptive_accuracy_uses_one_energy_scale_per_displacement() {
    for method in [Method::AncRfo, Method::Internal, Method::Lbfgs] {
        for tight_final in [false, true] {
            let energy = |x: &[f64], accuracy| {
                let (center, offset) = match accuracy {
                    Accuracy::Coarse => (0., -100.),
                    Accuracy::Normal => (0.004, 100.),
                    Accuracy::Tight => (0.004, 200.),
                };
                offset + 0.5 * (x[3] - center).powi(2)
            };
            let mut calls = Vec::new();
            let mut p = surface(|x, g, a| {
                calls.push((x.to_vec(), a));
                g.fill(0.);
                g[0] = 10.; // Fixed-atom forces do not set accuracy.
                g[3] = x[3] - if a == Accuracy::Coarse { 0. } else { 0.004 };
                Ok(energy(x, a))
            });
            let options = Options {
                adaptive_accuracy: true,
                tight_final,
                ..anchored(method)
            };
            let mut previous: Option<(usize, Vec<f64>, Accuracy)> = None;
            let mut transitions = Vec::new();
            let out = optimize(&mut p, &[1, 1], &INITIAL, &options, |s| {
                if let Some((index, x, accuracy)) = &previous {
                    if *index == s.index {
                        assert_eq!(x, s.positions);
                        assert_eq!((s.delta_energy, s.max_step, s.rms_step), (0., 0., 0.));
                        transitions.push((*accuracy, s.accuracy));
                    } else {
                        assert_eq!(*accuracy, s.accuracy);
                        assert_eq!(s.delta_energy, s.energy - energy(x, s.accuracy));
                    }
                }
                previous = Some((s.index, s.positions.to_vec(), s.accuracy));
                proceed(s)
            });
            drop(p);
            assert_eq!(out.status, Status::Converged);
            assert_eq!(out.evaluations, calls.len());
            assert_eq!(calls[0].1, Accuracy::Coarse);
            assert_eq!(transitions[0], (Accuracy::Coarse, Accuracy::Normal));
            assert_eq!(
                previous.unwrap().2,
                if tight_final {
                    Accuracy::Tight
                } else {
                    Accuracy::Normal
                }
            );
            assert!((out.positions[3] - 0.004).abs() < 1e-5);
        }
    }
}

#[test]
fn apparently_stationary_coarse_input_requires_a_normal_gradient() {
    let mut calls = Vec::new();
    let mut p = surface(|x, g, a| {
        calls.push(a);
        g.fill(0.);
        if a == Accuracy::Normal {
            g[3] = 1.;
        }
        Ok(x[3] * g[3])
    });
    let options = Options {
        adaptive_accuracy: true,
        tight_final: false,
        max_steps: 0,
        ..anchored(Method::AncRfo)
    };
    let out = optimize(&mut p, &[1, 1], &INITIAL, &options, proceed);
    drop(p);
    assert_eq!(calls, [Accuracy::Coarse, Accuracy::Normal]);
    assert_eq!(out.status, Status::MaxSteps);
    assert_eq!(out.gradient[3], 1.);
    assert_eq!(out.energies, [INITIAL[3]]);
}

#[test]
fn coarse_displacement_cannot_establish_normal_convergence() {
    let mut p = surface(|x, g, a| {
        let (k, slope) = if a == Accuracy::Coarse {
            (64., 0.)
        } else {
            (256., 1e-4)
        };
        g.fill(0.);
        g[3] = 2. * k * x[3] + slope;
        Ok(k * x[3].powi(2) + slope * x[3])
    });
    let options = Options {
        adaptive_accuracy: true,
        tight_final: false,
        max_steps: 1,
        ..anchored(Method::Lbfgs)
    };
    let out = optimize(
        &mut p,
        &[1, 1],
        &[0., 0., 0., 0.00025, 0., 0.],
        &options,
        proceed,
    );
    // Coarse reaches x=0 in one displacement (|ΔE| = 4e-6).
    // At Normal accuracy that displacement would have |ΔE| > 1.6e-5.
    assert_eq!(out.status, Status::MaxSteps);
    assert_eq!(out.steps, 1);
    assert_eq!(out.gradient[3], 1e-4);
}

#[test]
fn adaptive_accuracy_stays_normal_after_promotion() {
    let mut calls = Vec::new();
    let mut p = surface(|x, g, a| {
        calls.push(a);
        let dx = x[3] - if a == Accuracy::Coarse { 0.98 } else { 0. };
        g.fill(0.);
        g[3] = 0.01 * dx;
        Ok(0.005 * dx * dx)
    });
    let options = Options {
        adaptive_accuracy: true,
        tight_final: false,
        ..anchored(Method::Lbfgs)
    };
    let out = optimize(
        &mut p,
        &[1, 1],
        &[0., 0., 0., 1., 0., 0.],
        &options,
        proceed,
    );
    drop(p);
    assert_eq!(out.status, Status::Converged);
    assert_eq!(calls[0], Accuracy::Coarse);
    assert!(calls[1..].iter().all(|&a| a == Accuracy::Normal));
    assert!(out.positions[3].abs() < 1e-12);
}
