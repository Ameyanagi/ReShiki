use super::*;
use crate::ops::{
    error::ErrorKind,
    headless::HeadlessHost,
    host::{Call, ToolHost},
    naming::{Named, Naming, NamingFuture, Resolved},
    wire::{Principal, RequestId},
};
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicI64, Ordering},
};

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

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

/// Convert arguments for SMILES `text` with every optional argument null.
fn arguments(text: &str, to: &str) -> Value {
    json!({
        "input": {"format": "smiles", "text": text},
        "to": to,
        "pages": null,
        "svg_text": null,
        "max_width": null,
        "max_height": null,
    })
}

async fn ok(host: &HeadlessHost, tool: &str, arguments: Value) -> ToolResult {
    let result = host.call(call(tool, arguments)).await.unwrap();
    assert!(!result.is_error, "{tool}: {:?}", result.value);
    result
}

async fn open_documents(host: &HeadlessHost) -> usize {
    let listed = ok(host, "document_list", json!({})).await;
    listed.value["value"]["documents"].as_array().unwrap().len()
}

/// The call the audit counted: a picture of a SMILES took import, render and
/// document_close; convert takes one call and leaves no document behind.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_preview_takes_one_call_and_keeps_no_document() {
    let host = host();
    let mut old = arguments("CCO", "preview");
    let imported = ok(&host, "import", old["input"].take()).await;
    let document = imported.value["value"]["document"].clone();
    let rendered = ok(
        &host,
        "render",
        json!({
            "document": document, "format": "png", "max_width": null,
            "max_height": null, "ids": null, "svg_text": null,
        }),
    )
    .await;
    ok(&host, "document_close", json!({"document": document})).await;

    let converted = ok(&host, "convert", arguments("CCO", "preview")).await;
    assert_eq!(open_documents(&host).await, 0);
    let [image] = converted.images.as_slice() else {
        panic!("one image: {:?}", converted.value);
    };
    assert!(image.bytes.starts_with(PNG_SIGNATURE));
    // The same pixels as the three calls made.
    assert_eq!(image.bytes, rendered.images[0].bytes);
    let value = &converted.value["value"];
    assert_eq!(value["receipt"]["format"], "png");
    assert_eq!(value["receipt"]["byte_len"], image.bytes.len());
    assert_eq!(value["width"], rendered.value["value"]["width"]);
    assert_eq!(value["height"], rendered.value["value"]["height"]);
    assert_eq!(value["analysis"], imported.value["value"]["analysis"]);
    assert_eq!(value["analysis"]["formula"], "C2H6O");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_file_is_what_import_then_export_return() {
    let host = host();
    let imported = ok(&host, "import", json!({"format": "smiles", "text": "CCO"})).await;
    let document = imported.value["value"]["document"].clone();
    for format in ["mol", "smiles", "inchi", "cdxml", "cdx", "svg"] {
        let exported = ok(
            &host,
            "export",
            json!({"document": document, "format": format, "pages": null, "svg_text": null}),
        )
        .await;
        let converted = ok(&host, "convert", arguments("CCO", format)).await;
        assert!(converted.images.is_empty(), "{format}");
        assert_eq!(converted.files, exported.files, "{format}");
        let value = &converted.value["value"];
        assert_eq!(
            value["receipt"], exported.value["value"]["receipt"],
            "{format}"
        );
        assert_eq!(value["width"], Value::Null, "{format}");
        assert_eq!(value["height"], Value::Null, "{format}");
    }
    assert_eq!(open_documents(&host).await, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn svg_text_is_outlined_unless_live() {
    let host = host();
    for (mode, live) in [
        (Value::Null, false),
        (json!("outline"), false),
        (json!("live"), true),
    ] {
        let mut arguments = arguments("CCO", "svg");
        arguments["svg_text"] = mode.clone();
        let converted = ok(&host, "convert", arguments).await;
        let svg = String::from_utf8(converted.files[0].bytes.clone()).unwrap();
        assert_eq!(svg.contains("<text"), live, "{mode}");
    }
}

#[test]
fn the_size_belongs_to_a_preview_and_pages_to_pdf() {
    let budgets = Budgets::default();
    let error = |patch: Value| {
        let mut arguments = arguments("CCO", "preview");
        for (key, value) in patch.as_object().unwrap() {
            arguments[key] = value.clone();
        }
        decode(arguments, &budgets).unwrap_err()
    };
    let size = error(json!({"to": "svg", "max_width": 400}));
    assert_eq!(size.kind, ErrorKind::InvalidArguments);
    assert_eq!(size.param, Some("max_width"));
    assert_eq!(size.message, "max_width is valid only with to preview");
    let pages = error(json!({"to": "svg", "pages": true}));
    assert_eq!(pages.param, Some("pages"));
    assert_eq!(error(json!({"pages": true})).param, Some("pages"));
    let to = error(json!({"to": "jpeg"}));
    assert_eq!(to.param, Some("to"));
    assert!(to.message.starts_with("to must be one of preview, svg, "));
    let side = error(json!({"max_width": 0}));
    assert_eq!(side.param, Some("max_width"));
    // The import text budget applies before any work.
    let text = "C".repeat(budgets.max_text_bytes + 1);
    assert_eq!(
        decode(arguments(&text, "preview"), &budgets)
            .unwrap_err()
            .kind,
        ErrorKind::Budget
    );
    assert!(decode(arguments("CCO", "preview"), &budgets).is_ok());
    assert!(decode(error_free_pdf(), &budgets).is_ok());
}

fn error_free_pdf() -> Value {
    let mut arguments = arguments("CCO", "pdf");
    arguments["pages"] = json!(true);
    arguments
}

/// A naming hook that knows one name.
struct Ethanol;

impl Naming for Ethanol {
    fn resolve(&self, name: String) -> NamingFuture<Resolved> {
        Box::pin(async move {
            match name.as_str() {
                "ethanol" => Ok(Resolved {
                    smiles: "CCO".into(),
                    prepared: None,
                    identity: "CCO".into(),
                    warnings: vec!["A parser warning.".into()],
                }),
                // The hook's check rejects the drawing of this one.
                "methanol" => Ok(Resolved {
                    smiles: "CO".into(),
                    prepared: None,
                    identity: "another structure".into(),
                    warnings: Vec::new(),
                }),
                _ => Err(format!("Could not interpret chemical name {name:?}")),
            }
        })
    }

    fn verify(&self, identity: &str, doc: &crate::document::Document) -> Result<(), String> {
        if identity == "CCO" && doc.atoms.len() == 3 {
            Ok(())
        } else {
            Err("The drawing is not the named structure".into())
        }
    }

    fn generate(&self, doc: crate::document::Document) -> NamingFuture<(Named, Vec<String>)> {
        Box::pin(async move {
            let named = Named {
                name: format!("a drawing of {} atoms", doc.atoms.len()),
                status: "preferred".into(),
                preferred: true,
                reason: None,
                detail: "A test name.".into(),
                aliases: vec!["alcohol".into()],
                alternatives: Vec::new(),
            };
            Ok((named, vec!["A naming warning.".into()]))
        })
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn names_need_the_hosts_naming_hook() {
    let name = json!({"format": "name", "text": "ethanol"});
    let mut convert = arguments("", "smiles");
    convert["input"] = name.clone();

    // Without a hook both directions say so with their own code.
    let bare = host();
    for (tool, arguments) in [("import", name.clone()), ("convert", convert.clone())] {
        let result = bare.call(call(tool, arguments)).await.unwrap();
        assert!(result.is_error, "{tool}");
        assert_eq!(result.value["error"]["code"], "unsupported", "{tool}");
    }
    let created = ok(&bare, "document_new", json!({})).await;
    let document = created.value["value"]["document"].clone();
    let result = bare
        .call(call("name", json!({"document": document})))
        .await
        .unwrap();
    assert_eq!(result.value["error"]["code"], "unsupported");

    let host = host().with_naming(Arc::new(Ethanol));
    // The hook's warnings reach the envelope, in every direction.
    let warning = |text: &str| json!([{"message": text}]);
    let converted = ok(&host, "convert", convert.clone()).await;
    assert_eq!(converted.files[0].bytes, b"CCO");
    assert_eq!(converted.value["warnings"], warning("A parser warning."));
    let imported = ok(&host, "import", name).await;
    assert_eq!(imported.value["warnings"], warning("A parser warning."));
    let value = &imported.value["value"];
    assert_eq!(value["analysis"]["formula"], "C2H6O");
    let named = ok(&host, "name", json!({"document": value["document"]})).await;
    assert_eq!(named.value["warnings"], warning("A naming warning."));
    // A drawing the hook's check rejects is a failed call and no document.
    let before = open_documents(&host).await;
    let methanol = json!({"format": "name", "text": "methanol"});
    convert["input"] = methanol.clone();
    for (tool, arguments) in [("import", methanol), ("convert", convert)] {
        let result = host.call(call(tool, arguments)).await.unwrap();
        assert_eq!(result.value["error"]["code"], "failed", "{tool}");
        assert_eq!(
            result.value["error"]["message"], "The drawing is not the named structure",
            "{tool}"
        );
    }
    assert_eq!(open_documents(&host).await, before);
    assert_eq!(
        named.value["value"],
        json!({
            "name": "a drawing of 3 atoms",
            "status": "preferred",
            "preferred": true,
            "reason": null,
            "detail": "A test name.",
            "aliases": ["alcohol"],
            "alternatives": [],
        })
    );
    // An unknown name is the hook's message, as a failed call.
    let unknown = host
        .call(call(
            "import",
            json!({"format": "name", "text": "notachemical"}),
        ))
        .await
        .unwrap();
    assert_eq!(unknown.value["error"]["code"], "failed");
    assert_eq!(
        unknown.value["error"]["message"],
        "Could not interpret chemical name \"notachemical\""
    );
}

/// `rsk` and `reshiki` both name a native drawing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rsk_is_another_name_for_reshiki() {
    const NATIVE: &str = include_str!("../../../../../tests/fixtures/bond-join-regression.rsk");
    let host = host();
    let mut counts = Vec::new();
    for format in ["reshiki", "rsk"] {
        let imported = ok(&host, "import", json!({"format": format, "text": NATIVE})).await;
        counts.push(imported.value["value"]["counts"].clone());
    }
    assert_eq!(counts[0], counts[1]);
}

/// The sketch schema leaves the tool list and comes from the schema tool.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_schema_tool_returns_the_sketch_schema_compose_leaves_out() {
    let host = host();
    let result = ok(&host, "schema", json!({"name": "sketch"})).await;
    let value = &result.value["value"];
    assert_eq!(value["name"], "sketch");
    assert_eq!(
        value["schema"],
        crate::schema()["properties"]["sketch"]["anyOf"][1]
    );
    assert!(value["schema"]["properties"]["atoms"].is_object());
    let unknown = host
        .call(call("schema", json!({"name": "proposal"})))
        .await
        .unwrap();
    assert_eq!(unknown.value["error"]["code"], "invalid_arguments");
}
