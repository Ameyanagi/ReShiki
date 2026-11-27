//! Climbing-image NEB with the improved tangent of Henkelman & Jónsson, J. Chem. Phys. 113, 9978
//! (2000) \[Henkelman2000Tangent\], and climbing force of Henkelman, Uberuaga & Jónsson, J. Chem.
//! Phys. 113, 9901 (2000) \[Henkelman2000CINEB\]. The band is relaxed with FIRE (Bitzek et al.,
//! Phys. Rev. Lett. 97, 170201, 2006 \[Bitzek2006FIRE\]), using unit artificial masses and a
//! 0.2-bohr per-atom step cap. A fixed initial force scale sets the fictitious time unit and
//! scales equal springs from 0.1 Eh/bohr² upward on unusually stiff surfaces (including unscaled
//! analytic test potentials).

use std::ops::ControlFlow;

use crate::{
    Potential,
    linalg::{dot, norm},
};

use super::{Event, Minimum, TsError, TsOptions, emit, evaluate, mapping};

pub(super) struct Band {
    pub(super) path: Vec<Minimum>,
    pub(super) converged: bool,
    pub(super) climbing: usize,
    pub(super) tangent: Vec<f64>,
}

/// Optional local seeds, highest energy first. A midpoint bracketed by lower
/// link endpoints is direct evidence of a missed peak and may raise the handoff
/// reference above the original ceiling.
pub(super) struct RecoveryProbe {
    pub(super) seeds: Vec<RecoverySeed>,
    pub(super) ceiling_exhausted: bool,
    pub(super) diagnostics: Vec<String>,
}

pub(super) struct RecoverySeed {
    pub(super) point: Minimum,
    pub(super) gradient: Vec<f64>,
    pub(super) direction: Vec<f64>,
}

impl Band {
    /// Restore the handoff from an already validated molecular band. The same
    /// highest-image choice and improved tangent are used by a fresh NEB run.
    pub(super) fn from_saved(path: &[Minimum], masses: &[f64], converged: bool) -> Self {
        let energies: Vec<_> = path.iter().map(|point| point.energy).collect();
        let positions: Vec<_> = path.iter().map(|point| point.positions.clone()).collect();
        let climbing = highest(&energies);
        let weights = vec![1.0; masses.len()];
        let (tangent, _) = tangent(&positions, &energies, climbing, &weights);
        Self {
            path: path.to_vec(),
            converged,
            climbing,
            tangent,
        }
    }

