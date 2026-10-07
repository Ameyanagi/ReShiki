use super::*;
#[test]
fn toggle_preserves_member_count_coordinates_substituents_and_charges() {
    for n in 3..=8 {
        let mut doc = Document::default();
        let ids = editing::ring(&mut doc, Point::default(), n, false, 5.);
        let atom = ids[0];
        let extra = doc.add_atom("O", Point::new(100., 0.));
        doc.add_bond(atom, extra, 1, "plain");
        doc.atom_mut(atom).unwrap().charge = -1;
        let positions: Vec<_> = doc
            .atoms
            .iter()
            .map(|a| (a.id, a.position, a.charge))
            .collect();
        let edges: Vec<_> = doc.bonds.iter().map(|b| (b.a, b.b)).collect();
        assert!(toggle_selected_aromatic(&mut doc, &ids).unwrap());
        assert_eq!(
            doc.bonds.iter().filter(|b| b.order == 4).count(),
            n as usize
        );
        assert!(!toggle_selected_aromatic(&mut doc, &ids).unwrap());
        assert!(doc.bonds.iter().all(|b| b.order == 1));
        assert_eq!(
            edges,
            doc.bonds.iter().map(|b| (b.a, b.b)).collect::<Vec<_>>()
        );
        assert_eq!(
            positions,
            doc.atoms
                .iter()
                .map(|a| (a.id, a.position, a.charge))
                .collect::<Vec<_>>()
        );
    }
}
#[test]
fn partial_fused_and_disconnected_selections_are_not_rewritten() {
    let mut doc = Document::default();
    let ids = editing::ring(&mut doc, Point::default(), 6, false, 5.);
    let before = doc.clone();
    assert!(toggle_selected_aromatic(&mut doc, &ids[..5]).is_err());
    assert_eq!(doc, before);
    doc.add_bond(ids[0], ids[3], 1, "plain");
    let before = doc.clone();
    assert!(toggle_selected_aromatic(&mut doc, &ids).is_err());
    assert_eq!(doc, before);
}
