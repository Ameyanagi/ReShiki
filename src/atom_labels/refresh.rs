//! Derived drawing labels without identifiers, descriptors or a helper process.
//! One immutable snapshot is computed off the event-loop thread. Cache entries
//! are exact chemical drawings, including coordinates and stereo, and survive
//! only into the next snapshot. Errors are isolated to connected molecules.
use crate::{chemistry::document as chemistry, document::Document};
use std::{collections::HashMap, sync::Arc};

const MAX_CACHED_ATOMS: usize = 8192;
const MAX_CACHED_BONDS: usize = 16384;
const MAX_CACHED_COMPONENTS: usize = 1024;

#[derive(Debug, Default)]
pub struct Refresh {
    cache: HashMap<u64, Arc<Entry>>,
    atoms: HashMap<u64, (u32, Option<String>)>,
    bonds: HashMap<(u64, u64), Option<String>>,
    pub notice: Option<String>,
}

#[derive(Debug)]
struct Entry {
    source: Document,
    result: Result<Computed, String>,
}

#[derive(Debug, Default)]
struct Computed {
    atoms: Vec<(u64, u32, Option<String>)>,
    bonds: Vec<((u64, u64), Option<String>)>,
}

fn pair(a: u64, b: u64) -> (u64, u64) {
    (a.min(b), a.max(b))
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
        for component in components(source) {
            let Some(first) = component.atoms.first() else {
                continue;
            };
            let id = first.id;
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
                            .map(|(id, h, cip)| (*id, (*h, cip.clone()))),
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
            if let Some((h, cip)) = self.atoms.get(&atom.id) {
                atom.label_h = *h;
                atom.cip_label.clone_from(cip);
            }
        }
        for bond in &mut document.bonds {
            if let Some(cip) = self.bonds.get(&pair(bond.a, bond.b)) {
                bond.cip_label.clone_from(cip);
            }
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
    Ok(Computed {
        atoms: checked
            .atoms
            .into_iter()
            .map(|a| (a.id, a.label_h, a.cip_label))
            .collect(),
        bonds: checked
            .bonds
            .into_iter()
            .map(|b| (pair(b.a, b.b), b.cip_label))
            .collect(),
    })
}

fn components(document: &Document) -> Vec<Document> {
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
        result.push(Document::default());
    }
    for atom in &document.atoms {
        // Display settings, cached labels and captions are not chemical input.
        // Keep stable IDs, all atom/bond chemistry, coordinates and projection.
        let mut atom = atom.clone();
        atom.label_h = 0;
        atom.cip_label = None;
        atom.display = Default::default();
        atom.text_style = None;
        atom.marks.clear();
        if let Some(part) = component.get(&atom.id).and_then(|i| result.get_mut(*i)) {
            part.atoms.push(atom);
        }
    }
    for bond in &document.bonds {
        let mut bond = bond.clone();
        bond.cip_label = None;
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
    result
}

#[cfg(test)]
mod tests;
