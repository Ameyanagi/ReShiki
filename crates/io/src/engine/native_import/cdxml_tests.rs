use super::*;
use crate::engine::LocalEngine;

const ANY_ATOM_NOTICE: &str = "Drawing imported with A/a as labelled dummy atoms; only the labels are kept, not their any-atom (non-hydrogen) query meaning. Molecular properties are unavailable";
const VARIABLE_NOTICE: &str = "Drawing imported with variable labels as uninterpreted atom text; query semantics and molecular properties are unavailable";
const AA_FIXTURE: &str = include_str!("../../../../../tests/fixtures/aa-nicknames.cdxml");

fn nickname(label: &str, attributes: &str) -> String {
    format!(
        "<CDXML><page><fragment id='1'>\
         <n id='2' p='0 0' NodeType='GenericNickname' GenericNickname='{label}' {attributes}/>\
         </fragment></page></CDXML>"
    )
}

#[tokio::test]
async fn cdxml_any_atom_nicknames_import_as_labelled_dummies_with_warning() {
    let engine = LocalEngine::in_process();
    for label in ["A", "a"] {
        let response = engine
            .request(Request::import("cdxml", &nickname(label, "")))
            .await
            .unwrap();
        assert!(response.analysis.is_none());
        assert_eq!(response.warnings, [ANY_ATOM_NOTICE]);
        let document = response.document.unwrap();
        document.validate().unwrap();
        assert_eq!(document.atoms.len(), 1);
        let atom = &document.atoms[0];
        assert_eq!(atom.element, "*");
        assert_eq!(atom.display.variable.as_deref(), Some(label));
        assert!(atom.no_implicit);
        assert_eq!(atom.label_h, 0);
    }
}

#[tokio::test]
async fn cdxml_other_generic_nicknames_keep_their_import_policy() {
    let engine = LocalEngine::in_process();
    for label in ["Q", "M", "MH", "R1"] {
        let error = engine
            .request(Request::import("cdxml", &nickname(label, "")))
            .await
            .unwrap_err();
        assert!(
            error.contains("CDXML contains unsupported chemistry: query atom"),
            "{label}: {error}"
        );
    }
    for label in ["R", "X"] {
        let response = engine
            .request(Request::import("cdxml", &nickname(label, "")))
            .await
            .unwrap();
        assert!(response.analysis.is_none());
        assert_eq!(response.warnings, [VARIABLE_NOTICE]);
        let document = response.document.unwrap();
        assert_eq!(document.atoms[0].element, "*");
        assert_eq!(document.atoms[0].display.variable.as_deref(), Some(label));
        for attributes in [
            "Element='6'",
            "Charge='1'",
            "Isotope='2'",
            "NumHydrogens='1'",
        ] {
            let error = engine
                .request(Request::import("cdxml", &nickname(label, attributes)))
                .await
                .unwrap_err();
            assert!(
                error.contains("CDXML contains unsupported chemistry: query atom"),
                "{label} {attributes}: {error}"
            );
        }
    }
}

#[tokio::test]
async fn cdxml_aa_fixture_imports_connected_labels_and_survives_native_roundtrip() {
    // The old application path passed this unchanged to molecular preparation,
    // where exact A refused the whole import.
    let error = cdxml::prepare_cdxml(AA_FIXTURE).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("CDXML contains unsupported chemistry: query atom")
    );
    let response = LocalEngine::in_process()
        .request(Request::import("cdxml", AA_FIXTURE))
        .await
        .unwrap();
    assert!(response.analysis.is_none());
    assert_eq!(response.warnings, [ANY_ATOM_NOTICE]);
    let document = response.document.unwrap();
    document.validate().unwrap();
    assert_eq!(document.atoms.len(), 5);
    assert_eq!(document.bonds.len(), 4);
    assert_eq!(
        document
            .atoms
            .iter()
            .map(|atom| atom.element.as_str())
            .collect::<Vec<_>>(),
        ["C", "C", "O", "*", "*"]
    );
    for (index, label, neighbor) in [(3, "A", 0), (4, "a", 1)] {
        let atom = &document.atoms[index];
        assert_eq!(atom.display.variable.as_deref(), Some(label));
        let neighbor = document.atoms[neighbor].id;
        assert!(document.bonds.iter().any(|bond| {
            bond.order == 1
                && ((bond.a, bond.b) == (atom.id, neighbor)
                    || (bond.b, bond.a) == (atom.id, neighbor))
        }));
    }
    let reopened = Document::from_native_file(&document.file_json().unwrap()).unwrap();
    assert_eq!(reopened, document.current());

    for label in ["Q", "M", "MH"] {
        let xml = AA_FIXTURE.replace(
            "GenericNickname=\"a\"",
            &format!("GenericNickname=\"{label}\""),
        );
        let error = LocalEngine::in_process()
            .request(Request::import("cdxml", &xml))
            .await
            .unwrap_err();
        assert!(
            error.contains("CDXML contains unsupported chemistry: query atom"),
            "{label}: {error}"
        );
    }
}
