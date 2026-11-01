//! Fixtures shared by the naming tests.
use super::*;
use reshiki::naming::Provenance;

/// Ethanol as the native importer draws it.
pub(super) fn ethanol() -> Document {
    Document::from_json(include_bytes!(
        "../../../tests/fixtures/chemical-naming-rust/ethanol-rust-native.rsk"
    ))
    .unwrap()
}

/// A generated record for `smiles` named "ethan-1-ol", with no aliases or
/// warnings. Tests that need another title, name or source override fields.
pub(super) fn record(smiles: &str) -> Record {
    Record {
        title: "ethan-1-ol".into(),
        systematic_name: Some("ethan-1-ol".into()),
        name_status: Some(naming::NameStatus::Preferred),
        canonical_smiles: naming::canonical_smiles(smiles).unwrap(),
        smiles: smiles.into(),
        synonyms: vec![],
        warnings: vec![],
        provenance: Provenance::Openclatura,
    }
}
