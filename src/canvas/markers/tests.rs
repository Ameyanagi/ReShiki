use super::*;

#[test]
fn indexed_markers_preserve_document_and_selection_order_and_abbreviation_visibility() {
    let doc: Document = serde_json::from_str(include_str!(
        "../../../assets/examples/shortcut-examples.rsk"
    ))
    .unwrap();
    let all = doc.all_ids();
    for ids in [
        all.clone(),
        all.iter().step_by(3).copied().collect(),
        vec![],
        vec![all[0], all[0], u64::MAX],
    ] {
        let markers = Markers::new(&doc, &ids);
        let bonds = doc
            .bonds
            .iter()
            .filter(|bond| doc.bond_visible(bond.a, bond.b))
            .filter(|bond| ids.contains(&bond.a) && ids.contains(&bond.b))
            .filter_map(|bond| doc.atom(bond.a).zip(doc.atom(bond.b)))
            .map(|(a, b)| (a.position, b.position))
            .collect::<Vec<_>>();
        let atoms = ids
            .iter()
            .filter_map(|id| doc.atom(*id).filter(|atom| doc.atom_visible(atom.id)))
            .map(|atom| atom.position)
            .collect::<Vec<_>>();
        let captions = doc
            .annotations
            .iter()
            .filter(|caption| ids.contains(&caption.id))
            .map(|caption| {
                let (w, h) = caption.size();
                (caption.position, Size::new(w, h))
            })
            .collect::<Vec<_>>();
        assert_eq!(markers.bonds, bonds);
        assert_eq!(
            markers
                .atoms
                .iter()
                .map(|(_, atom)| *atom)
                .collect::<Vec<_>>(),
            atoms
        );
        assert_eq!(markers.captions, captions);
    }
}

#[test]
fn two_sided_group_bond_markers_meet_the_visible_label() {
    let mut doc = Document::default();
    let atoms: Vec<_> = (0..4)
        .map(|i| doc.add_atom("C", reshiki::document::Point::new(i as f32 * 42., 0.)))
        .collect();
    for pair in atoms.windows(2) {
        doc.add_bond(pair[0], pair[1], 1, "plain");
    }
    doc.contract(&atoms[1..3], "Link", "").unwrap();
    let markers = Markers::new(&doc, &doc.all_ids());
    assert_eq!(
        markers.bonds,
        vec![
            (World::new(0., 0.), World::new(42., 0.)),
            (World::new(42., 0.), World::new(126., 0.)),
        ]
    );
    assert!(!markers.atoms.iter().any(|(id, _)| *id == atoms[2]));
}
