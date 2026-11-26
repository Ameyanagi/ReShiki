//! Molecular geometry minimization with TRIC or ANC RFO and Cartesian L-BFGS.
//!
//! Full references: docs/references/xtb.md.

mod anc;
mod coordinates;
mod internal;
mod lbfgs;
mod rfo;
mod trust;

use std::{cell::LazyCell, ops::ControlFlow};

use crate::{
    Accuracy, EvalError, Potential,
    linalg::{dot, norm},
};
use anc::{Anc, History};
use coordinates::{Coordinates, metrics, project};
use internal::{Curvature as InternalCurvature, InternalModel};
use lbfgs::Lbfgs;
use trust::{DirectionError, Search, TrustRegion};
pub use trust::{Rebuild, Trust};

/// Coordinate model and search direction for minimization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Method {
    /// Translation–rotation–internal coordinates (TRIC), delocalized for RFO
    /// (Wang & Song 2016, Baker et al. 1996, Banerjee et al. 1985)
    /// \[Wang2016TRIC\] \[Baker1996DLC\] \[Banerjee1985PRFO\].
    /// Falls back to ANC if the local primitive count exceeds 128 times the
    /// atom count, before allocating dense Wilson rows.
    #[default]
    Internal,
    /// RFO in approximate normal coordinates from a Cartesian Hessian
    /// (Grimme et al. 2017, Banerjee et al. 1985) \[Grimme2017ANCopt\] \[Banerjee1985PRFO\].
    AncRfo,
    /// Limited-memory BFGS in free Cartesian coordinates with Armijo
    /// backtracking (Liu & Nocedal 1989, Armijo 1966) \[Liu1989LBFGS\] \[Armijo1966LineSearch\].
    Lbfgs,
}

/// Starting curvature for RFO. Rebuilds use the new coordinate basis and carry
/// the learned curvature (Lindh et al., Chem. Phys. Lett. 241, 423, 1995) \[Lindh1995Hessian\].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InitialHessian {
    /// Construct the Lindh model at the input geometry.
    #[default]
    Model,
    /// Request the potential's Hessian when the first step is needed. An unavailable,
    /// failed or non-finite Hessian falls back to Lindh. Cancellation stops the run.
    /// ANC and Internal restrict to free Cartesians and use max(|λ|,
    /// curvature_floor) on physical modes to start with positive curvature.
    /// With all atoms free, the projection removes global translation and rotation
    /// and retains relative fragment motion. Internal assigns
    /// curvature_floor to those global rigid directions before transforming to
    /// DLC, without the gradient-weighted coordinate second derivatives (Peng
    /// et al., J. Comput. Chem. 17, 49, 1996) \[Peng1996Redundant\].
    Potential,
    /// Use `Potential` up to and including `max_atoms`, counting fixed atoms too.
    Auto { max_atoms: usize },
}

/// Secant update of the RFO Hessian after an accepted displacement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Update {
    /// Direct BFGS with a positive-curvature test (Nocedal & Wright, §6.1) \[Nocedal2006Numerical\].
    #[default]
    Bfgs,
    /// Powell's damping enforces sᵀr ≥ 0.2 sᵀHs (Nocedal & Wright, 2nd ed., Procedure 18.2)
    /// \[Powell1978Damping\] \[Nocedal2006Numerical\].
    DampedBfgs,
    /// Blend SR1 and BFGS for minimization (Farkas & Schlegel, JCP 111, 10806, 1999)
    /// \[Farkas1999Update\]. Not Bofill's SR1/PSB update, which only the
    /// transition-state search uses.
    /// The model may become indefinite, in which case RFO uses the lowest augmented root.
    FarkasSchlegel,
}

/// Extrapolate from up to five accepted points in a single model frame so errors,
/// gradients and the trust radius share the same projected coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Acceleration {
    /// Use the usual RFO step (Banerjee et al., J. Phys. Chem. 89, 52, 1985) \[Banerjee1985PRFO\].
    #[default]
    None,
    /// Minimize the interpolated quasi-Newton error (Császár & Pulay, J. Mol. Struct.
    /// 114, 31, 1984) with safeguards from Farkas & Schlegel (PCCP 4, 11, 2002)
    /// \[Csaszar1984GDIIS\] \[Farkas2002GDIIS\].
    Gdiis,
    /// Li & Frisch's hybrid (JCTC 2, 835, 2006) uses RFO above an RMS force of
    /// 0.001 Eh/bohr, then energy DIIS and finally GDIIS below an RMS step of
    /// 0.0025 bohr (switches reported by Sanz García, Toulouse, 2016, pp. 53–54)
    /// \[Li2006GEDIIS\] \[SanzGarcia2016Thesis\].
    Gediis,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Level {
    #[default]
    Normal,
    Tight,
}

