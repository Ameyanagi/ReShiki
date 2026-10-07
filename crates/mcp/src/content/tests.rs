use super::*;
use crate::fake_host::{FAILURES, PDF, PNG, SVG, quote_heavy, versions};
use serde_json::{Map, json};

const UNLIMITED: usize = usize::MAX;

fn result(value: Value) -> ToolResult {
    let Value::Object(value) = value else {
        panic!("values are objects");
    };
    ToolResult {
        value,
        images: Vec::new(),
        files: Vec::new(),
        is_error: false,
    }
}

fn wire(result: &CallToolResult) -> Value {
    serde_json::to_value(result).unwrap()
}

fn decode(text: &Value) -> Vec<u8> {
    STANDARD.decode(text.as_str().unwrap()).unwrap()
}

#[test]
fn a_value_is_structured_content_with_a_text_copy() {
    let value = json!({"smiles": "CCO", "n": 1, "nested": {"list": [1, "\"x\""]}});
    let mapped = wire(&to_mcp(result(value.clone()), &versions(), UNLIMITED));
    assert_eq!(
        mapped,
        json!({
            "resultType": "complete",
            "content": [{"type": "text", "text": value.to_string()}],
            "structuredContent": value,
            "isError": false,
        })
    );
    let copy: Value = serde_json::from_str(mapped["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(copy, mapped["structuredContent"]);
}

#[test]
fn an_empty_value_is_still_an_object() {
    let mapped = wire(&to_mcp(result(json!({})), &versions(), UNLIMITED));
    assert_eq!(mapped["structuredContent"], json!({}));
    assert_eq!(mapped["content"], json!([{"type": "text", "text": "{}"}]));
}

#[test]
fn images_are_base64_image_content() {
    let r = ToolResult {
        images: vec![
            Image {
                mime: "image/png",
                bytes: PNG.to_vec(),
            },
            Image {
                mime: "image/svg+xml",
                bytes: SVG.as_bytes().to_vec(),
            },
        ],
        ..result(json!({"width": 1}))
    };
    let mapped = wire(&to_mcp(r, &versions(), UNLIMITED));
    let content = mapped["content"].as_array().unwrap();
    assert_eq!(content.len(), 3);
    assert_eq!(content[1]["type"], "image");
    assert_eq!(content[1]["mimeType"], "image/png");
    assert_eq!(decode(&content[1]["data"]), PNG);
    assert_eq!(content[2]["mimeType"], "image/svg+xml");
    assert_eq!(decode(&content[2]["data"]), SVG.as_bytes());
}

fn file(mime: &'static str, name: &str, bytes: &[u8]) -> ToolResult {
    ToolResult {
        files: vec![Blob {
            mime,
            name: name.into(),
            bytes: bytes.to_vec(),
        }],
        ..result(json!({}))
    }
}

#[test]
fn binary_files_are_base64_blobs() {
    let mapped = wire(&to_mcp(
        file("application/pdf", "drawing.pdf", PDF),
        &versions(),
        UNLIMITED,
    ));
    let resource = &mapped["content"][1];
    assert_eq!(resource["type"], "resource");
    assert_eq!(resource["resource"]["uri"], "reshiki:result/drawing.pdf");
    assert_eq!(resource["resource"]["mimeType"], "application/pdf");
    assert_eq!(decode(&resource["resource"]["blob"]), PDF);
    assert!(resource["resource"].get("text").is_none());
    // A PNG export is binary too.
    let mapped = wire(&to_mcp(
        file("image/png", "drawing.png", PNG),
        &versions(),
        UNLIMITED,
    ));
    assert_eq!(decode(&mapped["content"][1]["resource"]["blob"]), PNG);
}

#[test]
fn textual_files_are_text_resources() {
    for (mime, name) in [
        ("image/svg+xml", "drawing.svg"),
        ("chemical/x-cdxml", "drawing.cdxml"),
        ("chemical/x-mdl-molfile", "drawing.mol"),
        ("chemical/x-daylight-smiles", "drawing.smi"),
        ("chemical/x-inchi", "drawing.inchi"),
    ] {
        let mapped = wire(&to_mcp(
            file(mime, name, SVG.as_bytes()),
            &versions(),
            UNLIMITED,
        ));
        assert_eq!(
            mapped["content"][1],
            json!({
                "type": "resource",
                "resource": {
                    "uri": format!("reshiki:result/{name}"),
                    "mimeType": mime,
                    "text": SVG,
                },
            }),
            "{mime}"
        );
    }
}

#[test]
fn textual_files_that_are_not_utf8_are_blobs() {
    let bytes = b"<svg>\xff</svg>";
    let mapped = wire(&to_mcp(
        file("image/svg+xml", "drawing.svg", bytes),
        &versions(),
        UNLIMITED,
    ));
    let resource = &mapped["content"][1]["resource"];
    assert_eq!(resource["mimeType"], "image/svg+xml");
    assert_eq!(decode(&resource["blob"]), bytes);
}

#[test]
fn content_keeps_the_text_then_images_then_files_order() {
    let r = ToolResult {
        images: vec![Image {
            mime: "image/png",
            bytes: PNG.to_vec(),
        }],
        files: vec![
            Blob {
                mime: "application/pdf",
                name: "a.pdf".into(),
                bytes: PDF.to_vec(),
            },
            Blob {
                mime: "image/svg+xml",
                name: "b.svg".into(),
                bytes: SVG.as_bytes().to_vec(),
            },
        ],
        ..result(json!({"k": "v"}))
    };
    let mapped = wire(&to_mcp(r, &versions(), UNLIMITED));
    let kinds: Vec<&str> = mapped["content"]
        .as_array()
        .unwrap()
        .iter()
        .map(|block| block["type"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["text", "image", "resource", "resource"]);
    assert_eq!(
        mapped["content"][2]["resource"]["uri"],
        "reshiki:result/a.pdf"
    );
    assert_eq!(
        mapped["content"][3]["resource"]["uri"],
        "reshiki:result/b.svg"
    );
}

/// Every kind but the two protocol outcomes is a tool execution error.
#[test]
fn tool_errors_are_is_error_results() {
    for (kind, _) in FAILURES {
        if matches!(kind, ErrorKind::UnknownTool | ErrorKind::Cancelled) {
            continue;
        }
        let error = OpError::new(kind, "it failed");
        let mapped = wire(&to_mcp(
            ToolResult::error(&error, &versions()),
            &versions(),
            UNLIMITED,
        ));
        let structured = json!({
            "error": {"code": kind.code(), "message": "it failed"},
            "versions": {
                "app": "0.0.0-test",
                "operation_api": versions().operation_api,
                "engine_protocol": versions().engine_protocol,
                "document": versions().document,
            },
        });
        assert_eq!(
            mapped,
            json!({
                "resultType": "complete",
                "content": [{"type": "text", "text": structured.to_string()}],
                "structuredContent": structured,
                "isError": true,
            }),
            "{kind:?}"
        );
    }
}

/// The exact serialized size of the mapped result.
fn size(r: &ToolResult) -> usize {
    serde_json::to_vec(&build(r.clone())).unwrap().len()
}

fn assert_cap_boundary(r: &ToolResult) {
    let exact = size(r);
    let at_cap = to_mcp(r.clone(), &versions(), exact);
    assert_eq!(at_cap, build(r.clone()));
    let over = wire(&to_mcp(r.clone(), &versions(), exact - 1));
    assert_eq!(over["isError"], true);
    assert_eq!(
        over["structuredContent"]["error"],
        json!({
            "code": "budget",
            "message": format!(
                "Result exceeds {} bytes; request a smaller render or fewer objects",
                exact - 1
            ),
        })
    );
    assert_eq!(over["content"].as_array().unwrap().len(), 1);
}

#[test]
fn the_size_cap_is_exact_for_escaping_heavy_text() {
    let mut value = Map::new();
    value.insert("text".into(), quote_heavy(10_000).into());
    let r = result(Value::Object(value));
    // Escaping more than doubles the text, and the copy doubles it again.
    assert!(size(&r) > 40_000, "{}", size(&r));
    assert_cap_boundary(&r);
}

#[test]
fn the_size_cap_is_exact_for_several_content_parts() {
    let r = ToolResult {
        images: vec![Image {
            mime: "image/png",
            bytes: PNG.repeat(100),
        }],
        files: vec![
            Blob {
                mime: "application/pdf",
                name: "a.pdf".into(),
                bytes: PDF.repeat(100),
            },
            Blob {
                mime: "image/svg+xml",
                name: "b.svg".into(),
                bytes: SVG.repeat(100).into_bytes(),
            },
        ],
        ..result(json!({"label": quote_heavy(100)}))
    };
    assert_cap_boundary(&r);
}

#[test]
fn the_counter_fails_once_past_the_cap() {
    let mut counter = Counter { n: 0, cap: 4 };
    assert!(counter.write_all(b"ab").is_ok());
    assert!(counter.write_all(b"cd").is_ok());
    assert!(counter.write_all(b"e").is_err());
    let mut counter = Counter {
        n: usize::MAX - 1,
        cap: usize::MAX,
    };
    assert!(counter.write_all(b"ab").is_err());
}
