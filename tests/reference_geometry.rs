use reshiki::{
    document::{Document, Point},
    editing::reference::{self, Edge, Stretch},
    engine::{ChemistryEngine, LocalEngine, Request},
};

async fn identity(engine: &LocalEngine, doc: &Document) -> String {
    engine
        .execute(Request::molecule("analyze", doc.clone()))
        .await
        .unwrap()
        .analysis
        .unwrap()
        .smiles
}

async fn round_trip(engine: &LocalEngine, doc: &Document, format: &str) -> Document {
    let mut request = Request::molecule("export", doc.clone());
    request.format = Some(format.into());
    let output = engine.execute(request).await.unwrap().output.unwrap();
    engine
        .execute(Request::import(format, &output))
        .await
        .unwrap()
        .document
        .unwrap()
}

#[tokio::test]
async fn exact_alignment_and_axis_stretch_preserve_stereo_native_and_mol_identity() {
    let engine = LocalEngine::default();
    for smiles in ["CC[C@H](F)Cl", "CC[C@@H](F)Cl", "F/C=C/F", "F/C=C\\F"] {
        let original = engine
            .execute(Request::import_smiles(smiles))
            .await
            .unwrap()
            .document
            .unwrap();
        let expected = identity(&engine, &original).await;
        let bond = &original.bonds[0];
        let edge = Edge::Bond(bond.a, bond.b);
        let ids = original.all_ids();
        let pivot = Point::new(253.75, -118.25);
        let (copied, copied_ids) = reference::rotate(&original, &ids, pivot, 23.75, true).unwrap();
        assert_eq!(&copied.atoms[..original.atoms.len()], &original.atoms);
        assert!(copied_ids.iter().all(|id| !ids.contains(id)));
        let copied_identity = format!("{expected}.{expected}");
        assert_eq!(identity(&engine, &copied).await, copied_identity);
        let reopened_copy = Document::from_native_file(&copied.file_json().unwrap()).unwrap();
        assert_eq!(identity(&engine, &reopened_copy).await, copied_identity);
        let turn = reference::alignment_degrees(&original, edge, 17.3, true).unwrap();
        let (aligned, _) = reference::rotate(&original, &ids, pivot, turn, false).unwrap();
        let plan = Stretch::new(&aligned, bond.a, bond.b).unwrap();
        let stretched = plan.apply(&aligned, plan.length * 1.6).unwrap();
        for (stage, doc) in [
            ("baseline", &original),
            ("aligned", &aligned),
            ("stretched", &stretched),
        ] {
            assert_eq!(doc.bonds, original.bonds, "{smiles}: bond direction/stereo");
            assert_eq!(identity(&engine, doc).await, expected, "{stage} {smiles}");
            let reopened = Document::from_native_file(&doc.file_json().unwrap()).unwrap();
            assert_eq!(
                identity(&engine, &reopened).await,
                expected,
                "native {smiles}"
            );
            for format in ["mol", "cdxml"] {
                let imported = round_trip(&engine, doc, format).await;
                assert_eq!(
                    identity(&engine, &imported).await,
                    expected,
                    "{stage} {format} {smiles}"
                );
            }
        }
    }
}
