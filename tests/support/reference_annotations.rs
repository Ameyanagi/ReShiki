//! Accepted 0.9 input outside the unchanged Python importer's contract.
use reshiki::{
    chemistry::{document, smiles::write},
    document::Document,
};
use serde_json::Value;

pub fn check(document: &Document, expected: &Value) -> anyhow::Result<()> {
    document.validate().map_err(anyhow::Error::msg)?;
    let molecule = document::prepare(document)?;
    let smiles = write::write(&molecule.state, Default::default())?.text;
    assert_eq!(smiles, expected["smiles"].as_str().unwrap());
    assert_eq!(
        document.atoms.len() as u64,
        expected["atoms"].as_u64().unwrap()
    );
    assert_eq!(
        document.bonds.len() as u64,
        expected["bonds"].as_u64().unwrap()
    );
    assert_eq!(
        document.abbreviations.len() as u64,
        expected["abbreviations"].as_u64().unwrap()
    );
    Ok(())
}
