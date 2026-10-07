use super::*;
#[test]
fn raster_budget_rejects_invalid_and_extreme_sizes_without_allocating() {
    for (width, height) in [
        (0., 10.),
        (10., -1.),
        (f32::NAN, 10.),
        (10., f32::INFINITY),
        (f32::MAX, f32::MAX),
    ] {
        assert!(png_dimensions(width, height, RasterBudget::FILE).is_err());
    }
    assert_eq!(
        png_dimensions(100., 100., RasterBudget::FILE),
        Ok((1250, 1250, 1200))
    );
    assert_eq!(
        png_dimensions(800., 800., RasterBudget::FILE),
        Ok((5000, 5000, 600))
    );
    assert!(png_dimensions(800., 800., RasterBudget::clipboard(false)).is_err());
}

#[test]
fn windows_clipboard_resolution_fits_dib_office_and_native_picture_limits() {
    let budget = RasterBudget::clipboard(true);
    assert_eq!(png_dimensions(100., 100., budget), Ok((1250, 1250, 1200)));
    // The old fixed 1200 DPI raster passes the export budget (50M pixels),
    // but its 24-bit DIB alone is 150 MB. The Office RGBA preview is larger.
    assert_eq!(
        png_dimensions(800., 400., RasterBudget::clipboard(false)),
        Ok((10000, 5000, 1200))
    );
    assert_eq!(png_dimensions(800., 400., budget), Ok((5000, 2500, 600)));
    for (width, height) in [(800., 400.), (800., 800.), (2000., 20.), (10000., 300.)] {
        let (w, h, _) = png_dimensions(width, height, budget).unwrap();
        let pixels = u64::from(w) * u64::from(h);
        let stride = (u64::from(w) * 3 + 3) & !3;
        assert!(pixels * 4 <= 64 * 1024 * 1024, "Office RGBA preview");
        assert!(40 + stride * u64::from(h) <= 64 * 1024 * 1024, "CF_DIB");
        assert!(pixels <= crate::pictures::MAX_PIXELS && w <= 8192 && h <= 8192);
    }
    assert!(png_dimensions(20000., 20000., budget).is_err());
}
#[test]
fn clipboard_is_transparent_and_files_keep_canvas_background() {
    use crate::{
        canvas_theme::CanvasTheme,
        document::{Annotation, Point},
        typography::TextFormat,
    };
    let mut format = TextFormat::default();
    // Custom colors are exact on both canvases.
    format.style.color = crate::palette::Color::Custom([180, 50, 55]);
    let mut doc = Document::default();
    doc.annotations.push(Annotation {
        id: 1,
        position: Point::default(),
        text: "O".into(),
        format,
    });
    for theme in CanvasTheme::ALL {
        doc.canvas_theme = theme;
        for clipboard in [false, true] {
            let bytes = if clipboard {
                clipboard_png(&doc).unwrap()
            } else {
                drawing(&doc, "png").unwrap()
            };
            let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
                .read_info()
                .unwrap();
            assert_eq!(reader.info().pixel_dims.unwrap().xppu, 47244);
            let mut pixels = vec![0; reader.output_buffer_size()];
            let frame = reader.next_frame(&mut pixels).unwrap();
            let pixels = &pixels[..frame.buffer_size()];
            if clipboard {
                assert_eq!(pixels[3], 0, "Clipboard surround must be transparent");
                assert!(
                    pixels
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .any(|p| p[3] > 0 && p[3] < 255)
                );
            } else {
                assert_eq!(&pixels[..3], &theme.background());
                assert!(pixels.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
            }
            assert!(
                pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|p| p[..3] == [180, 50, 55])
            );
        }
    }
}
#[test]
fn office_clipboard_svg_outlines_text_without_moving_or_resizing_it() {
    let doc: Document = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/ui-drawn-ethanol.reshiki"
    ))
    .unwrap();
    let svg = String::from_utf8(clipboard_svg(&doc).unwrap()).unwrap();
    let xml = roxmltree::Document::parse(&svg).unwrap();
    assert!(
        xml.root_element()
            .attribute("width")
            .unwrap()
            .ends_with("pt")
    );
    assert!(!xml.descendants().any(|node| node.has_tag_name("text")));
    let mut options = resvg::usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    let original = resvg::usvg::Tree::from_str(&scene::svg(&doc), &options).unwrap();
    // The receiving computer need not have the original fonts installed.
    let restored = resvg::usvg::Tree::from_str(&svg, &Default::default()).unwrap();
    assert!((original.size().width() - restored.size().width()).abs() < 0.001);
    assert!((original.size().height() - restored.size().height()).abs() < 0.001);
    let render = |tree: &resvg::usvg::Tree| {
        let mut pixels = resvg::tiny_skia::Pixmap::new(
            (original.size().width() * 3.).ceil() as u32,
            (original.size().height() * 3.).ceil() as u32,
        )
        .unwrap();
        resvg::render(
            tree,
            resvg::tiny_skia::Transform::from_scale(3., 3.),
            &mut pixels.as_mut(),
        );
        pixels
    };
    let expected = render(&original);
    let actual = render(&restored);
    let difference: u64 = expected
        .data()
        .iter()
        .zip(actual.data())
        .map(|(a, b)| u64::from(a.abs_diff(*b)))
        .sum();
    assert!(difference < expected.data().len() as u64 / 100);
}

