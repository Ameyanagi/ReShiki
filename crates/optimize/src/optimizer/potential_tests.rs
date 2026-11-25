//! Deterministic analytic surfaces exercise constrained and molecular optimization.

use super::*;
use crate::{Accuracy, EvalError, Potential};
use std::ops::{Add, Div, Mul, Neg, Sub};

/// A small forward derivative keeps every molecular gradient exact without
/// introducing a finite-difference step or an external force-field dependency.
#[derive(Clone, Copy)]
struct Jet {
    value: f64,
    derivative: [f64; 12],
}

impl Jet {
    fn constant(value: f64) -> Self {
        Self {
            value,
            derivative: [0.0; 12],
        }
    }

    fn variable(value: f64, coordinate: usize) -> Self {
        let mut result = Self::constant(value);
        result.derivative[coordinate] = 1.0;
        result
    }

    fn sqrt(self) -> Self {
        let value = self.value.sqrt();
        Self {
            value,
            derivative: self.derivative.map(|derivative| derivative / (2.0 * value)),
        }
    }

    fn acos(self) -> Self {
        let scale = -(1.0 - self.value * self.value).sqrt().recip();
        Self {
            value: self.value.acos(),
            derivative: self.derivative.map(|derivative| derivative * scale),
        }
    }

    fn square(self) -> Self {
        self * self
    }
}

impl Add for Jet {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Self {
            value: self.value + rhs.value,
            derivative: std::array::from_fn(|i| self.derivative[i] + rhs.derivative[i]),
        }
    }
}

impl Sub for Jet {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self {
        self + -rhs
    }
}

impl Neg for Jet {
    type Output = Self;

    fn neg(self) -> Self {
        Self {
            value: -self.value,
            derivative: self.derivative.map(|derivative| -derivative),
        }
    }
}

#[allow(
    clippy::suspicious_arithmetic_impl,
    reason = "Forward derivatives use the product rule."
)]
impl Mul for Jet {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self {
        Self {
            value: self.value * rhs.value,
            derivative: std::array::from_fn(|i| {
                self.derivative[i] * rhs.value + self.value * rhs.derivative[i]
            }),
        }
    }
}

#[allow(
    clippy::suspicious_arithmetic_impl,
    reason = "Forward derivatives use the quotient rule."
)]
impl Div for Jet {
    type Output = Self;

    fn div(self, rhs: Self) -> Self {
        Self {
            value: self.value / rhs.value,
            derivative: std::array::from_fn(|i| {
                (self.derivative[i] * rhs.value - self.value * rhs.derivative[i])
                    / (rhs.value * rhs.value)
            }),
        }
    }
}

impl Add<f64> for Jet {
    type Output = Self;

    fn add(self, rhs: f64) -> Self {
        self + Self::constant(rhs)
    }
}

impl Sub<f64> for Jet {
    type Output = Self;

    fn sub(self, rhs: f64) -> Self {
        self - Self::constant(rhs)
    }
}

impl Mul<f64> for Jet {
    type Output = Self;

    fn mul(self, rhs: f64) -> Self {
        Self {
            value: self.value * rhs,
            derivative: self.derivative.map(|derivative| derivative * rhs),
        }
    }
}

struct AnalyticPotential<F> {
    energy: F,
}

impl<F: Fn(&[Jet]) -> Jet> Potential for AnalyticPotential<F> {
    fn evaluate(
        &mut self,
        positions: &[f64],
        gradient: &mut [f64],
        _accuracy: Accuracy,
    ) -> Result<f64, EvalError> {
        assert!(positions.len() <= 12);
        let variables: Vec<_> = positions
            .iter()
            .enumerate()
            .map(|(i, &value)| Jet::variable(value, i))
            .collect();
        let result = (self.energy)(&variables);
        gradient.copy_from_slice(&result.derivative[..positions.len()]);
        Ok(result.value)
    }
}

struct FixedGuard<P> {
    inner: P,
    initial: Vec<f64>,
    atoms: Vec<usize>,
    evaluations: usize,
}

impl<P: Potential> Potential for FixedGuard<P> {
    fn evaluate(
        &mut self,
        positions: &[f64],
        gradient: &mut [f64],
        accuracy: Accuracy,
    ) -> Result<f64, EvalError> {
        self.assert_fixed(positions);
        self.evaluations += 1;
        self.inner.evaluate(positions, gradient, accuracy)
    }

