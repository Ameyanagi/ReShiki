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

/// C-O-C where the O and its first C are the abbreviation OMe, plus a caption,
/// and a three-carbon ring under a multi-center attachment point bonded to
/// iron: `(doc, [member, anchor, outside, caption], ring, point)`.
fn analysis_drawing() -> (Document, [u64; 4], Vec<u64>, u64) {
    use reshiki::{attachments, document::Annotation};
    let mut doc = Document::default();
    let member = doc.add_atom("C", Point::default());
    let anchor = doc.add_atom("O", Point::new(42., 0.));
    let outside = doc.add_atom("C", Point::new(84., 0.));
    doc.add_bond(member, anchor, 1, "plain");
    doc.add_bond(anchor, outside, 1, "plain");
    doc.contract(&[anchor, member], "OMe", "MeO").unwrap();
    let caption = doc.next_id();
    doc.annotations.push(Annotation {
        id: caption,
        position: Point::new(0., 60.),
        text: "ether".into(),
        format: Default::default(),
    });
    let ring = vec![
        doc.add_atom("C", Point::new(200., 0.)),
        doc.add_atom("C", Point::new(230., 0.)),
        doc.add_atom("C", Point::new(215., 26.)),
    ];
    doc.add_bond(ring[0], ring[1], 1, "plain");
    doc.add_bond(ring[1], ring[2], 1, "plain");
    doc.add_bond(ring[2], ring[0], 1, "plain");
    let point = attachments::add(&mut doc, &ring, attachments::Kind::MultiCenter).unwrap();
    let iron = doc.add_atom("Fe", Point::new(215., 60.));
    doc.add_bond(point, iron, 1, "plain");
    (doc, [member, anchor, outside, caption], ring, point)
}

#[test]
fn ops_analyze_cuts_out_a_selection_as_the_app_property_document() {
    use reshiki_agent::ops::{analyze, wire::ObjectId};
    let (doc, [member, anchor, outside, caption], ring, point) = analysis_drawing();
    for selected in [
        // A member pulls in its whole abbreviation; the caption has no atom.
        vec![member, caption],
        vec![outside, anchor],
        // Complete targets carry their attachment point into the drawing.
        ring.clone(),
        vec![point],
        doc.atoms.iter().map(|atom| atom.id).collect(),
    ] {
        let (mut app, _) = App::new();
        app.tab.doc = doc.clone();
        app.tab.selected = selected.clone();
        let ids: Vec<ObjectId> = selected.iter().copied().map(ObjectId).collect();
        let part = analyze::part(&doc, Some(&ids)).unwrap();
        let key = app.property_request_key();
        assert_eq!(part.doc, app.property_document(&key), "{selected:?}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ops_export_details_match_the_app_figure_export() {
    use reshiki_agent::ops::{
        budget::Budgets,
        headless::HeadlessHost,
        host::{Call, ToolHost},
        wire::{Principal, RequestId},
    };
    // The aromatic five-ring stays unresolved, so both figures carry the
    // preparation notice after any PNG size.
    let mut doc = Document::default();
    editing::ring(&mut doc, Point::default(), 5, true, 42.);
    let native = String::from_utf8(doc.file_json().unwrap()).unwrap();
    let host = HeadlessHost::new(
        "9.8.7",
        Budgets {
            export_png_pixels: reshiki::export::FILE_PIXELS,
            ..Budgets::default()
        },
    );
    let call = |id, tool: &str, arguments| Call {
        principal: Principal::local(),
        request: RequestId::Int(id),
        tool: tool.into(),
        arguments,
        progress: None,
    };
    let imported = host
        .call(call(
            1,
            "import",
            serde_json::json!({"format": "reshiki", "text": native}),
        ))
        .await
        .unwrap();
    assert!(!imported.is_error, "{:?}", imported.value);
    let handle = imported.value["value"]["document"].clone();
    for (id, format) in [(2, "png"), (3, "svg")] {
        let exported = host
            .call(call(
                id,
                "export",
                serde_json::json!({"document": handle, "format": format, "pages": null}),
            ))
            .await
            .unwrap();
        assert!(!exported.is_error, "{format}: {:?}", exported.value);
        let warnings: Vec<String> = exported.value["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|warning| warning["message"].as_str().unwrap().to_owned())
            .collect();
        // The app opens the same native file and exports it with FILE_PIXELS.
        let app = reshiki::export::publication(
            &LocalEngine::default(),
            Document::from_native_file(native.as_bytes()).unwrap(),
            format,
            false,
            reshiki::export::FILE_PIXELS,
        )
        .await
        .unwrap();
        assert_eq!(warnings, app.details, "{format}");
        assert_eq!(warnings.len(), if format == "png" { 2 } else { 1 });
        assert!(exported.files[0].bytes == app.bytes, "{format}");
    }
}
