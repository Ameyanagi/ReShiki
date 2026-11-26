//! Calculation results, settings, provenance, and the saved (light) form.
//!
//! Full references: docs/references/xtb.md.

use std::{
    borrow::Cow,
    collections::{BTreeMap, HashSet},
    mem::{size_of, size_of_val},
};

use serde::{Deserialize, Serialize};

use crate::{
    Beta, Error, Frame, FrameHash, LIGHT_BOND_ORDER_MIN, MAX_COMPACT_VALUES, MAX_HESSIAN_ATOMS,
    MAX_RUN_EVALUATIONS, MAX_SAVED_CONFORMERS, OrbitalRequest, OrbitalSet, Orbitals, Spin,
    Trajectory, XtbMethod, check_calculation, compact::Q16_SCALE,
};

/// One calculation or imported job step tied to an exact frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalcResult {
    /// Geometry the values describe.
    pub frame: FrameHash,
    /// Starting geometry when it differs from the result geometry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_frame: Option<FrameHash>,
    /// Requested calculation task.
    pub task: Task,
    /// Optimized reactant and product for a transition-state search.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoints: Option<Endpoints>,
    /// Settings or imported job keywords.
    pub settings: Settings,
    /// Producer and source information.
    pub provenance: Provenance,
    /// Outcome of the job.
    pub status: Status,
    /// Total energy in hartree, when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub energy_hartree: Option<f64>,
    /// Named energy components in hartree.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub energy_terms: BTreeMap<String, f64>,
    /// Atom-major energy derivatives in hartree per bohr.
    #[serde(
        with = "crate::compact::f32",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub gradient_hartree_per_bohr: Vec<f64>,
    /// Row-major Cartesian Hessian in hartree per bohr squared, with atom-major axes.
    #[serde(
        with = "crate::compact::f32",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub hessian_hartree_per_bohr2: Vec<f64>,
    /// Available sets of per-atom charges in elementary charge units.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub charges: Vec<AtomicCharges>,
    /// Dipole xyz in elementary charge times bohr, about the frame origin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dipole_e_bohr: Option<[f64; 3]>,
    /// Sparse Wiberg–Mayer bond orders.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bond_orders: Vec<BondOrder>,
    /// Molecular orbital data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orbitals: Option<Orbitals>,
    /// Harmonic vibrational analysis.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vibrations: Option<Vibrations>,
    /// Thermochemical corrections and their analysis settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thermo: Option<Thermo>,
    /// Optimization path, ranked conformers, or transition-state path images.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trajectory: Option<Trajectory>,
    /// User choice to keep full results in the saved file.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub keep_full: bool,
    /// Total encoded bytes of heavy imported data omitted from the saved result.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub omitted_bytes: Option<usize>,
    /// Run metadata for a calculated result.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<RunSummary>,
}

/// Wall time, SCF effort, and optional search details for one run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunSummary {
    /// Wall-clock seconds from the start of the run to the result.
    pub seconds: f64,
    /// SCF iterations of each energy evaluation, in order; zero for methods without an SCF.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scf_iterations: Vec<u32>,
    /// When the run finished: RFC 3339, UTC, whole seconds (e.g. "2026-11-26T05:03:00Z").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished: Option<String>,
    /// Counts from a conformer search.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search: Option<SearchSummary>,
    /// Transition-state search details, retained with the light result.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition_state: Option<Box<TransitionStateRun>>,
}

/// Interpretation and audit details of one transition-state search.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransitionStateRun {
    /// Drawing arrow, if it still belongs to the document containing this result.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arrow: Option<u64>,
    /// Chemistry and export labels captured when the search was requested.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<TransitionStateContext>,
    /// Whether endpoint thermochemistry was requested for this search.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub free_energy: bool,
    /// Electronic energies of the search's optimized [reactant, product], in hartree.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint_energies_hartree: Option<[f64; 2]>,
    /// Corrections for those same endpoints, in [reactant, product] order.
    /// A missing slot means that endpoint's frequency analysis failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint_thermo: Option<[Option<Thermo>; 2]>,
    pub saddle_converged: bool,
    pub imaginary: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imaginary_cm1: Option<f64>,
    pub climbing_image: usize,
    pub mapping: TransitionStateMapping,
    pub diagnostics: Vec<(String, String)>,
}

/// Saved reaction identity, independent of live drawing labels and atom IDs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionStateContext {
    /// SHA-256 digest of the reactant's Calculate chemistry baseline, as lowercase hex.
    pub reactant_chemistry: String,
    /// SHA-256 digest of the product's Calculate chemistry baseline, as lowercase hex.
    pub product_chemistry: String,
    /// Reaction title used by input export.
    pub title: String,
    /// First reactant label used by exported file names.
    pub reactant: String,
    /// Whether one endpoint represents multiple species optimized together.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint_complex: Option<bool>,
}

/// Atom labels are captured at run start, in the direction of the reaction arrow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionStateMapping {
    pub formed: Vec<[MappedAtom; 2]>,
    pub broken: Vec<[MappedAtom; 2]>,
    pub hydrogen_moves: Vec<[MappedAtom; 2]>,
    pub automatic: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MappedAtom {
    pub element: String,
    pub number: u32,
}

impl TransitionStateRun {
    /// Optimized product minus reactant electronic energy, independent of the saddle check.
    pub fn reaction_energy_hartree(&self) -> Option<f64> {
        let [reactant, product] = self.endpoint_energies_hartree?;
        let delta = product - reactant;
        delta.is_finite().then_some(delta)
    }

