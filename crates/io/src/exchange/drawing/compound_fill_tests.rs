use super::*;
use resvg::{tiny_skia, usvg};

const BLUE: [u8; 3] = [128, 204, 255];
const INK: [u8; 3] = [38, 50, 56];

fn source(path: &str) -> Document {
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="96" height="96"><rect width="96" height="96" fill="#80ccff"/><path d="{path}" fill="#263238"/></svg>"##
    );
    crate::svg_drawing::read(svg.as_bytes(), Default::default())
        .unwrap()
        .drawing
}

fn rendered(doc: &Document) -> tiny_skia::Pixmap {
    let svg = crate::export::drawing(doc, "svg").unwrap();
    let tree = usvg::Tree::from_data(&svg, &Default::default()).unwrap();
    let size = tree.size().to_int_size();
    let mut pixels = tiny_skia::Pixmap::new(size.width(), size.height()).unwrap();
    pixels.fill(tiny_skia::Color::WHITE);
    resvg::render(&tree, Default::default(), &mut pixels.as_mut());
    pixels
}

// The background rectangle is an independent color landmark, so export's page
// translation and CDX coordinate quantization do not determine the expected ink.
fn sample(pixels: &tiny_skia::Pixmap, at: [f32; 2]) -> [u8; 3] {
    let (mut lo, mut hi) = ([u32::MAX; 2], [0; 2]);
    for (i, pixel) in pixels.data().as_chunks::<4>().0.iter().enumerate() {
        if pixel[..3] == BLUE {
            let xy = [i as u32 % pixels.width(), i as u32 / pixels.width()];
            for j in 0..2 {
                lo[j] = lo[j].min(xy[j]);
                hi[j] = hi[j].max(xy[j]);
            }
        }
    }
    assert!(lo[0] < hi[0] && lo[1] < hi[1]);
    let [x, y] = std::array::from_fn(|j| lo[j] + ((hi[j] - lo[j]) as f32 * at[j]).round() as u32);
    let start = ((y * pixels.width() + x) * 4) as usize;
    pixels.data()[start..start + 3].try_into().unwrap()
}

#[tokio::test]
async fn compound_fills_keep_holes_and_nonzero_overlaps_through_normal_exchange() {
    for (path, probes) in [
        // The hole must reveal the blue artwork below, not a white knockout.
        (
            "M16 16H80V80H16Z M32 32V64H64V32Z",
            vec![([0.5, 0.5], BLUE), ([0.25, 0.5], INK)],
        ),
        // Same winding overlaps add ink, while opposite winding cancels it.
        ("M16 16H64V64H16Z M32 32H80V80H32Z", vec![([0.5, 0.5], INK)]),
        (
            "M16 16H64V64H16Z M32 32V80H80V32Z",
            vec![([0.5, 0.5], BLUE), ([0.25, 0.25], INK), ([0.75, 0.75], INK)],
        ),
        ("M16 16H80V80H16Z M32 32H64V64H32Z", vec![([0.5, 0.5], INK)]),
        (
            "M16 16H80V80H16Z M24 24V72H72V24Z M40 40H56V56H40Z",
            vec![([0.5, 0.5], INK), ([0.3, 0.5], BLUE)],
        ),
        ("M16 16H48V80H16Z M48 16H80V80H48Z", vec![([0.5, 0.5], INK)]),
        (
            "M20 20H76V76H20Z M46 30V66H50V30Z",
            vec![([0.5, 0.5], BLUE), ([0.4, 0.5], INK)],
        ),
    ] {
        let doc = source(path);
        let original = rendered(&doc);
        for &(at, rgb) in &probes {
            assert!(
                sample(&original, at)
                    .iter()
                    .zip(rgb)
                    .all(|(&a, b)| a.abs_diff(b) <= 3),
                "source {path}"
            );
        }
        for format in ["cdxml", "cdx"] {
            let bytes = crate::formats::write(doc.clone(), format).await.unwrap();
            let back = crate::formats::read(bytes, format).await.unwrap();
            let pixels = rendered(&back);
            for &(at, rgb) in &probes {
                assert!(
                    sample(&pixels, at)
                        .iter()
                        .zip(rgb)
                        .all(|(&a, b)| a.abs_diff(b) <= 3),
                    "{format}: {path}"
                );
            }
        }
    }
}

