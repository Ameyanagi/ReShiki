//! Image-dependent pair-potential interpolation: Smidstrup, Pedersen, Stokbro & Jónsson, J. Chem.
//! Phys. 140, 214106 (2014) \[Smidstrup2014IDPP\], Eqs. (2)–(4). Every atom pair has a linearly
//! interpolated target distance and weight w(d)=d^-4. A cheap NEB/FIRE relaxation acts on this
//! analytic objective without Potential calls.

use std::ops::ControlFlow;

use crate::linalg::norm;

use super::{TsError, neb};

pub(super) fn interpolate(
    reactant: &[f64],
    product: &[f64],
    masses: &[f64],
    images: usize,
    on_cancel: &mut impl FnMut() -> ControlFlow<()>,
) -> Result<Vec<Vec<f64>>, TsError> {
    let mut band: Vec<Vec<f64>> = (0..images)
        .map(|i| {
            let fraction = i as f64 / (images - 1) as f64;
            reactant
                .iter()
                .zip(product)
                .map(|(a, b)| (1.0 - fraction) * a + fraction * b)
                .collect()
        })
        .collect();
    // Preserve endpoints exactly, including signed zeros and the final rounding
    // bit, rather than reconstructing them through interpolation arithmetic.
    band[0] = reactant.to_vec();
    band[images - 1] = product.to_vec();
    if reactant.len() != 3 * masses.len() || masses.len() < 2 {
        return Ok(band);
    }
    let mut pairs = Vec::new();
    for a in 0..masses.len() {
        for b in a + 1..masses.len() {
            pairs.push(Pair {
                a,
                b,
                start: distance(reactant, a, b),
                end: distance(product, a, b),
            });
        }
    }
    bend_crossings(&mut band, &pairs, masses);
    let mut fire = neb::Fire::new(images, reactant.len());
    let mut energies = vec![0.0; images];
    let mut gradients = vec![vec![0.0; reactant.len()]; images];
    let weights = vec![1.0; masses.len()];
    for iteration in 0usize..600 {
        if iteration.is_multiple_of(20) && on_cancel().is_break() {
            return Err(TsError::Cancelled);
        }
        for i in 1..images - 1 {
            let fraction = i as f64 / (images - 1) as f64;
            let (energy, gradient) = objective(&band[i], fraction, &pairs);
            energies[i] = energy;
            gradients[i] = gradient;
        }
        let forces = neb::forces(&band, &energies, &gradients, 0.1, None, &weights);
        if forces.maximum <= 1e-3 {
            break;
        }
        fire.advance(&mut band, &forces.values, masses, 0.1)?;
    }
    Ok(band)
}

struct Pair {
    a: usize,
    b: usize,
    start: f64,
    end: f64,
}

fn distance(x: &[f64], a: usize, b: usize) -> f64 {
    norm(&[
        x[3 * a] - x[3 * b],
        x[3 * a + 1] - x[3 * b + 1],
        x[3 * a + 2] - x[3 * b + 2],
    ])
}

fn objective(x: &[f64], fraction: f64, pairs: &[Pair]) -> (f64, Vec<f64>) {
    let mut energy = 0.0;
    let mut gradient = vec![0.0; x.len()];
    for pair in pairs {
        let target = (1.0 - fraction) * pair.start + fraction * pair.end;
        let delta: [f64; 3] = std::array::from_fn(|k| x[3 * pair.a + k] - x[3 * pair.b + k]);
        // Smooth regularization d = sqrt(r² + (1e-4 bohr)²) keeps the energy
        // and its derivative consistent even below the near-overlap scale.
        let distance = norm(&delta).hypot(1e-4);
        let difference = distance - target;
        let weight = distance.powi(-4);
        energy += difference * difference * weight;
        let radial =
            (2.0 * difference - 4.0 * difference * difference / distance) * weight / distance;
        for (k, &value) in delta.iter().enumerate() {
            let derivative = radial * value;
            gradient[3 * pair.a + k] += derivative;
            gradient[3 * pair.b + k] -= derivative;
        }
    }
    (energy, gradient)
}

