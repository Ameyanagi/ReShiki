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

/// Legacy RGB owners span connected components, including coordinate contacts;
/// strength, weights and overrides drive their RGB fade. Rear alpha targets whole
/// covalent molecules via covalent_groups, so rear_overrides can store per-atom
/// alpha inside a wider RGB owner. Strength zero marks an alpha-only scope with
/// no RGB fade (has_rgb_fade is false), created by alpha_only or materialization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scope {
    pub atoms: Vec<u64>,
    /// Automatic scopes follow retained XYZ; frozen scopes retain their weights.
    pub automatic: bool,
    pub strength: f32,
    /// Alpha of view-occluded ink. One preserves the existing opaque drawing.
    #[serde(default = "opaque", skip_serializing_if = "is_opaque")]
    pub rear_opacity: f32,
    /// Independent covalent-molecule alpha inside a legacy RGB owner. Keeping
    /// its atom set intact preserves live RGB normalization across contacts.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub rear_overrides: BTreeMap<u64, f32>,
    /// Retained compatibility metadata; not used for visibility.
    /// Legacy frozen covalent rear classification, still written for file
    /// compatibility.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub rear_weights: BTreeMap<u64, f32>,
    /// Normalized rear distance: zero is the front, one is the rear.
    pub weights: BTreeMap<u64, f32>,
    /// Explicit normalized paint overrides. Zero keeps the unmodified base ink.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub overrides: BTreeMap<u64, f32>,
}
fn opaque() -> f32 {
    1.
}
fn is_opaque(value: &f32) -> bool {
    *value == 1.
}
impl Scope {
    /// Whether this scope carries RGB fade.
    pub fn has_rgb_fade(&self) -> bool {
        self.strength > 0.
    }

    /// A new alpha-only scope: strength zero means no RGB fade; the automatic
    /// weights exist only to satisfy validate.
    pub fn alpha_only(doc: &Document, atoms: Vec<u64>, opacity: f32) -> Self {
        Self {
            weights: automatic_weights(doc, &atoms),
            atoms,
            automatic: true,
            strength: 0.,
            rear_opacity: opacity,
            rear_overrides: BTreeMap::new(),
            rear_weights: BTreeMap::new(),
            overrides: BTreeMap::new(),
        }
    }

    /// Every per-atom map lives here so a new map needs one edit.
    pub fn retain_members(&mut self, keep: impl Fn(u64) -> bool) {
        self.atoms.retain(|id| keep(*id));
        self.weights.retain(|id, _| keep(*id));
        self.overrides.retain(|id, _| keep(*id));
        self.rear_overrides.retain(|id, _| keep(*id));
        self.rear_weights.retain(|id, _| keep(*id));
    }

    /// Remap every atom and per-atom map, dropping scopes with no mapped atoms.
    pub fn remapped(&self, mapping: &HashMap<u64, u64>) -> Option<Self> {
        let atoms: Vec<_> = self
            .atoms
            .iter()
            .filter_map(|id| mapping.get(id).copied())
            .collect();
        if atoms.is_empty() {
            return None;
        }
        Some(Self {
            atoms,
            automatic: self.automatic,
            strength: self.strength,
            rear_opacity: self.rear_opacity,
            rear_overrides: self
                .rear_overrides
                .iter()
                .filter_map(|(id, value)| mapping.get(id).map(|id| (*id, *value)))
                .collect(),
            rear_weights: self
                .rear_weights
                .iter()
                .filter_map(|(id, value)| mapping.get(id).map(|id| (*id, *value)))
                .collect(),
            weights: self
                .weights
                .iter()
                .filter_map(|(id, value)| mapping.get(id).map(|id| (*id, *value)))
                .collect(),
            overrides: self
                .overrides
                .iter()
                .filter_map(|(id, value)| mapping.get(id).map(|id| (*id, *value)))
                .collect(),
        })
    }

    /// Clear rear alpha and its retained metadata without changing RGB fade.
    pub fn clear_rear_opacity(&mut self) {
        self.rear_opacity = 1.;
        self.rear_overrides.clear();
        self.rear_weights.clear();
    }