    fn hessian(&mut self, positions: &[f64], hessian: &mut [f64]) -> Option<Result<(), EvalError>> {
        self.assert_fixed(positions);
        self.inner.hessian(positions, hessian)
    }
}

impl<P> FixedGuard<P> {
    fn assert_fixed(&self, positions: &[f64]) {
        for &atom in &self.atoms {
            let start = 3 * atom;
            for (axis, (&actual, &initial)) in positions[start..start + 3]
                .iter()
                .zip(&self.initial[start..start + 3])
                .enumerate()
            {
                assert_eq!(
                    actual.to_bits(),
                    initial.to_bits(),
                    "fixed coordinate {} changed at evaluation {}",
                    start + axis,
                    self.evaluations
                );
            }
        }
    }
}

fn optimizer_variants() -> impl Iterator<Item = (Method, Update, Acceleration)> {
    std::iter::once((Method::Lbfgs, Update::Bfgs, Acceleration::None)).chain(
        [Update::Bfgs, Update::DampedBfgs, Update::Bofill]
            .into_iter()
            .flat_map(|update| {
                [
                    Acceleration::None,
                    Acceleration::Gdiis,
                    Acceleration::Gediis,
                ]
                .into_iter()
                .map(move |acceleration| (Method::AncRfo, update, acceleration))
            }),
    )
}

fn atom(positions: &[Jet], index: usize) -> [Jet; 3] {
    std::array::from_fn(|axis| positions[3 * index + axis])
}

fn subtract(a: [Jet; 3], b: [Jet; 3]) -> [Jet; 3] {
    std::array::from_fn(|axis| a[axis] - b[axis])
}

