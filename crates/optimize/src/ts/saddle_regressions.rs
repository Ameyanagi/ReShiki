//! Analytic counterexamples for trust-model recovery and force-only acceptance.

use super::*;
use crate::{Accuracy, EvalError};

#[derive(Default)]
struct NonstationaryCubic {
    hessian_points: Vec<f64>,
}

impl Potential for NonstationaryCubic {
    fn evaluate(&mut self, x: &[f64], g: &mut [f64], _: Accuracy) -> Result<f64, EvalError> {
        g[0] = 0.05 - x[0] + 7.0 * x[0] * x[0];
        Ok(0.05 * x[0] - 0.5 * x[0] * x[0] + 7.0 * x[0].powi(3) / 3.0)
    }

    fn hessian(&mut self, x: &[f64], h: &mut [f64]) -> Option<Result<(), EvalError>> {
        self.hessian_points.push(x[0]);
        h[0] = -1.0 + 14.0 * x[0];
        Some(Ok(()))
    }
}

#[test]
fn refreshed_current_hessian_cannot_silently_lose_the_negative_mode() {
    // g = 7x² - x + 0.05 has negative discriminant: there is no stationary point.
    let mut refreshed = false;
    let mut accepts_after_refresh = 0;
    let result = refine(
        &mut NonstationaryCubic::default(),
        &Minimum {
            positions: vec![0.0],
            energy: 0.0,
        },
        &[1.0],
        &[],
        &TsOptions {
            saddle_max_steps: 8,
            hessian_every: 0,
            ..TsOptions::default()
        },
        &mut |event| {
            match event {
                Event::SaddleStatus("refresh rejected secant model") => refreshed = true,
                Event::SaddleTrial { decision, .. }
                    if refreshed && decision.starts_with("accept") =>
                {
                    accepts_after_refresh += 1;
                }
                _ => {}
            }
            ControlFlow::Continue(())
        },
    )
    .unwrap();
    assert!(
        refreshed,
        "the secant model must be checked against the exact curvature"
    );
    assert_eq!(
        accepts_after_refresh, 0,
        "positive exact curvature replaced the followed negative mode"
    );
    assert!(!result.converged);
}

#[test]
fn first_confirmed_trial_mode_loss_refreshes_current_model_without_radius_retries() {
    let mut potential = NonstationaryCubic::default();
    let result = refine(
        &mut potential,
        &Minimum {
            positions: vec![0.0],
            energy: 0.0,
        },
        &[1.0],
        &[],
        &TsOptions {
            saddle_max_steps: 8,
            hessian_every: 0,
            ..TsOptions::default()
        },
        &mut |_| ControlFlow::Continue(()),
    )
    .unwrap();
    assert!(!result.converged);
    assert!(
        potential.hessian_points.len() <= 3,
        "one initial, one trial certification and one current refresh suffice: {:?}",
        potential.hessian_points
    );
}

struct FlatPlateau;

impl Potential for FlatPlateau {
    fn evaluate(&mut self, x: &[f64], g: &mut [f64], _: Accuracy) -> Result<f64, EvalError> {
        let left = (0.04 - x[0]).max(0.0);
        let right = (x[0] - 0.06).max(0.0);
        let slope = 0.0024100250626566417;
        g[0] = slope + left * left + right * right;
        Ok(slope * x[0] + (right.powi(3) - left.powi(3)) / 3.0)
    }

    fn hessian(&mut self, x: &[f64], h: &mut [f64]) -> Option<Result<(), EvalError>> {
        h[0] = 2.0 * ((x[0] - 0.06).max(0.0) - (0.04 - x[0]).max(0.0));
        Some(Ok(()))
    }
}

