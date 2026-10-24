//! Local deterministic name/structure conversion with native identity checks.
use crate::{
    chemistry,
    document::Document,
    editing,
    molecule_names::{self, IdentityRejection, MAX_NAMING_ATOMS, MAX_SMILES_BYTES, is_tetrahedral},
};
use std::collections::HashSet;

mod catalog;
pub use catalog::VERSION as ALIAS_CATALOG_VERSION;
pub use catalog::{Alias, AliasSource, aliases_for_smiles};

// The parsers run in bounded worker processes on the desktop, and in a
// dedicated Web Worker with its own bounded memory in the browser.
#[cfg(not(target_arch = "wasm32"))]
mod client;
mod local;
mod reverse;
mod rules;
pub mod worker;
#[cfg(not(target_arch = "wasm32"))]
pub use {
    local::{Cancel, resolve_name},
    reverse::generate_name,
};
#[cfg(target_arch = "wasm32")]
mod browser;
#[cfg(target_arch = "wasm32")]
pub use browser::{Cancel, generate_name, resolve_name, set_worker};

/// Why a local name request produced no result. Display prints the message
/// unchanged; the variant tells callers how to summarize it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The name or structure is outside what local naming supports.
    Rejected(String),
    /// The name describes several disconnected pieces, such as a salt or a
    /// mixture. Local naming accepts one connected molecule.
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Provenance {
    OpsinRust,
    OpsinRustRetainedName { source: &'static str },
    LocalRules,
}
impl Provenance {
    pub fn label(&self) -> String {
        match self {
            Self::OpsinRust => "OPSIN Rust port · local deterministic parser".into(),
            Self::OpsinRustRetainedName { source } => {
                format!("{source} retained name · parsed locally by OPSIN Rust port")
            }
            Self::LocalRules => "ReShiki rules 1 · verified by OPSIN Rust port".into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Record {
    pub title: String,
    /// The source's systematic name. OPSIN parses the supplied name; it does
    /// not produce a new systematic name from a graph.
    pub systematic_name: Option<String>,
    pub smiles: String,
    pub canonical_smiles: String,
    /// Curated offline aliases, separate from the generated systematic name.
    /// Every entry identifies its source and the exact matched chemical graph.
    pub synonyms: Vec<Alias>,
    pub warnings: Vec<String>,
    pub provenance: Provenance,
}

#[derive(Debug, Clone)]
pub struct Identity {
    pub smiles: String,
    pub warnings: Vec<String>,
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
        IdentityRejection::UnknownStereo => {
            const MESSAGE: &str = "Resolve unknown stereochemistry before generating a name; wavy stereo cannot be sent as unspecified stereo";
            MESSAGE.into()
        }
        IdentityRejection::NonclassicalStereo => {
            const MESSAGE: &str = "Naming supports tetrahedral and ordinary alkene stereochemistry only; other stereo classes cannot be discarded";
            MESSAGE.into()
        }
        IdentityRejection::Write(message) => message,
    })
}

/// Canonical isomeric graph identity, including isotope, charge and specified
/// stereo. Map numbers and atom order are not molecular identity.
pub fn canonical_smiles(text: &str) -> Result<String, String> {
    if text.len() > MAX_SMILES_BYTES || text.contains('|') || text.contains('>') {
        return Err(
            "Naming requires ordinary molecular SMILES without reaction or CX extensions".into(),
        );
    }
    bound_atom_tokens(text)?;
    // Native import cleans unusable winding and can discard non-tetrahedral
    // classes. Check the raw graph first, before that cleanup changes identity.
    // Unlike has_nonclassical_stereo, this raw check intentionally covers atoms only.
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
    canonical_state(&imported.prepared.state)
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

/// Require a complete connected molecule. Abbreviation selection expands to
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
    document_identity(&part)
}

pub fn document_identity(document: &Document) -> Result<Identity, String> {
    if document.atoms.is_empty() || document.atoms.len() > MAX_NAMING_ATOMS {
        return Err("Naming supports molecular graphs with 1–512 atoms".into());
    }
    if molecule_names::has_unknown_wavy_bond(document) {
        return Err("Resolve wavy/unknown stereochemistry before generating a name".into());
    }
    let molecule = chemistry::document::prepare(document).map_err(|e| e.to_string())?;
    let smiles = canonical_state(&molecule.state)?;
    if smiles.contains('.') {
        return Err("Select a single connected molecule; mixtures and disconnected salts are not supported for local naming".into());
    }
    Ok(Identity {
        smiles,
        warnings: vec!["Names describe specified stereochemistry only. Unspecified centers remain unspecified; no absolute configuration is inferred by the naming engine.".into()],
    })
}

pub fn verify_identity(expected: &str, actual: &str) -> Result<(), String> {
    if canonical_smiles(expected)? != canonical_smiles(actual)? {
        return Err("The source structure differs in connectivity, charge, isotope, tautomer or stereochemistry. No name was assigned".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
