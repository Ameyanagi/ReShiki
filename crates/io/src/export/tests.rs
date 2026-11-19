use super::*;

#[tokio::test]
async fn editable_png_contains_the_prepared_drawing_and_preserves_pixels() -> anyhow::Result<()> {
    use crate::{engine::LocalEngine, export};
    use anyhow::Context;

    let mut doc: Document = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/ui-drawn-ethanol.reshiki"
    ))?;
    for atom in &mut doc.atoms {
        atom.label_h = 0;
    }
    let engine = LocalEngine::in_process();
    let (prepared, notice) = export::figure_document(&engine, doc.clone())
        .await
        .map_err(anyhow::Error::msg)?;
    assert_eq!(notice, None);
    assert_ne!(
        prepared, doc,
        "The prepared snapshot refreshes hydrogen labels"
    );
    let plain = export::publication(
        &engine,
        doc.clone(),
        "png",
        false,
        false,
        Background::Canvas,
        crate::style::DEFAULT.png_dpi,
        export::FILE_PIXELS,
    )
    .await
    .map_err(anyhow::Error::msg)?;
    assert!(crate::payload::read(&plain.bytes).is_none());
    assert_eq!(plain.details.len(), 1);
    let editable = export::publication(
        &engine,
        doc,
        "png",
        false,
        true,
        Background::Canvas,
        crate::style::DEFAULT.png_dpi,
        export::FILE_PIXELS,
    )
    .await
    .map_err(anyhow::Error::msg)?;
    let embedded = crate::payload::read(&editable.bytes)
        .context("embedded drawing")?
        .map_err(anyhow::Error::msg)?;
    assert_eq!(embedded, crate::payload::visible(&prepared.current()));
    assert_eq!(
        editable.details,
        [
            plain.details[0].clone(),
            "Includes the editable drawing".into()
        ]
    );
    let mut decoded = Vec::new();
    for bytes in [&plain.bytes, &editable.bytes] {
        let mut reader = png::Decoder::new(std::io::Cursor::new(bytes)).read_info()?;
        let mut pixels = vec![0; reader.output_buffer_size()];
        let frame = reader.next_frame(&mut pixels)?;
        pixels.truncate(frame.buffer_size());
        decoded.push((frame.width, frame.height, pixels));
    }
    assert_eq!(decoded[0], decoded[1]);
    Ok(())
}

#[tokio::test]
async fn editable_png_preserves_custom_colors_from_older_dark_snapshots() {
    use crate::{
        canvas_theme::CanvasTheme,
        document::{Annotation, Point},
        palette::Color,
        typography::TextFormat,
    };

    for version in [15, 16] {
        let mut doc = Document {
            version,
            canvas_theme: CanvasTheme::Dark,
            ..Default::default()
        };
        let color = Color::Custom([49, 87, 123]);
        let mut format = TextFormat::default();
        format.style.color = color;
        doc.annotations.push(Annotation {
            id: 1,
            position: Point::default(),
            text: "O".into(),
            format,
        });
        let snapshot = doc.current();
        let exported = publication(
            &Default::default(),
            doc.clone(),
            "png",
            false,
            true,
            Background::Canvas,
            crate::style::DEFAULT.png_dpi,
            FILE_PIXELS,
        )
        .await
        .unwrap();
        let restored = crate::payload::read(&exported.bytes).unwrap().unwrap();
        assert_eq!(restored, snapshot);
        assert_eq!(restored.annotations[0].format.style.color, color);
        assert_eq!(
            crate::scene::svg_with_background(&restored),
            crate::scene::svg_with_background(&snapshot)
        );
        assert_eq!(
            crate::scene::svg_with_background(&snapshot),
            crate::scene::svg_with_background(&doc)
        );
        assert_eq!(doc.version, version);
    }
}

#[tokio::test]
async fn editable_figure_export_skips_a_drawing_over_the_embed_limit() {
    let mut doc = Document::default();
    // A detailed picture retains more editable data than its small printed size.
    let mut state = 0x1234_5678_u32;
    let pixels: Vec<u8> = (0..1024 * 1024 * 3)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as u8
        })
        .collect();
    let mut source = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut source, 1024, 1024);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&pixels)
            .unwrap();
    }
    let picture = crate::pictures::Picture::import(&source).unwrap();
    let mut graphic = picture.graphic(1, Default::default());
    crate::pictures::resize(&mut graphic, 12., 12.).unwrap();
    doc.graphics.push(graphic);
    assert!(crate::payload::encode(&doc).unwrap().len() > crate::payload::EMBED_LIMIT);
    for format in ["png", "svg", "svg-office", "pdf"] {
        let export = publication(
            &Default::default(),
            doc.clone(),
            format,
            false,
            true,
            Background::Canvas,
            crate::style::DEFAULT.png_dpi,
            FILE_PIXELS,
        )
        .await
        .unwrap();
        assert!(crate::payload::read(&export.bytes).is_none(), "{format}");
        assert_eq!(
            export.details.last().map(String::as_str),
            Some("Saved without the editable drawing: it is larger than 2 MB"),
            "{format}",
        );
        if format == "png" {
            let mut reader = png::Decoder::new(std::io::Cursor::new(export.bytes))
                .read_info()
                .unwrap();
            let mut pixels = vec![0; reader.output_buffer_size()];
            reader.next_frame(&mut pixels).unwrap();
        }
    }
}

#[test]
fn steroid_alpha_beta_figure_exports_share_the_canvas_labels() {
    use crate::document::Point;

    for bytes in [
        include_bytes!("../../../model/fonts/LiberationSans-Regular.ttf").as_slice(),
        include_bytes!("../../../model/fonts/LiberationSans-Italic.ttf").as_slice(),
    ] {
        crate::style::add_font(bytes.to_vec()).unwrap();
    }
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::default());
    let b = doc.add_atom("O", Point::new(42., 0.));
    doc.add_bond(a, b, 1, "plain");
    doc.atom_labels.stereo = true;
    let atom = doc.atom_mut(a).unwrap();
    atom.cip_label = Some("S".into());
    atom.steroid_label = Some("3β".into());
    atom.display.stereo.style.family = "Liberation Sans".into();
    doc.drawing_style.font_family = "Liberation Sans".into();
    for roman in [false, true] {
        doc.atom_labels.roman_stereo_parentheses = roman;
        doc.atom_labels.steroid_alpha_beta = false;
        let cip_png = drawing(&doc, "png").unwrap();
        let cip_pdf = drawing(&doc, "pdf").unwrap();
        doc.atom_labels.steroid_alpha_beta = true;
        let svg = String::from_utf8(drawing(&doc, "svg").unwrap()).unwrap();
        assert!(svg.contains("3β"), "{svg}");
        assert!(!svg.contains("(3β)"));
        let png = drawing(&doc, "png").unwrap();
        let pdf = drawing(&doc, "pdf").unwrap();
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert!(pdf.starts_with(b"%PDF-"));
        assert_ne!(png, cip_png);
        assert_ne!(pdf, cip_pdf);
        assert_eq!(doc.atom(a).unwrap().cip_label.as_deref(), Some("S"));
    }
}

