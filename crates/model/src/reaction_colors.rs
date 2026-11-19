//! Color a reaction by origin: each starting fragment's color follows its
//! atoms into the products along the atom maps, and the bonds and rings a
//! step forms are marked. Only ordinary bond and label colors, highlights,
//! ring fills and one legend text are written, so every export keeps them.
use crate::document::{Annotation, Document, Point};
use crate::palette::{Color, Hue, Row};
use crate::reactions::{Reaction, Role};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[cfg(test)]
mod tests;

/// Fragment hues in the order new starting materials take them.
const FRAGMENTS: [Hue; 7] = [
    Hue::Blue,
    Hue::Green,
    Hue::Purple,
    Hue::Orange,
    Hue::Teal,
    Hue::Indigo,
    Hue::Amber,
];
const SWATCH: char = '■';
const RING: char = '●';

/// Color by origin settings, kept on the reaction so a rerun repaints.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Colors {
    /// One hue per reactant, in reactant order. A reactant made by an
    /// earlier colored step keeps that step's colors instead.
    pub reactants: Vec<Hue>,
    /// Bonds and rings the step forms.
    pub formed: Hue,
    /// Highlight the bonds the step forms.
    #[serde(default = "yes")]
    pub bonds: bool,
    /// Also highlight bonds the step breaks or changes in order.
    #[serde(default)]
    pub changes: bool,
    /// Fill the rings the step forms.
    #[serde(default = "yes")]
    pub rings: bool,
    /// The legend text, one of the reaction's annotations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legend: Option<u64>,
}
fn yes() -> bool {
    true
}
impl Colors {
    /// Bonds a step breaks or reorders, in a hue apart from new bonds.
    fn changed(&self) -> Color {
        let hue = if self.formed == Hue::Amber {
            Hue::Orange
        } else {
            Hue::Amber
        };
        Color::Palette(hue, Row::Tint)
    }
    fn new() -> Self {
        Self {
            reactants: vec![],
            formed: Hue::Red,
            bonds: true,
            changes: false,
            rings: true,
            legend: None,
        }
    }
}

/// Why a reaction cannot be colored yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The reaction needs reactants and products.
    Roles,
    /// No product atom has an atom map.
    Maps,
}
impl Refusal {
    pub fn message(&self) -> &'static str {
        match self {
            Self::Roles => "Assign reactants and products first.",
            Self::Maps => "Map the atoms first: run Auto-map.",
        }
    }
}

/// The reactions joined to `arrow`'s through shared molecules, earlier steps
/// first: a step whose product is another's reactant comes before it.
fn chain(doc: &Document, arrow: u64) -> Vec<usize> {
    let atoms = |r: &Reaction, role: Role| -> HashSet<u64> {
        r.participants(role)
            .iter()
            .flat_map(|p| p.atoms.iter().copied())
            .collect()
    };
    let Some(start) = doc.reactions.iter().position(|r| r.arrow == arrow) else {
        return vec![];
    };
    let mut members = vec![start];
    let mut index = 0;
    while let Some(&current) = members.get(index) {
        index += 1;
        let Some(reaction) = doc.reactions.get(current) else {
            continue;
        };
        // Steps join where one's product is another's reactant; a shared
        // reagent or caption does not join them.
        let (made, used) = (
            atoms(reaction, Role::Product),
            atoms(reaction, Role::Reactant),
        );
        for (other, r) in doc.reactions.iter().enumerate() {
            if !members.contains(&other)
                && (!atoms(r, Role::Reactant).is_disjoint(&made)
                    || !atoms(r, Role::Product).is_disjoint(&used))
            {
                members.push(other);
            }
        }
    }
    // Earlier steps first, in drawing order otherwise, whichever arrow
    // started; a cycle keeps the remaining order.
    members.sort_unstable();
    let mut ordered = Vec::new();
    while !members.is_empty() {
        let next = members
            .iter()
            .position(|&m| {
                let reactants = doc
                    .reactions
                    .get(m)
                    .map(|r| atoms(r, Role::Reactant))
                    .unwrap_or_default();
                !members.iter().any(|&other| {
                    other != m
                        && doc
                            .reactions
                            .get(other)
                            .is_some_and(|r| !atoms(r, Role::Product).is_disjoint(&reactants))
                })
            })
            .unwrap_or(0);
        ordered.push(members.remove(next));
    }
    ordered
}