    /// Probe unresolved peaks next to the climbing image without changing the band.
    /// Adaptive insertion near a transition state follows the motivation of
    /// Kolsbjerg, Groves & Hammer, J. Chem. Phys. 145, 094107 (2016)
    /// \[Kolsbjerg2016AutoNEB\]. The two-midpoint selection rule here is our own.
    /// Molecular neighbors use a unit-weight fit in the climbing image's frame.
    /// The returned unit directions follow increasing image index.
    pub(super) fn intermediate_peaks(
        &self,
        p: &mut impl Potential,
        masses: &[f64],
        ceiling: f64,
    ) -> Result<RecoveryProbe, TsError> {
        let center = &self.path[self.climbing];
        let weights = if center.positions.len() == 3 * masses.len() {
            vec![1.0; masses.len()]
        } else {
            Vec::new()
        };
        // This exceeds ordinary SCF energy noise while remaining negligible
        // compared with a chemical barrier. Include a roundoff floor for large
        // constant offsets on generic analytic surfaces.
        let tolerance = 1e-8_f64.max(64.0 * f64::EPSILON * center.energy.abs());
        let mut result = RecoveryProbe {
            seeds: Vec::new(),
            ceiling_exhausted: false,
            diagnostics: Vec::new(),
        };
        for neighbor in [self.climbing - 1, self.climbing + 1] {
            let side = if neighbor < self.climbing {
                "left"
            } else {
                "right"
            };
            let positions = &self.path[neighbor].positions;
            let aligned = if weights.is_empty() {
                positions.clone()
            } else {
                mapping::superpose(&center.positions, positions, &weights)
            };
            let mut direction = subtract(&aligned, &center.positions);
            let length = norm(&direction);
            if length <= 1e-14 {
                continue;
            }
            let midpoint: Vec<_> = center
                .positions
                .iter()
                .zip(&direction)
                .map(|(position, displacement)| position + 0.5 * displacement)
                .collect();
            let (energy, gradient) = match evaluate(p, &midpoint) {
                Ok(evaluated) => evaluated,
                Err(error @ TsError::Scf(_)) => {
                    result
                        .diagnostics
                        .push(format!("{side} midpoint recovery probe: {error}"));
                    continue;
                }
                Err(error) => return Err(error),
            };
            if energy - center.energy <= tolerance {
                continue;
            }
            let bracketed = energy - self.path[neighbor].energy > tolerance;
            if energy > ceiling && !bracketed {
                result.ceiling_exhausted = true;
                result.diagnostics.push(format!(
                    "{side} midpoint exceeds saddle energy ceiling ({energy:.12} > {ceiling:.12} Eh)"
                ));
                continue;
            }
            let sign = if neighbor < self.climbing { -1.0 } else { 1.0 };
            direction
                .iter_mut()
                .for_each(|value| *value *= sign / length);
            result.seeds.push(RecoverySeed {
                point: Minimum {
                    positions: midpoint,
                    energy,
                },
                gradient,
                direction,
            });
        }
        // Stable sorting preserves the left-link tie break used by the original
        // two-midpoint rule, while retaining the other link if a seed fails SCF
        // or its exact stationary-point index disqualifies the handoff.
        result
            .seeds
            .sort_by(|a, b| b.point.energy.total_cmp(&a.point.energy));
        Ok(result)
    }
}

/// Endpoints and their already-computed energies remain fixed. Non-Cartesian
/// vectors (for example the two-dimensional Müller–Brown surface) use the same
/// numerical algorithm without molecular alignment.
pub(super) fn run<P: Potential + Send>(
    potentials: &mut [P],
    mut band: Vec<Vec<f64>>,
    endpoints: [f64; 2],
    masses: &[f64],
    options: &TsOptions,
    on_event: &mut impl FnMut(Event) -> ControlFlow<()>,
) -> Result<Band, TsError> {
    let images = band.len();
    let dimensions = band[0].len();
    let weights = if dimensions == 3 * masses.len() {
        vec![1.0; masses.len()]
    } else {
        Vec::new()
    };
    let mut fire = Fire::new(images, dimensions);
    let mut climbing = None;
    let mut energies = vec![0.0; images];
    energies[0] = endpoints[0];
    energies[images - 1] = endpoints[1];
    let mut gradients = vec![vec![0.0; dimensions]; images];
    let mut force_scale = 1.0_f64;

    for iteration in 0..=options.neb_max_iterations {
        evaluate_images(potentials, &band, &mut energies, &mut gradients)?;
        if iteration == 0 {
            force_scale = gradients.iter().map(|g| maximum(g)).fold(1.0, f64::max);
        }
        let highest = highest(&energies);
        let ordinary = forces(
            &band,
            &energies,
            &gradients,
            0.1 * force_scale,
            None,
            &weights,
        );
        // A short ordinary-NEB phase gives the tangent time to settle; a fixed
        // 30-iteration limit also activates climbing on very stiff surfaces.
        let activate = iteration >= 5 && (ordinary.max_perpendicular < 0.05 || iteration >= 30);
        if climbing.is_some() || activate {
            if climbing != Some(highest) {
                fire.reset();
            }
            climbing = Some(highest);
        }
        let projected = if climbing.is_some() {
            forces(
                &band,
                &energies,
                &gradients,
                0.1 * force_scale,
                climbing,
                &weights,
            )
        } else {
            ordinary
        };
        let converged = climbing.is_some()
            && projected.max_perpendicular <= 0.01
            && maximum(&projected.values[highest]) <= 0.005
            && projected.maximum <= 0.01;
        emit(
            on_event,
            Event::Neb {
                iteration,
                energies: &energies,
                max_force: projected.maximum,
                climbing,
                finished: converged || iteration == options.neb_max_iterations,
            },
        )?;
        if converged || iteration == options.neb_max_iterations {
            let (tangent, _) = tangent(&band, &energies, highest, &weights);
            return Ok(Band {
                path: band
                    .into_iter()
                    .zip(energies)
                    .map(|(positions, energy)| Minimum { positions, energy })
                    .collect(),
                converged,
                climbing: highest,
                tangent,
            });
        }
        let scaled: Vec<Vec<f64>> = projected
            .values
            .iter()
            .map(|image| image.iter().map(|force| force / force_scale).collect())
            .collect();
        fire.advance(&mut band, &scaled, masses, 0.2)?;
    }
    unreachable!("the final iteration returns the evaluated band")
}