#[test]
fn zero_curvature_refresh_stops_at_the_flat_plateau() {
    let result = refine(
        &mut FlatPlateau,
        &Minimum {
            positions: vec![0.0],
            energy: 0.0,
        },
        &[1.0],
        &[],
        &TsOptions {
            hessian_every: 0,
            ..TsOptions::default()
        },
        &mut |_| ControlFlow::Continue(()),
    )
    .unwrap();
    assert_eq!(result.reason, SaddleReason::FollowedModeLost);
    assert!((result.point.positions[0] - 0.05).abs() < 1e-12);
    assert_eq!(result.hessian.unwrap(), [0.0]);
}

struct FailingStretchRefresh(EvalError);

impl Potential for FailingStretchRefresh {
    fn evaluate(&mut self, x: &[f64], g: &mut [f64], _: Accuracy) -> Result<f64, EvalError> {
        let axis = std::array::from_fn::<_, 3, _>(|i| x[i + 3] - x[i]);
        let r = norm(&axis);
        let q = r - 2.0;
        let slope = 0.05 - q + 7.0 * q * q;
        for i in 0..3 {
            g[i + 3] = slope * axis[i] / r;
            g[i] = -g[i + 3];
        }
        Ok(0.05 * q - 0.5 * q * q + 7.0 * q.powi(3) / 3.0)
    }

    fn hessian(&mut self, x: &[f64], h: &mut [f64]) -> Option<Result<(), EvalError>> {
        let axis = std::array::from_fn::<_, 3, _>(|i| x[i + 3] - x[i]);
        let r = norm(&axis);
        let q = r - 2.0;
        if (0.075..0.08).contains(&q) {
            return Some(Err(self.0.clone()));
        }
        let transverse = (0.05 - q + 7.0 * q * q) / r;
        let curvature = -1.0 + 14.0 * q;
        for i in 0..6 {
            for j in 0..6 {
                let block = (curvature - transverse) * axis[i % 3] * axis[j % 3] / r.powi(2)
                    + if i % 3 == j % 3 { transverse } else { 0.0 };
                h[i * 6 + j] = if i / 3 == j / 3 { block } else { -block };
            }
        }
        Some(Ok(()))
    }
}

#[test]
fn refresh_scf_at_accepted_stretch_keeps_verified_candidate() {
    let start = Minimum {
        positions: vec![0., 0., 0., 2., 0., 0.],
        energy: 0.,
    };
    let band = Band {
        path: vec![start.clone(); 3],
        converged: false,
        climbing: 1,
        tangent: vec![-1., 0., 0., 1., 0., 0.],
    };
    for error in [
        EvalError::Cancelled,
        EvalError::Failed("worker lost".into()),
        EvalError::Scf("stretch refresh unavailable".into()),
    ] {
        let result = refine_band(
            &mut FailingStretchRefresh(error.clone()),
            &band,
            &[12.; 2],
            &TsOptions::default(),
            &mut |_| ControlFlow::Continue(()),
        );
        if matches!(error, EvalError::Scf(_)) {
            let result = result.expect("an exact index-one candidate was retained at q=0");
            assert!(!result.converged);
            assert_eq!(result.point.positions, start.positions);
            assert!(result.hessian.is_some());
            assert!(
                result
                    .diagnostics
                    .iter()
                    .any(|d| d.contains("secant model refresh")
                        && d.contains("stretch refresh unavailable"))
            );
        } else {
            assert!(matches!(result, Err(actual) if actual == TsError::from(error)));
        }
    }
}

#[test]
fn exact_mode_loss_at_radius_floor_traces_the_reported_reason() {
    // This short trial passes the energy ratio but crosses the inflection at x=0.004.
    struct FloorInflection;
    impl Potential for FloorInflection {
        fn evaluate(&mut self, x: &[f64], g: &mut [f64], _: Accuracy) -> Result<f64, EvalError> {
            g[0] = 0.005 - x[0] + 125.0 * x[0] * x[0];
            Ok(0.005 * x[0] - 0.5 * x[0] * x[0] + 125.0 * x[0].powi(3) / 3.0)
        }

        fn hessian(&mut self, x: &[f64], h: &mut [f64]) -> Option<Result<(), EvalError>> {
            h[0] = -1.0 + 250.0 * x[0];
            Some(Ok(()))
        }
    }
    let mut status = None;
    let result = refine(
        &mut FloorInflection,
        &Minimum {
            positions: vec![0.],
            energy: 0.0,
        },
        &[1.0],
        &[],
        &TsOptions {
            hessian_every: 1,
            ..TsOptions::default()
        },
        &mut |event| {
            if let Event::SaddleStatus(label) = event {
                status = Some(label);
            }
            ControlFlow::Continue(())
        },
    )
    .unwrap();
    assert_eq!(result.reason, SaddleReason::FollowedModeLost);
    assert_eq!(status.unwrap(), result.reason.to_string());
}