/// The map numbers used exactly once among these atoms; a repeated number
/// names no one atom.
fn unique_maps(doc: &Document, atoms: &HashSet<u64>) -> HashSet<u32> {
    let mut seen = HashSet::new();
    let mut repeated = HashSet::new();
    for map in atoms
        .iter()
        .filter_map(|id| doc.atom(*id).map(|a| a.map_num))
        .filter(|m| *m != 0)
    {
        if !seen.insert(map) {
            repeated.insert(map);
        }
    }
    &seen - &repeated
}

/// Atom-map pairs of the bonds on one side, with their orders, over the
/// side's unique maps.
fn mapped_bonds(doc: &Document, atoms: &HashSet<u64>) -> HashMap<(u32, u32), u8> {
    let unique = unique_maps(doc, atoms);
    let map = |id: u64| {
        doc.atom(id)
            .map(|a| a.map_num)
            .filter(|m| unique.contains(m))
    };
    doc.bonds
        .iter()
        .filter(|b| b.order != 0 && atoms.contains(&b.a) && atoms.contains(&b.b))
        .filter_map(|b| {
            let (x, y) = (map(b.a)?, map(b.b)?);
            Some(((x.min(y), x.max(y)), b.order))
        })
        .collect()
}

fn pair(a: u64, b: u64) -> (u64, u64) {
    (a.min(b), a.max(b))
}

/// What one step changes: bonds it forms, and bonds it breaks or reorders.
struct Changes {
    formed: HashSet<(u64, u64)>,
    changed: HashSet<(u64, u64)>,
}
fn changes(doc: &Document, reaction: &Reaction) -> Changes {
    let side = |role: Role| -> HashSet<u64> {
        reaction
            .participants(role)
            .iter()
            .flat_map(|p| p.atoms.iter().copied())
            .collect()
    };
    let (reactants, products) = (side(Role::Reactant), side(Role::Product));
    let (before, after) = (mapped_bonds(doc, &reactants), mapped_bonds(doc, &products));
    let (mapped_before, mapped_after) = (unique_maps(doc, &reactants), unique_maps(doc, &products));
    let map = |id: u64| doc.atom(id).map_or(0, |a| a.map_num);
    let mut result = Changes {
        formed: HashSet::new(),
        changed: HashSet::new(),
    };
    for bond in doc.bonds.iter().filter(|b| b.order != 0) {
        let (x, y) = (map(bond.a), map(bond.b));
        if x == 0 || y == 0 {
            continue;
        }
        let key = (x.min(y), x.max(y));
        if products.contains(&bond.a) && products.contains(&bond.b) {
            if !mapped_after.contains(&x) || !mapped_after.contains(&y) {
                continue;
            }
            match before.get(&key) {
                // Formed only between atoms the reactants also carry.
                None if mapped_before.contains(&x) && mapped_before.contains(&y) => {
                    result.formed.insert(pair(bond.a, bond.b));
                }
                Some(order) if *order != bond.order => {
                    result.changed.insert(pair(bond.a, bond.b));
                }
                _ => {}
            }
        } else if reactants.contains(&bond.a)
            && reactants.contains(&bond.b)
            && mapped_before.contains(&x)
            && mapped_before.contains(&y)
            && !after.contains_key(&key)
            && mapped_after.contains(&x)
            && mapped_after.contains(&y)
        {
            result.changed.insert(pair(bond.a, bond.b));
        }
    }
    result
}

/// The reactant and product atoms of these steps; reagents keep their paint.
fn chain_atoms(doc: &Document, steps: &[usize]) -> HashSet<u64> {
    steps
        .iter()
        .filter_map(|i| doc.reactions.get(*i))
        .flat_map(|r| [Role::Reactant, Role::Product].map(|role| r.participants(role)))
        .flatten()
        .flat_map(|p| p.atoms.iter().copied())
        .collect()
}

/// Every hue color by origin may have written for these settings.
fn paints(colors: &Colors) -> HashSet<Color> {
    colors
        .reactants
        .iter()
        .map(|hue| Color::Palette(*hue, Row::Strong))
        .chain([Color::Palette(colors.formed, Row::Tint), colors.changed()])
        .collect()
}

/// Remove this chain's earlier paint from its molecules.
fn clear(doc: &mut Document, atoms: &HashSet<u64>, paint: &HashSet<Color>) {
    for bond in doc
        .bonds
        .iter_mut()
        .filter(|b| atoms.contains(&b.a) && atoms.contains(&b.b))
    {
        if paint.contains(&bond.color) {
            bond.color = Color::Ink;
        }
        if bond.highlight.is_some_and(|c| paint.contains(&c)) {
            bond.highlight = None;
        }
    }
    for atom in doc.atoms.iter_mut().filter(|a| atoms.contains(&a.id)) {
        if let Some(style) = &mut atom.text_style
            && paint.contains(&style.color)
        {
            style.color = Color::Ink;
            atom.display.color_override = false;
        }
    }
    doc.ring_fills
        .retain(|f| !(paint.contains(&f.color) && f.atoms.iter().all(|id| atoms.contains(id))));
}

