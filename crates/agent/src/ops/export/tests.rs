use super::*;
use crate::{
    document::Point,
    editing,
    ops::{
        headless::HeadlessHost,
        host::{Call, ToolHost},
        wire::RequestId,
    },
    pages, scene,
};
use std::sync::atomic::{AtomicI64, Ordering};

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
const PREPARATION_NOTICE: &str = "Drawing preserved; chemistry needs review: ";
const NO_PAGES: &str = "Set up publication pages before exporting a page PDF.";
const LADDER: &str = "Drawing is too large for a PNG even at 72 dpi; use SVG or PDF.";

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

fn hosting_with(doc: Document, budgets: Budgets) -> (HeadlessHost, DocHandle) {
    let host = HeadlessHost::new("9.8.7", budgets);
    let created = host.store().create(&Principal::local(), doc).unwrap();
    (host, created.handle)
}

fn hosting(doc: Document) -> (HeadlessHost, DocHandle) {
    hosting_with(doc, Budgets::default())
}

async fn export_call(
    host: &HeadlessHost,
    handle: &DocHandle,
    format: &str,
    pages: Value,
) -> ToolResult {
    host.call(call(
        "export",
        json!({"document": handle.as_str(), "format": format, "pages": pages}),
    ))
    .await
    .unwrap()
}

async fn export_ok(host: &HeadlessHost, handle: &DocHandle, format: &str) -> ToolResult {
    let result = export_call(host, handle, format, Value::Null).await;
    assert!(!result.is_error, "{format}: {:?}", result.value);
    assert!(result.images.is_empty());
    result
}

/// Ethanol imported from SMILES, as clients start.
async fn ethanol(host: &HeadlessHost) -> DocHandle {
    let imported = host
        .call(call("import", json!({"format": "smiles", "text": "CCO"})))
        .await
        .unwrap();
    assert!(!imported.is_error, "{:?}", imported.value);
    DocHandle::new(imported.value["value"]["document"].as_str().unwrap()).unwrap()
}

/// An aromatic five-ring the engine cannot resolve, so figure exports keep
/// the drawing and add the preparation notice (tests/figure_exports.rs).
fn unresolved_ring() -> Document {
    let mut doc = Document::default();
    editing::ring(&mut doc, Point::default(), 5, true, 42.);
    doc
}