#[tokio::test]
async fn real_reflux_jacket_keeps_its_water_visible_after_editable_export() {
    let svg = include_bytes!("../../../../../tests/fixtures/svg-drawing/reflux-apparatus.svg");
    let doc = crate::svg_drawing::read(svg, Default::default())
        .unwrap()
        .drawing;
    let water = |pixels: &tiny_skia::Pixmap| {
        pixels
            .data()
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[..3] == [184, 220, 233])
            .count()
    };
    let before = water(&rendered(&doc));
    assert!(before > 100);
    for format in ["cdxml", "cdx"] {
        let bytes = crate::formats::write(doc.clone(), format).await.unwrap();
        if let Some(directory) = std::env::var_os("RESHIKI_COMPOUND_QA_DIR") {
            let directory = std::path::PathBuf::from(directory);
            std::fs::create_dir_all(&directory).unwrap();
            std::fs::write(
                directory.join(format!("Reflux compound fill.{format}")),
                &bytes,
            )
            .unwrap();
        }
        let back = crate::formats::read(bytes, format).await.unwrap();
        let after = water(&rendered(&back));
        assert!(
            after * 10 >= before * 9,
            "{format}: visible water {before} → {after}"
        );
    }
}

fn native_path(fill: bool, compound: bool) -> Document {
    use crate::{
        graphics::{Graphic, GraphicKind, GraphicStyle, PathCommand},
        palette::Color,
    };
    let mut g = Graphic::dragged(
        1,
        GraphicKind::Path,
        Point::default(),
        Point::new(1., 1.),
        GraphicStyle {
            fill: fill.then_some(Color::Custom(INK)),
            stroke: Color::Custom(BLUE),
            width_pt: 1.2,
            ..Default::default()
        },
        Default::default(),
        false,
    );
    g.origin = Point::default();
    g.axis_x = Point::new(1., 0.);
    g.axis_y = Point::new(0., 1.);
    g.node_types.clear();
    g.path = vec![
        PathCommand::Move(Point::new(0., 0.)),
        PathCommand::Cubic(
            Point::new(10., -20.),
            Point::new(20., -20.),
            Point::new(30., 0.),
        ),
        PathCommand::Line(Point::new(30., 30.)),
        PathCommand::Line(Point::new(0., 30.)),
        PathCommand::Close,
    ];
    if compound {
        g.path.extend([
            PathCommand::Move(Point::new(10., 5.)),
            PathCommand::Line(Point::new(10., 15.)),
            PathCommand::Line(Point::new(20., 15.)),
            PathCommand::Line(Point::new(20., 5.)),
            PathCommand::Close,
        ]);
    }
    Document {
        graphics: vec![g],
        ..Default::default()
    }
}

#[tokio::test]
async fn compound_fill_cuts_have_no_strokes_and_original_beziers_follow_the_fill() {
    for (fill, compound) in [(true, true), (true, false), (false, true)] {
        let doc = native_path(fill, compound);
        let scale = f64::from(doc.drawing_style.points_per_world());
        for format in ["cdxml", "cdx"] {
            let bytes = crate::formats::write(doc.clone(), format).await.unwrap();
            let xml = if format == "cdx" {
                crate::exchange::from_cdx(&bytes).unwrap()
            } else {
                String::from_utf8(bytes).unwrap()
            };
            let tree = roxmltree::Document::parse(&xml).unwrap();
            let curves: Vec<_> = tree
                .descendants()
                .filter(|n| n.has_tag_name("curve"))
                .collect();
            let outlines: Vec<_> = curves
                .iter()
                .filter(|n| n.attribute("FillType") == Some("None"))
                .collect();
            assert_eq!(outlines.len(), if compound { 2 } else { 1 });
            let first = curves
                .iter()
                .position(|n| n.attribute("FillType") == Some("None"))
                .unwrap();
            assert!(
                curves[..first]
                    .iter()
                    .all(|n| n.attribute("LineWidth") == Some("0"))
            );
            assert!(
                curves[first..]
                    .iter()
                    .all(|n| n.attribute("FillType") == Some("None"))
            );
            if fill && !compound {
                assert_eq!(curves.len(), 2);
            }
            if !fill {
                assert_eq!(curves.len(), 2);
            }
            let values: Vec<f64> = outlines[0]
                .attribute("CurvePoints")
                .unwrap()
                .split_whitespace()
                .map(|s| s.parse().unwrap())
                .collect();
            for (index, expected) in [(4, 10.), (5, -20.), (6, 20.), (7, -20.), (8, 30.), (9, 0.)] {
                assert!(
                    ((values[index] - values[index % 2 + 2]) / scale - expected).abs() < 0.001,
                    "{format}: original cubic control"
                );
            }
            assert!(outlines.iter().all(|n| {
                (n.attribute("LineWidth").unwrap().parse::<f32>().unwrap() - 1.2).abs() < 0.0001
            }));
        }
    }
}