    /// Per-atom maps in size-accounting order.
    pub fn member_maps(&self) -> [&BTreeMap<u64, f32>; 4] {
        [
            &self.weights,
            &self.overrides,
            &self.rear_weights,
            &self.rear_overrides,
        ]
    }

    pub fn rear_opacity_for(&self, id: u64) -> f32 {
        self.rear_overrides
            .get(&id)
            .copied()
            .unwrap_or(self.rear_opacity)
    }
    pub fn has_rear_opacity(&self) -> bool {
        self.atoms.iter().any(|id| self.rear_opacity_for(*id) < 1.)
    }
}

fn components(doc: &Document, ids: &[u64]) -> Vec<Vec<u64>> {
    component_groups(doc, ids, false)
}
fn component_groups(doc: &Document, ids: &[u64], covalent_only: bool) -> Vec<Vec<u64>> {
    let real: HashSet<_> = doc
        .atoms
        .iter()
        .filter(|a| a.centroid.is_empty())
        .map(|a| a.id)
        .collect();
    let mut remaining: HashSet<_> = ids.iter().copied().filter(|id| real.contains(id)).collect();
    let mut adjacent: HashMap<u64, Vec<u64>> = HashMap::new();
    for bond in &doc.bonds {
        if (!covalent_only || bond.is_covalent())
            && remaining.contains(&bond.a)
            && remaining.contains(&bond.b)
        {
            adjacent.entry(bond.a).or_default().push(bond.b);
            adjacent.entry(bond.b).or_default().push(bond.a);
        }
    }
    let mut starts: Vec<_> = remaining.iter().copied().collect();
    starts.sort_unstable();
    let mut result = Vec::new();
    for start in starts {
        if !remaining.contains(&start) {
            continue;
        }
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

pub(crate) fn automatic_weights(doc: &Document, ids: &[u64]) -> BTreeMap<u64, f32> {
    weights_for_groups(doc, components(doc, ids))
}

/// Alpha targets whole covalent molecules even when a legacy RGB owner spans
/// a coordinate contact or only part of a molecule has RGB paint.
fn covalent_groups(doc: &Document, ids: &[u64]) -> Vec<Vec<u64>> {
    let selected: HashSet<_> = ids.iter().copied().collect();
    let real: Vec<_> = doc
        .atoms
        .iter()
        .filter(|a| a.centroid.is_empty())
        .map(|a| a.id)
        .collect();
    component_groups(doc, &real, true)
        .into_iter()
        .filter(|members| members.iter().any(|id| selected.contains(id)))
        .collect()
}

/// Rear-opacity targets: whole covalent molecules of real, non-centroid atoms
/// containing any seed ID, returned in globally sorted order.
pub fn covalent_component_ids(doc: &Document, ids: &[u64]) -> Vec<u64> {
    let mut members: Vec<_> = covalent_groups(doc, ids).into_iter().flatten().collect();
    members.sort_unstable();
    members
}

/// Capture retained compatibility metadata: the legacy covalent rear
/// classification from XYZ. Not used for visibility.
pub(crate) fn capture_legacy_rear_weights(doc: &Document, ids: &[u64]) -> BTreeMap<u64, f32> {
    let selected: HashSet<_> = ids.iter().copied().collect();
    weights_for_groups(doc, covalent_groups(doc, ids))
        .into_iter()
        .filter(|(id, _)| selected.contains(id))
        .collect()
}
/// Resolve retained compatibility metadata: a scope's legacy covalent rear
/// classification (live XYZ, stored rear_weights or v22 weights). Not used for visibility.
pub(crate) fn effective_legacy_rear_weights(doc: &Document, scope: &Scope) -> BTreeMap<u64, f32> {
    if scope.automatic {
        capture_legacy_rear_weights(doc, &scope.atoms)
    } else if scope.rear_weights.is_empty() {
        // Earlier v22 frozen records stored their classification in weights.
        // Respect that captured presentation after their XYZ is edited.
        scope.weights.clone()
    } else {
        scope.rear_weights.clone()
    }
}
fn weights_for_groups(doc: &Document, groups: Vec<Vec<u64>>) -> BTreeMap<u64, f32> {
    let mut weights = BTreeMap::new();
    let depths_by_id: HashMap<_, _> = doc.atoms.iter().map(|a| (a.id, a.depth)).collect();
    for component in groups {
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
        .any(|scope| scope.automatic && scope.has_rgb_fade() && touches(scope, ids))
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
    let opacity: BTreeMap<_, _> = doc
        .depth_appearance
        .iter()
        .flat_map(|scope| {
            scope
                .atoms
                .iter()
                .map(|id| (*id, scope.rear_opacity_for(*id)))
        })
        .collect();
    clear(doc, &ids);
    for atoms in &groups {
        doc.depth_appearance.push(Scope {
            weights: automatic_weights(doc, atoms),
            atoms: atoms.clone(),
            automatic: true,
            strength,
            rear_opacity: 1.,
            rear_overrides: atoms
                .iter()
                .filter_map(|id| {
                    opacity
                        .get(id)
                        .copied()
                        .filter(|alpha| *alpha < 1.)
                        .map(|alpha| (*id, alpha))
                })
                .collect(),
            rear_weights: BTreeMap::new(),
            overrides: atoms
                .iter()
                .filter_map(|id| overrides.get(id).map(|weight| (*id, *weight)))
                .collect(),
        });
    }
    Ok(groups.len())
}

/// Change only rear alpha; existing RGB fade and frozen presentation remain.
pub fn set_rear_opacity(doc: &mut Document, ids: &[u64], opacity: f32) -> Result<usize, String> {
    if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
        return Err("Rear opacity must be between 0% and 100%".into());
    }
    let ids = doc.expand_abbreviation_selection(ids);
    let groups = covalent_groups(doc, &ids);
    let selected: HashSet<_> = groups.iter().flatten().copied().collect();
    let (covered, mut count) = retarget_existing_scopes(doc, &selected, opacity);
    if opacity < 1. {
        count += add_alpha_only_scopes(doc, groups, &covered, opacity);
        if count > 0 {
            doc.version = doc.version.max(22);
        }
    }
    Ok(count)
}

/// Retarget existing scopes while retaining their RGB state and legacy rear metadata.
fn retarget_existing_scopes(
    doc: &mut Document,
    selected: &HashSet<u64>,
    opacity: f32,
) -> (HashSet<u64>, usize) {
    let mut covered = HashSet::new();
    let mut count = 0;
    for index in 0..doc.depth_appearance.len() {
        let Some(scope) = doc.depth_appearance.get(index) else {
            continue;
        };
        let target: Vec<_> = scope
            .atoms
            .iter()
            .copied()
            .filter(|id| selected.contains(id))
            .collect();
        covered.extend(target.iter().copied());
        if target
            .iter()
            .all(|id| scope.rear_opacity_for(*id) == opacity)
        {
            continue;
        }
        let legacy = (!scope.automatic && scope.rear_weights.is_empty()).then(|| {
            if scope.has_rear_opacity() {
                effective_legacy_rear_weights(doc, scope)
            } else {
                capture_legacy_rear_weights(doc, &scope.atoms)
            }
        });
        let Some(scope) = doc.depth_appearance.get_mut(index) else {
            continue;
        };
        // Alpha never changes RGB membership, automatic state or its weights.
        if target.len() == scope.atoms.len() {
            scope.rear_opacity = opacity;
            scope.rear_overrides.clear();
        } else {
            for id in target {
                if opacity == scope.rear_opacity {
                    scope.rear_overrides.remove(&id);
                } else {
                    scope.rear_overrides.insert(id, opacity);
                }
            }
        }
        if scope.has_rear_opacity() {
            if let Some(weights) = legacy {
                scope.rear_weights = weights;
            }
        } else {
            scope.rear_weights.clear();
        }
        count += 1;
    }
    (covered, count)
}

/// Add alpha-only scopes for covalent groups not covered by existing RGB owners.
fn add_alpha_only_scopes(
    doc: &mut Document,
    groups: Vec<Vec<u64>>,
    covered: &HashSet<u64>,
    opacity: f32,
) -> usize {
    let mut count = 0;
    for group in groups {
        let atoms: Vec<_> = group
            .into_iter()
            .filter(|id| !covered.contains(id))
            .collect();
        if atoms.is_empty() {
            continue;
        }
        doc.depth_appearance
            .push(Scope::alpha_only(doc, atoms, opacity));
        count += 1;
    }
    count
}

/// None denotes mixed selected values; untouched components are 100% opaque.
pub fn rear_opacity(doc: &Document, ids: &[u64]) -> Option<f32> {
    let scoped = doc.depth_appearance.iter().map(|scope| scope.atoms.len());
    let mut opacity_by_id = HashMap::with_capacity(scoped.sum());
    for scope in &doc.depth_appearance {
        for &id in &scope.atoms {
            opacity_by_id
                .entry(id)
                .or_insert_with(|| scope.rear_opacity_for(id));
        }
    }
    let mut values = ids
        .iter()
        .map(|id| opacity_by_id.get(id).copied().unwrap_or(1.));
    let first = values.next().unwrap_or(1.);
    values.all(|v| v == first).then_some(first)
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
        .map(|(index, scope)| {
            (
                index,
                automatic_weights(doc, &scope.atoms),
                scope
                    .has_rear_opacity()
                    .then(|| capture_legacy_rear_weights(doc, &scope.atoms)),
            )
        })
        .collect();
    for (index, weights, rear_weights) in frozen {
        if let Some(scope) = doc.depth_appearance.get_mut(index) {
            scope.weights = weights;
            if let Some(weights) = rear_weights {
                scope.rear_weights = weights;
            }
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
        let before = scope.atoms.len();
        scope.retain_members(|id| !selected.contains(&id));
        count += before - scope.atoms.len();
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
        scope.retain_members(|id| members.contains(&id));
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
            || !scope.rear_opacity.is_finite()
            || !(0.0..=1.0).contains(&scope.rear_opacity)
            || scope
                .atoms
                .iter()
                .any(|id| !atoms.contains(id) || !owned.insert(*id))
            || scope.weights.len() != scope.atoms.len()
            || scope.atoms.iter().any(|id| !scope.weights.contains_key(id))
            || (!scope.rear_weights.is_empty() && scope.rear_weights.len() != scope.atoms.len())
            || scope
                .weights
                .iter()
                .chain(&scope.overrides)
                .chain(&scope.rear_overrides)
                .chain(&scope.rear_weights)
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
        for scope in doc
            .depth_appearance
            .iter()
            .filter(|scope| scope.has_rgb_fade())
        {
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
            atom.display.mapping.style.color =
                self.color(original.display.mapping.style.color, amount);
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
        result.depth_appearance.retain(Scope::has_rear_opacity);
        for scope in &mut result.depth_appearance {
            // Scope's strength-zero invariant makes materialized RGB owners alpha-only.
            scope.strength = 0.;
            scope.overrides.clear();
        }
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
            let partial_rear_component = scope.has_rear_opacity()
                && covalent_groups(doc, &scope.atoms)
                    .iter()
                    .flatten()
                    .any(|id| !selected.contains(id));
            if atoms.len() != scope.atoms.len() || partial_rear_component {
                result.weights = if scope.automatic {
                    automatic_weights(doc, &scope.atoms)
                } else {
                    scope.weights.clone()
                };
                if scope.has_rear_opacity() {
                    result.rear_weights = effective_legacy_rear_weights(doc, scope);
                }
                result.automatic = false;
            }
            let members: HashSet<_> = atoms.iter().copied().collect();
            result.atoms = atoms;
            result.retain_members(|id| members.contains(&id));
            Some(result)
        })
        .collect()
}

pub fn remap(scopes: &[Scope], mapping: &HashMap<u64, u64>) -> Vec<Scope> {
    scopes
        .iter()
        .filter_map(|scope| scope.remapped(mapping))
        .collect()
}

#[cfg(test)]
mod tests;
