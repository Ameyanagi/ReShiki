//! Transition-state searches on mapped Cartesian geometries in atomic units.
//!
//! A climbing-image elastic band supplies the starting point and direction for
//! partitioned rational-function optimization. Vibrational analysis and two
//! downhill intrinsic reaction paths characterize the resulting stationary point.

mod endpoint;
mod idpp;
mod irc;
mod mapping;
mod modes;
mod neb;
mod saddle;

#[cfg(test)]
mod tests;

pub use endpoint::{bonds, build_endpoint};
pub use mapping::least_motion_map;
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
    /// Rebuild interval in saddle steps. Zero uses Bofill updates throughout.
    pub hessian_every: usize,
    /// Maximum number of downhill IRC steps in each direction, before relaxation.
    pub irc_max_steps: usize,
    /// Mass-weighted displacement per IRC step (√amu bohr).
    pub irc_step: f64,
}

impl Default for TsOptions {
    fn default() -> Self {
        Self {
            images: 15,
            optimize_endpoints: true,
            neb_max_iterations: 300,
            saddle_max_steps: 100,
            hessian_every: 0,
            irc_max_steps: 60,
            irc_step: 0.15,
        }
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
        eigenvalue: f64,
    },
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
    /// Whether the P-RFO point met the Normal projected gradient tolerances.
    pub saddle_converged: bool,
    /// Ascending projected harmonic wavenumbers, with imaginary values negative.
    pub frequencies_cm1: Vec<f64>,
    /// Cartesian unit displacement corresponding to the lowest mass-weighted mode.
    pub transition_mode: Vec<f64>,
    /// Branches along +transition_mode and −transition_mode, respectively.
    /// Absent when the stationary point has no qualifying imaginary mode.
    pub irc: Option<[IrcBranch; 2]>,
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
        validate_geometry(self.numbers, self.product)?;
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
    mut on_step: impl FnMut(usize, f64) -> ControlFlow<()>,
) -> Result<Minimum, TsError> {
    let out = optimize(p, numbers, x, &Options::default(), |s| {
        on_step(s.index, s.energy)
    });
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
/// All qualifying modes appear in the report at a higher-order saddle.
/// A spectrum with no qualifying mode is reported without tracing IRC.
/// Image i uses potential i modulo the pool size. All other evaluations use
/// the first potential. The caller may cancel at any stage or progress event.
pub fn search<P: Potential + Send>(
    potentials: &mut [P],
    search: &Search,
    mut on_event: impl FnMut(Event) -> ControlFlow<()>,
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

    emit(&mut on_event, Event::Stage(Stage::Saddle))?;
    let (mut ts, saddle_converged) = saddle::refine(
        &mut pool[0],
        &band.path[band.climbing],
        &band.tangent,
        true,
        options,
        &mut on_event,
    )?;
    // Refit before building the Hessian so its Cartesian eigenvectors and all
    // reported geometries share the same mass-weighted reactant frame.
    ts.positions = mapping::superpose(&reactant.positions, &ts.positions, masses);
    emit(&mut on_event, Event::Stage(Stage::Frequencies))?;
    let hessian = saddle::hessian(&mut pool[0], &ts.positions)?;
    let (frequencies_cm1, transition_mode) = modes::frequencies(&ts.positions, &hessian, masses)?;
    let irc = if frequencies_cm1.iter().any(|&f| f < -IMAGINARY_CM1) {
        emit(&mut on_event, Event::Stage(Stage::Irc))?;
        Some(irc::branches(
            &mut pool[0],
            search,
            [&reactant, &product],
            &ts,
            &hessian,
            &transition_mode,
            &mut on_event,
        )?)
    } else {
        None
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
        frequencies_cm1,
        transition_mode,
        irc,
        connects,
        evaluations: pool.iter().map(|p| p.evaluations).sum(),
        hessians: pool.iter().map(|p| p.hessians).sum(),
    })
}
