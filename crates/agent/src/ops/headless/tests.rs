use super::*;
use crate::{
    canvas_tools,
    envelope::OPERATION_API_VERSION,
    ops::{exec::Hooks, policy::UNKNOWN_DOCUMENT, wire::versions_json},
    tool_spec::{Hints, valid_name},
};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    sync::atomic::{AtomicI64, Ordering},
    time::{Duration, Instant},
};

fn host() -> HeadlessHost {
    HeadlessHost::new("9.8.7", Budgets::default())
}

/// A call with a fresh request ID.
fn call_as(who: &Principal, tool: &str, arguments: Value) -> Call {
    static NEXT: AtomicI64 = AtomicI64::new(1);
    Call {
        principal: who.clone(),
        request: RequestId::Int(NEXT.fetch_add(1, Ordering::Relaxed)),
        tool: tool.into(),
        arguments,
        progress: None,
    }
}

fn call(tool: &str, arguments: Value) -> Call {
    call_as(&Principal::local(), tool, arguments)
}

fn error_code(result: &ToolResult) -> &Value {
    assert!(result.is_error, "{:?}", result.value);
    &result.value["error"]["code"]
}

#[test]
fn the_catalog_lists_each_tool_once_with_explicit_hints() {
    let names: Vec<_> = host().catalog().iter().map(|spec| spec.name).collect();
    assert_eq!(
        names,
        [
            "info",
            "document_new",
            "document_list",
            "document_close",
            "import",
            "inspect",
            "analyze",
            "render",
            "export",
            "compose",
            "apply"
        ]
    );
    let unique: HashSet<_> = names.iter().collect();
    assert_eq!(unique.len(), names.len());
    let hints = |read_only, destructive, idempotent| Hints {
        read_only,
        destructive,
        idempotent,
        open_world: false,
    };
    let expected = [
        hints(true, false, true),
        hints(false, false, false),
        hints(true, false, true),
        hints(false, true, true),
        hints(false, false, false),
        hints(true, false, true),
        hints(true, false, true),
        hints(true, false, true),
        hints(true, false, true),
        hints(false, false, false),
        hints(false, true, false),
    ];
    for (spec, hints) in catalog::SPECS.iter().zip(expected) {
        assert!(valid_name(spec.name), "{}", spec.name);
        assert!(
            canvas_tools::SPECS
                .iter()
                .all(|canvas| canvas.name != spec.name)
        );
        assert_eq!(spec.hints, Some(hints), "{}", spec.name);
        let schema = (spec.input_schema)();
        assert_eq!(schema["type"], "object", "{}", spec.name);
        assert_eq!(schema["additionalProperties"], false, "{}", spec.name);
        assert!(
            catalog::decode(spec.name, json!({}), &Budgets::default()).is_some(),
            "{}",
            spec.name
        );
    }
    assert!(catalog::decode("canvas_preview", json!({}), &Budgets::default()).is_none());
}

#[test]
fn document_new_states_the_default_lifetime_and_cap() {
    let budgets = Budgets::default();
    let description = documents::NEW.description;
    assert!(description.contains(&format!(
        "{} minutes by default",
        budgets.idle_ttl.as_secs() / 60
    )));
    assert!(description.contains(&format!("{} by default", budgets.max_documents)));
    assert!(description.contains("document_list") && description.contains("document_close"));
}

