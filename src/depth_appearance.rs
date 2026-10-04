//! Persistent paint for an editable projection, independent of chemical styles.
//!
//! Existing atom/bond/fill colors remain the editable base colors. A scope only
//! stores fade amounts; materialization consumes them on an export snapshot so
//! resolving that snapshot again cannot fade the same paint twice.
use crate::{
    document::Document,
    palette::{Color, Palette},
};
use serde::{Deserialize, Serialize};
use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap, HashSet},
};

pub const DEFAULT_STRENGTH: f32 = 0.62;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scope {
    pub atoms: Vec<u64>,
    /// Automatic scopes follow retained XYZ; frozen scopes retain their weights.
    pub automatic: bool,
    pub strength: f32,
    /// Normalized rear distance: zero is the front, one is the rear.
    pub weights: BTreeMap<u64, f32>,
    /// Explicit normalized paint overrides. Zero keeps the unmodified base ink.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub overrides: BTreeMap<u64, f32>,
}

fn components(doc: &Document, ids: &[u64]) -> Vec<Vec<u64>> {
    let real: HashSet<_> = doc
        .atoms
        .iter()
        .filter(|a| a.centroid.is_empty())
        .map(|a| a.id)
        .collect();
    let mut remaining: HashSet<_> = ids.iter().copied().filter(|id| real.contains(id)).collect();
    let mut adjacent: HashMap<u64, Vec<u64>> = HashMap::new();
    for bond in &doc.bonds {
        if remaining.contains(&bond.a) && remaining.contains(&bond.b) {
            adjacent.entry(bond.a).or_default().push(bond.b);
            adjacent.entry(bond.b).or_default().push(bond.a);
        }
    }
    let mut result = Vec::new();
    while let Some(start) = remaining.iter().copied().min() {
        let mut pending = vec![start];
        let mut members = Vec::new();
        while let Some(id) = pending.pop() {
            if remaining.remove(&id) {
                members.push(id);
                pending.extend(adjacent.get(&id).into_iter().flatten().copied());
            }
        }
        members.sort_unstable();
        result.push(members);
    }
    result
}

fn automatic_weights(doc: &Document, ids: &[u64]) -> BTreeMap<u64, f32> {
    let mut weights = BTreeMap::new();
    let depths_by_id: HashMap<_, _> = doc.atoms.iter().map(|a| (a.id, a.depth)).collect();
    for component in components(doc, ids) {
        let depths: Vec<_> = component
            .iter()
            .filter_map(|id| depths_by_id.get(id).map(|depth| (*id, *depth)))
            .collect();
        let lo = depths.iter().map(|(_, z)| *z).fold(f32::INFINITY, f32::min);
        let hi = depths
            .iter()
            .map(|(_, z)| *z)
            .fold(f32::NEG_INFINITY, f32::max);
        let span = hi - lo;
        for (id, depth) in depths {
            let weight = if span.is_finite() && span > 0.001 {
                ((hi - depth) / span).clamp(0., 1.)
            } else {
                0.
            };
            weights.insert(id, weight);
        }
    }
    weights
}

fn touches(scope: &Scope, ids: &[u64]) -> bool {
    ids.iter().any(|id| scope.atoms.contains(id))
}

pub fn has(doc: &Document, ids: &[u64]) -> bool {
    doc.depth_appearance.iter().any(|scope| touches(scope, ids))
}

pub fn is_automatic_for(doc: &Document, ids: &[u64]) -> bool {
    doc.depth_appearance
        .iter()
        .any(|scope| scope.automatic && touches(scope, ids))
}

/// Start independent automatic scopes for the selected connected components.
/// Re-enabling changes only paint; positions and chemical styles are untouched.
pub fn enable(doc: &mut Document, ids: &[u64], strength: f32) -> Result<usize, String> {
    if !strength.is_finite() || !(0.0..=1.0).contains(&strength) {
        return Err("Depth appearance strength must be between zero and one".into());
    }
    let ids = doc.expand_abbreviation_selection(ids);
    let groups = components(doc, &ids);
    let overrides: BTreeMap<_, _> = doc
        .depth_appearance
        .iter()
        .flat_map(|scope| scope.overrides.iter().map(|(id, weight)| (*id, *weight)))
        .collect();
    clear(doc, &ids);
    for atoms in &groups {
        doc.depth_appearance.push(Scope {
            weights: automatic_weights(doc, atoms),
            atoms: atoms.clone(),
            automatic: true,
            strength,
            overrides: atoms
                .iter()
                .filter_map(|id| overrides.get(id).map(|weight| (*id, *weight)))
                .collect(),
        });
    }
    Ok(groups.len())
}

/// Stop automatic restyling while retaining the final editable presentation.
/// A touched connected scope freezes together, including its unselected atoms.
pub fn freeze(doc: &mut Document, ids: &[u64]) -> usize {
    let mut count = 0;
    let frozen: Vec<_> = doc
        .depth_appearance
        .iter()
        .enumerate()
        .filter(|(_, scope)| scope.automatic && touches(scope, ids))
        .map(|(index, scope)| (index, automatic_weights(doc, &scope.atoms)))
        .collect();
    for (index, weights) in frozen {
        if let Some(scope) = doc.depth_appearance.get_mut(index) {
            scope.weights = weights;
            scope.automatic = false;
            count += 1;
        }
    }
    count
}

