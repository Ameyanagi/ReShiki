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
        assert_eq!(markers.atoms, atoms);
        assert_eq!(markers.captions, captions);
    }
}
