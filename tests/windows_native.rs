#![cfg(windows)]

use reshiki::{
    document::{Document, Point},
    printing::{self, Scope},
};

#[test]
fn windows_print_pipeline_preserves_bond_length_and_page_position() {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(210., 210.));
    let b = doc.add_atom("C", Point::new(252., 210.));
    doc.add_bond(a, b, 1, "plain");
    doc.page_layout = Some(reshiki::pages::Layout::default());
    let snapshot = printing::snapshot(&doc, &[], Scope::Document).unwrap();
    let job = printing::prepare(snapshot, "Windows geometry test".into()).unwrap();
    let bytes = reshiki_windows::render(&job.native, 144.).unwrap();
    let image = image::load_from_memory(&bytes).unwrap().into_rgb8();
    let mut dark = image.enumerate_pixels().filter(|(_, _, p)| p[0] < 100);
    let (x, y, _) = dark.next().unwrap();
    let (mut lo, mut hi) = ((x, y), (x, y));
    for (x, y, _) in dark {
        lo = (lo.0.min(x), lo.1.min(y));
        hi = (hi.0.max(x), hi.1.max(y));
    }
    // 210 world units = 72 pt = 144 pixels at 144 dpi, bond = 14.4 pt.
    assert!((lo.0 as i32 - 144).abs() <= 2, "{lo:?}");
    assert!((lo.1 as i32 - 144).abs() <= 2, "{lo:?}");
    assert!((hi.0 as i32 - 173).abs() <= 2, "{hi:?}");
    assert!(hi.1 - lo.1 <= 3);
}

#[tokio::test]
async fn windows_clipboard_roundtrip_preserves_editable_drawing_and_copy_image() {
    let original: Document =
        serde_json::from_str(include_str!("fixtures/ui-drawn-ethanol.reshiki")).unwrap();
    let engine = reshiki::engine::LocalEngine::default();
    let outcome = reshiki::clipboard::copy(engine.clone(), original.clone(), false)
        .await
        .unwrap();
    assert!(outcome.external_editable);
    let pasted = reshiki::clipboard::paste(engine.clone(), false)
        .await
        .unwrap();
    assert_eq!(pasted.atoms.len(), original.atoms.len());
    assert_eq!(pasted.bonds.len(), original.bonds.len());
    assert_eq!(pasted.annotations[0].text, original.annotations[0].text);
    assert_eq!(pasted.arrows.len(), original.arrows.len());
    reshiki::clipboard::copy(engine.clone(), original.clone(), true)
        .await
        .unwrap();
    let image = reshiki::clipboard::paste(engine, true).await.unwrap();
    assert!(image.atoms.is_empty());
    assert_eq!(image.graphics.len(), 1);
}

#[test]
fn windows_print_matches_svg_for_labels_captions_and_rotated_transparent_pictures() {
    use resvg::{tiny_skia, usvg};
    let mut doc: Document =
        serde_json::from_str(include_str!("fixtures/ui-drawn-ethanol.reshiki")).unwrap();
    let picture = image::RgbaImage::from_fn(20, 20, |x, y| {
        if x < 10 && y < 10 {
            image::Rgba([0, 0, 0, 0])
        } else {
            image::Rgba([220, 30, 60, 255])
        }
    });
    let mut encoded = std::io::Cursor::new(Vec::new());
    picture
        .write_to(&mut encoded, image::ImageFormat::Png)
        .unwrap();
    let graphic = reshiki::pictures::Picture::import(encoded.get_ref())
        .unwrap()
        .graphic(999, Point::new(200., 100.));
    doc.graphics.push(graphic);
    reshiki::pictures::resize(&mut doc.graphics[0], 120., 120.).unwrap();
    reshiki::editing::transform(&mut doc, &[999], reshiki::editing::Transform::Rotate(37.));
    let snapshot = printing::snapshot(&doc, &[], Scope::Document).unwrap();
    let job = printing::prepare(snapshot.clone(), "Visual regression".into()).unwrap();
    let bytes = reshiki_windows::render(&job.native, 144.).unwrap();
    let actual = image::load_from_memory(&bytes).unwrap().into_rgba8();
    let mut options = usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    let tree = usvg::Tree::from_str(&reshiki::scene::svg(&snapshot), &options).unwrap();
    let mut reference = tiny_skia::Pixmap::new(actual.width(), actual.height()).unwrap();
    reference.fill(tiny_skia::Color::WHITE);
    let svg = reshiki::scene::svg(&snapshot);
    let view_box: Vec<f32> = svg
        .split_once("viewBox=\"")
        .unwrap()
        .1
        .split('"')
        .next()
        .unwrap()
        .split_whitespace()
        .map(|n| n.parse().unwrap())
        .collect();
    let drawing = Point::new(view_box[0], view_box[1]);
    let (page, _) = snapshot.page_layout.as_ref().unwrap().bounds(0).unwrap();
    let scale = reshiki::style::DEFAULT.points_per_world() * 2.;
    let transform = tiny_skia::Transform::from_translate(
        (drawing.x - page.x) * scale,
        (drawing.y - page.y) * scale,
    )
    .pre_scale(1.5, 1.5);
    resvg::render(&tree, transform, &mut reference.as_mut());
    let expected =
        image::RgbaImage::from_raw(actual.width(), actual.height(), reference.take()).unwrap();
    let ink = |p: &image::Rgba<u8>| p[0] < 180 || p[1] < 180 || p[2] < 180;
    for (source, target) in [(&actual, &expected), (&expected, &actual)] {
        let mut total = 0;
        let mut matched = 0;
        for (x, y, _) in source.enumerate_pixels().filter(|(_, _, p)| ink(p)) {
            total += 1;
            if (y.saturating_sub(2)..=(y + 2).min(target.height() - 1)).any(|yy| {
                (x.saturating_sub(2)..=(x + 2).min(target.width() - 1))
                    .any(|xx| ink(target.get_pixel(xx, yy)))
            }) {
                matched += 1;
            }
        }
        assert!(total > 200);
        assert!(
            matched as f32 / total as f32 > 0.97,
            "only {matched}/{total} printed ink pixels match the SVG layout"
        );
    }
}
