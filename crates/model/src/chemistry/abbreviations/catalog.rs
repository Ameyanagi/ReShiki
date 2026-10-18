//! The abbreviation catalog: one human-editable JSON file of labels and SMILES.
//! Matching queries, formulas and template geometry are all derived from each
//! entry's SMILES, so adding a group means adding one entry.
use super::{Error, Result, invalid, matcher};
use crate::chemistry::{
    ELEMENTS, depict,
    smiles::{self, Prepared},
    stereo::Point3,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    sync::{Arc, OnceLock},
};

/// The bundled catalog, in detection tie-break order.
pub const BUILTIN: &str = include_str!("catalog.json");
pub const FORMAT: u32 = 1;

/// One catalog group as written in `catalog.json` or a user catalog file.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    /// Label shown on the canvas and typed on an endpoint.
    pub label: String,
    /// Label shown when the attachment bond leaves to the right (`TBSO`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reverse: String,
    /// SMILES whose first atom is the `*` attachment dummy.
    pub smiles: String,
    /// Optional Hill formula of the substituent, checked against the SMILES.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formula: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub category: String,
    /// Full names and other search terms; never typed as labels.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub names: Vec<String>,
    /// Other typed spellings that create this group under that spelling.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    /// Whether Contract common groups looks for this group.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub contract: bool,
}
fn yes() -> bool {
    true
}
fn is_true(value: &bool) -> bool {
    *value
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct File {
    pub format: u32,
    pub groups: Vec<Entry>,
}

/// A loaded catalog group: its entry metadata plus the derived native query.
/// Serialization is the RDKit-comparable query used by the reference tests.
#[derive(Debug, Serialize)]
pub struct Preset {
    pub label: String,
    pub reverse_label: String,
    pub smiles: String,
    pub(super) atoms: Vec<matcher::Atom>,
    pub(super) bonds: Vec<matcher::Bond>,
    #[serde(skip)]
    pub category: String,
    #[serde(skip)]
    pub names: Vec<String>,
    #[serde(skip)]
    pub aliases: Vec<String>,
    #[serde(skip)]
    pub formula: String,
    #[serde(skip)]
    pub contract: bool,
    /// Loaded from the user's catalog rather than the bundled file.
    #[serde(skip)]
    pub user: bool,
    #[serde(skip)]
    geometry: OnceLock<std::result::Result<Vec<Point3>, String>>,
}

pub type Catalog = Arc<[Arc<Preset>]>;

fn check_label(text: &str, what: &str, optional: bool) -> std::result::Result<(), String> {
    if text.is_empty() && optional {
        return Ok(());
    }
    if text.is_empty()
        || text.chars().count() > 32
        || text
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || ('\u{1c}'..='\u{1f}').contains(&c))
    {
        return Err(format!(
            "{what} \"{text}\" must be 1-32 characters without spaces"
        ));
    }
    Ok(())
}

impl Preset {
    /// Validate one entry and derive its query. Errors name the problem in
    /// words a user editing the catalog file can act on.
    pub fn from_entry(entry: Entry, user: bool) -> std::result::Result<Self, String> {
        check_label(&entry.label, "Label", false)?;
        check_label(&entry.reverse, "Reverse label", true)?;
        for alias in &entry.aliases {
            check_label(alias, "Alias", false)?;
        }
        if entry.smiles.matches('*').count() != 1 || !entry.smiles.starts_with('*') {
            return Err("SMILES must start with the single * attachment point".into());
        }
        let prepared =
            smiles::prepare(&entry.smiles).map_err(|error| format!("Invalid SMILES: {error}"))?;
        check_fragment(&prepared)?;
        let (atoms, bonds) = matcher::query(&prepared.state).map_err(|e| e.to_string())?;
        let formula = substituent_formula(&prepared)?;
        if let Some(expected) = &entry.formula
            && *expected != formula
        {
            return Err(format!(
                "Formula {expected} does not match the SMILES ({formula})"
            ));
        }
        let preset = Self {
            label: entry.label,
            reverse_label: entry.reverse,
            smiles: entry.smiles,
            atoms,
            bonds,
            category: entry.category,
            names: entry.names,
            aliases: entry.aliases,
            formula,
            contract: entry.contract,
            user,
            geometry: OnceLock::new(),
        };
        matcher::validate(&preset).map_err(|_| {
            "Groups need 1-31 connected atoms, one single attachment and at most 4 rings per atom"
                .to_string()
        })?;
        Ok(preset)
    }

    /// Template coordinates from ReShiki's own depiction of the SMILES, the
    /// same layout engine used for SMILES import. Index 0 is the dummy.
    pub fn positions(&self) -> Result<&[Point3]> {
        self.geometry
            .get_or_init(|| {
                let prepared = smiles::prepare(&self.smiles).map_err(|e| e.to_string())?;
                let ranks = prepared
                    .state
                    .properties
                    .atoms
                    .iter()
                    .map(|atom| atom.cip_rank)
                    .collect::<Vec<_>>();
                let conformer =
                    depict::compute(&prepared.state, &ranks, None, depict::Options::default())
                        .map_err(|e| e.to_string())?;
                if conformer.positions.len() != self.atoms.len()
                    || conformer
                        .positions
                        .iter()
                        .any(|p| !p.x.is_finite() || !p.y.is_finite() || p.z != 0.0)
                {
                    return Err("Invalid template geometry".into());
                }
                Ok(conformer.positions)
            })
            .as_ref()
            .map(Vec::as_slice)
            .map_err(|error| invalid(format!("{}: {error}", self.label)))
    }