/// Whether a product atom has a map number that a reactant atom also has.
fn shares_maps(doc: &Document, reaction: &Reaction) -> bool {
    let maps = |role: Role| -> HashSet<u32> {
        reaction
            .participants(role)
            .iter()
            .flat_map(|p| p.atoms.iter())
            .filter_map(|id| doc.atom(*id).map(|a| a.map_num))
            .filter(|m| *m != 0)
            .collect()
    };
    reaction.ready() && !maps(Role::Product).is_disjoint(&maps(Role::Reactant))
}

/// Color `arrow`'s reaction, and every step joined to it, by origin. Each
/// step without settings gets default hues that its chain does not use yet.
pub fn paint(doc: &mut Document, arrow: u64) -> Result<(), Refusal> {
    let reaction = doc
        .reactions
        .iter()
        .find(|r| r.arrow == arrow)
        .filter(|r| r.ready())
        .ok_or(Refusal::Roles)?;
    if !shares_maps(doc, reaction) {
        return Err(Refusal::Maps);
    }
    // Only steps whose sides share atom maps carry color.
    // Earlier paint goes from the whole chain, so a step that lost its maps
    // keeps none; only steps whose sides share atom maps are painted again.
    erase(doc, arrow);
    let steps: Vec<_> = chain(doc, arrow)
        .into_iter()
        .filter(|i| doc.reactions.get(*i).is_some_and(|r| shares_maps(doc, r)))
        .collect();
    // Settings first, so the chain shares distinct fragment hues.
    let mut taken: Vec<Hue> = steps
        .iter()
        .filter_map(|i| doc.reactions.get(*i)?.colors.as_ref())
        .flat_map(|c| c.reactants.iter().copied())
        .collect();
    for &i in &steps {
        let Some(reaction) = doc.reactions.get_mut(i) else {
            continue;
        };
        let count = reaction.reactants.len();
        let colors = reaction.colors.get_or_insert_with(Colors::new);
        while colors.reactants.len() < count {
            let hue = FRAGMENTS
                .into_iter()
                .find(|h| !taken.contains(h))
                // More reactants than hues: start the list again.
                .or_else(|| {
                    FRAGMENTS
                        .get(colors.reactants.len() % FRAGMENTS.len())
                        .copied()
                })
                .unwrap_or(Hue::Blue);
            taken.push(hue);
            colors.reactants.push(hue);
        }
    }
    let atoms = chain_atoms(doc, &steps);
    let paint: HashSet<Color> = steps
        .iter()
        .filter_map(|i| doc.reactions.get(*i)?.colors.as_ref())
        .flat_map(paints)
        .collect();
    // New default hues may differ from the erased ones.
    clear(doc, &atoms, &paint);

    let mut origin: HashMap<u64, Hue> = HashMap::new();
    let mut formed = Vec::new();
    // The hues fragments actually took, for the legend.
    let mut fresh: Vec<Hue> = Vec::new();
    for &i in &steps {
        let Some(reaction) = doc.reactions.get(i) else {
            continue;
        };
        let Some(colors) = reaction.colors.clone() else {
            continue;
        };
        for (participant, hue) in reaction.reactants.iter().zip(&colors.reactants) {
            if participant.atoms.iter().all(|id| origin.contains_key(id)) {
                continue;
            }
            for id in &participant.atoms {
                origin.entry(*id).or_insert(*hue);
            }
            if !fresh.contains(hue) {
                fresh.push(*hue);
            }
        }
        // A map number used twice on the reactant side names no one atom.
        let mut sources: HashMap<u32, Option<Hue>> = HashMap::new();
        for id in reaction.reactants.iter().flat_map(|p| p.atoms.iter()) {
            let (Some(atom), Some(hue)) = (doc.atom(*id), origin.get(id)) else {
                continue;
            };
            if atom.map_num != 0 {
                sources
                    .entry(atom.map_num)
                    .and_modify(|seen| *seen = None)
                    .or_insert(Some(*hue));
            }
        }
        for id in reaction.products.iter().flat_map(|p| p.atoms.iter()) {
            if let Some(Some(hue)) = doc.atom(*id).and_then(|a| sources.get(&a.map_num)) {
                origin.insert(*id, *hue);
            }
        }
        let step = changes(doc, reaction);
        let products: Vec<u64> = reaction
            .products
            .iter()
            .flat_map(|p| p.atoms.iter().copied())
            .collect();
        formed.push((colors, step, products));
    }

    let highlight: HashMap<(u64, u64), Color> = formed
        .iter()
        .flat_map(|(colors, step, _)| {
            let tint = Color::Palette(colors.formed, Row::Tint);
            let other = colors.changed();
            let bonds = step.formed.iter().filter(|_| colors.bonds);
            let changed = step.changed.iter().filter(|_| colors.changes);
            bonds
                .map(move |key| (*key, tint))
                .chain(changed.map(move |key| (*key, other)))
        })
        .collect();
    let new_bonds: HashSet<_> = formed
        .iter()
        .flat_map(|(_, step, _)| step.formed.iter().copied())
        .collect();
    for bond in &mut doc.bonds {
        let key = pair(bond.a, bond.b);
        if let Some(color) = highlight.get(&key) {
            bond.highlight = Some(*color);
        }
        if new_bonds.contains(&key) {
            continue;
        }
        if let (Some(a), Some(b)) = (origin.get(&bond.a), origin.get(&bond.b))
            && a == b
        {
            bond.color = Color::Palette(*a, Row::Strong);
        }
    }
    for atom in &mut doc.atoms {
        if atom.element == "C" {
            continue;
        }
        if let Some(hue) = origin.get(&atom.id) {
            let style = doc.drawing_style.text_style();
            atom.text_style.get_or_insert(style).color = Color::Palette(*hue, Row::Strong);
            atom.display.color_override = true;
        }
    }
    let mut ringed = Vec::new();
    for (colors, step, products) in &formed {
        if !colors.rings || step.formed.is_empty() {
            continue;
        }
        for ring in crate::ring_fills::selected_cycles(doc, products) {
            let closes = ring
                .iter()
                .zip(ring.iter().cycle().skip(1))
                .any(|(a, b)| step.formed.contains(&pair(*a, *b)));
            if closes {
                ringed.push(colors.formed);
                crate::ring_fills::apply(
                    doc,
                    &ring,
                    Some(Color::Palette(colors.formed, Row::Tint)),
                );
            }
        }
    }
    let steps: Vec<_> = formed.iter().map(|(c, s, _)| (c, s)).collect();
    let ids: Vec<_> = steps.iter().filter_map(|(c, _)| c.legend).collect();
    legends(doc, &ids, &fresh, &steps, &ringed);
    Ok(())
}

