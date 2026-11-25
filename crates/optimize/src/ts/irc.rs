//! Mass-weighted steepest descent on a local quadratic surface
//! (Page & McIver, J. Chem. Phys. 88, 922, 1988). With dq/dt = −g − Hq,
//! the displacement along Hessian mode i is gᵢ (exp(−λᵢt) − 1)/λᵢ.
//! Choose t by the mass-weighted arc length along this curve. Keep the final,
//! shorter step when the remaining arc to a quadratic minimum fits in it.

use super::{
    Event, IrcBranch, Minimum, Potential, Reached, Search, TsError, TsOptions, emit,
    endpoint::bonds_prevalidated as bonds, evaluate, mapping, minimize, modes, saddle,
};
use crate::linalg::{dot, norm, symmetric_eigen};
use std::ops::ControlFlow;

/// Stop at a mass-weighted force norm of 10⁻⁴ Eh/(bohr √amu).
const GRADIENT_TOLERANCE: f64 = 1e-4;
/// A shared bond graph needs agreement with an endpoint in mapped geometry.
const ENDPOINT_RMSD_ANGSTROM: f64 = 0.5;

/// Adaptive Simpson quadrature of the smooth, nonnegative modal speeds.
fn integral(f: &impl Fn(f64) -> f64, a: f64, b: f64, tolerance: f64) -> f64 {
    fn refine(
        f: &impl Fn(f64) -> f64,
        interval: [f64; 2],
        samples: [f64; 3],
        whole: f64,
        tolerance: f64,
        depth: usize,
    ) -> f64 {
        let [a, b] = interval;
        let [fa, fm, fb] = samples;
        let mid = a + (b - a) * 0.5;
        let fl = f(a + (mid - a) * 0.5);
        let fr = f(mid + (b - mid) * 0.5);
        let left = (mid - a) * (fa + 4. * fl + fm) / 6.;
        let right = (b - mid) * (fm + 4. * fr + fb) / 6.;
        let sum = left + right;
        if !sum.is_finite() {
            return f64::INFINITY;
        }
        let correction = (sum - whole) / 15.;
        if depth == 0 || correction.abs() <= tolerance {
            sum + correction
        } else {
            refine(f, [a, mid], [fa, fl, fm], left, tolerance * 0.5, depth - 1)
                + refine(f, [mid, b], [fm, fr, fb], right, tolerance * 0.5, depth - 1)
        }
    }
    let samples = [f(a), f(a + (b - a) * 0.5), f(b)];
    let whole = (b - a) * (samples[0] + 4. * samples[1] + samples[2]) / 6.;
    if whole.is_finite() {
        refine(f, [a, b], samples, whole, tolerance, 24)
    } else {
        f64::INFINITY
    }
}