struct SpectatorCubic {
    tilt: f64,
    fail_trial_hessian: bool,
}

#[test]
fn uncertifiable_force_only_trial_is_rejected_on_scf_failure() {
    let start = Minimum {
        positions: vec![0.002, 0.002, 0.0],
        energy: 0.0,
    };
    let mut rejected = false;
    let result = refine(
        &mut SpectatorCubic {
            tilt: 4e-5,
            fail_trial_hessian: true,
        },
        &start,
        &[1., 0., 0.],
        &[],
        &TsOptions::default(),
        &mut |event| {
            if let Event::SaddleTrial {
                decision: "reject: Hessian SCF failure",
                ..
            } = event
            {
                rejected = true;
            }
            ControlFlow::Continue(())
        },
    )
    .unwrap();
    assert!(rejected);
    assert_eq!(result.point.positions, start.positions);
    assert!(!result.converged);
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.contains("trial index unavailable"))
    );
}

impl Potential for SpectatorCubic {
    fn evaluate(&mut self, x: &[f64], g: &mut [f64], _: Accuracy) -> Result<f64, EvalError> {
        let d = x[0] - 0.002;
        g[0] = -2.0 * x[0] - 150.0 * d * d + 500.0 * x[2] * x[2];
        g[1] = 2.0 * x[1] + self.tilt;
        g[2] = (1000.0 * x[0] - 1.0) * x[2];
        Ok(
            -x[0] * x[0] + x[1] * x[1] + self.tilt * x[1] - 50.0 * d.powi(3)
                + 0.5 * (1000.0 * x[0] - 1.0) * x[2] * x[2],
        )
    }

    fn hessian(&mut self, x: &[f64], h: &mut [f64]) -> Option<Result<(), EvalError>> {
        if self.fail_trial_hessian && x[0] < 0.001 {
            return Some(Err(EvalError::Scf("trial index unavailable".into())));
        }
        h.fill(0.0);
        h[0] = -2.0 - 300.0 * (x[0] - 0.002);
        h[4] = 2.0;
        h[8] = 1000.0 * x[0] - 1.0;
        h[2] = 1000.0 * x[2];
        h[6] = h[2];
        Some(Ok(()))
    }
}

#[test]
fn force_only_acceptance_checks_the_exact_spectator_index() {
    let start = vec![0.002, 0.002, 0.0];
    let mut potential = SpectatorCubic {
        tilt: 4e-5,
        fail_trial_hessian: false,
    };
    let (energy, _) = evaluate(&mut potential, &start).unwrap();
    let mut projected_force_accepts = 0;
    let result = refine(
        &mut potential,
        &Minimum {
            positions: start.clone(),
            energy,
        },
        &[1.0, 0.0, 0.0],
        &[],
        &TsOptions {
            saddle_max_steps: 1,
            hessian_every: 0,
            ..TsOptions::default()
        },
        &mut |event| {
            if let Event::SaddleTrial {
                decision: "accept: projected force",
                ..
            } = event
            {
                projected_force_accepts += 1;
            }
            ControlFlow::Continue(())
        },
    )
    .unwrap();
    assert_eq!(
        projected_force_accepts, 0,
        "the spectator becomes a second negative mode"
    );
    assert_eq!(result.point.positions, start);
    assert!(!result.converged);
}

