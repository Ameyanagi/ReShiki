//! Derived drawing labels without identifiers, descriptors or a helper process.
//! One immutable snapshot is computed off the event-loop thread. Cache entries
//! are exact chemical drawings, including coordinates and stereo, and survive
//! only into the next snapshot. Errors are isolated to connected molecules.
use crate::{chemistry::document as chemistry, document::Document};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

const MAX_CACHED_ATOMS: usize = 8192;
const MAX_CACHED_BONDS: usize = 16384;
const MAX_CACHED_COMPONENTS: usize = 1024;

/// The notice of a drawing whose generic structures wait for expansion. It is
/// information, not an error: the app shows it muted.
pub const GENERIC_NOTICE: &str =
    "Chemical labels of generic structures are calculated after expansion";

/// The assigned CIP descriptor and the separate undefined-configuration flag.
type ComputedStereo = (Option<String>, bool);

#[derive(Debug, Default)]
pub struct Refresh {
    cache: HashMap<u64, Arc<Entry>>,
    atoms: HashMap<u64, (u32, Option<String>, bool)>,
    bonds: HashMap<(u64, u64), ComputedStereo>,
    pub notice: Option<String>,
}

#[derive(Debug)]
struct Entry {
    source: Document,
    result: Result<Computed, String>,
}

#[derive(Debug, Default)]
struct Computed {
    atoms: Vec<(u64, u32, Option<String>, bool)>,
    bonds: Vec<((u64, u64), ComputedStereo)>,
}

fn pair(a: u64, b: u64) -> (u64, u64) {
    (a.min(b), a.max(b))
}

/// Refreshes only the optional editor markers after an analysis response has
/// replaced computed chemical labels. Ordinary refresh keeps this off-thread.
pub fn refresh_undefined_stereo(document: &mut Document) {
    let undefined = if document.atom_labels.mark_undefined_stereocentres {
        crate::chemistry::stereo_lint::undefined(document)
    } else {
        Default::default()
    };
    let atoms: HashSet<_> = undefined.atoms.into_iter().collect();
    let bonds: HashSet<_> = undefined
        .bonds
        .into_iter()
        .map(|(a, b)| pair(a, b))
        .collect();
    for atom in &mut document.atoms {
        atom.undefined_stereo = atoms.contains(&atom.id);
    }
    for bond in &mut document.bonds {
        bond.undefined_stereo = bonds.contains(&pair(bond.a, bond.b));
    }
}

impl Refresh {
    pub fn calculate(source: &Document, previous: &Self) -> Result<Self, String> {
        if source.atoms.len() > 100_000 || source.bonds.len() > 300_000 {
            return Err("Molecular graph exceeds the atom or bond limit".into());
        }
        source.validate()?;
        let mut refresh = Self::default();
        let mut atoms = 0;
        let mut bonds = 0;
        let (components, unresolved) = components(source);
        if unresolved {
            refresh.notice = Some(GENERIC_NOTICE.into());
        }
        let distance_atoms = crate::models3d::crystal::analysis_atoms(source);
        for component in components {
            let Some(first) = component.atoms.first() else {
                continue;
            };
            let id = first.id;
            // A crystal's bonds come from distances, not chemistry: its atoms
            // are explicit and get no computed labels.
            if component
                .atoms
                .iter()
                .any(|a| distance_atoms.contains(&a.id))
            {
                continue;
            }
            let entry = previous
                .cache
                .get(&id)
                .filter(|entry| entry.source == component)
                .cloned()
                .unwrap_or_else(|| {
                    Arc::new(Entry {
                        result: compute(&component),
                        source: component,
                    })
                });
            match &entry.result {
                Ok(computed) => {
                    refresh.atoms.extend(
                        computed
                            .atoms
                            .iter()
                            .map(|(id, h, cip, undefined)| (*id, (*h, cip.clone(), *undefined))),
                    );
                    refresh.bonds.extend(computed.bonds.iter().cloned());
                }
                Err(error) => {
                    refresh.notice.get_or_insert_with(|| error.clone());
                }
            }
            if atoms + entry.source.atoms.len() <= MAX_CACHED_ATOMS
                && bonds + entry.source.bonds.len() <= MAX_CACHED_BONDS
                && refresh.cache.len() < MAX_CACHED_COMPONENTS
            {
                atoms += entry.source.atoms.len();
                bonds += entry.source.bonds.len();
                refresh.cache.insert(id, entry);
            }
        }
        Ok(refresh)
    }

    /// Only derived fields are applied. Selection, history and chemical input
    /// remain owned by the editor. The caller must check snapshot identity.
    pub fn apply(&self, document: &mut Document) {
        for atom in &mut document.atoms {
            atom.undefined_stereo = false;
            if let Some((h, cip, undefined)) = self.atoms.get(&atom.id) {
                atom.label_h = *h;
                atom.cip_label.clone_from(cip);
                atom.undefined_stereo = *undefined;
            }
        }
        for bond in &mut document.bonds {
            bond.undefined_stereo = false;
            if let Some((cip, undefined)) = self.bonds.get(&pair(bond.a, bond.b)) {
                bond.cip_label.clone_from(cip);
                bond.undefined_stereo = *undefined;
            }
        }
    }
}