/// Remove presentation from selected atoms; the original base colors were
/// never overwritten, so clearing restores them without a stale color backup.
pub fn clear(doc: &mut Document, ids: &[u64]) -> usize {
    let selected: HashSet<_> = ids.iter().copied().collect();
    let mut count = 0;
    for scope in &mut doc.depth_appearance {
        scope.atoms.retain(|id| {
            let keep = !selected.contains(id);
            count += usize::from(!keep);
            keep
        });
        scope.weights.retain(|id, _| !selected.contains(id));
        scope.overrides.retain(|id, _| !selected.contains(id));
    }
    doc.depth_appearance.retain(|scope| !scope.atoms.is_empty());
    count
}

/// Set an explicit paint amount independently of automatic depth. In
/// particular, Some(0.) lets manual foreground colors show without fading.
pub fn override_fade(doc: &mut Document, ids: &[u64], value: Option<f32>) -> Result<usize, String> {
    if value.is_some_and(|v| !v.is_finite() || !(0.0..=1.0).contains(&v)) {
        return Err("Depth paint override must be between zero and one".into());
    }
    let mut count = 0;
    for scope in &mut doc.depth_appearance {
        for id in ids.iter().filter(|id| scope.atoms.contains(id)) {
            if let Some(value) = value {
                scope.overrides.insert(*id, value);
            } else {
                scope.overrides.remove(id);
            }
            count += 1;
        }
    }
    Ok(count)
}

pub fn prune(doc: &mut Document) {
    let present: HashSet<_> = doc
        .atoms
        .iter()
        .filter(|a| a.centroid.is_empty())
        .map(|a| a.id)
        .collect();
    for scope in &mut doc.depth_appearance {
        scope.atoms.retain(|id| present.contains(id));
        let members: HashSet<_> = scope.atoms.iter().copied().collect();
        scope.weights.retain(|id, _| members.contains(id));
        scope.overrides.retain(|id, _| members.contains(id));
    }
    doc.depth_appearance.retain(|scope| !scope.atoms.is_empty());
}

pub fn validate(doc: &Document) -> Result<(), String> {
    if doc.depth_appearance.len() > 10_000 {
        return Err("Too many depth appearance scopes".into());
    }
    let atoms: HashSet<_> = doc
        .atoms
        .iter()
        .filter(|a| a.centroid.is_empty())
        .map(|a| a.id)
        .collect();
    let mut owned = HashSet::new();
    for scope in &doc.depth_appearance {
        let members: HashSet<_> = scope.atoms.iter().copied().collect();
        if scope.atoms.is_empty()
            || !scope.strength.is_finite()
            || !(0.0..=1.0).contains(&scope.strength)
            || scope
                .atoms
                .iter()
                .any(|id| !atoms.contains(id) || !owned.insert(*id))
            || scope.weights.len() != scope.atoms.len()
            || scope.atoms.iter().any(|id| !scope.weights.contains_key(id))
            || scope
                .weights
                .iter()
                .chain(&scope.overrides)
                .any(|(id, v)| !members.contains(id) || !v.is_finite() || !(0.0..=1.0).contains(v))
        {
            return Err("Invalid depth appearance scope".into());
        }
    }
    Ok(())
}

/// Effective weights are computed once per scene/snapshot, never per primitive.
pub struct Paint {
    weights: BTreeMap<u64, f32>,
    palette: Option<Palette>,
    paper: [u8; 3],
}
impl Paint {
    pub fn new(doc: &Document) -> Self {
        let mut weights = BTreeMap::new();
        for scope in &doc.depth_appearance {
            let values = if scope.automatic {
                automatic_weights(doc, &scope.atoms)
            } else {
                scope.weights.clone()
            };
            for (id, weight) in values {
                weights.insert(
                    id,
                    scope.overrides.get(&id).copied().unwrap_or(weight) * scope.strength,
                );
            }
        }
        // Nonchemical centroids/attachment anchors inherit their members'
        // paint, just as they inherit XYZ. Contacts therefore participate in
        // fading without introducing a carbon or a separate editable scope.
        for atom in doc.atoms.iter().filter(|atom| !atom.centroid.is_empty()) {
            if atom.centroid.iter().any(|id| weights.contains_key(id)) {
                let amount = atom
                    .centroid
                    .iter()
                    .map(|id| weights.get(id).copied().unwrap_or(0.))
                    .sum::<f32>()
                    / atom.centroid.len() as f32;
                weights.insert(atom.id, amount);
            }
        }
        let palette = (!weights.is_empty()).then(|| Palette::of(doc));
        Self {
            weights,
            palette,
            paper: doc.canvas_theme.background(),
        }
    }
    pub fn amount(&self, id: u64) -> f32 {
        self.weights.get(&id).copied().unwrap_or(0.)
    }
    pub fn mean(&self, ids: &[u64]) -> f32 {
        if ids.is_empty() {
            0.
        } else {
            ids.iter().map(|id| self.amount(*id)).sum::<f32>() / ids.len() as f32
        }
    }
    pub fn is_empty(&self) -> bool {
        self.weights.is_empty()
    }

