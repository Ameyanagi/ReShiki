#![cfg(windows)]

use reshiki::{
    document::{Document, Point},
    printing::{self, Scope},
};

fn emf_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn assert_vector_emf(bytes: &[u8], width_pt: f32, height_pt: f32) {
    assert_eq!(emf_u32(bytes, 0), 1, "missing EMR_HEADER");
    assert_eq!(emf_u32(bytes, 40), 0x464d4520, "missing EMF signature");
    assert_eq!(emf_u32(bytes, 48) as usize, bytes.len());
    for (lo, hi, points) in [(24, 32, width_pt), (28, 36, height_pt)] {
        let physical = emf_u32(bytes, hi) as i32 - emf_u32(bytes, lo) as i32;
        assert!(
            (physical as f32 - points * 2540. / 72.).abs() <= 1.,
            "physical EMF frame changed: {physical} vs {points} pt"
        );
    }
    let (mut offset, mut paths) = (0, 0);
    while offset < bytes.len() {
        let kind = emf_u32(bytes, offset);
        let size = emf_u32(bytes, offset + 4) as usize;
        assert!(size >= 8 && offset + size <= bytes.len());
        if [3, 8, 59, 86, 91].contains(&kind) {
            paths += 1;
        }
        assert!(
            ![77, 80, 81, 114, 116].contains(&kind),
            "rasterized drawing"
        );
        if kind == 76 {
            assert_eq!(emf_u32(bytes, offset + 84), 0);
            assert_eq!(emf_u32(bytes, offset + 92), 0);
        }
        offset += size;
    }
    assert!(paths > 0, "missing vector outlines");
}

fn assert_file_emf_intrinsic_size(bytes: &[u8], width_pt: f32, height_pt: f32) {
    let plus_header = emf_u32(bytes, 4) as usize;
    for (lo, hi, dpi_offset, points) in [
        (8, 16, plus_header + 36, width_pt),
        (12, 20, plus_header + 40, height_pt),
    ] {
        let pixels = emf_u32(bytes, hi) as i32 - emf_u32(bytes, lo) as i32 + 1;
        let dpi = emf_u32(bytes, dpi_offset) as f32;
        assert_eq!(dpi, 2540.);
        assert!(
            (pixels as f32 * 25.4 / dpi - points * 25.4 / 72.).abs() <= 0.021,
            "Office intrinsic size changed: {pixels}px / {dpi}dpi vs {points}pt"
        );
    }
}

#[test]
fn windows_emf_file_export_keeps_vectors_bounds_and_source_document() {
    let mut doc: Document =
        serde_json::from_str(include_str!("fixtures/ui-drawn-ethanol.reshiki")).unwrap();
    // Ordinary figure export crops the whole drawing, independently of pages.
    doc.page_layout = Some(reshiki::pages::Layout::default());
    for theme in reshiki::canvas_theme::CanvasTheme::ALL {
        doc.canvas_theme = theme;
        let original = doc.clone();
        let svg = reshiki::scene::svg_with_background(&doc);
        let tree = resvg::usvg::Tree::from_str(&svg, &Default::default()).unwrap();
        let file = reshiki::export::figure(&doc, "emf").unwrap();
        let clipboard = reshiki::export::clipboard_drawing(&doc, "emf").unwrap();
        assert_vector_emf(
            &file.bytes,
            tree.size().width() * 0.75,
            tree.size().height() * 0.75,
        );
        assert_file_emf_intrinsic_size(
            &file.bytes,
            tree.size().width() * 0.75,
            tree.size().height() * 0.75,
        );
        assert_vector_emf(
            &clipboard,
            tree.size().width() * 0.75,
            tree.size().height() * 0.75,
        );
        assert_ne!(
            file.bytes, clipboard,
            "file export lost its canvas background"
        );
        assert!(file.detail.is_none());
        assert_eq!(doc, original);
    }
    let mut oversized = Document::default();
    let a = oversized.add_atom("C", Point::default());
    let b = oversized.add_atom("C", Point::new(10000., 0.));
    oversized.add_bond(a, b, 1, "plain");
    assert!(
        reshiki::export::drawing(&oversized, "emf")
            .unwrap_err()
            .contains("40-inch")
    );
    doc.atoms[0].position.x = f32::NAN;
    assert!(reshiki::export::drawing(&doc, "emf").is_err());
}

#[test]
#[ignore = "writes real Windows EMF and PNG exports for visual review"]
fn windows_emf_file_export_review() {
    let output = std::path::PathBuf::from(
        std::env::var_os("RESHIKI_EMF_REVIEW_DIR").expect("set RESHIKI_EMF_REVIEW_DIR"),
    );
    std::fs::create_dir_all(&output).unwrap();
    let mut doc: Document =
        serde_json::from_str(include_str!("../docs/changes/fixtures/emf-export.rsk")).unwrap();
    for (name, theme) in [
        ("light", reshiki::canvas_theme::CanvasTheme::Light),
        ("dark", reshiki::canvas_theme::CanvasTheme::Dark),
    ] {
        doc.canvas_theme = theme;
        for format in ["emf", "png"] {
            std::fs::write(
                output.join(format!("{name}.{format}")),
                reshiki::export::drawing(&doc, format).unwrap(),
            )
            .unwrap();
        }
    }
    // A separate mixed vector/raster figure exercises picture placement and
    // transparency without changing the established light/dark comparison.
    let pixels = image::RgbaImage::from_fn(64, 48, |x, y| match (x < 32, y < 24) {
        (true, true) => image::Rgba([230, 30, 40, 255]),
        (false, true) => image::Rgba([30, 170, 80, 255]),
        (true, false) => image::Rgba([40, 80, 230, 255]),
        (false, false) => image::Rgba([255, 160, 0, 128]),
    });
    let mut png = std::io::Cursor::new(Vec::new());
    pixels.write_to(&mut png, image::ImageFormat::Png).unwrap();
    let picture = reshiki::pictures::Picture::import(png.get_ref()).unwrap();
    let mut graphic = picture.graphic(doc.next_id(), Point::new(-40., -115.));
    graphic.axis_x.x *= 2.;
    graphic.axis_y.y *= 2.;
    doc.graphics.push(graphic);
    for (name, theme) in [
        ("raster-light", reshiki::canvas_theme::CanvasTheme::Light),
        ("raster-dark", reshiki::canvas_theme::CanvasTheme::Dark),
    ] {
        doc.canvas_theme = theme;
        for format in ["emf", "png"] {
            std::fs::write(
                output.join(format!("{name}.{format}")),
                reshiki::export::drawing(&doc, format).unwrap(),
            )
            .unwrap();
        }
    }
}

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
