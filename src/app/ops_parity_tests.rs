//! The operation API's GUI-free paths (reshiki_agent::ops) against the app's
//! own paths for the same input.
use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use reshiki_agent::ops::import::finalize;

const AROMATIC_CDXML: &str = include_str!("../../tests/fixtures/aromatic-circle-native.cdxml");
const ABBREVIATIONS_CDX: &[u8] = include_bytes!("../../tests/fixtures/abbreviations-native.cdx");

fn import(format: &str, text: &str) -> Response {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(LocalEngine::default().request(Request::import(format, text)))
        .unwrap()
}

#[test]
fn ops_import_finalizes_a_response_as_the_app_applies_it_to_a_new_drawing() {
    let cdx = STANDARD.encode(ABBREVIATIONS_CDX);
    for (format, text) in [
        ("smiles", "CCO"),
        // Stereocenters carry computed CIP labels that must survive.
        ("smiles", "N[C@@H](C)C(=O)O"),
        ("rsmi", "CCO>>CC=O"),
        ("cdxml", AROMATIC_CDXML),
        ("cdx", cdx.as_str()),
    ] {
        let response = import(format, text);
        let ops = finalize(response.document.as_ref().unwrap()).unwrap();
        let (mut app, _) = App::new();
        let revision = app.tab.revision;
        let _ = app.engine_done(revision, Job::Import, Ok(response));
        assert!(!app.error, "{format}: {}", app.status);
        assert_eq!(app.tab.doc, ops, "{format}");
    }
}