    /// Visible opaque RGB fading: transparent copies retain the same ink as
    /// the source canvas, and no background-colored occlusion halo is added.
    pub fn color(&self, color: Color, amount: f32) -> Color {
        let source = self
            .palette
            .as_ref()
            .map_or_else(|| color.rgb(), |palette| palette.rgb(color));
        let paper = self.paper;
        Color::Custom(std::array::from_fn(|i| {
            let from = source.get(i).copied().unwrap_or(0) as f32;
            let to = paper.get(i).copied().unwrap_or(255) as f32;
            (from + (to - from) * amount).round().clamp(0., 255.) as u8
        }))
    }

    /// Shared figure/editable-exchange snapshot. Existing fields are the base
    /// colors; this owned copy contains only the resulting visible colors.
    pub fn materialize<'a>(&self, doc: &'a Document) -> Cow<'a, Document> {
        if self.is_empty() {
            return Cow::Borrowed(doc);
        }
        let resolved = crate::canvas_theme::resolved_document(doc);
        let base = resolved.as_ref();
        let mut result = base.clone();
        for (atom, original) in result.atoms.iter_mut().zip(&base.atoms) {
            let amount = self.amount(atom.id);
            if amount <= 0. {
                continue;
            }
            let mut style = original
                .text_style
                .clone()
                .unwrap_or_else(|| base.drawing_style.text_style());
            let base_ink = style.color;
            style.color = self.color(base_ink, amount);
            atom.text_style = Some(style);
            atom.display.color_override = true;
            let hydrogen = original.display.hydrogen_color.unwrap_or(base_ink);
            atom.display.hydrogen_color = Some(self.color(hydrogen, amount));
            atom.display.highlight = original.display.highlight.map(|c| self.color(c, amount));
            atom.display.stereo.style.color =
                self.color(original.display.stereo.style.color, amount);
            if let Some(number) = &mut atom.display.number {
                number.style.color = self.color(number.style.color, amount);
            }
        }
        for bond in &mut result.bonds {
            let amount = self.mean(&[bond.a, bond.b]);
            if amount <= 0. {
                continue;
            }
            bond.color = self.color(bond.color, amount);
            bond.highlight = bond.highlight.map(|c| self.color(c, amount));
            bond.indicator.style.color = self.color(bond.indicator.style.color, amount);
        }
        for fill in &mut result.ring_fills {
            let amount = self.mean(&fill.atoms);
            if amount > 0. {
                fill.color = self.color(fill.color, amount);
            }
        }
        for (group, original) in result.abbreviations.iter_mut().zip(&base.abbreviations) {
            let amount = self.amount(group.anchor);
            if amount <= 0. {
                continue;
            }
            let mut style = original.text_style(base);
            style.color = self.color(style.color, amount);
            group.label_style = Some(style);
            group.label_color_override = true;
            group.highlight = group.highlight.map(|c| self.color(c, amount));
        }
        // Materialization is terminal for this snapshot, making it idempotent.
        result.depth_appearance.clear();
        Cow::Owned(result)
    }
}

pub fn materialize(doc: &Document) -> Cow<'_, Document> {
    Paint::new(doc).materialize(doc)
}

/// Preserve the original component's paint on a partial clipboard selection.
pub fn selection(doc: &Document, ids: &[u64]) -> Vec<Scope> {
    let selected: HashSet<_> = ids.iter().copied().collect();
    doc.depth_appearance
        .iter()
        .filter_map(|scope| {
            let atoms: Vec<_> = scope
                .atoms
                .iter()
                .copied()
                .filter(|id| selected.contains(id))
                .collect();
            if atoms.is_empty() {
                return None;
            }
            let mut result = scope.clone();
            if atoms.len() != scope.atoms.len() {
                result.weights = if scope.automatic {
                    automatic_weights(doc, &scope.atoms)
                } else {
                    scope.weights.clone()
                };
                result.automatic = false;
            }
            let members: HashSet<_> = atoms.iter().copied().collect();
            result.atoms = atoms;
            result.weights.retain(|id, _| members.contains(id));
            result.overrides.retain(|id, _| members.contains(id));
            Some(result)
        })
        .collect()
}

pub fn remap(scopes: &[Scope], mapping: &HashMap<u64, u64>) -> Vec<Scope> {
    scopes
        .iter()
        .filter_map(|scope| {
            let atoms: Vec<_> = scope
                .atoms
                .iter()
                .filter_map(|id| mapping.get(id).copied())
                .collect();
            if atoms.is_empty() {
                return None;
            }
            Some(Scope {
                atoms,
                automatic: scope.automatic,
                strength: scope.strength,
                weights: scope
                    .weights
                    .iter()
                    .filter_map(|(id, value)| mapping.get(id).map(|id| (*id, *value)))
                    .collect(),
                overrides: scope
                    .overrides
                    .iter()
                    .filter_map(|(id, value)| mapping.get(id).map(|id| (*id, *value)))
                    .collect(),
            })
        })
        .collect()
}
