use super::*;
use crate::{
    document::{Annotation, Point},
    engine::Analysis,
    ops::{
        headless::HeadlessHost,
        host::{Call, ToolHost},
        wire::{RequestId, versions_json},
    },
};
use std::sync::atomic::{AtomicI64, Ordering};

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

/// A host holding `doc` as a session document, and its handle.
fn hosting(doc: Document) -> (HeadlessHost, DocHandle) {
    let host = HeadlessHost::new("9.8.7", Budgets::default());
    let created = host.store().create(&Principal::local(), doc).unwrap();
    (host, created.handle)
}

async fn analyze_call(host: &HeadlessHost, handle: &DocHandle, ids: Value) -> ToolResult {
    host.call(call(
        "analyze",
        json!({"document": handle.as_str(), "ids": ids}),
    ))
    .await
    .unwrap()
}

async fn analyze_ok(host: &HeadlessHost, handle: &DocHandle, ids: Value) -> Value {
    let result = analyze_call(host, handle, ids).await;
    assert!(!result.is_error, "{:?}", result.value);
    assert!(result.images.is_empty() && result.files.is_empty());
    Value::Object(result.value)
}

fn strings(ids: &[u64]) -> Value {
    ids.iter().map(|id| Value::from(id.to_string())).collect()
}

/// The engine's own analysis of `doc`, on the wire.
async fn engine_analysis(doc: Document) -> Value {
    let analysis = LocalEngine::default()
        .request(Request::molecule("analyze", doc))
        .await
        .unwrap()
        .analysis
        .unwrap();
    serde_json::to_value(AnalysisJson::from(analysis)).unwrap()
}

/// Ethanol, C-C-O, with every ID.
fn ethanol() -> (Document, [u64; 3]) {
    let mut doc = Document::default();
    let ids = [
        doc.add_atom("C", Point::new(0., 0.)),
        doc.add_atom("C", Point::new(42., 0.)),
        doc.add_atom("O", Point::new(84., 0.)),
    ];
    doc.add_bond(ids[0], ids[1], 1, "plain");
    doc.add_bond(ids[1], ids[2], 1, "plain");
    (doc, ids)
}

/// C-O-C where the O and its first C are the abbreviation OMe, plus a
/// caption: `[member, anchor, outside, caption]`.
fn methoxy() -> (Document, [u64; 4]) {
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
    (doc, [member, anchor, outside, caption])
}

