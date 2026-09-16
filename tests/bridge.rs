use moruno::engine::{ChemistryEngine, PythonEngine, Request};

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
