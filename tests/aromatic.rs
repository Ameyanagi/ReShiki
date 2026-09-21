use reshiki::{
    aromatic,
    document::Point,
    editing,
    engine::{LocalEngine, Request},
    scene,
};

#[tokio::test]
async fn circles_follow_ring_geometry_copy_color_and_editable_exchange() {
    let engine = LocalEngine::default();
    for (smiles, count) in [("c1ccccc1", 1), ("c1ccoc1", 1), ("c1ccc2ccccc2c1", 2)] {
        let original = engine
            .request(Request::import_smiles(smiles))
            .await
            .unwrap();
        let doc = original.document.unwrap();
        let mut request = Request::molecule("aromatic", doc.clone());
        request.selected_ids = Some(doc.all_ids());
        let response = engine.request(request).await.unwrap();
        assert_eq!(
            response.analysis.unwrap().smiles,
            original.analysis.unwrap().smiles
        );
        let mut circled = response.document.unwrap();
        assert_eq!(aromatic::circles(&circled).len(), count);
        for b in &mut circled.bonds {
            b.color = [27, 110, 100];
        }
        assert!(
            aromatic::circles(&circled)
                .iter()
                .all(|c| c.color == [27, 110, 100])
        );
        let primitives = scene::primitives(&circled);
        assert_eq!(primitives.iter().filter(|p| matches!(p,scene::Primitive::Path { commands,.. } if commands.iter().any(|p|matches!(p,reshiki::graphics::PathCommand::Cubic(..))))).count(),count);
        let fragment = editing::selection(&circled, &circled.all_ids());
        let mut doubled = circled.clone();
        editing::append(&mut doubled, &fragment, Point::new(250., 100.));
        assert_eq!(aromatic::circles(&doubled).len(), count * 2);
        let exported = engine
            .request(Request {
                format: Some("cdxml".into()),
                ..Request::molecule("export", doubled)
            })
            .await
            .unwrap();
        let imported = engine
            .request(Request::import("cdxml", exported.output.as_ref().unwrap()))
            .await
            .unwrap()
            .document
            .unwrap();
        assert!(imported.graphics.is_empty());
        assert_eq!(aromatic::circles(&imported).len(), count * 2);
        let id = circled.atoms[0].id;
        circled.delete(&[id]);
        assert!(aromatic::circles(&circled).len() < count);
    }
}

#[tokio::test]
async fn displayed_carbon_nitrogen_and_oxygen_hydrogens_follow_bond_valence() {
    let engine = LocalEngine::default();
    for (smiles, expected) in [
        ("C", vec![4]),
        ("CCC", vec![3, 2, 3]),
        ("N", vec![3]),
        ("CN", vec![3, 2]),
        ("CNC", vec![3, 1, 3]),
        ("CO", vec![3, 1]),
        ("C=O", vec![2, 0]),
    ] {
        let mut doc = engine
            .request(Request::import_smiles(smiles))
            .await
            .unwrap()
            .document
            .unwrap();
        for a in &mut doc.atoms {
            a.display.carbons = Some(reshiki::atom_labels::Carbons::All);
            a.display.hydrogens = Some(true);
        }
        let checked = engine
            .request(Request::molecule("analyze", doc.clone()))
            .await
            .unwrap()
            .document
            .unwrap();
        assert_eq!(
            checked.atoms.iter().map(|a| a.label_h).collect::<Vec<_>>(),
            expected
        );
        assert!(
            scene::primitives(&doc)
                .iter()
                .any(|p| matches!(p,scene::Primitive::Text {text,..} if text=="H"))
        );
    }
}