/// Tests required to declare a minimum at the selected [`Level`].
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Convergence {
    /// Require energy, maximum/RMS gradient and maximum/RMS displacement tests,
    /// with an exception for a maximum gradient below 1/100 of its threshold.
    #[default]
    Standard,
    /// Require the level's maximum/RMS gradient and absolute energy-change tests,
    /// plus a free projected gradient norm strictly below `gradient_norm`
    /// (hartree/bohr), on two consecutive accepted displacements at the same
    /// accuracy. A maximum gradient below 1/100 of its threshold waives the
    /// energy-change and displacement requirements. Step magnitudes are unrestricted.
    ///
    /// Coarse cannot qualify. A Tight recheck tests its fresh gradient and may
    /// use the qualifying Normal pair and its last energy change. After a failed
    /// recheck, only Tight displacements count toward a new pair.
    EnergyGradient { gradient_norm: f64 },
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

/// With the Tight recheck off, the defaults used 2,663 evaluations on 154
/// GFN2-xTB starts (0.85× `xtb --opt` by geometric mean across starts).
#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    /// Search method. Defaults to [`Method::Internal`].
    pub method: Method,
    /// RFO trust policy. The default global model-space radius starts at 0.3
    /// and ranges from 0.001 to 0.5. [`Trust::PerAtom`] also caps atom motion.
    pub trust: Trust,
    /// Minimum actual/predicted reduction ratio for an RFO trial. The default
    /// of zero disables this test and retains the 1e-6 Eh energy-rise limit.
    pub reject_ratio: f64,
    /// Curvature floor in hartree/bohr² (default 1e-3). Sets TRIC fragment
    /// motion priors and regularizes supplied Hessians and ANC curvature.
    pub curvature_floor: f64,
    /// Scheduled RFO basis rebuild and curvature transport. Defaults to every
    /// 30 accepted displacements.
    pub rebuild: Rebuild,
    /// Add weak nonbonded stretch curvature to Lindh, excluding covalent and
    /// 1–3 pairs. Off by default, with no measurable GFN2 benchmark gain.
    pub contacts: bool,
    /// Starting RFO Hessian. Defaults to the Lindh model. A GFN2 Hessian seed
    /// cost more to compute than it saved in gradient evaluations.
    pub initial_hessian: InitialHessian,
    /// RFO Hessian update. Defaults to BFGS. The SR1/BFGS blend saved a few calls on the
    /// GFN2 benchmark but took longer overall.
    pub update: Update,
    /// DIIS-family trial steps in the active RFO coordinates. Off by default,
    /// with no consistent gain on the GFN2 benchmark.
    pub acceleration: Acceleration,
    /// Start at Coarse and promote once to Normal when the maximum free gradient
    /// is at most 20× its convergence threshold. Off by default because it
    /// increased evaluations on the GFN2 benchmark.
    pub adaptive_accuracy: bool,
    /// Convergence thresholds. Defaults to [`Level::Normal`].
    pub level: Level,
    /// Defaults to [`Convergence::Standard`]. The energy-gradient alternative
    /// reached the reference minimum less often on the GFN2 benchmark.
    pub convergence: Convergence,
    /// Limit on accepted displacements. Defaults to 500.
    pub max_steps: usize,
    /// Zero-based atom indices whose input coordinates are held bit-identical.
    /// Empty by default.
    pub fixed_atoms: Vec<usize>,
    /// Recheck convergence with [`Accuracy::Tight`]. Off by default. Turn it on
    /// when frequencies or other properties follow, so the final geometry is
    /// converged with Tight SCF.
    pub tight_final: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            method: Method::default(),
            trust: Trust::default(),
            reject_ratio: 0.,
            curvature_floor: 1e-3,
            rebuild: Rebuild::default(),
            contacts: false,
            initial_hessian: InitialHessian::Model,
            update: Update::Bfgs,
            acceleration: Acceleration::None,
            adaptive_accuracy: false,
            level: Level::Normal,
            convergence: Convergence::Standard,
            max_steps: 500,
            fixed_atoms: Vec::new(),
            tight_final: false,
        }
    }
}