#[test]
fn cancelling_energy_acceptance_checks_the_exact_spectator_index() {
    // Removing the linear y term makes the modeled ascent and descent cancel.
    // The force ratio is about 0.989 although the trial has two negative modes.
    let start = vec![0.002, 0.002, 0.0];
    let mut potential = SpectatorCubic {
        tilt: 0.0,
        fail_trial_hessian: false,
    };
    let (energy, _) = evaluate(&mut potential, &start).unwrap();
    let mut accepts = 0;
    let mut rejected_index = false;
    let result = refine(
        &mut potential,
        &Minimum {
            positions: start.clone(),
            energy,
        },
        &[1.0, 0.0, 0.0],
        &[],
        &TsOptions {
            saddle_max_steps: 1,
            hessian_every: 0,
            ..TsOptions::default()
        },
        &mut |event| {
            if let Event::SaddleTrial { decision, .. } = event {
                accepts += usize::from(decision.starts_with("accept"));
                rejected_index |= decision == "reject: trial Hessian index";
            }
            ControlFlow::Continue(())
        },
    )
    .unwrap();
    assert!(
        rejected_index,
        "cancellation still needs an exact index check"
    );
    assert_eq!(accepts, 0, "the spectator becomes a second negative mode");
    assert_eq!(result.point.positions, start);
    assert!(!result.converged);
}

struct FloorCancellation {
    gradient: [f64; 2],
    direction: Vec<f64>,
}

impl Potential for FloorCancellation {
    fn evaluate(&mut self, x: &[f64], g: &mut [f64], _: Accuracy) -> Result<f64, EvalError> {
        let q = dot(x, &self.direction);
        let t = q / MIN_RADIUS;
        // The residual changes the energy at the floor while its first two
        // derivatives vanish there. Larger trial steps fail the energy model.
        let residual = 2000.0 * q.powi(3) * (1.0 - 1.5 * t + 0.6 * t * t);
        let slope = 6000.0 * q * q * (1.0 - t).powi(2);
        g[0] = self.gradient[0] - x[0] + slope * self.direction[0];
        g[1] = self.gradient[1] + x[1] + slope * self.direction[1];
        Ok(dot(x, &self.gradient) - 0.5 * x[0] * x[0] + 0.5 * x[1] * x[1] + residual)
    }

    fn hessian(&mut self, x: &[f64], h: &mut [f64]) -> Option<Result<(), EvalError>> {
        let q = dot(x, &self.direction);
        let t = q / MIN_RADIUS;
        let curvature = 12000.0 * q * (1.0 - t) * (1.0 - 2.0 * t);
        h.copy_from_slice(&[-1.0, 0.0, 0.0, 1.0]);
        for i in 0..2 {
            for j in 0..2 {
                h[i * 2 + j] += curvature * self.direction[i] * self.direction[j];
            }
        }
        Some(Ok(()))
    }
}

#[test]
fn force_only_acceptance_uses_the_same_roundoff_safe_floor_as_rejection() {
    // Select a clipped P-RFO direction that rounds one ulp above the radius.
    // The first pair is the review's (0.01, 0.02) scaled-norm counterexample;
    // the alternatives cover eigensolver roundoff on other architectures.
    let (gradient, direction) = [(0.01, 0.02), (0.008, 0.024)]
        .into_iter()
        .find_map(|(a, b)| {
            let gradient = [a / (1.0 - a * a), -b / (1.0 - b * b)];
            let raw = rfo_step(&[-1.0, 1.0], &[1.0, 0.0, 0.0, 1.0], &gradient, 0, 0.1).unwrap();
            let length = norm(&raw);
            let clipped = rfo_step(
                &[-1.0, 1.0],
                &[1.0, 0.0, 0.0, 1.0],
                &gradient,
                0,
                MIN_RADIUS,
            )
            .unwrap();
            (norm(&clipped) > MIN_RADIUS).then(|| {
                (
                    gradient,
                    raw.into_iter().map(|v| v / length).collect::<Vec<_>>(),
                )
            })
        })
        .expect("the scaled-norm fixture must round above the floor");
    let mut force_accepts = 0;
    let result = refine(
        &mut FloorCancellation {
            gradient,
            direction,
        },
        &Minimum {
            positions: vec![0.0, 0.0],
            energy: 0.0,
        },
        &[1.0, 0.0],
        &[],
        &TsOptions {
            saddle_max_steps: 1,
            ..TsOptions::default()
        },
        &mut |event| {
            if let Event::SaddleTrial {
                decision: "accept: projected force",
                ..
            } = event
            {
                force_accepts += 1;
            }
            ControlFlow::Continue(())
        },
    )
    .unwrap();
    assert_eq!(
        force_accepts, 1,
        "a clipped floor step must receive the force check"
    );
    assert!(norm(&result.point.positions) > 0.0);
}