/// The hydrogens atom `id`'s label shows once the labels refresh, worked out
/// for its own molecule only, so placing a mark can allow for them at once.
pub fn label_hydrogens(document: &Document, id: u64) -> Option<u32> {
    let mut adjacent = HashMap::<u64, Vec<u64>>::new();
    for (a, b) in document.bonds.iter().map(|b| (b.a, b.b)).chain(
        document
            .atoms
            .iter()
            .flat_map(|a| a.centroid.iter().map(move |id| (a.id, *id))),
    ) {
        adjacent.entry(a).or_default().push(b);
        adjacent.entry(b).or_default().push(a);
    }
    let mut members = std::collections::HashSet::from([id]);
    let mut pending = vec![id];
    while let Some(at) = pending.pop() {
        for &next in adjacent.get(&at).into_iter().flatten() {
            if members.insert(next) {
                pending.push(next);
            }
        }
    }
    let molecule = Document {
        generic: document.generic.clone(),
        atoms: document
            .atoms
            .iter()
            .filter(|a| members.contains(&a.id))
            .cloned()
            .collect(),
        bonds: document
            .bonds
            .iter()
            .filter(|b| members.contains(&b.a))
            .cloned()
            .collect(),
        ..Document::default()
    };
    let (components, _) = components(&molecule);
    let computed = compute(components.first()?).ok()?;
    computed
        .atoms
        .into_iter()
        .find_map(|(atom, h, _, _)| (atom == id).then_some(h))
}

/// Works out the hydrogen labels of the molecules an edit from `before`
/// changed, so the edit is never shown with them bare: those with an atom
/// whose valence inputs changed, or that gained, lost or changed a bond.
/// Other molecules keep theirs until the off-thread refresh. Like it, failed
/// and crystal molecules keep their labels.
pub fn refresh_edited(before: &Document, document: &mut Document) {
    if !crate::transaction::chemistry_changed(before, document) {
        return;
    }
    let valence = |a: &crate::document::Atom| {
        (
            a.element.clone(),
            a.charge,
            a.radical_electrons,
            a.explicit_h,
            a.no_implicit,
            a.aromatic,
            a.isotope,
        )
    };
    let atoms: HashMap<_, _> = before.atoms.iter().map(|a| (a.id, a)).collect();
    let old: HashMap<_, _> = before.bonds.iter().map(|b| (pair(b.a, b.b), b)).collect();
    let new: HashMap<_, _> = document.bonds.iter().map(|b| (pair(b.a, b.b), b)).collect();
    let edited: HashSet<_> = document
        .atoms
        .iter()
        .filter(|a| {
            atoms
                .get(&a.id)
                .is_none_or(|old| valence(old) != valence(a))
        })
        .map(|a| a.id)
        .chain(
            document
                .bonds
                .iter()
                .filter(|b| {
                    old.get(&pair(b.a, b.b))
                        .is_none_or(|o| (o.a, o.order) != (b.a, b.order))
                })
                .chain(
                    before
                        .bonds
                        .iter()
                        .filter(|b| !new.contains_key(&pair(b.a, b.b))),
                )
                .flat_map(|b| [b.a, b.b]),
        )
        .collect();
    if edited.is_empty() {
        return;
    }
    let distance_atoms = crate::models3d::crystal::analysis_atoms(document);
    let mut hydrogens = HashMap::new();
    for mut component in components(document).0 {
        // This synchronous edit repair consumes only hydrogens. Leave the
        // optional stereogenic-unit pass to the ordinary background refresh.
        component.atom_labels.mark_undefined_stereocentres = false;
        if component.atoms.iter().any(|a| edited.contains(&a.id))
            && !component
                .atoms
                .iter()
                .any(|a| distance_atoms.contains(&a.id))
            && let Ok(computed) = compute(&component)
        {
            hydrogens.extend(computed.atoms.into_iter().map(|(id, h, _, _)| (id, h)));
        }
    }
    for atom in &mut document.atoms {
        if let Some(h) = hydrogens.get(&atom.id) {
            atom.label_h = *h;
        }
    }
}