/// Write the legend of every step that has one. One text holds a colored
/// swatch and a name per entry; names you edit are kept while the entries
/// stay the same. New and changed bonds and new rings get one entry per hue.
fn legends(
    doc: &mut Document,
    ids: &[u64],
    fragments: &[Hue],
    steps: &[(&Colors, &Changes)],
    ringed: &[Hue],
) {
    let mut entries: Vec<(char, Color, String)> = fragments
        .iter()
        .enumerate()
        .map(|(i, hue)| {
            (
                SWATCH,
                Color::Palette(*hue, Row::Strong),
                format!("Reactant {}", i + 1),
            )
        })
        .collect();
    // Strong swatches: a small square in a pale tint is hard to see.
    let strong = |color: Color| match color {
        Color::Palette(hue, _) => Color::Palette(hue, Row::Strong),
        other => other,
    };
    let mut push = |glyph: char, color: Color, name: &str| {
        if !entries
            .iter()
            .any(|(g, c, n)| *g == glyph && *c == color && n == name)
        {
            entries.push((glyph, color, name.into()));
        }
    };
    for (colors, step) in steps {
        if colors.bonds && !step.formed.is_empty() {
            push(
                SWATCH,
                Color::Palette(colors.formed, Row::Strong),
                "New bond",
            );
        }
    }
    for (colors, step) in steps {
        if colors.changes && !step.changed.is_empty() {
            push(SWATCH, strong(colors.changed()), "Changed bond");
        }
    }
    for hue in ringed {
        push(RING, Color::Palette(*hue, Row::Strong), "New ring");
    }
    for &id in ids {
        legend(doc, id, &entries);
    }
}

