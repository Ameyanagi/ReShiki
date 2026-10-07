//! End-to-end scenarios for HeadlessHost, driven through `ToolHost::call`
//! only, as a transport drives it. This file does not declare `mod support;`:
//! it needs none of those helpers, and unused ones would fail -D warnings.
use reshiki_agent::ops::{
    budget::Budgets,
    error::ErrorKind,
    headless::HeadlessHost,
    host::{Call, ToolHost},
    result::ToolResult,
    wire::{Principal, RequestId},
};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    sync::atomic::{AtomicI64, Ordering},
};

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

fn host() -> HeadlessHost {
    HeadlessHost::new("9.8.7", Budgets::default())
}

/// A call from the stdio principal with a fresh request ID.
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

/// What every result carries, success or tool error: a JSON object (MCP
/// `structuredContent` must be one in 2025-11-25) with this build's versions
/// (crates/model/src/document.rs:151 for the document version).
fn assert_envelope(tool: &str, result: &ToolResult) {
    let value = Value::Object(result.value.clone());
    assert!(value.is_object(), "{tool}");
    assert_eq!(
        value["versions"],
        json!({"app": "9.8.7", "operation_api": 1, "engine_protocol": 1, "document": 19}),
        "{tool}"
    );
}

/// The result of a call that must succeed.
async fn ok(host: &HeadlessHost, tool: &str, arguments: Value) -> ToolResult {
    let result = host
        .call(call(tool, arguments))
        .await
        .unwrap_or_else(|error| panic!("{tool}: {error}"));
    assert!(!result.is_error, "{tool}: {:?}", result.value);
    assert_envelope(tool, &result);
    result
}

/// The stable code of a call that must end in a tool execution error.
async fn error_code(host: &HeadlessHost, tool: &str, arguments: Value) -> String {
    let result = host
        .call(call(tool, arguments.clone()))
        .await
        .unwrap_or_else(|error| panic!("{tool} {arguments}: {error}"));
    assert!(result.is_error, "{tool} {arguments}: {:?}", result.value);
    assert_envelope(tool, &result);
    assert!(result.images.is_empty() && result.files.is_empty());
    let error = &result.value["error"];
    assert!(error["message"].is_string(), "{tool} {arguments}");
    error["code"].as_str().unwrap().to_owned()
}