#[test]
fn fallback_holds_radius_through_cancellation_until_energy_agrees() {
    let mut radius = MIN_RADIUS;
    let mut awaiting_energy = false;
    update_accepted_radius(
        &mut radius,
        &mut awaiting_energy,
        MIN_RADIUS,
        1.0,
        false,
        true,
    );
    assert_eq!(radius, MIN_RADIUS);
    assert!(awaiting_energy);

    // A cancelled energy prediction still uses force as its merit function.
    update_accepted_radius(
        &mut radius,
        &mut awaiting_energy,
        MIN_RADIUS,
        1.0,
        false,
        false,
    );
    assert_eq!(radius, MIN_RADIUS);
    assert!(awaiting_energy);

    update_accepted_radius(
        &mut radius,
        &mut awaiting_energy,
        MIN_RADIUS,
        1.0,
        true,
        false,
    );
    assert_eq!(radius, 1.5 * MIN_RADIUS);
    assert!(!awaiting_energy);

    // Cancellation itself establishes the hold even before another fallback.
    let mut ordinary_radius = MIN_RADIUS;
    update_accepted_radius(
        &mut ordinary_radius,
        &mut awaiting_energy,
        MIN_RADIUS,
        1.0,
        false,
        true,
    );
    assert_eq!(ordinary_radius, MIN_RADIUS);
    assert!(awaiting_energy);
}

struct CancellationWithIncreasingForce;

impl Potential for CancellationWithIncreasingForce {
    fn evaluate(&mut self, x: &[f64], g: &mut [f64], _: Accuracy) -> Result<f64, EvalError> {
        // Follow the stiffer positive mode. At the initial step ascent and
        // descent cancel in energy while the exact squared force increases.
        let slope = 0.10377087445737959;
        g[0] = 2.0 * x[0];
        g[1] = x[1] + slope;
        Ok(x[0] * x[0] + 0.5 * x[1] * x[1] + slope * x[1])
    }

    fn hessian(&mut self, _: &[f64], h: &mut [f64]) -> Option<Result<(), EvalError>> {
        h.copy_from_slice(&[2.0, 0.0, 0.0, 1.0]);
        Some(Ok(()))
    }
}

#[test]
fn cancellation_trace_reports_the_signed_force_model_ratio() {
    let mut observed = None;
    refine(
        &mut CancellationWithIncreasingForce,
        &Minimum {
            positions: vec![0.0, 0.0],
            energy: 0.0,
        },
        &[1.0, 0.0],
        &[],
        &TsOptions {
            saddle_max_steps: 1,
            ..TsOptions::default()
        },
        &mut |event| {
            if let Event::SaddleTrial {
                ratio,
                force_ratio,
                decision,
                ..
            } = event
            {
                observed.get_or_insert((ratio, force_ratio, decision));
            }
            ControlFlow::Continue(())
        },
    )
    .unwrap();
    let (selected, force, decision) = observed.unwrap();
    assert_eq!(decision, "reject: model ratio");
    assert!((selected - 1.0).abs() < 1e-12);
    assert!(
        (force - selected).abs() < 1e-12,
        "the signed model ratio must not be logged as zero"
    );
}