fn messages(result: &ToolResult) -> Vec<String> {
    result.value["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|warning| warning["message"].as_str().unwrap().to_owned())
        .collect()
}

fn error(result: &ToolResult) -> (&str, &str) {
    assert!(result.is_error, "{:?}", result.value);
    (
        result.value["error"]["code"].as_str().unwrap(),
        result.value["error"]["message"].as_str().unwrap(),
    )
}

#[test]
fn formats_are_exactly_the_published_export_formats() {
    for name in EXPORT_FORMATS {
        let format: Format = serde_json::from_value(json!(name)).unwrap();
        assert_eq!(format.name(), *name);
    }
    for excluded in ["emf", "cdx", "rxn", "rsmi", "reshiki", "PNG"] {
        assert!(serde_json::from_value::<Format>(json!(excluded)).is_err());
    }
}

#[test]
fn pages_is_valid_only_with_pdf() {
    let decoded = |format: &str, pages: Value| {
        decode(json!({"document": "doc_1", "format": format, "pages": pages})).map(drop)
    };
    for name in EXPORT_FORMATS {
        assert_eq!(decoded(name, Value::Null), Ok(()), "{name}");
        assert_eq!(decoded(name, json!(false)), Ok(()), "{name}");
        let pages = decoded(name, json!(true));
        if *name == "pdf" {
            assert_eq!(pages, Ok(()));
        } else {
            assert_eq!(
                pages,
                Err(OpError::new(
                    ErrorKind::InvalidArguments,
                    "pages is valid only with format pdf"
                )),
                "{name}"
            );
        }
    }
    let omitted = decode(json!({"document": "doc_1", "format": "pdf"})).unwrap_err();
    assert_eq!(omitted.message, "missing field `pages`");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_format_is_non_empty_with_a_correct_receipt() {
    let host = HeadlessHost::new("9.8.7", Budgets::default());
    let handle = ethanol(&host).await;
    for name in EXPORT_FORMATS {
        let format: Format = serde_json::from_value(json!(name)).unwrap();
        let result = export_ok(&host, &handle, name).await;
        let [file] = result.files.as_slice() else {
            panic!("{name}: one file")
        };
        assert_eq!(file.name, format!("drawing.{name}"));
        assert_eq!(file.mime, format.mime());
        assert!(!file.bytes.is_empty(), "{name}");
        let receipt = &result.value["value"]["receipt"];
        assert_eq!(receipt["format"], *name);
        assert_eq!(receipt["byte_len"], file.bytes.len());
        assert_eq!(result.value["validation"], json!({"status": "valid"}));
        let text = String::from_utf8_lossy(&file.bytes);
        match *name {
            "png" => {
                assert!(file.bytes.starts_with(PNG_SIGNATURE));
                let detail = receipt["detail"].as_str().unwrap();
                assert!(detail.starts_with("PNG: "), "{detail}");
                // The receipt's detail is also the first warning, as the app
                // shows it first in its status.
                assert_eq!(messages(&result).first().map(String::as_str), Some(detail));
            }
            other => {
                assert_eq!(receipt["detail"], Value::Null, "{other}");
                match other {
                    "svg" => assert!(text.starts_with("<svg"), "{text}"),
                    "pdf" => assert!(file.bytes.starts_with(b"%PDF-")),
                    "cdxml" => assert!(text.contains("<CDXML"), "{text}"),
                    "mol" => assert!(text.contains("M  END"), "{text}"),
                    "smiles" => assert_eq!(text.trim(), "CCO"),
                    "inchi" => assert!(text.starts_with("InChI=1S/C2H6O/"), "{text}"),
                    _ => panic!("untested format {other}"),
                }
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn figures_are_the_publication_export() {
    let doc = unresolved_ring();
    let (host, handle) = hosting(doc.clone());
    let engine = LocalEngine::default();
    for format in ["svg", "pdf", "png"] {
        let expected = crate::export::publication(
            &engine,
            doc.clone(),
            format,
            false,
            Budgets::default().export_png_pixels,
        )
        .await
        .unwrap();
        let result = export_ok(&host, &handle, format).await;
        // The details become the warnings in order, the PNG size first and
        // the preparation notice last.
        let warnings = messages(&result);
        assert_eq!(warnings, expected.details, "{format}");
        assert!(
            warnings.last().unwrap().starts_with(PREPARATION_NOTICE),
            "{format}: {warnings:?}"
        );
        assert_eq!(warnings.len(), if format == "png" { 2 } else { 1 });
        // The PDF is not byte-stable between runs; compare its structure.
        if format == "pdf" {
            assert!(result.files[0].bytes.starts_with(b"%PDF-"));
        } else {
            assert!(result.files[0].bytes == expected.bytes, "{format}");
        }
    }
    // Publication semantics: the drawing's background, unlike render's
    // preview.
    let svg = export_ok(&host, &handle, "svg").await;
    assert_eq!(
        svg.files[0].bytes,
        scene::svg_with_background(&doc).into_bytes()
    );
    assert_ne!(svg.files[0].bytes, scene::svg(&doc).into_bytes());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_page_pdf_needs_publication_pages() {
    let mut doc = unresolved_ring();
    let (host, handle) = hosting(doc.clone());
    let result = export_call(&host, &handle, "pdf", json!(true)).await;
    assert_eq!(error(&result), ("failed", NO_PAGES));
    doc.page_layout = Some(pages::Layout {
        columns: 2,
        ..pages::Layout::default()
    });
    let (host, handle) = hosting(doc);
    let result = export_call(&host, &handle, "pdf", json!(true)).await;
    assert!(!result.is_error, "{:?}", result.value);
    let pdf = String::from_utf8_lossy(&result.files[0].bytes).into_owned();
    assert!(pdf.starts_with("%PDF-"));
    assert!(pdf.contains("/Count 2"), "two pages");
    assert_eq!(result.value["value"]["receipt"]["detail"], Value::Null);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_png_beyond_the_pixel_budget_gives_the_resolution_ladder_error() {
    let (host, handle) = hosting_with(
        unresolved_ring(),
        Budgets {
            export_png_pixels: 100,
            ..Budgets::default()
        },
    );
    let result = export_call(&host, &handle, "png", Value::Null).await;
    assert_eq!(error(&result), ("failed", LADDER));
    // Vector figures have no pixel budget.
    export_ok(&host, &handle, "svg").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_file_must_fit_the_output_budget() {
    let host = HeadlessHost::new(
        "9.8.7",
        Budgets {
            max_output_bytes: 10,
            ..Budgets::default()
        },
    );
    let handle = ethanol(&host).await;
    for format in ["png", "mol"] {
        let result = export_call(&host, &handle, format, Value::Null).await;
        let (code, message) = error(&result);
        assert_eq!(code, "budget");
        assert!(
            message.starts_with(&format!("The {format} file is "))
                && message.ends_with(
                    "max_output_bytes allows at most 10. Use svg or pdf, or reduce the drawing"
                ),
            "{message}"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_engine_error_fails_with_its_message() {
    let (host, handle) = hosting(Document::default());
    let result = export_call(&host, &handle, "smiles", Value::Null).await;
    let (code, message) = error(&result);
    assert_eq!(code, "failed");
    assert!(!message.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn exporting_never_changes_the_document() {
    let host = HeadlessHost::new("9.8.7", Budgets::default());
    let handle = ethanol(&host).await;
    let before = host
        .store()
        .snapshot(&Principal::local(), &handle, Access::Read)
        .unwrap();
    for format in ["svg", "mol"] {
        export_ok(&host, &handle, format).await;
    }
    let after = host
        .store()
        .snapshot(&Principal::local(), &handle, Access::Read)
        .unwrap();
    assert_eq!(after.revision, before.revision);
    assert!(Arc::ptr_eq(&after.doc, &before.doc));
}
