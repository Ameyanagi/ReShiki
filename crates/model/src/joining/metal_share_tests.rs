use super::*;
use crate::templates::capacity;

fn metal_pair(target_element: &str) -> (Document, u64, u64) {
    let mut doc = Document::default();
    let source = doc.add_atom("Co", Point::new(200., 0.));
    let target = doc.add_atom(target_element, Point::default());
    doc.atom_mut(source).unwrap().charge = 3;
    doc.atom_mut(target).unwrap().charge = 3;
    (doc, source, target)
}

#[test]
fn prepared_share_atom_merges_isolated_cobalt_ions() {
    let (doc, source, target) = metal_pair("Co");
    let before = doc.clone();
    let target_atom = doc.atom(target).unwrap();
    assert_eq!(capacity(target_atom), 0);
    let prepared = Prepared::new(&doc, &[source]).unwrap();
    let (joined, selected) = prepared
        .place(
            target_atom.position,
            None,
            1.,
            Anchor::Atom(source),
            Connection::ShareAtom,
        )
        .unwrap();

    assert_eq!(doc, before);
    assert_eq!(joined.atoms, vec![target_atom.clone()]);
    assert!(joined.bonds.is_empty());
    assert_eq!(selected, vec![target]);
    joined.validate().unwrap();
}

#[test]
fn prepared_share_atom_retains_both_cobalt_ligands_and_original_ids() {
    for order in [1, crate::bonds::DATIVE] {
        let (mut doc, source, target) = metal_pair("Co");
        let source_ligand = doc.add_atom("N", Point::new(242., 0.));
        let target_ligand = doc.add_atom("O", Point::new(-42., 0.));
        doc.add_bond(source_ligand, source, order, "plain");
        doc.add_bond(target_ligand, target, order, "plain");
        let before = doc.clone();
        let target_atom = doc.atom(target).unwrap();
        let fixed_ligand = doc.atom(target_ligand).unwrap();
        let prepared = Prepared::new(&doc, &[source]).unwrap();
        assert_eq!(prepared.fragment.atoms.len(), 2);
        let (joined, selected) = prepared
            .place(
                target_atom.position,
                None,
                1.,
                Anchor::Atom(source),
                Connection::ShareAtom,
            )
            .unwrap();

        assert_eq!(doc, before);
        assert_eq!((joined.atoms.len(), joined.bonds.len()), (3, 2));
        assert!(joined.atom(source).is_none());
        assert_eq!(joined.atom(target).unwrap(), target_atom);
        assert_eq!(joined.atom(target_ligand).unwrap(), fixed_ligand);
        assert_eq!(joined.atom(source_ligand).unwrap().element, "N");
        assert_eq!(selected, vec![target, source_ligand]);
        for ligand in [source_ligand, target_ligand] {
            let (_, bond) = joined.bond_between(ligand, target).unwrap();
            assert_eq!((bond.a, bond.b, bond.order), (ligand, target, order));
        }
        joined.validate().unwrap();
    }
}