    /// Optimized product minus reactant Gibbs energy at the saved analysis conditions.
    pub fn reaction_gibbs_hartree(&self) -> Option<f64> {
        let [reactant, product] = self.endpoint_energies_hartree?;
        let [reactant_thermo, product_thermo] = self.endpoint_thermo.as_ref()?;
        let (reactant_thermo, product_thermo) =
            (reactant_thermo.as_ref()?, product_thermo.as_ref()?);
        let delta = (product + product_thermo.gibbs_correction_hartree)
            - (reactant + reactant_thermo.gibbs_correction_hartree);
        delta.is_finite().then_some(delta)
    }

    fn validate(&self, result: &CalcResult, atoms: usize) -> Result<(), Error> {
        let valid_pairs = |pairs: &[[MappedAtom; 2]]| {
            pairs.len() <= atoms * atoms
                && pairs.iter().flatten().all(|atom| {
                    atom.number != 0
                        && (1..=3).contains(&atom.element.len())
                        && atom.element.bytes().all(|byte| byte.is_ascii_alphabetic())
                })
        };
        if result.task != Task::TransitionState
            || self.arrow == Some(0)
            || self.endpoint_energies_hartree.is_some_and(|energies| {
                result.endpoints.is_none() || energies.iter().any(|energy| !energy.is_finite())
            })
            || self.endpoint_thermo.as_ref().is_some_and(|corrections| {
                !self.free_energy
                    || self.endpoint_energies_hartree.is_none()
                    || corrections.iter().flatten().any(|thermo| {
                        (thermo.temperature_k - 298.15).abs() > 1e-6
                            || (thermo.pressure_pa - 101_325.).abs() > 1e-3
                            || (thermo.frequency_scale - 1.).abs() > 1e-12
                            || (thermo.rotor_cutoff_cm1 - 50.).abs() > 1e-12
                            || (thermo.imaginary_cutoff_cm1 - 20.).abs() > 1e-12
                            || thermo.symmetry_number == 0
                    })
            })
            || self.context.as_ref().is_some_and(|context| {
                [&context.reactant_chemistry, &context.product_chemistry]
                    .into_iter()
                    .any(|digest| {
                        digest.len() != 64
                            || !digest
                                .bytes()
                                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                    })
                    || context.title.len() > 256 * 1024
                    || context.reactant.len() > 256 * 1024
            })
            || self.imaginary > 3 * atoms
            || self
                .imaginary_cm1
                .is_some_and(|value| !value.is_finite() || value >= 0.)
            || result
                .trajectory
                .as_ref()
                .is_none_or(|path| self.climbing_image >= path.energies_hartree.len())
            || !valid_pairs(&self.mapping.formed)
            || !valid_pairs(&self.mapping.broken)
            || !valid_pairs(&self.mapping.hydrogen_moves)
            || self.diagnostics.len() > 64
            || self
                .diagnostics
                .iter()
                .any(|(label, value)| label.len() > 128 || value.len() > 8192)
        {
            return Err(Error::Invalid(
                "invalid transition-state run details".into(),
            ));
        }
        Ok(())
    }

    fn heap_bytes(&self) -> usize {
        let pairs = |pairs: &[[MappedAtom; 2]]| {
            size_of_val(pairs)
                + pairs
                    .iter()
                    .flatten()
                    .map(|atom| atom.element.len())
                    .sum::<usize>()
        };
        size_of::<Self>()
            + self.endpoint_thermo.as_ref().map_or(0, |corrections| {
                corrections
                    .iter()
                    .flatten()
                    .map(|thermo| thermo.point_group.as_ref().map_or(0, String::len))
                    .sum::<usize>()
            })
            + self.context.as_ref().map_or(0, |context| {
                context.reactant_chemistry.len()
                    + context.product_chemistry.len()
                    + context.title.len()
                    + context.reactant.len()
            })
            + pairs(&self.mapping.formed)
            + pairs(&self.mapping.broken)
            + pairs(&self.mapping.hydrogen_moves)
            + size_of_val(self.diagnostics.as_slice())
            + self
                .diagnostics
                .iter()
                .map(|(label, value)| label.len() + value.len())
                .sum::<usize>()
    }
}

/// Counts at each stage of a conformer search.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchSummary {
    pub requested: usize,
    pub made: usize,
    /// Starts passed to clustering after minimization.
    pub minimized: usize,
    /// If no force-field start converged, clustering used the unconverged starts.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub used_unconverged_starts: bool,
    pub distinct: usize,
    pub clusters: usize,
    /// Representatives planned for xTB re-ranking. Zero when xTB minimizes every start.
    pub rerank_planned: usize,
    /// Representatives that converged in xTB re-ranking.
    pub reranked: usize,
    pub kept: usize,
}

/// Calculation task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Task {
    /// Evaluate the total energy.
    Energy,
    /// Evaluate energy derivatives.
    Gradient,
    /// Optimize the geometry.
    Optimize,
    /// Evaluate second energy derivatives.
    Hessian,
    /// Search for and rank conformers.
    Conformers,
    /// Search for a transition state between two geometries.
    TransitionState,
}

/// Optimized endpoints of a transition-state search.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Endpoints {
    /// Geometry of the reactant.
    pub reactant: FrameHash,
    /// Geometry of the product.
    pub product: FrameHash,
}