#[test]
fn missing_named_family_keeps_text_in_svg_tree_and_raster_export() {
    let mut db = resvg::usvg::fontdb::Database::new();
    for bytes in [
        include_bytes!("../../../model/fonts/LiberationSans-Regular.ttf").as_slice(),
        include_bytes!("../../../model/fonts/LiberationSans-Bold.ttf").as_slice(),
        include_bytes!("../../../model/fonts/LiberationSans-Italic.ttf").as_slice(),
        include_bytes!("../../../model/fonts/LiberationSans-BoldItalic.ttf").as_slice(),
    ] {
        db.load_font_data(bytes.to_vec());
    }
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64">
        <text x="8" y="48" font-family="Arial" font-size="40">O</text>
    </svg>"#;
    let db = std::sync::Arc::new(db);
    let default_tree = resvg::usvg::Tree::from_str(
        svg,
        &resvg::usvg::Options {
            fontdb: db.clone(),
            ..Default::default()
        },
    )
    .unwrap();
    // With neither Arial nor the generic serif installed, usvg drops the span.
    assert!(default_tree.root().children().is_empty());
    let tree = parse_svg_with_fontdb(svg, db).unwrap();
    let bounds = tree.root().bounding_box();
    assert!(bounds.width() > 0. && bounds.height() > 0.);
    assert!(
        tree.root()
            .children()
            .iter()
            .any(|node| matches!(node, resvg::usvg::Node::Text(_)))
    );
    let outlined = String::from_utf8(office_svg(&tree).unwrap()).unwrap();
    let outlined = roxmltree::Document::parse(&outlined).unwrap();
    assert!(!outlined.descendants().any(|node| node.has_tag_name("text")));
    assert!(outlined.descendants().any(|node| node.has_tag_name("path")));
    let mut pixmap = resvg::tiny_skia::Pixmap::new(64, 64).unwrap();
    pixmap.fill(resvg::tiny_skia::Color::WHITE);
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    assert!(
        pixmap
            .pixels()
            .iter()
            .any(|p| p.red() < 128 && p.alpha() > 0)
    );
}

#[test]
fn bundled_only_font_database_outlines_arial_labels() {
    let mut db = resvg::usvg::fontdb::Database::new();
    crate::style::load_bundled_fonts(&mut db);
    let tree = parse_svg_with_fontdb(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64">
            <text x="8" y="48" font-family="Arial" font-size="40">O</text>
        </svg>"#,
        std::sync::Arc::new(db),
    )
    .unwrap();
    let outlined = String::from_utf8(office_svg(&tree).unwrap()).unwrap();
    let xml = roxmltree::Document::parse(&outlined).unwrap();
    assert!(!xml.descendants().any(|node| node.has_tag_name("text")));
    assert!(xml.descendants().any(|node| node.has_tag_name("path")));
}

#[test]
fn raster_budget_rejects_invalid_and_extreme_sizes_without_allocating() {
    for (width, height) in [
        (0., 10.),
        (10., -1.),
        (f32::NAN, 10.),
        (10., f32::INFINITY),
        (f32::MAX, f32::MAX),
    ] {
        assert!(png_dimensions(width, height, RasterBudget::file(1200, FILE_PIXELS)).is_err());
    }
    assert_eq!(
        png_dimensions(100., 100., RasterBudget::file(1200, FILE_PIXELS)),
        Ok((1250, 1250, 1200))
    );
    assert_eq!(
        png_dimensions(800., 800., RasterBudget::file(1200, FILE_PIXELS)),
        Ok((5000, 5000, 600))
    );
    assert!(png_dimensions(800., 800., RasterBudget::clipboard(false, 1200)).is_err());
}

#[test]
fn png_sides_are_capped_whatever_the_pixel_count() {
    // One long line: 2.6 million pixels at 72 dpi, but 43,688 of them wide.
    assert_eq!(
        png_dimensions(58_250., 80., RasterBudget::file(1200, FILE_PIXELS))
            .err()
            .as_deref(),
        Some("Drawing is too large for a PNG even at 72 dpi; use SVG or PDF.")
    );
    // The widest PNG, and one that steps down a resolution to stay within it.
    assert_eq!(
        png_dimensions(43_689., 80., RasterBudget::file(1200, FILE_PIXELS)),
        Ok((FILE_SIDE, 60, 72))
    );
    assert_eq!(
        png_dimensions(3_000., 80., RasterBudget::file(1200, FILE_PIXELS)),
        Ok((18_750, 500, 600))
    );
}

#[test]
fn an_oversized_png_is_refused_before_the_scene_is_written() {
    use crate::document::Point;
    let written = || SCENES.with(std::cell::Cell::get);
    let mut atoms = Document::default();
    atoms.add_atom("C", Point::default());
    atoms.add_atom("N", Point::new(200_000., 200_000.));
    // One line of text, far shorter than a text box holds, beside an atom.
    let mut text = Document::default();
    text.add_atom("C", Point::default());
    text.annotations.push(crate::document::Annotation {
        id: text.next_id(),
        position: Point::default(),
        text: "x".repeat(10_000),
        format: Default::default(),
    });
    let mut text_alone = text.clone();
    text_alone.atoms.clear();
    let mut graphic = Document::default();
    graphic.graphics.push(crate::graphics::Graphic::dragged(
        1,
        crate::graphics::GraphicKind::Rectangle,
        Point::default(),
        Point::new(200_000., 200_000.),
        Default::default(),
        Default::default(),
        false,
    ));
    for doc in [&atoms, &text, &text_alone, &graphic] {
        doc.validate().unwrap();
        let before = written();
        for result in [
            figure(doc, "png"),
            figure_with_budget(doc, "png", 1_000),
            clipboard_figure(doc, "png"),
        ] {
            let error = result.err().unwrap();
            assert!(error.starts_with("Drawing is too large for a"), "{error}");
        }
        assert_eq!(written(), before);
    }
    // Vector formats have no pixel budget.
    let before = written();
    assert!(figure(&atoms, "svg").is_ok());
    assert_eq!(written(), before + 1);
}

