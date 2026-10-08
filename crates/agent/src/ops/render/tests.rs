use super::*;
use crate::{
    document::{Annotation, Document, Point},
    ops::{
        headless::HeadlessHost,
        host::{Call, ToolHost},
        wire::RequestId,
    },
};
use std::sync::atomic::{AtomicI64, Ordering};

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

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

/// A render call; `size` is `[max_width, max_height]`.
fn arguments(handle: &DocHandle, format: &str, size: [Value; 2], ids: Value) -> Value {
    let [max_width, max_height] = size;
    json!({
        "document": handle.as_str(),
        "format": format,
        "max_width": max_width,
        "max_height": max_height,
        "ids": ids,
    })
}

async fn render_ok(host: &HeadlessHost, arguments: Value) -> ToolResult {
    let result = host.call(call("render", arguments)).await.unwrap();
    assert!(!result.is_error, "{:?}", result.value);
    result
}

fn strings(ids: &[u64]) -> Value {
    ids.iter().map(|id| Value::from(id.to_string())).collect()
}

/// Ethanol beside a caption: `[carbon, carbon, oxygen, caption]`.
fn drawing() -> (Document, [u64; 4]) {
    let mut doc = Document::default();
    let atoms = [
        doc.add_atom("C", Point::new(0., 0.)),
        doc.add_atom("C", Point::new(42., 0.)),
        doc.add_atom("O", Point::new(84., 0.)),
    ];
    doc.add_bond(atoms[0], atoms[1], 1, "plain");
    doc.add_bond(atoms[1], atoms[2], 1, "plain");
    let caption = doc.next_id();
    doc.annotations.push(Annotation {
        id: caption,
        position: Point::new(200., 0.),
        text: "ethanol".into(),
        format: Default::default(),
    });
    let [a, b, c] = atoms;
    (doc, [a, b, c, caption])
}

fn decode_size(width: Value, height: Value) -> Result<(u32, u32), OpError> {
    decode(
        json!({"document": "doc_1", "format": "png", "max_width": width, "max_height": height, "ids": null}),
        &Budgets::default(),
    )
    .map(|render| (render.width, render.height))
}

#[test]
fn the_size_is_checked_at_decode_time() {
    let budgets = Budgets::default();
    assert_eq!(decode_size(Value::Null, Value::Null), Ok((1600, 1000)));
    assert_eq!(decode_size(json!(1), json!(1)), Ok((1, 1)));
    assert_eq!(decode_size(json!(8192), Value::Null), Ok((8192, 1000)));
    // 8192 × 1953 is just within 16,000,000 pixels.
    assert_eq!(decode_size(json!(8192), json!(1953)), Ok((8192, 1953)));
    for (width, height, name) in [
        (json!(0), Value::Null, "max_width"),
        (Value::Null, json!(0), "max_height"),
        (json!(8193), json!(1), "max_width"),
    ] {
        assert_eq!(
            decode_size(width, height),
            Err(OpError::new(
                ErrorKind::InvalidArguments,
                format!("{name} must be from 1 to 8192 pixels")
            ))
        );
    }
    let error = decode_size(json!(8192), json!(1954)).unwrap_err();
    assert_eq!(error.kind, ErrorKind::Budget);
    assert_eq!(
        error.message,
        "8192 × 1954 is 16007168 pixels; render.max_pixels allows at most 16000000"
    );
    let error = decode_size(json!(8192), json!(8192)).unwrap_err();
    assert_eq!(error.kind, ErrorKind::Budget);
    // The schema states the default side limits.
    let side = &schema()["properties"]["max_width"]["anyOf"][1];
    assert_eq!(side["minimum"], budgets.render.min_side);
    assert_eq!(side["maximum"], budgets.render.max_side);
    assert_eq!(schema()["properties"]["max_height"]["anyOf"][1], *side);
}

#[test]
fn ids_over_the_budget_are_refused_at_decode_time() {
    let budgets = Budgets::default();
    let decoded = |n: usize| {
        decode(
            json!({"document": "doc_1", "format": "svg", "max_width": null, "max_height": null, "ids": vec![json!("1"); n]}),
            &budgets,
        )
        .map(drop)
    };
    assert!(decoded(budgets.max_ids).is_ok());
    assert_eq!(
        decoded(budgets.max_ids + 1).unwrap_err().kind,
        ErrorKind::Budget
    );
}

