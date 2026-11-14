//! Atom and bond lookups by atom ID, built once per scene stage instead of
//! scanning the whole drawing for every atom or bond.
use crate::document::{Atom, Bond, Document, Point};
use std::collections::HashMap;

#[cfg(test)]
thread_local! {
    static CONSTRUCTIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
#[cfg(test)]
pub(super) fn construction_count() -> usize {
    CONSTRUCTIONS.with(std::cell::Cell::get)
}

pub(crate) struct Graph<'d> {
    doc: &'d Document,
    atoms: HashMap<u64, &'d Atom>,
    /// Indices of the bonds touching each atom, in document order.
    bonds: HashMap<u64, Vec<usize>>,
    /// Each bond's end positions, by bond index.
    ends: Vec<Option<(Point, Point)>>,
    collapsed: crate::abbreviations::Membership<'d>,
    /// Atom indices by x position, sorted on first use.
    by_x: std::cell::OnceCell<Vec<(f32, usize)>>,
}

impl<'d> Graph<'d> {
    pub(crate) fn new(doc: &'d Document) -> Self {
        #[cfg(test)]
        CONSTRUCTIONS.with(|count| count.set(count.get() + 1));
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
            collapsed: crate::abbreviations::Membership::new(doc),
            by_x: std::cell::OnceCell::new(),
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

    /// The atoms no farther than `reach` from `x` along the x axis, in
    /// document order.
    pub(crate) fn within_x(&self, x: f32, reach: f32) -> Vec<&'d Atom> {
        let doc = self.doc;
        let sorted = self.by_x.get_or_init(|| {
            let mut sorted: Vec<_> = doc
                .atoms
                .iter()
                .enumerate()
                .map(|(index, atom)| (atom.position.x, index))
                .collect();
            sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
            sorted
        });
        let start = sorted.partition_point(|(at, _)| *at < x - reach);
        let mut found: Vec<_> = sorted
            .iter()
            .skip(start)
            .take_while(|(at, _)| *at <= x + reach)
            .map(|(_, index)| *index)
            .collect();
        found.sort_unstable();
        found
            .into_iter()
            .filter_map(|index| doc.atoms.get(index))
            .collect()
    }

    /// [`Document::atom_visible`] without rescanning every abbreviation.
    pub(crate) fn atom_visible(&self, id: u64) -> bool {
        self.collapsed.atom_visible(id)
    }

    /// [`Document::bond_visible`] without rescanning every abbreviation.
    pub(crate) fn bond_visible(&self, a: u64, b: u64) -> bool {
        self.collapsed.bond_visible(a, b)
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
            assert_eq!(graph.atom_visible(id), doc.atom_visible(id));
        }
        for (i, bond) in doc.bonds.iter().enumerate() {
            let ends = doc
                .atom(bond.a)
                .zip(doc.atom(bond.b))
                .map(|(a, b)| (a.position, b.position));
            assert_eq!(graph.ends(i), ends);
            assert_eq!(
                graph.bond_visible(bond.a, bond.b),
                doc.bond_visible(bond.a, bond.b)
            );
        }
        for atom in &doc.atoms {
            let x = atom.position.x;
            let scanned: Vec<_> = doc
                .atoms
                .iter()
                .filter(|other| (other.position.x - x).abs() <= 40.)
                .map(std::ptr::from_ref)
                .collect();
            let indexed: Vec<_> = graph
                .within_x(x, 40.)
                .into_iter()
                .map(std::ptr::from_ref)
                .collect();
            assert_eq!(indexed, scanned);
        }
        for group in &doc.abbreviations {
            assert_eq!(
                group.faces_left_in(&doc, Some(&graph)),
                group.faces_left(&doc)
            );
        }
        assert!(graph.atom(u64::MAX).is_none());
        assert_eq!(graph.bonds(u64::MAX).count(), 0);
    }
}
