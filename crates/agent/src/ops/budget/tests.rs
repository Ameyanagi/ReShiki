use super::*;
use crate::{document::Point, pictures::Picture};
use serde_json::json;

/// A 2 × 1 RGBA PNG.
const PNG_BASE64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAIAAAABCAYAAAD0In+KAAAADklEQVR4nGP4z8AAQv8BD/kD/YURmXYAAAAASUVORK5CYII=";

fn picture() -> Picture {
    serde_json::from_value(json!(PNG_BASE64)).unwrap()
}

fn with_pictures(pictures: &[&Picture]) -> Document {
    let mut doc = Document::default();
    for (id, picture) in (1..).zip(pictures) {
        doc.graphics.push(picture.graphic(id, Point::new(0., 0.)));
    }
    doc
}

#[test]
fn defaults_are_the_documented_values() {
    let budgets = Budgets::default();
    assert_eq!(budgets.max_request_bytes, 24 * 1024 * 1024);
    assert_eq!(budgets.max_text_bytes, reshiki_io::exchange::LIMIT);
    assert_eq!(budgets.max_text_bytes, 16 * 1024 * 1024);
    assert_eq!(budgets.max_cdx_base64, 22_369_624);
    assert_eq!(budgets.max_documents, 16);
    assert_eq!(budgets.max_objects, 100_000);
    assert_eq!(budgets.max_ids, 5_000);
    assert_eq!(budgets.max_session_weight, 2_000_000);
    assert_eq!(budgets.max_session_picture_bytes, 128 * 1024 * 1024);
    assert_eq!(budgets.history_depth, 20);
    assert_eq!(budgets.max_inspect_objects, 5_000);
    assert_eq!(
        budgets.render,
        RenderBudget {
            default_width: 1600,
            default_height: 1000,
            min_side: 1,
            max_side: 8192,
            max_pixels: 16_000_000,
        }
    );
    assert_eq!(budgets.render.max_pixels, crate::pictures::MAX_PIXELS);
    assert_eq!(budgets.export_png_pixels, 16_000_000);
    assert_eq!(budgets.max_output_bytes, 16 * 1024 * 1024);
    assert_eq!(budgets.max_output_bytes.div_ceil(3) * 4, 22_369_624);
    assert_eq!(budgets.concurrency, 2);
    assert_eq!(budgets.queue, 8);
    assert_eq!(budgets.op_deadline, Duration::from_secs(120));
    assert_eq!(budgets.idle_ttl, Duration::from_secs(3600));
    assert_eq!(budgets.idempotency_receipts, 32);
    assert_eq!(budgets.max_idempotency_key_bytes, 128);
    assert_eq!(budgets.validate(), Ok(()));
}

/// One change to the default budgets.
type Change = fn(&mut Budgets);

fn validate_with(change: impl FnOnce(&mut Budgets)) -> Result<(), String> {
    let mut budgets = Budgets::default();
    change(&mut budgets);
    budgets.validate()
}

#[test]
fn validate_rejects_each_zero_count() {
    let zeroed: [(&str, Change); 13] = [
        ("max_request_bytes", |b| b.max_request_bytes = 0),
        ("max_text_bytes", |b| b.max_text_bytes = 0),
        ("max_cdx_base64", |b| b.max_cdx_base64 = 0),
        ("max_documents", |b| b.max_documents = 0),
        ("max_objects", |b| b.max_objects = 0),
        ("max_ids", |b| b.max_ids = 0),
        ("history_depth", |b| b.history_depth = 0),
        ("max_inspect_objects", |b| b.max_inspect_objects = 0),
        ("max_output_bytes", |b| b.max_output_bytes = 0),
        ("concurrency", |b| b.concurrency = 0),
        ("queue", |b| b.queue = 0),
        ("idempotency_receipts", |b| b.idempotency_receipts = 0),
        ("max_idempotency_key_bytes", |b| {
            b.max_idempotency_key_bytes = 0
        }),
    ];
    for (name, change) in zeroed {
        assert_eq!(
            validate_with(change),
            Err(format!("{name} must be positive"))
        );
    }
}