/// Calculation or import outcome.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// The calculation converged.
    Converged,
    /// The calculation ended without convergence.
    NotConverged {
        /// Producer's explanation.
        reason: String,
    },
    /// The calculation failed.
    Failed {
        /// Producer's explanation.
        reason: String,
    },
}

/// Charges assigned by one population analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AtomicCharges {
    /// Charge assignment scheme.
    pub scheme: ChargeScheme,
    /// One charge per atom, in elementary charge units.
    pub values: Vec<f64>,
}

/// Population analysis scheme for atomic charges.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChargeScheme {
    /// Mulliken populations \[Mulliken1955Population\].
    Mulliken,
    /// Electronegativity equilibration, as in GFN-FF \[Spicher2020GFNFF\].
    Eeq,
    /// Charge Model 5 \[Marenich2012CM5\].
    Cm5,
    /// Hirshfeld partitioning \[Hirshfeld1977Charges\].
    Hirshfeld,
    /// Löwdin populations \[Lowdin1950Orthogonality\].
    Loewdin,
    /// A producer-supplied scheme name.
    Other(String),
}

/// Wiberg–Mayer bond order for one unordered atom pair \[Wiberg1968BondOrder\]
/// \[Mayer1983BondOrder\].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BondOrder {
    /// Lower zero-based atom index.
    pub a: u32,
    /// Higher zero-based atom index.
    pub b: u32,
    /// Dimensionless bond order.
    pub order: f64,
}

/// Harmonic vibrational frequencies and optional mode data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Vibrations {
    /// Frequencies in inverse centimetres, ascending, with imaginary modes negative.
    pub frequencies_cm1: Vec<f64>,
    /// Infrared intensities in kilometres per mole, one per frequency if available.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ir_intensities_km_mol: Vec<f64>,
    /// Cartesian displacement vectors, mode-major and atom-major within each mode.
    /// Modes are unit norm as produced. Renormalize each mode before use.
    #[serde(
        with = "crate::compact::q16",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub modes: Vec<f64>,
    /// Reduced masses in unified atomic mass units, one per frequency if available.
    #[serde(
        with = "crate::compact::f32",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub reduced_masses_amu: Vec<f64>,
    /// Analysis masses in unified atomic mass units, one per atom if available.
    #[serde(
        with = "crate::compact::f32",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub masses_amu: Vec<f64>,
}

impl Vibrations {
    /// Count all strictly negative frequencies.
    pub fn imaginary_count(&self) -> usize {
        self.frequencies_cm1
            .iter()
            .filter(|&&frequency| frequency < 0.0)
            .count()
    }
}

/// Thermochemical corrections relative to the reported electronic energy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Thermo {
    /// Analysis temperature in kelvin.
    pub temperature_k: f64,
    /// Analysis pressure in pascals.
    pub pressure_pa: f64,
    /// Zero-point energy in hartree.
    pub zero_point_energy_hartree: f64,
    /// Enthalpy minus electronic energy, including zero-point energy, in hartree.
    pub enthalpy_correction_hartree: f64,
    /// Entropy in hartree per kelvin.
    pub entropy_hartree_per_k: f64,
    /// Gibbs free energy minus electronic energy, in hartree.
    pub gibbs_correction_hartree: f64,
    /// Heat capacity in hartree per kelvin, if available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heat_capacity_hartree_per_k: Option<f64>,
    /// Dimensionless frequency scale factor.
    pub frequency_scale: f64,
    /// Rotor cutoff in inverse centimetres.
    pub rotor_cutoff_cm1: f64,
    /// Imaginary frequency cutoff in inverse centimetres.
    pub imaginary_cutoff_cm1: f64,
    /// Rotational symmetry number.
    pub symmetry_number: u32,
    /// Raw Schoenflies point-group label reported by the engine, if available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub point_group: Option<String>,
}

/// Settings for a reproducible xTB calculation or an imported job.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Settings {
    /// xTB settings.
    Xtb(XtbSettings),
    /// Settings described by the source file.
    Imported {
        /// Keywords reported by the source.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        keywords: Vec<String>,
    },
}

/// xTB calculation options, with omitted overrides using engine defaults.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct XtbSettings {
    /// Engine method.
    pub method: XtbMethod,
    /// Numerical conventions selected in the engine.
    #[serde(default, skip_serializing_if = "XtbConventions::is_default")]
    pub conventions: XtbConventions,
    /// Optional implicit solvent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub solvent: Option<Solvent>,
    /// Electronic temperature override in kelvin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub electronic_temperature_k: Option<f64>,
    /// Dimensionless SCF accuracy override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accuracy: Option<f64>,
    /// SCF iteration count override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_iterations: Option<u32>,
    /// SCF settings to try in order after a failed attempt.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scf_retries: Vec<ScfRetry>,
    /// Required topology reference for GFN-FF.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gfnff: Option<GfnFfTopology>,
    /// Worker threads the calculation used; `None` in results saved before it was recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threads: Option<u32>,
    /// Optimizer convergence level of an optimization; `None` when not recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub convergence: Option<Convergence>,
    /// Zero-based frame atoms the optimizer held fixed, ascending.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fixed_atoms: Vec<u32>,
}

/// Optimizer convergence level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Convergence {
    /// Normal convergence criteria.
    Normal,
    /// Tight convergence criteria.
    Tight,
}