#[test]
fn ids_over_the_budget_are_refused_at_decode_time() {
    let budgets = Budgets::default();
    let decoded = |n: usize| {
        decode(
            json!({"document": "doc_1", "ids": vec![json!("1"); n]}),
            &budgets,
        )
        .map(drop)
    };
    assert_eq!(decoded(budgets.max_ids), Ok(()));
    let error = decoded(budgets.max_ids + 1).unwrap_err();
    assert_eq!(error.kind, ErrorKind::Budget);
    let omitted = decode(json!({"document": "doc_1"}), &budgets).unwrap_err();
    assert_eq!(omitted.kind, ErrorKind::InvalidArguments);
    assert_eq!(omitted.message, "missing field `ids`");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_whole_drawing_is_analyzed_without_changing_it() {
    let (doc, ids) = ethanol();
    let (host, handle) = hosting(doc.clone());
    let before = host
        .store()
        .snapshot(&Principal::local(), &handle, Access::Read)
        .unwrap();
    let result = analyze_ok(&host, &handle, Value::Null).await;
    assert_eq!(result["value"]["analysis"]["formula"], "C2H6O");
    assert_eq!(result["value"]["analysis"], engine_analysis(doc).await);
    assert_eq!(result["value"]["analyzed_atoms"], strings(&ids));
    assert_eq!(result["warnings"], json!([]));
    assert_eq!(result["validation"], json!({"status": "valid"}));
    assert_eq!(
        result["versions"],
        versions_json(&Versions::current("9.8.7"))
    );
    // Read-only: the stored document, its revision and its labels stay.
    let after = host
        .store()
        .snapshot(&Principal::local(), &handle, Access::Read)
        .unwrap();
    assert_eq!(after.revision, before.revision);
    assert!(Arc::ptr_eq(&after.doc, &before.doc));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_selection_inside_an_abbreviation_analyzes_the_whole_abbreviation() {
    let (doc, [member, anchor, outside, caption]) = methoxy();
    let (host, handle) = hosting(doc.clone());
    // The caption carries no atom; the outside carbon is not selected.
    let result = analyze_ok(&host, &handle, strings(&[member, caption])).await;
    assert_eq!(
        result["value"]["analyzed_atoms"],
        strings(&[member, anchor])
    );
    let part = editing::analysis_document(&doc, &[member, anchor]);
    assert_eq!(result["value"]["analysis"], engine_analysis(part).await);
    assert_eq!(result["value"]["analysis"]["formula"], "CH4O");
    let whole = analyze_ok(&host, &handle, strings(&[member, anchor, outside])).await;
    assert_eq!(whole["value"]["analysis"]["formula"], "C2H6O");
}

#[test]
fn part_cuts_out_the_selection_as_the_inspector_does() {
    let (doc, [member, anchor, _, caption]) = methoxy();
    let part = part(&doc, Some(&[ObjectId(member), ObjectId(caption)])).unwrap();
    assert_eq!(part.atoms, [member, anchor]);
    assert_eq!(
        part.doc,
        editing::analysis_document(&doc, &[member, anchor])
    );
    let whole = super::part(&doc, None).unwrap();
    assert_eq!(whole.doc, doc);
    assert_eq!(
        whole.atoms,
        doc.atoms.iter().map(|atom| atom.id).collect::<Vec<_>>()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unknown_ids_name_the_first_missing_one() {
    let (doc, [member, _, outside, caption]) = methoxy();
    let (host, handle) = hosting(doc);
    let missing = caption + 100;
    for (ids, first) in [
        (strings(&[missing]), missing),
        (
            strings(&[member, missing + 1, missing, outside]),
            missing + 1,
        ),
    ] {
        let result = analyze_call(&host, &handle, ids).await;
        assert!(result.is_error);
        assert_eq!(
            result.value["error"],
            json!({"code": "unknown_object", "message": format!("Object {first} is not in the drawing")})
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_selection_without_atoms_is_refused() {
    let (doc, [.., caption]) = methoxy();
    let (host, handle) = hosting(doc);
    for ids in [strings(&[caption]), json!([])] {
        let result = analyze_call(&host, &handle, ids).await;
        assert!(result.is_error);
        assert_eq!(
            result.value["error"],
            json!({"code": "invalid_arguments", "message": NO_ATOMS})
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_engine_error_fails_with_its_message() {
    let (host, handle) = hosting(Document::default());
    let result = analyze_call(&host, &handle, Value::Null).await;
    assert!(result.is_error);
    assert_eq!(result.value["error"]["code"], "failed");
    assert!(
        !result.value["error"]["message"]
            .as_str()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn a_missing_analysis_warns_for_the_drawing_and_fails_a_selection() {
    let response = |analysis: Option<Analysis>| Response {
        document: None,
        analysis,
        output: None,
        engine_version: String::new(),
        warnings: vec!["engine warning".into()],
    };
    let versions = Versions::current("9.8.7");
    let whole = analyzed(response(None), true, vec![1, 2], versions.clone()).unwrap();
    assert!(!whole.is_error);
    assert_eq!(whole.value["value"]["analysis"], Value::Null);
    assert_eq!(whole.value["value"]["analyzed_atoms"], json!(["1", "2"]));
    assert_eq!(
        whole.value["warnings"],
        json!([{"message": "engine warning"}, {"message": NO_PROPERTIES}])
    );
    let error = analyzed(response(None), false, vec![1], versions.clone()).unwrap_err();
    assert_eq!(error, OpError::new(ErrorKind::Failed, NO_PROPERTIES));
    let analysis = Analysis {
        smiles: "C".into(),
        formula: "CH4".into(),
        mass: 16.04,
        exact_mass: 16.03,
        logp: 0.64,
        tpsa: 0.,
        donors: 0,
        acceptors: 0,
        rings: 0,
        unpaired_electrons: 0,
        inchi: "InChI=1S/CH4/h1H4".into(),
        inchikey: "VNWKTOKETHGBQD-UHFFFAOYSA-N".into(),
    };
    let part = analyzed(response(Some(analysis)), false, vec![1], versions).unwrap();
    assert_eq!(part.value["value"]["analysis"]["formula"], "CH4");
    assert_eq!(
        part.value["warnings"],
        json!([{"message": "engine warning"}])
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unknown_handle_is_an_unknown_document() {
    let host = HeadlessHost::new("9.8.7", Budgets::default());
    let handle = DocHandle::new("doc_0123456789abcdef").unwrap();
    let result = analyze_call(&host, &handle, Value::Null).await;
    assert!(result.is_error);
    assert_eq!(result.value["error"]["code"], "unknown_document");
}
