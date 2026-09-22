//! RXN and reaction SMILES interchange with explicit participant roles.
//! Component arrangement follows RDKit ReactionWriter.cpp (2026.03.6).
//! Copyright (C) 2010-2024 Novartis Institutes for BioMedical Research Inc.
//! and other RDKit contributors.
//! BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
use super::{document, molfile};
use crate::{document::Document, reactions::Role};
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fmt::Write,
};
mod drawing;
pub use drawing::Drawing;
mod layout;
mod smiles;
pub use smiles::{ReadError as SmilesError, SmilesReaction, read as read_smiles};

/// Parsed participants in file order; drawing placement is a separate operation.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Imported {
    pub reactants: Vec<molfile::Imported>,
    pub products: Vec<molfile::Imported>,
    pub agents: Vec<molfile::Imported>,
}

/// Read an RXN file without flattening query atoms or losing explicit hydrogens.
pub fn read_rxn(text: &str) -> Result<Imported, molfile::ReadError> {
    molfile::read_reaction(text)
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid reaction: {0}")]
    Invalid(String),
    #[error(transparent)]
    Preparation(#[from] document::Error),
    #[error(transparent)]
    Molecular(#[from] molfile::Error),
    #[error(transparent)]
    Smiles(#[from] super::smiles::write::Error),
    #[error("Reaction exceeds the 10,000 atom or 16 MB output limit")]
    Limit,
    #[error("Could not format reaction output: {0}")]
    Formatting(#[from] std::fmt::Error),
}

fn invalid(message: impl Into<String>) -> Error {
    Error::Invalid(message.into())
}

pub const EXPORT_WARNING: &str = "Reaction files preserve participants, atom maps and stereo. Save .reshiki to retain captions, arrow appearance and drawing layout.";

struct OutputRow {
    label: &'static str,
    parts: Vec<(String, usize)>,
    count: usize,
}

fn prepare_output(
    doc: &Document,
    selected: Option<&[u64]>,
    render: impl Fn(&document::Molecule) -> Result<String, Error>,
) -> Result<Vec<OutputRow>, Error> {
    doc.validate().map_err(invalid)?;
    let selected: HashSet<_> = selected.unwrap_or_default().iter().copied().collect();
    let mut reactions = doc
        .reactions
        .iter()
        .filter(|r| selected.is_empty() || selected.contains(&r.arrow));
    let source = reactions
        .next()
        .ok_or_else(|| invalid("Choose one defined reaction before exporting"))?;
    if reactions.next().is_some() {
        return Err(invalid("Choose one defined reaction before exporting"));
    }
    if source.reactants.is_empty() || source.products.is_empty() {
        return Err(invalid(
            "Assign at least one reactant and one product before exporting",
        ));
    }
    let atoms: HashMap<_, _> = doc
        .atoms
        .iter()
        .enumerate()
        .map(|(i, a)| (a.id, i))
        .collect();
    let mut incident: HashMap<u64, Vec<usize>> = HashMap::new();
    for (i, bond) in doc.bonds.iter().enumerate() {
        incident.entry(bond.a).or_default().push(i);
        incident.entry(bond.b).or_default().push(i);
    }
    let mut expanded = 0usize;
    let mut rows = Vec::new();
    for (role, label) in [
        (Role::Reactant, "REACTANT"),
        (Role::Product, "PRODUCT"),
        (Role::Agent, "AGENT"),
    ] {
        let mut maps = HashSet::new();
        let mut parts = Vec::new();
        let mut count = 0usize;
        for participant in source.participants(role) {
            let members: HashSet<_> = participant.atoms.iter().copied().collect();
            let coefficient = usize::from(participant.coefficient);
            expanded = expanded
                .checked_add(members.len().checked_mul(coefficient).ok_or(Error::Limit)?)
                .ok_or(Error::Limit)?;
            if expanded > 10_000 {
                return Err(Error::Limit);
            }
            // Original array order, rather than participant ID order, defines
            // the atom and bond indices written to the reaction file.
            let mut indices = members
                .iter()
                .map(|id| {
                    atoms
                        .get(id)
                        .copied()
                        .ok_or_else(|| invalid("Missing participant atom"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            indices.sort_unstable();
            let mut fragment = Document::default();
            for i in indices {
                fragment.atoms.push(
                    doc.atoms
                        .get(i)
                        .ok_or_else(|| invalid("Missing reaction atom"))?
                        .clone(),
                );
            }
            let bonds: BTreeSet<_> = members
                .iter()
                .flat_map(|id| incident.get(id).into_iter().flatten().copied())
                .collect();
            for i in bonds {
                let bond = doc
                    .bonds
                    .get(i)
                    .ok_or_else(|| invalid("Missing reaction bond"))?;
                if matches!(bond.order, 0 | 6 | 7) {
                    return Err(invalid(
                        "Reaction exchange cannot preserve hydrogen, partial or quadruple bonds; save the native drawing",
                    ));
                }
                if !members.contains(&bond.a) || !members.contains(&bond.b) {
                    return Err(invalid("Assign complete molecules to reaction roles"));
                }
                fragment.bonds.push(bond.clone());
            }
            let molecule = document::prepare(&fragment)?;
            for atom in &molecule.state.metadata.atoms {
                if atom.map_number != 0 && (coefficient > 1 || !maps.insert(atom.map_number)) {
                    return Err(invalid(
                        "Atom map numbers must be unique on each reaction side; repeated mapped molecules need separate mappings",
                    ));
                }
            }
            let text = render(&molecule)?;
            count = count.checked_add(coefficient).ok_or(Error::Limit)?;
            parts.push((text, coefficient));
        }
        rows.push(OutputRow {
            label,
            parts,
            count,
        });
    }
    Ok(rows)
}

/// Export an RXN file with explicit participant roles and coefficients.
pub fn write_rxn(doc: &Document, selected: Option<&[u64]>) -> Result<String, Error> {
    let rows = prepare_output(doc, selected, |molecule| {
        Ok(molfile::reaction_ctab(molecule)?)
    })?;
    let mut output = String::from("$RXN V3000\n\n      RDKit\n\nM  V30 COUNTS");
    for row in &rows {
        write!(output, " {}", row.count)?;
    }
    output.push('\n');
    for OutputRow { label, parts, .. } in rows {
        writeln!(output, "M  V30 BEGIN {label}")?;
        for (ctab, coefficient) in parts {
            let length = output
                .len()
                .checked_add(ctab.len().checked_mul(coefficient).ok_or(Error::Limit)?)
                .ok_or(Error::Limit)?;
            if length > 16 * 1024 * 1024 - 256 {
                return Err(Error::Limit);
            }
            for _ in 0..coefficient {
                output.push_str(&ctab);
            }
        }
        writeln!(output, "M  V30 END {label}")?;
    }
    output.push_str("M  END\n");
    Ok(output)
}

/// Canonical reaction SMILES, preserving grouped components and repeated roles.
/// Drawing positions and captions do not determine reaction membership.
pub fn write_smiles(doc: &Document, selected: Option<&[u64]>) -> Result<String, Error> {
    use super::smiles::write;
    let rows = prepare_output(doc, selected, |molecule| {
        let text = write::write(&molecule.state, write::Options::default())?.text;
        // Default molecular output has no custom symbols: a dot identifies a
        // disconnected participant, which reaction SMILES groups in parentheses.
        Ok(if text.contains('.') {
            format!("({text})")
        } else {
            text
        })
    })?;
    let mut output = String::new();
    // RXN records products before agents; reaction SMILES puts agents between.
    for (position, id) in [0, 2, 1].into_iter().enumerate() {
        if position > 0 {
            output.push('>');
        }
        let row = rows.get(id).ok_or(Error::Limit)?;
        let mut parts = row
            .parts
            .iter()
            .flat_map(|(text, count)| std::iter::repeat_n(text.as_str(), *count))
            .collect::<Vec<_>>();
        parts.sort_unstable();
        for (i, part) in parts.into_iter().enumerate() {
            let length = output
                .len()
                .checked_add(part.len())
                .and_then(|s| s.checked_add(3))
                .ok_or(Error::Limit)?;
            if length > 16 * 1024 * 1024 {
                return Err(Error::Limit);
            }
            if i > 0 {
                output.push('.');
            }
            output.push_str(part);
        }
    }
    Ok(output)
}