/// Each worker owns its potential and evaluates ascending image indices. This
/// preserves the specified image i -> potential i % workers assignment without
/// locking stateful potentials or changing reduction order with worker count.
fn evaluate_images<P: Potential + Send>(
    potentials: &mut [P],
    band: &[Vec<f64>],
    energies: &mut [f64],
    gradients: &mut [Vec<f64>],
) -> Result<(), TsError> {
    let workers = potentials.len();
    if workers == 1 {
        for i in 1..band.len() - 1 {
            let (energy, gradient) = evaluate(&mut potentials[0], &band[i])?;
            energies[i] = energy;
            gradients[i] = gradient;
        }
        return Ok(());
    }
    let evaluated = std::thread::scope(|scope| {
        let handles: Vec<_> = potentials
            .iter_mut()
            .enumerate()
            .filter(|(worker, _)| (1..band.len() - 1).any(|i| i % workers == *worker))
            .map(|(worker, potential)| {
                scope.spawn(move || {
                    let mut results = Vec::new();
                    for i in (1..band.len() - 1).filter(|i| i % workers == worker) {
                        let result = evaluate(potential, &band[i]);
                        let failed = result.is_err();
                        results.push((i, result));
                        if failed {
                            break;
                        }
                    }
                    results
                })
            })
            .collect();
        let mut results = Vec::new();
        for handle in handles {
            results.extend(
                handle
                    .join()
                    .map_err(|_| TsError::Failed("NEB evaluation worker panicked".into()))?,
            );
        }
        Ok::<_, TsError>(results)
    })?;
    let mut evaluated = evaluated;
    evaluated.sort_by_key(|(i, _)| *i);
    for (i, result) in evaluated {
        let (energy, gradient) = result?;
        energies[i] = energy;
        gradients[i] = gradient;
    }
    Ok(())
}

fn highest(energies: &[f64]) -> usize {
    (2..energies.len() - 1).fold(1, |best, i| {
        if energies[i] > energies[best] {
            i
        } else {
            best
        }
    })
}

pub(super) struct Forces {
    pub(super) values: Vec<Vec<f64>>,
    pub(super) maximum: f64,
    pub(super) max_perpendicular: f64,
}

pub(super) fn forces(
    band: &[Vec<f64>],
    energies: &[f64],
    gradients: &[Vec<f64>],
    spring: f64,
    climbing: Option<usize>,
    weights: &[f64],
) -> Forces {
    let mut result = Forces {
        values: vec![vec![0.0; band[0].len()]; band.len()],
        maximum: 0.0,
        max_perpendicular: 0.0,
    };
    for (i, gradient) in gradients.iter().enumerate().take(band.len() - 1).skip(1) {
        let (tangent, [forward, backward]) = tangent(band, energies, i, weights);
        let parallel = dot(gradient, &tangent);
        let spring_force = spring * (forward - backward);
        for (k, (&g, &t)) in gradient.iter().zip(&tangent).enumerate() {
            let perpendicular = -g + parallel * t;
            result.max_perpendicular = result.max_perpendicular.max(perpendicular.abs());
            result.values[i][k] = if climbing == Some(i) {
                -g + 2.0 * parallel * t
            } else {
                perpendicular + spring_force * t
            };
        }
        result.maximum = result.maximum.max(maximum(&result.values[i]));
    }
    result
}

