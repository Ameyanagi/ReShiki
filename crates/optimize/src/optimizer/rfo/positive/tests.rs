use super::*;

#[test]
fn weak_force_in_a_flat_mode_is_resolved_in_step_coordinates() {
    let n = 24;
    let mut hessian = vec![0.; n * n];
    for i in 0..n {
        hessian[i * n + i] = 1. + i as f64 * 0.1;
    }
    hessian[(n - 1) * n + n - 1] = 1e-12;
    let mut gradient = vec![0.; n];
    gradient[0] = 1e-5;
    gradient[n - 1] = 1e-18;

    // The diagonal secular equation is an independent scalar oracle. Its
    // weak flat-mode force is below a 1e-12*||g|| residual tolerance, but its
    // displacement is approximately 1e-8 and cannot be discarded.
    let mut lower = 0.;
    let mut upper = norm(&gradient);
    for _ in 0..100 {
        let t = lower + 0.5 * (upper - lower);
        let value: f64 = gradient
            .iter()
            .enumerate()
            .map(|(i, g)| g * (g / (hessian[i * n + i] + t)))
            .sum();
        if value > t {
            lower = t;
        } else {
            upper = t;
        }
    }
    let shift = lower + 0.5 * (upper - lower);
    let expected: Vec<_> = gradient
        .iter()
        .enumerate()
        .map(|(i, g)| -g / (hessian[i * n + i] + shift))
        .collect();
    let actual = step(&hessian, &gradient).expect("flat positive model must be solved accurately");
    let difference: Vec<_> = actual.iter().zip(&expected).map(|(a, b)| a - b).collect();
    assert!(norm(&difference) < 1e-11 * norm(&expected));
    assert!((actual[n - 1] / expected[n - 1] - 1.).abs() < 1e-11);
}

#[test]
fn coupled_positive_models_match_dense_steps_across_gradient_scales() {
    let n = 32;
    let mut hessian = vec![0.; n * n];
    for i in 0..n {
        hessian[i * n + i] = 1e-4 * (1e4_f64).powf(i as f64 / (n - 1) as f64);
    }
    for k in 0..3 {
        let u: Vec<_> = (0..n)
            .map(|i| 0.1 * (0.31 * (i + 1) as f64 * (k + 1) as f64).sin())
            .collect();
        for i in 0..n {
            for j in 0..n {
                hessian[i * n + j] += u[i] * u[j];
            }
        }
    }
    for scale in [1e-2, 1e-5, 1e-8] {
        let gradient: Vec<_> = (0..n)
            .map(|i| scale * (i as f64 * 0.43 + 0.8).sin())
            .collect();
        let actual = step(&hessian, &gradient).unwrap();
        let dense = super::super::dense_step(&hessian, &gradient).unwrap();
        let difference: Vec<_> = actual.iter().zip(&dense).map(|(a, b)| a - b).collect();
        assert!(norm(&difference) < 1e-7 * norm(&dense));
        let shift = -dot(&gradient, &actual);
        let residual: Vec<_> = product(&hessian, &actual, shift)
            .iter()
            .zip(&gradient)
            .map(|(hs, g)| hs + g)
            .collect();
        assert!(norm(&residual) <= 2e-12 * norm(&gradient));
        assert!(norm(&actual) < 1.);
    }
}

#[test]
fn dispatch_retains_dense_small_and_indefinite_models() {
    let hessian = [1.5, 0., 0., 2.];
    let gradient = [1., -0.1];
    assert_eq!(
        super::super::step(&hessian, &gradient, true),
        super::super::dense_step(&hessian, &gradient),
    );
    let n = 24;
    let mut hessian = vec![0.; n * n];
    for i in 0..n {
        hessian[i * n + i] = 1.;
    }
    hessian[0] = -2.;
    let mut gradient = vec![0.; n];
    gradient[n - 1] = 0.1;
    // The lowest mode is orthogonal to the gradient and homogeneous axis.
    assert!(super::super::step(&hessian, &gradient, false).is_none());
    assert!(super::super::step(&hessian, &vec![0.; n], true).is_none());
}

#[test]
fn changing_energy_units_preserves_the_positive_model_step() {
    let n = 24;
    let mut hessian = vec![0.; n * n];
    for i in 0..n {
        hessian[i * n + i] = 0.1 + i as f64 * 0.05;
        if i > 0 {
            hessian[i * n + i - 1] = 0.02;
            hessian[(i - 1) * n + i] = 0.02;
        }
    }
    let gradient: Vec<_> = (0..n).map(|i| 0.01 * (i as f64 + 0.3).sin()).collect();
    let expected = step(&hessian, &gradient).unwrap();
    for scale in [1e-80, 1e80] {
        let h: Vec<_> = hessian.iter().map(|v| scale * v).collect();
        let g: Vec<_> = gradient.iter().map(|v| scale * v).collect();
        let actual = step(&h, &g).unwrap();
        let difference: Vec<_> = actual.iter().zip(&expected).map(|(a, b)| a - b).collect();
        assert!(norm(&difference) <= 1e-10 * norm(&expected));
    }
}