/// An accepted geometry, including re-evaluations at a new accuracy at the same index.
/// The input and same-geometry accuracy changes report zero energy change and step lengths.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Step<'a> {
    pub index: usize,
    pub energy: f64,
    pub delta_energy: f64,
    pub max_gradient: f64,
    pub rms_gradient: f64,
    pub max_step: f64,
    pub rms_step: f64,
    /// Radius used for the accepted RFO step in global or per-free-atom units
    /// (see [`Trust`]). L-BFGS reports its Cartesian atom limit after backtracking.
    pub trust_radius: f64,
    /// Whether the accepted proposal reached the boundary used by the trust
    /// controller. With [`Trust::PerAtom`], either model or Cartesian length
    /// can reach the radius; the separate atom cap alone does not set this flag.
    /// L-BFGS uses the reported Cartesian atom limit after backtracking.
    /// False for the input and same-geometry accuracy changes.
    pub on_boundary: bool,
    /// Actual/predicted energy change for the accepted trial. RFO uses its
    /// rational model; L-BFGS uses the linear prediction g·s after backtracking.
    /// NaN for the input and same-geometry accuracy changes.
    pub ratio: f64,
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
    /// Energy/gradient calls, including rejected trials and failed calls.
    pub evaluations: usize,
    /// Calls to [`Potential::hessian`], including errors and unavailability probes.
    pub hessian_evaluations: usize,
    pub method: Method,
    /// Algorithms selected during the run, in order, including fallback attempts.
    pub methods: Vec<Method>,
    /// One energy per accepted geometry, including the input. A successful
    /// accuracy change replaces the entry at the same geometry.
    pub energies: Vec<f64>,
}

/// A restricted RFO proposal. Internal back-transforms can recover a useful
/// partial displacement, with its prediction evaluated at the attained point.
pub(crate) struct TrialStep {
    pub cartesian: Vec<f64>,
    pub predicted: f64,
    pub length: f64,
    pub converged_transform: bool,
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

#[derive(Clone, Copy)]
struct Diagnostics {
    radius: f64,
    on_boundary: bool,
    ratio: f64,
}

impl Diagnostics {
    fn at_geometry(radius: f64) -> Self {
        Self {
            radius,
            on_boundary: false,
            ratio: f64::NAN,
        }
    }
}

impl Point {
    fn step(
        &self,
        index: usize,
        motion: Motion,
        diagnostics: Diagnostics,
        accuracy: Accuracy,
    ) -> Step<'_> {
        let (max_gradient, rms_gradient) = metrics(&self.active_gradient);
        Step {
            index,
            energy: self.energy,
            delta_energy: motion.de,
            max_gradient,
            rms_gradient,
            max_step: motion.max,
            rms_step: motion.rms,
            trust_radius: diagnostics.radius,
            on_boundary: diagnostics.on_boundary,
            ratio: diagnostics.ratio,
            accuracy,
            positions: &self.positions,
        }
    }
}

impl Step<'_> {
    fn converged(&self, c: Criteria, has_displacement: bool) -> bool {
        self.accuracy != Accuracy::Coarse
            && self.max_gradient <= c.max_gradient
            && self.rms_gradient <= c.rms_gradient
            && (self.max_gradient < c.max_gradient / 100.
                || (has_displacement
                    && self.delta_energy.abs() <= c.energy
                    && self.max_step <= c.max_step
                    && self.rms_step <= c.rms_step))
    }

    fn energy_gradient(&self, c: Criteria, gradient_norm: f64, bound: f64) -> bool {
        self.accuracy != Accuracy::Coarse
            && self.max_gradient <= c.max_gradient
            && self.rms_gradient <= c.rms_gradient
            && self.delta_energy.abs() <= c.energy
            && gradient_norm < bound
    }
}

