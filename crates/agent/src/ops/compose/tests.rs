use super::*;
use crate::{
    Molecule, Step,
    ops::{
        exec::Hooks,
        headless::HeadlessHost,
        host::{Call, ToolHost},
        progress::{Progress, Sink},
        wire::{RequestId, versions_json},
    },
    style::DrawingStyle,
};
use std::sync::{
    Mutex,
    atomic::{AtomicI64, Ordering},
};

fn host() -> HeadlessHost {
    HeadlessHost::new("9.8.7", Budgets::default())
}

fn call(tool: &str, arguments: Value) -> Call {
    static NEXT: AtomicI64 = AtomicI64::new(1);
    Call {
        principal: Principal::local(),
        request: RequestId::Int(NEXT.fetch_add(1, Ordering::Relaxed)),
        tool: tool.into(),
        arguments,
        progress: None,
    }
}

fn molecule(smiles: &str, label: &str) -> Molecule {
    Molecule {
        smiles: smiles.into(),
        label: label.into(),
        coefficient: 1,
        rotation: 0.,
        compact: false,
    }
}

/// A replica of `reaction()` in tests/assistant.rs.
fn reaction() -> Proposal {
    Proposal {
        sketch: None,
        replace_ids: vec![],
        composition: Default::default(),
        explanation: "Esterification".into(),
        molecules: vec![],
        reactions: vec![Step {
            reactants: vec![
                molecule("CC(=O)O", "Acetic acid"),
                molecule("CCO", "Ethanol"),
            ],
            products: vec![
                molecule("CCOC(C)=O", "Ethyl acetate"),
                molecule("O", "Water"),
            ],
            conditions: "H₂SO₄\nheat".into(),
            arrow: "forward".into(),
            title: String::new(),
            role: Default::default(),
            direction: None,
        }],
    }
}

/// Two bonded carbons drawn as an editable diagram.
fn sketched() -> Proposal {
    serde_json::from_value(json!({
        "explanation": "",
        "replace_ids": [],
        "molecules": [],
        "reactions": [],
        "composition": {"arrangement": "rows", "columns": 2, "width_pt": 540, "preserve_details": false},
        "sketch": {
            "atoms": [
                {"element": "C", "x": 0, "y": 0, "charge": 0, "isotope": 0, "hydrogens": 0, "color": null, "variable": null},
                {"element": "C", "x": 1, "y": 0, "charge": 0, "isotope": 0, "hydrogens": 0, "color": null, "variable": null}
            ],
            "bonds": [{"a": 0, "b": 1, "order": 1, "display": "plain", "ring_arc": false}],
            "shapes": [], "tilts": [], "centroids": [], "arrows": [], "captions": [], "abbreviations": [], "ligands": []
        }
    }))
    .unwrap()
}

/// A drawing style that differs from the default in every derived field.
fn custom_style() -> DrawingStyle {
    let mut style = DrawingStyle::default();
    style.set_bond_length(18.);
    style.font_family = "Helvetica".into();
    style.font_size_pt = 12.;
    style.line_width_pt = 1.;
    style
}

fn arguments(proposal: &Proposal, style_document: Option<&DocHandle>) -> Value {
    json!({
        "proposal": proposal,
        "style_document": style_document.map(DocHandle::as_str),
    })
}

/// The result value of a successful compose.
async fn compose_ok(host: &HeadlessHost, request: Call) -> Value {
    let result = host.call(request).await.unwrap();
    assert!(!result.is_error, "{:?}", result.value);
    assert!(result.images.is_empty() && result.files.is_empty());
    Value::Object(result.value)
}

/// The stored document behind a compose result.
fn stored(host: &HeadlessHost, composed: &Value) -> Arc<Document> {
    let handle = DocHandle::new(composed["value"]["document"].as_str().unwrap()).unwrap();
    host.store()
        .snapshot(&Principal::local(), &handle, Access::Read)
        .unwrap()
        .doc
}

/// The assistant's draft for `proposal`: render, then finish the layout
/// (src/assistant/codex.rs).
async fn drafted(proposal: &Proposal, settings: &DrawingSettings) -> (Document, Vec<String>) {
    let mut doc = crate::render(&LocalEngine::default(), proposal, settings)
        .await
        .unwrap();
    let changes = composition::finish_layout(proposal, &mut doc)
        .into_iter()
        .collect();
    (doc, changes)
}