/// Numerical conventions for xTB calculations.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum XtbConventions {
    /// Axtb's native conventions.
    #[default]
    Axtb,
    /// Reference xTB conventions.
    #[serde(rename = "xtb-6.8")]
    Xtb68,
}

impl XtbConventions {
    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// Overrides for one SCF retry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScfRetry {
    /// Electronic temperature in kelvin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub electronic_temperature_k: Option<f64>,
    /// SCF iteration limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_iterations: Option<u32>,
}

/// Implicit solvent settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Solvent {
    /// Implicit solvent model.
    pub model: SolventModel,
    /// Engine solvent name.
    pub name: String,
    /// Solvation reference state.
    #[serde(default)]
    pub state: SolvationState,
    /// Optional composite energy correction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correction: Option<SolventCorrection>,
}

/// Implicit solvent model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SolventModel {
    /// Analytical linearized Poisson–Boltzmann model (Ehlert et al. 2021)
    /// \[Ehlert2021ALPB\].
    Alpb,
    /// Generalized Born surface area model (Ehlert et al. 2021) \[Ehlert2021ALPB\].
    Gbsa,
}

/// Solvation reference state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SolvationState {
    /// Default solvation free energy state.
    #[default]
    Gsolv,
    /// One bar to one mole per litre standard state.
    Bar1mol,
    /// Engine reference state.
    Reference,
}

/// Composite solvent energy correction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SolventCorrection {
    /// Add the GFN2-xTB energy difference between solvated and gas-phase calculations.
    Gfn2,
}

/// Reference geometry and fragment charges that pin a GFN-FF topology.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GfnFfTopology {
    /// Exact frame used to establish the topology.
    pub reference: FrameHash,
    /// Charges in elementary charge units, in first-atom fragment order.
    /// An empty list assigns the total charge to the first fragment.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fragment_charges: Vec<i32>,
}

/// Producer metadata and origin of a result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Provenance {
    /// Program that produced the data.
    pub program: String,
    /// Version reported by the producer, if available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program_version: Option<String>,
    /// Method name reported by the producer, if available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    /// Basis name reported by the producer, if available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub basis: Option<String>,
    /// Calculation revision or source file.
    pub source: Source,
}

/// Origin of the result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// Result from a calculation.
    Calculated {
        /// Engine revision pinning method defaults.
        engine_revision: String,
    },
    /// Result from a file.
    Imported {
        /// Format of the source file.
        format: String,
        /// Source file name.
        file_name: String,
        /// SHA-256 digest of the source file, if available.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        file_sha256: Option<String>,
        /// Job or step index in the source file.
        job: u32,
    },
}