impl From<EvalError> for Status {
    fn from(error: EvalError) -> Self {
        match error {
            EvalError::Cancelled => Self::Cancelled,
            EvalError::Scf(m) => Self::ScfFailure(m),
            EvalError::Failed(m) => Self::Failed(m),
        }
    }
}

struct Evaluator<'a, P: ?Sized> {
    potential: &'a mut P,
    coords: Coordinates,
    accuracy: Accuracy,
    count: usize,
    hessian_count: usize,
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
            .evaluate(&positions, &mut gradient, self.accuracy)?;
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

    fn hessian(&mut self, positions: &[f64]) -> Result<Option<Vec<f64>>, Status> {
        let n = positions.len();
        let Some(size) = n.checked_mul(n) else {
            return Ok(None);
        };
        let mut h = Vec::new();
        if h.try_reserve_exact(size).is_err() {
            return Ok(None);
        }
        h.resize(size, f64::NAN);
        self.hessian_count += 1;
        match self.potential.hessian(positions, &mut h) {
            Some(Err(EvalError::Cancelled)) => return Err(Status::Cancelled),
            Some(Ok(())) if h.iter().all(|x| x.is_finite()) => {}
            _ => return Ok(None),
        }
        // Finite differences of analytic gradients can have small antisymmetric errors.
        for i in 0..n {
            for j in 0..i {
                let entry = f64::midpoint(h[i * n + j], h[j * n + i]);
                h[i * n + j] = entry;
                h[j * n + i] = entry;
            }
        }
        Ok(Some(h))
    }

    fn trial(&mut self, point: &Point, step: &[f64]) -> Result<Point, Status> {
        let mut positions = point.positions.clone();
        for (&i, s) in self.coords.indices.iter().zip(step) {
            positions[i] += s;
        }
        self.evaluate(positions)
    }
}

struct Engine<'a> {
    options: &'a Options,
    method: Method,
    methods: Vec<Method>,
    anc: Option<Anc>,
    internal: Option<InternalModel>,
    internal_built_at: usize,
    initial_hessian: Option<Vec<f64>>,
    history: History,
    lbfgs: Lbfgs,
    trust: TrustRegion,
}

impl<'a> Engine<'a> {
    fn new(options: &'a Options, free_atoms: usize) -> Self {
        Self {
            options,
            method: options.method,
            methods: vec![options.method],
            anc: None,
            internal: None,
            internal_built_at: 0,
            initial_hessian: None,
            history: History::default(),
            lbfgs: Lbfgs::default(),
            trust: TrustRegion::new(options, free_atoms),
        }
    }

    fn search_anc<P: Potential + ?Sized>(
        &mut self,
        eval: &mut Evaluator<P>,
        numbers: &[u8],
        point: &Point,
        steps: usize,
        accuracy_changed: bool,
    ) -> Result<Option<(Point, Diagnostics)>, Status> {
        if self.anc.as_ref().is_none_or(|a| {
            self.trust
                .should_rebuild(self.options.rebuild, steps - a.built_at)
        }) {
            self.anc = Anc::build(
                numbers,
                &eval.coords,
                point,
                steps,
                self.anc.as_ref(),
                self.initial_hessian.take(),
                self.options,
            )
            .ok();
            self.history.clear();
            self.trust.rebuilt();
        }
        let Some(anc) = &self.anc else {
            return Ok(None);
        };
        let diis = if self.options.acceleration == Acceleration::None {
            None
        } else {
            self.history.push(anc, point, &eval.coords);
            self.history
                .direction(anc, self.options.acceleration, self.trust.model_radius())
        };
        let rfo = LazyCell::new(|| anc.direction(&point.active_gradient));
        let search = self.trust.search(
            eval,
            point,
            accuracy_changed,
            |bound| bound.restrict(|r| diis.as_ref()?.restricted(r)).map(Ok),
            |bound| {
                bound
                    .restrict(|r| rfo.as_ref()?.restricted(r))
                    .ok_or(DirectionError::NoStep)
            },
        )?;
        Ok(search.accepted())
    }

