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
    let abbreviations = expected["abbreviations"].as_array().unwrap();
    assert_eq!(document.abbreviations.len(), abbreviations.len());
    for (actual, expected) in document.abbreviations.iter().zip(abbreviations) {
        assert_eq!(actual.label, expected["label"].as_str().unwrap());
        assert_eq!(
            actual.members.len() as u64,
            expected["members"].as_u64().unwrap()
        );
        assert_eq!(actual.members, vec![actual.anchor]);
        let anchor = document
            .atoms
            .iter()
            .find(|a| a.id == actual.anchor)
            .unwrap();
        assert_eq!(anchor.element, expected["element"].as_str().unwrap());
        let mut orders: Vec<_> = document
            .bonds
            .iter()
            .filter(|b| b.a == actual.anchor || b.b == actual.anchor)
            .map(|b| b.order)
            .collect();
        orders.sort_unstable();
        assert_eq!(serde_json::to_value(orders)?, expected["bond_orders"]);
    }
    Ok(())
}