impl CalcResult {
    /// Check the frame identity, physical arrays, settings, and size limits.
    pub fn validate(&self, frame: &Frame) -> Result<(), Error> {
        if let Settings::Xtb(settings) = &self.settings {
            if !matches!(&self.provenance.source, Source::Calculated { engine_revision } if !engine_revision.is_empty())
            {
                return Err(Error::Invalid(
                    "xTB settings require calculated provenance with an engine revision".into(),
                ));
            }
            let request = if self.orbitals.is_some() {
                OrbitalRequest::Levels
            } else {
                OrbitalRequest::None
            };
            check_calculation(self.task, settings, frame, request)?;
            if !settings.method.capabilities().bond_orders && !self.bond_orders.is_empty() {
                return Err(Error::Unsupported(
                    "GFN-FF does not provide bond orders".into(),
                ));
            }
        } else {
            frame.validate()?;
        }
        if self.frame != frame.hash() {
            return Err(Error::Invalid(
                "result frame hash does not match geometry".into(),
            ));
        }
        if self.endpoints.is_some() != (self.task == Task::TransitionState) {
            return Err(Error::Invalid(
                "endpoints are required exactly for transition-state results".into(),
            ));
        }
        if let Some(run) = &self.run {
            if !run.seconds.is_finite() || run.seconds < 0.0 {
                return Err(Error::Invalid(
                    "run time must be finite and nonnegative".into(),
                ));
            }
            if run.scf_iterations.len() > MAX_RUN_EVALUATIONS {
                return Err(Error::TooLarge {
                    what: "SCF iteration counts",
                    count: run.scf_iterations.len(),
                    max: MAX_RUN_EVALUATIONS,
                });
            }
            if let Some(ts) = &run.transition_state {
                ts.validate(self, frame.numbers.len())?;
            }
        }
        let n_atoms = frame.numbers.len();
        let axes = n_atoms * 3;
        if (self.task == Task::Hessian
            || !self.hessian_hartree_per_bohr2.is_empty()
            || self.vibrations.is_some())
            && n_atoms > MAX_HESSIAN_ATOMS
        {
            return Err(Error::TooLarge {
                what: "Hessian or vibrational analysis atoms",
                count: n_atoms,
                max: MAX_HESSIAN_ATOMS,
            });
        }
        for (what, values, expected) in [
            ("gradient", self.gradient_hartree_per_bohr.as_slice(), axes),
            (
                "Hessian",
                self.hessian_hartree_per_bohr2.as_slice(),
                axes * axes,
            ),
        ] {
            if values.len() > MAX_COMPACT_VALUES {
                return Err(Error::TooLarge {
                    what,
                    count: values.len(),
                    max: MAX_COMPACT_VALUES,
                });
            }
            if !values.is_empty()
                && (values.len() != expected || values.iter().any(|&v| !(v as f32).is_finite()))
            {
                return Err(Error::Invalid(format!("invalid {what} length or values")));
            }
        }
        if self.energy_hartree.is_some_and(|e| !e.is_finite())
            || self.energy_terms.values().any(|v| !v.is_finite())
            || self.dipole_e_bohr.iter().flatten().any(|v| !v.is_finite())
        {
            return Err(Error::Invalid("non-finite energy or dipole".into()));
        }
        for charges in &self.charges {
            if charges.values.len() != n_atoms || charges.values.iter().any(|v| !v.is_finite()) {
                return Err(Error::Invalid(
                    "invalid atomic charge count or values".into(),
                ));
            }
        }
        let mut pairs = HashSet::new();
        for bond in &self.bond_orders {
            if bond.a >= bond.b || bond.b as usize >= n_atoms || !bond.order.is_finite() {
                return Err(Error::Invalid("invalid bond order or atom indices".into()));
            }
            if !pairs.insert((bond.a, bond.b)) {
                return Err(Error::Invalid("duplicate bond order pair".into()));
            }
        }
        if let Some(orbitals) = &self.orbitals {
            orbitals.validate(frame)?;
        }
        if let Some(v) = &self.vibrations {
            let modes = v.frequencies_cm1.len();
            if modes > axes.saturating_sub(5)
                || v.frequencies_cm1.iter().any(|v| !v.is_finite())
                || v.frequencies_cm1.windows(2).any(|w| w[0] > w[1])
            {
                return Err(Error::Invalid("invalid vibrational frequencies".into()));
            }
            for (values, expected) in [
                (v.ir_intensities_km_mol.as_slice(), modes),
                (v.modes.as_slice(), modes * axes),
                (v.reduced_masses_amu.as_slice(), modes),
                (v.masses_amu.as_slice(), n_atoms),
            ] {
                if !values.is_empty() && values.len() != expected {
                    return Err(Error::Invalid("invalid vibrational array length".into()));
                }
            }
            if v.ir_intensities_km_mol.iter().any(|v| !v.is_finite())
                || v.reduced_masses_amu.iter().chain(&v.masses_amu).any(|&v| {
                    let mass = v as f32;
                    !mass.is_finite() || mass <= 0.0
                })
                || v.modes.iter().any(|v| !(-1.0..=1.0).contains(v))
            {
                return Err(Error::Invalid("invalid vibrational array values".into()));
            }
            if !v.modes.is_empty()
                && v.modes
                    .chunks_exact(axes)
                    .any(|mode| !mode.iter().any(|x| (x.abs() * Q16_SCALE).round() >= 1.0))
            {
                return Err(Error::Invalid(
                    "vibrational mode is zero at q16 precision".into(),
                ));
            }
        }
        for t in self.thermo.iter().chain(
            self.run
                .as_ref()
                .and_then(|run| run.transition_state.as_ref())
                .and_then(|search| search.endpoint_thermo.as_ref())
                .into_iter()
                .flatten()
                .flatten(),
        ) {
            if [
                t.temperature_k,
                t.pressure_pa,
                t.zero_point_energy_hartree,
                t.enthalpy_correction_hartree,
                t.entropy_hartree_per_k,
                t.gibbs_correction_hartree,
                t.frequency_scale,
                t.rotor_cutoff_cm1,
                t.imaginary_cutoff_cm1,
            ]
            .into_iter()
            .any(|v| !v.is_finite())
                || t.heat_capacity_hartree_per_k
                    .is_some_and(|v| !v.is_finite())
            {
                return Err(Error::Invalid("non-finite thermochemistry value".into()));
            }
            let gibbs_correction =
                t.enthalpy_correction_hartree - t.temperature_k * t.entropy_hartree_per_k;
            if t.temperature_k <= 0.0
                || t.pressure_pa <= 0.0
                || (t.gibbs_correction_hartree - gibbs_correction).abs() > 1e-5
            {
                return Err(Error::Invalid(
                    "invalid thermochemistry temperature, pressure, or Gibbs correction".into(),
                ));
            }
        }
        if self.trajectory.is_some()
            && !matches!(
                self.task,
                Task::Optimize | Task::Conformers | Task::TransitionState
            )
        {
            return Err(Error::Invalid(
                "only optimization, conformer and transition-state results carry a trajectory"
                    .into(),
            ));
        }
        if let Some(path) = &self.trajectory {
            path.validate()?;
            let steps = path.steps();
            if self.task == Task::Conformers && steps > MAX_SAVED_CONFORMERS {
                return Err(Error::TooLarge {
                    what: "saved conformers",
                    count: steps,
                    max: MAX_SAVED_CONFORMERS,
                });
            }
            if path.numbers != frame.numbers
                || (0..n_atoms).any(|i| {
                    path.isotopes.get(i).copied().unwrap_or(0)
                        != frame.isotopes.get(i).copied().unwrap_or(0)
                })
                || path.masses_amu != frame.masses_amu
                || path.charge != frame.charge
                || path.multiplicity != frame.multiplicity
            {
                return Err(Error::Invalid(
                    "trajectory and result frame describe different atoms or states".into(),
                ));
            }
            let positions = &path.positions_angstrom;
            if !positions.is_empty() {
                let step = match self.task {
                    Task::Optimize => &positions[positions.len() - axes..],
                    Task::Conformers => &positions[..axes],
                    _ => &[],
                };
                if step
                    .iter()
                    .zip(frame.positions_angstrom.iter().flatten())
                    .any(|(&a, &b)| a as f32 != b as f32)
                {
                    return Err(Error::Invalid(
                        "trajectory positions differ from the result frame".into(),
                    ));
                }
            }
        }
        Ok(())
    }

