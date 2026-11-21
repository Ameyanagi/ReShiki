//! Gradient and see-through fills against ChemDraw's faded fills (FadePercent).
use super::*;
use crate::graphics::{Effects, Gradient, GradientKind, Graphic, GraphicKind, GraphicStyle};
use crate::palette::Color;

const RED: Color = Color::Custom([255, 0, 0]);
const BLUE: Color = Color::Custom([0, 0, 255]);

fn shape(id: u64, kind: GraphicKind, style: GraphicStyle) -> Graphic {
    Graphic::dragged(
        id,
        kind,
        Point::new(id as f32 * 30., 0.),
        Point::new(id as f32 * 30. + 20., 10.),
        style,
        Default::default(),
        false,
    )
}

fn faded(fill: Color, stroke: Color, opacity: f32) -> GraphicStyle {
    GraphicStyle {
        stroke,
        fill: Some(fill),
        effects: Effects {
            opacity: Some(opacity),
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Each graphic's or curve's attributes, as written.
fn nodes(xml: &str, tag: &str, key: &str) -> Vec<String> {
    let tree = roxmltree::Document::parse(xml).unwrap();
    tree.descendants()
        .filter(|n| n.has_tag_name(tag))
        .filter_map(|n| n.attribute(key).map(str::to_owned))
        .collect()
}

#[tokio::test]
async fn chemdraw_faded_fills_open_as_see_through_fills() {
    // Measured with ChemDraw's own renderer: a plain closed shape's inside is
    // its color at FadePercent tenths of a percent over white; a filled or
    // shaded shape, a line, an arrow and FadePercent 0 or 1000 show no fade.
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<CDXML><colortable><color r="1" g="1" b="1"/><color r="0" g="0" b="0"/><color r="1" g="0" b="0"/></colortable>
<page id="1">
<graphic id="2" GraphicType="Rectangle" RectangleType="Plain" color="4" FadePercent="300" BoundingBox="10 10 40 30"/>
<graphic id="3" GraphicType="Oval" OvalType="Circle" color="4" FadePercent="700" BoundingBox="70 20 60 20" Center3D="60 20 0" MajorAxisEnd3D="70 20 0" MinorAxisEnd3D="60 10 0"/>
<curve id="4" CurveType="1" Closed="yes" color="4" FadePercent="500" CurvePoints="80 10 80 10 100 10 100 30 100 30 80 30"/>
<graphic id="5" GraphicType="Rectangle" RectangleType="Filled" color="4" FadePercent="300" BoundingBox="110 10 140 30"/>
<graphic id="6" GraphicType="Oval" OvalType="Circle Shaded" color="4" FadePercent="300" BoundingBox="170 20 160 20" Center3D="160 20 0" MajorAxisEnd3D="170 20 0" MinorAxisEnd3D="160 10 0"/>
<graphic id="7" GraphicType="Line" color="4" FadePercent="300" BoundingBox="10 50 40 50"/>
<graphic id="8" GraphicType="Rectangle" RectangleType="Plain" color="4" FadePercent="0" BoundingBox="50 40 80 60"/>
<graphic id="9" GraphicType="Rectangle" RectangleType="Plain" color="4" FadePercent="1000" BoundingBox="90 40 120 60"/>
<arrow id="10" ArrowheadHead="Full" ArrowheadType="Solid" color="4" FadePercent="300" Head3D="170 50 0" Tail3D="130 50 0"/>
</page></CDXML>"#;
    let doc = crate::formats::read(xml.into(), "cdxml").await.unwrap();
    let fade: Vec<_> = doc
        .graphics
        .iter()
        .map(|g| (g.style.effects.opacity, g.style.fill.map(|c| c.rgb())))
        .collect();
    let red = Some([255, 0, 0]);
    assert_eq!(
        fade,
        [
            (Some(0.3), red),
            (Some(0.7), red),
            (Some(0.5), red),
            (None, red),
            (None, red),
            (None, None),
            (None, None),
            (None, None),
        ]
    );
    assert_eq!(doc.arrows.len(), 1);
    assert_eq!(doc.file_version(), crate::document::DRAWING_ELEMENTS);
}

#[tokio::test]
async fn a_fade_that_is_not_a_number_is_still_refused() {
    let xml = r#"<CDXML><page id="1"><graphic id="2" GraphicType="Rectangle" FadePercent="nan" BoundingBox="10 10 40 30"/></page></CDXML>"#;
    let error = crate::formats::read(xml.into(), "cdxml")
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("Transparent/faded CDXML graphics are not supported yet"));
}

#[tokio::test]
async fn see_through_fills_export_as_faded_fills_and_come_back() {
    // A drawn curve made into a closed path, as the shading tests do.
    let mut path = shape(3, GraphicKind::Curve, faded(BLUE, BLUE, 0.25));
    path.kind = GraphicKind::Path;
    path.style = faded(BLUE, BLUE, 0.25);
    path.path = vec![
        PathCommand::Move(Point::new(100., 0.)),
        PathCommand::Line(Point::new(120., 0.)),
        PathCommand::Line(Point::new(110., 15.)),
        PathCommand::Close,
    ];
    (path.origin, path.axis_x, path.axis_y) =
        (Point::default(), Point::new(1., 0.), Point::new(0., 1.));
    let document = Document {
        graphics: vec![
            shape(1, GraphicKind::Rectangle, faded(RED, RED, 0.3)),
            shape(2, GraphicKind::Ellipse, faded(RED, RED, 0.6)),
            path,
        ],
        ..Default::default()
    };
    for format in ["cdxml", "cdx"] {
        let bytes = crate::formats::write(document.clone(), format)
            .await
            .unwrap();
        let xml = if format == "cdx" {
            crate::exchange::from_cdx(&bytes).unwrap()
        } else {
            String::from_utf8(bytes.clone()).unwrap()
        };
        // ChemDraw's own faded shapes: plain, with FadePercent and no fill.
        assert_eq!(
            nodes(&xml, "graphic", "FadePercent"),
            ["300", "600"],
            "{format}"
        );
        assert_eq!(
            nodes(&xml, "graphic", "RectangleType"),
            ["Plain"],
            "{format}"
        );
        assert_eq!(nodes(&xml, "graphic", "OvalType"), [""], "{format}");
        assert_eq!(nodes(&xml, "curve", "FadePercent"), ["250"], "{format}");
        assert!(
            !nodes(&xml, "curve", "FillType").contains(&"Solid".into()),
            "{format}"
        );
        let back = crate::formats::read(bytes, format).await.unwrap();
        let opacity: Vec<_> = back
            .graphics
            .iter()
            .map(|g| {
                (
                    g.kind,
                    g.style.effects.opacity,
                    g.style.fill.map(|c| c.rgb()),
                )
            })
            .collect();
        assert_eq!(
            opacity,
            [
                (GraphicKind::Rectangle, Some(0.3), Some([255, 0, 0])),
                (GraphicKind::Ellipse, Some(0.6), Some([255, 0, 0])),
                (GraphicKind::Path, Some(0.25), Some([0, 0, 255])),
            ],
            "{format}"
        );
    }
}

#[tokio::test]
async fn gradients_and_other_see_through_color_export_as_the_color_chemdraw_shows() {
    let gradient = GraphicStyle {
        stroke: BLUE,
        fill: Some(BLUE),
        effects: Effects {
            gradient: Some(Gradient {
                from: Gradient::PAPER,
                kind: GradientKind::Radial,
            }),
            ..Default::default()
        },
        ..Default::default()
    };
    let mut line = shape(
        3,
        GraphicKind::Line,
        GraphicStyle {
            stroke: BLUE,
            ..Default::default()
        },
    );
    line.style.effects.opacity = Some(0.5);
    let document = Document {
        graphics: vec![
            shape(1, GraphicKind::Rectangle, gradient),
            // A fill in another color than its outline cannot fade in ChemDraw.
            shape(2, GraphicKind::Rectangle, faded(RED, Color::Ink, 0.5)),
            line,
        ],
        ..Default::default()
    };
    let bytes = crate::formats::write(document.clone(), "cdxml")
        .await
        .unwrap();
    let xml = String::from_utf8(bytes.clone()).unwrap();
    assert!(!xml.contains("FadePercent"));
    let back = crate::formats::read(bytes, "cdxml").await.unwrap();
    let colors: Vec<_> = back
        .graphics
        .iter()
        .map(|g| (g.style.fill.map(|c| c.rgb()), g.style.effects))
        .collect();
    // Halfway from white to blue; red over white at half strength.
    assert!(colors.contains(&(Some([128, 128, 255]), Effects::default())));
    assert!(colors.contains(&(Some([255, 128, 128]), Effects::default())));
    // The line fades over white paper.
    assert!(
        back.graphics
            .iter()
            .any(|g| g.style.fill.is_none() && g.style.stroke.rgb() == [128, 128, 255])
    );
    let notices = crate::engine::export_notices::notices(
        &document,
        "cdxml",
        crate::engine::export_notices::Route::Prepared {
            source: &document,
            figure_only: false,
        },
    );
    assert!(notices.contains(&crate::graphics::EXPORT_NOTICE.to_owned()));
}

#[tokio::test]
async fn ring_fill_effects_export_as_the_flat_color_chemdraw_shows() {
    use crate::ring_fills::RingEffect;
    let mut document = Document::default();
    let ring = crate::editing::ring(&mut document, Point::new(0., 0.), 6, false, 30.);
    crate::ring_fills::apply(&mut document, &ring, Some(BLUE));
    for (effect, rgb) in [
        (RingEffect::Gradient, [128, 128, 255]),
        (RingEffect::SeeThrough, [128, 128, 255]),
    ] {
        crate::ring_fills::set_effect(&mut document, &ring, effect);
        let bytes = crate::formats::write(document.clone(), "cdxml")
            .await
            .unwrap();
        let back = crate::formats::read(bytes, "cdxml").await.unwrap();
        assert_eq!(back.ring_fills.len(), 1, "{effect}");
        assert_eq!(back.ring_fills[0].color.rgb(), rgb, "{effect}");
        assert_eq!(back.ring_fills[0].effect, RingEffect::Flat, "{effect}");
    }
}

#[tokio::test]
async fn glows_and_discs_export_flat_with_a_notice() {
    use crate::ring_fills::RingEffect;
    let mut document = Document::default();
    let ring = crate::editing::ring(&mut document, Point::new(0., 0.), 6, false, 30.);
    crate::ring_fills::apply(&mut document, &ring, Some(BLUE));
    for effect in [RingEffect::Glow, RingEffect::Disc] {
        crate::ring_fills::set_effect(&mut document, &ring, effect);
        let bytes = crate::formats::write(document.clone(), "cdxml")
            .await
            .unwrap();
        let back = crate::formats::read(bytes, "cdxml").await.unwrap();
        assert_eq!(back.ring_fills[0].color.rgb(), [0, 0, 255], "{effect}");
        assert_eq!(back.ring_fills[0].effect, RingEffect::Flat, "{effect}");
    }
    document.ring_fills.clear();
    document.highlight_style = crate::highlights::HighlightStyle::Glow;
    document.atoms[0].display.highlight = Some(RED);
    let notices = crate::engine::export_notices::notices(
        &document,
        "cdx",
        crate::engine::export_notices::Route::Prepared {
            source: &document,
            figure_only: false,
        },
    );
    assert!(notices.contains(&crate::graphics::GLOW_NOTICE.to_owned()));
    let bytes = crate::formats::write(document, "cdxml").await.unwrap();
    let back = crate::formats::read(bytes, "cdxml").await.unwrap();
    // ChemDraw keeps the highlight itself, flat.
    assert_eq!(
        back.atoms[0].display.highlight.map(|c| c.rgb()),
        Some([255, 0, 0])
    );
    assert!(back.highlight_style.is_flat());
}
