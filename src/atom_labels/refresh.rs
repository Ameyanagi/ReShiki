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
        bond.color = [0, 0, 0];
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
mod tests {
    use super::*;
    use crate::{
        document::Point,
        engine::{LocalEngine, Request},
    };

    fn alcohol() -> Document {
        let mut doc = Document::default();
        let c = doc.add_atom("C", Point::default());
        let o = doc.add_atom("O", Point::new(42., 0.));
        doc.add_bond(c, o, 1, "plain");
        doc
    }

    #[tokio::test]
    async fn labels_match_full_analysis_for_aromatic_charged_radical_and_stereo_input() {
        let engine = LocalEngine::default();
        for smiles in [
            "CCO",
            "N",
            "[NH4+]",
            "C[O-]",
            "[OH]",
            "[13CH3]O",
            "c1cc[nH]c1",
            "c1ncccc1",
            "OP(=O)(O)O",
            "CS(=O)(=O)O",
            "[2H]O",
            "C[N+](=O)[O-]",
            "C[C@H](O)F",
            "F/C=C/F",
            "F/C=C\\F",
            "C[C@H](O)F.C[C@@H](O)F",
        ] {
            let source = engine
                .request(Request::import_smiles(smiles))
                .await
                .unwrap()
                .document
                .unwrap();
            let checked = engine
                .request(Request::molecule("analyze", source.clone()))
                .await
                .unwrap()
                .document
                .unwrap();
            let result = Refresh::calculate(&source, &Refresh::default()).unwrap();
            assert!(result.notice.is_none(), "{smiles}: {:?}", result.notice);
            let mut actual = source.clone();
            let mut expected = source.clone();
            result.apply(&mut actual);
            crate::atom_labels::refresh_computed(&mut expected, &checked);
            assert_eq!(actual, expected, "{smiles}");
        }
    }

    #[test]
    fn independent_fragments_survive_errors_and_only_unchanged_chemistry_is_reused() {
        let mut source = alcohol();
        let invalid = source.add_atom("O", Point::new(100., 100.));
        source.atom_mut(invalid).unwrap().explicit_h = 5;
        let cached = Refresh::calculate(&source, &Refresh::default()).unwrap();
        assert!(cached.notice.is_some());
        cached.apply(&mut source);
        assert_eq!(source.atoms[1].label_h, 1);
        let repeated = Refresh::calculate(&source, &cached).unwrap();
        for id in [1, invalid] {
            assert!(Arc::ptr_eq(&cached.cache[&id], &repeated.cache[&id]));
        }
        source.atoms[1].element = "N".into();
        let changed = Refresh::calculate(&source, &cached).unwrap();
        assert!(!Arc::ptr_eq(&cached.cache[&1], &changed.cache[&1]));
        assert!(Arc::ptr_eq(
            &cached.cache[&invalid],
            &changed.cache[&invalid]
        ));
        changed.apply(&mut source);
        assert_eq!(source.atoms[1].label_h, 2);
        source.atoms[1].position.y += 5.;
        let moved = Refresh::calculate(&source, &changed).unwrap();
        assert!(!Arc::ptr_eq(&changed.cache[&1], &moved.cache[&1]));
        source.atoms[1].charge = 1;
        let charged = Refresh::calculate(&source, &moved).unwrap();
        charged.apply(&mut source);
        assert_eq!(source.atoms[1].label_h, 3);
        source.atoms[1].explicit_h = 1;
        source.atoms[1].no_implicit = true;
        let explicit = Refresh::calculate(&source, &charged).unwrap();
        assert!(!Arc::ptr_eq(&charged.cache[&1], &explicit.cache[&1]));
    }

    #[test]
    fn gallery_attachment_does_not_block_ordinary_oxygen_and_display_is_preserved() {
        let mut source: Document =
            serde_json::from_str(include_str!("../../assets/examples/shortcut-examples.rsk"))
                .unwrap();
        let ids = crate::editing::append(&mut source, &alcohol(), Point::new(-100., -100.));
        let before = source.clone();
        let result = Refresh::calculate(&source, &Refresh::default()).unwrap();
        assert!(result.notice.is_none(), "{:?}", result.notice);
        result.apply(&mut source);
        assert_eq!(source.atom(ids[1]).unwrap().label_h, 1);
        for (before, after) in before.atoms.iter().zip(&source.atoms) {
            let mut atom = before.clone();
            atom.label_h = after.label_h;
            atom.cip_label.clone_from(&after.cip_label);
            assert_eq!(&atom, after);
        }
        for (before, after) in before.bonds.iter().zip(&source.bonds) {
            let mut bond = before.clone();
            bond.cip_label.clone_from(&after.cip_label);
            assert_eq!(&bond, after);
        }
        assert_eq!(source.abbreviations, before.abbreviations);
    }

    #[test]
    fn unsupported_components_keep_labels_without_hiding_invalid_drawings() {
        for kind in [
            Some(crate::attachments::Kind::MultiCenter),
            Some(crate::attachments::Kind::Variable),
            None,
        ] {
            let mut source = alcohol();
            source.atoms[1].label_h = 1;
            let point = crate::projection::add_centroid(&mut source, &[1, 2]).unwrap();
            source.atom_mut(point).unwrap().attachment = kind;
            let before = source.clone();
            let first = Refresh::calculate(&source, &Refresh::default()).unwrap();
            let cached = Refresh::calculate(&source, &first).unwrap();
            assert!(cached.notice.is_none());
            assert!(Arc::ptr_eq(&first.cache[&1], &cached.cache[&1]));
            cached.apply(&mut source);
            assert_eq!(source, before, "Keep labels and attachment semantics");
            assert!(
                chemistry::prepare(&source).is_err(),
                "Export stays restricted"
            );

            let invalid = source.add_atom("O", Point::new(100., 100.));
            source.atom_mut(invalid).unwrap().explicit_h = 5;
            let result = Refresh::calculate(&source, &cached).unwrap();
            assert!(result.notice.is_some(), "Independent valence errors remain");

            source.atom_mut(point).unwrap().centroid = vec![999, 1000];
            assert!(
                Refresh::calculate(&source, &result).is_err(),
                "Malformed attachment targets must still fail validation"
            );
        }
    }

    #[test]
    fn cache_is_bounded_and_deleted_components_are_evicted() {
        let mut source = Document::default();
        for _ in 0..MAX_CACHED_COMPONENTS + 2 {
            source.add_atom("O", Point::default());
        }
        let cached = Refresh::calculate(&source, &Refresh::default()).unwrap();
        assert_eq!(cached.cache.len(), MAX_CACHED_COMPONENTS);
        let result = Refresh::calculate(&alcohol(), &cached).unwrap();
        assert_eq!(result.cache.len(), 1);
        let empty = Refresh::calculate(&Document::default(), &result).unwrap();
        assert!(empty.cache.is_empty());
    }
}
