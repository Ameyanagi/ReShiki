//! Property captions linked to a molecule: one caption, one line per shown
//! property, recalculated in the transaction of every structure edit.
//!
//! The caption is an ordinary annotation, so every export keeps its text.
//! Editing the text makes it plain text, as with linked chemical names;
//! deleting the molecule removes it.
use crate::{
    document::{Annotation, Atom, Bond, Document, Point},
    typography::{Script, StyleChange, TextAlign, TextFormat, TextSpan},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Property {
    Formula,
    Weight,
    ExactMass,
    LogP,
    Tpsa,
    Analysis,
}
impl Property {
    /// The Properties panel's order, which is also the caption's line order.
    pub const ALL: [Self; 6] = [
        Self::Formula,
        Self::Weight,
        Self::ExactMass,
        Self::LogP,
        Self::Tpsa,
        Self::Analysis,
    ];
    /// The menu name. Caption lines use the same words, except elemental
    /// analysis, which reads "Anal. Calcd for …".
    pub fn name(self) -> &'static str {
        match self {
            Self::Formula => "Formula",
            Self::Weight => "Mol. Wt.",
            Self::ExactMass => "Exact Mass",
            Self::LogP => "cLogP",
            Self::Tpsa => "TPSA",
            Self::Analysis => "Elemental analysis",
        }
    }
    /// ChemDraw's ChemicalPropertyType, for the types checked against
    /// ChemDraw-made files. cLogP and TPSA have no confirmed code.
    pub fn chemdraw(self) -> Option<u32> {
        match self {
            Self::Formula => Some(2),
            Self::ExactMass => Some(3),
            Self::Weight => Some(4),
            Self::Analysis => Some(6),
            Self::LogP | Self::Tpsa => None,
        }
    }
    pub fn from_chemdraw(code: u32) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.chemdraw() == Some(code))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PropertyCaption {
    pub annotation: u64,
    /// Every atom of the molecule, in drawing order.
    pub atoms: Vec<u64>,
    /// In [`Property::ALL`] order.
    pub properties: Vec<Property>,
    /// Mol. Wt. with two decimals instead of three (the Settings choice).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub two_decimal_weight: bool,
    /// The text last written. Different caption text means the user edited it.
    pub text: String,
}

fn covers(link: &PropertyCaption, atoms: &[u64]) -> bool {
    link.atoms.len() == atoms.len() && atoms.iter().all(|id| link.atoms.contains(id))
}

/// The caption shown for exactly this molecule.
pub fn caption<'a>(doc: &'a Document, atoms: &[u64]) -> Option<&'a PropertyCaption> {
    doc.property_captions
        .iter()
        .find(|link| covers(link, atoms))
}

/// Whether `property` is shown for this molecule.
pub fn shown(doc: &Document, atoms: &[u64], property: Property) -> bool {
    caption(doc, atoms).is_some_and(|link| link.properties.contains(&property))
}

struct Values {
    formula: String,
    weight: f64,
    exact_mass: f64,
    logp: f64,
    tpsa: f64,
    percent: Vec<(String, f64)>,
}

#[cfg(test)]
thread_local! { static CALCULATIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }

/// The Properties panel's calculation, for one molecule.
fn values(doc: &Document, atoms: &[u64]) -> Option<Values> {
    #[cfg(test)]
    CALCULATIONS.with(|c| c.set(c.get() + 1));
    use crate::chemistry::{descriptors, document, properties, rings};
    let molecule = document::prepare(&crate::editing::analysis_document(doc, atoms)).ok()?;
    let graph = &molecule.state.graph;
    let found = properties(&graph.atom_facts().ok()?).ok()?;
    let ring_atoms = rings::perceive(graph, Default::default()).ok()?.atoms;
    let calculated = descriptors::calculate(graph, &ring_atoms).ok()?;
    // Counts become plain digits that the caption's formula style subscripts.
    let mut formula: String = found
        .display_formula
        .chars()
        .map(|c| match c {
            '₀'..='₉' => char::from(b'0' + (u32::from(c) - u32::from('₀')) as u8),
            c => c,
        })
        .collect();
    if doc.drawing_style.charge_before_radical
        && let Some((head, charge)) = formula.split_once("]•")
    {
        formula = format!("{head}]{charge}•");
    }
    Some(Values {
        formula,
        weight: found.mass,
        exact_mass: found.exact_mass,
        logp: calculated.logp,
        tpsa: calculated.tpsa,
        percent: found.mass_percent,
    })
}

