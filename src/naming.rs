//! Local deterministic name/structure conversion with native identity checks.
use crate::{
    chemistry,
    document::Document,
    editing,
    molecule_names::{self, IdentityRejection, MAX_NAMING_ATOMS, MAX_SMILES_BYTES, is_tetrahedral},
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

mod catalog;
pub use catalog::VERSION as ALIAS_CATALOG_VERSION;
pub use catalog::{Alias, AliasSource, aliases_for_smiles};

// The parsers run in bounded worker processes on the desktop, and in a
// dedicated Web Worker with its own bounded memory in the browser.
#[cfg(not(target_arch = "wasm32"))]
mod client;
mod local;
mod online;
pub use online::resolve_name_online;
pub mod openclatura;
mod reverse;
mod symbols;
/// Translate explicit one-letter peptide input into its three-letter title.
pub fn peptide_sequence(name: &str) -> Result<String, Error> {
    symbols::peptide_sequence(name)
}
#[cfg(test)]
mod s2n_harness;
pub mod worker;
#[cfg(not(target_arch = "wasm32"))]
pub use {
    local::{Cancel, resolve_name},
    reverse::{generate_name, generate_name_with_options},
};
#[cfg(target_arch = "wasm32")]
mod browser;
#[cfg(target_arch = "wasm32")]
pub use browser::{
    Cancel, generate_name, generate_name_with_options, resolve_name, set_worker, worker_loading,
};

/// Notation used for isotopically substituted structures.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IsotopeStyle {
    #[default]
    Iupac,
    Boughton,
}

/// Desktop workers do not download a naming module.
#[cfg(not(target_arch = "wasm32"))]
pub fn worker_loading() -> bool {
    false
}

/// Why a local name request produced no result. Display prints the message
/// unchanged; the variant tells callers how to summarize it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The name or structure is outside what local naming supports.
    Rejected(String),
    /// The name describes several disconnected pieces, such as a salt or a
    /// mixture returned by a source requiring a connected molecule.
    Disconnected(String),
    /// The bounded worker could not be started, supervised or understood.
    Worker(String),
    /// The request was cancelled before it finished.
    Cancelled(String),
}
impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rejected(message)
            | Self::Disconnected(message)
            | Self::Worker(message)
            | Self::Cancelled(message) => formatter.write_str(message),
        }
    }
}
impl std::error::Error for Error {}

/// Preference assessment made by the rules that produced a generated name.
/// This is independent of the later OPSIN structure reconstruction check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", content = "reason", rename_all = "snake_case")]
pub enum NameStatus {
    Preferred,
    NotPreferred(NameReason),
    Unverified(NameReason),
}
impl NameStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Preferred => "Preferred IUPAC name",
            Self::NotPreferred(_) => "Systematic name, not the preferred IUPAC name",
            Self::Unverified(_) => "Systematic name, preference not verified",
        }
    }

    pub fn detail(self) -> &'static str {
        match self {
            Self::Preferred => {
                "No rule on this name's path is known to depart from the preferred IUPAC name (IUPAC 2013)."
            }
            Self::NotPreferred(reason) | Self::Unverified(reason) => reason.detail(),
        }
    }
}