fn bond_lengths(doc: &Document) -> Vec<f32> {
    doc.bonds
        .iter()
        .map(|bond| {
            let a = doc.atom(bond.a).unwrap().position;
            a.distance(doc.atom(bond.b).unwrap().position)
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_default_style_composes_as_the_assistant_without_a_canvas() {
    let host = host();
    let proposal = reaction();
    let composed = compose_ok(&host, call("compose", arguments(&proposal, None))).await;
    let (doc, changes) = drafted(&proposal, &DrawingSettings::default()).await;
    assert_eq!(*stored(&host, &composed), doc);
    assert_eq!(doc.atoms.len(), 14);
    let value = &composed["value"];
    assert_eq!(value["revision"], "0");
    assert_eq!(value["atoms"], 14);
    assert_eq!(value["bonds"], doc.bonds.len());
    assert_eq!(value["arrows"], 1);
    assert_eq!(
        value["bounds"],
        json!(scene::selection_bounds(&doc, &doc.all_ids()).unwrap())
    );
    assert_eq!(
        value["review_issues"],
        json!(review::quality(&doc, &proposal.composition))
    );
    assert_eq!(value["changes"], json!(changes));
    assert_eq!(composed["warnings"], json!([]));
    assert_eq!(composed["validation"], json!({"status": "valid"}));
    assert_eq!(
        composed["versions"],
        versions_json(&Versions::current("9.8.7"))
    );
    assert_eq!(host.store().list(&Principal::local()).len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_style_document_composes_with_its_drawing_style() {
    let host = host();
    let style = Document {
        drawing_style: custom_style(),
        ..Default::default()
    };
    let handle = host
        .store()
        .create(&Principal::local(), style.clone())
        .unwrap()
        .handle;
    let proposal = reaction();
    let composed = compose_ok(&host, call("compose", arguments(&proposal, Some(&handle)))).await;
    let settings = DrawingSettings::for_document(&style);
    let (doc, changes) = drafted(&proposal, &settings).await;
    let stored = stored(&host, &composed);
    assert_eq!(*stored, doc);
    assert_eq!(composed["value"]["changes"], json!(changes));
    assert_eq!(stored.drawing_style, style.drawing_style);
    let text = style.drawing_style.text_style();
    assert_eq!((text.family.as_str(), text.size_pt), ("Helvetica", 12.));
    assert!(
        stored
            .atoms
            .iter()
            .all(|atom| atom.text_style.as_ref() == Some(&text))
    );
    let length = style.drawing_style.bond_length_world;
    assert!((length - crate::style::DEFAULT.bond_length_world).abs() > 1.);
    assert!(
        bond_lengths(&stored)
            .iter()
            .all(|bond| (bond - length).abs() < 0.01),
        "{:?} != {length}",
        bond_lengths(&stored)
    );
    // The style document is only read.
    let listed = host.store().list(&Principal::local());
    assert_eq!(listed.len(), 2);
    assert_eq!(
        (&listed[0].handle, listed[0].revision),
        (&handle, Revision(0))
    );
}

#[test]
fn settings_for_a_document_follow_its_drawing_style() {
    let mut doc = Document {
        drawing_style: custom_style(),
        ..Default::default()
    };
    doc.atom_labels.stereo = !doc.atom_labels.stereo;
    let settings = DrawingSettings::for_document(&doc);
    let style = &doc.drawing_style;
    assert_eq!(settings.drawing_style, *style);
    assert_eq!(settings.format.style, style.text_style());
    assert_eq!(settings.format.spans, Vec::new());
    assert_eq!(settings.bond_length, style.bond_length_world);
    assert_eq!(settings.bond_color, crate::palette::Color::Ink);
    assert_eq!(settings.arrow_style.width_pt, 1.);
    assert_eq!(
        crate::arrows::ArrowStyle {
            width_pt: crate::arrows::ArrowStyle::default().width_pt,
            ..settings.arrow_style
        },
        crate::arrows::ArrowStyle::default()
    );
    assert_eq!(settings.labels, doc.atom_labels);
}

#[test]
fn an_off_grid_rotation_gets_the_assistant_message() {
    let mut proposal = reaction();
    proposal.reactions[0].reactants[0].rotation = 45.;
    let error = decode(arguments(&proposal, None)).unwrap_err();
    assert_eq!(error.kind, ErrorKind::InvalidArguments);
    assert_eq!(
        error.message,
        "Use at most 32 molecules with short labels and valid SMILES"
    );
    assert_eq!(proposal.validate(), Err(error.message));
}

#[test]
fn proposal_errors_read_as_on_the_assistant_path() {
    let mut proposal = serde_json::to_value(reaction()).unwrap();
    proposal["shading"] = json!(true);
    // As the Codex Proposal turn decodes (src/assistant/codex.rs).
    let codex = serde_json::from_value::<Proposal>(proposal.clone())
        .unwrap_err()
        .to_string();
    let error = decode(json!({"proposal": proposal, "style_document": null})).unwrap_err();
    assert_eq!(error.kind, ErrorKind::InvalidArguments);
    assert_eq!(error.message, codex);
    assert!(error.message.starts_with("unknown field `shading`"));
}

#[test]
fn replacement_ids_are_rejected() {
    let mut proposal = reaction();
    proposal.replace_ids = vec![1];
    let error = decode(arguments(&proposal, None)).unwrap_err();
    assert_eq!(error.kind, ErrorKind::InvalidArguments);
    assert_eq!(error.message, REPLACE_IDS);
    // validate() runs first, as on the assistant's path.
    proposal.replace_ids = vec![0];
    assert_eq!(
        decode(arguments(&proposal, None)).unwrap_err().message,
        "The proposal is too large; request a smaller drawing"
    );
}

#[test]
fn a_proposal_without_a_drawing_is_rejected() {
    let proposal = Proposal {
        explanation: "Which ester did you mean?".into(),
        ..Default::default()
    };
    let error = decode(arguments(&proposal, None)).unwrap_err();
    assert_eq!(error.kind, ErrorKind::InvalidArguments);
    assert_eq!(error.message, NO_DRAWING);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_invalid_proposal_is_a_tool_error_that_creates_nothing() {
    let host = host();
    let mut proposal = reaction();
    proposal.reactions[0].reactants[0].rotation = 45.;
    let result = host
        .call(call("compose", arguments(&proposal, None)))
        .await
        .unwrap();
    assert!(result.is_error);
    assert_eq!(
        result.value["error"],
        json!({"code": "invalid_arguments", "message": "Use at most 32 molecules with short labels and valid SMILES"})
    );
    assert!(host.store().list(&Principal::local()).is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unknown_style_document_is_a_tool_error() {
    let host = host();
    let handle = DocHandle::new("doc_missing").unwrap();
    let result = host
        .call(call("compose", arguments(&reaction(), Some(&handle))))
        .await
        .unwrap();
    assert!(result.is_error);
    assert_eq!(result.value["error"]["code"], "unknown_document");
    assert!(host.store().list(&Principal::local()).is_empty());
}

/// The progress path needs a sink: without one, [`layout`] passes `None` to
/// the layout, so no channel exists and no preview documents are composed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn progress_reports_strictly_increase() {
    let host = host();
    let seen = Arc::new(Mutex::new(Vec::<Progress>::new()));
    let sink: Sink = {
        let seen = Arc::clone(&seen);
        Arc::new(move |progress| seen.lock().unwrap().push(progress))
    };
    let proposal = reaction();
    let mut request = call("compose", arguments(&proposal, None));
    request.progress = Some(sink);
    let composed = compose_ok(&host, request).await;
    let seen = seen.lock().unwrap().clone();
    assert!(!seen.is_empty());
    assert!(
        seen.windows(2)
            .all(|pair| pair[0].completed < pair[1].completed),
        "{seen:?}"
    );
    assert!(
        seen.iter()
            .all(|progress| progress.total == Some(4.) && progress.message.is_none())
    );
    // Every report arrives before the call ends.
    assert_eq!(seen.last().map(|progress| progress.completed), Some(4.));
    // Progress never changes the drawing.
    let (doc, _) = drafted(&proposal, &DrawingSettings::default()).await;
    assert_eq!(*stored(&host, &composed), doc);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancel_before_the_layout_creates_no_document() {
    let host = host();
    let style = host
        .store()
        .create(&Principal::local(), Document::default())
        .unwrap()
        .handle;
    let before = host.store().list(&Principal::local());
    let exec = host.exec().clone();
    let request = call("compose", arguments(&reaction(), Some(&style)));
    let id = request.request.clone();
    exec.set_hooks(Hooks {
        acquired: Some(Arc::new({
            let exec = exec.clone();
            move || exec.cancel(&Principal::local(), &id)
        })),
        ..Hooks::default()
    });
    let error = host.call(request).await.unwrap_err();
    assert_eq!(error.kind, ErrorKind::Cancelled);
    assert_eq!(host.store().list(&Principal::local()), before);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancel_during_the_layout_is_honored_before_the_document_is_created() {
    let host = host();
    let exec = host.exec().clone();
    let mut request = call("compose", arguments(&reaction(), None));
    let id = request.request.clone();
    let sink: Sink = Arc::new(move |_| exec.cancel(&Principal::local(), &id));
    request.progress = Some(sink);
    let error = host.call(request).await.unwrap_err();
    assert_eq!(error.kind, ErrorKind::Cancelled);
    assert!(host.store().list(&Principal::local()).is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_sketch_carries_the_review_note() {
    let host = host();
    let proposal = sketched();
    let composed = compose_ok(&host, call("compose", arguments(&proposal, None))).await;
    assert_eq!(
        composed["warnings"],
        json!([{"message": sketch::REVIEW_NOTE}])
    );
    let (doc, changes) = drafted(&proposal, &DrawingSettings::default()).await;
    assert!(changes.is_empty());
    assert_eq!(composed["value"]["changes"], json!([]));
    assert_eq!(*stored(&host, &composed), doc);
}
