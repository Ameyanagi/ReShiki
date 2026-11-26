//! Transition-state searches on mapped Cartesian geometries in atomic units.
//!
//! A climbing-image elastic band supplies the starting point and direction for
//! partitioned rational-function optimization. Vibrational analysis and two
//! downhill intrinsic reaction paths characterize the resulting stationary point.
//!
//! Bracketed keys such as \[Henkelman2000CINEB\] name the full citations in
//! docs/references/xtb.md.

mod endpoint;
mod idpp;
mod irc;
mod mapping;
mod modes;
mod neb;
mod reactive_start;
mod saddle;

#[cfg(test)]
mod tests;

pub use endpoint::{bonds, build_endpoint};
pub use mapping::least_motion_map;
pub use reactive_start::{ReactiveStart, prepare_reactive_start};
use std::ops::ControlFlow;

use crate::{Accuracy, EvalError, KCAL_PER_HARTREE, Options, Potential, Status, optimize};

/// Upper bound for searches using dense Cartesian Hessians.
pub const MAX_ATOMS: usize = 120;
/// An imaginary frequency must exceed this magnitude in cm⁻¹ to qualify.
pub const IMAGINARY_CM1: f64 = 50.0;

/// Mapped endpoints in bohr. Atom i in the reactant corresponds to atom i in the product.
#[derive(Debug, Clone, Copy)]
pub struct Search<'a> {
    pub numbers: &'a [u8],
    pub masses_amu: &'a [f64],
    pub reactant: &'a [f64],
    pub product: &'a [f64],
    pub options: &'a TsOptions,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TsOptions {
    /// Number of images, including both endpoints (5 through 24).
    pub images: usize,
    /// Relax both input geometries at the optimizer's Normal level.
    pub optimize_endpoints: bool,
    pub neb_max_iterations: usize,
    pub saddle_max_steps: usize,
    /// Energy headroom in kcal/mol above the climbing image. A successful
    /// handoff to an observed between-image peak raises the reference to that
    /// midpoint's energy before applying the same headroom.
    pub saddle_headroom_kcal: f64,
    /// Periodic Hessian rebuild interval in accepted saddle steps. Zero disables
    /// scheduled builds. Mode tracking, failed secant steps and the final
    /// stationarity check can also request a fresh Hessian.
    pub hessian_every: usize,
    /// Maximum number of downhill IRC steps in each direction, before relaxation.
    pub irc_max_steps: usize,
    /// Mass-weighted arc length per IRC step (√amu bohr).
    pub irc_step: f64,
}

impl Default for TsOptions {
    fn default() -> Self {
        Self {
            images: 15,
            optimize_endpoints: true,
            neb_max_iterations: 300,
            saddle_max_steps: 100,
            saddle_headroom_kcal: 10.0,
            hessian_every: 0,
            irc_max_steps: 60,
            irc_step: 0.15,
        }
    }
}

/// Why saddle refinement stopped. The final spectrum independently reports
/// whether a stationary point has one qualifying imaginary frequency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaddleReason {
    Stationary,
    AcceptedStepLimit,
    MinimumRadiusExhausted,
    EnergyCeiling,
    FollowedModeLost,
    HessianBudgetExhausted,
}

impl std::fmt::Display for SaddleReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Stationary => "stationary",
            Self::AcceptedStepLimit => "accepted-step limit",
            Self::MinimumRadiusExhausted => "minimum radius exhausted",
            Self::EnergyCeiling => "energy ceiling exhausted",
            Self::FollowedModeLost => "followed mode lost",
            Self::HessianBudgetExhausted => "recovery Hessian budget exhausted",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Endpoints,
    Interpolation,
    Neb,
    Saddle,
    Frequencies,
    Irc,
}

