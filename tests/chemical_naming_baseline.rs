//! Retained pre-feature controls for issue #50. These run without networking.
use anyhow::{Context, ensure};
use reshiki::{
    chemistry::smiles,
    engine::{LocalEngine, Request},
};

#[test]
fn naming_reference_graphs_are_valid_and_distinct_before_ui_changes() -> anyhow::Result<()> {
    let fixtures = [
        ("ethanol", "CCO", 3),
        ("aspirin", "CC(=O)Oc1ccccc1C(=O)O", 13),
        ("(2R)-2-hydroxypropanoic acid", "C[C@@H](O)C(=O)O", 6),
        ("(2S)-2-hydroxypropanoic acid", "C[C@H](O)C(=O)O", 6),
        ("(E)-but-2-ene", "C/C=C/C", 4),
        ("(Z)-but-2-ene", "C/C=C\\C", 4),
        ("stereo unspecified lactic acid", "CC(O)C(=O)O", 6),
        ("carbon-13 ethanol", "[13CH3]CO", 3),
        ("acetate", "CC(=O)[O-]", 4),
    ];
    let mut outputs = Vec::new();
    for (name, text, atoms) in fixtures {
        let molecule = smiles::read(text).with_context(|| name)?;
        ensure!(molecule.prepared.state.graph.atoms.len() == atoms, "{name}");
        let output = smiles::write::write(&molecule.prepared.state, Default::default())?.text;
        ensure!(
            !outputs.contains(&output),
            "Fixture lost its identity: {name}"
        );
        outputs.push(output);
    }
    Ok(())
}

#[tokio::test]
async fn existing_import_rejects_names_and_accepts_the_graph_control() -> anyhow::Result<()> {
    let engine = LocalEngine::default();
    ensure!(
        engine
            .request(Request::import_smiles("ethanol"))
            .await
            .is_err()
    );
    let response = engine
        .request(Request::import_smiles("CCO"))
        .await
        .map_err(anyhow::Error::msg)?;
    let doc = response
        .document
        .context("Missing editable ethanol drawing")?;
    ensure!(doc.atoms.len() == 3 && doc.bonds.len() == 2);
    doc.validate().map_err(anyhow::Error::msg)?;
    Ok(())
}