/// The caption text and the byte ranges written in formula style.
fn write(
    values: Option<&Values>,
    properties: &[Property],
    two_decimal_weight: bool,
) -> (String, Vec<Range<usize>>) {
    let mut text = String::new();
    let mut formulas = Vec::new();
    let mut formula = |text: &mut String, formula: &str| {
        formulas.push(text.len()..text.len() + formula.len());
        text.push_str(formula);
    };
    for &property in Property::ALL.iter().filter(|p| properties.contains(p)) {
        if !text.is_empty() {
            text.push('\n');
        }
        let Some(v) = values else {
            let label = match property {
                Property::Analysis => "Anal. Calcd",
                _ => property.name(),
            };
            text.push_str(&format!("{label}: –"));
            continue;
        };
        match property {
            Property::Formula => {
                text.push_str("Formula: ");
                formula(&mut text, &v.formula);
            }
            Property::Weight => {
                let places = if two_decimal_weight { 2 } else { 3 };
                text.push_str(&format!("Mol. Wt.: {:.places$}", v.weight));
            }
            Property::ExactMass => text.push_str(&format!("Exact Mass: {:.4}", v.exact_mass)),
            Property::LogP => text.push_str(&format!("cLogP: {:.2}", v.logp)),
            Property::Tpsa => text.push_str(&format!("TPSA: {:.2}", v.tpsa)),
            Property::Analysis => {
                text.push_str("Anal. Calcd for ");
                formula(&mut text, &v.formula);
                let elements: Vec<_> = ["C", "H", "N", "S"]
                    .into_iter()
                    .filter_map(|symbol| {
                        v.percent
                            .iter()
                            .find(|(element, _)| element == symbol)
                            .map(|(_, percent)| format!("{symbol}, {percent:.2}"))
                    })
                    .collect();
                text.push_str(": ");
                if elements.is_empty() {
                    text.push('–');
                }
                text.push_str(&elements.join("; "));
            }
        }
    }
    (text, formulas)
}

/// Rewrite the caption from its molecule, keeping its style and center.
/// `shown` is the properties of the current text. Each line changes like a
/// typed edit of that property's line, so styling stays where the text stays.
fn refresh(
    doc: &Document,
    link: &mut PropertyCaption,
    caption: &mut Annotation,
    shown: &[Property],
) {
    let (text, formulas) = write(
        values(doc, &link.atoms).as_ref(),
        &link.properties,
        link.two_decimal_weight,
    );
    if text != caption.text {
        let center = caption.position.x + caption.size().0 / 2.;
        let mut lines = HashMap::new();
        let mut start = 0;
        for (property, line) in shown.iter().zip(caption.text.split('\n')) {
            lines.insert(*property, start..start + line.len());
            start += line.len() + 1;
        }
        let mut styles = vec![];
        for (property, line) in link.properties.iter().zip(text.split('\n')) {
            if !styles.is_empty() {
                styles.push(caption.format.style.clone());
            }
            let old = lines.get(property).cloned().unwrap_or_default();
            let mut format = caption.format.clone();
            format.spans = caption
                .format
                .spans
                .iter()
                .filter(|s| s.start < old.end && old.start < s.end)
                .map(|s| TextSpan {
                    start: s.start.max(old.start) - old.start,
                    end: s.end.min(old.end) - old.start,
                    style: s.style.clone(),
                })
                .collect();
            let old = &caption.text[old];
            // An empty range at the end makes `edited` find the changed part.
            format.edited(old, line, old.len()..old.len());
            styles.extend(line.char_indices().map(|(i, _)| format.at(i).clone()));
        }
        for ((i, _), style) in text.char_indices().zip(&mut styles) {
            if formulas.iter().any(|r| r.contains(&i)) {
                StyleChange::Formula(true).apply(style);
            }
        }
        caption.format.pack(&text, styles);
        caption.text = text.clone();
        caption.position.x = center - caption.size().0 / 2.;
    }
    link.text = text;
}