#[test]
fn vector_pdf_and_png_contain_real_drawing_data() {
    let d: Document = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/ui-drawn-ethanol.reshiki"
    ))
    .unwrap();
    let pdf = drawing(&d, "pdf").unwrap();
    assert!(pdf.starts_with(b"%PDF-"));
    assert!(pdf.len() > 500);
    let png = drawing(&d, "png").unwrap();
    assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert!(png.len() > 1000);
    let reader = png::Decoder::new(std::io::Cursor::new(&png))
        .read_info()
        .unwrap();
    assert_eq!(reader.info().pixel_dims.unwrap().xppu, 47244);
}

#[test]
fn physical_scale_survives_svg_and_png_export() {
    use crate::document::Point;
    let mut d = Document::default();
    let a = d.add_atom("C", Point::default());
    let b = d.add_atom("C", Point::new(42.0, 0.0));
    d.add_bond(a, b, 1, "plain");
    let svg = scene::svg(&d);
    let tree = resvg::usvg::Tree::from_str(&svg, &Default::default()).unwrap();
    // A 14.4 pt bond plus a 4 pt border on each side, independent of screen zoom.
    assert!((tree.size().width() * 72.0 / 96.0 - 22.4).abs() < 0.001);
    let png = drawing(&d, "png").unwrap();
    let reader = png::Decoder::new(std::io::Cursor::new(&png))
        .read_info()
        .unwrap();
    assert_eq!(
        reader.info().width,
        (22.4_f32 / 72.0 * 1200.0).ceil() as u32
    );
}

fn parity_fixtures() -> [(&'static str, Document); 2] {
    [
        (
            "bond-join-regression",
            Document::from_json(include_bytes!(
                "../../../../tests/fixtures/bond-join-regression.rsk"
            ))
            .unwrap(),
        ),
        (
            "coordination-layout",
            Document::from_json(include_bytes!(
                "../../../../tests/fixtures/coordination-layout.rsk"
            ))
            .unwrap(),
        ),
    ]
}

/// Export fingerprints to compare before and after an export refactor. Bytes
/// depend on the installed fonts, so compare runs on the same machine only.
#[test]
#[ignore = "prints export fingerprints for a same-machine before/after comparison"]
fn export_parity_dump() {
    use std::hash::{DefaultHasher, Hash, Hasher};
    for (name, doc) in parity_fixtures() {
        for (clipboard, format) in [
            (false, "png"),
            (false, "svg"),
            (false, "pdf"),
            (true, "png"),
            (true, "svg"),
        ] {
            let kind = if clipboard {
                "clipboard_figure"
            } else {
                "figure"
            };
            let result = if clipboard {
                clipboard_figure(&doc, format)
            } else {
                figure(&doc, format)
            };
            match result {
                Ok(figure) => {
                    let mut hasher = DefaultHasher::new();
                    figure.bytes.hash(&mut hasher);
                    println!(
                        "{name} {kind}({format}): byte_len={} hash={:016x} detail={:?}",
                        figure.bytes.len(),
                        hasher.finish(),
                        figure.detail
                    );
                }
                Err(error) => println!("{name} {kind}({format}): error={error:?}"),
            }
        }
    }
}

#[cfg(not(windows))]
#[test]
fn clipboard_png_keeps_the_fixed_preferred_resolution() {
    let [(_, doc), _] = parity_fixtures();
    let detail = clipboard_figure(&doc, "png").unwrap().detail.unwrap();
    assert!(
        detail.ends_with(&format!("at {} dpi", crate::style::DEFAULT.png_dpi)),
        "{detail}"
    );
}

#[test]
fn figure_budgets_step_down_the_resolution_ladder_to_72_dpi() {
    let [(_, doc), _] = parity_fixtures();
    let tree = parse_svg(scene::svg_with_background(&doc)).unwrap();
    // png_dimensions scales CSS pixels by dpi / 96.
    let side = |length: f32| u64::from((length * 0.75).ceil() as u32);
    let at_72 = side(tree.size().width()) * side(tree.size().height());
    let file = figure(&doc, "png").unwrap();
    let full = figure_with_budget(&doc, "png", FILE_PIXELS).unwrap();
    assert_eq!((full.bytes, full.detail), (file.bytes, file.detail));
    let reduced = figure_with_budget(&doc, "png", at_72).unwrap();
    let detail = reduced.detail.unwrap();
    assert!(detail.ends_with("at 72 dpi"), "{detail}");
    assert!(!detail.ends_with(&format!("at {} dpi", crate::style::DEFAULT.png_dpi)));
    assert_eq!(
        figure_with_budget(&doc, "png", at_72 - 1).err().as_deref(),
        Some("Drawing is too large for a PNG even at 72 dpi; use SVG or PDF.")
    );
    // Vector formats have no pixel budget.
    assert!(figure_with_budget(&doc, "svg", 1).is_ok());
}