#[test]
fn outlined_svg_is_never_smaller_than_its_floor() {
    let doc = Document::from_json(include_bytes!(
        "../../../../assets/examples/shortcut-examples.rsk"
    ))
    .unwrap();
    let svg = scene::svg_of(
        &doc,
        scene::vector_primitives(&doc),
        Some(doc.canvas_theme.background()),
    );
    let tree = parse_svg(svg).unwrap();
    let (floor, outlined) = (
        outlined_floor(tree.root()),
        office_svg(&tree).unwrap().len(),
    );
    // Close enough to refuse a figure of twice the limit.
    assert!(
        outlined / 2 < floor && floor <= outlined,
        "{floor} {outlined}"
    );
    let limit = |max_bytes| {
        render_drawing(
            &doc,
            "svg-office",
            false,
            Background::Canvas,
            RasterBudget::file(1200, FILE_PIXELS),
            max_bytes,
            None,
        )
        .map(|figure| figure.bytes.len())
    };
    assert_eq!(limit(floor - 1), Err(OVER_BYTES.into()));
    // A limit the floor fits is left to the caller, which has the exact size.
    assert_eq!(limit(floor), Ok(outlined));
    // Live text is the scene as written: there is nothing to save.
    assert!(
        render_drawing(
            &doc,
            "svg",
            false,
            Background::Canvas,
            RasterBudget::file(1200, FILE_PIXELS),
            1,
            None,
        )
        .is_ok()
    );
}

