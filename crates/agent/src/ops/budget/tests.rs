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

fn validate_with(change: impl FnOnce(&mut Budgets)) -> Result<(), String> {
    let mut budgets = Budgets::default();
    change(&mut budgets);
    budgets.validate()
}

#[test]
fn validate_rejects_inconsistent_budgets() {
    let positive = |name: &str| Err(format!("{name} must be positive"));
    assert_eq!(
        validate_with(|b| b.concurrency = 0),
        positive("concurrency")
    );
    assert_eq!(validate_with(|b| b.max_ids = 0), positive("max_ids"));
    assert_eq!(
        validate_with(|b| b.max_session_weight = 0),
        Err("Session and export limits must be positive".into())
    );
    assert_eq!(
        validate_with(|b| b.op_deadline = Duration::ZERO),
        Err("op_deadline and idle_ttl must be positive".into())
    );
    assert_eq!(
        validate_with(|b| b.max_cdx_base64 = b.max_request_bytes + 1),
        Err("Import limits must fit within max_request_bytes".into())
    );
    let render = Err("The default render size must be within the render limits".into());
    assert_eq!(validate_with(|b| b.render.max_side = 1000), render);
    assert_eq!(validate_with(|b| b.render.max_pixels = 1_599_999), render);
    assert_eq!(validate_with(|b| b.render.min_side = 0), render);
}

#[test]
fn cost_counts_every_object_plus_one() {
    let empty = Document::default();
    assert_eq!(objects(&empty), 0);
    assert_eq!(cost(&empty), 1);
    let doc = with_pictures(&[&picture(), &picture()]);
    assert_eq!(objects(&doc), 2);
    assert_eq!(cost(&doc), 3);
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