#[test]
fn png_size_reads_the_ihdr_chunk() {
    let png = canvas_tools::image_within(&drawing().0, 120, 80).unwrap();
    let (width, height) = png_size(&png).unwrap();
    assert!((1..=120).contains(&width) && (1..=80).contains(&height));
    assert_eq!(png_size(&png[..23]), None);
    assert_eq!(png_size(b"not a png at all, not at all"), None);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn png_dimensions_respect_max_width_and_max_height() {
    let (doc, _) = drawing();
    let (host, handle) = hosting(doc.clone());
    for (size, limit) in [
        ([Value::Null, Value::Null], (1600, 1000)),
        ([json!(120), json!(80)], (120, 80)),
        ([json!(40), Value::Null], (40, 1000)),
        ([json!(1), json!(1)], (1, 1)),
    ] {
        let result = render_ok(&host, arguments(&handle, "png", size, Value::Null)).await;
        assert!(result.files.is_empty());
        let [image] = result.images.as_slice() else {
            panic!("one image")
        };
        assert_eq!(image.mime, "image/png");
        assert!(image.bytes.starts_with(PNG_SIGNATURE));
        let (width, height) = png_size(&image.bytes).unwrap();
        assert!(width <= limit.0 && height <= limit.1, "{width} × {height}");
        assert_eq!(
            result.value["value"],
            json!({"width": width, "height": height, "byte_len": image.bytes.len()})
        );
        // The assistant's canvas preview at the same size.
        assert_eq!(
            image.bytes,
            canvas_tools::image_within(&doc, limit.0, limit.1).unwrap()
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_zero_width_is_an_invalid_argument_then_a_valid_call_succeeds() {
    let (host, handle) = hosting(drawing().0);
    let result = host
        .call(call(
            "render",
            arguments(&handle, "png", [json!(0), Value::Null], Value::Null),
        ))
        .await
        .unwrap();
    assert!(result.is_error);
    assert_eq!(
        result.value["error"],
        json!({"code": "invalid_arguments", "message": "max_width must be from 1 to 8192 pixels"})
    );
    render_ok(
        &host,
        arguments(&handle, "png", [json!(1), Value::Null], Value::Null),
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn svg_is_the_preview_svg() {
    let (doc, _) = drawing();
    let (host, handle) = hosting(doc.clone());
    let result = render_ok(
        &host,
        arguments(&handle, "svg", [Value::Null, Value::Null], Value::Null),
    )
    .await;
    assert!(result.images.is_empty());
    let [file] = result.files.as_slice() else {
        panic!("one file")
    };
    assert_eq!(
        (file.mime, file.name.as_str()),
        ("image/svg+xml", "drawing.svg")
    );
    assert!(file.bytes.starts_with(b"<svg"));
    // Preview semantics: no background, unlike a publication export.
    assert_eq!(file.bytes, scene::svg(&doc).into_bytes());
    assert_ne!(file.bytes, scene::svg_with_background(&doc).into_bytes());
    assert_eq!(
        result.value["value"],
        json!({"width": null, "height": null, "byte_len": file.bytes.len()})
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_selection_renders_only_its_part() {
    let (doc, [_, carbon, oxygen, _]) = drawing();
    let (host, handle) = hosting(doc.clone());
    let part = editing::selection(&doc, &[carbon, oxygen]);
    assert_eq!(part.atoms.len(), 2);
    let ids = strings(&[carbon, oxygen]);
    let svg = render_ok(
        &host,
        arguments(&handle, "svg", [Value::Null, Value::Null], ids.clone()),
    )
    .await;
    assert_eq!(svg.files[0].bytes, scene::svg(&part).into_bytes());
    let png = render_ok(
        &host,
        arguments(&handle, "png", [json!(300), json!(200)], ids),
    )
    .await;
    assert_eq!(
        png.images[0].bytes,
        canvas_tools::image_within(&part, 300, 200).unwrap()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unknown_ids_name_the_first_missing_one() {
    let (doc, [carbon, .., caption]) = drawing();
    let (host, handle) = hosting(doc);
    let missing = caption + 100;
    for format in ["png", "svg"] {
        for (ids, first) in [
            (strings(&[missing]), missing),
            (strings(&[carbon, missing + 1, missing]), missing + 1),
        ] {
            let result = host
                .call(call(
                    "render",
                    arguments(&handle, format, [Value::Null, Value::Null], ids),
                ))
                .await
                .unwrap();
            assert!(result.is_error);
            assert_eq!(
                result.value["error"],
                json!({"code": "unknown_object", "message": format!("Object {first} is not in the drawing")})
            );
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_preview_must_fit_the_output_budget() {
    let (host, handle) = hosting_with(
        drawing().0,
        Budgets {
            max_output_bytes: 100,
            ..Budgets::default()
        },
    );
    for format in ["png", "svg"] {
        let result = host
            .call(call(
                "render",
                arguments(&handle, format, [Value::Null, Value::Null], Value::Null),
            ))
            .await
            .unwrap();
        assert!(result.is_error);
        assert_eq!(result.value["error"]["code"], "budget");
        let message = result.value["error"]["message"].as_str().unwrap();
        assert!(
            message.starts_with(&format!("The {format} preview is "))
                && message.contains("max_output_bytes allows at most 100."),
            "{message}"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rendering_never_changes_the_document() {
    let (host, handle) = hosting(drawing().0);
    let before = host
        .store()
        .snapshot(&Principal::local(), &handle, Access::Read)
        .unwrap();
    for format in ["png", "svg"] {
        render_ok(
            &host,
            arguments(&handle, format, [Value::Null, Value::Null], Value::Null),
        )
        .await;
    }
    let after = host
        .store()
        .snapshot(&Principal::local(), &handle, Access::Read)
        .unwrap();
    assert_eq!(after.revision, before.revision);
    assert!(Arc::ptr_eq(&after.doc, &before.doc));
}
