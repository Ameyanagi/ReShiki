use super::*;
use crate::graphics::{Effects, Graphic, GraphicKind, GraphicStyle, LinePattern, PathCommand};

const OWNER: [(&str, &[u8]); 2] = [
    (
        "cdxml",
        include_bytes!("../../../../../tests/fixtures/chemdraw-drawing-elements/drawing.cdxml"),
    ),
    (
        "cdx",
        include_bytes!("../../../../../tests/fixtures/chemdraw-drawing-elements/drawing.cdx"),
    ),
];

#[tokio::test]
async fn the_owners_shaded_shadowed_and_wavy_shapes_import_from_cdx_and_cdxml() {
    for (format, bytes) in OWNER {
        let doc = crate::formats::read(bytes.to_vec(), format)
            .await
            .unwrap_or_else(|error| panic!("{format}: {error}"));
        let effects: Vec<_> = doc.graphics.iter().map(|g| g.style.effects).collect();
        assert_eq!(effects.iter().filter(|e| e.shaded).count(), 4, "{format}");
        assert_eq!(
            effects
                .iter()
                .filter_map(|e| e.shadow_pt)
                .collect::<Vec<_>>(),
            [4.; 4],
            "{format}"
        );
        // ChemDraw's shaded shapes paint their one color.
        for g in doc.graphics.iter().filter(|g| g.style.effects.shaded) {
            assert_eq!(g.style.fill, Some(g.style.stroke), "{format}");
        }
        // ChemDraw's lines are graphic lines, the wavy one among them.
        assert!(doc.arrows.is_empty(), "{format}");
        let lines: Vec<_> = doc
            .graphics
            .iter()
            .filter(|g| g.kind == GraphicKind::Line)
            .map(|g| g.style.pattern)
            .collect();
        assert_eq!(lines.len(), 5, "{format}");
        assert_eq!(
            lines.iter().filter(|p| **p == LinePattern::Wavy).count(),
            1,
            "{format}"
        );
        assert_eq!(doc.file_version(), crate::document::DRAWING_ELEMENTS);
        // The shaded rounded rectangle: CornerRadius 600 on a 20.05 × 14.05 pt box.
        let rounded = doc
            .graphics
            .iter()
            .find(|g| g.kind == GraphicKind::RoundedRectangle && g.style.effects.shaded)
            .unwrap();
        let side = rounded
            .axis_x
            .distance(Point::default())
            .min(rounded.axis_y.distance(Point::default()));
        assert!(rounded.style.corner_pt.is_some());
        assert!(
            (rounded.corner_radius() / side - 6. / 14.05).abs() < 0.01,
            "{format}"
        );
    }
}

#[tokio::test]
async fn chemdraw_shading_rules_for_curves_and_headed_wavy_lines() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<CDXML><colortable><color r="1" g="1" b="1"/><color r="0" g="0" b="0"/><color r="0" g="0" b="1"/></colortable>
<page id="1">
<curve id="2" CurveType="256" color="4" CurvePoints="10 10 10 10 40 10 70 40 70 40 70 40"/>
<curve id="3" CurveType="0" LineType="Wavy" CurvePoints="10 80 10 80 40 90 70 80 70 80 70 80"/>
<arrow id="4" LineType="Wavy" ArrowheadHead="Full" ArrowheadType="Solid" Head3D="90 10 0" Tail3D="90 60 0"/>
<arrow id="5" LineType="Dashed Bold" ArrowheadType="Solid" Head3D="110 10 0" Tail3D="110 60 0"/>
<graphic id="6" GraphicType="Line" LineType="Wavy" BoundingBox="130 10 130 60"/>
</page></CDXML>"#;
    let doc = crate::formats::read(xml.into(), "cdxml").await.unwrap();
    let shaded = &doc.graphics[0];
    assert!(shaded.style.effects.shaded);
    // ChemDraw shades an open curve as if it were closed, in its own color.
    assert_eq!(shaded.path.last(), Some(&PathCommand::Close));
    assert_eq!(shaded.style.fill.map(|c| c.rgb()), Some([0, 0, 255]));
    // ChemDraw draws a wavy curve and a headed wavy arrow straight.
    assert_eq!(doc.graphics[1].style.pattern, LinePattern::Solid);
    assert_eq!(doc.arrows[0].appearance().pattern, LinePattern::Solid);
    // A line without heads is a graphic line, dashed and bold together.
    assert_eq!(doc.arrows.len(), 1);
    let bold_dashed = &doc.graphics[2];
    assert_eq!(bold_dashed.kind, GraphicKind::Line);
    assert_eq!(bold_dashed.style.pattern, LinePattern::Dashed);
    assert_eq!(bold_dashed.style.width_pt, 2.);
    assert_eq!(doc.graphics[3].kind, GraphicKind::Line);
    assert_eq!(doc.graphics[3].style.pattern, LinePattern::Wavy);
}

