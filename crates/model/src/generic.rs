//! Finite generic definitions are drawing data until deliberately materialized.
//! Expansion never changes the source and never infers an unspecified polymer.
use crate::document::{Document, Point};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
mod authoring;
mod budget;
mod certify;
mod clearance;
mod definitions;
mod depiction;
mod errors;
mod expansion;
mod geometry;
mod graph;
mod guards;
mod ink;
pub mod limits;
mod materialize;
mod prepared;
pub(crate) mod probe;
mod references;
mod selection;
mod symmetry;
mod variable;

pub(crate) use authoring::split_link_label;
pub use authoring::{alternative, apply_label, define_bracket, set_repeat_unit};
pub use definitions::{prune, selection, validate};
pub use expansion::{expand, expand_drawings};
pub use guards::{atom_references, component_intersects, intersects, touches};
pub use materialize::alternative_fragment;
pub use prepared::{
    ExpansionInput, PreparedExpansion, expand_prepared, prepare_component_expansion_input,
    prepare_expansion_input,
};
pub use selection::component_scope;

use budget::{Work, drawing_size};
#[cfg(test)]
use expansion::expand_drawing_inner;
#[cfg(test)]
use materialize::{clear_after_expansion, link, repeat};
#[cfg(test)]
pub(crate) use prepared::prepare_expansion;

use definitions::{check_range, pair, printable};
use guards::reject_affected_stereo;
use limits::{MAX_LABEL_CHARS, MAX_OUTPUTS, MAX_SOURCE_ATOMS};

/// Polymer flip types that finite expansion and MOL export accept.
pub const EXPANDABLE_FLIPS: [&str; 3] = ["", "NoFlip", "Unspecified"];