fn legend(doc: &mut Document, id: u64, entries: &[(char, Color, String)]) {
    let Some(text) = doc.annotations.iter_mut().find(|a| a.id == id) else {
        return;
    };
    let glyphs: Vec<_> = text
        .text
        .char_indices()
        .filter(|(_, c)| *c == SWATCH || *c == RING)
        .collect();
    let same =
        glyphs.len() == entries.len() && glyphs.iter().zip(entries).all(|((_, g), (e, ..))| g == e);
    if !same {
        text.text = entries
            .iter()
            .map(|(glyph, _, name)| format!("{glyph} {name}"))
            .collect::<Vec<_>>()
            .join("   ");
        text.format.spans.clear();
    }
    let starts: Vec<_> = text
        .text
        .char_indices()
        .filter(|(_, c)| *c == SWATCH || *c == RING)
        .map(|(i, c)| (i, i + c.len_utf8()))
        .collect();
    // Recolor only the glyphs, keeping the rest of each entry's style.
    for ((start, end), (_, color, _)) in starts.into_iter().zip(entries) {
        let change = crate::typography::StyleChange::Color(*color);
        text.format.apply(&text.text, Some(start..end), &change);
    }
}

/// Show or hide the legend below `arrow`'s reaction, in caption `format`.
pub fn set_legend(
    doc: &mut Document,
    arrow: u64,
    shown: bool,
    format: crate::typography::TextFormat,
) -> Result<(), Refusal> {
    let Some(index) = doc.reactions.iter().position(|r| r.arrow == arrow) else {
        return Err(Refusal::Roles);
    };
    let current = doc
        .reactions
        .get(index)
        .and_then(|r| r.colors.as_ref()?.legend);
    match (current, shown) {
        (Some(id), false) => {
            doc.annotations.retain(|a| a.id != id);
            crate::reactions::prune(doc);
            if let Some(colors) = doc.reactions.get_mut(index).and_then(|r| r.colors.as_mut()) {
                colors.legend = None;
            }
            Ok(())
        }
        (None, true) => {
            let id = doc.next_id();
            let at = legend_position(doc, arrow);
            doc.annotations.push(Annotation {
                id,
                position: at,
                text: String::new(),
                format,
            });
            if let Some(reaction) = doc.reactions.get_mut(index) {
                reaction.annotations.push(id);
                reaction.colors.get_or_insert_with(Colors::new).legend = Some(id);
            }
            paint(doc, arrow)
        }
        _ => Ok(()),
    }
}

/// Below the reaction's molecules and arrow, left aligned.
fn legend_position(doc: &Document, arrow: u64) -> Point {
    let Some(reaction) = doc.reactions.iter().find(|r| r.arrow == arrow) else {
        return Point::default();
    };
    let mut points: Vec<Point> = Role::ALL
        .into_iter()
        .flat_map(|role| reaction.participants(role))
        .flat_map(|p| p.atoms.iter())
        .filter_map(|id| doc.atom(*id).map(|a| a.position))
        .collect();
    points.extend(
        doc.arrows
            .iter()
            .filter(|a| a.id == arrow)
            .flat_map(|a| [a.start, a.end]),
    );
    let left = points.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
    let bottom = points.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max);
    if !left.is_finite() || !bottom.is_finite() {
        return Point::default();
    }
    Point::new(left, bottom + doc.drawing_style.bond_length_world * 1.2)
}

/// Remove the paint of `arrow`'s reaction and its chain, keeping settings.
/// Returns the chain's reactions and their settings.
fn erase(doc: &mut Document, arrow: u64) -> (Vec<usize>, Vec<Colors>) {
    let steps = chain(doc, arrow);
    let colors: Vec<Colors> = steps
        .iter()
        .filter_map(|i| doc.reactions.get(*i)?.colors.clone())
        .collect();
    let atoms = chain_atoms(doc, &steps);
    let paint: HashSet<Color> = colors.iter().flat_map(paints).collect();
    clear(doc, &atoms, &paint);
    (steps, colors)
}

/// Change the settings of `arrow`'s reaction and repaint. The old paint goes
/// first, so a changed hue leaves nothing behind.
pub fn update(
    doc: &mut Document,
    arrow: u64,
    change: impl FnOnce(&mut Colors),
) -> Result<(), Refusal> {
    erase(doc, arrow);
    let reaction = doc
        .reactions
        .iter_mut()
        .find(|r| r.arrow == arrow)
        .ok_or(Refusal::Roles)?;
    change(reaction.colors.get_or_insert_with(Colors::new));
    paint(doc, arrow)
}

/// Remove the colors and settings of `arrow`'s reaction and its chain.
pub fn clear_paint(doc: &mut Document, arrow: u64) -> bool {
    let (steps, colors) = erase(doc, arrow);
    if colors.is_empty() {
        return false;
    }
    let legends: HashSet<u64> = colors.iter().filter_map(|c| c.legend).collect();
    doc.annotations.retain(|a| !legends.contains(&a.id));
    for &i in &steps {
        if let Some(reaction) = doc.reactions.get_mut(i) {
            reaction.colors = None;
        }
    }
    crate::reactions::prune(doc);
    true
}