/// Eqs. (8)–(11) of Henkelman & Jónsson (2000). Flat-energy plateaus use a
/// centered secant. Coincident images search progressively wider secants; an
/// entirely coincident band has a zero tangent and retains the physical force.
fn tangent(band: &[Vec<f64>], energies: &[f64], i: usize, weights: &[f64]) -> (Vec<f64>, [f64; 2]) {
    let forward = link(band, i, i + 1, weights);
    let backward: Vec<_> = link(band, i, i - 1, weights)
        .into_iter()
        .map(|v| -v)
        .collect();
    let lengths = [norm(&forward), norm(&backward)];
    let mut tangent = if energies[i + 1] > energies[i] && energies[i] > energies[i - 1] {
        forward.clone()
    } else if energies[i + 1] < energies[i] && energies[i] < energies[i - 1] {
        backward.clone()
    } else {
        let a = (energies[i + 1] - energies[i]).abs();
        let b = (energies[i - 1] - energies[i]).abs();
        let largest = a.max(b);
        // Scale the weights to avoid losing a geometrically valid secant on
        // very small energy scales or overflowing on very large ones.
        let smallest = if largest > 0.0 {
            a.min(b) / largest
        } else {
            1.0
        };
        let (w_forward, w_backward) = if energies[i + 1] >= energies[i - 1] {
            (1.0, smallest)
        } else {
            (smallest, 1.0)
        };
        forward
            .iter()
            .zip(&backward)
            .map(|(a, b)| w_forward * a + w_backward * b)
            .collect()
    };
    if norm(&tangent) <= 1e-14 {
        // A centered secant can also cancel at a cusp. Prefer a nonzero local
        // link before looking beyond neighboring images.
        tangent = if norm(&forward) >= norm(&backward) {
            forward
        } else {
            backward
        };
        if norm(&tangent) <= 1e-14 {
            for offset in 2..band.len() {
                if i + offset < band.len() {
                    tangent = link(band, i, i + offset, weights);
                }
                if norm(&tangent) <= 1e-14 && offset <= i {
                    tangent = link(band, i, i - offset, weights)
                        .into_iter()
                        .map(|v| -v)
                        .collect();
                }
                if norm(&tangent) > 1e-14 {
                    break;
                }
            }
        }
    }
    let length = norm(&tangent);
    if length > 0.0 {
        for value in &mut tangent {
            *value /= length;
        }
    }
    (tangent, lengths)
}

pub(super) fn maximum(values: &[f64]) -> f64 {
    values
        .iter()
        .fold(0.0_f64, |largest, value| largest.max(value.abs()))
}

fn subtract(a: &[f64], b: &[f64]) -> Vec<f64> {
    a.iter().zip(b).map(|(a, b)| a - b).collect()
}

/// A molecular link compares shapes in the center image's frame. Unit weights
/// match the Cartesian metric of the gradients: the optimal fit makes the link
/// orthogonal to rigid translations and rotations at the center. Raw secants
/// in a mass-weighted display frame would retain artificial rigid components.
fn link(band: &[Vec<f64>], center: usize, neighbor: usize, weights: &[f64]) -> Vec<f64> {
    if weights.is_empty() {
        subtract(&band[neighbor], &band[center])
    } else {
        subtract(
            &mapping::superpose(&band[center], &band[neighbor], weights),
            &band[center],
        )
    }
}

