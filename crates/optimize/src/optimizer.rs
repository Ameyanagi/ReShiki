//! Molecular geometry minimization with ANC-RFO and Cartesian L-BFGS.

mod anc;
mod coordinates;
mod lbfgs;

use std::ops::ControlFlow;

use crate::{Accuracy, EvalError, Potential, linalg::dot};
use anc::Anc;
use coordinates::{Coordinates, metrics, project};
use lbfgs::Lbfgs;

/// ANC-RFO uses a Lindh model Hessian and falls back to Cartesian L-BFGS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Method {
    #[default]
    AncRfo,
    Lbfgs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Level {
    #[default]
    Normal,
    Tight,
}

/// Convergence thresholds in hartree, hartree/bohr and bohr.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Criteria {
    pub energy: f64,
    pub max_gradient: f64,
    pub rms_gradient: f64,
    pub max_step: f64,
    pub rms_step: f64,
}

impl Level {
    /// Returns energy, maximum/RMS gradient and maximum/RMS step tolerances.
    ///
    /// | Level | Absolute ΔE (Eh) | Max gradient (Eh/bohr) | RMS gradient (Eh/bohr) | Max step (bohr) | RMS step (bohr) |
    /// |-------|-----------|------------------------|------------------------|-----------------|-----------------|
    /// | Normal | 5e-6 | 4.5e-4 | 3e-4 | 1.8e-3 | 1.2e-3 |
    /// | Tight | 1e-6 | 1.5e-5 | 1e-5 | 6e-5 | 4e-5 |
    pub fn criteria(self) -> Criteria {
        match self {
            Self::Normal => Criteria {
                energy: 5e-6,
                max_gradient: 4.5e-4,
                rms_gradient: 3e-4,
                max_step: 1.8e-3,
                rms_step: 1.2e-3,
            },
            Self::Tight => Criteria {
                energy: 1e-6,
                max_gradient: 1.5e-5,
                rms_gradient: 1e-5,
                max_step: 6e-5,
                rms_step: 4e-5,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub method: Method,
    pub level: Level,
    pub max_steps: usize,
    /// Zero-based atom indices whose input coordinates are held exactly fixed.
    pub fixed_atoms: Vec<usize>,
    /// Recheck convergence with [`Accuracy::Tight`]. With `false`,
    /// [`Status::Converged`] reflects only Normal-accuracy evaluations.
    pub tight_final: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            method: Method::AncRfo,
            level: Level::Normal,
            max_steps: 500,
            fixed_atoms: Vec::new(),
            tight_final: true,
        }
    }
}

/// An accepted geometry, including a Tight re-evaluation at the same index.
/// Index zero has zero energy change and step lengths.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Step<'a> {
    pub index: usize,
    pub energy: f64,
    pub delta_energy: f64,
    pub max_gradient: f64,
    pub rms_gradient: f64,
    pub max_step: f64,
    pub rms_step: f64,
    /// ANC Euclidean trust radius used for this step, or the per-atom
    /// displacement limit after L-BFGS backtracking, in bohr.
    pub trust_radius: f64,
    pub accuracy: Accuracy,
    pub positions: &'a [f64],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Converged,
    MaxSteps,
    ScfFailure(String),
    Failed(String),
    Cancelled,
}

/// The last accepted geometry and the full, unprojected gradient evaluated there.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub status: Status,
    pub positions: Vec<f64>,
    pub energy: f64,
    pub gradient: Vec<f64>,
    pub steps: usize,
    pub evaluations: usize,
    pub method: Method,
    /// One energy per accepted geometry, including the input. A successful
    /// Tight re-evaluation replaces the entry at the same geometry.
    pub energies: Vec<f64>,
}

struct Point {
    positions: Vec<f64>,
    energy: f64,
    gradient: Vec<f64>,
    active_gradient: Vec<f64>,
    rigid: Vec<Vec<f64>>,
}

#[derive(Clone, Copy, Default)]
struct Motion {
    de: f64,
    max: f64,
    rms: f64,
}

