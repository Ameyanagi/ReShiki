//! Atom and bond lookups by atom ID, built once per scene stage instead of
//! scanning the whole drawing for every atom or bond.
use crate::document::{Atom, Bond, Document, Point};
use std::collections::HashMap;

pub(crate) struct Graph<'d> {
    doc: &'d Document,
    atoms: HashMap<u64, &'d Atom>,
    /// Indices of the bonds touching each atom, in document order.
    bonds: HashMap<u64, Vec<usize>>,
    /// Each bond's end positions, by bond index.
    ends: Vec<Option<(Point, Point)>>,
}

impl<'d> Graph<'d> {
    pub(crate) fn new(doc: &'d Document) -> Self {
        let mut atoms = HashMap::with_capacity(doc.atoms.len());
        for atom in &doc.atoms {
            atoms.entry(atom.id).or_insert(atom);
        }
        let mut bonds: HashMap<u64, Vec<usize>> = HashMap::new();
        for (index, bond) in doc.bonds.iter().enumerate() {
            bonds.entry(bond.a).or_default().push(index);
            if bond.b != bond.a {
                bonds.entry(bond.b).or_default().push(index);
            }
        }
        let ends = doc
            .bonds
            .iter()
            .map(|b| {
                let (a, b) = atoms.get(&b.a).zip(atoms.get(&b.b))?;
                Some((a.position, b.position))
            })
            .collect();
        Self {
            doc,
            atoms,
            bonds,
            ends,
        }
    }

    /// The first atom with this ID, like [`Document::atom`].
    pub(crate) fn atom(&self, id: u64) -> Option<&'d Atom> {
        self.atoms.get(&id).copied()
    }

    /// The bonds touching this atom with their indices, in document order.
    pub(crate) fn bonds(&self, id: u64) -> impl Iterator<Item = (usize, &'d Bond)> + '_ {
        let doc = self.doc;
        self.bonds
            .get(&id)
            .into_iter()
            .flatten()
            .filter_map(move |&i| Some((i, doc.bonds.get(i)?)))
    }

    /// How many bonds touch this atom.
    pub(crate) fn degree(&self, id: u64) -> usize {
        self.bonds.get(&id).map_or(0, Vec::len)
    }

    /// The end positions of the bond at this index, when both atoms exist.
    pub(crate) fn ends(&self, index: usize) -> Option<(Point, Point)> {
        self.ends.get(index).copied().flatten()
    }

    /// [`crate::atom_labels::visible`] without rescanning every bond.
    pub(crate) fn visible(&self, atom: &Atom) -> bool {
        crate::atom_labels::visible_with_degree(atom, self.doc, || self.degree(atom.id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The index answers exactly what the linear scans it replaces did,
    /// including a self-bond (counted once) and a duplicate atom ID (the
    /// first one wins).
    #[test]
    fn lookups_match_linear_scans() {
        let mut doc: Document = serde_json::from_str(include_str!(
            "../../../../assets/examples/shortcut-examples.rsk"
        ))
        .unwrap();
        let first = doc.atoms[0].id;
        let mut self_bond = doc.bonds[0].clone();
        (self_bond.a, self_bond.b) = (first, first);
        doc.bonds.push(self_bond);
        let mut duplicate = doc.atoms[1].clone();
        duplicate.id = first;
        doc.atoms.push(duplicate);
        let graph = Graph::new(&doc);
        for atom in &doc.atoms {
            let id = atom.id;
            assert!(std::ptr::eq(graph.atom(id).unwrap(), doc.atom(id).unwrap()));
            let scanned: Vec<_> = doc
                .bonds
                .iter()
                .enumerate()
                .filter(|(_, b)| b.a == id || b.b == id)
                .map(|(i, _)| i)
                .collect();
            let indexed: Vec<_> = graph.bonds(id).map(|(i, _)| i).collect();
            assert_eq!(indexed, scanned);
            assert_eq!(graph.degree(id), scanned.len());
            assert_eq!(graph.visible(atom), crate::atom_labels::visible(atom, &doc));
        }
        for (i, bond) in doc.bonds.iter().enumerate() {
            let ends = doc
                .atom(bond.a)
                .zip(doc.atom(bond.b))
                .map(|(a, b)| (a.position, b.position));
            assert_eq!(graph.ends(i), ends);
        }
        assert!(graph.atom(u64::MAX).is_none());
        assert_eq!(graph.bonds(u64::MAX).count(), 0);
    }
}