fn shape(id: u64, kind: GraphicKind, end: (f32, f32), style: GraphicStyle) -> Graphic {
    Graphic::dragged(
        id,
        kind,
        Point::new(id as f32 * 30., 0.),
        Point::new(id as f32 * 30. + end.0, end.1),
        style,
        Default::default(),
        false,
    )
}

#[tokio::test]
async fn effects_export_as_native_chemdraw_shapes_and_come_back() {
    use crate::arrows::{ArrowStyle, Head, Preset};
    let shaded = GraphicStyle {
        fill: Some(crate::palette::Color::Custom([0, 0, 255])),
        stroke: crate::palette::Color::Custom([0, 0, 255]),
        effects: Effects {
            shaded: true,
            shadow_pt: None,
            ..Default::default()
        },
        ..Default::default()
    };
    let shadowed = GraphicStyle {
        pattern: LinePattern::Dashed,
        effects: Effects {
            shaded: false,
            shadow_pt: Some(2.),
            ..Default::default()
        },
        ..Default::default()
    };
    let mut path = shape(5, GraphicKind::Curve, (20., 10.), shaded.clone());
    path.kind = GraphicKind::Path;
    // A drawn curve starts plain; shading applies once it is closed.
    path.style = shaded.clone();
    path.path = vec![
        PathCommand::Move(Point::new(150., 0.)),
        PathCommand::Line(Point::new(170., 0.)),
        PathCommand::Line(Point::new(160., 15.)),
        PathCommand::Close,
    ];
    (path.origin, path.axis_x, path.axis_y) =
        (Point::default(), Point::new(1., 0.), Point::new(0., 1.));
    let wavy = GraphicStyle {
        pattern: LinePattern::Wavy,
        ..Default::default()
    };
    let document = Document {
        graphics: vec![
            shape(1, GraphicKind::Rectangle, (20., 10.), shaded.clone()),
            shape(
                2,
                GraphicKind::RoundedRectangle,
                (20., 10.),
                shadowed.clone(),
            ),
            shape(3, GraphicKind::Ellipse, (20., 20.), shaded.clone()),
            shape(4, GraphicKind::Line, (20., 10.), wavy),
            path,
        ],
        arrows: vec![crate::document::Arrow::new(
            9,
            Point::new(0., 60.),
            Point::new(40., 60.),
            Preset::Forward,
            ArrowStyle {
                head: Head::None,
                pattern: LinePattern::Wavy,
                ..ArrowStyle::DEFAULT
            },
        )],
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
        let tree = roxmltree::Document::parse(&xml).unwrap();
        // CDX flags come back in bit order, so compare them as sets.
        let attribute = |tag: &str, key: &str| -> Vec<String> {
            tree.descendants()
                .filter(|n| n.has_tag_name(tag))
                .filter_map(|n| {
                    let mut words: Vec<_> = n.attribute(key)?.split_whitespace().collect();
                    words.sort();
                    Some(words.join(" "))
                })
                .collect()
        };
        assert_eq!(
            attribute("graphic", "RectangleType"),
            ["Shaded", "Dashed RoundEdge Shadow"],
            "{format}"
        );
        assert_eq!(
            attribute("graphic", "OvalType"),
            ["Circle Shaded"],
            "{format}"
        );
        assert_eq!(attribute("graphic", "ShadowSize"), ["200"], "{format}");
        assert_eq!(attribute("graphic", "LineType"), ["Wavy"], "{format}");
        assert_eq!(attribute("arrow", "LineType"), ["Wavy"], "{format}");
        assert!(
            attribute("curve", "FillType").contains(&"Shaded".into()),
            "{format}"
        );

        let back = crate::formats::read(bytes, format).await.unwrap();
        let effects = |kind: GraphicKind| {
            back.graphics
                .iter()
                .find(|g| g.kind == kind)
                .map(|g| (g.style.effects, g.style.pattern))
        };
        assert_eq!(
            effects(GraphicKind::Rectangle),
            Some((shaded.effects, LinePattern::Solid)),
            "{format}"
        );
        assert_eq!(
            effects(GraphicKind::RoundedRectangle),
            Some((shadowed.effects, LinePattern::Dashed)),
            "{format}"
        );
        assert_eq!(
            effects(GraphicKind::Ellipse).map(|e| e.0),
            Some(shaded.effects),
            "{format}"
        );
        assert_eq!(
            effects(GraphicKind::Line).map(|e| e.1),
            Some(LinePattern::Wavy),
            "{format}"
        );
        assert!(
            effects(GraphicKind::Path).is_some_and(|e| e.0.shaded),
            "{format}"
        );
        // A wavy arrow without heads comes back as a wavy line.
        assert!(back.arrows.is_empty(), "{format}");
        assert_eq!(
            back.graphics
                .iter()
                .filter(|g| g.kind == GraphicKind::Line && g.style.pattern == LinePattern::Wavy)
                .count(),
            2,
            "{format}"
        );
        // The shadow is painted, so the page leaves room for it.
        assert_eq!(back.file_version(), crate::document::DRAWING_ELEMENTS);
    }
}