/// The naming rule family responsible for a preference assessment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NameReason {
    SulfonylPrefix,
    AminoPrefix,
    FunctionalClass,
    HydrideParent,
    UnretainedParent,
    DiamideLocants,
    FusionComponent,
    HigherOrderFusion,
    BenzoFusion,
    Multiplicative,
    AcylPrefix,
    RingAssembly,
    VonBaeyerFusionFallback,
    OpsinFusionLayout,
    BridgedFusedParent,
    UnauditedParent,
    SubstitutedRetainedParent,
    AtomOrder,
    AmineChalcogenide,
    PrincipalGroupTie,
    IsotopePlacement,
    BoughtonIsotope,
    RadicalIon,
    AnionParent,
    Phane,
    RelativeRingStereo,
    UnknownConfiguration,
    AxialAlternative,
}
impl NameReason {
    pub fn detail(self) -> &'static str {
        match self {
            Self::AxialAlternative => {
                "Uses Ra/Sa axial descriptors; M and P are preferred to Ra and Sa (P-93.5.7.1)."
            }
            Self::SulfonylPrefix => {
                "Uses a concatenated sulfonyl or sulfinyl prefix such as N-methyltosylamino or (methylsulfinyl)amino; preferred IUPAC names use acid-derived forms such as N,4-dimethylbenzene-1-sulfonamido or methanesulfinamido (P-65.3.2.2.2, P-66.1.1.4.3)."
            }
            Self::AminoPrefix => {
                "Uses a guanidino prefix substituted on its outer nitrogens or with an E/Z descriptor, such as 3-methylguanidino; preferred IUPAC names cite carbamimidoyl inside an amino prefix, as in (N-methylcarbamimidoyl)amino (P-66.4.1.2.1.3)."
            }
            Self::FunctionalClass => {
                "Uses a functional-class form such as a hydrazone, a chiral sulfoxide, a mixed chalcogen peroxide or a phosphane oxide with halide or alkoxy ligands; preferred IUPAC names use substitutive or acid-derived forms such as ethylidenehydrazine, (S)-(methanesulfinyl)ethane, [(phenylselanyl)tellanyl]benzene or dimethylphosphinic fluoride (P-68.3.1.2.2, P-93.3.4.1, P-63.3.2, P-68.3.2.3.1)."
            }
            Self::HydrideParent => {
                "Uses a substituted parent hydride such as chloro(methyl)azane, 1,2-dimethyldisulfane or ethoxy(methyl)sulfane; preferred IUPAC names use methylhypochlorous amide, (methyldisulfanyl)methane or [(methylsulfanyl)oxy]ethane (P-68.5.3, P-63.3.1, P-63.3.2)."
            }
            Self::UnretainedParent => {
                "Uses a substituted methanal or methanenitrile oxide, an N-substituted ethanediamide, or a formic acid, formyl halide or formamide substituted on carbon; preferred IUPAC names use N-substituted oxamide (P-66.1.1.1.2.1) or carbonic and carbamic acid derivatives such as carbonochloridic acid (P-65.1.8.1)."
            }
            Self::DiamideLocants => {
                "Uses N or N' locants on a di- or polyamide; the preferred IUPAC name cites the parent locant on each nitrogen, as in N1-methylbutanediamide (P-66.1.1.3.1.1)."
            }
            Self::FusionComponent => {
                "Uses isoxazole or isothiazole as a fusion parent component, as in cyclopenta[c]isoxazole or benzo[d]isothiazole; preferred IUPAC names use [1,2]oxazole and [1,2]thiazole, or a benzo name such as 1,2-benzothiazole (P-25.3.2.1.2, P-25.2.2.4)."
            }
            Self::HigherOrderFusion => {
                "Cites an isolated benzo ring as a separate higher-order component, as in [1,3]thiazolo[4',5':1,2]benzo[3,4-d][1,3]oxazole; preferred IUPAC names treat the benzo ring and an ortho-fused heteromonocycle as one benzoheterocycle component, as in [1,3]thiazolo[5,4-e][1,3]benzoxazole (P-25.3.5, P-25.2.2.4; FR-3.5)."
            }
            Self::BenzoFusion => {
                "Uses a fusion descriptor for a benzene ring ortho-fused to a heteromonocycle, as in benzo[b]oxepine; preferred IUPAC names cite the heteroatom locants of the whole bicycle before its benzo name, as in 1-benzoxepine, or the retained name, as in quinoline (P-25.2.2.4, P-25.2.1)."
            }
            Self::Multiplicative => {
                "Names identical senior parent units joined by a linking group substitutively; the preferred IUPAC name is multiplicative. The units include identical substituents and suffix groups at identical locants, while the central linking group need not be symmetric, as in 1,1'-oxydibenzene or 2,2'-sulfanediyldiacetic acid (P-15.3, P-51.1.5, P-51.3.1)."
            }
            Self::AcylPrefix => {
                "Uses an acyl prefix built from a substituent, such as (4-methylphenyl)carbonyl, cyclohexylcarbonothioyl, (dimethyloxophosphanyl) or (dimethyloxoarsanyl), or an oxophosphanyl prefix on a parent, as in (methyloxophosphanyl)methane; preferred IUPAC names use the acid-derived forms 4-methylbenzoyl, cyclohexanecarbothioyl, dimethylphosphinoyl, dimethylarsinoyl and dimethyl-lambda5-phosphanone (P-65.1.7.4.2, P-67.1.4.1.1.3, P-68.3.2.3.1)."
            }
            Self::RingAssembly => {
                "Uses a substitutive or ring-assembly name where the preferred name is not yet supported. This includes double-bond junctions, assemblies requiring skeletal-replacement or hydro prefixes, and linear phanes with at least four ring systems and seven skeletal nodes (P-28, P-31.2.3.3.5, P-51.3.2, P-52.2.5.1, P-52.2.7)."
            }
            Self::VonBaeyerFusionFallback => {
                "Uses a von Baeyer name, with skeletal replacement where needed, for a bridge-free ortho- or ortho- and peri-fused ring system with at least two rings of five or more members; preferred IUPAC names use fusion nomenclature, as in decahydronaphthalene rather than bicyclo[4.4.0]decane (P-52.2.4.4.2, P-31.1.4.2.4)."
            }
            Self::OpsinFusionLayout => {
                "Uses a fusion layout or citation order selected for OPSIN compatibility; its preferred IUPAC numbering or alphabetical citation order (FR-3.6 / P-25.3.4.2.3) has not been verified."
            }
            Self::BridgedFusedParent => {
                "The preferred parent choice for this bridged-fused system has not been verified."
            }
            Self::UnauditedParent => {
                "Porphyrin, corrin and gonane are retained parents without a PIN designation in P-10; preferred name selection for this path has not been verified."
            }
            Self::SubstitutedRetainedParent => {
                "Whether a substituted acetaldehyde keeps its retained name in the preferred IUPAC name has not been verified."
            }
            Self::AtomOrder => {
                "The name can depend on input atom order; its preferred numbering has not been verified."
            }
            Self::AmineChalcogenide => {
                "Amine oxide-type names (amine sulfide, selenide, telluride) have not been checked against the preferred IUPAC form."
            }
            Self::RelativeRingStereo => {
                "Uses cis/trans for an achiral cyclic compound; preferred IUPAC names use CIP r/s descriptors, while cis/trans is used in general nomenclature (P-93.5.1.1.2)."
            }
            Self::UnknownConfiguration => {
                "Omits a descriptor for unknown configuration; preferred IUPAC names indicate unknown configuration with Ξ or ξ (P-91.2.1.2.1)."
            }
            Self::PrincipalGroupTie => {
                "Equally senior principal groups, such as a chain and a ring carboxylic acid, were chosen by atom order; the preferred parent (P-44.1) has not been verified."
            }
            Self::IsotopePlacement => {
                "The isotope descriptor reconstructs the exact structure, but its preferred placement and numbering under P-82.2 and P-82.6 have not been verified."
            }
            Self::BoughtonIsotope => {
                "Uses Boughton/CAS isotope notation; IUPAC prefers parenthesised nuclide descriptors (P-80, P-82.2)."
            }
            Self::RadicalIon => {
                "Uses the descriptive ‘radical cation’ or ‘radical anion’ form (RC-85.1); preferred systematic radical-ion names use the cumulative ionic and radical suffixes of P-75 (including the cation suffixes in P-73)."
            }
            Self::AnionParent => {
                "The preferred parent, suffix and retained-name choice for this hydroxy- or acid-derived anion have not been verified (P-72.2.2.2.1, P-72.2.2.2.2); for example, methoxide is preferred to methanolate."
            }
            Self::Phane => {
                "The structure is a cyclophane: at least six nodes, two or more rings and a mancude ring. Cyclic phane names rank above fused, bridged fused and von Baeyer names, so the preferred IUPAC name is a phane name (P-52.2.5.1, P-52.2.5.2). ReShiki has not verified the numbering and citation order of its phane name."
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Provenance {
    OpsinRust,
    OpsinRustSymbols { name: String },
    OpsinRustRetainedName { source: &'static str },
    Openclatura,
    PubChem { cid: u64 },
}
impl Provenance {
    pub fn source_url(&self) -> Option<String> {
        match self {
            Self::PubChem { cid } => {
                Some(format!("https://pubchem.ncbi.nlm.nih.gov/compound/{cid}"))
            }
            _ => None,
        }
    }
    pub fn label(&self) -> String {
        match self {
            Self::OpsinRust => "OPSIN Rust port · local deterministic parser".into(),
            Self::OpsinRustSymbols { .. } => "OPSIN Rust port · expanded symbol notation".into(),
            Self::OpsinRustRetainedName { source } => {
                format!("{source} retained name · parsed locally by OPSIN Rust port")
            }
            Self::PubChem { cid } => format!("PubChem database · CID {cid}"),
            Self::Openclatura => "openclatura 0.4.3 Rust port · verified by OPSIN Rust port".into(),
        }
    }
}

/// An independently reconstructed alternative generated name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AlternativeName {
    pub name: String,
    pub status: NameStatus,
}

#[derive(Debug, Clone)]
pub struct Record {
    pub title: String,
    /// The source's systematic name. OPSIN parses the supplied name; it does
    /// not produce a new systematic name from a graph.
    pub systematic_name: Option<String>,
    /// Only generated names carry a rule-path preference assessment.
    pub name_status: Option<NameStatus>,
    pub alternatives: Vec<AlternativeName>,
    /// Parent locants whose unknown configuration the primary name omits.
    pub omitted_stereo: Vec<String>,
    /// Unknown stereo units the primary name omits, located or not.
    pub omitted_units: usize,
    pub smiles: String,
    pub canonical_smiles: String,
    pub locants: Option<Locants>,
    /// Curated offline aliases, separate from the generated systematic name.
    /// Every entry identifies its source and the exact matched chemical graph.
    pub synonyms: Vec<Alias>,
    pub warnings: Vec<String>,
    pub provenance: Provenance,
}
impl Record {
    /// The expanded input read by OPSIN, separate from a generated name.
    pub fn interpreted_name(&self) -> Option<&str> {
        match &self.provenance {
            Provenance::OpsinRustSymbols { name } => Some(name),
            _ => None,
        }
    }
}

impl Record {
    /// What the primary name leaves out of a drawing with unknown configuration.
    pub fn omitted_stereo_note(&self) -> Option<String> {
        match self.omitted_units {
            0 => None,
            units if units == self.omitted_stereo.len() => Some(format!(
                "Configuration at {} {} not specified in name",
                if units == 1 { "position" } else { "positions" },
                self.omitted_stereo.join(", ")
            )),
            1 => Some("Configuration of 1 unit drawn as unknown not specified in name".into()),
            units => Some(format!(
                "Configuration of {units} units drawn as unknown not specified in name"
            )),
        }
    }
}

/// Principal-parent locants in the reconstruction's own SMILES atom order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Locants {
    pub smiles: String,
    pub atoms: Vec<Option<String>>,
}

impl Locants {
    /// Map only the same constitution; drawing order, coordinates, stereo and
    /// drawn hydrogen atoms (such as those at ring fusions) may differ.
    pub fn for_document(&self, document: &Document) -> HashMap<u64, String> {
        let mapped = || {
            // Fold each plain drawn hydrogen into its one neighbour's count.
            let mut heavy = document.clone();
            let folded: Vec<_> = heavy
                .atoms
                .iter()
                .filter(|a| a.element == "H" && a.isotope == 0 && a.charge == 0)
                .filter_map(|a| {
                    let mut bonds = heavy.bonds.iter().filter(|b| b.a == a.id || b.b == a.id);
                    match (bonds.next(), bonds.next()) {
                        (Some(b), None) => Some((a.id, if b.a == a.id { b.b } else { b.a })),
                        _ => None,
                    }
                })
                .collect();
            for &(_, neighbour) in &folded {
                heavy.atom_mut(neighbour)?.explicit_h += 1;
            }
            heavy.delete(&folded.iter().map(|&(h, _)| h).collect::<Vec<_>>());
            let molecule = chemistry::document::prepare(&heavy).ok()?;
            // Reading may remove explicit hydrogens, which would shift indices.
            let source = chemistry::smiles::read(&self.smiles).ok()?.prepared.state;
            if source.graph.atoms.len() != self.atoms.len() {
                return None;
            }
            let write = |state| {
                let options = chemistry::smiles::write::Options {
                    ignore_maps: true,
                    symbols: chemistry::smiles::symbols::Options {
                        isomeric: false,
                        ..Default::default()
                    },
                    ..Default::default()
                };
                chemistry::smiles::write::write(state, options).ok()
            };
            let (source, target) = (write(&source)?, write(&molecule.state)?);
            (source.text == target.text).then(|| {
                target
                    .atom_order
                    .iter()
                    .zip(&source.atom_order)
                    .filter_map(|(&target, &source)| {
                        let locant = self.atoms.get(source)?.as_ref()?;
                        Some((*molecule.ids.get(target)?, locant.clone()))
                    })
                    .collect()
            })
        };
        mapped().unwrap_or_default()
    }
}

/// Atom numbers use Unicode superscript digits for OPSIN's interior locants.
pub fn locant_text(locant: &str) -> String {
    let mut superscript = false;
    locant
        .chars()
        .filter_map(|c| {
            if c == '^' {
                superscript = true;
                None
            } else if superscript && c.is_ascii_digit() {
                "⁰¹²³⁴⁵⁶⁷⁸⁹".chars().nth(c as usize - '0' as usize)
            } else {
                superscript = false;
                Some(c)
            }
        })
        .collect()
}

#[derive(Debug, Clone)]
pub struct Identity {
    pub smiles: String,
    /// Unknown-unit and axis markers for generation; ordinary atom maps are removed.
    pub naming_smiles: Option<String>,
    /// Unknown-unit markers including stereogenic units with no drawn configuration.
    /// Used only when explicit unknown descriptors are requested.
    pub unknown_stereo_smiles: Option<String>,
    /// Whether the UI requested explicit IUPAC Ξ/ξ descriptors.
    pub write_unknown_stereo: bool,
    pub warnings: Vec<String>,
}

impl Identity {
    fn generation_smiles<'a>(&'a self, expected: &'a str) -> &'a str {
        let explicit = if self.write_unknown_stereo {
            self.unknown_stereo_smiles.as_deref()
        } else {
            None
        };
        explicit
            .or(self.naming_smiles.as_deref())
            .unwrap_or(expected)
    }
}

fn canonical_state(state: &chemistry::stereo::perception::State) -> Result<String, String> {
    if state.graph.atoms.is_empty() || state.graph.atoms.len() > MAX_NAMING_ATOMS {
        return Err("Naming supports molecular graphs with 1–512 atoms".into());
    }
    // molecule_names::identity keeps its component checks and one semantics
    // message; this adapter retains the naming-specific diagnostics and order.
    molecule_names::canonical_identity(state).map_err(|error| match error {
        IdentityRejection::UnsupportedAtomsOrBonds => {
            "Naming does not support query atoms, radicals, exotic bonds or relative stereo groups"
                .into()
        }
        IdentityRejection::NonclassicalStereo => {
            const MESSAGE: &str = "Naming supports tetrahedral, ordinary alkene and biaryl axis stereochemistry; other stereo classes cannot be discarded";
            MESSAGE.into()
        }
        IdentityRejection::Write(message) => message,
    })
}

/// Canonical isomeric graph identity, including isotope, charge and specified
/// stereo. Reserved axis markers carry biaryl configuration; other maps and
/// atom order are not molecular identity.
pub fn canonical_smiles(text: &str) -> Result<String, String> {
    if text.len() > MAX_SMILES_BYTES || text.contains('|') || text.contains('>') {
        return Err(
            "Naming requires ordinary molecular SMILES without reaction or CX extensions".into(),
        );
    }
    bound_atom_tokens(text)?;
    // Native import cleans unusable winding and can discard non-tetrahedral
    // classes. Check the raw graph first, before that cleanup changes identity.
    // Unlike canonical_identity, this raw check intentionally covers atoms only.
    let raw = chemistry::smiles::parse(text).map_err(|e| e.to_string())?;
    if raw.metadata.atoms.iter().any(|m| m.chiral_tag > 2) {
        return Err("Naming does not support allene or coordination stereo classes; no plain graph was substituted".into());
    }
    let specified_atoms = raw
        .metadata
        .atoms
        .iter()
        .filter(|m| is_tetrahedral(m.chiral_tag))
        .count();
    let imported = chemistry::smiles::read(text).map_err(|e| e.to_string())?;
    if specified_atoms
        != imported
            .prepared
            .state
            .metadata
            .atoms
            .iter()
            .filter(|m| is_tetrahedral(m.chiral_tag))
            .count()
    {
        return Err("Native import could not retain every specified tetrahedral center; no name or graph was assigned".into());
    }
    let state = molecule_names::apply_axis_markers(&imported.prepared.state)
        .map_err(|_| "Naming received invalid biaryl axis markers".to_owned())?;
    canonical_state(&state)
}

/// Reject oversized output before native sanitization/ranking can allocate or
/// expand a graph. Bracket atoms count once; Cl/Br are two-character atoms.
fn bound_atom_tokens(text: &str) -> Result<(), String> {
    let bytes = text.as_bytes();
    let mut i = 0;
    let mut atoms = 0usize;
    while let Some(&b) = bytes.get(i) {
        if b == b'[' {
            atoms += 1;
            let end = bytes
                .get(i + 1..)
                .and_then(|s| s.iter().position(|&b| b == b']'))
                .ok_or("Unclosed naming SMILES atom")?;
            i += end + 2;
        } else {
            if b.is_ascii_alphabetic() || b == b'*' {
                atoms += 1;
                if (b == b'C' && bytes.get(i + 1) == Some(&b'l'))
                    || (b == b'B' && bytes.get(i + 1) == Some(&b'r'))
                {
                    i += 1;
                }
            }
            i += 1;
        }
        if atoms > MAX_NAMING_ATOMS {
            return Err(
                "Naming supports at most 512 input atom tokens; no oversized graph was imported"
                    .into(),
            );
        }
    }
    Ok(())
}

/// Require complete molecules or salt ions. Abbreviation selection expands to
/// its complete underlying graph; display labels never replace chemical atoms.
pub fn selected_identity(document: &Document, selected: &[u64]) -> Result<Identity, String> {
    let ids: HashSet<_> = editing::analysis_atoms(document, selected)
        .into_iter()
        .collect();
    if ids.is_empty() {
        return Err("Select one complete molecule before generating its name".into());
    }
    if document
        .bonds
        .iter()
        .any(|b| ids.contains(&b.a) != ids.contains(&b.b))
    {
        return Err(
            "The selection cuts a bond. Select the complete molecule before generating a name"
                .into(),
        );
    }
    let part = editing::selection(document, &ids.iter().copied().collect::<Vec<_>>());
    drawing_identity(&part, document)
}

/// Canonical identity of a whole drawing, which may hold several components,
/// such as the ions of a parsed salt.
pub fn structure_smiles(document: &Document) -> Result<String, String> {
    if document.atoms.is_empty() || document.atoms.len() > MAX_NAMING_ATOMS {
        return Err("Naming supports molecular graphs with 1–512 atoms".into());
    }
    let molecule = chemistry::document::prepare(document).map_err(|e| e.to_string())?;
    canonical_state(&molecule.state)
}

pub fn document_identity(document: &Document) -> Result<Identity, String> {
    drawing_identity(document, document)
}

fn drawing_identity(document: &Document, names: &Document) -> Result<Identity, String> {
    if document.atoms.is_empty() || document.atoms.len() > MAX_NAMING_ATOMS {
        return Err("Naming supports molecular graphs with 1–512 atoms".into());
    }
    let molecule = chemistry::document::prepare(document).map_err(|e| e.to_string())?;
    let smiles = canonical_state(&molecule.state)?;
    if smiles.contains('.') && !molecule_names::charge_balanced_salt(document) {
        return Err("Select one complete molecule or a charge-balanced salt; neutral components, mixtures, net-charged sets and zwitterions mixed with ions are unsupported".into());
    }
    let mut marked = molecule_names::unspecified_unknown_stereo(&molecule.state);
    let mut unknown = vec![0; marked.metadata.atoms.len()];
    for (idx, (is_unknown, marker)) in molecule_names::unknown_atom_units(&molecule.state)
        .into_iter()
        .zip(&mut unknown)
        .enumerate()
    {
        if !is_unknown {
            continue;
        }
        // A decorative wavy bond on an achiral atom has no stereolocant.
        let stereogenic = [1, 2].into_iter().all(|tag| {
            let mut trial = marked.clone();
            if let Some(atom) = trial.metadata.atoms.get_mut(idx) {
                atom.chiral_tag = tag;
            }
            chemistry::stereo::cip::label::assign(&trial, &Default::default()).is_ok_and(
                |assignment| {
                    assignment
                        .state
                        .properties
                        .atoms
                        .get(idx)
                        .is_some_and(|atom| atom.cip_code.is_some())
                },
            )
        });
        if stereogenic {
            *marker |= 1;
        }
    }
    for (idx, (bond, metadata)) in molecule
        .state
        .graph
        .bonds
        .iter()
        .zip(&molecule.state.metadata.bonds)
        .enumerate()
    {
        if bond.order != 2 || !(metadata.unknown_stereo || metadata.stereo == 1) {
            continue;
        }
        // As for atoms, a crossed bond that cannot be E/Z has no stereolocant.
        let neighbour = |atom: usize, other: usize| {
            marked.graph.bonds.iter().find_map(|b| {
                (b.a == atom && b.b != other)
                    .then_some(b.b)
                    .or((b.b == atom && b.a != other).then_some(b.a))
            })
        };
        let (Some(first), Some(last)) = (neighbour(bond.a, bond.b), neighbour(bond.b, bond.a))
        else {
            continue;
        };
        let mut trial = marked.clone();
        if let Some(metadata) = trial.metadata.bonds.get_mut(idx) {
            metadata.stereo = 4;
            metadata.stereo_atoms = vec![first, last];
        }
        if chemistry::stereo::cip::label::assign(&trial, &Default::default()).is_ok_and(
            |assignment| {
                assignment
                    .state
                    .properties
                    .bond_codes
                    .get(idx)
                    .is_some_and(Option::is_some)
            },
        ) {
            for endpoint in [bond.a, bond.b] {
                if let Some(marker) = unknown.get_mut(endpoint) {
                    *marker |= 2;
                }
            }
        }
    }
    marked = molecule_names::encode_axis_markers(&marked)
        .map_err(|_| "Naming could not retain the biaryl axis configuration".to_owned())?;
    // Unknown markers only grow between calls; atoms without one keep their
    // biaryl axis map.
    let mut marker_smiles = |unknown: &[i32]| -> Result<Option<String>, String> {
        for (metadata, marker) in marked.metadata.atoms.iter_mut().zip(unknown) {
            if *marker != 0 {
                metadata.map_number = 900_000 + marker;
                metadata.map_present = true;
            }
        }
        if unknown.iter().any(|&marker| marker != 0) {
            Ok(Some(
                chemistry::smiles::write::write(&marked, Default::default())
                    .map_err(|error| error.to_string())?
                    .text,
            ))
        } else {
            Ok(None)
        }
    };
    // Keep the original wavy-only input independent of the naming preference.
    let naming_smiles = marker_smiles(&unknown)?;
    let undefined = chemistry::stereo_lint::undefined(document);
    let unknown_stereo_smiles = if undefined.atoms.is_empty() && undefined.bonds.is_empty() {
        None
    } else {
        let indices: HashMap<_, _> = molecule
            .ids
            .iter()
            .enumerate()
            .map(|(index, &id)| (id, index))
            .collect();
        for id in &undefined.atoms {
            if let Some(marker) = indices.get(id).and_then(|&index| unknown.get_mut(index)) {
                *marker |= 1;
            }
        }
        for (a, b) in &undefined.bonds {
            for id in [a, b] {
                if let Some(marker) = indices.get(id).and_then(|&index| unknown.get_mut(index)) {
                    *marker |= 2;
                }
            }
        }
        marker_smiles(&unknown)?
    };
    let mut units = undefined
        .atoms
        .into_iter()
        .map(|id| chemistry::stereo_lint::atom_name(names, id))
        .chain(undefined.bonds.into_iter().map(|(a, b)| {
            format!(
                "the {}={} bond",
                chemistry::stereo_lint::atom_name(names, a),
                chemistry::stereo_lint::atom_name(names, b)
            )
        }))
        .collect::<Vec<_>>();
    let warnings = match units.pop() {
        None => vec![],
        Some(unit) if units.is_empty() => vec![format!(
            "{unit} has no configuration; the name leaves it unspecified"
        )],
        Some(last) => vec![format!(
            "{} and {last} have no configuration; the name leaves them unspecified",
            units.join(", ")
        )],
    };
    Ok(Identity {
        smiles,
        naming_smiles,
        unknown_stereo_smiles,
        write_unknown_stereo: false,
        warnings,
    })
}

/// An identity as editable SMILES. SMILES has no axis layer, so this leaves
/// out the private biaryl axis maps that `canonical_smiles` may carry.
pub fn plain_smiles(identity: &str) -> String {
    let stripped = [molecule_names::AXIS_M, molecule_names::AXIS_P]
        .iter()
        .fold(identity.to_owned(), |text, map| {
            text.replace(&format!(":{map}]"), "]")
        });
    canonical_smiles(&stripped).unwrap_or(stripped)
}

pub fn verify_identity(expected: &str, actual: &str) -> Result<(), String> {
    if canonical_smiles(expected)? != canonical_smiles(actual)? {
        return Err("The source structure differs in connectivity, charge, isotope, tautomer or stereochemistry. No name was assigned".into());
    }
    Ok(())
}

#[cfg(test)]
mod axial_tests;
#[cfg(test)]
mod tests;