impl Point {
    fn step(&self, index: usize, motion: Motion, radius: f64, accuracy: Accuracy) -> Step<'_> {
        let (max_gradient, rms_gradient) = metrics(&self.active_gradient);
        Step {
            index,
            energy: self.energy,
            delta_energy: motion.de,
            max_gradient,
            rms_gradient,
            max_step: motion.max,
            rms_step: motion.rms,
            trust_radius: radius,
            accuracy,
            positions: &self.positions,
        }
    }
}

impl Step<'_> {
    fn converged(&self, c: Criteria) -> bool {
        self.max_gradient <= c.max_gradient
            && self.rms_gradient <= c.rms_gradient
            && (self.max_gradient < c.max_gradient / 100.
                || (self.index > 0
                    && self.delta_energy.abs() <= c.energy
                    && self.max_step <= c.max_step
                    && self.rms_step <= c.rms_step))
    }
}

struct Evaluator<'a, P: ?Sized> {
    potential: &'a mut P,
    coords: Coordinates,
    accuracy: Accuracy,
    count: usize,
}

impl<P: Potential + ?Sized> Evaluator<'_, P> {
    fn evaluate(&mut self, positions: Vec<f64>) -> Result<Point, Status> {
        if !positions.iter().all(|x| x.is_finite()) {
            return Err(Status::Failed("non-finite trial position".into()));
        }
        let mut gradient = vec![f64::NAN; positions.len()];
        self.count += 1;
        let energy = self
            .potential
            .evaluate(&positions, &mut gradient, self.accuracy)
            .map_err(|e| match e {
                EvalError::Cancelled => Status::Cancelled,
                EvalError::Scf(m) => Status::ScfFailure(m),
                EvalError::Failed(m) => Status::Failed(m),
            })?;
        if !energy.is_finite() || !gradient.iter().all(|x| x.is_finite()) {
            return Err(Status::Failed(
                "non-finite potential energy or gradient".into(),
            ));
        }
        let mut active_gradient = self
            .coords
            .indices
            .iter()
            .map(|&i| gradient[i])
            .collect::<Vec<_>>();
        let rigid = self.coords.rigid(&positions);
        project(&mut active_gradient, &rigid);
        if !active_gradient.iter().all(|x| x.is_finite()) {
            return Err(Status::Failed("non-finite projected gradient".into()));
        }
        Ok(Point {
            positions,
            energy,
            gradient,
            active_gradient,
            rigid,
        })
    }

    fn trial(&mut self, point: &Point, step: &[f64]) -> Result<Point, Status> {
        let mut positions = point.positions.clone();
        for (&i, s) in self.coords.indices.iter().zip(step) {
            positions[i] += s;
        }
        self.evaluate(positions)
    }
}

struct Engine {
    method: Method,
    anc: Option<Anc>,
    lbfgs: Lbfgs,
    radius: f64,
}

impl Engine {
    fn new(method: Method) -> Self {
        Self {
            method,
            anc: None,
            lbfgs: Lbfgs::default(),
            radius: 0.3,
        }
    }

    fn search_anc<P: Potential + ?Sized>(
        &mut self,
        eval: &mut Evaluator<P>,
        numbers: &[u8],
        point: &Point,
        steps: usize,
    ) -> Result<Option<(Point, f64)>, Status> {
        if self.anc.as_ref().is_none_or(|a| steps - a.built_at >= 30) {
            self.anc = Anc::build(numbers, &eval.coords, point, steps, self.anc.as_ref()).ok();
        }
        let Some(direction) = self
            .anc
            .as_ref()
            .and_then(|a| a.direction(&point.active_gradient))
        else {
            return Ok(None);
        };
        for _ in 0..10 {
            let radius = self.radius;
            let Some((s, predicted, length)) = direction.restricted(radius) else {
                return Ok(None);
            };
            let trial = eval.trial(point, &s)?;
            let actual = trial.energy - point.energy;
            if actual > 1e-6 {
                self.radius = 0.25 * length;
                if self.radius < 1e-3 {
                    break;
                }
                continue;
            }
            let ratio = actual / predicted;
            if ratio < 0.25 {
                self.radius = (0.25 * radius).max(1e-3);
            } else if ratio > 0.75 && length == radius {
                self.radius = (2. * radius).min(0.5);
            }
            return Ok(Some((trial, radius)));
        }
        Ok(None)
    }