/// Progress on the calling thread. Energies are hartree, forces and gradients hartree/bohr.
#[derive(Debug, Clone, Copy)]
pub enum Event<'a> {
    Endpoint {
        product: bool,
        step: usize,
        energy: f64,
    },
    Neb {
        iteration: usize,
        energies: &'a [f64],
        max_force: f64,
        climbing: Option<usize>,
    },
    Saddle {
        step: usize,
        energy: f64,
        max_gradient: f64,
        rms_gradient: f64,
        eigenvalue: f64,
        overlap: f64,
        radius: f64,
        /// Newton displacement along the IRC launch mode, when checked.
        mode_displacement: Option<f64>,
        /// Whether the current Hessian was evaluated at this geometry.
        exact: bool,
    },
    SaddleTrial {
        energy: f64,
        max_gradient: f64,
        rms_gradient: f64,
        length: f64,
        /// Ratio used for the trust decision, including force-based acceptance.
        ratio: f64,
        /// Measured over predicted energy change. May be undefined for a zero prediction.
        energy_ratio: f64,
        /// Measured over predicted squared-force reduction. May be undefined for a zero prediction.
        force_ratio: f64,
        decision: &'static str,
    },
    SaddleStatus(&'static str),
    /// Steps after the downhill path also cover the branch's final minimization.
    Irc {
        forward: bool,
        step: usize,
        energy: f64,
    },
    Stage(Stage),
}

/// Cartesian positions in bohr and their energy in hartree.
#[derive(Debug, Clone, PartialEq)]
pub struct Minimum {
    pub positions: Vec<f64>,
    pub energy: f64,
}

/// Identity of a relaxed IRC end under the supplied atom map. A unique match to
/// an endpoint's covalent bond graph includes its other conformers and separated
/// fragments. When both endpoint graphs match, the closer endpoint must have
/// superposed all-atom RMSD below 0.5 Å. Otherwise the end is `Other`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reached {
    Reactant,
    Product,
    Other,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IrcBranch {
    /// TS energy, each accepted downhill point, and the relaxed end energy.
    pub energies: Vec<f64>,
    /// Accepted downhill points in bohr, excluding the TS and relaxed end.
    pub path: Vec<Minimum>,
    /// Whether the downhill trace reached its gradient tolerance before relaxation.
    pub completed: bool,
    pub end: Minimum,
    pub reached: Reached,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TsReport {
    pub reactant: Minimum,
    pub product: Minimum,
    /// Final band including endpoints, in the reactant frame.
    pub path: Vec<Minimum>,
    /// Whether the physical band met the force tolerances before refinement.
    pub neb_converged: bool,
    pub climbing_image: usize,
    pub ts: Minimum,
    /// Whether P-RFO met the Normal projected gradient tolerances and its IRC launch-mode
    /// Newton displacement was below one tenth of the mass-weighted IRC step.
    pub saddle_converged: bool,
    pub saddle_reason: SaddleReason,
    /// Diagnostics from optional recovery and candidate checks, including
    /// geometry-specific SCF failures and unresolved energy ceilings.
    /// Cancellation and fatal potential errors remain search errors.
    pub saddle_diagnostics: Vec<String>,
    /// Rebuilt Cartesian Hessian at the fitted TS in Eh/bohr², stored row-major.
    pub hessian: Vec<f64>,
    /// Ascending projected harmonic wavenumbers, with imaginary values negative.
    pub frequencies_cm1: Vec<f64>,
    /// Number of projected frequencies strictly below −[`IMAGINARY_CM1`].
    pub imaginary: usize,
    /// Cartesian unit displacement corresponding to the lowest mass-weighted mode.
    pub transition_mode: Vec<f64>,
    /// Branches along +transition_mode and −transition_mode, respectively.
    /// Present only for a converged candidate with one qualifying imaginary frequency.
    pub irc: Option<[IrcBranch; 2]>,
    /// Failure during IRC verification after characterization. Cancellation is
    /// returned as [`TsError::Cancelled`] instead of being recorded here.
    pub irc_error: Option<String>,
    /// Whether the two relaxed IRC ends classify as the two requested endpoints.
    pub connects: bool,
    /// All energy/gradient calls, including central differences and minimizations.
    pub evaluations: usize,
    /// Hessian builds, counting an unavailable analytic probe plus its differences as one.
    pub hessians: usize,
}

impl TsReport {
    pub fn forward_barrier_kcal(&self) -> f64 {
        (self.ts.energy - self.reactant.energy) * KCAL_PER_HARTREE
    }

    pub fn reverse_barrier_kcal(&self) -> f64 {
        (self.ts.energy - self.product.energy) * KCAL_PER_HARTREE
    }
}

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum TsError {
    #[error("invalid transition-state input: {0}")]
    Invalid(String),
    #[error("SCF failure: {0}")]
    Scf(String),
    #[error("{0}")]
    Failed(String),
    #[error("transition-state search cancelled")]
    Cancelled,
}

impl From<EvalError> for TsError {
    fn from(error: EvalError) -> Self {
        match error {
            EvalError::Scf(message) => Self::Scf(message),
            EvalError::Failed(message) => Self::Failed(message),
            EvalError::Cancelled => Self::Cancelled,
        }
    }
}

fn validate_geometry(numbers: &[u8], x: &[f64]) -> Result<(), TsError> {
    if !(2..=MAX_ATOMS).contains(&numbers.len()) || numbers.iter().any(|z| !(1..=118).contains(z)) {
        return Err(TsError::Invalid(format!(
            "expected 2..={MAX_ATOMS} atoms with atomic numbers in 1..=118"
        )));
    }
    if x.len() != 3 * numbers.len() || !x.iter().all(|v| v.is_finite()) {
        return Err(TsError::Invalid(
            "expected three finite coordinates per atom".into(),
        ));
    }
    Ok(())
}

impl Search<'_> {
    fn validate(&self) -> Result<(), TsError> {
        validate_geometry(self.numbers, self.reactant)?;
        endpoint::validate_nuclei_separation(self.reactant)?;
        validate_geometry(self.numbers, self.product)?;
        endpoint::validate_nuclei_separation(self.product)?;
        if self.masses_amu.len() != self.numbers.len()
            || !self.masses_amu.iter().all(|&m| m.is_finite() && m > 0.)
        {
            return Err(TsError::Invalid(
                "expected one finite positive mass per atom".into(),
            ));
        }
        if !(5..=24).contains(&self.options.images) {
            return Err(TsError::Invalid("images must be in 5..=24".into()));
        }
        if !self.options.saddle_headroom_kcal.is_finite() || self.options.saddle_headroom_kcal < 0.
        {
            return Err(TsError::Invalid(
                "saddle headroom must be finite and nonnegative".into(),
            ));
        }
        if !self.options.irc_step.is_finite() || self.options.irc_step <= 0. {
            return Err(TsError::Invalid(
                "IRC step must be finite and positive".into(),
            ));
        }
        Ok(())
    }
}

