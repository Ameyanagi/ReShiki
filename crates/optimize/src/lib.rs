//! Geometry optimization and conformer ranking for any energy-and-gradient potential.
//!
//! Written from the published methods (TRIC, Wang & Song 2016, Lindh et al. 1995
//! model Hessian, rational function optimization, BFGS, L-BFGS, Horn quaternion
//! superposition, and Butina clustering). The implementation is independent of
//! xtb, tblite, and axtb.

#![forbid(unsafe_code)]

pub mod conformers;
mod linalg;

mod model_hessian;
mod optimizer;

pub use optimizer::{
    Acceleration, Convergence, Criteria, InitialHessian, Level, Method, Options, Outcome, Rebuild,
    Status, Step, Trust, Update, optimize,
};

/// Hartree in kcal/mol (CODATA 2022 hartree, thermochemical calorie).
pub const KCAL_PER_HARTREE: f64 = 627.509_474_063;
/// Ångströms per bohr, as used by the saved scientific data.
pub use reshiki_qcdata::ANGSTROM_PER_BOHR;

/// Accuracy a potential should use for one evaluation.
///
/// Optimization starts at [`Accuracy::Normal`], or [`Accuracy::Coarse`] with
/// [`Options::adaptive_accuracy`]. Opt in with [`Options::tight_final`] to
/// switch to [`Accuracy::Tight`] once the convergence criteria are met.
/// Potentials with fixed accuracy ignore this setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accuracy {
    /// Looser early SCF convergence. Potentials with fixed accuracy treat this as `Normal`.
    Coarse,
    /// Ordinary accuracy for optimization steps.
    Normal,
    /// Tighter accuracy (for xTB: tighter SCF convergence) for the final geometry.
    Tight,
}

/// Why a potential could not produce an energy and gradient.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvalError {
    /// The SCF did not converge.
    Scf(String),
    /// The caller cancelled the evaluation (for example by stopping its worker).
    Cancelled,
    /// Any other failure, such as a crashed worker.
    Failed(String),
}

/// An energy surface in atomic units.
///
/// Positions are Cartesian coordinates in bohr, `[x0, y0, z0, x1, ...]`, in atom
/// order. Implementations return the energy in hartree and write the gradient in
/// hartree/bohr, same layout, into `gradient` (already sized `positions.len()`).
///
/// With no fixed atoms, the energy must be invariant under rigid translations
/// and rotations because the optimizer projects out those motions. For a surface
/// that breaks this invariance, such as an external restraint, set at least one
/// index in [`Options::fixed_atoms`].
///
/// A potential may keep state between calls (xTB restarts its SCF from the
/// previous wavefunction); the optimizer may evaluate trial geometries that it
/// later rejects, so that state must only be a starting guess.
pub trait Potential {
    /// Evaluate the energy and gradient at `positions`.
    fn evaluate(
        &mut self,
        positions: &[f64],
        gradient: &mut [f64],
        accuracy: Accuracy,
    ) -> Result<f64, EvalError>;

    /// Write the Cartesian Hessian in hartree/bohr² at `positions` into the sized,
    /// row-major 3N × 3N buffer, in the same atom order. Return `None` if unavailable.
    fn hessian(
        &mut self,
        _positions: &[f64],
        _hessian: &mut [f64],
    ) -> Option<Result<(), EvalError>> {
        None
    }
}