fn handle(result: &ToolResult) -> String {
    result.value["value"]["document"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// An object ID, which must be a decimal string.
fn id(value: &Value) -> String {
    let id = value
        .as_str()
        .unwrap_or_else(|| panic!("{value} is not a string"));
    assert!(id.parse::<u64>().is_ok(), "{id}");
    id.to_owned()
}

/// The `field` of each item of the array `items`, as object IDs.
fn ids_of(items: &Value, field: &str) -> Vec<String> {
    let items = items.as_array().unwrap();
    items.iter().map(|item| id(&item[field])).collect()
}

fn molecule(smiles: &str, label: &str) -> Value {
    json!({"smiles": smiles, "label": label, "coefficient": 1, "rotation": 0, "compact": false})
}

/// An esterification scheme, as the assistant would propose it.
fn reaction() -> Value {
    json!({
        "explanation": "Esterification",
        "replace_ids": [],
        "molecules": [],
        "reactions": [{
            "reactants": [molecule("CC(=O)O", "Acetic acid"), molecule("CCO", "Ethanol")],
            "products": [molecule("CCOC(C)=O", "Ethyl acetate"), molecule("O", "Water")],
            "conditions": "H2SO4",
            "arrow": "forward",
            "title": "",
            "role": "main",
            "direction": null,
        }],
        "composition": {"arrangement": "rows", "columns": 2, "width_pt": 540, "preserve_details": false},
        "sketch": null,
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_session_goes_from_import_through_compose_and_apply_to_close() {
    let host = host();
    let info = ok(&host, "info", json!({})).await;
    assert_eq!(info.value["operation_api"], 1);

    let imported = ok(&host, "import", json!({"format": "smiles", "text": "CCO"})).await;
    assert_eq!(imported.value["validation"], json!({"status": "valid"}));
    assert_eq!(imported.value["value"]["revision"], "0");
    let atoms = imported.value["value"]["counts"]["atoms"].clone();
    let document = handle(&imported);

    let composed = ok(
        &host,
        "compose",
        json!({"proposal": reaction(), "style_document": null}),
    )
    .await;
    assert_eq!(composed.value["value"]["arrows"], 1);
    let source = handle(&composed);

    let applied = ok(
        &host,
        "apply",
        json!({
            "document": document,
            "edit": "insert",
            "source": source,
            "ids": null,
            "base_revision": "0",
            "idempotency_key": null,
        }),
    )
    .await;
    let value = &applied.value["value"];
    assert_eq!(value["effect"], "applied");
    assert_eq!(value["revision"], "1");
    assert_eq!(value["recorded"], true);
    assert_eq!(value["deleted"], json!([]));
    let inserted: Vec<String> = value["inserted"]
        .as_array()
        .unwrap()
        .iter()
        .map(id)
        .collect();
    assert!(!inserted.is_empty());
    assert_eq!(ids_of(&value["id_remap"], "inserted"), inserted);
    assert_eq!(ids_of(&value["id_remap"], "source").len(), inserted.len());

    let inspected = ok(&host, "inspect", json!({"document": document, "ids": null})).await;
    assert_eq!(inspected.value["revision"], "1");
    let mut listed = HashSet::new();
    for kind in ["atoms", "annotations", "arrows", "graphics"] {
        listed.extend(ids_of(&inspected.value[kind], "id"));
    }
    for id in &inserted {
        assert!(listed.contains(id), "inserted {id} is not listed");
    }
    assert_eq!(inspected.value["truncated"], false);

    let rendered = ok(
        &host,
        "render",
        json!({"document": document, "format": "png", "max_width": 400, "max_height": 300, "ids": null}),
    )
    .await;
    let [image] = rendered.images.as_slice() else {
        panic!("one image: {:?}", rendered.value)
    };
    assert_eq!(image.mime, "image/png");
    assert!(image.bytes.starts_with(PNG_SIGNATURE));
    assert!(rendered.files.is_empty());
    assert_eq!(rendered.value["value"]["byte_len"], image.bytes.len());
    assert!(rendered.value["value"]["width"].as_u64().unwrap() <= 400);

    for (format, mime, marker) in [
        ("svg", "image/svg+xml", "<svg"),
        ("mol", "chemical/x-mdl-molfile", "M  END"),
    ] {
        let exported = ok(
            &host,
            "export",
            json!({"document": document, "format": format, "pages": null}),
        )
        .await;
        let [file] = exported.files.as_slice() else {
            panic!("one {format} file: {:?}", exported.value)
        };
        assert_eq!(file.mime, mime);
        assert_eq!(file.name, format!("drawing.{format}"));
        assert!(
            String::from_utf8_lossy(&file.bytes).contains(marker),
            "{format}"
        );
        let receipt = &exported.value["value"]["receipt"];
        assert_eq!(receipt["format"], format);
        assert_eq!(receipt["byte_len"], file.bytes.len());
    }

    let undone = ok(
        &host,
        "apply",
        json!({
            "document": document,
            "edit": "undo",
            "source": null,
            "ids": null,
            "base_revision": "1",
            "idempotency_key": null,
        }),
    )
    .await;
    assert_eq!(undone.value["value"]["revision"], "2");
    assert_eq!(undone.value["value"]["recorded"], true);
    let restored = ok(&host, "inspect", json!({"document": document, "ids": null})).await;
    assert_eq!(restored.value["counts"]["atoms"], atoms);
    assert_eq!(restored.value["counts"]["arrows"], 0);

    for open in [&document, &source] {
        let closed = ok(&host, "document_close", json!({"document": open})).await;
        assert_eq!(closed.value["closed"], true);
    }
    let listed = ok(&host, "document_list", json!({})).await;
    assert_eq!(listed.value["documents"], json!([]));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn errors_are_a_protocol_error_or_coded_tool_errors() {
    let host = host();
    let error = host
        .call(call("canvas_preview", json!({})))
        .await
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::UnknownTool);
    assert_eq!(error.kind.code(), "unknown_tool");

    let created = ok(&host, "document_new", json!({})).await;
    let document = created.value["document"].as_str().unwrap().to_owned();
    for (tool, arguments) in [
        ("info", json!([])),
        ("inspect", json!({"document": document})),
        ("inspect", json!({"document": document, "ids": [1]})),
        (
            "render",
            json!({"document": document, "format": "gif", "max_width": null, "max_height": null, "ids": null}),
        ),
        (
            "apply",
            json!({"document": document, "edit": "undo", "source": null, "ids": null, "base_revision": null, "idempotency_key": null}),
        ),
    ] {
        assert_eq!(
            error_code(&host, tool, arguments.clone()).await,
            "invalid_arguments",
            "{tool} {arguments}"
        );
    }

    let unknown = json!({"document": "doc_0", "ids": null});
    assert_eq!(
        error_code(&host, "inspect", unknown).await,
        "unknown_document"
    );

    let stale = json!({
        "document": document,
        "edit": "undo",
        "source": null,
        "ids": null,
        "base_revision": "1",
        "idempotency_key": null,
    });
    assert_eq!(error_code(&host, "apply", stale).await, "stale");

    let too_many: Vec<String> = (1..=5_001).map(|id| id.to_string()).collect();
    for (tool, arguments) in [
        (
            "render",
            json!({"document": document, "format": "png", "max_width": 8192, "max_height": 8192, "ids": null}),
        ),
        ("inspect", json!({"document": document, "ids": too_many})),
    ] {
        assert_eq!(error_code(&host, tool, arguments).await, "budget", "{tool}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_17_mib_import_is_refused_as_over_budget() {
    let host = host();
    let text = "C".repeat(17 * 1024 * 1024);
    let result = host
        .call(call("import", json!({"format": "smiles", "text": text})))
        .await
        .unwrap();
    assert!(result.is_error, "{:?}", result.value);
    assert_envelope("import", &result);
    assert_eq!(result.value["error"]["code"], "budget");
    let message = result.value["error"]["message"].as_str().unwrap();
    assert!(message.contains("max_text_bytes"), "{message}");
    let listed = ok(&host, "document_list", json!({})).await;
    assert_eq!(listed.value["documents"], json!([]));
}