    /// Typed spellings: the label first, then aliases.
    pub fn spellings(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.label.as_str()).chain(self.aliases.iter().map(String::as_str))
    }
}

/// Reject what a planar collapsed group cannot carry: stereo, atom maps,
/// enhanced-stereo groups, or a second fragment.
fn check_fragment(prepared: &Prepared) -> std::result::Result<(), String> {
    let state = &prepared.state;
    let graph = &state.graph;
    // Replacement keeps the drawing's single attachment bond.
    if graph
        .bonds
        .iter()
        .any(|bond| (bond.a == 0 || bond.b == 0) && bond.order != 1)
    {
        return Err("The * attachment must use a single bond".into());
    }
    if graph
        .atoms
        .first()
        .is_none_or(|atom| atom.atomic_number != 0)
    {
        return Err("SMILES must start with the * attachment point".into());
    }
    if graph
        .atoms
        .iter()
        .skip(1)
        .any(|atom| atom.atomic_number == 0)
    {
        return Err("Only one * attachment point is allowed".into());
    }
    if state
        .metadata
        .atoms
        .iter()
        .any(|a| a.chiral_tag != 0 || a.map_present)
        || state
            .metadata
            .bonds
            .iter()
            .any(|b| b.stereo != 0 || !b.stereo_atoms.is_empty())
        || !state.metadata.groups.is_empty()
    {
        return Err("Stereo and atom-map annotations are not supported in groups".into());
    }
    Ok(())
}

/// Hill formula of the substituent R in *-R, with a net charge suffix.
fn substituent_formula(prepared: &Prepared) -> std::result::Result<String, String> {
    let state = &prepared.state;
    let mut counts = BTreeMap::<&str, u32>::new();
    let mut charge = 0i32;
    for (atom, valence) in state.graph.atoms.iter().zip(&state.valences).skip(1) {
        let symbol = ELEMENTS
            .get(usize::from(atom.atomic_number))
            .ok_or("Unknown element")?
            .symbol;
        *counts.entry(symbol).or_default() += 1;
        let hydrogens = u32::from(atom.explicit_hydrogens) + valence.implicit_hydrogens;
        if hydrogens > 0 {
            *counts.entry("H").or_default() += hydrogens;
        }
        charge += i32::from(atom.charge);
    }
    let mut text = String::new();
    let mut push = |symbol: &str, count: u32| {
        text.push_str(symbol);
        if count > 1 {
            text.push_str(&count.to_string());
        }
    };
    let carbon = counts.remove("C");
    if let Some(count) = carbon {
        push("C", count);
        if let Some(count) = counts.remove("H") {
            push("H", count);
        }
    }
    for (symbol, count) in counts {
        push(symbol, count);
    }
    match charge {
        0 => {}
        1 => text.push('+'),
        -1 => text.push('-'),
        c if c > 0 => text.push_str(&format!("{c}+")),
        c => text.push_str(&format!("{}-", -c)),
    }
    Ok(text)
}

/// Parse a catalog file. Each invalid entry is reported with its position
/// and label; the valid entries are still returned.
pub fn parse(text: &str, user: bool) -> (Vec<Preset>, Vec<String>) {
    let file: File = match serde_json::from_str(text) {
        Ok(file) => file,
        Err(error) => return (Vec::new(), vec![format!("Invalid catalog file: {error}")]),
    };
    if file.format != FORMAT {
        return (
            Vec::new(),
            vec![format!(
                "Unsupported catalog format {} (expected {FORMAT})",
                file.format
            )],
        );
    }
    let mut presets = Vec::new();
    let mut errors = Vec::new();
    let mut seen = HashSet::new();
    for (index, entry) in file.groups.into_iter().enumerate() {
        let name = format!("Group {} ({})", index + 1, entry.label);
        let spellings = std::iter::once(entry.label.clone())
            .chain(entry.aliases.iter().cloned())
            .collect::<Vec<_>>();
        if let Some(clash) = spellings.iter().find(|s| seen.contains(*s)) {
            errors.push(format!("{name}: \"{clash}\" is defined twice"));
            continue;
        }
        match Preset::from_entry(entry, user) {
            Ok(preset) => {
                seen.extend(spellings);
                presets.push(preset);
            }
            Err(error) => errors.push(format!("{name}: {error}")),
        }
    }
    (presets, errors)
}

fn builtin() -> Result<&'static Catalog> {
    static DATA: OnceLock<Result<Catalog>> = OnceLock::new();
    DATA.get_or_init(|| {
        let (presets, errors) = parse(BUILTIN, false);
        if let Some(error) = errors.first() {
            return Err(invalid(format!("Bundled catalog: {error}")));
        }
        Ok(presets.into_iter().map(Arc::new).collect())
    })
    .as_ref()
    .map_err(Error::clone)
}

/// The active catalog in detection tie-break order.
pub fn presets() -> Result<Catalog> {
    builtin().cloned()
}

/// Find the group created by a typed spelling. The flag is true for an alias.
pub fn lookup(text: &str) -> Result<Option<(Arc<Preset>, bool)>> {
    Ok(presets()?.iter().find_map(|preset| {
        if preset.label == text {
            Some((preset.clone(), false))
        } else if preset.aliases.iter().any(|alias| alias == text) {
            Some((preset.clone(), true))
        } else {
            None
        }
    }))
}

#[cfg(test)]
mod tests;
