use super::*;
use crate::{
    document::{Document, Point},
    ops::{
        headless::HeadlessHost,
        host::{Call, ToolHost},
        wire::RequestId,
    },
};

fn ethyl_acetate() -> Document {
    Document::from_json(include_bytes!(
        "../../../../../tests/fixtures/nmr/ethyl-acetate.rsk"
    ))
    .unwrap()
}

/// A host holding `doc` as a session document, and its handle.
fn hosting(doc: Document) -> (HeadlessHost, DocHandle) {
    let host = HeadlessHost::new("9.8.7", Budgets::default());
    let created = host.store().create(&Principal::local(), doc).unwrap();
    (host, created.handle)
}

async fn predict(host: &HeadlessHost, handle: &DocHandle, nucleus: &str, ids: Value) -> ToolResult {
    host.call(Call {
        principal: Principal::local(),
        request: RequestId::Int(1),
        tool: "predict_nmr".into(),
        arguments: json!({"document": handle.as_str(), "nucleus": nucleus, "ids": ids}),
        progress: None,
    })
    .await
    .unwrap()
}

/// The error code and message of a failed call.
fn error(result: &ToolResult) -> (Value, Value) {
    assert!(result.is_error, "{:?}", result.value);
    let error = &result.value["error"];
    (error["code"].clone(), error["message"].clone())
}

#[test]
fn the_nucleus_is_1h_or_13c_and_ids_name_at_least_one_atom() {
    let budgets = Budgets::default();
    let decoded = |args: Value| decode(args, &budgets).map(drop);
    for nucleus in ["1H", "13C"] {
        assert_eq!(
            decoded(json!({"document": "doc_1", "nucleus": nucleus, "ids": null})),
            Ok(())
        );
    }
    for nucleus in ["H1", "C13", "1h", "15N", ""] {
        let error =
            decoded(json!({"document": "doc_1", "nucleus": nucleus, "ids": null})).unwrap_err();
        assert_eq!(error.kind, ErrorKind::InvalidArguments, "{nucleus}");
        assert_eq!(error.param, Some("nucleus"));
    }
    let empty = decoded(json!({"document": "doc_1", "nucleus": "1H", "ids": []})).unwrap_err();
    assert_eq!(empty.kind, ErrorKind::InvalidArguments);
    assert_eq!(empty.param, Some("ids"));
    let many = vec![json!("1"); budgets.max_ids + 1];
    let over = decoded(json!({"document": "doc_1", "nucleus": "1H", "ids": many})).unwrap_err();
    assert_eq!(over.kind, ErrorKind::Budget);
    // An omitted nullable argument reads as null.
    assert_eq!(
        decoded(json!({"document": "doc_1", "nucleus": "1H"})),
        Ok(())
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_value_is_the_scripting_prediction_at_the_document_revision() {
    let doc = ethyl_acetate();
    let (host, handle) = hosting(doc.clone());
    for nucleus in [Nucleus::H1, Nucleus::C13] {
        let result = predict(&host, &handle, nucleus.code(), Value::Null).await;
        assert!(!result.is_error, "{:?}", result.value);
        let mut expected =
            serde_json::to_value(nmr::predict_drawing(&doc, None, nucleus).unwrap()).unwrap();
        expected["revision"] = json!("0");
        assert_eq!(result.value["value"], expected);
        assert_eq!(result.value["warnings"], json!([]));
    }
    let after = host
        .store()
        .snapshot(&Principal::local(), &handle, Access::Read)
        .unwrap();
    assert_eq!(*after.doc, doc);
    assert_eq!(after.revision, Revision(0));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ids_select_their_molecule_and_unknown_ids_are_reported() {
    let mut doc = ethyl_acetate();
    let methane = doc.add_atom("C", Point::new(500., 0.));
    let (host, handle) = hosting(doc);
    let result = predict(&host, &handle, "13C", json!([methane.to_string()])).await;
    assert!(!result.is_error, "{:?}", result.value);
    assert_eq!(
        result.value["value"]["rows"][0]["atom"],
        methane.to_string()
    );
    assert_eq!(result.value["value"]["rows"].as_array().unwrap().len(), 1);

    let (code, message) = error(&predict(&host, &handle, "13C", Value::Null).await);
    assert_eq!(code, "failed");
    assert_eq!(
        message,
        "NMR prediction needs exactly one molecule; this drawing has 2"
    );
    let both = json!(["1", methane.to_string()]);
    let (code, message) = error(&predict(&host, &handle, "13C", both).await);
    assert_eq!(code, "failed");
    assert_eq!(
        message,
        "NMR prediction needs exactly one molecule; the given IDs cover 2"
    );
    let unknown = predict(&host, &handle, "13C", json!(["999"])).await;
    assert_eq!(error(&unknown).0, "unknown_object");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_molecule_without_sites_for_the_nucleus_fails() {
    let mut doc = Document::default();
    doc.add_atom("O", Point::default());
    let (host, handle) = hosting(doc);
    let (code, message) = error(&predict(&host, &handle, "13C", Value::Null).await);
    assert_eq!(code, "failed");
    assert_eq!(message, "This molecule has no supported ¹³C sites");
}
