//! Property captions linked to a molecule: one caption, one line per shown
//! property, recalculated in the transaction of every structure edit.
//!
//! The caption is an ordinary annotation, so every export keeps its text.
//! Editing a property line makes it plain text, as with linked chemical names;
//! editing the name line sets a custom name instead. Deleting the molecule
//! removes it. A custom name stays with the molecule when its caption is
//! hidden or becomes plain text.
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
    /// The lines "Show formula & masses below" adds.
    pub const MASSES: [Self; 3] = [Self::Formula, Self::Weight, Self::ExactMass];
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
    /// ChemDraw's ChemicalPropertyType, for the types checked against
    /// ChemDraw-made files. cLogP and TPSA have no confirmed code. The CDX
    /// documentation lists 1 for a chemical name, but ChemDraw 26 stops
    /// updating linked text that includes it, so the name has none.
    pub fn chemdraw(self) -> Option<u32> {
        match self {
            Self::Name | Self::LogP | Self::Tpsa => None,
            Self::Formula => Some(2),
            Self::ExactMass => Some(3),
            Self::Weight => Some(4),
            Self::Analysis => Some(6),
        }
    }
    pub fn from_chemdraw(code: u32) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.chemdraw() == Some(code))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PropertyCaption {
    /// The caption; none when the link only keeps a custom name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotation: Option<u64>,
    /// Every atom of the molecule, in drawing order.
    pub atoms: Vec<u64>,
    /// In [`Property::ALL`] order.
    pub properties: Vec<Property>,
    /// Mol. Wt. with two decimals instead of three (the Settings choice).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub two_decimal_weight: bool,
    /// Decimal places chosen for this caption's Mol. Wt.; none follows
    /// `two_decimal_weight`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight_places: Option<u8>,
    /// Decimal places chosen for this caption's Exact Mass; none is four.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exact_mass_places: Option<u8>,
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
    /// The most decimal places a mass line can choose.
    pub const MAX_PLACES: u8 = 5;

    fn default_places(&self, property: Property) -> u8 {
        match property {
            Property::Weight if !self.two_decimal_weight => 3,
            Property::ExactMass => 4,
            _ => 2,
        }
    }

    /// The decimal places a value line is written with.
    pub fn places(&self, property: Property) -> u8 {
        match property {
            Property::Weight => self.weight_places,
            Property::ExactMass => self.exact_mass_places,
            _ => None,
        }
        .unwrap_or_else(|| self.default_places(property))
    }

    /// Choose the decimal places of Mol. Wt. or Exact Mass, from 0 to
    /// [`Self::MAX_PLACES`]. The default (Mol. Wt.: `two_decimal_weight`, the
    /// current Settings choice; Exact Mass: four) is kept as no choice, so the
    /// file stays readable by older versions and Mol. Wt. follows Settings.
    /// Choosing the places already shown changes nothing.
    pub fn set_places(&mut self, property: Property, places: u8, two_decimal_weight: bool) {
        if places == self.places(property) {
            return;
        }
        match property {
            Property::Weight => {
                let default = if two_decimal_weight { 2 } else { 3 };
                if places == default {
                    self.two_decimal_weight = two_decimal_weight;
                }
                self.weight_places = (places != default).then_some(places);
            }
            Property::ExactMass => {
                self.exact_mass_places =
                    (places != self.default_places(property)).then_some(places);
            }
            _ => {}
        }
    }

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

    /// Hide the caption. Returns whether the link stays to keep a custom name.
    fn detach(&mut self) -> bool {
        self.annotation = None;
        self.properties.clear();
        self.text.clear();
        self.weight_places = None;
        self.exact_mass_places = None;
        !self.custom_name.is_empty()
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
            Property::Weight | Property::ExactMass | Property::LogP | Property::Tpsa => {
                let value = match property {
                    Property::Weight => v.weight,
                    Property::ExactMass => v.exact_mass,
                    Property::LogP => v.logp,
                    _ => v.tpsa,
                };
                let places = usize::from(link.places(property));
                text.push_str(&format!("{}: {value:.places$}", property.name()));
            }
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
    let width = caption.size().0;
    if text != caption.text {
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
            let mut format = part(&caption.format, old.clone());
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
    wrap(doc, link, caption);
    // The caption stays centered under the molecule.
    caption.position.x += (width - caption.size().0) / 2.;
    link.text = text;
}

/// The format of part of a caption, unwrapped.
fn part(format: &TextFormat, range: Range<usize>) -> TextFormat {
    TextFormat {
        spans: format
            .spans
            .iter()
            .filter(|s| s.start < range.end && range.start < s.end)
            .map(|s| TextSpan {
                start: s.start.max(range.start) - range.start,
                end: s.end.min(range.end) - range.start,
                style: s.style.clone(),
            })
            .collect(),
        width_pt: None,
        ..format.clone()
    }
}

/// A name line wraps like a shown chemical name: to the molecule's width,
/// between three and six bond lengths. Property lines never wrap. A caption
/// without a name line keeps its own width, unless it is one this set.
fn wrap(doc: &Document, link: &PropertyCaption, caption: &mut Annotation) {
    let bounds = (link.properties.first() == Some(&Property::Name))
        .then(|| crate::scene::selection_bounds(doc, &link.atoms))
        .flatten();
    let Some((lo, hi)) = bounds else {
        if caption.format.name_breaks {
            caption.format.width_pt = None;
            caption.format.name_breaks = false;
        }
        return;
    };
    let text = &caption.text;
    let natural = |range: Range<usize>| {
        crate::typography::layout(&text[range.clone()], &part(&caption.format, range)).width
    };
    let end = text.find('\n').unwrap_or(text.len());
    let bond = doc.drawing_style.bond_length_world;
    let name = (hi.x - lo.x)
        .max(bond * 3.)
        .max(natural(0..end).min(bond * 6.));
    let properties = if end < text.len() {
        natural(end + 1..text.len())
    } else {
        0.
    };
    // Whole points, rounded up, so no property line wraps, within the
    // widest text box.
    let points = f64::from(name.max(properties)) * f64::from(doc.drawing_style.points_per_world());
    caption.format.width_pt = Some(points.ceil().min(2000.) as f32);
    caption.format.name_breaks = true;
}

/// The molecule's bottom center, which its caption follows.
fn anchor(doc: &Document, atoms: &[u64]) -> Option<Point> {
    let (lo, hi) = crate::scene::selection_bounds(doc, atoms)?;
    Some(Point::new((lo.x + hi.x) / 2., hi.y))
}

fn rewrap(doc: &Document, link: &PropertyCaption, caption: &mut Annotation) {
    let width = caption.size().0;
    wrap(doc, link, caption);
    caption.position.x += (width - caption.size().0) / 2.;
}

/// Follow geometry-only previews without recalculating properties, checking
/// identity, or changing links. Explicitly moved captions keep their offsets.
pub(crate) fn preview_layout(doc: &mut Document, before: &Document) {
    let captions: Vec<_> = doc
        .property_captions
        .iter()
        .filter_map(|link| {
            let mut caption = doc
                .annotations
                .iter()
                .find(|a| Some(a.id) == link.annotation && a.text == link.text)?
                .clone();
            let old = before
                .property_captions
                .iter()
                .find(|old| old.annotation == link.annotation)?;
            let shift = before
                .annotations
                .iter()
                .any(|a| a.id == caption.id && a.position == caption.position)
                .then(|| anchor(before, &old.atoms))
                .flatten()
                .zip(anchor(doc, &link.atoms));
            if link.properties.first() == Some(&Property::Name)
                && reshaped(before, doc, &link.atoms)
            {
                rewrap(doc, link, &mut caption);
            }
            if let Some((from, to)) = shift {
                caption.position = caption.position.offset(to.x - from.x, to.y - from.y);
            }
            Some(caption)
        })
        .collect();
    for caption in captions {
        if let Some(current) = doc.annotations.iter_mut().find(|a| a.id == caption.id) {
            *current = caption;
        }
    }
}

/// Show or hide one property of the molecule made of `atoms`, which must be
/// one whole connected component. The first property adds the caption below
/// the molecule. Hiding the last removes it, and the link stays only to keep a
/// custom name.
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
    let mut link = index.map_or_else(
        || PropertyCaption {
            two_decimal_weight,
            ..Default::default()
        },
        |index| doc.property_captions.remove(index),
    );
    if link.annotation.is_none() {
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
        link.annotation = Some(id);
    }
    let shown = link.properties.clone();
    if link.properties.contains(&property) {
        link.properties.retain(|p| *p != property);
    } else {
        link.properties.push(property);
        link.properties
            .sort_by_key(|p| Property::ALL.iter().position(|q| q == p));
    }
    if link.properties.is_empty() {
        doc.delete(link.annotation.as_slice());
        if link.detach() {
            doc.property_captions.push(link);
        }
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
        .find(|a| Some(a.id) == link.annotation)
        .cloned()
        .ok_or("The property caption is missing")?;
    refresh(doc, &mut link, &mut caption, shown);
    if let Some(current) = doc.annotations.iter_mut().find(|a| a.id == caption.id) {
        *current = caption;
    }
    doc.property_captions.push(link);
    Ok(())
}

/// Show each of `properties` the molecule's caption does not show yet, as
/// [`toggle`] would one by one.
pub fn show(
    doc: &mut Document,
    atoms: &[u64],
    properties: &[Property],
    format: TextFormat,
    two_decimal_weight: bool,
) -> Result<(), String> {
    for &property in properties {
        if !shown(doc, atoms, property) {
            toggle(doc, atoms, property, format.clone(), two_decimal_weight)?;
        }
    }
    Ok(())
}

/// Choose the decimal places of a mass line in this molecule's caption;
/// the edit's transaction rewrites the text.
pub fn set_places(
    doc: &mut Document,
    atoms: &[u64],
    property: Property,
    places: u8,
    two_decimal_weight: bool,
) -> Result<(), String> {
    doc.property_captions
        .iter_mut()
        .find(|link| covers(link, atoms))
        .ok_or("The property caption is missing")?
        .set_places(property, places, two_decimal_weight);
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
        // A link without a caption is named again when it shows one.
        if link.smiles == smiles
            && link.name.is_empty()
            && link.tracks_name()
            && link.annotation.is_some()
        {
            link.name = name.into();
            changed = true;
            if let Some(mut caption) = doc
                .annotations
                .iter()
                .find(|a| Some(a.id) == link.annotation)
                .cloned()
            {
                let shown = link.properties.clone();
                refresh(doc, &mut link, &mut caption, &shown);
                if let Some(current) = doc.annotations.iter_mut().find(|a| a.id == caption.id) {
                    *current = caption;
                }
            }
        }
        doc.property_captions.push(link);
    }
    changed
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
/// molecule's atoms and position. Edited text unlinks it, keeping only a
/// custom name. A deleted molecule removes its caption. Molecules joined together keep the first caption, and
/// a split molecule keeps its caption on the largest piece; a shown caption
/// wins over a link that only keeps a name. Only a molecule whose atoms or
/// bonds changed is recalculated; an empty `before` recalculates every caption.
pub fn reconcile(doc: &mut Document, before: &Document) {
    let mut links: Vec<_> = std::mem::take(&mut doc.property_captions)
        .into_iter()
        .enumerate()
        .collect();
    links.sort_by_key(|(_, link)| link.annotation.is_none());
    let mut kept = vec![];
    let mut claimed = HashSet::new();
    for (index, mut link) in links {
        let mut caption = doc
            .annotations
            .iter()
            .find(|a| Some(a.id) == link.annotation)
            .cloned();
        // Typing over the name line names the molecule; any other change to
        // the text makes it plain text.
        let mut typed = None;
        if let Some(text) = caption.as_ref().map(|c| &c.text)
            && *text != link.text
        {
            typed = typed_name(&link, text).map(str::to_owned);
            if typed.is_none() {
                caption = None;
            }
        }
        if caption.is_none() && link.annotation.is_some() && !link.detach() {
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
            doc.delete(link.annotation.as_slice());
            continue;
        }
        claimed.extend(whole.iter().copied());
        // A link without a caption is known by its atoms.
        let previous = before.property_captions.iter().find(|old| {
            old.annotation == link.annotation
                && (link.annotation.is_some() || old.atoms == link.atoms)
        });
        let shift = previous
            .filter(|_| {
                caption.as_ref().is_some_and(|caption| {
                    before
                        .annotations
                        .iter()
                        .any(|a| a.id == caption.id && a.position == caption.position)
                })
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
                    && old.places(Property::Weight) == link.places(Property::Weight)
                    && old.places(Property::ExactMass) == link.places(Property::ExactMass)
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
        if let Some(mut caption) = caption {
            if !unchanged {
                let shown = link.properties.clone();
                refresh(doc, &mut link, &mut caption, &shown);
            } else if link.properties.first() == Some(&Property::Name)
                && (reshaped(before, doc, &link.atoms)
                    || before.annotations.iter().any(|old| {
                        old.id == caption.id
                            && TextFormat {
                                width_pt: caption.format.width_pt,
                                name_breaks: caption.format.name_breaks,
                                ..old.format.clone()
                            } != caption.format
                    }))
            {
                // Restyled text or a reshaped molecule wraps the name again;
                // a width set by hand stays until the values change.
                rewrap(doc, &link, &mut caption);
            }
            if let Some((from, to)) = shift.zip(anchor(doc, &link.atoms)) {
                caption.position = caption.position.offset(to.x - from.x, to.y - from.y);
            }
            if let Some(current) = doc.annotations.iter_mut().find(|a| a.id == caption.id) {
                *current = caption;
            }
        }
        kept.push((index, link));
    }
    kept.sort_by_key(|(index, _)| *index);
    doc.property_captions = kept.into_iter().map(|(_, link)| link).collect();
}

/// Prune links after explicit deletion: a caption whose molecule is gone goes
/// with it, unless its text was edited. A deleted caption keeps only a custom
/// name.
pub fn prune(doc: &mut Document) {
    let mut removed = Vec::new();
    let atoms: HashSet<_> = doc.atoms.iter().map(|a| a.id).collect();
    doc.property_captions.retain_mut(|link| {
        link.atoms.retain(|id| atoms.contains(id));
        let caption = doc
            .annotations
            .iter()
            .find(|a| Some(a.id) == link.annotation);
        if link.atoms.is_empty()
            && let Some(caption) = caption.filter(|a| a.text == link.text)
        {
            removed.push(caption.id);
        }
        !link.atoms.is_empty() && (caption.is_some() || link.detach())
    });
    doc.annotations.retain(|a| !removed.contains(&a.id));
}

pub fn validate(doc: &Document) -> Result<(), String> {
    let mut captions = HashSet::new();
    let mut atoms = HashSet::new();
    for link in &doc.property_captions {
        let mut properties = HashSet::new();
        let caption = match link.annotation {
            Some(id) => {
                captions.insert(id)
                    && doc.annotations.iter().any(|a| a.id == id)
                    && !doc.molecule_names.iter().any(|n| n.annotation == id)
                    && !link.properties.is_empty()
                    && !link.text.is_empty()
            }
            // Only a custom name is kept without a caption.
            None => {
                link.properties.is_empty() && link.text.is_empty() && !link.custom_name.is_empty()
            }
        };
        if !caption
            || link.atoms.is_empty()
            || link
                .atoms
                .iter()
                .any(|id| !atoms.insert(*id) || doc.atom(*id).is_none())
            || !link.properties.iter().all(|p| properties.insert(*p))
            || link.name.contains('\n')
            || link.custom_name.contains('\n')
            || [link.weight_places, link.exact_mass_places]
                .into_iter()
                .flatten()
                .any(|places| places > PropertyCaption::MAX_PLACES)
        {
            return Err("Invalid property caption link".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