struct Counted<'a, P> {
    potential: &'a mut P,
    evaluations: usize,
    hessians: usize,
}

impl<P: Potential> Potential for Counted<'_, P> {
    fn evaluate(&mut self, x: &[f64], g: &mut [f64], a: Accuracy) -> Result<f64, EvalError> {
        self.evaluations += 1;
        self.potential.evaluate(x, g, a)
    }

    fn hessian(&mut self, x: &[f64], h: &mut [f64]) -> Option<Result<(), EvalError>> {
        self.hessians += 1;
        self.potential.hessian(x, h)
    }
}

fn evaluate(p: &mut impl Potential, x: &[f64]) -> Result<(f64, Vec<f64>), TsError> {
    let mut g = vec![f64::NAN; x.len()];
    let e = p.evaluate(x, &mut g, Accuracy::Normal)?;
    if !e.is_finite() || !g.iter().all(|v| v.is_finite()) {
        return Err(TsError::Failed(
            "non-finite potential energy or gradient".into(),
        ));
    }
    Ok((e, g))
}

fn minimize(
    p: &mut impl Potential,
    numbers: &[u8],
    x: &[f64],
    on_step: impl FnMut(usize, f64) -> ControlFlow<()>,
) -> Result<Minimum, TsError> {
    minimize_with_options(p, numbers, x, &Options::default(), on_step)
}

fn minimize_with_options(
    p: &mut impl Potential,
    numbers: &[u8],
    x: &[f64],
    options: &Options,
    mut on_step: impl FnMut(usize, f64) -> ControlFlow<()>,
) -> Result<Minimum, TsError> {
    let out = optimize(p, numbers, x, options, |s| on_step(s.index, s.energy));
    match out.status {
        Status::Converged => Ok(Minimum {
            positions: out.positions,
            energy: out.energy,
        }),
        Status::MaxSteps => Err(TsError::Failed(
            "endpoint minimization reached its step limit".into(),
        )),
        Status::ScfFailure(m) => Err(TsError::Scf(m)),
        Status::Failed(m) => Err(TsError::Failed(m)),
        Status::Cancelled => Err(TsError::Cancelled),
    }
}