/// The molecule's bottom center, which its caption follows.
fn anchor(doc: &Document, atoms: &[u64]) -> Option<Point> {
    let (lo, hi) = crate::scene::selection_bounds(doc, atoms)?;
    Some(Point::new((lo.x + hi.x) / 2., hi.y))
}

/// Show or hide one property of the molecule made of `atoms`, which must be
/// one whole connected component. The first property adds the caption below
/// the molecule. Hiding the last removes it.
pub fn toggle(
    doc: &mut Document,
    atoms: &[u64],
    property: Property,
    mut format: TextFormat,
    two_decimal_weight: bool,
) -> Result<(), String> {
    let whole = atoms
        .first()
        .map_or(vec![], |id| crate::molecule_names::component(doc, *id));
    if whole.is_empty() || whole.len() != atoms.len() || atoms.iter().any(|id| !whole.contains(id))
    {
        return Err("Select one whole molecule to show its properties".into());
    }
    let index = doc
        .property_captions
        .iter()
        .position(|link| covers(link, &whole));
    let mut link = match index {
        Some(index) => doc.property_captions.remove(index),
        None => {
            let (_, hi) =
                crate::scene::selection_bounds(doc, atoms).ok_or("The molecule has no bounds")?;
            format.spans.clear();
            format.style.formula = false;
            format.style.script = Script::Normal;
            format.style.size_pt = format.style.size_pt.max(doc.drawing_style.font_size_pt);
            format.alignment = TextAlign::Center;
            format.width_pt = None;
            format.name_breaks = false;
            let id = doc.next_id();
            let gap = doc.drawing_style.font_size().max(12.) * 0.5;
            let center = anchor(doc, atoms).map_or(0., |p| p.x);
            doc.annotations.push(Annotation {
                id,
                position: Point::new(center, hi.y + gap),
                text: String::new(),
                format,
            });
            PropertyCaption {
                annotation: id,
                atoms: vec![],
                properties: vec![],
                two_decimal_weight,
                text: String::new(),
            }
        }
    };
    let shown = link.properties.clone();
    if link.properties.contains(&property) {
        link.properties.retain(|p| *p != property);
    } else {
        link.properties.push(property);
        link.properties
            .sort_by_key(|p| Property::ALL.iter().position(|q| q == p));
    }
    if link.properties.is_empty() {
        doc.delete(&[link.annotation]);
        return Ok(());
    }
    link.atoms = whole;
    link.two_decimal_weight = two_decimal_weight;
    let mut caption = doc
        .annotations
        .iter()
        .find(|a| a.id == link.annotation)
        .cloned()
        .ok_or("The property caption is missing")?;
    // An edited caption is plain text; showing a property links it again.
    refresh(doc, &mut link, &mut caption, &shown);
    if let Some(current) = doc.annotations.iter_mut().find(|a| a.id == link.annotation) {
        *current = caption;
    }
    doc.property_captions.push(link);
    Ok(())
}

/// The molecule's atoms, wherever they are drawn, and its bonds.
fn chemistry(doc: &Document, atoms: &[u64]) -> (Vec<Atom>, Vec<Bond>) {
    let atoms: HashSet<_> = atoms.iter().collect();
    (
        doc.atoms
            .iter()
            .filter(|a| atoms.contains(&a.id))
            .map(|a| Atom {
                position: Point::default(),
                ..a.clone()
            })
            .collect(),
        doc.bonds
            .iter()
            .filter(|b| atoms.contains(&b.a) || atoms.contains(&b.b))
            .cloned()
            .collect(),
    )
}