    fn search_internal<P: Potential + ?Sized>(
        &mut self,
        eval: &mut Evaluator<P>,
        numbers: &[u8],
        point: &Point,
        steps: usize,
        accuracy_changed: bool,
    ) -> Result<Option<(Point, Diagnostics)>, Status> {
        let x = &point.positions;
        let mut rebuild = self
            .trust
            .should_rebuild(self.options.rebuild, steps - self.internal_built_at)
            || self
                .internal
                .as_ref()
                .is_some_and(|model| model.needs_rebuild(x));
        if self.internal.is_none() {
            self.internal = InternalModel::build(
                numbers,
                &eval.coords.indices,
                x,
                false,
                InternalCurvature::Initial(self.initial_hessian.as_deref(), self.options),
            )
            .ok();
        }
        let Some(model) = &mut self.internal else {
            return Ok(None);
        };
        let mut retried_chart = false;
        loop {
            if rebuild {
                if model.rebuild(x).is_err() {
                    return Ok(None);
                }
                self.history.clear();
                self.trust.rebuilt();
                self.internal_built_at = steps;
            }
            let diis = if self.options.acceleration == Acceleration::None {
                None
            } else {
                self.history
                    .push_internal(model, x, &point.active_gradient, point.energy);
                self.history.internal_direction(
                    model,
                    x,
                    self.options.acceleration,
                    self.trust.model_radius(),
                )
            };
            let rfo = LazyCell::new(|| model.direction(x, &point.active_gradient));
            let search = self.trust.search(
                eval,
                point,
                accuracy_changed,
                |bound| {
                    diis.map(|d| {
                        bound
                            .restrict(|r| d.restricted(x, r))
                            .ok_or(DirectionError::BackTransform)
                    })
                },
                |bound| {
                    let direction = rfo.as_ref().ok_or(DirectionError::NoStep)?;
                    bound
                        .restrict(|r| direction.restricted(x, r))
                        .ok_or(DirectionError::BackTransform)
                },
            )?;
            match search {
                Search::Rebuild if !retried_chart => {
                    rebuild = true;
                    retried_chart = true;
                }
                result => return Ok(result.accepted()),
            }
        }
    }