#[tokio::test]
async fn curved_compound_interiors_have_no_cut_lines_in_the_returned_model() {
    let doc = source(
        "M16 48C16 5 80 5 80 48C80 91 16 91 16 48Z M32 48C32 69 64 69 64 48C64 27 32 27 32 48Z",
    );
    for format in ["cdxml", "cdx"] {
        let bytes = crate::formats::write(doc.clone(), format).await.unwrap();
        let back = crate::formats::read(bytes, format).await.unwrap();
        let pixels = rendered(&back);
        for y in 30..=66 {
            assert_eq!(
                sample(&pixels, [0.28, y as f32 / 96.]),
                INK,
                "{format}: no interior seam at y={y}"
            );
        }
        assert_eq!(sample(&pixels, [0.5, 0.5]), BLUE);
    }
}

#[tokio::test]
async fn compound_effects_keep_the_existing_export_route_and_geometry_limits_fail_cleanly() {
    for (opacity, shaded) in [(Some(0.5), false), (None, true)] {
        let mut doc = native_path(true, true);
        let style = &mut doc.graphics[0].style;
        style.stroke = style.fill.unwrap();
        style.effects.opacity = opacity;
        style.effects.shaded = shaded;
        for format in ["cdxml", "cdx"] {
            let bytes = crate::formats::write(doc.clone(), format).await.unwrap();
            let xml = if format == "cdx" {
                crate::exchange::from_cdx(&bytes).unwrap()
            } else {
                String::from_utf8(bytes).unwrap()
            };
            let tree = roxmltree::Document::parse(&xml).unwrap();
            let curves: Vec<_> = tree
                .descendants()
                .filter(|n| n.has_tag_name("curve"))
                .collect();
            if opacity.is_some() {
                assert_eq!(curves.len(), 2);
                assert!(
                    curves
                        .iter()
                        .all(|n| n.attribute("FadePercent") == Some("500"))
                );
            } else {
                assert_eq!(curves.len(), 4);
                assert_eq!(
                    curves
                        .iter()
                        .filter(|n| n.attribute("FillType") == Some("Shaded")
                            && n.attribute("CurveType") == Some("257"))
                        .count(),
                    2
                );
            }
        }
    }
    let mut large = native_path(true, true);
    large.graphics[0].path.clear();
    for i in 0..3334 {
        let x = i as f32 * 2.;
        large.graphics[0].path.extend([
            PathCommand::Move(Point::new(x, 0.)),
            PathCommand::Line(Point::new(x + 1., 0.)),
            PathCommand::Line(Point::new(x + 1., 1.)),
            PathCommand::Line(Point::new(x, 1.)),
            PathCommand::Close,
        ]);
    }
    for format in ["cdxml", "cdx"] {
        let error = crate::formats::write(large.clone(), format)
            .await
            .unwrap_err();
        assert!(error.contains("size or work limit"), "{format}: {error}");
    }
}

#[tokio::test]
async fn compound_close_continuations_do_not_turn_a_triangle_into_a_rectangle() {
    use crate::{
        graphics::{Graphic, GraphicKind, GraphicStyle},
        palette::Color,
    };
    let mut doc = native_path(true, true);
    doc.graphics[0].style.stroke = Color::Custom(INK);
    doc.graphics[0].style.width_pt = 0.1;
    doc.graphics[0].path = vec![
        PathCommand::Move(Point::new(0., 0.)),
        PathCommand::Line(Point::new(20., 0.)),
        PathCommand::Line(Point::new(20., 20.)),
        PathCommand::Close,
        PathCommand::Line(Point::new(0., 20.)),
        PathCommand::Close,
        PathCommand::Move(Point::new(23., 0.)),
        PathCommand::Line(Point::new(27., 0.)),
        PathCommand::Line(Point::new(27., 4.)),
        PathCommand::Close,
    ];
    doc.graphics.insert(
        0,
        Graphic::dragged(
            2,
            GraphicKind::Rectangle,
            Point::new(-5., -5.),
            Point::new(30., 30.),
            GraphicStyle {
                fill: Some(Color::Custom(BLUE)),
                stroke: Color::Custom(BLUE),
                ..Default::default()
            },
            Default::default(),
            false,
        ),
    );
    let probes = [([8. / 35., 21. / 35.], BLUE), ([21. / 35., 8. / 35.], INK)];
    let pixels = rendered(&doc);
    for &(at, rgb) in &probes {
        assert_eq!(sample(&pixels, at), rgb, "source");
    }
    for format in ["cdxml", "cdx"] {
        let bytes = crate::formats::write(doc.clone(), format).await.unwrap();
        let back = crate::formats::read(bytes, format).await.unwrap();
        let pixels = rendered(&back);
        for &(at, rgb) in &probes {
            assert_eq!(sample(&pixels, at), rgb, "{format}");
        }
    }
}