/// A detached concrete drawing and its positioned label/object extents. These
/// bounds are transient layout output, never authoritative document metadata.
#[derive(Debug)]
pub struct ExpandedDrawing {
    pub document: Document,
    pub bounds: (Point, Point),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Generic {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alternatives: Vec<AlternativeSite>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<LinkNode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repeats: Vec<RepeatUnit>,
}
impl Generic {
    pub fn is_empty(&self) -> bool {
        self.alternatives.is_empty() && self.links.is_empty() && self.repeats.is_empty()
    }
    /// Whether an alternative or link site owns this atom's label.
    pub fn owns_label(&self, atom: u64) -> bool {
        self.alternatives.iter().any(|site| site.atom == atom)
            || self.links.iter().any(|site| site.atom == atom)
    }
    pub fn extend(&mut self, other: Self) {
        self.alternatives.extend(other.alternatives);
        self.links.extend(other.links);
        self.repeats.extend(other.repeats);
    }
    pub fn remap(&mut self, ids: &HashMap<u64, u64>) -> Option<()> {
        for site in &mut self.alternatives {
            site.atom = *ids.get(&site.atom)?;
        }
        for link in &mut self.links {
            link.atom = *ids.get(&link.atom)?;
        }
        for unit in &mut self.repeats {
            if let Some(id) = &mut unit.bracket {
                *id = *ids.get(id)?;
            }
            for id in &mut unit.bracket_objects {
                *id = *ids.get(id)?;
            }
            for id in &mut unit.members {
                *id = *ids.get(id)?;
            }
            for c in &mut unit.crossings {
                c.inside = *ids.get(&c.inside)?;
                c.outside = *ids.get(&c.outside)?;
            }
            for id in &mut unit.crossing_brackets {
                *id = *ids.get(id)?;
            }
        }
        Some(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Alternative {
    Element {
        symbol: String,
        #[serde(default)]
        isotope: u32,
    },
    Group {
        label: String,
    },
    Unresolved {
        label: String,
    },
}
impl Alternative {
    pub fn display_symbol(&self) -> &str {
        match self {
            Self::Element { symbol, isotope } if symbol == "H" && *isotope == 2 => "D",
            Self::Element { symbol, isotope } if symbol == "H" && *isotope == 3 => "T",
            _ => self.symbol(),
        }
    }

    fn symbol(&self) -> &str {
        match self {
            Self::Element { symbol, .. } => symbol,
            Self::Group { label } | Self::Unresolved { label } => label,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlternativeSite {
    pub atom: u64,
    pub choices: Vec<Alternative>,
    #[serde(default)]
    pub excluded: bool,
}
impl AlternativeSite {
    pub fn label(&self) -> String {
        Self::label_with_symbols(self.excluded, &self.choices, Alternative::display_symbol)
    }

    /// CDXML's anonymous import fallback uses raw symbols; adding D/T aliases
    /// there would change the imported label.
    pub fn label_without_isotope_aliases(excluded: bool, choices: &[Alternative]) -> String {
        Self::label_with_symbols(excluded, choices, Alternative::symbol)
    }

    fn label_with_symbols(
        excluded: bool,
        choices: &[Alternative],
        symbol: fn(&Alternative) -> &str,
    ) -> String {
        format!(
            "[{}{}]",
            if excluded { "NOT " } else { "" },
            choices.iter().map(symbol).collect::<Vec<_>>().join(",")
        )
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkNode {
    pub atom: u64,
    pub min: u32,
    pub max: u32,
}
impl LinkNode {
    pub fn label(&self, element: &str) -> String {
        Self::format_label(element, self.min, self.max)
    }

    pub fn format_label(element: &str, min: u32, max: u32) -> String {
        format!("[{element}]{min}-{max}")
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Crossing {
    pub inside: u64,
    pub outside: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepeatPattern {
    HeadToTail,
    HeadToHead,
    EitherUnknown,
}
impl RepeatPattern {
    pub fn cdxml(self) -> &'static str {
        match self {
            Self::HeadToTail => "HeadToTail",
            Self::HeadToHead => "HeadToHead",
            Self::EitherUnknown => "EitherUnknown",
        }
    }

    pub fn from_cdxml(value: &str) -> Option<Self> {
        match value {
            "HeadToTail" => Some(Self::HeadToTail),
            "HeadToHead" => Some(Self::HeadToHead),
            "EitherUnknown" => Some(Self::EitherUnknown),
            _ => None,
        }
    }

    pub fn molfile_connect(self) -> &'static str {
        match self {
            Self::HeadToTail => "HT",
            Self::HeadToHead => "HH",
            Self::EitherUnknown => "EU",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RepeatCount {
    Finite { min: u32, max: u32 },
    Indefinite { label: String },
}
impl RepeatCount {
    pub fn parse(label: &str) -> Result<Self, String> {
        let label = label.trim();
        // This limit measures bytes; definitions::printable counts characters.
        // Unifying them would change behavior pinned by q-03.
        if label.is_empty() || label.len() > MAX_LABEL_CHARS || label.chars().any(char::is_control)
        {
            return Err("Enter a repeat count or label, such as 1-3 or n".into());
        }
        if let Ok(n) = label.parse::<u32>() {
            check_range(n, n)?;
            return Ok(Self::Finite { min: n, max: n });
        }
        if let Some((a, b)) = label.split_once('-')
            && a.trim().chars().all(|c| c.is_ascii_digit())
            && b.trim().chars().all(|c| c.is_ascii_digit())
        {
            let min = a.trim().parse().map_err(|_| "Invalid repeat count")?;
            let max = b.trim().parse().map_err(|_| "Invalid repeat count")?;
            check_range(min, max)?;
            return Ok(Self::Finite { min, max });
        }
        Ok(Self::Indefinite {
            label: label.into(),
        })
    }
    fn range(&self) -> Result<(u32, u32), String> {
        match self {
            Self::Finite { min, max } => Ok((*min, *max)),
            Self::Indefinite { .. } => {
                Err("An indefinite polymer needs a finite repeat count before expansion".into())
            }
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepeatUnit {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bracket: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bracket_objects: Vec<u64>,
    pub members: Vec<u64>,
    /// Ordered head and tail bonds for an ordinary linear unit. Larger sets
    /// retain ladder/copolymer input without inventing a connection order.
    pub crossings: Vec<Crossing>,
    /// Source bracketattachment graphic IDs, in crossing order. Empty uses
    /// the primary paired graphic for definitions created in ReShiki.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub crossing_brackets: Vec<u64>,
    /// Number of leading crossing bonds explicitly assigned as heads. Absence
    /// preserves an input whose head/tail ports were never specified.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_count: Option<usize>,
    pub usage: String,
    pub pattern: RepeatPattern,
    pub count: RepeatCount,
    pub label: String,
    #[serde(default)]
    pub flip: String,
}
impl RepeatUnit {
    pub fn default_head_count(crossings: usize) -> Option<usize> {
        if crossings == 2 { Some(1) } else { None }
    }
}
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_outputs: usize,
    pub max_total_atoms: usize,
    pub max_work: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_outputs: MAX_OUTPUTS,
            max_total_atoms: MAX_SOURCE_ATOMS,
            max_work: 5_000_000,
        }
    }
}
pub const NOTICE: &str = "Generic choices retained. Expand generic structures to choose concrete atoms, attachment positions or finite repeats before calculating identifiers or molecular properties.";
/// Joining referenced generic structures requires concrete chemical choices first.
pub const JOIN_NOTICE: &str =
    "Expand generic structures before joining referenced generic atoms or repeating units.";

pub fn present(doc: &Document) -> bool {
    !doc.generic.is_empty() || doc.atoms.iter().any(|a| a.is_variable_attachment())
}
fn bonds_at(doc: &Document, id: u64) -> Vec<&crate::document::Bond> {
    doc.bonds
        .iter()
        .filter(|b| b.a == id || b.b == id)
        .collect()
}

#[cfg(test)]
mod drawing_test_support;

#[cfg(test)]
mod test_support;

#[cfg(test)]
mod tests;