#[derive(Default)]
struct RecoveryBudgetCubic {
    hessians: usize,
}

impl Potential for RecoveryBudgetCubic {
    fn evaluate(&mut self, x: &[f64], g: &mut [f64], _: Accuracy) -> Result<f64, EvalError> {
        // g = (x - 1/2)² + 1/4 never vanishes. Repeated exact trial losses
        // surround the inflection and recur after the single permitted restart.
        g[0] = 0.5 - x[0] + x[0] * x[0];
        Ok(0.5 * x[0] - 0.5 * x[0] * x[0] + x[0].powi(3) / 3.0)
    }

    fn hessian(&mut self, x: &[f64], h: &mut [f64]) -> Option<Result<(), EvalError>> {
        self.hessians += 1;
        h[0] = -1.0 + 2.0 * x[0];
        Some(Ok(()))
    }
}

#[test]
fn extra_hessian_budget_is_global_across_the_recovery_restart() {
    let mut potential = RecoveryBudgetCubic::default();
    // At 0.05 the first leg crosses the inflection in three accepted steps,
    // spending two checks before the restart. The restarted, smaller-radius
    // leg then consumes the remaining shared recovery allowance.
    let path = [-1.0, 0.05, -2.0]
        .into_iter()
        .map(|x| Minimum {
            positions: vec![x],
            energy: 0.5 * x - 0.5 * x * x + x * x * x / 3.0,
        })
        .collect();
    let band = Band {
        path,
        converged: false,
        climbing: 1,
        tangent: vec![1.0],
    };
    let mut restarts = 0;
    let result = refine_band(
        &mut potential,
        &band,
        &[],
        &TsOptions {
            hessian_every: 0,
            ..TsOptions::default()
        },
        &mut |event| {
            if let Event::SaddleStatus("restart from band with fresh Hessian") = event {
                restarts += 1;
            }
            ControlFlow::Continue(())
        },
    )
    .unwrap();
    assert_eq!(restarts, 1);
    assert_eq!(result.reason, SaddleReason::HessianBudgetExhausted);
    assert!(!result.converged);
    assert_eq!(
        potential.hessians, MAX_RECOVERY_HESSIANS,
        "one initial Hessian plus seven extras, preserving the stationarity slot"
    );
}

struct CountingQuadratic {
    stable_curvature: f64,
    hessians: usize,
}

impl Potential for CountingQuadratic {
    fn evaluate(&mut self, x: &[f64], g: &mut [f64], _: Accuracy) -> Result<f64, EvalError> {
        g[0] = -2.0 * x[0];
        g[1] = self.stable_curvature * x[1];
        Ok(-x[0] * x[0] + 0.5 * self.stable_curvature * x[1] * x[1])
    }

    fn hessian(&mut self, _: &[f64], h: &mut [f64]) -> Option<Result<(), EvalError>> {
        self.hessians += 1;
        h.copy_from_slice(&[-2.0, 0.0, 0.0, self.stable_curvature]);
        Some(Ok(()))
    }
}

#[test]
fn cancelling_energy_acceptance_holds_the_radius_without_a_prior_fallback() {
    let mut potential = CountingQuadratic {
        stable_curvature: 2.0,
        hessians: 0,
    };
    let mut radii = Vec::new();
    let result = refine(
        &mut potential,
        &Minimum {
            positions: vec![0.3, 0.3],
            energy: 0.0,
        },
        &[1.0, 0.0],
        &[],
        &TsOptions {
            hessian_every: 0,
            ..TsOptions::default()
        },
        &mut |event| {
            if let Event::Saddle { radius, .. } = event {
                radii.push(radius);
            }
            ControlFlow::Continue(())
        },
    )
    .unwrap();
    assert!(result.converged);
    assert!(
        radii.len() > 2,
        "exercise repeated radius-limited cancellation"
    );
    assert!(radii.iter().all(|radius| *radius == 0.1), "{radii:?}");
    assert_eq!(potential.hessians, radii.len());
}