#[tokio::test]
async fn compound_three_disjoint_contours_return_to_the_hub_and_keep_reflected_winding() {
    let original = source("M16 16H24V24H16Z M72 16H80V24H72Z M44 72H52V80H44Z");
    for reflected in [false, true] {
        let mut doc = original.clone();
        if reflected {
            for g in &mut doc.graphics {
                g.origin.x = -g.origin.x;
                g.axis_x.x = -g.axis_x.x;
                g.axis_y.x = -g.axis_y.x;
            }
        }
        for format in ["cdxml", "cdx"] {
            let bytes = crate::formats::write(doc.clone(), format).await.unwrap();
            let back = crate::formats::read(bytes, format).await.unwrap();
            let pixels = rendered(&back);
            assert_eq!(
                sample(&pixels, [0.5, 0.45]),
                BLUE,
                "{format}: no false triangle"
            );
            assert_eq!(
                sample(&pixels, [0.5, 16. / 96.]),
                BLUE,
                "{format}: no bridge stroke"
            );
            let fill = back
                .graphics
                .iter()
                .find(|g| g.is_fill_only_path() && g.style.fill.map(|c| c.rgb()) == Some(INK))
                .unwrap_or_else(|| {
                    panic!(
                        "{format}: filled objects {:?}",
                        back.graphics
                            .iter()
                            .map(|g| (g.kind, g.style.width_pt, g.style.fill))
                            .collect::<Vec<_>>()
                    )
                });
            let blue = back
                .graphics
                .iter()
                .find(|g| g.style.fill.map(|c| c.rgb()) == Some(BLUE))
                .unwrap();
            let (lo, hi) = blue.shape_bounds();
            let point = |x, y| Point::new(lo.x + (hi.x - lo.x) * x, lo.y + (hi.y - lo.y) * y);
            assert!(
                !fill.hit(point(0.5, 16. / 96.), 2.),
                "{format}: no invisible bridge target"
            );
            assert!(!fill.hit(point(0.5, 0.45), 2.));
            assert!(fill.hit(point(0.2, 0.2), 2.));
            let saved = back.file_json().unwrap();
            let reopened = Document::from_json(&saved).unwrap();
            assert_eq!(rendered(&reopened).data(), pixels.data());
        }
    }
}

#[tokio::test]
async fn compound_borderless_reader_keeps_explicit_solid_zero_but_not_bold_or_other_styles() {
    for (attributes, expected) in [
        ("CurveType='129' FillType='Solid'", 0.),
        ("CurveType='133' FillType='Solid' BoldWidth='2'", 2.),
        (
            "CurveType='129' FillType='Solid' LineType='Bold' BoldWidth='2'",
            2.,
        ),
        ("CurveType='1' FillType='None'", 0.1),
        ("CurveType='257' FillType='Shaded'", 0.1),
        ("CurveType='1' FillType='None' FadePercent='500'", 0.1),
    ] {
        let xml = format!(
            "<CDXML><page><curve id='2' {attributes} LineWidth='0' Closed='yes' CurvePoints='0 0 0 0 0 0 20 0 20 0 20 0 20 20 20 20 20 20 0 20 0 20 0 20'/></page></CDXML>"
        );
        let doc = crate::formats::read(xml.into_bytes(), "cdxml")
            .await
            .unwrap();
        assert_eq!(doc.graphics.len(), 1);
        assert!(
            (doc.graphics[0].style.width_pt - expected).abs() < 0.0001,
            "{attributes}"
        );
    }
}