#[test]
fn validate_rejects_inconsistent_budgets() {
    let session = "Session and export limits must be positive";
    let timing = "op_deadline and idle_ttl must be positive";
    let import = "Import limits must fit within max_request_bytes";
    let render = "The default render size must be within the render limits";
    let cases: [(Change, &str); 14] = [
        (|b| b.max_session_weight = 0, session),
        (|b| b.max_session_picture_bytes = 0, session),
        (|b| b.export_png_pixels = 0, session),
        (|b| b.op_deadline = Duration::ZERO, timing),
        (|b| b.idle_ttl = Duration::ZERO, timing),
        (|b| b.max_text_bytes = b.max_request_bytes + 1, import),
        (|b| b.max_cdx_base64 = b.max_request_bytes + 1, import),
        (|b| b.render.min_side = 0, render),
        (|b| b.render.default_width = 0, render),
        (|b| b.render.default_width = 8193, render),
        (|b| b.render.min_side = 1001, render),
        (|b| b.render.default_height = 8193, render),
        (|b| b.render.max_side = 1599, render),
        (|b| b.render.max_pixels = 1_599_999, render),
    ];
    for (change, message) in cases {
        assert_eq!(validate_with(change), Err(message.into()));
    }
    assert_eq!(
        validate_with(|b| {
            b.max_text_bytes = b.max_request_bytes;
            b.max_cdx_base64 = b.max_request_bytes;
            b.render.min_side = 1000;
            b.render.max_side = 1600;
            b.render.max_pixels = 1_600_000;
        }),
        Ok(())
    );
}

#[test]
fn objects_counts_each_category_once() {
    let empty = Document::default();
    assert_eq!(objects(&empty), 0);
    assert_eq!(cost(&empty), 1);
    let mut doc: Document = serde_json::from_value(json!({
        "version": crate::document::VERSION,
        "atoms": [{"id": 1, "element": "C", "position": {"x": 0, "y": 0}}],
        "bonds": [{"a": 1, "b": 1, "order": 1}],
        "annotations": [{"id": 2, "position": {"x": 0, "y": 0}, "text": "x"}],
        "arrows": [{"id": 3, "start": {"x": 0, "y": 0}, "end": {"x": 1, "y": 0}}],
        "groups": [{"id": 5, "members": [1]}],
    }))
    .unwrap();
    doc.graphics.push(picture().graphic(4, Point::new(0., 0.)));
    assert_eq!(objects(&doc), 6);
    assert_eq!(cost(&doc), 7);
    // Clearing one category at a time removes exactly one object each, so a
    // category that is skipped, doubled or swapped for another fails here.
    let clears: [fn(&mut Document); 6] = [
        |d| d.atoms.clear(),
        |d| d.bonds.clear(),
        |d| d.annotations.clear(),
        |d| d.arrows.clear(),
        |d| d.graphics.clear(),
        |d| d.groups.clear(),
    ];
    for (left, clear) in (0..6).rev().zip(clears) {
        clear(&mut doc);
        assert_eq!(objects(&doc), left);
        assert_eq!(cost(&doc), left as u64 + 1);
    }
}

#[test]
fn picture_memory_counts_a_shared_picture_once() {
    let shared = picture();
    let size = shared.png().len() as u64;
    assert!(size > 0);
    let doc = with_pictures(&[&shared, &shared.clone()]);
    let frame = doc.clone();
    assert_eq!(picture_memory([&doc]), size);
    assert_eq!(picture_memory([&doc, &frame, &doc]), size);

    let separate = picture();
    let other = with_pictures(&[&separate]);
    assert_eq!(
        picture_memory([&doc, &frame, &other]),
        size + separate.png().len() as u64
    );
    assert_eq!(picture_memory([&Document::default()]), 0);
    assert_eq!(picture_memory(std::iter::empty::<&Document>()), 0);
}

#[test]
fn picture_memory_includes_original_metafile_once_across_history() {
    let mut emf = vec![0; 108];
    for (offset, value) in [
        (0, 1),
        (4, 88),
        (32, 2540),
        (36, 1270),
        (40, 0x464d4520),
        (44, 0x10000),
        (48, 108),
        (52, 2),
        (56, 1),
        (88, 14),
        (92, 20),
        (104, 20),
    ] {
        emf[offset..offset + 4].copy_from_slice(&u32::to_le_bytes(value));
    }
    let preview = picture();
    let original = crate::pictures::Picture::from_emf(&emf, preview.png()).unwrap();
    let document = with_pictures(&[&original, &original.clone()]);
    assert_eq!(
        picture_memory([&document, &document.clone()]),
        (original.png().len() + emf.len()) as u64
    );
}
