//! Mass-weighted steepest descent on a local quadratic surface
//! (Page & McIver, J. Chem. Phys. 88, 922, 1988). With dq/dt = −g − Hq,
//! the displacement along Hessian mode i is gᵢ (exp(−λᵢt) − 1)/λᵢ.
//! Choose t by the mass-weighted chord length. Keep the final, shorter step
//! when the quadratic minimum is within that distance.

use super::{
    Event, IrcBranch, Minimum, Potential, Reached, Search, TsError, TsOptions, bonds, emit,
    evaluate, mapping, minimize, modes, saddle,
};
use crate::linalg::{dot, norm, symmetric_eigen};
use std::ops::ControlFlow;

/// Stop at a mass-weighted force norm of 10⁻⁴ Eh/(bohr √amu).
const GRADIENT_TOLERANCE: f64 = 1e-4;

fn quadratic_step(values: &[f64], vectors: &[f64], gradient: &[f64], length: f64) -> Vec<f64> {
    let n = gradient.len();
    let slopes: Vec<_> = vectors.chunks_exact(n).map(|u| dot(gradient, u)).collect();
    let at_time = |t: f64| -> Vec<f64> {
        values
            .iter()
            .zip(&slopes)
            .map(|(&lambda, &g)| {
                if g == 0. {
                    0.
                } else if lambda == 0. {
                    -g * t
                } else {
                    g * ((-lambda * t).exp_m1() / lambda)
                }
            })
            .collect()
    };
    // Each modal displacement grows monotonically in magnitude. A finite
    // asymptote exists only when every excited direction curves upward.
    let to_minimum: Option<Vec<_>> = values
        .iter()
        .zip(&slopes)
        .map(|(&lambda, &g)| {
            if g == 0. {
                Some(0.)
            } else if lambda > 0. {
                Some(-g / lambda)
            } else {
                None
            }
        })
        .collect();
    let coefficients = if let Some(delta) = to_minimum.filter(|s| norm(s) <= length) {
        delta
    } else {
        let mut lo = 0.;
        let mut hi = (length / norm(&slopes)).clamp(f64::MIN_POSITIVE, f64::MAX);
        while hi < f64::MAX && norm(&at_time(hi)) < length {
            lo = hi;
            hi = (hi * 2.).min(f64::MAX);
        }
        for _ in 0..50 {
            let mid = lo + (hi - lo) * 0.5;
            if norm(&at_time(mid)) < length {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        at_time(hi)
    };
    let mut step = vec![0.; n];
    for (&c, u) in coefficients.iter().zip(vectors.chunks_exact(n)) {
        for (s, &component) in step.iter_mut().zip(u) {
            *s += c * component;
        }
    }
    step
}

pub(super) struct Descent<'a> {
    pub(super) ts: &'a Minimum,
    pub(super) mode: &'a [f64],
    pub(super) hessian: &'a [f64],
    /// Empty for a general Euclidean potential, otherwise one mass per atom.
    pub(super) masses: &'a [f64],
    pub(super) options: &'a TsOptions,
}

impl Descent<'_> {
    pub(super) fn trace(
        &self,
        p: &mut impl Potential,
        forward: bool,
        on_event: &mut impl FnMut(Event) -> ControlFlow<()>,
    ) -> Result<(Minimum, Vec<Minimum>), TsError> {
        let n = self.ts.positions.len();
        let scale: Vec<f64> = if self.masses.is_empty() {
            vec![1.; n]
        } else {
            self.masses.iter().flat_map(|m| [m.sqrt(); 3]).collect()
        };
        let mut point = self.ts.clone();
        let mut path = Vec::new();
        emit(
            on_event,
            Event::Irc {
                forward,
                step: 0,
                energy: point.energy,
            },
        )?;
        if self.options.irc_max_steps == 0 {
            return Ok((point, path));
        }
        let weight_gradient =
            |g: Vec<f64>| -> Vec<f64> { g.iter().zip(&scale).map(|(v, w)| v / w).collect() };
        let mut gradient = weight_gradient(evaluate(p, &point.positions)?.1);
        let mut h: Vec<_> = self
            .hessian
            .iter()
            .enumerate()
            .map(|(i, v)| v / (scale[i / n] * scale[i % n]))
            .collect();

        for step in 1..=self.options.irc_max_steps {
            let mut displacement = if step == 1 {
                let mut direction: Vec<_> =
                    self.mode.iter().zip(&scale).map(|(v, w)| v * w).collect();
                let factor =
                    self.options.irc_step / norm(&direction) * if forward { 1. } else { -1. };
                for d in &mut direction {
                    *d *= factor;
                }
                direction
            } else {
                if norm(&gradient) <= GRADIENT_TOLERANCE {
                    break;
                }
                let (values, vectors) = if self.masses.is_empty() {
                    symmetric_eigen(h.clone(), n).map_err(TsError::Failed)?
                } else {
                    modes::spectrum(&point.positions, &h, self.masses)?
                };
                quadratic_step(&values, &vectors, &gradient, self.options.irc_step)
            };
            let mut accepted = None;
            // A long initial displacement can overshoot a narrow well. Halve
            // it along the same mode until it actually descends. On subsequent
            // uphill proposals, hand the last downhill point to the minimizer.
            for _ in 0..if step == 1 { 16 } else { 1 } {
                let x: Vec<_> = point
                    .positions
                    .iter()
                    .zip(&displacement)
                    .zip(&scale)
                    .map(|((&x, &dq), &w)| x + dq / w)
                    .collect();
                let (energy, g) = evaluate(p, &x)?;
                if energy < point.energy {
                    accepted = Some((
                        Minimum {
                            positions: x,
                            energy,
                        },
                        weight_gradient(g),
                    ));
                    break;
                }
                for d in &mut displacement {
                    *d *= 0.5;
                }
            }
            let Some((next, next_gradient)) = accepted else {
                if step == 1 {
                    return Err(TsError::Failed(
                        "IRC could not leave the saddle downhill along the transition mode".into(),
                    ));
                }
                break;
            };
            let change: Vec<_> = next_gradient
                .iter()
                .zip(&gradient)
                .map(|(a, b)| a - b)
                .collect();
            saddle::bofill(&mut h, &displacement, &change);
            point = next;
            gradient = next_gradient;
            path.push(point.clone());
            emit(
                on_event,
                Event::Irc {
                    forward,
                    step,
                    energy: point.energy,
                },
            )?;
        }
        Ok((point, path))
    }
}

