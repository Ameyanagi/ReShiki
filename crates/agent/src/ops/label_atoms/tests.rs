use super::*;
use crate::{
    document::Document,
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

/// A host holding ethyl acetate as a session document, and its handle.
fn hosting() -> (HeadlessHost, DocHandle) {
    let host = HeadlessHost::new("9.8.7", Budgets::default());
    let created = host
        .store()
        .create(&Principal::local(), ethyl_acetate())
        .unwrap();
    (host, created.handle)
}

async fn call(host: &HeadlessHost, tool: &str, arguments: Value) -> ToolResult {
    host.call(Call {
        principal: Principal::local(),
        request: RequestId::Int(1),
        tool: tool.into(),
        arguments,
        progress: None,
    })
    .await
    .unwrap()
}

/// The ¹³C rows as label_atoms takes them, and the revision predicted.
async fn predicted(host: &HeadlessHost, handle: &DocHandle) -> (Value, Value) {
    let arguments = json!({"document": handle.as_str(), "nucleus": "13C", "ids": null});
    let result = call(host, "predict_nmr", arguments).await;
    let value = &result.value["value"];
    let rows = value["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| json!({"atom": row["atom"], "ppm": row["ppm"]}))
        .collect();
    (rows, value["revision"].clone())
}

fn label(handle: &DocHandle, rows: &Value, base: &Value, key: Value) -> Value {
    json!({
        "document": handle.as_str(), "rows": rows, "format": "both",
        "base_revision": base, "idempotency_key": key,
    })
}

fn stored(host: &HeadlessHost, handle: &DocHandle) -> (Document, Revision) {
    let snapshot = host
        .store()
        .snapshot(
            &Principal::local(),
            handle,
            crate::ops::policy::Access::Read,
        )
        .unwrap();
    ((*snapshot.doc).clone(), snapshot.revision)
}

/// The error code and param of a failed call.
fn error(result: &ToolResult) -> (Value, Value) {
    assert!(result.is_error, "{:?}", result.value);
    let error = &result.value["error"];
    (error["code"].clone(), error["param"].clone())
}

#[test]
fn decode_checks_the_format_the_row_count_and_the_key() {
    let budgets = Budgets::default();
    let decoded = |rows: Value, format: &str, key: Value| {
        let args = json!({
            "document": "doc_1", "rows": rows, "format": format,
            "base_revision": "0", "idempotency_key": key,
        });
        decode(args, &budgets).map(drop)
    };
    let row = json!([{"atom": "1", "ppm": 14.2}, {"atom": "3", "ppm": null}]);
    for format in ["numbers", "ppm", "both"] {
        assert_eq!(decoded(row.clone(), format, Value::Null), Ok(()));
    }
    let format = decoded(row.clone(), "Both", Value::Null).unwrap_err();
    assert_eq!(
        (format.kind, format.param),
        (ErrorKind::InvalidArguments, Some("format"))
    );
    let many = Value::Array(vec![json!({"atom": "1", "ppm": 1.0}); budgets.max_ids + 1]);
    let over = decoded(many, "both", Value::Null).unwrap_err();
    assert_eq!((over.kind, over.param), (ErrorKind::Budget, Some("rows")));
    let long = json!("k".repeat(budgets.max_idempotency_key_bytes + 1));
    assert_eq!(
        decoded(row.clone(), "both", long).unwrap_err().kind,
        ErrorKind::InvalidArguments
    );
    let unbound =
        json!({"document": "doc_1", "rows": row, "format": "both", "idempotency_key": null});
    let error = decode(unbound, &budgets).unwrap_err();
    assert_eq!(error.message, "missing field `base_revision`");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn labels_land_at_the_predicted_revision_as_one_undo_step() {
    let (host, handle) = hosting();
    let (rows, base) = predicted(&host, &handle).await;
    assert_eq!(base, "0");
    let result = call(
        &host,
        "label_atoms",
        label(&handle, &rows, &base, Value::Null),
    )
    .await;
    assert!(!result.is_error, "{:?}", result.value);
    assert_eq!(
        result.value["value"],
        json!({
            "effect": "applied", "document": handle.as_str(), "revision": "1",
            "recorded": true, "labeled": ["1", "2", "4", "5"],
        })
    );
    let (labelled, revision) = stored(&host, &handle);
    assert_eq!(revision, Revision(1));
    let number = |id| {
        labelled
            .atom(id)
            .unwrap()
            .display
            .number
            .clone()
            .unwrap()
            .text
    };
    assert_eq!(number(4), "#4 · 170.700 ppm");
    assert_eq!(labelled.atom(3).unwrap().display.number, None);
    // The same rows again change nothing.
    let again = call(
        &host,
        "label_atoms",
        label(&handle, &rows, &json!("1"), Value::Null),
    )
    .await;
    assert_eq!(again.value["value"]["recorded"], false);
    assert_eq!(again.value["value"]["revision"], "1");
    let undo = json!({
        "document": handle.as_str(), "edit": "undo", "source": null, "ids": null,
        "base_revision": "1", "idempotency_key": null,
    });
    let undone = call(&host, "apply", undo).await;
    assert!(!undone.is_error, "{:?}", undone.value);
    let (restored, _) = stored(&host, &handle);
    assert!(
        restored
            .atoms
            .iter()
            .all(|atom| atom.display.number.is_none())
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rows_from_an_older_revision_are_stale_and_change_nothing() {
    let (host, handle) = hosting();
    let (rows, base) = predicted(&host, &handle).await;
    let delete = json!({
        "document": handle.as_str(), "edit": "delete", "source": null, "ids": ["6"],
        "base_revision": base, "idempotency_key": null,
    });
    assert!(!call(&host, "apply", delete).await.is_error);
    let (before, revision) = stored(&host, &handle);
    let result = call(
        &host,
        "label_atoms",
        label(&handle, &rows, &base, Value::Null),
    )
    .await;
    assert_eq!(error(&result), (json!("stale"), json!("base_revision")));
    assert_eq!(stored(&host, &handle), (before, revision));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_bad_row_anywhere_changes_nothing() {
    let (host, handle) = hosting();
    let (rows, base) = predicted(&host, &handle).await;
    let before = stored(&host, &handle);
    for (bad, expected) in [
        (
            json!({"atom": "99", "ppm": null}),
            (json!("unknown_object"), json!("rows")),
        ),
        // "#2 · 100000000000000000000.000 ppm" is 34 characters.
        (
            json!({"atom": "2", "ppm": 1e20}),
            (json!("invalid_arguments"), json!("rows")),
        ),
    ] {
        let mut rows = rows.clone();
        rows.as_array_mut().unwrap().push(bad);
        let result = call(
            &host,
            "label_atoms",
            label(&handle, &rows, &base, Value::Null),
        )
        .await;
        assert_eq!(error(&result), expected);
        assert_eq!(stored(&host, &handle), before);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn one_key_labels_once() {
    let (host, handle) = hosting();
    let (rows, base) = predicted(&host, &handle).await;
    let keyed = label(&handle, &rows, &base, json!("k"));
    let first = call(&host, "label_atoms", keyed.clone()).await;
    let second = call(&host, "label_atoms", keyed).await;
    assert_eq!(first.value, second.value);
    assert_eq!(stored(&host, &handle).1, Revision(1));
    // Without the key, the same request is stale now.
    let unkeyed = call(
        &host,
        "label_atoms",
        label(&handle, &rows, &base, Value::Null),
    )
    .await;
    assert_eq!(error(&unkeyed).0, "stale");
    let mut other = label(&handle, &rows, &base, json!("k"));
    other["format"] = json!("ppm");
    assert_eq!(
        error(&call(&host, "label_atoms", other).await).0,
        "invalid_arguments"
    );
}