/// Follow the analytic quadratic descent in time intervals that at most double.
/// Resolving the fastest curvature first prevents a stiff mode's initial arc
/// from disappearing inside a much longer integration interval.
fn quadratic_proposal(
    values: &[f64],
    vectors: &[f64],
    gradient: &[f64],
    length: f64,
) -> (Vec<f64>, f64) {
    let n = gradient.len();
    let slopes: Vec<_> = vectors.chunks_exact(n).map(|u| dot(gradient, u)).collect();
    let slope_norm = norm(&slopes);
    if slope_norm == 0. {
        return (vec![0.; n], 0.);
    }
    let speed = |t: f64| {
        values
            .iter()
            .zip(&slopes)
            .fold(0.0_f64, |s, (&lambda, &g)| {
                let component = if g == 0. {
                    0.
                } else {
                    (g.abs().ln() - lambda * t).exp()
                };
                s.hypot(component)
            })
    };
    let at_time = |t: f64| {
        values
            .iter()
            .zip(&slopes)
            .map(|(&lambda, &g)| {
                if g == 0. {
                    0.
                } else if lambda == 0. {
                    -g * t
                } else {
                    g * (-lambda * t).exp_m1() / lambda
                }
            })
            .collect::<Vec<_>>()
    };
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
    let fastest = values
        .iter()
        .zip(&slopes)
        .filter(|(_, g)| **g != 0.)
        .map(|(lambda, _)| lambda.abs())
        .fold(0.0_f64, f64::max);
    let tolerance = length * 1e-11;
    let mut lo = 0.;
    let mut hi = (length / slope_norm)
        .min(fastest.recip())
        .clamp(f64::MIN_POSITIVE, f64::MAX);
    let mut accumulated = 0.;
    let (coefficients, actual_arc) = loop {
        let segment = integral(&speed, lo, hi, tolerance);
        if accumulated + segment >= length {
            let target = length - accumulated;
            let start = lo;
            for _ in 0..50 {
                let mid = lo + (hi - lo) * 0.5;
                if integral(&speed, start, mid, tolerance) < target {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            break (at_time(hi), length);
        }
        accumulated += segment;
        if let Some(minimum) = &to_minimum {
            // Integrating the norm is bounded above by the sum of the modal
            // remaining lengths, even for arbitrarily separated curvatures.
            let tail: f64 = values
                .iter()
                .zip(&slopes)
                .map(|(&lambda, &g)| {
                    if g == 0. {
                        0.
                    } else {
                        (g.abs().ln() - lambda.ln() - lambda * hi).exp()
                    }
                })
                .sum();
            if tail <= tolerance {
                break (minimum.clone(), accumulated + tail * 0.5);
            }
        }
        if hi == f64::MAX {
            // At the largest representable time, keep the integrated part of
            // the curve and its actual arc for any subsequent retry.
            break (at_time(hi), accumulated);
        }
        lo = hi;
        hi = (hi * 2.).min(f64::MAX);
    };
    let mut step = vec![0.; n];
    for (&c, u) in coefficients.iter().zip(vectors.chunks_exact(n)) {
        for (s, &component) in step.iter_mut().zip(u) {
            *s += c * component;
        }
    }
    (step, actual_arc)
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
    ) -> Result<(Minimum, Vec<Minimum>, bool), TsError> {
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
            return Ok((point, path, false));
        }
        let weight_gradient =
            |g: Vec<f64>| -> Vec<f64> { g.iter().zip(&scale).map(|(v, w)| v / w).collect() };
        let (energy, gradient) = evaluate(p, &point.positions)?;
        point.energy = energy;
        let mut gradient = weight_gradient(gradient);
        let inverse_sqrt: Vec<_> = scale.iter().map(|w| w.recip()).collect();
        let mut h = modes::mass_weight(self.hessian, &inverse_sqrt);
        let mut completed = false;

        for step in 1..=self.options.irc_max_steps {
            let spectrum = if step == 1 {
                None
            } else {
                Some(if self.masses.is_empty() {
                    symmetric_eigen(h.clone(), n).map_err(TsError::Failed)?
                } else {
                    modes::spectrum(&point.positions, &h, self.masses)?
                })
            };
            let mut accepted = None;
            let mut length = self.options.irc_step;
            // Check each proposal in four subintervals. Endpoint energy alone
            // can accept a jump across a minimum and another saddle. An uphill
            // segment or reversed directional gradient instead halves the
            // requested arc, with at most four halvings on EVERY step.
            for _ in 0..=4 {
                let displacement = if let Some((values, vectors)) = &spectrum {
                    let (displacement, actual_arc) =
                        quadratic_proposal(values, vectors, &gradient, length);
                    length = actual_arc;
                    displacement
                } else {
                    let mut direction: Vec<_> =
                        self.mode.iter().zip(&scale).map(|(v, w)| v * w).collect();
                    let factor = length / norm(&direction) * if forward { 1. } else { -1. };
                    direction.iter_mut().for_each(|d| *d *= factor);
                    direction
                };
                let mut previous_energy = point.energy;
                let mut previous_force = norm(&gradient);
                let mut trial = None;
                for substep in 1..=4 {
                    let fraction = f64::from(substep) / 4.;
                    let x: Vec<_> = point
                        .positions
                        .iter()
                        .zip(&displacement)
                        .zip(&scale)
                        .map(|((&x, &dq), &w)| x + fraction * dq / w)
                        .collect();
                    let (energy, g) = evaluate(p, &x)?;
                    let g = weight_gradient(g);
                    let force = norm(&g);
                    let roundoff = 16. * f64::EPSILON * point.energy.abs().max(1.);
                    if energy > previous_energy + roundoff
                        || dot(&g, &displacement) > GRADIENT_TOLERANCE * norm(&displacement)
                    {
                        trial = None;
                        break;
                    }
                    previous_energy = energy;
                    trial = Some((
                        Minimum {
                            positions: x,
                            energy,
                        },
                        g,
                        force <= previous_force,
                    ));
                    previous_force = force;
                }
                if let Some((next, next_gradient, approaching)) =
                    trial.filter(|(m, _, _)| m.energy < point.energy)
                {
                    accepted = Some((next, next_gradient, displacement, approaching));
                    break;
                }
                length *= 0.5;
            }
            let Some((next, next_gradient, displacement, approaching)) = accepted else {
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
            if approaching && norm(&gradient) <= GRADIENT_TOLERANCE {
                completed = true;
                break;
            }
        }
        Ok((point, path, completed))
    }
}

/// Unique labeled bond-graph agreement determines the endpoint, including
/// conformers and separated fragments. When the graphs are identical, keep
/// the atom map fixed, fit all atoms equally with a proper rotation, and accept
/// the closer endpoint only below 0.5 A RMSD. A third geometry is `Other`.
fn classify(
    end: &[f64],
    numbers: &[u8],
    endpoints: [&Minimum; 2],
    graphs: &[Vec<(usize, usize)>; 2],
) -> Reached {
    let graph = bonds(numbers, end);
    match (graph == graphs[0], graph == graphs[1]) {
        (true, true) => {
            let weights = vec![1.; numbers.len()];
            let loss = endpoints.map(|reference| {
                let aligned = mapping::superpose(&reference.positions, end, &weights);
                aligned
                    .iter()
                    .zip(&reference.positions)
                    .map(|(a, b)| (a - b).powi(2))
                    .sum::<f64>()
                    / numbers.len() as f64
            });
            let closest = loss[0].min(loss[1]);
            if loss[0] != loss[1]
                && closest.sqrt() * crate::ANGSTROM_PER_BOHR < ENDPOINT_RMSD_ANGSTROM
            {
                if closest == loss[0] {
                    Reached::Reactant
                } else {
                    Reached::Product
                }
            } else {
                Reached::Other
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
        let (last, path, completed) = descent.trace(p, forward, on_event)?;
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
        let reached = if path.is_empty() {
            // A failed or disabled launch supplies no branch evidence, even
            // if the subsequent unrestricted minimizer finds an endpoint.
            Reached::Other
        } else {
            classify(&end.positions, search.numbers, endpoints, &graphs)
        };
        Ok(IrcBranch {
            energies,
            path,
            end,
            reached,
            completed,
        })
    };
    Ok([branch(true)?, branch(false)?])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Accuracy, EvalError};

    fn proceed(_: Event) -> ControlFlow<()> {
        ControlFlow::Continue(())
    }

    fn pair(r: f64) -> Minimum {
        Minimum {
            positions: vec![-r / 2., 0., 0., r / 2., 0., 0.],
            energy: 0.,
        }
    }

    fn classify_pair(r: f64, ends: [f64; 2]) -> Reached {
        let endpoints = ends.map(pair);
        let graphs = endpoints.each_ref().map(|m| bonds(&[6, 6], &m.positions));
        classify(&pair(r).positions, &[6, 6], endpoints.each_ref(), &graphs)
    }

    #[test]
    fn exact_review_intermediate_is_inside_the_prescribed_rmsd_tolerance() {
        // The 2.0-bohr intermediate differs from the 2.3-bohr endpoint by
        // only 0.15 bohr all-atom RMSD, i.e. 0.0794 A after proper fitting.
        assert_eq!(classify_pair(2.0, [1.1, 2.3]), Reached::Product);
    }

    #[test]
    fn same_graph_third_geometry_outside_absolute_tolerance_is_other() {
        // All are unbonded pairs. The closest mapped endpoint still differs
        // by 2 bohr per atom (1.058 A), so relative closeness is insufficient.
        assert_eq!(classify_pair(10., [5., 6.]), Reached::Other);
    }

    #[test]
    fn same_graph_exact_rmsd_tie_is_other() {
        // Both endpoints share the C-C graph and lie 0.25 bohr per atom
        // from this fitted geometry, well inside the 0.5 A cutoff.
        assert_eq!(classify_pair(2., [1.5, 2.5]), Reached::Other);
    }

    #[test]
    fn unique_graph_wins_even_for_a_distant_separated_fragment_geometry() {
        // The unique graph match identifies the endpoint at either fragment separation.
        assert_eq!(classify_pair(20., [5., 1.1]), Reached::Reactant);
        assert_eq!(classify_pair(20., [1.1, 5.]), Reached::Product);
    }

    #[test]
    fn graph_matching_neither_endpoint_is_other() {
        assert_eq!(classify_pair(5., [1.1, 2.3]), Reached::Other);
    }

    #[test]
    fn shared_graph_fit_preserves_atom_map_and_uses_a_proper_rotation() {
        let reference = Minimum {
            positions: vec![0., 0., 0., 5., 0., 0., 0., 6., 0., 0., 0., 7.],
            energy: 0.,
        };
        let product = Minimum {
            positions: reference.positions.iter().map(|v| 3. * v).collect(),
            energy: 0.,
        };
        let numbers = [6; 4];
        let graphs = [&reference, &product].map(|m| bonds(&numbers, &m.positions));
        let transformed: Vec<_> = reference
            .positions
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[2] + 10., p[0] - 4., p[1] + 8.])
            .collect();
        assert_eq!(
            classify(&transformed, &numbers, [&reference, &product], &graphs),
            Reached::Reactant
        );
        let reflected: Vec<_> = reference
            .positions
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [-p[0], p[1], p[2]])
            .collect();
        assert_eq!(
            classify(&reflected, &numbers, [&reference, &product], &graphs),
            Reached::Other
        );
    }

    #[test]
    fn quadratic_time_is_set_by_page_mciver_arc_length() {
        let step = quadratic_proposal(&[100., 1.], &[1., 0., 0., 1.], &[10., 0.1], 0.15).0;
        // Independent numerical integration of hypot(10 exp(-100t),
        // 0.1 exp(-t)) gives t=0.79479957 for an arc of 0.15.
        assert!((step[0] + 0.1).abs() < 1e-9, "{step:?}");
        assert!((step[1] + 0.054_832_824_6).abs() < 1e-7, "{step:?}");
        assert!(norm(&step) < 0.15);
    }

    #[test]
    fn quadratic_minimum_uses_remaining_arc_instead_of_its_chord() {
        let args = (&[100., 1.][..], &[1., 0., 0., 1.][..], &[10., 0.1][..]);
        // The chord to the minimum is sqrt(0.02), but the remaining arc
        // is 0.195167..., so a 0.19 step must stop short of the minimum.
        let shorter = quadratic_proposal(args.0, args.1, args.2, 0.19).0;
        assert!(shorter[1] > -0.099, "{shorter:?}");
        let full = quadratic_proposal(args.0, args.1, args.2, 0.20).0;
        assert!((full[0] + 0.1).abs() < 1e-12 && (full[1] + 0.1).abs() < 1e-12);
    }

    #[test]
    fn stiff_quadratic_modes_keep_the_fast_initial_arc() {
        // The first mode contributes almost exactly 0.1 before the slow
        // mode changes appreciably. An arc of 0.15 therefore leaves about
        // 0.05 in the second mode, for either sign of its tiny curvature.
        for curvature in [1e-10, -1e-10] {
            let step =
                quadratic_proposal(&[1., curvature], &[1., 0., 0., 1.], &[0.1, 1e-11], 0.15).0;
            assert!((step[0] + 0.1).abs() < 1e-10, "{step:?}");
            assert!((step[1] + 0.05).abs() < 2e-8, "{step:?}");
        }
        let full = quadratic_proposal(&[1., 1e-10], &[1., 0., 0., 1.], &[0.1, 1e-11], 0.21).0;
        assert!((full[0] + 0.1).abs() < 1e-12 && (full[1] + 0.1).abs() < 1e-12);
    }

    struct Quartic;

    impl Potential for Quartic {
        fn evaluate(&mut self, x: &[f64], g: &mut [f64], _: Accuracy) -> Result<f64, EvalError> {
            let q = x[0] * x[0] - 1.;
            g[0] = x[0] * q;
            Ok(q * q / 4.)
        }
    }

    #[test]
    fn uphill_later_steps_retry_at_shorter_lengths() {
        let ts = Minimum {
            positions: vec![0.],
            energy: 0.25,
        };
        let options = TsOptions {
            irc_step: 0.8,
            irc_max_steps: 2,
            ..Default::default()
        };
        let result = Descent {
            ts: &ts,
            mode: &[1.],
            hessian: &[-1.],
            masses: &[],
            options: &options,
        }
        .trace(&mut Quartic, true, &mut proceed)
        .unwrap();
        assert_eq!(result.1.len(), 2, "{:?}", result.1);
        assert!((result.0.positions[0] - 1.).abs() < 1e-9, "{:?}", result.0);
    }

    struct TripleWell;

    impl Potential for TripleWell {
        fn evaluate(&mut self, x: &[f64], g: &mut [f64], _: Accuracy) -> Result<f64, EvalError> {
            let d: [f64; 3] = std::array::from_fn(|k| x[3 + k] - x[k]);
            let r = norm(&d);
            let f = (r - 1.1) * (r - 2.) * (r - 2.3);
            let first = (r - 2.) * (r - 2.3) + (r - 1.1) * (r - 2.3) + (r - 1.1) * (r - 2.);
            for k in 0..3 {
                g[k] = -2. * f * first * d[k] / r;
                g[3 + k] = -g[k];
            }
            Ok(f * f)
        }
    }

    #[test]
    fn exact_triple_well_launch_does_not_skip_the_middle_minimum() {
        let r = (10.8 - 4.68_f64.sqrt()) / 6.;
        let mut ts = pair(r);
        ts.energy = evaluate(&mut TripleWell, &ts.positions).unwrap().0;
        let hessian = saddle::hessian(&mut TripleWell, &ts.positions).unwrap();
        let direction = [
            -std::f64::consts::FRAC_1_SQRT_2,
            0.,
            0.,
            std::f64::consts::FRAC_1_SQRT_2,
            0.,
            0.,
        ];
        for max_steps in [1, 60] {
            let options = TsOptions {
                irc_step: 2.107_921,
                irc_max_steps: max_steps,
                ..Default::default()
            };
            let result = Descent {
                ts: &ts,
                mode: &direction,
                hessian: &hessian,
                masses: &[12., 12.],
                options: &options,
            }
            .trace(&mut TripleWell, true, &mut proceed)
            .unwrap();
            assert!(!result.1.is_empty());
            let first = &result.1[0].positions;
            let first_r = norm(&(0..3).map(|k| first[k + 3] - first[k]).collect::<Vec<_>>());
            assert!(first_r <= 2., "launch skipped the 2.0-bohr well: {first_r}");
            assert!(first_r > r);
            if max_steps > 1 {
                let end = &result.0.positions;
                let reached_r = norm(&(0..3).map(|k| end[k + 3] - end[k]).collect::<Vec<_>>());
                assert!((reached_r - 2.).abs() < 0.01, "wrong well: {reached_r}");
            }
        }
    }

    #[test]
    fn completion_requires_irc_gradient_convergence_before_the_step_limit() {
        let ts = Minimum {
            positions: vec![0.],
            energy: 0.25,
        };
        for (steps, expected) in [(0, false), (1, false), (2, true)] {
            let options = TsOptions {
                irc_step: 0.8,
                irc_max_steps: steps,
                ..Default::default()
            };
            let (_, _, completed) = Descent {
                ts: &ts,
                mode: &[1.],
                hessian: &[-1.],
                masses: &[],
                options: &options,
            }
            .trace(&mut Quartic, true, &mut |_| ControlFlow::Continue(()))
            .unwrap();
            assert_eq!(completed, expected, "step budget {steps}");
        }
    }

    struct SoftQuartic;

    impl Potential for SoftQuartic {
        fn evaluate(&mut self, x: &[f64], g: &mut [f64], _: Accuracy) -> Result<f64, EvalError> {
            let q = x[0] * x[0] - 1.;
            g[0] = 0.0008 * x[0] * q;
            Ok(0.0008 * q * q / 4.)
        }
    }

    struct ShiftedSoftSaddle;

    impl Potential for ShiftedSoftSaddle {
        fn evaluate(&mut self, x: &[f64], g: &mut [f64], _: Accuracy) -> Result<f64, EvalError> {
            let curvature = (51. / 5_140.487_14_f64).powi(2);
            g[0] = -curvature * x[0];
            Ok(1e-7 - 0.5 * curvature * x[0] * x[0])
        }
    }

    #[test]
    fn irc_launch_uses_fresh_ts_energy_with_its_gradient() {
        let ts = Minimum {
            positions: vec![0.],
            energy: 0.,
        };
        let options = TsOptions {
            irc_max_steps: 1,
            ..TsOptions::default()
        };
        let curvature = (51. / 5_140.487_14_f64).powi(2);
        for forward in [true, false] {
            let (end, path, _) = Descent {
                ts: &ts,
                mode: &[1.],
                hessian: &[-curvature],
                masses: &[],
                options: &options,
            }
            .trace(&mut ShiftedSoftSaddle, forward, &mut proceed)
            .unwrap();
            assert_eq!(
                path.len(),
                1,
                "fresh SCF baseline rejected a downhill launch"
            );
            assert!((end.positions[0].abs() - options.irc_step).abs() < 1e-14);
            assert!(end.energy < 1e-7);
        }
    }

    #[test]
    fn a_soft_launch_below_force_tolerance_must_keep_descending() {
        let ts = Minimum {
            positions: vec![0.],
            energy: 0.0002,
        };
        for steps in [1, 60] {
            let options = TsOptions {
                irc_step: 0.061_237,
                irc_max_steps: steps,
                ..Default::default()
            };
            let (end, _, completed) = Descent {
                ts: &ts,
                mode: &[1.],
                hessian: &[-0.0008],
                masses: &[],
                options: &options,
            }
            .trace(&mut SoftQuartic, true, &mut |_| ControlFlow::Continue(()))
            .unwrap();
            if steps == 1 {
                assert!(
                    evaluate(&mut SoftQuartic, &end.positions).unwrap().1[0].abs()
                        < GRADIENT_TOLERANCE
                );
                assert!(!completed, "force still grows away from the saddle");
            } else {
                assert!(completed);
                assert!(
                    3. * end.positions[0].powi(2) - 1. > 0.,
                    "not in the minimum neighborhood: {end:?}"
                );
            }
        }
    }

    struct UphillLaunch {
        evaluations: usize,
    }

    impl Potential for UphillLaunch {
        fn evaluate(&mut self, x: &[f64], g: &mut [f64], _: Accuracy) -> Result<f64, EvalError> {
            self.evaluations += 1;
            g[0] = x[0];
            Ok(x[0] * x[0] / 2.)
        }
    }

    #[test]
    fn exhausted_launch_reports_incomplete_after_exactly_four_halvings() {
        let ts = Minimum {
            positions: vec![0.],
            energy: 0.,
        };
        let options = TsOptions::default();
        let mut potential = UphillLaunch { evaluations: 0 };
        let (_, path, completed) = Descent {
            ts: &ts,
            mode: &[1.],
            hessian: &[1.],
            masses: &[],
            options: &options,
        }
        .trace(&mut potential, true, &mut |_| ControlFlow::Continue(()))
        .unwrap();
        assert!(path.is_empty());
        assert!(!completed);
        // One initial gradient and one rejected first subpoint on each of
        // the full-size, half, quarter, eighth and sixteenth proposals.
        assert_eq!(potential.evaluations, 6);
    }

    struct RadialBowl;

    impl Potential for RadialBowl {
        fn evaluate(&mut self, x: &[f64], g: &mut [f64], _: Accuracy) -> Result<f64, EvalError> {
            let d: [f64; 3] = std::array::from_fn(|k| x[3 + k] - x[k]);
            let r = norm(&d);
            let delta = r - 1.1;
            for k in 0..3 {
                g[k] = -delta * d[k] / r;
                g[3 + k] = -g[k];
            }
            Ok(delta * delta / 2.)
        }
    }

    #[test]
    fn minimizer_handoff_cannot_classify_a_branch_that_never_launched() {
        let reactant = Minimum {
            positions: vec![-0.55, 0., 0., 0.55, 0., 0.],
            energy: 0.,
        };
        let product = Minimum {
            positions: vec![-2.5, 0., 0., 2.5, 0., 0.],
            energy: 0.,
        };
        let hessian = saddle::hessian(&mut RadialBowl, &reactant.positions).unwrap();
        let direction = [
            -std::f64::consts::FRAC_1_SQRT_2,
            0.,
            0.,
            std::f64::consts::FRAC_1_SQRT_2,
            0.,
            0.,
        ];
        for steps in [0, 1] {
            let options = TsOptions {
                irc_max_steps: steps,
                ..Default::default()
            };
            let search = Search {
                numbers: &[6, 6],
                masses_amu: &[12., 12.],
                reactant: &reactant.positions,
                product: &product.positions,
                options: &options,
            };
            let branches = branches(
                &mut RadialBowl,
                &search,
                [&reactant, &product],
                &reactant,
                &hessian,
                &direction,
                &mut |_| ControlFlow::Continue(()),
            )
            .unwrap();
            for branch in branches {
                assert!(branch.path.is_empty());
                assert!(!branch.completed);
                assert_eq!(branch.reached, Reached::Other);
            }
        }
    }
}