/// Exact linear crossings have no transverse gradient. Give only those pairs
/// whose continuous linear paths approach within 25% of their endpoint distance
/// a deterministic, smooth transverse bend. Opposite mass-scaled displacements
/// preserve the center of mass, and sin(pi*t) keeps both endpoints fixed.
fn bend_crossings(band: &mut [Vec<f64>], pairs: &[Pair], masses: &[f64]) {
    let first = band[0].clone();
    let last = band[band.len() - 1].clone();
    let intervals = band.len() - 1;
    for pair in pairs {
        let from: [f64; 3] = std::array::from_fn(|k| first[3 * pair.a + k] - first[3 * pair.b + k]);
        let change: [f64; 3] =
            std::array::from_fn(|k| last[3 * pair.a + k] - last[3 * pair.b + k] - from[k]);
        let square = change.iter().map(|value| value * value).sum::<f64>();
        if square <= 1e-20 {
            continue;
        }
        let closest = -from.iter().zip(change).map(|(a, b)| a * b).sum::<f64>() / square;
        if !(0.0..1.0).contains(&closest) {
            continue;
        }
        let residual: [f64; 3] = std::array::from_fn(|k| from[k] + closest * change[k]);
        let reference = pair.start.min(pair.end);
        if norm(&residual) >= 0.25 * reference {
            continue;
        }
        let axis = (0..3)
            .min_by(|&a, &b| change[a].abs().total_cmp(&change[b].abs()))
            .unwrap();
        let mut transverse: [f64; 3] =
            std::array::from_fn(|k| f64::from(k == axis) - change[k] * change[axis] / square);
        let length = norm(&transverse);
        for value in &mut transverse {
            *value /= length;
        }
        let sum = masses[pair.a] + masses[pair.b];
        let amplitude = 0.15 * reference;
        for (i, image) in band.iter_mut().enumerate().take(intervals).skip(1) {
            let bend = amplitude * (std::f64::consts::PI * i as f64 / intervals as f64).sin();
            for (k, &direction) in transverse.iter().enumerate() {
                image[3 * pair.a + k] += bend * direction * masses[pair.b] / sum;
                image[3 * pair.b + k] -= bend * direction * masses[pair.a] / sum;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analytic_pair_gradient_includes_distance_weight_derivative() {
        let x = [0.2, -0.4, 0.7, 1.3, 0.8, -0.2];
        let pairs = [Pair {
            a: 0,
            b: 1,
            start: 1.0,
            end: 2.4,
        }];
        let (_, gradient) = objective(&x, 0.3, &pairs);
        for k in 0..x.len() {
            let mut plus = x;
            let mut minus = x;
            plus[k] += 1e-5;
            minus[k] -= 1e-5;
            let numerical =
                (objective(&plus, 0.3, &pairs).0 - objective(&minus, 0.3, &pairs).0) / 2e-5;
            assert!((gradient[k] - numerical).abs() < 1e-10);
        }
    }

    #[test]
    fn near_overlap_energy_and_gradient_use_the_same_regularization() {
        let x = [0.0, 0.0, 0.0, 1e-5, -2e-5, 3e-5];
        let pairs = [Pair {
            a: 0,
            b: 1,
            start: 1.0,
            end: 2.4,
        }];
        let (energy, gradient) = objective(&x, 0.3, &pairs);
        assert!(energy.is_finite());
        for k in 0..x.len() {
            let mut plus = x;
            let mut minus = x;
            plus[k] += 1e-9;
            minus[k] -= 1e-9;
            let numerical =
                (objective(&plus, 0.3, &pairs).0 - objective(&minus, 0.3, &pairs).0) / 2e-9;
            assert!((gradient[k] - numerical).abs() < 1e-7 * gradient[k].abs());
        }
    }

    #[test]
    fn crossing_pairs_are_separated_and_endpoints_stay_fixed() {
        let start = [-1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 3.0];
        let end = [1.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 3.0];
        let band = interpolate(
            &start,
            &end,
            &[1.0; 4],
            5,
            &mut || ControlFlow::Continue(()),
        )
        .unwrap();
        assert_eq!(band[0], start);
        assert_eq!(band[4], end);
        for image in &band {
            assert!(image.iter().all(|value| value.is_finite()));
            for a in 0..4 {
                for b in a + 1..4 {
                    assert!(distance(image, a, b) > 0.25);
                }
            }
        }
    }

    #[test]
    fn callback_cancels_during_idpp_iterations() {
        let start = [-1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 3.0];
        let end = [1.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 3.0];
        let mut polls = 0;
        let result = interpolate(&start, &end, &[1.0; 4], 5, &mut || {
            polls += 1;
            // Allow the initial poll and cancel only after relaxation has run.
            if polls == 2 {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        });
        assert_eq!(polls, 2);
        assert_eq!(result.unwrap_err(), TsError::Cancelled);
    }
}