fn emit(on_event: &mut impl FnMut(Event) -> ControlFlow<()>, event: Event) -> Result<(), TsError> {
    match on_event(event) {
        ControlFlow::Break(()) => Err(TsError::Cancelled),
        ControlFlow::Continue(()) => Ok(()),
    }
}

/// Locate and characterize a saddle between two atom-mapped geometries.
/// Refinement proceeds from the highest interior image even if NEB exhausts
/// its budget. A first-order saddle has one frequency below −[`IMAGINARY_CM1`].
/// The report retains the spectrum and stationarity status of each candidate.
/// IRC runs only for a converged refinement with exactly one qualifying mode.
/// Image i uses potential i modulo the pool size. All other evaluations use
/// the first potential. The caller may cancel at any stage or progress event.
pub fn search<P: Potential + Send>(
    potentials: &mut [P],
    search: &Search,
    on_event: impl FnMut(Event) -> ControlFlow<()>,
) -> Result<TsReport, TsError> {
    search_with_cancel(potentials, search, on_event, || ControlFlow::Continue(()))
}

/// Like [`search`], with additional cancellation polling during IDPP interpolation.
/// `on_cancel` runs before the first IDPP iteration and at most 20 iterations
/// apart thereafter. Return `ControlFlow::Break(())` to cancel the search.
pub fn search_with_cancel<P: Potential + Send>(
    potentials: &mut [P],
    search: &Search,
    mut on_event: impl FnMut(Event) -> ControlFlow<()>,
    mut on_cancel: impl FnMut() -> ControlFlow<()>,
) -> Result<TsReport, TsError> {
    search.validate()?;
    if potentials.is_empty() {
        return Err(TsError::Invalid(
            "at least one potential is required".into(),
        ));
    }
    let mut pool: Vec<_> = potentials
        .iter_mut()
        .map(|potential| Counted {
            potential,
            evaluations: 0,
            hessians: 0,
        })
        .collect();
    let options = search.options;
    let masses = search.masses_amu;
    emit(&mut on_event, Event::Stage(Stage::Endpoints))?;
    let mut endpoint = |product, x: &[f64]| {
        let progress = |step, energy| Event::Endpoint {
            product,
            step,
            energy,
        };
        if options.optimize_endpoints {
            minimize(&mut pool[0], search.numbers, x, |i, e| {
                on_event(progress(i, e))
            })
        } else {
            let (energy, _) = evaluate(&mut pool[0], x)?;
            emit(&mut on_event, progress(0, energy))?;
            Ok(Minimum {
                positions: x.to_vec(),
                energy,
            })
        }
    };
    let reactant = endpoint(false, search.reactant)?;
    let mut product = endpoint(true, search.product)?;
    product.positions = mapping::superpose(&reactant.positions, &product.positions, masses);

    emit(&mut on_event, Event::Stage(Stage::Interpolation))?;
    let images = idpp::interpolate(
        &reactant.positions,
        &product.positions,
        masses,
        options.images,
        &mut on_cancel,
    )?;
    emit(&mut on_event, Event::Stage(Stage::Neb))?;
    let band = neb::run(
        &mut pool,
        images,
        [reactant.energy, product.energy],
        masses,
        options,
        &mut on_event,
    )?;

    let mut report =
        refine_and_characterize(&mut pool[0], search, band, reactant, product, &mut on_event)?;
    report.evaluations = pool.iter().map(|p| p.evaluations).sum();
    report.hessians = pool.iter().map(|p| p.hessians).sum();
    Ok(report)
}