    /// Copy the light data, retaining vibrational masses and the basis and requested
    /// coefficient rows if present. Cartesian vibrational modes are omitted.
    ///
    /// Expects a validated result and may panic otherwise.
    pub fn light(&self, pinned: &[(Spin, u32)]) -> Self {
        let orbitals = self.orbitals.as_ref().map(|orbitals| {
            let n_ao = orbitals.basis.as_ref().map_or(0, |basis| {
                basis.shells.iter().map(|shell| shell.n_functions()).sum()
            });
            let pins: HashSet<_> = pinned.iter().copied().collect();
            let light_set = |set: &OrbitalSet, spin, shared| {
                let mut light = OrbitalSet {
                    energies_hartree: set.energies_hartree.clone(),
                    occupations: set.occupations.clone(),
                    symmetries: set.symmetries.clone(),
                    rows: Vec::new(),
                    coefficients: Vec::new(),
                };
                for (i, &row) in set.rows.iter().enumerate() {
                    if pins.contains(&(spin, row)) || (shared && pins.contains(&(Spin::Beta, row)))
                    {
                        light.rows.push(row);
                        light
                            .coefficients
                            .extend_from_slice(&set.coefficients[i * n_ao..(i + 1) * n_ao]);
                    }
                }
                light
            };
            let shared = !matches!(orbitals.beta, Beta::Unrestricted(_));
            let alpha = light_set(&orbitals.alpha, Spin::Alpha, shared);
            let beta = match &orbitals.beta {
                Beta::Unrestricted(set) => Beta::Unrestricted(light_set(set, Spin::Beta, false)),
                shared => shared.clone(),
            };
            let has_rows = !alpha.rows.is_empty()
                || matches!(&beta, Beta::Unrestricted(set) if !set.rows.is_empty());
            Orbitals {
                basis: if has_rows {
                    orbitals.basis.clone()
                } else {
                    None
                },
                core_electrons: orbitals.core_electrons.clone(),
                alpha,
                beta,
            }
        });
        Self {
            frame: self.frame,
            input_frame: self.input_frame,
            task: self.task,
            endpoints: self.endpoints,
            settings: self.settings.clone(),
            provenance: self.provenance.clone(),
            status: self.status.clone(),
            energy_hartree: self.energy_hartree,
            energy_terms: self.energy_terms.clone(),
            gradient_hartree_per_bohr: Vec::new(),
            hessian_hartree_per_bohr2: Vec::new(),
            charges: self.charges.clone(),
            dipole_e_bohr: self.dipole_e_bohr,
            bond_orders: self
                .bond_orders
                .iter()
                .filter(|b| b.order >= LIGHT_BOND_ORDER_MIN)
                .cloned()
                .collect(),
            orbitals,
            vibrations: self.vibrations.as_ref().map(|v| Vibrations {
                frequencies_cm1: v.frequencies_cm1.clone(),
                ir_intensities_km_mol: v.ir_intensities_km_mol.clone(),
                modes: Vec::new(),
                reduced_masses_amu: v.reduced_masses_amu.clone(),
                masses_amu: v.masses_amu.clone(),
            }),
            thermo: self.thermo.clone(),
            trajectory: self.trajectory.as_ref().map(|path| Trajectory {
                numbers: path.numbers.clone(),
                isotopes: path.isotopes.clone(),
                masses_amu: path.masses_amu.clone(),
                charge: path.charge,
                multiplicity: path.multiplicity,
                positions_angstrom: if self.task == Task::Conformers {
                    path.positions_angstrom.clone()
                } else {
                    Vec::new()
                },
                energies_hartree: path.energies_hartree.clone(),
                labels: path.labels.clone(),
                minimizer_energies_hartree: path.minimizer_energies_hartree.clone(),
                cluster_sizes: path.cluster_sizes.clone(),
                rmsd_to_best_angstrom: path.rmsd_to_best_angstrom.clone(),
            }),
            keep_full: self.keep_full,
            omitted_bytes: self.omitted_bytes,
            run: self.run.clone(),
        }
    }