#[test]
fn windows_clipboard_resolution_fits_dib_office_and_native_picture_limits() {
    let budget = RasterBudget::clipboard(true, 1200);
    assert_eq!(png_dimensions(100., 100., budget), Ok((1250, 1250, 1200)));
    // The old fixed 1200 DPI raster passes the export budget (50M pixels),
    // but its 24-bit DIB alone is 150 MB. The Office RGBA preview is larger.
    assert_eq!(
        png_dimensions(800., 400., RasterBudget::clipboard(false, 1200)),
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

/// A dark-canvas drawing: a bond and an O label in Ink, and a custom red label.
fn dark_ink_drawing() -> Document {
    use crate::{
        canvas_theme::CanvasTheme,
        document::{Annotation, Point},
        typography::TextFormat,
    };
    let mut doc = Document {
        canvas_theme: CanvasTheme::Dark,
        ..Default::default()
    };
    let a = doc.add_atom("C", Point::default());
    let b = doc.add_atom("O", Point::new(40., 0.));
    doc.add_bond(a, b, 1, "plain");
    let mut format = TextFormat::default();
    format.style.color = crate::palette::Color::Custom([180, 50, 55]);
    doc.annotations.push(Annotation {
        id: doc.next_id(),
        position: Point::new(0., 40.),
        text: "N".into(),
        format,
    });
    doc
}

fn rgba(png: &[u8]) -> Vec<[u8; 4]> {
    let mut reader = png::Decoder::new(std::io::Cursor::new(png))
        .read_info()
        .unwrap();
    let mut pixels = vec![0; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut pixels).unwrap();
    pixels[..frame.buffer_size()].as_chunks::<4>().0.to_vec()
}

#[test]
fn picture_backgrounds_paint_the_png_and_swap_black_and_white_ink() {
    let doc = dark_ink_drawing();
    let navy = [30, 42, 68];
    let paper = [244, 239, 230];
    // Background, corner pixel, and the ink that must appear.
    let cases = [
        (Background::Transparent, [0, 0, 0, 0], [255, 255, 255]),
        (Background::Canvas, [0, 0, 0, 255], [255, 255, 255]),
        (Background::Color([255; 3]), [255, 255, 255, 255], [0, 0, 0]),
        (Background::Color([0; 3]), [0, 0, 0, 255], [255, 255, 255]),
        (Background::Color(paper), [244, 239, 230, 255], [0, 0, 0]),
        (Background::Color(navy), [30, 42, 68, 255], [255, 255, 255]),
    ];
    for (background, corner, ink) in cases {
        for clipboard in [true, false] {
            let bytes = if clipboard {
                clipboard_figure_with_drawing(
                    &doc,
                    "png",
                    background,
                    crate::style::DEFAULT.png_dpi,
                    None,
                )
            } else {
                figure_with_budget_and_drawing(
                    &doc,
                    "png",
                    background,
                    RasterBudget::file(1200, FILE_PIXELS),
                    None,
                )
            }
            .unwrap()
            .bytes;
            let pixels = rgba(&bytes);
            assert_eq!(pixels[0], corner, "{background:?}");
            assert_eq!(pixels[pixels.len() - 1], corner, "{background:?}");
            assert!(
                pixels.iter().any(|p| p[..3] == ink && p[3] == 255),
                "{background:?} keeps readable ink"
            );
            let other = ink.map(|c| 255 - c);
            assert!(
                !pixels
                    .iter()
                    .any(|p| p[..3] == other && p[3] == 255 && *p != corner),
                "{background:?} has no ink of the other brightness"
            );
            // The custom color stays exact on every background.
            assert!(pixels.iter().any(|p| p[..3] == [180, 50, 55]));
        }
    }
}

/// Office rebuilds the preview of an edited drawing on Copy's background.
#[cfg(windows)]
#[test]
fn office_preview_rebuild_keeps_the_copy_background() {
    let doc = dark_ink_drawing();
    let white = office_preview(
        &doc,
        Background::Color([255; 3]),
        crate::style::DEFAULT.png_dpi,
    )
    .unwrap();
    let pixels = rgba(&white.png);
    assert_eq!(pixels[0], [255; 4]);
    assert!(pixels.contains(&[0, 0, 0, 255]), "dark ink");
    let transparent =
        office_preview(&doc, Background::Transparent, crate::style::DEFAULT.png_dpi).unwrap();
    assert_eq!(rgba(&transparent.png)[0], [0; 4]);
    assert_ne!(white.metafile, transparent.metafile);
}

#[test]
fn picture_backgrounds_keep_palette_and_element_colors() {
    use crate::{
        canvas_theme::{CanvasTheme, ColorTheme, element_color},
        document::{Annotation, Point},
        palette::{Color, Hue, Palette, Row},
        typography::TextFormat,
    };
    for canvas in CanvasTheme::ALL {
        let mut doc = Document {
            canvas_theme: canvas,
            color_theme: ColorTheme::Jmol,
            ..Default::default()
        };
        doc.add_atom("N", Point::default());
        let blue = Color::Palette(Hue::Blue, Row::Strong);
        let mut format = TextFormat::default();
        format.style.color = blue;
        doc.annotations.push(Annotation {
            id: doc.next_id(),
            position: Point::new(0., 40.),
            text: "M".into(),
            format,
        });
        // The colors as the drawing's own canvas shows them.
        let expected = [
            Palette::of(&doc).rgb(blue),
            element_color(&doc, "N", canvas),
        ];
        let paper = if canvas.is_light() { [0; 3] } else { [255; 3] };
        for clipboard in [true, false] {
            let background = Background::Color(paper);
            let bytes = if clipboard {
                clipboard_figure_with_drawing(
                    &doc,
                    "png",
                    background,
                    crate::style::DEFAULT.png_dpi,
                    None,
                )
            } else {
                figure_with_budget_and_drawing(
                    &doc,
                    "png",
                    background,
                    RasterBudget::file(1200, FILE_PIXELS),
                    None,
                )
            }
            .unwrap()
            .bytes;
            let pixels = rgba(&bytes);
            for rgb in expected {
                assert!(
                    pixels.iter().any(|p| p[..3] == rgb && p[3] == 255),
                    "{canvas} drawing on {paper:?} keeps {rgb:?}"
                );
            }
        }
    }
}

/// A 3D model keeps the drawing's theme, a user theme too, when a color
/// background flips the canvas: its balls match the same drawing on that canvas.
#[test]
fn picture_backgrounds_keep_the_theme_of_3d_models() {
    use crate::{
        canvas_theme::{CanvasTheme, ColorTheme},
        models3d::{Display, ForceField, Model3d, Placed, set_display},
    };
    let mut doc = Document {
        canvas_theme: CanvasTheme::Dark,
        color_theme: ColorTheme::Jmol,
        ..Default::default()
    };
    let c = doc.add_atom("C", crate::document::Point::new(0., 0.));
    let o = doc.add_atom("O", crate::document::Point::new(30., 0.));
    doc.add_bond(c, o, 2, "plain");
    let atoms = [(c, [-0.6, 0., -0.2]), (o, [0.6, 0., 0.2])]
        .map(|(id, xyz)| Placed { id, xyz })
        .to_vec();
    doc.models
        .push(Model3d::new(&doc, atoms, vec![], ForceField::Mmff94s));
    crate::models3d::require_version(&mut doc);
    assert!(set_display(&mut doc, 0, Display::ThreeD));
    crate::palette::set_hues(&mut doc, crate::palette::Hues::default());
    assert!(doc.custom_theme.is_some());
    let gradients = |doc: &Document, background| {
        let svg = String::from_utf8(
            figure_with_budget_and_drawing(
                doc,
                "svg",
                background,
                RasterBudget::file(1200, FILE_PIXELS),
                None,
            )
            .unwrap()
            .bytes,
        )
        .unwrap();
        svg.split("<radialGradient")
            .skip(1)
            .map(|g| {
                g.split_once('>')
                    .unwrap()
                    .1
                    .split("</radialGradient")
                    .next()
                    .unwrap()
                    .to_owned()
            })
            .collect::<Vec<_>>()
    };
    let white = gradients(&doc, Background::Color([255; 3]));
    assert!(!white.is_empty());
    let light = Document {
        canvas_theme: CanvasTheme::Light,
        ..doc.clone()
    };
    assert_eq!(white, gradients(&light, Background::Canvas));
    let publication = Document {
        color_theme: ColorTheme::Publication,
        custom_theme: None,
        ..light
    };
    assert_ne!(white, gradients(&publication, Background::Canvas));
}

#[test]
fn picture_backgrounds_reach_svg_paper_and_ink() {
    let doc = dark_ink_drawing();
    let svg = |background| {
        String::from_utf8(
            figure_with_budget_and_drawing(
                &doc,
                "svg",
                background,
                RasterBudget::file(1200, FILE_PIXELS),
                None,
            )
            .unwrap()
            .bytes,
        )
        .unwrap()
    };
    let white = svg(Background::Color([255; 3]));
    assert!(white.contains("<rect") && white.contains("fill=\"rgb(255,255,255)\""));
    assert!(white.contains("rgb(0,0,0)"), "dark ink on white");
    let canvas = svg(Background::Canvas);
    assert!(canvas.contains("fill=\"rgb(0,0,0)\"/>") && canvas.contains("rgb(255,255,255)"));
    assert!(!svg(Background::Transparent).contains("<rect"));
    assert_eq!(
        svg(Background::Canvas),
        String::from_utf8(drawing(&doc, "svg").unwrap()).unwrap(),
        "file export keeps the canvas by default"
    );
    assert_eq!(
        svg(Background::Transparent),
        scene::svg(&doc),
        "transparent keeps the canvas ink, as Copy did"
    );
}
#[test]
fn outlined_svg_and_office_exports_preserve_text_geometry_and_physical_size() {
    let doc: Document = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/ui-drawn-ethanol.reshiki"
    ))
    .unwrap();
    for theme in crate::canvas_theme::CanvasTheme::ALL {
        let mut doc = doc.clone();
        doc.canvas_theme = theme;
        let vector = |paper| scene::svg_of(&doc, scene::vector_primitives(&doc), paper);
        assert_eq!(
            drawing(&doc, "svg").unwrap(),
            vector(Some(theme.background())).into_bytes()
        );
        for clipboard in [false, true] {
            let source = vector((!clipboard).then(|| theme.background()));
            let bytes = if clipboard {
                clipboard_figure(&doc, "svg-office").unwrap().bytes
            } else {
                drawing(&doc, "svg-office").unwrap()
            };
            assert_eq!(outlined_svg(source.clone()).unwrap(), bytes);
            let svg = String::from_utf8(bytes).unwrap();
            let xml = roxmltree::Document::parse(&svg).unwrap();
            assert!(
                xml.root_element()
                    .attribute("width")
                    .unwrap()
                    .ends_with("pt")
            );
            assert!(!xml.descendants().any(|node| node.has_tag_name("text")));
            assert!(xml.descendants().any(|node| node.has_tag_name("path")));
            let original = parse_svg(source).unwrap();
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
    }
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

/// The resolution and physical size a PNG's pHYs chunk records.
fn png_physical(png: &[u8]) -> (u32, f64, f64) {
    let reader = png::Decoder::new(std::io::Cursor::new(png))
        .read_info()
        .unwrap();
    let info = reader.info();
    let dims = info.pixel_dims.unwrap();
    assert_eq!(dims.unit, png::Unit::Meter);
    assert_eq!(dims.xppu, dims.yppu);
    let per_inch = f64::from(dims.xppu) * 0.0254;
    (
        per_inch.round() as u32,
        f64::from(info.width) / per_inch,
        f64::from(info.height) / per_inch,
    )
}

#[tokio::test]
async fn file_png_uses_the_chosen_resolution_at_the_same_physical_size() {
    let [(_, doc), _] = parity_fixtures();
    let engine = crate::engine::LocalEngine::in_process();
    let mut sizes = Vec::new();
    for dpi in [150, 300, 600, 1200] {
        let png = publication(
            &engine,
            doc.clone(),
            "png",
            false,
            false,
            Background::Canvas,
            dpi,
            FILE_PIXELS,
        )
        .await
        .unwrap();
        let (recorded, width, height) = png_physical(&png.bytes);
        assert_eq!(recorded, dpi);
        assert!(
            png.details[0].ends_with(&format!("at {dpi} dpi")),
            "{:?}",
            png.details
        );
        sizes.push((dpi, width, height));
    }
    let (_, width, height) = sizes[3];
    for (dpi, w, h) in sizes {
        // Whole pixels round the size up by less than one pixel.
        let pixel = 1. / f64::from(dpi);
        assert!(
            (w - width).abs() <= pixel && (h - height).abs() <= pixel,
            "{dpi} dpi"
        );
    }
    let at = |dpi| {
        let size = parse_svg(scene::svg_with_background(&doc)).unwrap().size();
        png_pixels((size.width(), size.height()), dpi)
    };
    let (w300, _) = at(300);
    let (w1200, _) = at(1200);
    assert!(
        w1200 >= 4 * w300 - 4 && w1200 <= 4 * w300,
        "{w300} and {w1200}"
    );
}

#[test]
fn a_lower_chosen_resolution_steps_down_from_itself_and_errors_name_it() {
    // 4000 × 4000 CSS pixels: 125 million pixels at 300 dpi, 39 million at 150.
    assert_eq!(
        png_dimensions(4000., 4000., RasterBudget::file(300, FILE_PIXELS)),
        Ok((6250, 6250, 150))
    );
    for dpi in [300, 600, 1200] {
        assert_eq!(
            png_dimensions(4000., 4000., RasterBudget::clipboard(false, dpi)),
            Err(format!(
                "Drawing is too large for a {dpi} dpi PNG; use SVG or PDF."
            ))
        );
    }
    // A drawing that fits keeps the chosen resolution, never a higher one.
    assert_eq!(
        png_dimensions(96., 48., RasterBudget::file(150, FILE_PIXELS)),
        Ok((150, 75, 150))
    );
    assert_eq!(
        png_dimensions(96., 48., RasterBudget::clipboard(true, 300)),
        Ok((300, 150, 300))
    );
}

#[test]
fn close_ups_are_the_exported_pixels_at_each_resolution() {
    let [(_, doc), _] = parity_fixtures();
    let (lo, hi) = scene::bounds(&scene::primitives(&doc));
    let middle = |a: &crate::document::Atom| {
        (a.position.x - (lo.x + hi.x) / 2.).hypot(a.position.y - (lo.y + hi.y) / 2.)
    };
    let focus = doc
        .atoms
        .iter()
        .min_by(|a, b| middle(a).total_cmp(&middle(b)))
        .unwrap()
        .position;
    let dpis = [150, 300, 600, 1200];
    let close_ups = png_close_ups(&doc, Background::Canvas, focus, 12., &dpis).unwrap();
    let tree = parse_svg(scene::svg_with_background(&doc)).unwrap();
    assert_eq!(close_ups.size, (tree.size().width(), tree.size().height()));
    let sides: Vec<_> = close_ups.pixels.iter().map(|(side, _)| *side).collect();
    // 12 points is 1/6 inch.
    assert_eq!(sides, [25, 50, 100, 200]);
    for ((side, pixels), dpi) in close_ups.pixels.iter().zip(dpis) {
        assert_eq!(pixels.len(), (side * side * 4) as usize);
        // The close-up is a piece of the file export at the same resolution.
        let file = figure_with_budget_and_drawing(
            &doc,
            "png",
            Background::Canvas,
            RasterBudget::file(dpi, FILE_PIXELS),
            None,
        )
        .unwrap();
        let reader = png::Decoder::new(std::io::Cursor::new(&file.bytes))
            .read_info()
            .unwrap();
        let width = reader.info().width as usize;
        let whole = rgba(&file.bytes);
        let close: Vec<[u8; 4]> = pixels.as_chunks::<4>().0.to_vec();
        let side = *side as usize;
        // The close-up's corner, 6 points up and left of the focus.
        let inch = |v: f32| v * crate::style::DEFAULT.points_per_world() / 72.;
        let corner = |at: f32, lo: f32| ((inch(at - lo) - 6. / 72.) * dpi as f32).round() as usize;
        let (x, y) = (corner(focus.x, lo.x), corner(focus.y, lo.y));
        for row in 0..side {
            let start = (y + row) * width + x;
            for (a, b) in whole[start..start + side]
                .iter()
                .zip(&close[row * side..(row + 1) * side])
            {
                // Antialiasing may differ by one coverage step (16 of 255)
                // where the picture starts at a different pixel.
                assert!(
                    a.iter().zip(b).all(|(a, b)| a.abs_diff(*b) <= 16),
                    "{dpi} dpi close-up differs from the export: {a:?} and {b:?}"
                );
            }
        }
        assert!(
            close.iter().any(|p| p != &close[0]),
            "{dpi} dpi close-up shows ink"
        );
    }
}

#[test]
fn png_fits_sizes_a_scene_to_the_pixel_as_the_parsed_svg_does() {
    use crate::document::Point;
    // At 1200 dpi this is 10,822 x 7,392 pixels, just under 80 million. Scaling
    // in another order than the SVG parser rounds the width up to 10,823.
    let (lo, hi) = (Point::new(0., 0.), Point::new(1893.8502, 1293.43));
    let budget = RasterBudget::clipboard(false, 1200);
    let points = crate::style::DEFAULT.points_per_world();
    let tree = parse_svg(format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 1 1\" width=\"{}pt\" height=\"{}pt\"/>",
        hi.x * points,
        hi.y * points
    ))
    .unwrap();
    assert_eq!(
        png_dimensions(tree.size().width(), tree.size().height(), budget),
        Ok((10_822, 7_392, crate::style::DEFAULT.png_dpi))
    );
    assert_eq!(png_fits((lo, hi), budget), Ok(()));
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

#[test]
fn mechanism_attachment_92_figures_retain_resolved_curve_and_report_link_loss() {
    use crate::{
        arrow_anchors::{self, Pick},
        arrows::{ArrowStyle, Preset},
        document::Point,
    };
    let mut d = Document::from_native_file(include_bytes!(
        "../../../../tests/fixtures/mechanism-attachments-92/before.rsk"
    ))
    .unwrap();
    let source = arrow_anchors::pick(&d, Point::new(0., -29.166668), 2.).unwrap();
    arrow_anchors::create(
        &mut d,
        &source,
        &Pick::Atom(1),
        Preset::Curved,
        ArrowStyle::preset(Preset::Curved),
    )
    .unwrap();
    let mut free = d.clone();
    for a in &mut free.arrows {
        a.start_anchor = None;
        a.end_anchor = None;
    }
    for format in ["svg", "png", "pdf"] {
        let attached = figure(&d, format).unwrap();
        let detached = figure(&free, format).unwrap();
        assert_eq!(attached.bytes, detached.bytes, "{format} resolved geometry");
        assert!(
            attached
                .detail
                .unwrap()
                .contains(arrow_anchors::EXPORT_NOTICE)
        );
        assert!(!detached.detail.unwrap_or_default().contains("attachment"));
    }
    let (_, notices) = crate::exchange::drawing::write_clipboard(&d).unwrap();
    assert!(notices.iter().any(|n| n == arrow_anchors::EXPORT_NOTICE));
}

#[test]
fn transparent_label_halo_reaches_vector_readers_as_a_named_knockout_mask() {
    use crate::{
        document::Point,
        graphics::{GraphicKind, GraphicStyle},
        scientific::{Drawing, OrbitalKind, Phase},
    };
    let mut doc = Document::default();
    doc.add_atom("O", Point::default());
    Drawing {
        kind: GraphicKind::Orbital(OrbitalKind::P),
        style: GraphicStyle::default(),
        phase: Phase::Solid,
        flipped: false,
        attach: false,
        snap_orbitals: true,
    }
    .place(&mut doc, Point::default(), Point::new(0., -42.), false, 10.)
    .unwrap();
    // The Windows metafile writer paints this named mask content in paper.
    fn halo(group: &resvg::usvg::Group) -> bool {
        group.mask().is_some_and(|mask| {
            mask.root().children().iter().any(|node| {
                matches!(node, resvg::usvg::Node::Group(ink)
                    if ink.id().starts_with("reshiki-label-halo") && ink.has_children())
            })
        }) || group.children().iter().any(|node| match node {
            resvg::usvg::Node::Group(group) => halo(group),
            _ => false,
        })
    }
    assert!(halo(parse_svg(scene::svg(&doc)).unwrap().root()));
    assert!(!halo(
        parse_svg(scene::svg_with_background(&doc)).unwrap().root()
    ));
    let pdf = clipboard_drawing(&doc, "pdf").unwrap();
    assert!(
        pdf.windows(6).any(|window| window == b"/SMask"),
        "A transparent PDF knocks the halo out with a soft mask"
    );
}

fn vector_payload_document() -> Document {
    let mut doc = Document::default();
    doc.annotations.push(crate::document::Annotation {
        id: 1,
        position: Default::default(),
        text: "Editable 日本語".into(),
        format: Default::default(),
    });
    doc
}

fn pdf_media_box(bytes: &[u8]) -> Vec<f32> {
    let text = String::from_utf8_lossy(bytes);
    text.split_once("/MediaBox [")
        .unwrap()
        .1
        .split_once(']')
        .unwrap()
        .0
        .split_whitespace()
        .map(|number| number.parse().unwrap())
        .collect()
}

fn pdf_text_object(bytes: &[u8], id: u32) -> &str {
    let marker = format!("\n{id} 0 obj\n");
    let start = bytes
        .windows(marker.len())
        .position(|window| window == marker.as_bytes())
        .unwrap()
        + marker.len();
    let rest = &bytes[start..];
    let end = rest
        .windows(6)
        .position(|window| window == b"endobj")
        .unwrap();
    std::str::from_utf8(&rest[..end]).unwrap()
}

fn assert_pdf_xref(bytes: &[u8]) {
    let marker = b"startxref\n";
    let start = bytes
        .windows(marker.len())
        .rposition(|window| window == marker)
        .unwrap()
        + marker.len();
    let offset: usize = std::str::from_utf8(&bytes[start..])
        .unwrap()
        .lines()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    let xref = std::str::from_utf8(&bytes[offset..]).unwrap();
    let mut lines = xref.lines();
    assert_eq!(lines.next(), Some("xref"));
    let header = lines.next().unwrap().split_whitespace().collect::<Vec<_>>();
    assert_eq!(header[0], "0");
    let count: usize = header[1].parse().unwrap();
    for (id, line) in lines.take(count).enumerate() {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields[2] == "n" {
            let offset: usize = fields[0].parse().unwrap();
            let object = format!("{id} {} obj", fields[1].parse::<u16>().unwrap());
            assert!(
                bytes[offset..].starts_with(object.as_bytes()),
                "xref object {id}"
            );
        }
    }
}

#[test]
fn single_pdf_preserves_the_old_physical_size_and_scales_the_xobject() {
    let doc = vector_payload_document();
    let tree = parse_svg(scene::svg_with_background(&doc)).unwrap();
    let old = svg2pdf::to_pdf(
        &tree,
        svg2pdf::ConversionOptions::default(),
        svg2pdf::PageOptions { dpi: 96. },
    )
    .unwrap();
    let new = drawing(&doc, "pdf").unwrap();
    let envelope = crate::payload::encode(&doc).unwrap();
    let editable = single_pdf(&tree, Some(&envelope)).unwrap();
    let old_box = pdf_media_box(&old);
    assert_eq!(old_box.len(), 4);
    for bytes in [&new, &editable] {
        assert!(pdf_text_object(bytes, 2).contains("/Count 1"));
        assert_eq!(
            String::from_utf8_lossy(bytes)
                .matches("/MediaBox [")
                .count(),
            1
        );
        let new_box = pdf_media_box(bytes);
        assert!(
            String::from_utf8_lossy(bytes)
                .contains("/Group <<\n    /Type /Group\n    /S /Transparency")
        );
        assert_eq!(new_box.len(), 4);
        for (old, new) in old_box.iter().zip(&new_box) {
            assert!((old - new).abs() < 0.01, "old {old}, new {new}");
        }
    }
    let content = pdf_text_object(&new, 4);
    let transform = content.lines().find(|line| line.ends_with(" cm")).unwrap();
    let transform = transform
        .strip_suffix(" cm")
        .unwrap()
        .split_whitespace()
        .map(|number| number.parse::<f32>().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(transform.len(), 6);
    assert!((transform[0] - tree.size().width() * 0.75).abs() < 0.01);
    assert!((transform[3] - tree.size().height() * 0.75).abs() < 0.01);
    assert_eq!(
        [transform[1], transform[2], transform[4], transform[5]],
        [0.; 4]
    );
    assert!(content.contains("/Drawing Do"));
    assert!(!pdf_text_object(&new, 1).contains("/Metadata"));
    assert_pdf_xref(&new);
}

#[test]
fn editable_pdf_has_catalog_xmp_without_compression_and_valid_cross_references() {
    use std::io::Write;

    let doc = vector_payload_document();
    let envelope = crate::payload::encode(&doc).unwrap();
    let bytes = clipboard_figure_with_drawing(
        &doc,
        "pdf",
        Background::Transparent,
        crate::style::DEFAULT.png_dpi,
        Some(&envelope),
    )
    .unwrap()
    .bytes;
    let catalog = pdf_text_object(&bytes, 1);
    let metadata_id = catalog
        .split_once("/Metadata ")
        .unwrap()
        .1
        .split_whitespace()
        .next()
        .unwrap()
        .parse::<u32>()
        .unwrap();
    let metadata = pdf_text_object(&bytes, metadata_id);
    let (dictionary, stream) = metadata.split_once("stream\n").unwrap();
    assert!(dictionary.contains("/Type /Metadata"));
    assert!(dictionary.contains("/Subtype /XML"));
    assert!(!dictionary.contains("/Filter"));
    let packet = stream.split_once("\nendstream").unwrap().0;
    assert_eq!(packet, crate::payload::xmp(&envelope));
    roxmltree::Document::parse(packet).unwrap();
    assert_eq!(
        crate::payload::read(&bytes).unwrap().unwrap(),
        crate::payload::visible(&doc)
    );
    assert_pdf_xref(&bytes);
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(&bytes).unwrap();
    match std::process::Command::new("qpdf")
        .arg("--check")
        .arg(file.path())
        .output()
    {
        Ok(output) => assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => panic!("Could not check the PDF: {error}"),
    }
}

#[tokio::test]
async fn publication_vectors_include_the_prepared_drawing_only_when_enabled() {
    let doc = vector_payload_document();
    let engine = Default::default();
    let mut live_drawing = None;
    for format in ["svg", "svg-office", "pdf"] {
        let plain = publication(
            &engine,
            doc.clone(),
            format,
            false,
            false,
            Background::Canvas,
            crate::style::DEFAULT.png_dpi,
            FILE_PIXELS,
        )
        .await
        .unwrap();
        let editable = publication(
            &engine,
            doc.clone(),
            format,
            false,
            true,
            Background::Canvas,
            crate::style::DEFAULT.png_dpi,
            FILE_PIXELS,
        )
        .await
        .unwrap();
        assert!(crate::payload::read(&plain.bytes).is_none(), "{format}");
        let restored = crate::payload::read(&editable.bytes).unwrap().unwrap();
        assert_eq!(
            restored,
            crate::payload::visible(&doc.current()),
            "{format}"
        );
        if format == "svg" {
            live_drawing = Some(restored);
        } else if format == "svg-office" {
            assert_eq!(&restored, live_drawing.as_ref().unwrap());
        }
        if matches!(format, "svg" | "svg-office") {
            let xml =
                roxmltree::Document::parse(std::str::from_utf8(&editable.bytes).unwrap()).unwrap();
            assert_eq!(
                xml.descendants().any(|node| node.has_tag_name("text")),
                format == "svg",
                "{format}"
            );
        }
        assert!(plain.details.is_empty(), "{format}");
        assert_eq!(
            editable.details,
            ["Includes the editable drawing"],
            "{format}"
        );
        if format == "pdf" {
            assert_eq!(pdf_media_box(&plain.bytes), pdf_media_box(&editable.bytes));
        } else {
            let original = parse_svg(String::from_utf8(plain.bytes).unwrap()).unwrap();
            let restored = parse_svg(String::from_utf8(editable.bytes).unwrap()).unwrap();
            assert_eq!(
                original.to_string(&Default::default()),
                restored.to_string(&Default::default())
            );
        }
    }
    let mut pages = doc;
    pages.page_layout = Some(Default::default());
    let plain = publication(
        &engine,
        pages.clone(),
        "pdf",
        true,
        false,
        Background::Canvas,
        crate::style::DEFAULT.png_dpi,
        FILE_PIXELS,
    )
    .await
    .unwrap();
    let editable = publication(
        &engine,
        pages,
        "pdf",
        true,
        true,
        Background::Canvas,
        crate::style::DEFAULT.png_dpi,
        FILE_PIXELS,
    )
    .await
    .unwrap();
    assert_eq!(editable.details, plain.details);
    assert!(crate::payload::read(&editable.bytes).is_none());
}

/// Space-filling balls that overlap export as the visible part of each, a
/// filled path with no contact shadow over it; PDF and PNG render it.
#[test]
fn space_filling_models_export_their_visible_parts_as_clipped_paths() {
    use crate::models3d::{Display, ForceField, Model3d, Placed, Style, set_display};
    let mut doc = Document::default();
    let c = doc.add_atom("C", crate::document::Point::new(0., 0.));
    let o = doc.add_atom("O", crate::document::Point::new(30., 0.));
    doc.add_bond(c, o, 2, "plain");
    let atoms = [(c, [-0.6, 0., -0.2]), (o, [0.6, 0., 0.2])]
        .map(|(id, xyz)| Placed { id, xyz })
        .to_vec();
    let mut model = Model3d::new(&doc, atoms, vec![], ForceField::Mmff94s);
    model.style = Style::SpaceFilling;
    doc.models.push(model);
    crate::models3d::require_version(&mut doc);
    assert!(set_display(&mut doc, 0, Display::ThreeD));

    let svg = String::from_utf8(figure(&doc, "svg").unwrap().bytes).unwrap();
    let region = "fill-rule=\"evenodd\" fill=\"url(#";
    assert_eq!(svg.matches(region).count(), 2);
    assert!(!svg.contains("<circle"));
    // No shadow: nothing clipped to a ball and no translucent black fills.
    assert!(!svg.contains("<clipPath") && !svg.contains("fill-opacity="));
    assert!(!svg.contains("fill=\"rgb(0,0,0)\""));
    let pdf = figure(&doc, "pdf").unwrap().bytes;
    assert!(pdf.starts_with(b"%PDF"));
    let png = figure(&doc, "png").unwrap().bytes;
    assert!(png.starts_with(b"\x89PNG"));
}

/// Every export used to load and scan the system fonts again; it now lays text
/// out with the database labels are measured with.
#[test]
fn exports_share_the_measuring_font_database() {
    let tree = parse_svg(scene::svg(&Document::default())).unwrap();
    assert!(std::sync::Arc::ptr_eq(
        tree.fontdb(),
        &crate::style::fontdb()
    ));
}

#[test]
fn previews_fit_the_box_on_the_chosen_background_and_show_the_png_pixels() {
    let doc = dark_ink_drawing();
    // On white the dark canvas's white ink turns black, as in the export.
    let Fitted {
        size,
        pixels: (width, height),
        rgba: pixels,
    } = png_preview(&doc, Background::Color([255; 3]), (400, 400), None).unwrap();
    assert!(width == 400 || height == 400, "{width} × {height}");
    assert!(width <= 400 && height <= 400);
    assert!(size.0 > 0. && size.1 > 0.);
    let pixels: Vec<[u8; 4]> = pixels.as_chunks::<4>().0.to_vec();
    assert_eq!(pixels.len(), (width * height) as usize);
    assert_eq!(pixels[0], [255; 4]);
    assert!(pixels.contains(&[0, 0, 0, 255]), "dark ink");
    let clear = png_preview(&doc, Background::Transparent, (400, 400), None).unwrap();
    assert_eq!(clear.rgba[..4], [0; 4]);
    // A PNG at a resolution below the box shows the file's own pixels.
    let file = figure_with_budget_and_drawing(
        &doc,
        "png",
        Background::Canvas,
        RasterBudget::file(150, FILE_PIXELS),
        None,
    )
    .unwrap();
    let Fitted {
        pixels: (width, height),
        rgba: pixels,
        ..
    } = png_preview(&doc, Background::Canvas, (4000, 4000), Some(150)).unwrap();
    let reader = png::Decoder::new(std::io::Cursor::new(&file.bytes))
        .read_info()
        .unwrap();
    assert_eq!((width, height), (reader.info().width, reader.info().height));
    assert_eq!(pixels.as_chunks::<4>().0.to_vec(), rgba(&file.bytes));
    // A PNG larger than the tile is scaled down to fit it, ink and all.
    assert!(width > 48 && height > 28, "{width} × {height}");
    let Fitted {
        pixels: (width, height),
        rgba: pixels,
        ..
    } = png_preview(&doc, Background::Color([255; 3]), (48, 28), Some(150)).unwrap();
    assert!((width == 48 && height <= 28) || (height == 28 && width <= 48));
    assert!(
        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[..3].iter().all(|&c| c < 96))
    );
}

/// A large drawing is written at a lower resolution, and its label says so.
#[test]
fn file_sizes_follow_the_adaptive_budget() {
    // 30 × 15 cm.
    let size = (30. / 2.54 * 96., 15. / 2.54 * 96.);
    assert_eq!(png_file_size(size, 300).unwrap().2, 300);
    let (width, height, dpi) = png_file_size(size, 1200).unwrap();
    assert_eq!(dpi, 600);
    assert_eq!((width, height), png_pixels(size, 600));
}

/// Drawings with every kind of bond corner: ring and chain mitres, fused
/// and three-way atoms, labels, wedges, colors, crossings and carbonyls.
fn bond_corner_drawings() -> Vec<(&'static str, Document)> {
    use crate::document::Point;
    let mut drawings: Vec<_> = [
        (
            "bond-join-regression",
            &include_bytes!("../../../../tests/fixtures/bond-join-regression.rsk")[..],
        ),
        (
            "artemisinin",
            include_bytes!("../../../../tests/fixtures/qa-artemisinin-after.rsk"),
        ),
        (
            "esterification",
            include_bytes!("../../../../tests/fixtures/esterification-scheme.rsk"),
        ),
        (
            "tilted-fused",
            include_bytes!("../../../../tests/fixtures/tilted-fused-double-bonds.rsk"),
        ),
        (
            "phenylalanine",
            include_bytes!("../../../../tests/fixtures/phenylalanine-rotation.rsk"),
        ),
    ]
    .into_iter()
    .map(|(name, bytes)| (name, Document::from_json(bytes).unwrap()))
    .collect();
    let mut doc = Document::default();
    let hexagon: Vec<_> = (0..6)
        .map(|i| {
            let angle = std::f32::consts::FRAC_PI_3 * i as f32 + 0.5;
            doc.add_atom("C", Point::new(30. * angle.cos(), 30. * angle.sin()))
        })
        .collect();
    for i in 0..6 {
        doc.add_bond(hexagon[i], hexagon[(i + 1) % 6], 1 + (i % 2) as u8, "plain");
    }
    let chain: Vec<_> = [
        (60., 10.),
        (86., 25.),
        (112., 10.),
        (138., 25.),
        (164., 10.),
    ]
    .into_iter()
    .map(|(x, y)| doc.add_atom("C", Point::new(x, y)))
    .collect();
    doc.add_bond(hexagon[0], chain[0], 1, "plain");
    doc.add_bond(chain[0], chain[1], 1, "wedge");
    doc.add_bond(chain[1], chain[2], 1, "plain");
    doc.add_bond(chain[2], chain[3], 1, "plain");
    doc.add_bond(chain[3], chain[4], 2, "plain");
    doc.atoms.last_mut().unwrap().element = "O".into();
    doc.bonds[2].color = crate::palette::Color::Custom([200, 0, 0]);
    // A bond crossing the chain.
    let over = doc.add_atom("C", Point::new(120., -20.));
    let under = doc.add_atom("N", Point::new(130., 50.));
    doc.add_bond(over, under, 1, "plain");
    // A right angle between two colors. (A corner sharper than 29° is
    // left out: its outline also narrows the inner edge, which a stroke
    // keeps at full width.)
    let right =
        [(180., 40.), (220., 40.), (180., 80.)].map(|(x, y)| doc.add_atom("C", Point::new(x, y)));
    doc.add_bond(right[1], right[0], 1, "plain");
    doc.add_bond(right[0], right[2], 1, "plain");
    doc.bonds.last_mut().unwrap().color = crate::palette::Color::Custom([0, 0, 200]);
    drawings.push(("ring-chain-crossing", doc));
    drawings
}

/// Vector export strokes plain bonds; at 1200 dpi its pixels must match the
/// filled outlines the canvas and PNG draw, ends and corners included.
#[test]
fn vector_bond_lines_paint_the_same_ink_as_the_filled_outlines() {
    for (name, doc) in bond_corner_drawings() {
        let filled = scene::primitives(&doc);
        let stroked = scene::vector_primitives(&doc);
        assert!(
            stroked
                .iter()
                .any(|p| matches!(p, scene::Primitive::Polyline { .. })),
            "{name}"
        );
        let pixels = |primitives| {
            let tree = parse_svg(scene::svg_of(&doc, primitives, None)).unwrap();
            let size = png_pixels((tree.size().width(), tree.size().height()), 1200);
            (size, raster(&tree, None, 1200., size, (0., 0.)).unwrap())
        };
        let (size, before) = pixels(filled);
        let (stroked_size, after) = pixels(stroked);
        assert_eq!(size, stroked_size, "{name}: the bounds are the outlines'");
        let (mut ink, mut differ, mut total) = (0, 0, 0.);
        for (a, b) in before.pixels().iter().zip(after.pixels()) {
            // Colors too, so a corner painted in the neighbour's color counts.
            let channels = |c: &resvg::tiny_skia::PremultipliedColorU8| {
                [c.red(), c.green(), c.blue(), c.alpha()]
            };
            let change = channels(a)
                .into_iter()
                .zip(channels(b))
                .map(|(a, b)| a.abs_diff(b))
                .max()
                .unwrap_or_default();
            ink += usize::from(a.alpha().max(b.alpha()) > 0);
            if change > 127 {
                let i = (a as *const _ as usize - before.pixels().as_ptr() as usize) / 4;
                let (w, _) = size;
                if differ % 40 == 0 {
                    println!(
                        "DBG {} {} {:?} {:?}",
                        i as u32 % w,
                        i as u32 / w,
                        channels(a),
                        channels(b)
                    );
                }
            }
            differ += usize::from(change > 127);
            total += f64::from(change) / 255.;
        }
        println!(
            "{name}: {ink} ink pixels, {differ} differ by half, {total:.1} pixels of ink moved"
        );
        assert!(
            differ * 10_000 <= ink && total * 1000. <= ink as f64,
            "{name}: {differ} of {ink} pixels differ, {total:.1} pixels of ink moved"
        );
        let svg = String::from_utf8(drawing(&doc, "svg").unwrap()).unwrap();
        assert!(
            svg.contains("stroke-linejoin=\"miter\""),
            "{name}: SVG export strokes bonds"
        );
    }
}
