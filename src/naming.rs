//! Local deterministic name/structure conversion with native identity checks.
use crate::{chemistry, document::Document, editing};
use std::collections::HashSet;

mod local;
pub use local::{Cancel, resolve_name};
mod reverse;
mod rules;
pub mod worker;
pub use reverse::generate_name;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Provenance {
    OpsinRust,
    LocalRules,
}
impl Provenance {
    pub fn label(&self) -> String {
        match self {
            Self::OpsinRust => "OPSIN Rust port · local deterministic parser".into(),
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
    pub synonyms: Vec<String>,
    pub warnings: Vec<String>,
    pub provenance: Provenance,
}

#[derive(Debug, Clone)]
pub struct Identity {
    pub smiles: String,
    pub warnings: Vec<String>,
}

fn canonical_state(state: &chemistry::stereo::perception::State) -> Result<String, String> {
    if state.graph.atoms.is_empty() || state.graph.atoms.len() > 512 {
        return Err("Naming supports molecular graphs with 1–512 atoms".into());
    }
    if state
        .graph
        .atoms
        .iter()
        .any(|a| a.atomic_number == 0 || a.radical_electrons != 0)
        || state
            .graph
            .bonds
            .iter()
            .any(|b| !(1..=4).contains(&b.order))
        || !state.metadata.groups.is_empty()
    {
        return Err(
            "Naming does not support query atoms, radicals, exotic bonds or relative stereo groups"
                .into(),
        );
    }
    if state.properties.atoms.iter().any(|a| a.unknown)
        || state
            .metadata
            .bonds
            .iter()
            .any(|b| b.unknown_stereo || b.stereo == 1)
    {
        return Err("Resolve unknown stereochemistry before generating a name; wavy stereo cannot be sent as unspecified stereo".into());
    }
    if state.metadata.atoms.iter().any(|m| m.chiral_tag > 2)
        || state.metadata.bonds.iter().any(|m| m.stereo > 5)
    {
        return Err("Naming supports tetrahedral and ordinary alkene stereochemistry only; other stereo classes cannot be discarded".into());
    }
    let mut state = state.clone();
    for atom in &mut state.metadata.atoms {
        atom.map_number = 0;
        atom.map_present = false;
    }
    chemistry::smiles::write::write(
        &state,
        chemistry::smiles::write::Options {
            ignore_maps: true,
            ..Default::default()
        },
    )
    .map(|s| s.text)
    .map_err(|e| e.to_string())
}

/// Canonical isomeric graph identity, including isotope, charge and specified
/// stereo. Map numbers and atom order are not molecular identity.
pub fn canonical_smiles(text: &str) -> Result<String, String> {
    if text.len() > 32_768 || text.contains('|') || text.contains('>') {
        return Err(
            "Naming requires ordinary molecular SMILES without reaction or CX extensions".into(),
        );
    }
    bound_atom_tokens(text)?;
    // Native import cleans unusable winding and can discard non-tetrahedral
    // classes. Check the raw graph first, before that cleanup changes identity.
    let raw = chemistry::smiles::parse(text).map_err(|e| e.to_string())?;
    if raw.metadata.atoms.iter().any(|m| m.chiral_tag > 2) {
        return Err("Naming does not support allene or coordination stereo classes; no plain graph was substituted".into());
    }
    let specified_atoms = raw
        .metadata
        .atoms
        .iter()
        .filter(|m| matches!(m.chiral_tag, 1 | 2))
        .count();
    let imported = chemistry::smiles::read(text).map_err(|e| e.to_string())?;
    if specified_atoms
        != imported
            .prepared
            .state
            .metadata
            .atoms
            .iter()
            .filter(|m| matches!(m.chiral_tag, 1 | 2))
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
        if atoms > 512 {
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
    if document.atoms.is_empty() || document.atoms.len() > 512 {
        return Err("Naming supports molecular graphs with 1–512 atoms".into());
    }
    if document
        .bonds
        .iter()
        .any(|b| !b.projection && b.display == "wavy")
    {
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
