//! Abbreviation validation and fixed-preset detection without changing chemistry.
//! Query adjustment, matching order and overlap rules follow RDKit 2026.03.6
//! AbbreviationsUtils.cpp, Abbreviations.cpp and Substruct/vf2.hpp.
//! Copyright (C) 2020 Greg Landrum and T5 Informatics GmbH; RDKit contributors.
//! BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
mod matcher;
mod replacement;
use super::{RDKIT_VERSION, document::Molecule};
use crate::{abbreviations::Abbreviation, document::Document};
pub use replacement::{Error as ReplacementError, replace};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    sync::OnceLock,
};

#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    #[error("Abbreviations require document version 10")]
    Version,
    #[error("An abbreviation needs a label")]
    Label,
    #[error("Invalid abbreviation label")]
    InvalidLabel,
    #[error("Invalid or overlapping abbreviation atoms")]
    Members,
    #[error("Abbreviations need one attachment atom and at most one outside bond")]
    Attachment,
    #[error("An abbreviation must be connected")]
    Connected,
    #[error("Unknown abbreviation")]
    Unknown,
    #[error("Abbreviation detection: {0}")]
    Invalid(String),
    #[error("Abbreviation matching work limit exceeded")]
    WorkLimit,
}
type Result<T> = std::result::Result<T, Error>;
fn invalid(message: impl Into<String>) -> Error {
    Error::Invalid(message.into())
}
fn at<T>(items: &[T], index: usize) -> Result<&T> {
    items
        .get(index)
        .ok_or_else(|| invalid("Missing graph item"))
}

fn check_size(document: &Document) -> Result<()> {
    if document.atoms.len() > 100_000
        || document.bonds.len() > 300_000
        || document.abbreviations.len() > 100_000
    {
        return Err(invalid("Document size limit exceeded"));
    }
    let mut members = 0usize;
    for group in &document.abbreviations {
        members = members
            .checked_add(group.members.len())
            .ok_or_else(|| invalid("Abbreviation storage limit exceeded"))?;
        if members > 1_000_000 {
            return Err(invalid("Abbreviation storage limit exceeded"));
        }
    }
    Ok(())
}

/// Native adjusted SMARTS predicates for the application's fixed definitions.
/// Exposed labels/SMILES also let replacement use this same preset catalog.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Preset {
    pub label: String,
    pub reverse_label: String,
    pub smiles: String,
    atoms: Vec<matcher::Atom>,
    bonds: Vec<matcher::Bond>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Data {
    rdkit_version: String,
    presets: Vec<Preset>,
}

pub fn presets() -> Result<&'static [Preset]> {
    static DATA: OnceLock<Result<Data>> = OnceLock::new();
    DATA.get_or_init(|| {
        let data: Data = serde_json::from_str(include_str!("abbreviations/presets.json"))
            .map_err(|error| invalid(format!("Invalid preset data: {error}")))?;
        if data.rdkit_version != RDKIT_VERSION
            || data.presets.len() != crate::abbreviations::PRESETS.len()
        {
            return Err(invalid("Preset data version or count mismatch"));
        }
        for (preset, expected) in data.presets.iter().zip(crate::abbreviations::PRESETS) {
            if preset.label != *expected || preset.smiles.is_empty() {
                return Err(invalid("Invalid preset definition"));
            }
            matcher::validate(preset)?;
        }
        Ok(data)
    })
    .as_ref()
    .map(|data| data.presets.as_slice())
    .map_err(Clone::clone)
}

