//! Quantum chemistry data, canonical orbital conventions, and storage formats.

#![forbid(unsafe_code)]

mod basis;
mod capabilities;
/// Serde codecs for compact floating-point arrays.
pub mod compact;
mod frame;
mod molden;
mod result;
/// Thermochemistry, IR spectra, and normal-mode geometries.
pub mod vibrational;

pub use basis::{
    BasisSet, Beta, OrbitalSet, Orbitals, Shell, Spin, canonical_cartesian_order,
    contraction_overlap, primitive_norm,
};
pub use capabilities::{
    Capabilities, OrbitalRequest, SolventSupport, XtbMethod, check_calculation,
};
pub use frame::{Frame, FrameHash, Trajectory};
pub use molden::{
    CartesianNorm, axial_norm_ratio, mo_from_molden, mo_to_molden, molden_cartesian_order,
    molden_pure_order, shell_from_molden, shell_to_molden,
};
pub use result::{
    AtomicCharges, BondOrder, CalcResult, ChargeScheme, Convergence, Endpoints, GfnFfTopology,
    MappedAtom, Provenance, RunSummary, ScfRetry, SearchSummary, Settings, SolvationState, Solvent,
    SolventCorrection, SolventModel, Source, Status, Task, Thermo, TransitionStateContext,
    TransitionStateMapping, TransitionStateRun, Vibrations, XtbConventions, XtbSettings,
};

/// Ångströms per bohr, CODATA 2022.
pub const ANGSTROM_PER_BOHR: f64 = 0.529177210544;
/// Debye per elementary charge times bohr, CODATA 2022.
pub const DEBYE_PER_E_BOHR: f64 = 2.541746473;
/// Atom limit for xTB energies, gradients, and optimizations.
pub const MAX_CALC_ATOMS: usize = 500;
/// Atom limit for Hessians, vibrational analysis, and xTB conformer re-ranking.
pub const MAX_HESSIAN_ATOMS: usize = 200;
/// Atom limit for transition-state searches.
pub const MAX_TS_ATOMS: usize = 120;
/// Maximum number of conformers retained in a conformer result.
pub const MAX_SAVED_CONFORMERS: usize = 32;
/// Step limit for trajectories and frames read from a single file.
pub const MAX_FRAMES: usize = 1_000;
/// Atom limit for a stored scientific frame.
pub const MAX_FRAME_ATOMS: usize = 10_000;
/// Atom limit summed over all trajectory steps.
pub const MAX_TRAJECTORY_ATOMS: usize = 1_000_000;
/// Atomic orbital count limit for a basis.
pub const MAX_BASIS_FUNCTIONS: usize = 10_000;
/// Molecular orbital coefficient count limit per spin channel.
pub const MAX_COEFFICIENTS: usize = 4_194_304;
/// Primitive count limit per contracted shell.
pub const MAX_PRIMITIVES: usize = 64;
/// Angular momentum quantum number limit.
pub const MAX_L: u8 = 5;
/// Energy evaluations a saved run summary may list.
pub const MAX_RUN_EVALUATIONS: usize = 100_000;
/// Value count limit for one compact array.
pub const MAX_COMPACT_VALUES: usize = 4_194_304;
/// Decoded bytes allowed per encoded byte in a compact array.
pub const MAX_COMPACT_EXPANSION: usize = 32;
/// Encoded heavy bytes allowed per molecule by default for imported results.
pub const MAX_IMPORTED_HEAVY_BYTES: usize = 2_000_000;
/// Minimum dimensionless bond order retained in a light result.
pub const LIGHT_BOND_ORDER_MIN: f64 = 0.1;

/// Validation or conversion failure.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A count exceeded its format or calculation limit.
    #[error("too many {what}: {count} exceeds {max}")]
    TooLarge {
        /// The quantity being counted.
        what: &'static str,
        /// The actual count.
        count: usize,
        /// The maximum allowed count.
        max: usize,
    },
    /// Data or settings violate the format.
    #[error("{0}")]
    Invalid(String),
    /// The method or format cannot represent the request.
    #[error("{0}")]
    Unsupported(String),
}
