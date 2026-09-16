use moruno::engine::{ChemistryEngine, PythonEngine, Request};
use moruno::{
    document::{Document, Point},
    editing::{self, Transform},
};

#[tokio::test]
async fn ring_attachments_preserve_methyl_and_methylene_identity() {
    let engine = PythonEngine::default();
    for (order, expected) in [(1, "CC1CCCCC1"), (2, "C=C1CCCCC1")] {
        let mut doc = Document::default();
        let a = doc.add_atom("C", Point::default());
        let b = doc.add_atom("C", Point::new(36.373066, -21.0));
        doc.add_bond(a, b, order, "plain");
        editing::ring(&mut doc, Point::new(36.373066, -21.0), 6, false, 5.0);
        let analysis = engine
            .execute(Request::molecule("analyze", doc))
            .await
            .unwrap()
            .analysis
            .unwrap();
        assert_eq!(analysis.smiles, expected);
    }
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::default());
    let b = doc.add_atom("C", Point::new(60.0, 0.0));
    doc.add_bond(a, b, 1, "plain");
    let ids = editing::ring(&mut doc, Point::new(200.0, 200.0), 5, false, 5.0);
    let p = doc.atom(ids[0]).unwrap().position;
    let q = doc.atom(ids[1]).unwrap().position;
    editing::snap_ring(
        &mut doc,
        &ids,
        Point::new(30.0 - (p.x + q.x) / 2.0, -(p.y + q.y) / 2.0),
        5.0,
    )
    .unwrap();
    let analysis = engine
        .execute(Request::molecule("analyze", doc))
        .await
        .unwrap()
        .analysis
        .unwrap();
    assert_eq!(analysis.smiles, "C1CCCC1");
    assert_eq!(analysis.formula, "C5H10");
}

#[tokio::test]
async fn copy_and_reflection_preserve_stereochemistry() {
    let engine = PythonEngine::default();
    for smiles in ["N[C@@H](C)C(=O)O", "F/C=C/F", "F/C=C\\F"] {
        let initial = engine
            .execute(Request::import_smiles(smiles))
            .await
            .unwrap();
        let expected = initial.analysis.unwrap().smiles;
        let part = initial.document.unwrap();
        let mut doc = Document::default();
        editing::append(&mut doc, &part, Point::default());
        let ids = editing::append(&mut doc, &part, Point::new(200.0, 100.0));
        editing::transform(&mut doc, &ids, Transform::FlipHorizontal);
        editing::transform(&mut doc, &ids, Transform::Rotate(30.0));
        editing::transform_about(&mut doc, &ids, Point::new(100.0, -50.0), 1.8, 73.0);
        doc.atoms.reverse();
        doc.bonds.reverse();
        doc.validate().unwrap();
        let result = engine
            .execute(Request::molecule("analyze", doc))
            .await
            .unwrap();
        assert_eq!(
            result.analysis.unwrap().smiles,
            format!("{expected}.{expected}")
        );
    }
}

#[tokio::test]
async fn ring_tools_build_chemically_valid_rings_and_fused_aromatics() {
    let engine = PythonEngine::default();
    for n in 3..=8 {
        let mut doc = Document::default();
        editing::ring(&mut doc, Point::default(), n, false, 5.0);
        let result = engine
            .execute(Request::molecule("analyze", doc))
            .await
            .unwrap();
        assert_eq!(result.analysis.unwrap().formula, format!("C{n}H{}", 2 * n));
    }
    let mut doc = Document::default();
    editing::ring(&mut doc, Point::default(), 6, true, 5.0);
    // Fuse after chemistry has converted the first aromatic ring to Kekule form.
    let mut doc = engine
        .execute(Request::molecule("analyze", doc))
        .await
        .unwrap()
        .document
        .unwrap();
    let bond = doc.bonds[0].clone();
    let a = doc.atom(bond.a).unwrap().position;
    let b = doc.atom(bond.b).unwrap().position;
    editing::ring(
        &mut doc,
        Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0),
        6,
        true,
        5.0,
    );
    let result = engine
        .execute(Request::molecule("analyze", doc))
        .await
        .unwrap();
    assert_eq!(result.analysis.unwrap().formula, "C10H8");
}

#[tokio::test]
async fn python_bridge_preserves_identity_across_cleanup() {
    let engine = PythonEngine::default();
    let imported = engine
        .execute(Request::import_smiles("N[C@@H](C)C(=O)O"))
        .await
        .unwrap();
    let initial_smiles = imported.analysis.unwrap().smiles;
    let doc = imported.document.unwrap();
    let ids: Vec<_> = doc.atoms.iter().map(|a| a.id).collect();
    let cleaned = engine
        .execute(Request::molecule("clean", doc))
        .await
        .unwrap();
    assert_eq!(cleaned.analysis.unwrap().smiles, initial_smiles);
    assert_eq!(
        cleaned
            .document
            .unwrap()
            .atoms
            .iter()
            .map(|a| a.id)
            .collect::<Vec<_>>(),
        ids
    );
}

#[tokio::test]
async fn bridge_can_restart_after_invalid_input() {
    let engine = PythonEngine::default();
    assert!(
        engine
            .execute(Request::import_smiles("not a molecule"))
            .await
            .is_err()
    );
    assert_eq!(
        engine
            .execute(Request::import_smiles("CCO"))
            .await
            .unwrap()
            .analysis
            .unwrap()
            .formula,
        "C2H6O"
    );
}
