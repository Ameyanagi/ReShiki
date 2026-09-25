//! Collapsed labels are a view of real atoms, never a replacement for chemistry.
use crate::document::{Document, Point};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Abbreviation {
    pub label: String,
    #[serde(default)]
    pub reverse_label: String,
    pub anchor: u64,
    pub members: Vec<u64>,
    #[serde(default, skip_serializing_if = "LabelAlignment::is_auto")]
    pub alignment: LabelAlignment,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LabelAlignment {
    #[default]
    Auto,
    Left,
    Center,
    Right,
    Above,
}
impl LabelAlignment {
    pub const ALL: [Self; 5] = [
        Self::Auto,
        Self::Left,
        Self::Center,
        Self::Right,
        Self::Above,
    ];
    pub fn is_auto(&self) -> bool {
        *self == Self::Auto
    }
    pub fn cdxml(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Left => "Left",
            Self::Center => "Center",
            Self::Right => "Right",
            Self::Above => "Above",
        }
    }
    pub fn from_cdxml(value: &str) -> Result<Self, String> {
        match value {
            "Auto" | "Best" => Ok(Self::Auto),
            "Left" => Ok(Self::Left),
            "Center" => Ok(Self::Center),
            "Right" => Ok(Self::Right),
            "Above" => Ok(Self::Above),
            _ => Err("Unsupported abbreviation label alignment".into()),
        }
    }
}
impl std::fmt::Display for LabelAlignment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Auto => "Automatic",
            Self::Left => "Flush left",
            Self::Center => "Centered",
            Self::Right => "Flush right",
            Self::Above => "Stacked above",
        })
    }
}

pub const PRESETS: &[&str] = &[
    "OMe", "OEt", "Me", "Et", "nPr", "iPr", "nBu", "tBu", "Ph", "Bn", "Boc", "Cbz", "Fmoc", "Ac",
    "OAc", "Bz", "OBz", "Ts", "OTs", "Ms", "OMs", "TMS", "TBS", "CF3", "CN", "NO2", "CO2H",
    "CO2Me", "CO2Et",
];

impl Abbreviation {
    pub fn validate(&self, doc: &Document) -> Result<(), String> {
        for label in [&self.label, &self.reverse_label] {
            if label.chars().count() > 32 || label.chars().any(char::is_control) {
                return Err(
                    "Abbreviation labels must contain at most 32 printable characters".into(),
                );
            }
        }
        if self.label.trim().is_empty() || self.members.is_empty() {
            return Err("An abbreviation needs a label and a connected fragment".into());
        }
        let members: HashSet<_> = self.members.iter().copied().collect();
        if members.len() != self.members.len()
            || !members.contains(&self.anchor)
            || members.iter().any(|id| doc.atom(*id).is_none())
        {
            return Err("Invalid abbreviation atoms or attachment".into());
        }
        let mut reached = HashSet::from([self.anchor]);
        for (a, b) in crate::attachments::edges(doc) {
            if members.contains(&a) != members.contains(&b) && a != self.anchor && b != self.anchor
            {
                return Err(
                    "Only the abbreviation's attachment atom can connect outside it".into(),
                );
            }
        }
        loop {
            let before = reached.len();
            for (a, b) in crate::attachments::edges(doc) {
                if members.contains(&a)
                    && members.contains(&b)
                    && (reached.contains(&a) || reached.contains(&b))
                {
                    reached.extend([a, b]);
                }
            }
            if reached.len() == before {
                break;
            }
        }
        if reached.len() != members.len() {
            return Err("Select a connected fragment to abbreviate".into());
        }
        Ok(())
    }

    pub fn faces_left(&self, doc: &Document) -> bool {
        match self.alignment {
            LabelAlignment::Left => return false,
            LabelAlignment::Right => return true,
            _ => {}
        }
        let Some(anchor) = doc.atom(self.anchor) else {
            return false;
        };
        doc.bonds
            .iter()
            .find_map(|b| {
                let other = if b.a == self.anchor {
                    b.b
                } else if b.b == self.anchor {
                    b.a
                } else {
                    return None;
                };
                (!self.members.contains(&other))
                    .then(|| doc.atom(other))
                    .flatten()
            })
            .is_some_and(|a| a.position.x > anchor.position.x + 0.1)
    }

    pub fn text<'a>(&'a self, doc: &Document) -> &'a str {
        if self.faces_left(doc) && !self.reverse_label.trim().is_empty() {
            &self.reverse_label
        } else {
            &self.label
        }
    }
}

/// The bond belongs to an element glyph, never to a following subscript.
/// Nicknames that are not formulas retain their edge-character alignment.
pub(crate) fn anchor_range(text: &str, left: bool) -> std::ops::Range<usize> {
    let mut tokens = Vec::new();
    let mut chars = text.char_indices().peekable();
    let mut formula = true;
    while let Some((start, c)) = chars.next() {
        if !c.is_ascii_uppercase() {
            formula = false;
            break;
        }
        let mut end = start + c.len_utf8();
        if let Some(&(i, c)) = chars.peek().filter(|(_, c)| c.is_ascii_lowercase()) {
            end = i + c.len_utf8();
            chars.next();
        }
        let symbol = text.get(start..end).unwrap_or_default();
        if !crate::editing::ELEMENTS.contains(&symbol) {
            formula = false;
            break;
        }
        if symbol != "H" {
            tokens.push(start..end);
        }
        while chars
            .peek()
            .is_some_and(|(_, c)| c.is_ascii_digit() || ('₀'..='₉').contains(c))
        {
            chars.next();
        }
    }
    if formula && let Some(range) = if left { tokens.last() } else { tokens.first() } {
        return range.clone();
    }
    let character = if left {
        text.char_indices().next_back()
    } else {
        text.char_indices().next()
    };
    character.map(|(i, c)| i..i + c.len_utf8()).unwrap_or(0..0)
}