fn dot(a: [Jet; 3], b: [Jet; 3]) -> Jet {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [Jet; 3], b: [Jet; 3]) -> [Jet; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn length(vector: [Jet; 3]) -> Jet {
    dot(vector, vector).sqrt()
}

fn distance(positions: &[Jet], a: usize, b: usize) -> Jet {
    length(subtract(atom(positions, a), atom(positions, b)))
}

fn angle(positions: &[Jet], a: usize, b: usize, c: usize) -> Jet {
    let left = subtract(atom(positions, a), atom(positions, b));
    let right = subtract(atom(positions, c), atom(positions, b));
    (dot(left, right) / (length(left) * length(right))).acos()
}

fn dihedral_cosine(positions: &[Jet]) -> Jet {
    let first = subtract(atom(positions, 1), atom(positions, 0));
    let middle = subtract(atom(positions, 2), atom(positions, 1));
    let last = subtract(atom(positions, 3), atom(positions, 2));
    let left = cross(first, middle);
    let right = cross(middle, last);
    dot(left, right) / (length(left) * length(right))
}

fn values(positions: &[f64]) -> Vec<Jet> {
    positions.iter().copied().map(Jet::constant).collect()
}

fn assert_converged(outcome: &Outcome, options: &Options) {
    assert_eq!(
        outcome.status,
        Status::Converged,
        "{options:?}: status {:?}, steps {}, energy {}",
        outcome.status,
        outcome.steps,
        outcome.energy
    );
    assert_eq!(
        outcome.method, options.method,
        "{options:?}: benchmark converged through an unexpected fallback"
    );
    assert!(
        outcome
            .positions
            .iter()
            .all(|position| position.is_finite())
    );
    assert!(outcome.energy.is_finite());
}

const HARMONIC_REFERENCE: [f64; 9] = [-0.2, 0.1, -0.05, 2.5, 0.1, 0.2, 0.9, 2.0, 0.7];
const FREE_HESSIAN: [[f64; 3]; 3] = [[30.0, 4.0, -1.5], [4.0, 8.0, 1.0], [-1.5, 1.0, 2.5]];
const ANCHOR_COUPLING: [[f64; 6]; 3] = [
    [0.8, -0.3, 0.2, -0.4, 0.1, 0.0],
    [0.1, 0.5, -0.2, 0.2, -0.6, 0.3],
    [-0.2, 0.1, 0.4, 0.3, 0.2, -0.5],
];

fn coupled_harmonic_energy(positions: &[Jet]) -> Jet {
    let displacement: [Jet; 9] = std::array::from_fn(|i| positions[i] - HARMONIC_REFERENCE[i]);
    let mut energy = Jet::constant(0.0);
    for (i, fixed) in displacement[..6].iter().enumerate() {
        energy = energy + fixed.square() * (2.5 + i as f64 * 0.5);
    }
    for i in 0..3 {
        for j in 0..3 {
            energy =
                energy + displacement[6 + i] * displacement[6 + j] * (0.5 * FREE_HESSIAN[i][j]);
        }
        for j in 0..6 {
            energy = energy + displacement[6 + i] * displacement[j] * ANCHOR_COUPLING[i][j];
        }
    }
    energy
}

fn determinant(matrix: [[f64; 3]; 3]) -> f64 {
    matrix[0][0] * (matrix[1][1] * matrix[2][2] - matrix[1][2] * matrix[2][1])
        - matrix[0][1] * (matrix[1][0] * matrix[2][2] - matrix[1][2] * matrix[2][0])
        + matrix[0][2] * (matrix[1][0] * matrix[2][1] - matrix[1][1] * matrix[2][0])
}

fn harmonic_constrained_minimum(initial: &[f64]) -> [f64; 3] {
    let rhs: [f64; 3] = std::array::from_fn(|i| {
        -(0..6)
            .map(|j| ANCHOR_COUPLING[i][j] * (initial[j] - HARMONIC_REFERENCE[j]))
            .sum::<f64>()
    });
    // Cramer's rule is independent of the optimizer's linear algebra.
    std::array::from_fn(|coordinate| {
        let mut numerator = FREE_HESSIAN;
        for row in 0..3 {
            numerator[row][coordinate] = rhs[row];
        }
        HARMONIC_REFERENCE[6 + coordinate] + determinant(numerator) / determinant(FREE_HESSIAN)
    })
}

#[derive(Default)]
struct CoupledHarmonic {
    gradient_evaluations: usize,
    hessian_evaluations: usize,
}

impl Potential for CoupledHarmonic {
    fn evaluate(
        &mut self,
        positions: &[f64],
        gradient: &mut [f64],
        accuracy: Accuracy,
    ) -> Result<f64, EvalError> {
        self.gradient_evaluations += 1;
        AnalyticPotential {
            energy: coupled_harmonic_energy,
        }
        .evaluate(positions, gradient, accuracy)
    }

    fn hessian(&mut self, positions: &[f64], hessian: &mut [f64]) -> Option<Result<(), EvalError>> {
        assert_eq!(positions.len(), 9);
        assert_eq!(hessian.len(), 81);
        self.hessian_evaluations += 1;
        hessian.fill(0.0);
        for i in 0..6 {
            hessian[i * 9 + i] = 5.0 + i as f64;
        }
        for i in 0..3 {
            for j in 0..3 {
                hessian[(6 + i) * 9 + 6 + j] = FREE_HESSIAN[i][j];
            }
            for j in 0..6 {
                hessian[(6 + i) * 9 + j] = ANCHOR_COUPLING[i][j];
                hessian[j * 9 + 6 + i] = ANCHOR_COUPLING[i][j];
            }
        }
        Some(Ok(()))
    }
}

fn run_coupled_harmonic_with_hessian(initial_hessian: InitialHessian) -> Outcome {
    let initial = [0.0, -0.0, 0.0, 2.4, 0.2, 0.1, 0.4, 2.7, -0.3];
    let expected = harmonic_constrained_minimum(&initial);
    let mut potential = FixedGuard {
        inner: CoupledHarmonic::default(),
        initial: initial.to_vec(),
        atoms: vec![0, 1],
        evaluations: 0,
    };
    let options = Options {
        initial_hessian,
        level: Level::Tight,
        fixed_atoms: vec![0, 1],
        ..Options::default()
    };
    let outcome = optimize(&mut potential, &[6, 6, 6], &initial, &options, |step| {
        assert_fixed_anchors(step.positions, &initial);
        ControlFlow::Continue(())
    });
    assert_converged(&outcome, &options);
    assert_fixed_anchors(&outcome.positions, &initial);
    for (actual, expected) in outcome.positions[6..].iter().zip(expected) {
        assert!(
            (actual - expected).abs() < 1.0e-6,
            "{initial_hessian:?}: {:?}, expected {expected}",
            outcome.positions
        );
    }
    let mut minimum = initial;
    minimum[6..].copy_from_slice(&expected);
    let minimum_energy = coupled_harmonic_energy(&values(&minimum)).value;
    assert!((outcome.energy - minimum_energy).abs() < 1.0e-11);
    assert!(
        outcome.gradient[..6]
            .iter()
            .any(|value| value.abs() > 1.0e-2)
    );
    assert_eq!(outcome.evaluations, potential.evaluations);
    assert_eq!(outcome.evaluations, potential.inner.gradient_evaluations);
    assert_eq!(
        outcome.hessian_evaluations,
        potential.inner.hessian_evaluations
    );
    outcome
}

fn assert_fixed_anchors(positions: &[f64], initial: &[f64]) {
    for (actual, initial) in positions[..6].iter().zip(&initial[..6]) {
        assert_eq!(actual.to_bits(), initial.to_bits());
    }
}

#[test]
fn exact_coupled_harmonic_hessian_uses_fewer_gradient_evaluations_than_lindh() {
    let model = run_coupled_harmonic_with_hessian(InitialHessian::Model);
    let potential = run_coupled_harmonic_with_hessian(InitialHessian::Potential);
    assert_eq!(model.hessian_evaluations, 0);
    assert_eq!(potential.hessian_evaluations, 1);
    assert!(
        potential.evaluations < model.evaluations,
        "exact Hessian used {} gradients, Lindh used {}",
        potential.evaluations,
        model.evaluations
    );
}

#[test]
fn automatic_hessian_threshold_counts_all_atoms_including_fixed_anchors() {
    let model = run_coupled_harmonic_with_hessian(InitialHessian::Model);
    let above_threshold = run_coupled_harmonic_with_hessian(InitialHessian::Auto { max_atoms: 2 });
    assert_eq!(above_threshold.hessian_evaluations, 0);
    assert_eq!(above_threshold.evaluations, model.evaluations);
    assert_eq!(above_threshold.positions, model.positions);

    let potential = run_coupled_harmonic_with_hessian(InitialHessian::Potential);
    let at_threshold = run_coupled_harmonic_with_hessian(InitialHessian::Auto { max_atoms: 3 });
    assert_eq!(at_threshold.hessian_evaluations, 1);
    assert_eq!(at_threshold.evaluations, potential.evaluations);
    assert_eq!(at_threshold.positions, potential.positions);
}

#[test]
fn unanchored_coupled_harmonic_reaches_known_cartesian_coordinates() {
    let reference = [-2.25, 0.0, 0.0, 0.25, 0.0, 0.0, 2.0, 0.0, 0.0];
    let initial = [-2.75, 0.0, 0.0, -0.5, 0.0, 0.0, 3.25, 0.0, 0.0];
    // Zero-centroid axial displacements stay perpendicular to all rigid modes.
    // Distinct stretch curvatures and their coupling move all three atoms.
    for (method, update, acceleration) in optimizer_variants() {
        let mut potential = AnalyticPotential {
            energy: |positions: &[Jet]| {
                let displacement: [Jet; 9] = std::array::from_fn(|i| positions[i] - reference[i]);
                let mut energy = Jet::constant(0.0);
                for (axis, stiffness) in [3.0, 1.5, 0.75].into_iter().enumerate() {
                    let left = displacement[3 + axis] - displacement[axis];
                    let right = displacement[6 + axis] - displacement[3 + axis];
                    energy = energy
                        + (left.square() * 4.0 + right.square() * 1.5 + left * right * 0.8)
                            * stiffness;
                }
                energy
            },
        };
        let options = Options {
            method,
            update,
            acceleration,
            level: Level::Tight,
            max_steps: 200,
            ..Options::default()
        };
        let outcome = optimize(&mut potential, &[6, 6, 6], &initial, &options, |_| {
            ControlFlow::Continue(())
        });
        assert_converged(&outcome, &options);
        for (&actual, expected) in outcome.positions.iter().zip(reference) {
            assert!(
                (actual - expected).abs() < 1.0e-5,
                "{method:?}: positions {:?}, expected {reference:?}",
                outcome.positions
            );
        }
    }
}

#[test]
fn coupled_anisotropic_harmonic_reaches_the_constrained_minimum() {
    // Two fixed anchors establish a Cartesian frame. Their displacement from
    // the full potential's minimum shifts the free atom's minimum through B.
    let initial = [0.0, -0.0, 0.0, 2.4, 0.2, 0.1, 0.4, 2.7, -0.3];
    let expected = harmonic_constrained_minimum(&initial);
    assert!((expected[2] - HARMONIC_REFERENCE[8]).abs() > 1.0e-3);
    for (method, update, acceleration) in optimizer_variants() {
        let mut potential = FixedGuard {
            inner: AnalyticPotential {
                energy: coupled_harmonic_energy,
            },
            initial: initial.to_vec(),
            atoms: vec![0, 1],
            evaluations: 0,
        };
        let options = Options {
            method,
            update,
            acceleration,
            level: Level::Tight,
            max_steps: 400,
            fixed_atoms: vec![0, 1],
            ..Options::default()
        };
        let outcome = optimize(&mut potential, &[6, 6, 6], &initial, &options, |_| {
            ControlFlow::Continue(())
        });
        assert_converged(&outcome, &options);
        for (&actual, &original) in outcome.positions[..6].iter().zip(&initial[..6]) {
            assert_eq!(actual.to_bits(), original.to_bits());
        }
        for coordinate in 0..3 {
            assert!(
                (outcome.positions[6 + coordinate] - expected[coordinate]).abs() < 1.0e-5,
                "{method:?}: free position {:?}, expected {expected:?}",
                &outcome.positions[6..]
            );
        }
        assert!(potential.evaluations > 2);
        assert_eq!(outcome.evaluations, potential.evaluations);
        let mut gradient = [0.0; 9];
        potential
            .evaluate(&outcome.positions, &mut gradient, Accuracy::Tight)
            .unwrap();
        assert!(gradient[..6].iter().any(|value| value.abs() > 1.0e-2));
    }
}

fn rosenbrock_energy(positions: &[Jet]) -> Jet {
    let x = positions[6];
    let y = positions[7];
    ((x - 1.0).square() + (y - x.square()).square() * 25.0) * 10.0
        + (positions[8] - 0.5).square() * 2.0
}

#[test]
fn both_methods_follow_a_coupled_rosenbrock_valley() {
    let initial = [-3.0, -0.0, 0.0, -3.0, 2.0, 0.0, -1.1, 1.2, 0.2];
    for (method, update, acceleration) in optimizer_variants() {
        let mut potential = FixedGuard {
            inner: AnalyticPotential {
                energy: rosenbrock_energy,
            },
            initial: initial.to_vec(),
            atoms: vec![0, 1],
            evaluations: 0,
        };
        let options = Options {
            method,
            update,
            acceleration,
            level: Level::Tight,
            max_steps: 800,
            fixed_atoms: vec![0, 1],
            ..Options::default()
        };
        let outcome = optimize(&mut potential, &[6, 6, 6], &initial, &options, |_| {
            ControlFlow::Continue(())
        });
        assert_converged(&outcome, &options);
        for (actual, expected) in outcome.positions[6..].iter().zip([1.0, 1.0, 0.5]) {
            assert!(
                (actual - expected).abs() < 1.0e-4,
                "{method:?}: {:?}",
                outcome.positions
            );
        }
        assert!(outcome.energy < 1.0e-8, "{method:?}: {}", outcome.energy);
    }
}

const TORSION_BOND: f64 = 2.8;
const TORSION_ANGLE: f64 = 1.910_633_236_249_018_6;

fn torsion_energy(positions: &[Jet]) -> Jet {
    let mut energy = Jet::constant(0.0);
    for pair in 0..3 {
        energy = energy + (distance(positions, pair, pair + 1) - TORSION_BOND).square() * 2.0;
    }
    for first in 0..2 {
        energy = energy
            + (angle(positions, first, first + 1, first + 2) - TORSION_ANGLE).square() * 0.75;
    }
    let cosine = dihedral_cosine(positions);
    // cos(3 phi) = 4 cos(phi)^3 - 3 cos(phi), avoiding a dihedral branch cut.
    energy + (cosine.square() * cosine * 4.0 - cosine * 3.0 + 1.0) * 0.02
}

fn chain_geometry(bonds: [f64; 3], angles: [f64; 2], phi: f64) -> [f64; 12] {
    [
        bonds[0] * angles[0].sin(),
        bonds[0] * angles[0].cos(),
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        bonds[1],
        0.0,
        bonds[2] * angles[1].sin() * phi.cos(),
        bonds[1] - bonds[2] * angles[1].cos(),
        bonds[2] * angles[1].sin() * phi.sin(),
    ]
}

#[test]
fn a_stiff_four_atom_chain_relaxes_to_a_staggered_torsion() {
    let initial = chain_geometry([2.95, 2.62, 2.90], [1.78, 2.04], 0.43);
    for (method, update, acceleration) in optimizer_variants() {
        let mut potential = AnalyticPotential {
            energy: torsion_energy,
        };
        let options = Options {
            method,
            update,
            acceleration,
            level: Level::Tight,
            max_steps: 600,
            ..Options::default()
        };
        let outcome = optimize(&mut potential, &[6; 4], &initial, &options, |_| {
            ControlFlow::Continue(())
        });
        assert_converged(&outcome, &options);
        let geometry = values(&outcome.positions);
        for first in 0..3 {
            assert!((distance(&geometry, first, first + 1).value - TORSION_BOND).abs() < 1.0e-4);
        }
        for first in 0..2 {
            assert!(
                (angle(&geometry, first, first + 1, first + 2).value - TORSION_ANGLE).abs()
                    < 1.0e-4
            );
        }
        let cosine = dihedral_cosine(&geometry).value;
        let cosine_triple = 4.0 * cosine.powi(3) - 3.0 * cosine;
        assert!(
            cosine_triple < -1.0 + 1.0e-7,
            "{method:?}: cos(3 phi)={cosine_triple}"
        );
        assert!(outcome.energy < 1.0e-8, "{method:?}: {}", outcome.energy);
    }
}

fn carbon_dioxide_energy(positions: &[Jet]) -> Jet {
    let left = subtract(atom(positions, 0), atom(positions, 1));
    let right = subtract(atom(positions, 2), atom(positions, 1));
    let left_length = length(left);
    let right_length = length(right);
    (left_length - 2.2).square() * 3.0
        + (right_length - 2.2).square() * 3.0
        + (dot(left, right) / (left_length * right_length) + 1.0) * 0.8
}

#[test]
fn linear_and_bent_carbon_dioxide_geometries_converge() {
    let initial_geometries = [
        [-2.5, 0.0, 0.0, 0.1, 0.0, 0.0, 2.0, 0.0, 0.0],
        [-2.2, 0.3, 0.1, 0.1, -0.2, 0.0, 2.0, 0.4, -0.1],
    ];
    for (method, update, acceleration) in optimizer_variants() {
        for initial in initial_geometries {
            let mut potential = AnalyticPotential {
                energy: carbon_dioxide_energy,
            };
            let options = Options {
                method,
                update,
                acceleration,
                level: Level::Tight,
                max_steps: 400,
                ..Options::default()
            };
            let outcome = optimize(&mut potential, &[8, 6, 8], &initial, &options, |_| {
                ControlFlow::Continue(())
            });
            assert_converged(&outcome, &options);
            let geometry = values(&outcome.positions);
            for end in [0, 2] {
                assert!((distance(&geometry, end, 1).value - 2.2).abs() < 1.0e-4);
            }
            let left = subtract(atom(&geometry, 0), atom(&geometry, 1));
            let right = subtract(atom(&geometry, 2), atom(&geometry, 1));
            let cosine = (dot(left, right) / (length(left) * length(right))).value;
            assert!(cosine < -1.0 + 1.0e-7, "{method:?}: angle cosine={cosine}");
            assert!(outcome.energy < 1.0e-8, "{method:?}: {}", outcome.energy);
        }
    }
}

fn disconnected_energy(positions: &[Jet]) -> Jet {
    (distance(positions, 0, 1) - 1.8).square() * 2.0
        + (distance(positions, 2, 3) - 2.6).square() * 5.0
}

#[test]
fn disconnected_fragments_relax_without_a_linking_potential() {
    let initial = [
        -7.2, 0.2, 0.1, -4.7, -0.3, 0.0, 5.2, 1.0, 0.4, 6.8, 1.8, -0.2,
    ];
    for (method, update, acceleration) in optimizer_variants() {
        let mut potential = AnalyticPotential {
            energy: disconnected_energy,
        };
        let options = Options {
            method,
            update,
            acceleration,
            level: Level::Tight,
            max_steps: 400,
            ..Options::default()
        };
        let outcome = optimize(&mut potential, &[1, 1, 17, 17], &initial, &options, |_| {
            ControlFlow::Continue(())
        });
        assert_converged(&outcome, &options);
        let geometry = values(&outcome.positions);
        assert!((distance(&geometry, 0, 1).value - 1.8).abs() < 1.0e-5);
        assert!((distance(&geometry, 2, 3).value - 2.6).abs() < 1.0e-5);
        assert!(outcome.energy < 1.0e-9, "{method:?}: {}", outcome.energy);
    }
}
