use moruno::{
    document::{Arrow, Document, History, Point},
    editing,
    engine::{ChemistryEngine, PythonEngine, Request},
    reactions::{self, Role},
};

fn drawing() -> Document {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::default());
    let b = doc.add_atom("O", Point::new(42., 0.));
    doc.add_bond(a, b, 1, "plain");
    doc.add_atom("C", Point::new(300., 0.));
    doc.arrows.push(Arrow::new(
        4,
        Point::new(120., 0.),
        Point::new(240., 0.),
        Default::default(),
        Default::default(),
    ));
    reactions::assign(&mut doc, 4, &[1], Role::Reactant).unwrap();
    reactions::assign(&mut doc, 4, &[3], Role::Product).unwrap();
    doc
}

#[test]
fn roles_select_whole_molecules_and_survive_history_copy_and_delete() {
    let mut doc = drawing();
    assert_eq!(doc.reactions[0].reactants[0].atoms, [1, 2]);
    let before = doc.clone();
    reactions::assign(&mut doc, 4, &[2], Role::Agent).unwrap();
    assert!(doc.reactions[0].reactants.is_empty());
    assert_eq!(doc.reactions[0].agents[0].atoms, [1, 2]);
    let mut history = History::default();
    history.commit(before.clone(), &doc);
    history.undo(&mut doc);
    assert_eq!(doc, before);
    let serialized = serde_json::to_vec(&doc).unwrap();
    let restored: Document = serde_json::from_slice(&serialized).unwrap();
    assert_eq!(restored, doc);
    let part = editing::selection(&doc, &doc.reactions[0].ids());
    editing::append(&mut doc, &part, Point::new(0., 200.));
    doc.validate().unwrap();
    assert_eq!(doc.reactions.len(), 2);
    assert_ne!(doc.reactions[0].arrow, doc.reactions[1].arrow);
    assert!(
        doc.reactions[1]
            .ids()
            .iter()
            .all(|id| !doc.reactions[0].ids().contains(id))
    );
    assert!(editing::selection(&doc, &[1, 2]).reactions.is_empty());
    doc.delete(&[1]);
    doc.validate().unwrap();
    assert_eq!(doc.reactions[0].reactants[0].atoms, [2]);
    doc.delete(&[4]);
    assert_eq!(doc.reactions.len(), 1);
}

#[test]
fn growth_and_invalid_cross_role_edits_are_checked() {
    let mut doc = drawing();
    let c = doc.add_atom("C", Point::new(-42., 0.));
    doc.add_bond(1, c, 1, "plain");
    reactions::reconcile(&mut doc).unwrap();
    assert!(doc.reactions[0].reactants[0].atoms.contains(&c));
    doc.validate().unwrap();
    let before = doc.clone();
    assert!(reactions::assign(&mut doc, 99, &[1], Role::Agent).is_err());
    assert_eq!(doc, before);
    doc.add_bond(1, 3, 1, "plain");
    assert!(reactions::reconcile(&mut doc).is_err());
    let mut bad = before;
    bad.reactions[0].products[0].atoms.push(1);
    assert!(bad.validate().is_err());
}

#[tokio::test]
async fn reaction_bridge_preserves_roles_stereo_mapping_and_cleanup() {
    let engine = PythonEngine::default();
    for smiles in [
        "[CH3:1][OH:2]>O>[CH2:1]=[O:2]",
        "C[C@H](O)Cl>O>C[C@@H](O)Cl",
        "F/C=C/Cl>>F/C=C\\Cl",
    ] {
        let doc = engine
            .execute(Request::import("rsmi", smiles))
            .await
            .unwrap()
            .document
            .unwrap();
        doc.validate().unwrap();
        let mut canonical = Request::molecule("export", doc.clone());
        canonical.format = Some("rsmi".into());
        let expected = engine.execute(canonical).await.unwrap().output.unwrap();
        for format in ["rxn", "rsmi"] {
            let mut request = Request::molecule("export", doc.clone());
            request.format = Some(format.into());
            let output = engine.execute(request).await.unwrap().output.unwrap();
            let back = engine
                .execute(Request::import(format, &output))
                .await
                .unwrap()
                .document
                .unwrap();
            back.validate().unwrap();
            let mut request = Request::molecule("export", back);
            request.format = Some("rsmi".into());
            assert_eq!(
                engine.execute(request).await.unwrap().output.unwrap(),
                expected
            );
        }
        let clean = engine
            .execute(Request::molecule("clean", doc.clone()))
            .await
            .unwrap()
            .document
            .unwrap();
        assert_eq!(clean.reactions, doc.reactions);
        assert_eq!(clean.arrows, doc.arrows);
    }
    assert_eq!(
        moruno::clipboard::text_request("CCO>>CC=O")
            .format
            .as_deref(),
        Some("rsmi")
    );
    assert_eq!(
        moruno::clipboard::text_request("$RXN\nM  END")
            .format
            .as_deref(),
        Some("rxn")
    );
    assert_eq!(
        moruno::clipboard::text_request("N->[Fe]").format.as_deref(),
        Some("smiles")
    );
}