    fn search_lbfgs<P: Potential + ?Sized>(
        &mut self,
        eval: &mut Evaluator<P>,
        point: &Point,
    ) -> Result<(Point, f64), Status> {
        loop {
            let Some(mut d) = self.lbfgs.direction(&point.active_gradient, &point.rigid) else {
                return Err(Status::Failed("no finite L-BFGS descent direction".into()));
            };
            let slope = dot(&point.active_gradient, &d);
            let mut scale = 1.;
            for _ in 0..=10 {
                let trial = eval.trial(point, &d)?;
                if trial.energy <= point.energy + 1e-4 * scale * slope {
                    return Ok((trial, 0.3 * scale));
                }
                scale *= 0.5;
                d.iter_mut().for_each(|v| *v *= 0.5);
            }
            if !self.lbfgs.has_history() {
                return Err(Status::Failed("L-BFGS Armijo line search exhausted".into()));
            }
            self.lbfgs.clear();
        }
    }

    fn advance<P: Potential + ?Sized>(
        &mut self,
        eval: &mut Evaluator<P>,
        numbers: &[u8],
        point: &Point,
        steps: usize,
    ) -> Result<(Point, Motion, f64), Status> {
        let trial = if self.method == Method::AncRfo {
            self.search_anc(eval, numbers, point, steps)?
        } else {
            None
        };
        let (next, radius) = if let Some(trial) = trial {
            trial
        } else {
            self.method = Method::Lbfgs;
            self.anc = None;
            self.search_lbfgs(eval, point)?
        };
        // Use the displacement that survived floating-point addition at the trial.
        let s: Vec<_> = eval
            .coords
            .indices
            .iter()
            .map(|&i| next.positions[i] - point.positions[i])
            .collect();
        let y: Vec<_> = next
            .active_gradient
            .iter()
            .zip(&point.active_gradient)
            .map(|(n, o)| n - o)
            .collect();
        let (max, rms) = metrics(&s);
        let motion = Motion {
            de: next.energy - point.energy,
            max,
            rms,
        };
        if let Some(anc) = &mut self.anc {
            anc.update(&s, &y);
        } else {
            self.lbfgs.update(s, y);
        }
        Ok((next, motion, radius))
    }
}