/// FIRE parameters from Bitzek et al. (2006): Nmin=5, finc=1.1, fdec=0.5,
/// alpha_start=0.1, falpha=0.99. Symplectic Euler supplies the MD propagation;
/// fictitious time starts at 0.05 and is capped at 1.0 in atomic-unit inputs.
pub(super) struct Fire {
    velocity: Vec<Vec<f64>>,
    dt: f64,
    alpha: f64,
    positive: usize,
}

impl Fire {
    pub(super) fn new(images: usize, dimensions: usize) -> Self {
        Self {
            velocity: vec![vec![0.0; dimensions]; images],
            dt: 0.05,
            alpha: 0.1,
            positive: 0,
        }
    }

    fn reset(&mut self) {
        for velocity in &mut self.velocity {
            velocity.fill(0.0);
        }
        self.dt = 0.05;
        self.alpha = 0.1;
        self.positive = 0;
    }

    pub(super) fn advance(
        &mut self,
        band: &mut [Vec<f64>],
        forces: &[Vec<f64>],
        masses: &[f64],
        step_cap: f64,
    ) -> Result<(), TsError> {
        let interior = 1..band.len() - 1;
        let power = interior
            .clone()
            .map(|i| dot(&self.velocity[i], &forces[i]))
            .sum::<f64>();
        if power > 0.0 {
            let speed = interior
                .clone()
                .map(|i| norm(&self.velocity[i]).powi(2))
                .sum::<f64>()
                .sqrt();
            let force = interior
                .clone()
                .map(|i| norm(&forces[i]).powi(2))
                .sum::<f64>()
                .sqrt();
            for i in interior.clone() {
                for (velocity, force_component) in self.velocity[i].iter_mut().zip(&forces[i]) {
                    *velocity = (1.0 - self.alpha) * *velocity
                        + self.alpha * speed / force * force_component;
                }
            }
            self.positive += 1;
            if self.positive > 5 {
                self.dt = (self.dt * 1.1).min(1.0);
                self.alpha *= 0.99;
            }
        } else {
            self.dt *= 0.5;
            self.alpha = 0.1;
            self.positive = 0;
            for i in interior.clone() {
                self.velocity[i].fill(0.0);
            }
        }
        let molecular = !masses.is_empty() && band[0].len() == 3 * masses.len();
        let weights = vec![1.0; masses.len()];
        for i in interior {
            for (velocity, force) in self.velocity[i].iter_mut().zip(&forces[i]) {
                *velocity += self.dt * force;
            }
            let largest_step = if molecular {
                self.velocity[i]
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .map(|v| self.dt * norm(v))
                    .fold(0.0_f64, f64::max)
            } else {
                self.dt * norm(&self.velocity[i])
            };
            let scale = if largest_step > step_cap {
                step_cap / largest_step
            } else {
                1.0
            };
            let old = band[i].clone();
            for ((coordinate, velocity), &previous) in
                band[i].iter_mut().zip(&mut self.velocity[i]).zip(&old)
            {
                *velocity *= scale;
                *coordinate = previous + self.dt * *velocity;
            }
            if molecular {
                // Use the same fixed reference as the superposed product.
                // Independent fits avoid accumulated rotations along the band.
                band[i] = mapping::superpose(&band[0], &band[i], masses);
                // Velocities use physical motion in the new image's frame,
                // excluding the rigid shift introduced by the display fit.
                let previous = mapping::superpose(&band[i], &old, &weights);
                for ((velocity, &coordinate), &previous) in
                    self.velocity[i].iter_mut().zip(&band[i]).zip(&previous)
                {
                    *velocity = (coordinate - previous) / self.dt;
                }
            }
            if band[i].iter().any(|value| !value.is_finite()) {
                return Err(TsError::Failed("NEB displacement became non-finite".into()));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Accuracy, EvalError};

    struct HiddenPeaks {
        calls: Vec<f64>,
    }

    impl Potential for HiddenPeaks {
        fn evaluate(
            &mut self,
            positions: &[f64],
            gradient: &mut [f64],
            _: Accuracy,
        ) -> Result<f64, EvalError> {
            let x = positions[0];
            self.calls.push(x);
            gradient[0] = -4.0 * x * (x * x - 0.25) + 0.05;
            Ok(-(x * x - 0.25).powi(2) + 0.05 * x)
        }
    }

    #[test]
    fn intermediate_peak_recovers_the_highest_allowed_unbracketed_midpoint() {
        let band = Band {
            path: [-1.0_f64, 0.0, 1.0]
                .into_iter()
                .map(|x| Minimum {
                    positions: vec![x],
                    // Higher link endpoints do not bracket the hidden peak.
                    energy: if x == 0.0 { -0.0625 } else { 1.0 },
                })
                .collect(),
            converged: false,
            climbing: 1,
            tangent: vec![1.0],
        };
        for (ceiling, expected) in [(1.0, Some(0.5)), (0.0, Some(-0.5)), (-0.03, None)] {
            let mut potential = HiddenPeaks { calls: Vec::new() };
            let result = band
                .intermediate_peaks(&mut potential, &[], ceiling)
                .unwrap()
                .seeds
                .into_iter()
                .next();
            assert_eq!(potential.calls, [-0.5, 0.5]);
            assert_eq!(
                result.as_ref().map(|seed| seed.point.positions[0]),
                expected
            );
            if let Some(seed) = result {
                assert!(seed.point.energy > band.path[band.climbing].energy);
                assert_eq!(seed.direction, [1.0]);
            }
        }
        assert_eq!(band.path[1].positions, [0.0]);
    }

    #[test]
    fn intermediate_peak_skips_links_that_only_rotate_and_translate() {
        struct MustNotEvaluate;
        impl Potential for MustNotEvaluate {
            fn evaluate(
                &mut self,
                _: &[f64],
                _: &mut [f64],
                _: Accuracy,
            ) -> Result<f64, EvalError> {
                panic!("rigid copies do not contain an intermediate shape");
            }
        }
        let center = vec![-1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 2.0, 0.0];
        let rotated: Vec<_> = center
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [7.0 - p[1], -3.0 + p[0], 2.0 + p[2]])
            .collect();
        let band = Band {
            path: [rotated.clone(), center, rotated]
                .into_iter()
                .map(|positions| Minimum {
                    positions,
                    energy: 0.0,
                })
                .collect(),
            converged: false,
            climbing: 1,
            tangent: vec![0.0; 9],
        };
        assert!(
            band.intermediate_peaks(&mut MustNotEvaluate, &[1.0, 12.0, 35.45], 1.0)
                .unwrap()
                .seeds
                .is_empty()
        );
    }

    #[test]
    fn shape_links_exclude_rigid_motion_with_unequal_mass_reference_fit() {
        let first = vec![-0.55, 0.0, 0.0, 0.55, 0.0, 0.0, 0.0, 2.2, 0.0];
        let second = vec![-1.15, 0.0, 0.0, 1.15, 0.0, 0.0, 0.0, 1.99, 0.0];
        let second = mapping::superpose(&first, &second, &[1.0, 2.0, 4.0]);
        let band = vec![first, second];
        let displacement = link(&band, 0, 1, &[1.0; 3]);
        let raw = subtract(&band[1], &band[0]);
        let net = |v: &[f64]| -> [f64; 3] {
            std::array::from_fn(|k| v.as_chunks::<3>().0.iter().map(|atom| atom[k]).sum())
        };
        assert!(norm(&net(&raw)) > 0.1);
        assert!(norm(&net(&displacement)) < 1e-12);
        let mut torque = [0.0; 3];
        for (r, d) in band[0]
            .as_chunks::<3>()
            .0
            .iter()
            .zip(displacement.as_chunks::<3>().0)
        {
            torque[0] += r[1] * d[2] - r[2] * d[1];
            torque[1] += r[2] * d[0] - r[0] * d[2];
            torque[2] += r[0] * d[1] - r[1] * d[0];
        }
        assert!(norm(&torque) < 1e-12);
    }
}