#[tokio::test]
async fn an_outline_in_another_color_follows_the_native_shape_and_wavy_lines_keep_their_start() {
    let style = GraphicStyle {
        stroke: crate::palette::Color::Ink,
        fill: Some(crate::palette::Color::Custom([0, 0, 255])),
        effects: Effects {
            shaded: true,
            shadow_pt: Some(4.),
            ..Default::default()
        },
        ..Default::default()
    };
    let mut line = shape(
        2,
        GraphicKind::Line,
        (30., 0.),
        GraphicStyle {
            pattern: LinePattern::Wavy,
            ..Default::default()
        },
    );
    // Heads switched off keep their settings but draw nothing.
    line.heads = Some(Default::default());
    let document = Document {
        graphics: vec![
            shape(1, GraphicKind::Ellipse, (20., 10.), style),
            line.clone(),
        ],
        ..Default::default()
    };
    let bytes = crate::formats::write(document, "cdxml").await.unwrap();
    let xml = String::from_utf8(bytes.clone()).unwrap();
    let tree = roxmltree::Document::parse(&xml).unwrap();
    let oval = tree
        .descendants()
        .find(|n| n.attribute("OvalType").is_some())
        .unwrap();
    let group = oval.parent().unwrap();
    assert!(group.has_tag_name("group"));
    assert!(
        group.children().any(|n| n.has_tag_name("curve")),
        "the black outline"
    );
    let back = crate::formats::read(bytes, "cdxml").await.unwrap();
    let oval = back
        .graphics
        .iter()
        .find(|g| g.kind == GraphicKind::Ellipse)
        .unwrap();
    assert_eq!(oval.style.fill.map(|c| c.rgb()), Some([0, 0, 255]));
    assert!(oval.style.effects.shaded);
    let wavy = back
        .graphics
        .iter()
        .find(|g| g.kind == GraphicKind::Line)
        .unwrap();
    assert_eq!(wavy.style.pattern, LinePattern::Wavy);
    // Same start and direction, so the waves fall on the same side.
    let direction = |g: &Graphic| g.axis_x.x.signum();
    assert_eq!(direction(wavy), direction(&line));
}

#[tokio::test]
async fn a_corner_too_large_for_cdx_exports_as_curves() {
    let mut big = shape(
        1,
        GraphicKind::RoundedRectangle,
        (2400., 2400.),
        GraphicStyle {
            corner_pt: Some(400.),
            effects: Effects {
                shaded: false,
                shadow_pt: Some(4.),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    big.style.fill = None;
    let document = Document {
        graphics: vec![big],
        ..Default::default()
    };
    let bytes = crate::formats::write(document, "cdx").await.unwrap();
    let xml = crate::exchange::from_cdx(&bytes).unwrap();
    assert!(!xml.contains("CornerRadius"));
    assert!(xml.contains("<curve"));
}

#[tokio::test]
async fn lines_without_heads_leave_as_chemdraw_lines_and_come_back_as_lines() {
    let dashed = GraphicStyle {
        pattern: LinePattern::Dashed,
        width_pt: 2.,
        ..Default::default()
    };
    let document = Document {
        graphics: vec![
            shape(1, GraphicKind::Line, (30., 10.), GraphicStyle::default()),
            shape(2, GraphicKind::Line, (-20., 5.), dashed),
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
        let tree = roxmltree::Document::parse(&xml).unwrap();
        let types: Vec<_> = tree
            .descendants()
            .filter(|n| n.attribute("GraphicType") == Some("Line"))
            .map(|n| n.attribute("LineType").unwrap_or("Solid"))
            .collect();
        assert_eq!(types, ["Solid", "Dashed"], "{format}");
        let back = crate::formats::read(bytes, format).await.unwrap();
        assert_eq!(back.graphics.len(), 2, "{format}");
        for (line, original) in back.graphics.iter().zip(&document.graphics) {
            assert_eq!(line.kind, GraphicKind::Line, "{format}");
            assert_eq!(line.style.pattern, original.style.pattern, "{format}");
            assert!((line.style.width_pt - original.style.width_pt).abs() < 0.01);
            // Same start and direction, wherever the page puts the drawing.
            assert!(line.axis_x.distance(original.axis_x) < 0.01, "{format}");
        }
        let offset = |g: &[Graphic]| {
            Point::new(g[1].origin.x - g[0].origin.x, g[1].origin.y - g[0].origin.y)
        };
        assert!(offset(&back.graphics).distance(offset(&document.graphics)) < 0.01);
    }
}
