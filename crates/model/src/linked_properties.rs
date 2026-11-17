//! Property captions linked to a molecule: one caption, one line per shown
//! property, recalculated in the transaction of every structure edit.
//!
//! The caption is an ordinary annotation, so every export keeps its text.
//! Editing a property line makes it plain text, as with linked chemical names;
//! editing the name line sets a custom name instead. Deleting the molecule
//! removes it.
//!
//! The name line shows a custom name, or the automatic name the app generates
//! with the local name generator, which runs outside the transaction. An edit
//! that changes the structure clears the automatic name; the app writes the
//! new one into the same Undo step when it arrives ([`named`]).
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
    Name,
    Formula,
    Weight,
    ExactMass,
    LogP,
    Tpsa,
    Analysis,
}
impl Property {
    /// The Properties panel's order, which is also the caption's line order.
    pub const ALL: [Self; 7] = [
        Self::Name,
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
            Self::Name => "Name",
            Self::Formula => "Formula",
            Self::Weight => "Mol. Wt.",
            Self::ExactMass => "Exact Mass",
            Self::LogP => "cLogP",
            Self::Tpsa => "TPSA",
            Self::Analysis => "Elemental analysis",
        }
    }
    /// ChemDraw's ChemicalPropertyType: the chemical name from the CDX
    /// documentation, the others checked against ChemDraw-made files. cLogP
    /// and TPSA have no confirmed code.
    pub fn chemdraw(self) -> Option<u32> {
        match self {
            Self::Name => Some(1),
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

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
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
    /// The molecule's canonical identity while it has a name line or a custom
    /// name; empty when it cannot be named.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub smiles: String,
    /// The automatic name of `smiles`; empty until the generator answers.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// A name the user typed, shown instead of the automatic name.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub custom_name: String,
    /// The identity when the custom name was typed.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub custom_smiles: String,
}
impl PropertyCaption {
    /// Whether the molecule's name is tracked: shown, or typed.
    fn tracks_name(&self) -> bool {
        self.properties.contains(&Property::Name) || !self.custom_name.is_empty()
    }

    /// The name line: the custom name, else the automatic one.
    pub fn name_line(&self) -> &str {
        if self.custom_name.is_empty() {
            &self.name
        } else {
            &self.custom_name
        }
    }

    /// The structure changed after the custom name was typed.
    pub fn renamed_structure_changed(&self) -> bool {
        !self.custom_name.is_empty() && self.custom_smiles != self.smiles
    }

    /// A name line the app still has to generate.
    pub fn needs_name(&self) -> bool {
        self.properties.contains(&Property::Name) && self.name.is_empty() && !self.smiles.is_empty()
    }

    /// Follow the molecule's identity. A new structure drops the automatic name.
    fn identify(&mut self, doc: &Document) {
        let smiles = crate::molecule_names::identity(doc, &self.atoms).unwrap_or_default();
        if smiles != self.smiles {
            self.name.clear();
            self.smiles = smiles;
        }
    }
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
fn write(values: Option<&Values>, link: &PropertyCaption) -> (String, Vec<Range<usize>>) {
    let mut text = String::new();
    let mut formulas = Vec::new();
    let mut formula = |text: &mut String, formula: &str| {
        formulas.push(text.len()..text.len() + formula.len());
        text.push_str(formula);
    };
    for &property in Property::ALL.iter().filter(|p| link.properties.contains(p)) {
        if !text.is_empty() {
            text.push('\n');
        }
        if property == Property::Name {
            let name = link.name_line();
            text.push_str(if name.is_empty() { "–" } else { name });
            continue;
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
            Property::Name => {}
            Property::Weight => {
                let places = if link.two_decimal_weight { 2 } else { 3 };
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
    let (text, formulas) = write(values(doc, &link.atoms).as_ref(), link);
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
            // A new or emptied line takes the default style.
            let new = lines.get(property).is_none_or(|line| line.is_empty());
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
            // A name line starts bold, like a compound heading.
            styles.extend(line.char_indices().map(|(i, _)| {
                let mut style = format.at(i).clone();
                if new && *property == Property::Name {
                    StyleChange::Bold(true).apply(&mut style);
                }
                style
            }));
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
    // A new name, typed or generated, takes the style of its first letter
    // throughout, so typing over a bold name keeps it bold.
    let line = |text: &str| text.split('\n').next().unwrap_or_default().to_owned();
    if link.properties.first() == Some(&Property::Name) && line(&link.text) != line(&text) {
        let end = text.find('\n').unwrap_or(text.len());
        let first = caption.format.at(0).clone();
        let styles = text
            .char_indices()
            .map(|(i, _)| {
                if i < end {
                    first.clone()
                } else {
                    caption.format.at(i).clone()
                }
            })
            .collect();
        caption.format.pack(&text, styles);
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
                two_decimal_weight,
                ..Default::default()
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
    if link.tracks_name() {
        link.identify(doc);
    } else {
        // An untracked name would go stale.
        link.name.clear();
        link.smiles.clear();
    }
    // An edited caption is plain text; showing a property links it again.
    rewrite(doc, link, &shown)
}

/// Write the link's caption from its molecule and keep the link.
fn rewrite(
    doc: &mut Document,
    mut link: PropertyCaption,
    shown: &[Property],
) -> Result<(), String> {
    let mut caption = doc
        .annotations
        .iter()
        .find(|a| a.id == link.annotation)
        .cloned()
        .ok_or("The property caption is missing")?;
    refresh(doc, &mut link, &mut caption, shown);
    if let Some(current) = doc.annotations.iter_mut().find(|a| a.id == link.annotation) {
        *current = caption;
    }
    doc.property_captions.push(link);
    Ok(())
}

/// Name the molecule made of `atoms`: a custom name, or the automatic name
/// when `name` is empty or equals it. The name line is shown, with a new
/// caption if the molecule has none.
pub fn rename(
    doc: &mut Document,
    atoms: &[u64],
    name: &str,
    format: TextFormat,
    two_decimal_weight: bool,
) -> Result<(), String> {
    if !shown(doc, atoms, Property::Name) {
        toggle(doc, atoms, Property::Name, format, two_decimal_weight)?;
    }
    let index = doc
        .property_captions
        .iter()
        .position(|link| covers(link, atoms))
        .ok_or("The property caption is missing")?;
    let mut link = doc.property_captions.remove(index);
    let shown = link.properties.clone();
    set_custom(&mut link, name);
    rewrite(doc, link, &shown)
}

/// The name typed over the caption's first line, when the property lines
/// below it are unchanged.
fn typed_name<'a>(link: &PropertyCaption, text: &'a str) -> Option<&'a str> {
    if link.properties.first() != Some(&Property::Name) {
        return None;
    }
    text.strip_suffix(link.text.find('\n').map_or("", |i| &link.text[i..]))
}

/// A typed name, on one line; empty or the automatic name means automatic.
fn set_custom(link: &mut PropertyCaption, name: &str) {
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() || name == link.name {
        link.custom_name.clear();
        link.custom_smiles.clear();
    } else if name != link.custom_name {
        link.custom_name = name;
        link.custom_smiles = link.smiles.clone();
    }
}

/// The generator named `smiles`: write the name into every caption of that
/// structure still waiting for one, without an Undo step of its own. Returns
/// whether a link changed.
pub fn named(doc: &mut Document, smiles: &str, name: &str) -> bool {
    let mut changed = false;
    for mut link in std::mem::take(&mut doc.property_captions) {
        if link.smiles == smiles && link.name.is_empty() && link.tracks_name() {
            link.name = name.into();
            changed = true;
            if let Some(mut caption) = doc
                .annotations
                .iter()
                .find(|a| a.id == link.annotation)
                .cloned()
            {
                let shown = link.properties.clone();
                refresh(doc, &mut link, &mut caption, &shown);
                if let Some(current) = doc.annotations.iter_mut().find(|a| a.id == link.annotation)
                {
                    *current = caption;
                }
            }
        }
        doc.property_captions.push(link);
    }
    changed
}

/// Identify the molecules of name lines read from a file. The name line as
/// saved is the automatic name until the structure changes, so opening a file
/// does not rename it.
pub fn identify_names(doc: &mut Document) {
    let mut links = std::mem::take(&mut doc.property_captions);
    for link in links.iter_mut().filter(|link| link.tracks_name()) {
        link.identify(doc);
        if link.properties.first() == Some(&Property::Name) {
            let line = link.text.split('\n').next().unwrap_or_default();
            if line != "–" {
                link.name = line.into();
            }
        }
    }
    doc.property_captions = links;
}

/// Whether the atoms moved other than all together.
fn reshaped(before: &Document, doc: &Document, atoms: &[u64]) -> bool {
    let positions = |doc: &Document| {
        atoms
            .iter()
            .map(|id| doc.atom(*id).map(|a| a.position))
            .collect::<Option<Vec<_>>>()
    };
    let (Some(old), Some(new)) = (positions(before), positions(doc)) else {
        return true;
    };
    let (Some(a), Some(b)) = (old.first(), new.first()) else {
        return false;
    };
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    old.iter()
        .zip(&new)
        .any(|(a, b)| (b.x - a.x - dx).abs() > 1e-3 || (b.y - a.y - dy).abs() > 1e-3)
}

/// The molecule's atoms, wherever they are drawn, its bonds and any
/// attachment to it.
fn chemistry(doc: &Document, atoms: &[u64]) -> (Vec<Atom>, Vec<Bond>) {
    let atoms: HashSet<_> = atoms.iter().collect();
    (
        doc.atoms
            .iter()
            .filter(|a| atoms.contains(&a.id) || a.centroid.iter().any(|id| atoms.contains(id)))
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
        // Typing over the name line names the molecule; any other change to
        // the text makes it plain text.
        let typed = if caption.text == link.text {
            None
        } else if let Some(name) = typed_name(&link, &caption.text) {
            Some(name.to_owned())
        } else {
            continue;
        };
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
        let same = previous.is_some_and(|old| {
            covers(old, &whole)
                && before.generic == doc.generic
                && chemistry(before, &whole) == chemistry(doc, &whole)
        });
        let mut unchanged = same
            && typed.is_none()
            && previous.is_some_and(|old| {
                old.properties == link.properties
                    && old.two_decimal_weight == link.two_decimal_weight
            })
            && before.drawing_style == doc.drawing_style;
        // Stereo read from the drawing can change with the shape, not with a move.
        let identify = link.tracks_name() && !(same && !reshaped(before, doc, &whole));
        link.atoms = whole;
        if identify {
            let name = link.name.clone();
            link.identify(doc);
            // A dropped name leaves the caption at once.
            unchanged &= link.name == name;
        }
        if let Some(name) = typed {
            set_custom(&mut link, &name);
        }
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
            || link.name.contains('\n')
            || link.custom_name.contains('\n')
        {
            return Err("Invalid property caption link".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