/// Validate precisely the abbreviation contract, independently of preparation.
/// Graph connectivity is indexed once, making even large collapsed groups linear.
pub fn validate(document: &Document) -> Result<()> {
    if !document.abbreviations.is_empty() && document.version < 10 {
        return Err(Error::Version);
    }
    check_size(document)?;
    let ids: HashSet<_> = document.atoms.iter().map(|a| a.id).collect();
    let mut adjacency: HashMap<u64, Vec<u64>> = HashMap::new();
    for (a, b) in crate::attachments::edges(document) {
        adjacency.entry(a).or_default().push(b);
        adjacency.entry(b).or_default().push(a);
    }
    let mut used = HashSet::<u64>::new();
    for group in &document.abbreviations {
        // Python str.strip also treats these four information separators as space.
        if group
            .label
            .chars()
            .all(|c| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
        {
            return Err(Error::Label);
        }
        if [&group.label, &group.reverse_label]
            .iter()
            .any(|text| text.chars().take(33).count() > 32 || text.chars().any(char::is_control))
        {
            return Err(Error::InvalidLabel);
        }
        if group.members.len() > ids.len() {
            return Err(Error::Members);
        }
        let members: HashSet<_> = group.members.iter().copied().collect();
        if members.is_empty()
            || members.len() != group.members.len()
            || !members.contains(&group.anchor)
            || members
                .iter()
                .any(|id| !ids.contains(id) || used.contains(id))
        {
            return Err(Error::Members);
        }
        used.extend(&members);
        let mut external = 0usize;
        for &id in &members {
            for &other in adjacency.get(&id).into_iter().flatten() {
                if !members.contains(&other) {
                    external += 1;
                    if external > 1 || id != group.anchor {
                        return Err(Error::Attachment);
                    }
                }
            }
        }
        let mut pending = vec![group.anchor];
        let mut reached = HashSet::from([group.anchor]);
        while let Some(id) = pending.pop() {
            for &other in adjacency.get(&id).into_iter().flatten() {
                if members.contains(&other) && reached.insert(other) {
                    pending.push(other);
                }
            }
        }
        if reached.len() != members.len() {
            return Err(Error::Connected);
        }
    }
    Ok(())
}

/// Add preset labels to a detached drawing. Selection and atom annotations filter
/// the native winning matches, so excluded groups never expose smaller matches.
/// The supplied molecule must be the prepared state for this document.
pub fn find(
    document: &Document,
    molecule: &Molecule,
    selection: &[u64],
    label: Option<&str>,
) -> Result<Document> {
    let mut available: Vec<_> = presets()?
        .iter()
        .filter(|preset| label.is_none_or(|label| label.is_empty() || label == preset.label))
        .collect();
    if available.is_empty() {
        return Err(Error::Unknown);
    }
    check_size(document)?;
    available.sort_by_key(|preset| std::cmp::Reverse(preset.atoms.len()));
    let atoms: HashMap<_, _> = document.atoms.iter().map(|a| (a.id, a)).collect();
    if molecule.rdkit_version != RDKIT_VERSION
        || molecule.ids.len() != molecule.state.graph.atoms.len()
        || atoms.len() != document.atoms.len()
        || molecule.ids.len() != atoms.len()
        || molecule.ids.iter().copied().collect::<HashSet<_>>().len() != atoms.len()
        || molecule.ids.iter().any(|id| !atoms.contains_key(id))
    {
        return Err(invalid("Prepared molecule identities changed"));
    }
    if selection.len() > 100_000 {
        return Err(invalid("Selection size limit exceeded"));
    }
    let target = matcher::Target::new(&molecule.state.graph, &molecule.state.rings)?;
    let mut covered = HashSet::new();
    let mut first_atoms = HashSet::new();
    let mut tentative = Vec::new();
    let mut work = matcher::Work::default();
    for preset in available {
        // maxCoverage=1.0 is exclusive; the dummy does not count as coverage.
        if preset.atoms.len() > molecule.ids.len() {
            continue;
        }
        for mapping in target.matches(preset, &mut work)? {
            let anchor = *at(&mapping, 1)?;
            if first_atoms.contains(&anchor)
                || mapping.iter().skip(1).any(|atom| covered.contains(atom))
            {
                continue;
            }
            covered.extend(mapping.iter().skip(1).copied());
            first_atoms.insert(anchor);
            if !first_atoms.contains(at(&mapping, 0)?) {
                tentative.push((preset, mapping));
            }
        }
    }
    let selected: HashSet<_> = selection.iter().copied().collect();
    let mut result = document.clone();
    let mut used: HashSet<_> = result
        .abbreviations
        .iter()
        .flat_map(|group| group.members.iter().copied())
        .collect();
    for (preset, mapping) in tentative {
        if first_atoms.contains(at(&mapping, 0)?) {
            continue;
        }
        let members = mapping
            .iter()
            .skip(1)
            .map(|&i| at(&molecule.ids, i).copied())
            .collect::<Result<Vec<_>>>()?;
        if members
            .iter()
            .any(|id| used.contains(id) || !selected.is_empty() && !selected.contains(id))
        {
            continue;
        }
        let mut excluded = false;
        for id in &members {
            let atom = atoms
                .get(id)
                .ok_or_else(|| invalid("Missing drawing atom"))?;
            excluded |= atom.isotope != 0
                || atom.map_num != 0
                || atom.stereo.is_some()
                || !atom.marks.is_empty()
                || atom.display.number.is_some();
        }
        if excluded {
            continue;
        }
        let anchor = *at(&molecule.ids, *at(&mapping, 1)?)?;
        used.extend(&members);
        result.abbreviations.push(Abbreviation {
            label: preset.label.clone(),
            reverse_label: preset.reverse_label.clone(),
            anchor,
            members,
        });
    }
    result.version = 15;
    validate(&result)?;
    Ok(result)
}