/// Refine the highest interior image of a saved band, then characterize the
/// candidate with the same frequency and IRC checks as [`search`].
///
/// The saved images and their finite energies must be in reactant-to-product
/// order, in bohr and hartree. Their count must match `search.options.images`,
/// and the endpoint coordinates must match `search.reactant` and
/// `search.product`. Endpoint optimization, interpolation and NEB are skipped;
/// `neb_converged` preserves the earlier band's convergence status. All images
/// must describe the same potential and atomic state as `potential`.
///
/// Events begin at [`Stage::Saddle`]. Returning `ControlFlow::Break(())` from
/// `on_event` cancels refinement or characterization just as in [`search`].
pub fn refine_from_band<P: Potential>(
    potential: &mut P,
    search: &Search<'_>,
    path: &[Minimum],
    neb_converged: bool,
    mut on_event: impl FnMut(Event) -> ControlFlow<()>,
) -> Result<TsReport, TsError> {
    search.validate()?;
    if path.len() != search.options.images {
        return Err(TsError::Invalid(
            "saved band length must match the requested image count".into(),
        ));
    }
    for point in path {
        validate_geometry(search.numbers, &point.positions)?;
        endpoint::validate_nuclei_separation(&point.positions)?;
        if !point.energy.is_finite() {
            return Err(TsError::Invalid(
                "saved band energies must be finite".into(),
            ));
        }
    }
    // Search validation guarantees at least five images, so these endpoints
    // exist once the saved length has been checked.
    let reactant = &path[0];
    let product = &path[path.len() - 1];
    if reactant.positions.as_slice() != search.reactant
        || product.positions.as_slice() != search.product
    {
        return Err(TsError::Invalid(
            "saved band endpoints must match the search endpoints".into(),
        ));
    }
    let mut counted = Counted {
        potential,
        evaluations: 0,
        hessians: 0,
    };
    let band = neb::Band::from_saved(path, search.masses_amu, neb_converged);
    refine_and_characterize(
        &mut counted,
        search,
        band,
        reactant.clone(),
        product.clone(),
        &mut on_event,
    )
}

fn refine_and_characterize<P: Potential>(
    potential: &mut Counted<'_, P>,
    search: &Search<'_>,
    band: neb::Band,
    reactant: Minimum,
    product: Minimum,
    on_event: &mut impl FnMut(Event) -> ControlFlow<()>,
) -> Result<TsReport, TsError> {
    let options = search.options;
    let masses = search.masses_amu;
    emit(on_event, Event::Stage(Stage::Saddle))?;
    let saddle::Refinement {
        point: mut ts,
        converged: saddle_converged,
        reason: saddle_reason,
        diagnostics: saddle_diagnostics,
        hessian,
    } = saddle::refine_band(potential, &band, masses, options, on_event)?;
    // The exact Hessian that certified the Newton displacement also supplies the
    // spectrum. Rotate it with the fitted geometry so the reported mode and IRC
    // launch share that frame. An exhausted Bofill model needs a fresh build.
    let fit = mapping::Fit::new(&reactant.positions, &ts.positions, masses);
    ts.positions = fit.apply(&ts.positions);
    emit(on_event, Event::Stage(Stage::Frequencies))?;
    let hessian = match hessian {
        Some(h) => fit.rotate_hessian(&h, ts.positions.len()),
        None => saddle::hessian(potential, &ts.positions)?,
    };
    let (frequencies_cm1, transition_mode) = modes::frequencies(&ts.positions, &hessian, masses)?;
    let imaginary = frequencies_cm1
        .iter()
        .filter(|&&f| f < -IMAGINARY_CM1)
        .count();
    let (irc, irc_error) = if saddle_converged && imaginary == 1 {
        emit(on_event, Event::Stage(Stage::Irc))?;
        match irc::branches(
            potential,
            search,
            [&reactant, &product],
            &ts,
            &hessian,
            &transition_mode,
            on_event,
        ) {
            Ok(branches) => (Some(branches), None),
            Err(TsError::Cancelled) => return Err(TsError::Cancelled),
            Err(error) => (None, Some(error.to_string())),
        }
    } else {
        (None, None)
    };
    let connects = irc.as_ref().is_some_and(|[plus, minus]| {
        matches!(
            (plus.reached, minus.reached),
            (Reached::Reactant, Reached::Product) | (Reached::Product, Reached::Reactant)
        )
    });
    Ok(TsReport {
        reactant,
        product,
        path: band.path,
        neb_converged: band.converged,
        climbing_image: band.climbing,
        ts,
        saddle_converged,
        saddle_reason,
        saddle_diagnostics,
        hessian,
        imaginary,
        frequencies_cm1,
        transition_mode,
        irc,
        irc_error,
        connects,
        evaluations: potential.evaluations,
        hessians: potential.hessians,
    })
}