/// Minimize an energy in atomic units, reporting the input, every accepted step,
/// and each successful Tight re-evaluation at the same accepted-step index.
///
/// Atomic numbers must be in 1..=118. Positions must be finite, with three
/// components per atom. Fixed indices must be unique and in range, leaving at
/// least one free atom. Invalid inputs return `Failed` before any evaluation.
///
/// Maximum absolute component and RMS use only free Cartesian components.
/// RMS is their Euclidean norm divided by the square root of their count.
/// With no anchors, the gradient is projected perpendicular to the
/// instantaneous translations and rotations.
/// Both gradient thresholds must pass (≤). In addition, the last accepted
/// |energy change|, maximum step and RMS step must all pass (≤), or the maximum
/// gradient must be strictly below 1/100 of its threshold. Only this last rule
/// can converge at the initial point.
///
/// With `tight_final`, first convergence triggers a Tight evaluation at the same
/// geometry. Its gradient is tested with the preceding accepted displacement and
/// energy change. The callback receives the Tight sample before convergence is
/// tested and may cancel. If minimization continues, all trials use Tight.
/// ANC keeps its basis, curvature and trust radius. L-BFGS clears its history
/// at the accuracy change.
///
/// ANC starts from the projected Lindh curvature, flooring internal eigenvalues
/// at 0.001 Eh/bohr². Davidson iteration finds the lowest augmented-Hessian
/// eigenvector, with a dense solve on nonconvergence. ANC scales the step
/// to a Euclidean trust radius, initially 0.3 bohr. The rational prediction is
/// (g·s + s·Hs/2)/(1 + s·s). Actual/predicted below 0.25 quarters the radius
/// (minimum 0.001 bohr); above 0.75 on the boundary doubles it (maximum 0.5).
/// An energy rise above 1e-6 Eh rejects the trial and sets the radius to one
/// quarter of the attempted step length.
/// Ten successive rejections, a rejected trial shrinking below the minimum, or
/// an unusable ANC model switches to L-BFGS. Accepted BFGS updates require
/// y·s > 1e-12 |y||s| and positive model curvature. Every 30 accepted steps
/// the Lindh basis is rebuilt. Learned curvature is transformed into the new
/// basis, using the 0.001 floor for components outside the old internal space.
/// A change of internal dimension starts from the fresh Lindh curvature.
///
/// L-BFGS retains 20 pairs, uses initial inverse curvature 2 bohr²/Eh and then
/// (s·y)/(y·y), and limits the largest atom displacement to 0.3 bohr. Armijo
/// backtracking (c1 = 1e-4) tries the capped step and at most ten halvings.
/// An exhausted search with history retries once with the initial scaling.
/// Nonpositive curvature or a non-descent direction clears the history.
///
/// A callback break, failed trial, or failed accuracy change returns the last
/// accepted point. All potential calls count as evaluations. Only accepted
/// displacements count toward `max_steps`.
pub fn optimize<P: Potential + ?Sized>(
    potential: &mut P,
    numbers: &[u8],
    positions: &[f64],
    options: &Options,
    mut on_step: impl FnMut(&Step) -> ControlFlow<()>,
) -> Outcome {
    let mut out = Outcome {
        status: Status::MaxSteps,
        positions: positions.to_vec(),
        energy: f64::NAN,
        gradient: vec![f64::NAN; positions.len()],
        steps: 0,
        evaluations: 0,
        method: options.method,
        energies: Vec::new(),
    };
    let coords = match Coordinates::new(numbers, positions, &options.fixed_atoms) {
        Ok(c) => c,
        Err(m) => {
            out.status = Status::Failed(m);
            return out;
        }
    };
    let mut eval = Evaluator {
        potential,
        coords,
        accuracy: Accuracy::Normal,
        count: 0,
    };
    let mut point = match eval.evaluate(positions.to_vec()) {
        Ok(p) => p,
        Err(status) => {
            out.status = status;
            out.evaluations = eval.count;
            return out;
        }
    };
    let mut engine = Engine::new(options.method);
    let criteria = options.level.criteria();
    let mut motion = Motion::default();
    let mut radius = 0.3;
    out.energies.push(point.energy);
    out.status = loop {
        let info = point.step(out.steps, motion, radius, eval.accuracy);
        if on_step(&info).is_break() {
            break Status::Cancelled;
        }
        if info.converged(criteria) {
            if !options.tight_final || eval.accuracy == Accuracy::Tight {
                break Status::Converged;
            }
            eval.accuracy = Accuracy::Tight;
            point = match eval.evaluate(point.positions.clone()) {
                Ok(p) => p,
                Err(status) => break status,
            };
            out.energies[out.steps] = point.energy;
            engine.lbfgs.clear();
            continue;
        }
        if out.steps == options.max_steps {
            break Status::MaxSteps;
        }
        match engine.advance(&mut eval, numbers, &point, out.steps) {
            Ok(accepted) => (point, motion, radius) = accepted,
            Err(status) => break status,
        }
        out.steps += 1;
        out.energies.push(point.energy);
    };
    out.positions = point.positions;
    out.energy = point.energy;
    out.gradient = point.gradient;
    out.evaluations = eval.count;
    out.method = engine.method;
    out
}

#[cfg(test)]
mod potential_tests;

#[cfg(test)]
mod tests;