#[test]
fn decode_check_reports_the_decoder_message() {
    assert_eq!(catalog::decode_check("info", json!({})), Ok(()));
    assert_eq!(
        catalog::decode_check("document_close", json!({})),
        Err("missing field `document`".into())
    );
    assert_eq!(
        catalog::decode_check("document_close", json!(["doc_1"])),
        Err("Tool arguments must be a JSON object".into())
    );
    assert_eq!(
        catalog::decode_check("canvas_preview", json!({})),
        Err("Unknown tool canvas_preview".into())
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unknown_tool_is_a_protocol_error() {
    let error = host()
        .call(call("canvas_preview", json!({})))
        .await
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::UnknownTool);
    assert!(error.kind.is_protocol_error());
    assert_eq!(error.message, "Unknown tool canvas_preview");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bad_arguments_are_tool_errors_decoded_before_the_executor() {
    let host = host();
    // The executor now refuses every call as busy.
    host.drained().await;
    for (tool, arguments) in [
        ("info", json!({"verbose": true})),
        ("info", json!(null)),
        ("document_new", json!([])),
        ("document_list", json!({"document": null})),
        ("document_close", json!({})),
        ("document_close", json!({"document": "no spaces"})),
        (
            "document_close",
            json!({"document": "doc_1", "force": null}),
        ),
    ] {
        let result = host.call(call(tool, arguments.clone())).await.unwrap();
        assert_eq!(
            error_code(&result),
            "invalid_arguments",
            "{tool} {arguments}"
        );
        assert_eq!(
            result.value["versions"],
            versions_json(&Versions::current("9.8.7"))
        );
    }
    let busy = host.call(call("info", json!({}))).await.unwrap();
    assert_eq!(error_code(&busy), "busy");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn info_reports_versions_budgets_and_formats() {
    let info = host().call(call("info", json!({}))).await.unwrap();
    assert!(!info.is_error);
    assert_eq!(info.value["operation_api"], OPERATION_API_VERSION);
    assert_eq!(
        info.value["versions"],
        versions_json(&Versions::current("9.8.7"))
    );
    let budgets = &info.value["budgets"];
    assert_eq!(budgets["max_documents"], 16);
    assert_eq!(budgets["max_session_weight"], 2_000_000);
    assert_eq!(budgets["history_depth"], 20);
    assert_eq!(budgets["op_deadline_seconds"], 120);
    assert_eq!(budgets["idle_ttl_seconds"], 3600);
    assert_eq!(budgets["render"]["max_side"], 8192);
    assert_eq!(budgets.as_object().unwrap().len(), 19);
    assert_eq!(
        info.value["formats"],
        json!({
            "import": ["auto", "smiles", "mol", "rxn", "rsmi", "inchi", "cdxml", "cdx", "reshiki"],
            "export": ["svg", "pdf", "png", "cdxml", "mol", "smiles", "inchi"],
        })
    );
    assert!(info.images.is_empty() && info.files.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn documents_are_created_listed_and_closed_by_their_owner() {
    let host = host();
    let mut handles = Vec::new();
    for _ in 0..2 {
        let created = host.call(call("document_new", json!({}))).await.unwrap();
        assert!(!created.is_error);
        assert_eq!(created.value["revision"], "0");
        handles.push(created.value["document"].as_str().unwrap().to_owned());
    }
    let list = host.call(call("document_list", json!({}))).await.unwrap();
    assert_eq!(
        list.value["documents"],
        json!([
            {"document": handles[0], "revision": "0", "objects": 0},
            {"document": handles[1], "revision": "0", "objects": 0},
        ])
    );
    let other = Principal::new("other");
    let theirs = host
        .call(call_as(&other, "document_list", json!({})))
        .await
        .unwrap();
    assert_eq!(theirs.value["documents"], json!([]));
    let foreign = host
        .call(call_as(
            &other,
            "document_close",
            json!({"document": handles[0]}),
        ))
        .await
        .unwrap();
    assert_eq!(error_code(&foreign), "unknown_document");
    assert_eq!(foreign.value["error"]["message"], UNKNOWN_DOCUMENT);

    let closed = host
        .call(call("document_close", json!({"document": handles[0]})))
        .await
        .unwrap();
    assert_eq!(
        Value::Object(closed.value),
        json!({
            "document": handles[0],
            "closed": true,
            "versions": versions_json(&Versions::current("9.8.7")),
        })
    );
    let again = host
        .call(call("document_close", json!({"document": handles[0]})))
        .await
        .unwrap();
    assert_eq!(error_code(&again), "unknown_document");
    let live = host.store().list(&Principal::local());
    assert_eq!(live.len(), 1);
    assert_eq!(live[0].handle.as_str(), handles[1]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancelled_call_returns_err_and_never_runs() {
    let host = host();
    let exec = host.exec().clone();
    let request = call("document_new", json!({}));
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
    assert!(host.store().list(&Principal::local()).is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_tool_times_out_once_its_deadline_has_passed() {
    let host = HeadlessHost::new(
        "9.8.7",
        Budgets {
            op_deadline: Duration::from_nanos(1),
            ..Budgets::default()
        },
    );
    // Each call wins its permit only after its deadline has passed.
    host.exec().set_hooks(Hooks {
        acquired: Some(Arc::new(|| {
            let until = Instant::now() + Duration::from_nanos(1);
            while Instant::now() < until {
                std::hint::spin_loop();
            }
        })),
        ..Hooks::default()
    });
    for tool in ["info", "document_new", "document_list"] {
        let result = host.call(call(tool, json!({}))).await.unwrap();
        assert_eq!(error_code(&result), "timeout", "{tool}");
    }
    assert!(host.store().list(&Principal::local()).is_empty());
}
