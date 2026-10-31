//! A fusion citation must identify one locant-labelled skeletal graph.
use super::super::molecule::Molecule;
use std::collections::{BTreeMap, BTreeSet};

/// Preserve elements and adjacency while leaving bond state to the parent proof.
pub(super) fn numbered_parent_graphs_agree<'a>(
    mol: &Molecule,
    maps: impl IntoIterator<Item = &'a BTreeMap<usize, String>>,
) -> bool {
    let mut reference = None;
    for mapping in maps {
        let vertices: BTreeSet<_> = mapping
            .iter()
            .map(|(&atom, locant)| (locant.as_str(), mol.atoms[&atom].symbol.as_str()))
            .collect();
        let edges: BTreeSet<BTreeSet<&str>> = mol
            .bonds
            .values()
            .filter(|bond| mapping.contains_key(&bond.u) && mapping.contains_key(&bond.v))
            .map(|bond| BTreeSet::from([mapping[&bond.u].as_str(), mapping[&bond.v].as_str()]))
            .collect();
        let signature = (vertices, edges);
        if let Some(previous) = &reference {
            if previous != &signature {
                return false;
            }
        } else {
            reference = Some(signature);
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::super::super::molecule::{Atom, Bond};
    use super::*;

    #[test]
    fn tied_maps_require_identical_elements_and_skeletal_edges() {
        let mut mol = Molecule::new();
        for (id, symbol) in [(0, "C"), (1, "N"), (2, "C")] {
            mol.add_atom(Atom::new(id, symbol)).unwrap();
        }
        mol.add_bond(Bond::new(0, 0, 1, 1)).unwrap();
        mol.add_bond(Bond::new(1, 1, 2, 2)).unwrap();
        let forward = BTreeMap::from([(0, "1".into()), (1, "2".into()), (2, "3".into())]);
        let reverse = BTreeMap::from([(2, "1".into()), (1, "2".into()), (0, "3".into())]);
        let moved_n = BTreeMap::from([(0, "2".into()), (1, "1".into()), (2, "3".into())]);
        assert!(numbered_parent_graphs_agree(&mol, [&forward, &reverse]));
        assert!(!numbered_parent_graphs_agree(&mol, [&forward, &moved_n]));
        assert!(numbered_parent_graphs_agree(&mol, std::iter::empty()));
    }
}
