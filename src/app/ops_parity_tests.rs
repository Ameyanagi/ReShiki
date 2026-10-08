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

#[test]
fn ops_compose_settings_follow_the_app_drawing_defaults() {
    use reshiki_agent::DrawingSettings;
    let mut style = reshiki::style::DrawingStyle::default();
    style.set_bond_length(18.);
    style.font_family = "Helvetica".into();
    style.font_size_pt = 12.;
    style.line_width_pt = 1.;
    let (mut app, _) = App::new();
    let defaults = DrawingSettings::for_document(&app.tab.doc);
    app.tab.doc.drawing_style = style;
    app.tab.doc.atom_labels.stereo = !app.tab.doc.atom_labels.stereo;
    app.sync_drawing_defaults();
    let settings = DrawingSettings::for_document(&app.tab.doc);
    assert_eq!(settings.bond_length, app.tab.bond_drawing.length);
    assert_eq!(settings.format, app.tab.caption_format);
    assert_eq!(settings.arrow_style, app.tab.arrows.style);
    assert_eq!(settings.drawing_style, app.tab.doc.drawing_style);
    assert_eq!(settings.labels, app.tab.doc.atom_labels);
    // Every derived field changed, so none matches by default alone.
    assert_ne!(settings.bond_length, defaults.bond_length);
    assert_ne!(settings.format, defaults.format);
    assert_ne!(settings.arrow_style, defaults.arrow_style);
    assert_ne!(settings.labels, defaults.labels);
}

/// The apply results of the operation API, read back from its session store.
mod apply {
    use super::*;
    use reshiki_agent::ops::{
        budget::Budgets,
        headless::HeadlessHost,
        host::{Call, ToolHost},
        policy::Access,
        store::Documents,
        wire::{DocHandle, Principal, RequestId},
    };
    use serde_json::{Value, json};
    use std::sync::atomic::{AtomicI64, Ordering};

    async fn call(host: &HeadlessHost, tool: &str, arguments: Value) -> Value {
        static NEXT: AtomicI64 = AtomicI64::new(1);
        let result = host
            .call(Call {
                principal: Principal::local(),
                request: RequestId::Int(NEXT.fetch_add(1, Ordering::Relaxed)),
                tool: tool.into(),
                arguments,
                progress: None,
            })
            .await
            .unwrap();
        assert!(!result.is_error, "{tool}: {:?}", result.value);
        Value::Object(result.value)
    }

    async fn import(host: &HeadlessHost, native: &str) -> DocHandle {
        let imported = call(host, "import", json!({"format": "reshiki", "text": native})).await;
        DocHandle::new(imported["value"]["document"].as_str().unwrap()).unwrap()
    }

    fn stored(host: &HeadlessHost, handle: &DocHandle) -> (Document, String) {
        let snapshot = host
            .store()
            .snapshot(&Principal::local(), handle, Access::Read)
            .unwrap();
        ((*snapshot.doc).clone(), snapshot.revision.to_string())
    }

    /// The document's undo and redo frames, counted by stepping through them
    /// with apply: none to redo, then every undo.
    async fn frames(host: &HeadlessHost, handle: &DocHandle) -> usize {
        let step = |edit: &str, base: String| json!({"document": handle.as_str(), "edit": edit, "source": null, "ids": null, "base_revision": base, "idempotency_key": null});
        let redone = call(host, "apply", step("redo", stored(host, handle).1)).await;
        assert_eq!(redone["value"]["recorded"], false);
        let mut frames = 0;
        loop {
            let undone = call(host, "apply", step("undo", stored(host, handle).1)).await;
            if undone["value"]["recorded"] == false {
                return frames;
            }
            frames += 1;
        }
    }

    /// The app applies a draft and refreshes its labels as the event loop
    /// would: the label task runs to completion before anything else.
    async fn app_apply(base: &Document, fragment: &Document, replace: Vec<u64>) -> App {
        use iced::futures::StreamExt;
        let (mut app, _) = App::new();
        app.tab.busy = false;
        app.tab.doc = base.clone();
        app.assistant.draft = Some(assistant::Draft {
            proposal: Default::default(),
            fragment: fragment.clone(),
            review: Default::default(),
            revision: app.tab.revision,
            epoch: app.tab.file_epoch,
            replace,
        });
        let _ = app.assistant_action(assistant::Action::Apply);
        assert!(app.assistant.draft.is_none(), "The draft was not applied");
        let mut labels = iced_runtime::task::into_stream(app.start_label_refresh())
            .expect("The chemistry changed, so a label refresh starts");
        let Some(iced_runtime::Action::Output(Message::LabelsReady(key, result))) =
            labels.next().await
        else {
            panic!("The label task must return its completion");
        };
        app.labels_ready(key, result);
        app
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ops_apply_inserts_as_the_app_applies_a_draft_with_its_label_refresh() {
        let (doc, [member, _, _, caption], ring, _) = analysis_drawing();
        // Ethanol: its oxygen gets a computed hydrogen.
        let mut fragment = Document::default();
        let atoms = [
            fragment.add_atom("C", Point::default()),
            fragment.add_atom("C", Point::new(42., 0.)),
            fragment.add_atom("O", Point::new(63., 36.373)),
        ];
        fragment.add_bond(atoms[0], atoms[1], 1, "plain");
        fragment.add_bond(atoms[1], atoms[2], 1, "plain");
        // Both sides start from the drawings the ops import reads.
        let base_native = String::from_utf8(doc.file_json().unwrap()).unwrap();
        let fragment_native = String::from_utf8(fragment.file_json().unwrap()).unwrap();
        let base = Document::from_native_file(base_native.as_bytes()).unwrap();
        let fragment = Document::from_native_file(fragment_native.as_bytes()).unwrap();
        let host = HeadlessHost::new("9.8.7", Budgets::default());
        for ids in [vec![], vec![member], vec![caption], ring] {
            let app = app_apply(&base, &fragment, base.expand_abbreviation_selection(&ids)).await;
            let document = import(&host, &base_native).await;
            let source = import(&host, &fragment_native).await;
            let ids: Vec<String> = ids.iter().map(u64::to_string).collect();
            call(
                &host,
                "apply",
                json!({
                    "document": document.as_str(),
                    "edit": "insert",
                    "source": source.as_str(),
                    "ids": ids,
                    "base_revision": "0",
                    "idempotency_key": null,
                }),
            )
            .await;
            let (applied, _) = stored(&host, &document);
            assert_eq!(applied, app.tab.doc, "{ids:?}");
            assert!(
                applied
                    .atoms
                    .iter()
                    .any(|atom| atom.element == "O" && atom.label_h == 1)
            );
            assert_eq!(
                frames(&host, &document).await,
                app.tab.history.frames(),
                "{ids:?}"
            );
        }
    }
}
