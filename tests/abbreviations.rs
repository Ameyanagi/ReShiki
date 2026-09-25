use reshiki::{
    document::{Document, History, Point},
    editing,
    engine::{LocalEngine, Request},
    scene,
};

fn ether() -> (Document, [u64; 3]) {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(0., 0.));
    let b = doc.add_atom("O", Point::new(42., 0.));
    let c = doc.add_atom("C", Point::new(63., 36.373));
    doc.add_bond(a, b, 1, "plain");
    doc.add_bond(b, c, 1, "plain");
    (doc, [a, b, c])
}
#[test]
fn collapse_is_a_view_with_atomic_selection_transform_copy_delete_and_undo() {
    let (mut doc, [a, b, c]) = ether();
    let original = doc.clone();
    doc.contract(&[b, c], "OMe", "MeO").unwrap();
    let mut history = History::default();
    history.commit(original.clone(), &doc);
    assert_eq!(doc.atoms, original.atoms);
    assert_eq!(doc.bonds, original.bonds);
    assert!(doc.atom_visible(b));
    assert!(!doc.atom_visible(c));
    assert_eq!(doc.expand_integral_groups(&[b]), vec![b, c]);
    assert!(doc.nearest(Point::new(63., 36.373), 1.).is_none());
    let svg = scene::svg(&doc);
    assert!(svg.contains("OMe"));
    assert!(!svg.contains("MeO"));
    let copy = editing::selection(&doc, &[b]);
    assert_eq!(copy.atoms.len(), 2);
    assert_eq!(copy.abbreviations.len(), 1);
    let ids = editing::append(&mut doc, &copy, Point::new(160., 0.));
    doc.validate().unwrap();
    assert_eq!(ids.len(), 2);
    assert_eq!(doc.abbreviations.len(), 2);
    assert_ne!(doc.abbreviations[0].anchor, doc.abbreviations[1].anchor);
    let old = doc.atom(c).unwrap().position;
    doc.translate(&[b], 10., 15.);
    assert_eq!(doc.atom(c).unwrap().position, old.offset(10., 15.));
    let before = doc.clone();
    doc.delete(&[b]);
    doc.validate().unwrap();
    assert!(doc.atom(b).is_none() && doc.atom(c).is_none());
    assert!(doc.atom(a).is_some());
    assert_eq!(doc.abbreviations.len(), 1);
    let mut history2 = History::default();
    history2.commit(before.clone(), &doc);
    history2.undo(&mut doc);
    assert_eq!(doc, before);
    history.undo(&mut doc);
    assert_eq!(doc, original);
}
#[test]
fn labels_follow_orientation_preserve_font_and_exclude_hidden_extents() {
    let (mut doc, [a, b, c]) = ether();
    doc.contract(&[b, c], "OMe", "MeO").unwrap();
    doc.atom_mut(c).unwrap().position = Point::new(5000., 5000.);
    let (_, hi) = scene::selection_bounds(&doc, &doc.all_ids()).unwrap();
    assert!(hi.x < 200. && hi.y < 200.);
    editing::transform(&mut doc, &[a, b, c], editing::Transform::FlipHorizontal);
    assert!(scene::svg(&doc).contains("MeO"));
    let saved = serde_json::to_string(&doc).unwrap();
    let reopened: Document = serde_json::from_str(&saved).unwrap();
    reopened.validate().unwrap();
    assert_eq!(doc, reopened);
    let positions = doc.atoms.clone();
    assert_eq!(doc.expand_abbreviations(&[b]), 1);
    assert_eq!(doc.atoms, positions);
    assert!(scene::selection_bounds(&doc, &doc.all_ids()).unwrap().0.x < -1000.);
}
#[test]
fn invalid_fragments_are_atomic_and_chemical_changes_reveal_the_group() {
    let (mut doc, [a, b, c]) = ether();
    let original = doc.clone();
    for ids in [vec![a, c], vec![999]] {
        assert!(doc.contract(&ids, "X", "").is_err());
        assert_eq!(doc, original);
    }
    assert!(doc.contract(&[b, c], "bad\nlabel", "").is_err());
    doc.contract(&[b, c], "OMe", "MeO").unwrap();
    let before = doc.clone();
    doc.atom_mut(c).unwrap().element = "N".into();
    doc.reconcile_abbreviations(&before);
    assert!(doc.abbreviations.is_empty());
    doc = before.clone();
    doc.translate(&[b], 15., 0.);
    doc.reconcile_abbreviations(&before);
    assert_eq!(doc.abbreviations.len(), 1);
    doc.version = 9;
    assert!(doc.validate().is_err());
    doc.version = 10;
    doc.abbreviations[0].members.push(999);
    assert!(doc.validate().is_err());
}

#[test]
fn contracting_an_internal_group_keeps_all_connections_on_its_anchor() {
    let (mut doc, [a, b, c]) = ether();
    let original_atoms = doc.atoms.clone();
    let original_bonds = doc.bonds.clone();
    doc.contract(&[b], "O", "").unwrap();
    doc.validate().unwrap();
    assert_eq!(doc.abbreviations[0].anchor, b);
    assert_eq!(doc.atoms, original_atoms);
    assert_eq!(doc.bonds, original_bonds);
    assert!(doc.bond_visible(a, b) && doc.bond_visible(b, c));

    // Connections on different members of a group are still ambiguous.
    let d = doc.add_atom("C", Point::new(150., 30.));
    doc.add_bond(c, d, 1, "plain");
    let before = doc.clone();
    assert!(doc.contract(&[b, c], "OC", "").is_err());
    assert_eq!(doc, before);
}
#[tokio::test]
async fn checked_chemistry_cleanup_and_editable_exchange_keep_full_identity() {
    let engine = LocalEngine::default();
    let original = engine
        .request(Request::import_smiles("COc1ccc(NC(=O)OC(C)(C)C)cc1"))
        .await
        .unwrap();
    let doc = original.document.unwrap();
    let mut request = Request::molecule("abbreviate", doc.clone());
    request.selected_ids = Some(doc.all_ids());
    let result = engine.request(request).await.unwrap();
    let collapsed = result.document.unwrap();
    assert_eq!(collapsed.atoms.len(), 16);
    assert_eq!(collapsed.abbreviations.len(), 2);
    assert_eq!(
        original.analysis.as_ref().unwrap().inchikey,
        result.analysis.as_ref().unwrap().inchikey
    );
    for operation in ["analyze", "clean"] {
        let response = engine
            .request(Request::molecule(operation, collapsed.clone()))
            .await
            .unwrap();
        assert_eq!(
            response.document.unwrap().abbreviations,
            collapsed.abbreviations
        );
        assert_eq!(
            response.analysis.unwrap().inchikey,
            original.analysis.as_ref().unwrap().inchikey
        );
    }
    for format in ["cdxml", "cdx"] {
        let mut request = Request::molecule("export", collapsed.clone());
        request.format = Some(format.into());
        let output = engine.request(request).await.unwrap().output.unwrap();
        let imported = engine
            .request(Request::import(format, &output))
            .await
            .unwrap();
        assert_eq!(
            imported.analysis.unwrap().inchikey,
            original.analysis.as_ref().unwrap().inchikey
        );
        let restored = imported.document.unwrap();
        restored.validate().unwrap();
        assert_eq!(restored.abbreviations.len(), 2);
        assert_eq!(restored.atoms.len(), 16);
    }
    assert!(
        reshiki::export::drawing(&collapsed, "pdf")
            .unwrap()
            .starts_with(b"%PDF-")
    );
}