fn compute(source: &Document) -> Result<Computed, String> {
    // The complete drawing was validated before splitting it into components.
    // Attachment/centroid components have no ordinary molecular graph to
    // analyze. Preserve their supplied labels without reporting a false error;
    // the inspector already explains the analysis/export limitation. Do not
    // synthesize bonds or let these components block independent molecules.
    if source.atoms.iter().any(|atom| !atom.centroid.is_empty()) {
        return Ok(Computed::default());
    }
    // Use the same sanitization, drawing reconstruction and full CIP pass as
    // Analyze/exports. Never approximate chemical valence for quick labels.
    let molecule = chemistry::prepare(source).map_err(|e| e.to_string())?;
    let drawing = chemistry::for_drawing(&molecule, source).map_err(|e| e.to_string())?;
    let labels = drawing.labels().map_err(|e| e.to_string())?;
    let checked = drawing.finish(labels).map_err(|e| e.to_string())?;
    // Possible-centre perception is an optional editor aid. Keep it out of
    // ordinary label refreshes and never place a question mark in CIP output.
    let undefined = if source.atom_labels.mark_undefined_stereocentres {
        crate::chemistry::stereo_lint::undefined(source)
    } else {
        Default::default()
    };
    let undefined_atoms: HashSet<_> = undefined.atoms.into_iter().collect();
    let undefined_bonds: HashSet<_> = undefined
        .bonds
        .into_iter()
        .map(|(a, b)| pair(a, b))
        .collect();
    Ok(Computed {
        atoms: checked
            .atoms
            .into_iter()
            .map(|a| {
                (
                    a.id,
                    a.label_h,
                    a.cip_label,
                    undefined_atoms.contains(&a.id),
                )
            })
            .collect(),
        bonds: checked
            .bonds
            .into_iter()
            .map(|b| {
                let edge = pair(b.a, b.b);
                (edge, (b.cip_label, undefined_bonds.contains(&edge)))
            })
            .collect(),
    })
}

pub(crate) fn components(document: &Document) -> (Vec<Document>, bool) {
    let mut adjacent = HashMap::<u64, Vec<u64>>::new();
    for (a, b) in document.bonds.iter().map(|b| (b.a, b.b)).chain(
        document
            .atoms
            .iter()
            .flat_map(|a| a.centroid.iter().map(move |id| (a.id, *id))),
    ) {
        adjacent.entry(a).or_default().push(b);
        adjacent.entry(b).or_default().push(a);
    }
    let mut component = HashMap::new();
    let mut result = Vec::new();
    for atom in &document.atoms {
        if component.contains_key(&atom.id) {
            continue;
        }
        let index = result.len();
        component.insert(atom.id, index);
        let mut pending = vec![atom.id];
        while let Some(id) = pending.pop() {
            for neighbor in adjacent.get(&id).into_iter().flatten() {
                if !component.contains_key(neighbor) {
                    component.insert(*neighbor, index);
                    pending.push(*neighbor);
                }
            }
        }
        let mut part = Document::default();
        // The optional perception pass is part of the cache key. All other
        // presentation settings remain outside the chemical snapshot.
        part.atom_labels.mark_undefined_stereocentres =
            document.atom_labels.mark_undefined_stereocentres;
        result.push(part);
    }
    // Mark dependencies using the existing atom/component index before any
    // component copy or cache comparison. Never copy large typed definitions
    // into every analysis fragment or reuse an earlier concrete cache entry.
    let mut unresolved = vec![false; result.len()];
    for id in crate::generic::atom_references(document) {
        if let Some(flag) = component
            .get(&id)
            .and_then(|index| unresolved.get_mut(*index))
        {
            *flag = true;
        }
    }
    for atom in &document.atoms {
        if component
            .get(&atom.id)
            .is_some_and(|index| unresolved.get(*index).copied().unwrap_or(true))
        {
            continue;
        }
        // Display settings, cached labels and captions are not chemical input.
        // Keep stable IDs, all atom/bond chemistry, coordinates and projection.
        let mut atom = atom.clone();
        atom.label_h = 0;
        atom.cip_label = None;
        atom.undefined_stereo = false;
        atom.display = Default::default();
        atom.text_style = None;
        atom.marks.clear();
        if let Some(part) = component.get(&atom.id).and_then(|i| result.get_mut(*i)) {
            part.atoms.push(atom);
        }
    }
    for bond in &document.bonds {
        if component
            .get(&bond.a)
            .is_some_and(|index| unresolved.get(*index).copied().unwrap_or(true))
        {
            continue;
        }
        let mut bond = bond.clone();
        bond.cip_label = None;
        bond.undefined_stereo = false;
        bond.color = crate::palette::Color::Ink;
        bond.indicator = Default::default();
        bond.z_order = 0;
        bond.double_position = Default::default();
        bond.secondary_display = None;
        bond.ring_arc = false;
        if let Some(part) = component.get(&bond.a).and_then(|i| result.get_mut(*i)) {
            part.bonds.push(bond);
        }
    }
    let any_unresolved = unresolved.iter().any(|value| *value);
    let ordinary = result
        .into_iter()
        .zip(unresolved)
        .filter_map(|(part, unresolved)| (!unresolved).then_some(part))
        .collect();
    (ordinary, any_unresolved)
}

#[cfg(test)]
mod tests;