    /// Select the saved form using the encoded imported-data budget in bytes.
    ///
    /// Expects a validated result and may panic otherwise.
    pub fn for_file(&self, heavy_budget: usize, pinned: &[(Spin, u32)]) -> Cow<'_, Self> {
        if self.keep_full {
            return Cow::Borrowed(self);
        }
        if matches!(self.settings, Settings::Xtb(_)) {
            return Cow::Owned(self.light(pinned));
        }
        let bytes = self.heavy_bytes();
        if bytes <= heavy_budget {
            Cow::Borrowed(self)
        } else {
            let mut light = self.light(pinned);
            let dropped_bytes = bytes.saturating_sub(light.heavy_bytes());
            light.omitted_bytes = Some(
                self.omitted_bytes
                    .unwrap_or(0)
                    .saturating_add(dropped_bytes),
            );
            // Dropping data must make the file smaller: pinned rows can
            // encode larger than the whole set (deflate's stored fallback),
            // and the counter itself takes room.
            let size = |result: &Self| {
                serde_json::to_vec_pretty(result)
                    .expect("saved result serializes to JSON")
                    .len()
            };
            if dropped_bytes == 0 || size(&light) >= size(self) {
                return Cow::Borrowed(self);
            }
            Cow::Owned(light)
        }
    }

    /// Measure the additional pretty-printed JSON bytes of the full result.
    ///
    /// Expects a validated result and may panic otherwise.
    pub fn heavy_bytes(&self) -> usize {
        let full = serde_json::to_vec_pretty(self)
            .expect("full result serializes to JSON")
            .len();
        let light = serde_json::to_vec_pretty(&self.light(&[]))
            .expect("light result serializes to JSON")
            .len();
        full.saturating_sub(light)
    }

    /// Measure the pretty-printed JSON for the saved form in bytes.
    ///
    /// Expects a validated result and may panic otherwise.
    pub fn saved_bytes(&self, heavy_budget: usize, pinned: &[(Spin, u32)]) -> usize {
        serde_json::to_vec_pretty(&self.for_file(heavy_budget, pinned))
            .expect("saved result serializes to JSON")
            .len()
    }

    /// Estimate the bytes owned by heap allocations in the result.
    pub fn heap_bytes(&self) -> usize {
        let mut bytes = size_of_val(self.gradient_hartree_per_bohr.as_slice())
            + size_of_val(self.hessian_hartree_per_bohr2.as_slice())
            + size_of_val(self.bond_orders.as_slice())
            + self
                .energy_terms
                .keys()
                .map(|k| size_of::<(String, f64)>() + k.len())
                .sum::<usize>()
            + size_of_val(self.charges.as_slice())
            + self
                .charges
                .iter()
                .map(|c| {
                    size_of_val(c.values.as_slice())
                        + match &c.scheme {
                            ChargeScheme::Other(name) => name.len(),
                            _ => 0,
                        }
                })
                .sum::<usize>();
        bytes += match &self.settings {
            Settings::Imported { keywords } => {
                size_of_val(keywords.as_slice()) + keywords.iter().map(String::len).sum::<usize>()
            }
            Settings::Xtb(settings) => {
                settings.solvent.as_ref().map_or(0, |s| s.name.len())
                    + size_of_val(settings.scf_retries.as_slice())
                    + size_of_val(settings.fixed_atoms.as_slice())
                    + settings
                        .gfnff
                        .as_ref()
                        .map_or(0, |g| size_of_val(g.fragment_charges.as_slice()))
            }
        };
        bytes += self.provenance.program.len()
            + [
                &self.provenance.program_version,
                &self.provenance.method,
                &self.provenance.basis,
            ]
            .into_iter()
            .flatten()
            .map(String::len)
            .sum::<usize>();
        bytes += match &self.provenance.source {
            Source::Calculated { engine_revision } => engine_revision.len(),
            Source::Imported {
                format,
                file_name,
                file_sha256,
                ..
            } => format.len() + file_name.len() + file_sha256.as_ref().map_or(0, String::len),
        };
        bytes += match &self.status {
            Status::Converged => 0,
            Status::NotConverged { reason } | Status::Failed { reason } => reason.len(),
        };
        if let Some(v) = &self.vibrations {
            bytes += size_of_val(v.frequencies_cm1.as_slice())
                + size_of_val(v.ir_intensities_km_mol.as_slice())
                + size_of_val(v.modes.as_slice())
                + size_of_val(v.reduced_masses_amu.as_slice())
                + size_of_val(v.masses_amu.as_slice());
        }
        if let Some(run) = &self.run {
            bytes += size_of_val(run.scf_iterations.as_slice())
                + run.finished.as_ref().map_or(0, String::len)
                + run
                    .transition_state
                    .as_deref()
                    .map_or(0, TransitionStateRun::heap_bytes);
        }
        bytes
            + self.orbitals.as_ref().map_or(0, Orbitals::heap_bytes)
            + self.trajectory.as_ref().map_or(0, Trajectory::heap_bytes)
            + self
                .thermo
                .as_ref()
                .and_then(|thermo| thermo.point_group.as_ref())
                .map_or(0, String::len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result() -> (Frame, CalcResult) {
        let frame = Frame {
            numbers: vec![8, 1, 1],
            positions_angstrom: vec![[0.0; 3]; 3],
            isotopes: vec![],
            masses_amu: vec![],
            charge: 0,
            multiplicity: 1,
            drawing_atoms: vec![],
        };
        let result = CalcResult {
            frame: frame.hash(),
            input_frame: None,
            task: Task::Hessian,
            endpoints: None,
            settings: Settings::Imported { keywords: vec![] },
            provenance: Provenance {
                program: "test".into(),
                program_version: None,
                method: None,
                basis: None,
                source: Source::Imported {
                    format: "log".into(),
                    file_name: "test.log".into(),
                    file_sha256: None,
                    job: 0,
                },
            },
            status: Status::Converged,
            energy_hartree: None,
            energy_terms: BTreeMap::new(),
            gradient_hartree_per_bohr: vec![0.0; 9],
            hessian_hartree_per_bohr2: vec![0.0; 81],
            charges: vec![AtomicCharges {
                scheme: ChargeScheme::Mulliken,
                values: vec![-0.8, 0.4, 0.4],
            }],
            dipole_e_bohr: None,
            bond_orders: vec![BondOrder {
                a: 0,
                b: 1,
                order: 1.0,
            }],
            orbitals: None,
            vibrations: Some(Vibrations {
                frequencies_cm1: vec![-1e-9, 0.0, 1600.0],
                ir_intensities_km_mol: vec![0.0; 3],
                modes: [vec![1.0], vec![0.0; 8]].concat().repeat(3),
                reduced_masses_amu: vec![1.0; 3],
                masses_amu: vec![16.0, 1.0, 1.0],
            }),
            thermo: None,
            trajectory: None,
            keep_full: false,
            omitted_bytes: None,
            run: None,
        };
        (frame, result)
    }

    #[test]
    fn checks_frame_identity_and_result_array_lengths() {
        let (frame, valid) = result();
        valid.validate(&frame).unwrap();
        let mut changed = frame.clone();
        changed.positions_angstrom[0][0] = 0.01;
        assert!(valid.validate(&changed).is_err());

        let mut result = valid.clone();
        result.gradient_hartree_per_bohr.pop();
        assert!(result.validate(&frame).is_err());
        result = valid.clone();
        result.hessian_hartree_per_bohr2.pop();
        assert!(result.validate(&frame).is_err());
        result = valid;
        result.charges[0].values.pop();
        assert!(result.validate(&frame).is_err());
    }

    #[test]
    fn checks_sparse_bond_orders() {
        let (frame, valid) = result();
        for (a, b) in [(0, 3), (1, 1), (2, 1)] {
            let mut result = valid.clone();
            result.bond_orders[0].a = a;
            result.bond_orders[0].b = b;
            assert!(result.validate(&frame).is_err());
        }
        let mut result = valid;
        result.bond_orders.push(result.bond_orders[0].clone());
        assert!(result.validate(&frame).is_err());
    }

    #[test]
    fn checks_vibration_lengths_and_mode_range() {
        let (frame, valid) = result();
        assert_eq!(valid.vibrations.as_ref().unwrap().imaginary_count(), 1);
        let arrays: [fn(&mut Vibrations) -> &mut Vec<f64>; 4] = [
            |v: &mut Vibrations| -> &mut Vec<f64> { &mut v.ir_intensities_km_mol },
            |v: &mut Vibrations| -> &mut Vec<f64> { &mut v.modes },
            |v: &mut Vibrations| -> &mut Vec<f64> { &mut v.reduced_masses_amu },
            |v: &mut Vibrations| -> &mut Vec<f64> { &mut v.masses_amu },
        ];
        for select in arrays {
            let mut result = valid.clone();
            select(result.vibrations.as_mut().unwrap()).pop();
            assert!(result.validate(&frame).is_err());
        }
        let mut result = valid;
        result.vibrations.as_mut().unwrap().modes[0] = 1.0001;
        assert!(result.validate(&frame).is_err());
    }

    #[test]
    fn every_vibrational_mode_must_survive_q16_rounding() {
        let (frame, mut result) = result();
        for component in [0.0, 1e-6] {
            result.vibrations.as_mut().unwrap().modes[..9].fill(component);
            assert!(result.validate(&frame).is_err());
        }
        result.vibrations.as_mut().unwrap().modes[0] = -0.5 / Q16_SCALE;
        result.validate(&frame).unwrap();
        let json = serde_json::to_vec(&result).unwrap();
        let decoded: CalcResult = serde_json::from_slice(&json).unwrap();
        decoded.validate(&frame).unwrap();
    }

    #[test]
    fn vibrational_mode_at_old_norm_limit_round_trips() {
        let (mut frame, original) = result();
        frame.numbers = vec![1, 1];
        frame.positions_angstrom = vec![[0.0; 3], [0.74, 0.0, 0.0]];
        let a = 0.707_813_180_860_952_8;
        let result = CalcResult {
            frame: frame.hash(),
            gradient_hartree_per_bohr: vec![],
            hessian_hartree_per_bohr2: vec![],
            charges: vec![],
            vibrations: Some(Vibrations {
                frequencies_cm1: vec![1600.0],
                ir_intensities_km_mol: vec![],
                modes: vec![a, 0.0, 0.0, -a, 0.0, 0.0],
                reduced_masses_amu: vec![],
                masses_amu: vec![],
            }),
            ..original
        };
        result.validate(&frame).unwrap();
        let json = serde_json::to_vec(&result).unwrap();
        let decoded: CalcResult = serde_json::from_slice(&json).unwrap();
        decoded.validate(&frame).unwrap();
    }

    #[test]
    fn vibrational_mode_count_respects_cartesian_limit() {
        let (frame, mut result) = result();
        let vibrations = result.vibrations.as_mut().unwrap();
        vibrations.ir_intensities_km_mol.clear();
        vibrations.modes.clear();
        vibrations.reduced_masses_amu.clear();
        vibrations.frequencies_cm1.push(1700.0);
        result.validate(&frame).unwrap();
        result
            .vibrations
            .as_mut()
            .unwrap()
            .frequencies_cm1
            .push(1800.0);
        assert!(result.validate(&frame).is_err());
    }

    #[test]
    fn vibrational_masses_must_be_positive() {
        let (frame, valid) = result();
        for mass in [0.0, -1.0, f64::MIN_POSITIVE] {
            let mut result = valid.clone();
            result.vibrations.as_mut().unwrap().masses_amu[0] = mass;
            assert!(result.validate(&frame).is_err());

            let mut result = valid.clone();
            result.vibrations.as_mut().unwrap().reduced_masses_amu[0] = mass;
            assert!(result.validate(&frame).is_err());
        }
    }
}