/// Compare labeled bond sets. When both endpoints have the same set, keep
/// the atom map fixed and choose the smaller superposed mass-weighted RMSD.
fn classify(
    end: &[f64],
    numbers: &[u8],
    masses: &[f64],
    endpoints: [&Minimum; 2],
    graphs: &[Vec<(usize, usize)>; 2],
) -> Reached {
    let graph = bonds(numbers, end);
    match (graph == graphs[0], graph == graphs[1]) {
        (true, true) => {
            let loss = endpoints.map(|reference| {
                let aligned = mapping::superpose(&reference.positions, end, masses);
                aligned
                    .iter()
                    .zip(&reference.positions)
                    .enumerate()
                    .map(|(i, (a, b))| masses[i / 3] * (a - b).powi(2))
                    .sum::<f64>()
            });
            if loss[0] <= loss[1] {
                Reached::Reactant
            } else {
                Reached::Product
            }
        }
        (true, false) => Reached::Reactant,
        (false, true) => Reached::Product,
        (false, false) => Reached::Other,
    }
}

pub(super) fn branches(
    p: &mut impl Potential,
    search: &Search,
    endpoints: [&Minimum; 2],
    ts: &Minimum,
    hessian: &[f64],
    mode: &[f64],
    on_event: &mut impl FnMut(Event) -> ControlFlow<()>,
) -> Result<[IrcBranch; 2], TsError> {
    let descent = Descent {
        ts,
        mode,
        hessian,
        masses: search.masses_amu,
        options: search.options,
    };
    let graphs = endpoints.map(|m| bonds(search.numbers, &m.positions));
    let mut branch = |forward| -> Result<IrcBranch, TsError> {
        let (last, path) = descent.trace(p, forward, on_event)?;
        let mut end = minimize(p, search.numbers, &last.positions, |i, energy| {
            if i == 0 {
                ControlFlow::Continue(())
            } else {
                on_event(Event::Irc {
                    forward,
                    step: path.len() + i,
                    energy,
                })
            }
        })?;
        end.positions =
            mapping::superpose(&endpoints[0].positions, &end.positions, search.masses_amu);
        let energies = std::iter::once(ts.energy)
            .chain(path.iter().map(|point| point.energy))
            .chain(std::iter::once(end.energy))
            .collect();
        let reached = classify(
            &end.positions,
            search.numbers,
            search.masses_amu,
            endpoints,
            &graphs,
        );
        Ok(IrcBranch {
            energies,
            path,
            end,
            reached,
        })
    };
    Ok([branch(true)?, branch(false)?])
}