#[derive(Default)]
struct ExhaustedRecoveryQuadratic {
    hessians: usize,
}

impl Potential for ExhaustedRecoveryQuadratic {
    fn evaluate(&mut self, x: &[f64], g: &mut [f64], _: Accuracy) -> Result<f64, EvalError> {
        // A C2 staircase changes the energy between 0.01-bohr nodes while
        // leaving their gradient and Hessian equal to the concave quadratic.
        // Each floor step requires force-only certification; below the last
        // node the next Newton step has an ordinary, reliable energy ratio.
        let t = ((x[0] - 0.0004) / MIN_RADIUS).max(0.0);
        let fraction = t - t.floor();
        let smooth = fraction.powi(3) * (10.0 - 15.0 * fraction + 6.0 * fraction * fraction);
        let slope = 30.0 * fraction * fraction * (1.0 - fraction).powi(2);
        g[0] = -x[0] + slope;
        Ok(-0.5 * x[0] * x[0] + MIN_RADIUS * (t.floor() + smooth))
    }

    fn hessian(&mut self, x: &[f64], h: &mut [f64]) -> Option<Result<(), EvalError>> {
        self.hessians += 1;
        let t = ((x[0] - 0.0004) / MIN_RADIUS).max(0.0);
        let fraction = t - t.floor();
        h[0] = -1.0 + 60.0 * fraction * (1.0 - fraction) * (1.0 - 2.0 * fraction) / MIN_RADIUS;
        Some(Ok(()))
    }
}

#[test]
fn recovery_budget_keeps_a_final_stationarity_check_available() {
    // Spend every ordinary recovery slot before the force-converged trial.
    // Derive the path length from the actual budget so the old eight-slot
    // allocation reaches the same final check with its entire cap exhausted.
    let mut budget = HessianBudget::default();
    let mut slots = 0;
    while budget.reserve() {
        slots += 1;
    }

    let mut potential = ExhaustedRecoveryQuadratic::default();
    let start = vec![0.0004 + slots as f64 * MIN_RADIUS];
    let (energy, _) = evaluate(&mut potential, &start).unwrap();
    let mut force_accepts = 0;
    let mut ordinary_accepts = 0;
    let result = refine(
        &mut potential,
        &Minimum {
            positions: start,
            energy,
        },
        &[1.0],
        &[],
        &TsOptions {
            hessian_every: 0,
            ..TsOptions::default()
        },
        &mut |event| {
            if let Event::SaddleTrial { decision, .. } = event {
                force_accepts += usize::from(decision == "accept: projected force");
                ordinary_accepts += usize::from(decision == "accept");
            }
            ControlFlow::Continue(())
        },
    )
    .unwrap();
    assert_eq!(force_accepts, slots);
    assert!(
        result.converged,
        "the reserved final Hessian must certify the root"
    );
    assert_eq!(slots, MAX_RECOVERY_HESSIANS - 1);
    assert_eq!(ordinary_accepts, 1);
    assert!(result.point.positions[0].abs() < 1e-8);
    assert_eq!(potential.hessians, 1 + MAX_RECOVERY_HESSIANS);
}

#[test]
fn accepted_periodic_hessians_do_not_consume_the_extra_work_budget() {
    let mut potential = CountingQuadratic {
        stable_curvature: 3.0,
        hessians: 0,
    };
    let mut accepted = 0;
    let result = refine(
        &mut potential,
        &Minimum {
            positions: vec![2.0, 2.0],
            energy: 2.0,
        },
        &[1.0, 0.0],
        &[],
        &TsOptions {
            hessian_every: 1,
            ..TsOptions::default()
        },
        &mut |event| {
            if let Event::Saddle { step, .. } = event {
                accepted = step;
            }
            ControlFlow::Continue(())
        },
    )
    .unwrap();
    assert!(result.converged);
    assert!(accepted > MAX_RECOVERY_HESSIANS);
    assert_eq!(potential.hessians, accepted + 1);
}