    fn search_lbfgs<P: Potential + ?Sized>(
        &mut self,
        eval: &mut Evaluator<P>,
        point: &Point,
        accuracy_changed: bool,
    ) -> Result<(Point, Diagnostics), Status> {
        loop {
            let Some(mut d) = self.lbfgs.direction(&point.active_gradient, &point.rigid) else {
                return Err(Status::Failed("no finite L-BFGS descent direction".into()));
            };
            let slope = dot(&point.active_gradient, &d);
            let mut scale = 1.;
            // The accuracy switch discarded the learned inverse curvature.
            // A stiff mode can need a much shorter first step on this surface.
            let halvings = if accuracy_changed { 20 } else { 10 };
            for _ in 0..=halvings {
                let trial = eval.trial(point, &d)?;
                if trial.energy <= point.energy + 1e-4 * scale * slope {
                    let radius = 0.3 * scale;
                    let largest = d
                        .as_chunks::<3>()
                        .0
                        .iter()
                        .map(|atom| norm(atom))
                        .fold(0., f64::max);
                    let diagnostics = Diagnostics {
                        radius,
                        on_boundary: largest >= radius * (1. - 32. * f64::EPSILON),
                        ratio: (trial.energy - point.energy) / (scale * slope),
                    };
                    return Ok((trial, diagnostics));
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
        accuracy_changed: bool,
    ) -> Result<(Point, Motion, Diagnostics), Status> {
        let (next, diagnostics) = loop {
            match self.method {
                Method::Internal => {
                    if let Some(trial) =
                        self.search_internal(eval, numbers, point, steps, accuracy_changed)?
                    {
                        // The seed remains available to ANC until the input point moves.
                        self.initial_hessian = None;
                        break trial;
                    }
                    self.method = Method::AncRfo;
                    self.methods.push(self.method);
                    self.internal = None;
                    self.history.clear();
                    self.trust = TrustRegion::new(self.options, eval.coords.indices.len() / 3);
                }
                Method::AncRfo => {
                    if let Some(trial) =
                        self.search_anc(eval, numbers, point, steps, accuracy_changed)?
                    {
                        break trial;
                    }
                    self.method = Method::Lbfgs;
                    self.methods.push(self.method);
                    self.anc = None;
                }
                Method::Lbfgs => break self.search_lbfgs(eval, point, accuracy_changed)?,
            }
        };
        // Use the displacement that survived floating-point addition at the trial.
        let s: Vec<_> = eval
            .coords
            .indices
            .iter()
            .map(|&i| next.positions[i] - point.positions[i])
            .collect();
        let (max, rms) = metrics(&s);
        let motion = Motion {
            de: next.energy - point.energy,
            max,
            rms,
        };
        if let Some(model) = &mut self.internal {
            model.update(
                &point.positions,
                &next.positions,
                &point.active_gradient,
                &next.active_gradient,
                self.options.update,
            );
        } else {
            let y = next
                .active_gradient
                .iter()
                .zip(&point.active_gradient)
                .map(|(new, old)| new - old)
                .collect::<Vec<_>>();
            if let Some(anc) = &mut self.anc {
                anc.update(&s, &y, self.options.update);
            } else {
                self.lbfgs.update(s, y);
            }
        }
        Ok((next, motion, diagnostics))
    }
}

/// Minimize an energy in atomic units. By default, TRIC RFO starts with the
/// Lindh model Hessian and updates it with BFGS after accepted displacements.
///
/// The default global trust radius bounds the Euclidean step in model space.
/// It starts at 0.3, shrinks by a factor of four for an accepted reduction
/// ratio below 0.25, and doubles for a boundary step above 0.75, within
/// [0.001, 0.5]. By default, a trial energy rise above 1e-6 Eh causes
/// rejection. TRIC transports curvature through free Cartesians when
/// rebuilding after 30 accepted steps by default, when its chart becomes
/// invalid, or after two consecutive incomplete or failed back-transforms.
///
/// An unusable model or exhausted search switches methods at the last accepted
/// geometry in the order Internal → ANC-RFO → L-BFGS. An energy or gradient
/// evaluation error stops the run.
///
/// The default [`Convergence::Standard`] uses [`Level`] thresholds on absolute
/// energy change and maximum/RMS free Cartesian gradient and step components.
/// A maximum gradient below 1/100 of its threshold waives the energy and step tests.
/// Without held atoms, the gradient is projected off global rigid motion.
/// [`Convergence::EnergyGradient`] instead requires two qualifying accepted
/// displacements and a gradient norm bound. The same small-gradient exception
/// waives its energy-change and displacement-pair tests. Only Normal or Tight
/// can converge.
///
/// Evaluations start at [`Accuracy::Normal`] by default. With
/// [`Options::tight_final`] enabled, the optimizer rechecks the converged
/// geometry with a fresh Tight gradient, using the preceding Normal
/// displacement and energy change if available. Further steps stay at Tight.
/// Adaptive accuracy starts at Coarse, then switches to Normal at the gradient
/// cutoff and resets the convergence displacement.
///
/// Atomic numbers must be in 1..=118, with three finite Cartesian components
/// per atom. Held indices must be unique and in range, with at least one free
/// atom. Held coordinates stay bit-identical to the input.
///
/// The callback sees the input, accepted steps, and successful accuracy
/// re-evaluations. A callback break or [`EvalError::Cancelled`] returns
/// [`Status::Cancelled`] with the last accepted sample, if available.
/// Only accepted displacements count toward [`Options::max_steps`].
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
        hessian_evaluations: 0,
        method: options.method,
        methods: vec![options.method],
        energies: Vec::new(),
    };
    let coords = match Coordinates::new(numbers, positions, &options.fixed_atoms) {
        Ok(c) => c,
        Err(m) => {
            out.status = Status::Failed(m);
            return out;
        }
    };
    if let Convergence::EnergyGradient { gradient_norm } = options.convergence
        && (!gradient_norm.is_finite() || gradient_norm <= 0.)
    {
        out.status = Status::Failed("gradient_norm must be finite and positive".into());
        return out;
    }
    if options.method != Method::Lbfgs
        && let Err(message) = options.validate_rfo()
    {
        out.status = Status::Failed(message.into());
        return out;
    }
    let mut eval = Evaluator {
        potential,
        coords,
        accuracy: if options.adaptive_accuracy {
            Accuracy::Coarse
        } else {
            Accuracy::Normal
        },
        count: 0,
        hessian_count: 0,
    };
    let mut point = match eval.evaluate(positions.to_vec()) {
        Ok(p) => p,
        Err(status) => {
            out.status = status;
            out.evaluations = eval.count;
            return out;
        }
    };
    let mut engine = Engine::new(options, eval.coords.indices.len() / 3);
    let request_hessian = options.method != Method::Lbfgs
        && match options.initial_hessian {
            InitialHessian::Model => false,
            InitialHessian::Potential => true,
            InitialHessian::Auto { max_atoms } => numbers.len() <= max_atoms,
        };
    let criteria = options.level.criteria();
    let mut motion = None;
    let mut accuracy_changed = false;
    let mut verification_motion = None;
    let mut qualifying_steps = 0_u8;
    let mut diagnostics = Diagnostics::at_geometry(if options.method == Method::Lbfgs {
        0.3
    } else {
        engine.trust.radius
    });
    out.energies.push(point.energy);
    out.status = loop {
        let info = point.step(
            out.steps,
            motion.unwrap_or_default(),
            diagnostics,
            eval.accuracy,
        );
        if on_step(&info).is_break() {
            break Status::Cancelled;
        }
        let convergence_motion = motion.or(verification_motion);
        // Re-evaluations report no movement, but the Tight convergence check
        // may still use the accepted Normal displacement that triggered it.
        let mut convergence_info = info;
        if let Some(previous) = convergence_motion {
            convergence_info.delta_energy = previous.de;
            convergence_info.max_step = previous.max;
            convergence_info.rms_step = previous.rms;
        }
        let converged = match options.convergence {
            Convergence::Standard => {
                convergence_info.converged(criteria, convergence_motion.is_some())
            }
            Convergence::EnergyGradient { gradient_norm } => {
                let active_gradient_norm = norm(&point.active_gradient);
                let small_gradient = convergence_info.converged(criteria, false)
                    && active_gradient_norm < gradient_norm;
                let qualifies = convergence_motion.is_some()
                    && convergence_info.energy_gradient(
                        criteria,
                        active_gradient_norm,
                        gradient_norm,
                    );
                if motion.is_some() {
                    qualifying_steps = if qualifies {
                        qualifying_steps.saturating_add(1)
                    } else {
                        0
                    };
                } else if !qualifies || qualifying_steps < 2 {
                    // Only a complete pair can survive a Tight recheck.
                    qualifying_steps = 0;
                }
                small_gradient || (qualifies && qualifying_steps >= 2)
            }
        };
        let next_accuracy = if converged {
            if !options.tight_final || eval.accuracy == Accuracy::Tight {
                break Status::Converged;
            }
            Accuracy::Tight
        } else if eval.accuracy == Accuracy::Coarse
            && info.max_gradient <= 20. * criteria.max_gradient
        {
            Accuracy::Normal
        } else {
            eval.accuracy
        };
        if next_accuracy != eval.accuracy {
            verification_motion = if next_accuracy == Accuracy::Tight {
                motion
            } else {
                None
            };
            eval.accuracy = next_accuracy;
            point = match eval.evaluate(point.positions.clone()) {
                Ok(p) => p,
                Err(status) => break status,
            };
            accuracy_changed = true;
            out.energies[out.steps] = point.energy;
            engine.lbfgs.clear();
            engine.history.clear();
            motion = None;
            diagnostics = Diagnostics::at_geometry(diagnostics.radius);
            continue;
        }
        if out.steps == options.max_steps {
            break Status::MaxSteps;
        }
        if request_hessian && out.steps == 0 {
            engine.initial_hessian = match eval.hessian(&point.positions) {
                Ok(h) => h,
                Err(status) => break status,
            };
        }
        match engine.advance(&mut eval, numbers, &point, out.steps, accuracy_changed) {
            Ok((next, displacement, next_diagnostics)) => {
                point = next;
                motion = Some(displacement);
                accuracy_changed = false;
                verification_motion = None;
                diagnostics = next_diagnostics;
            }
            Err(status) => break status,
        }
        out.steps += 1;
        out.energies.push(point.energy);
    };
    out.positions = point.positions;
    out.energy = point.energy;
    out.gradient = point.gradient;
    out.evaluations = eval.count;
    out.hessian_evaluations = eval.hessian_count;
    out.method = engine.method;
    out.methods = engine.methods;
    out
}

#[cfg(test)]
mod potential_tests;

#[cfg(test)]
mod tests;