impl Document {
    pub fn abbreviation(&self, anchor: u64) -> Option<&Abbreviation> {
        self.abbreviations.iter().find(|a| a.anchor == anchor)
    }
    pub fn atom_visible(&self, id: u64) -> bool {
        !self
            .abbreviations
            .iter()
            .any(|a| a.anchor != id && a.members.contains(&id))
    }
    pub fn bond_visible(&self, a: u64, b: u64) -> bool {
        self.atom_visible(a)
            && self.atom_visible(b)
            && !self
                .abbreviations
                .iter()
                .any(|g| g.members.contains(&a) && g.members.contains(&b))
    }
    pub fn expand_abbreviation_selection(&self, ids: &[u64]) -> Vec<u64> {
        let mut selected: HashSet<_> = ids.iter().copied().collect();
        for group in &self.abbreviations {
            if group.members.iter().any(|id| selected.contains(id)) {
                selected.extend(&group.members);
            }
        }
        self.all_ids()
            .into_iter()
            .filter(|id| selected.contains(id))
            .collect()
    }
    pub fn validate_abbreviations(&self) -> Result<(), String> {
        if self.version < 10 && !self.abbreviations.is_empty() {
            return Err("Abbreviations require document version 10".into());
        }
        let mut used = HashSet::new();
        for abbreviation in &self.abbreviations {
            abbreviation.validate(self)?;
            if abbreviation.members.iter().any(|id| !used.insert(*id)) {
                return Err("Abbreviations cannot overlap".into());
            }
        }
        Ok(())
    }
    pub fn contract(
        &mut self,
        ids: &[u64],
        label: &str,
        reverse_label: &str,
    ) -> Result<(), String> {
        let members: Vec<_> = self
            .expand_abbreviation_selection(ids)
            .into_iter()
            .filter(|id| self.atom(*id).is_some())
            .collect();
        let anchor = self
            .bonds
            .iter()
            .find_map(|b| {
                if members.contains(&b.a) != members.contains(&b.b) {
                    Some(if members.contains(&b.a) { b.a } else { b.b })
                } else {
                    None
                }
            })
            .or_else(|| members.first().copied())
            .ok_or("Select atoms to abbreviate")?;
        let abbreviation = Abbreviation {
            alignment: Default::default(),
            label: label.trim().into(),
            reverse_label: reverse_label.trim().into(),
            anchor,
            members,
        };
        abbreviation.validate(self)?;
        self.abbreviations
            .retain(|a| !a.members.iter().any(|id| abbreviation.members.contains(id)));
        self.abbreviations.push(abbreviation);
        self.version = self.version.max(11);
        Ok(())
    }
    pub fn expand_abbreviations(&mut self, ids: &[u64]) -> usize {
        let before = self.abbreviations.len();
        self.abbreviations
            .retain(|a| !a.members.iter().any(|id| ids.contains(id)));
        before - self.abbreviations.len()
    }
    /// Topology/element changes reveal affected groups so labels never conceal
    /// an edit to their original chemical definition. Geometry/styles stay collapsed.
    pub fn reconcile_abbreviations(&mut self, before: &Document) {
        let keep: Vec<_> = self
            .abbreviations
            .iter()
            .filter(|g| {
                if g.validate(self).is_err() {
                    return false;
                }
                if !before.abbreviations.contains(g) {
                    return true;
                }
                let atoms_unchanged = g.members.iter().all(|id| {
                    self.atom(*id).zip(before.atom(*id)).is_some_and(|(a, b)| {
                        (
                            &a.element,
                            a.charge,
                            a.isotope,
                            a.explicit_h,
                            a.no_implicit,
                            a.radical_electrons,
                            a.map_num,
                            a.attachment,
                            &a.centroid,
                        ) == (
                            &b.element,
                            b.charge,
                            b.isotope,
                            b.explicit_h,
                            b.no_implicit,
                            b.radical_electrons,
                            b.map_num,
                            b.attachment,
                            &b.centroid,
                        )
                    })
                });
                let bonds = |d: &Document| {
                    let mut bonds: Vec<_> = d
                        .bonds
                        .iter()
                        .filter(|b| g.members.contains(&b.a) || g.members.contains(&b.b))
                        .map(|b| (b.a.min(b.b), b.a.max(b.b), b.order))
                        .collect();
                    bonds.sort_unstable();
                    bonds
                };
                atoms_unchanged && bonds(self) == bonds(before)
            })
            .cloned()
            .collect();
        self.abbreviations = keep;
    }
}

pub fn label_hit(doc: &Document, point: Point, radius: f32) -> Option<u64> {
    doc.abbreviations.iter().rev().find_map(|g| {
        let atom = doc.atom(g.anchor)?;
        let (lo, hi) = crate::scene::atom_label_bounds(atom, doc)?;
        (point.x >= lo.x - radius
            && point.x <= hi.x + radius
            && point.y >= lo.y - radius
            && point.y <= hi.y + radius)
            .then_some(g.anchor)
    })
}