/// Called in the same transaction as the edit: each linked caption follows its
/// molecule's atoms and position. Edited text unlinks it. A deleted molecule
/// removes its caption. Molecules joined together keep the first caption, and
/// a split molecule keeps its caption on the largest piece. Only a molecule
/// whose atoms or bonds changed is recalculated; an empty `before`
/// recalculates every caption.
pub fn reconcile(doc: &mut Document, before: &Document) {
    let links = std::mem::take(&mut doc.property_captions);
    let mut claimed = HashSet::new();
    for mut link in links {
        let Some(mut caption) = doc
            .annotations
            .iter()
            .find(|a| a.id == link.annotation)
            .cloned()
        else {
            continue;
        };
        if caption.text != link.text {
            continue;
        }
        let mut seen = HashSet::new();
        let mut whole = vec![];
        for id in &link.atoms {
            if seen.insert(*id) {
                let piece = crate::molecule_names::component(doc, *id);
                seen.extend(piece.iter().copied());
                if piece.len() > whole.len() {
                    whole = piece;
                }
            }
        }
        if whole.is_empty() || whole.iter().any(|id| claimed.contains(id)) {
            doc.delete(&[link.annotation]);
            continue;
        }
        claimed.extend(whole.iter().copied());
        let previous = before
            .property_captions
            .iter()
            .find(|old| old.annotation == link.annotation);
        let shift = previous
            .filter(|_| {
                before
                    .annotations
                    .iter()
                    .any(|a| a.id == link.annotation && a.position == caption.position)
            })
            .and_then(|old| anchor(before, &old.atoms));
        let unchanged = previous.is_some_and(|old| {
            old.properties == link.properties
                && old.two_decimal_weight == link.two_decimal_weight
                && before.drawing_style == doc.drawing_style
                && before.generic == doc.generic
                && chemistry(before, &whole) == chemistry(doc, &whole)
        });
        link.atoms = whole;
        if !unchanged {
            let shown = link.properties.clone();
            refresh(doc, &mut link, &mut caption, &shown);
        }
        if let Some((from, to)) = shift.zip(anchor(doc, &link.atoms)) {
            caption.position = caption.position.offset(to.x - from.x, to.y - from.y);
        }
        if let Some(current) = doc.annotations.iter_mut().find(|a| a.id == link.annotation) {
            *current = caption;
        }
        doc.property_captions.push(link);
    }
}

/// Prune links after explicit deletion: a caption whose molecule is gone goes
/// with it, unless its text was edited.
pub fn prune(doc: &mut Document) {
    let mut removed = Vec::new();
    let atoms: HashSet<_> = doc.atoms.iter().map(|a| a.id).collect();
    doc.property_captions.retain_mut(|link| {
        link.atoms.retain(|id| atoms.contains(id));
        let caption = doc.annotations.iter().find(|a| a.id == link.annotation);
        if link.atoms.is_empty() && caption.is_some_and(|a| a.text == link.text) {
            removed.push(link.annotation);
        }
        !link.atoms.is_empty() && caption.is_some()
    });
    doc.annotations.retain(|a| !removed.contains(&a.id));
}

pub fn validate(doc: &Document) -> Result<(), String> {
    let mut captions = HashSet::new();
    let mut atoms = HashSet::new();
    for link in &doc.property_captions {
        let mut properties = HashSet::new();
        if !captions.insert(link.annotation)
            || !doc.annotations.iter().any(|a| a.id == link.annotation)
            || doc
                .molecule_names
                .iter()
                .any(|n| n.annotation == link.annotation)
            || link.atoms.is_empty()
            || link
                .atoms
                .iter()
                .any(|id| !atoms.insert(*id) || doc.atom(*id).is_none())
            || link.properties.is_empty()
            || !link.properties.iter().all(|p| properties.insert(*p))
            || link.text.is_empty()
        {
            return Err("Invalid property caption link".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
