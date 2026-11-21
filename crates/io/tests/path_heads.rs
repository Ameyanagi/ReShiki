//! Endpoint decorations retain their editable path and exchange metadata (#762).
use reshiki_io::{exchange, export, formats};
use reshiki_model::{
    arrows::{Head, HeadShape},
    document::{Document, Point},
    graphics::{BracketSides, Graphic, GraphicKind, GraphicStyle, PathCommand, PathHeads},
    palette::Color,
};

fn drawing(heads: Option<PathHeads>) -> Document {
    let mut document = Document::default();
    let mut graphic = Graphic::pen_curve(
        1,
        Point::default(),
        Point::new(120., 20.),
        GraphicStyle {
            stroke: Color::Custom([180, 30, 80]),
            width_pt: 1.2,
            ..Default::default()
        },
    );
    graphic.path = vec![
        PathCommand::Move(Point::new(0., 0.)),
        PathCommand::Cubic(
            Point::new(0., -30.),
            Point::new(20., 25.),
            Point::new(40., 10.),
        ),
        PathCommand::Cubic(
            Point::new(65., -15.),
            Point::new(65., 55.),
            Point::new(80., 30.),
        ),
        PathCommand::Cubic(
            Point::new(95., 5.),
            Point::new(120., 50.),
            Point::new(120., 20.),
        ),
    ];
    graphic.heads = heads;
    graphic.node_types.clear();
    document.graphics.push(graphic);
    document
}

fn heads(head: Head, tail: Head, shape: HeadShape) -> PathHeads {
    PathHeads {
        head,
        tail,
        shape,
        head_length_pt: 8.,
        head_width_pt: 3.,
        head_notch: 0.25,
    }
}

#[tokio::test]
async fn native_path_heads_roundtrip_and_legacy_files_omit_the_key() {
    for decoration in [None, Some(heads(Head::Left, Head::Full, HeadShape::Hollow))] {
        let original = drawing(decoration);
        let bytes = formats::write(original.clone(), "rsk").await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            json["graphics"][0].get("heads").is_some(),
            decoration.is_some()
        );
        let restored = formats::read(bytes, "rsk").await.unwrap();
        assert_eq!(restored.graphics, original.graphics);
    }
}

#[tokio::test]
async fn multisegment_path_heads_roundtrip_as_one_curve_in_cdxml_and_cdx() {
    for decoration in [
        heads(Head::Full, Head::None, HeadShape::Solid),
        heads(Head::Left, Head::Right, HeadShape::Hollow),
        heads(Head::Right, Head::Full, HeadShape::Open),
    ] {
        let original = drawing(Some(decoration));
        for format in ["cdxml", "cdx"] {
            let bytes = formats::write(original.clone(), format).await.unwrap();
            let xml = if format == "cdx" {
                exchange::from_cdx(&bytes).unwrap()
            } else {
                String::from_utf8(bytes.clone()).unwrap()
            };
            let tree = roxmltree::Document::parse(&xml).unwrap();
            let curves: Vec<_> = tree
                .descendants()
                .filter(|n| n.has_tag_name("curve"))
                .collect();
            assert_eq!(curves.len(), 1, "{format}: {xml}");
            assert_eq!(curves[0].attribute("HeadSize"), Some("800"));
            assert_eq!(curves[0].attribute("ArrowheadCenterSize"), Some("600"));
            assert_eq!(curves[0].attribute("ArrowheadWidth"), Some("300"));
            assert_eq!(curves[0].attribute("Closed").unwrap_or("no"), "no");
            assert_eq!(curves[0].attribute("FillType"), Some("None"));
            assert_eq!(
                tree.descendants()
                    .filter(|n| n.has_tag_name("arrow"))
                    .count(),
                0
            );

            let restored = formats::read(bytes, format).await.unwrap();
            assert!(restored.arrows.is_empty());
            assert_eq!(restored.graphics.len(), 1);
            let graphic = &restored.graphics[0];
            assert_eq!(graphic.kind, GraphicKind::Path);
            assert_eq!(graphic.heads, Some(decoration));
            let style = &original.graphics[0].style;
            assert_eq!(graphic.style.stroke, style.stroke);
            assert_eq!(graphic.style.fill, style.fill);
            assert_eq!(graphic.style.pattern, style.pattern);
            assert!((graphic.style.width_pt - style.width_pt).abs() < 0.0001);
            let actual = graphic.commands();
            let expected = original.graphics[0].commands();
            assert_eq!(actual.len(), expected.len());
            let origin = |path: &[PathCommand]| path[0].points()[0];
            let relative = |p: Point, o: Point| Point::new(p.x - o.x, p.y - o.y);
            for (a, b) in actual.iter().zip(&expected) {
                assert_eq!(std::mem::discriminant(a), std::mem::discriminant(b));
                for (a, b) in a.points().into_iter().zip(b.points()) {
                    assert!(
                        relative(a, origin(&actual)).distance(relative(b, origin(&expected)))
                            < 0.001
                    );
                }
            }
        }
    }
}

#[tokio::test]
async fn disabled_path_heads_keep_settings_in_cdxml_and_cdx() {
    let decoration = PathHeads {
        head_length_pt: 9.,
        ..heads(Head::None, Head::None, HeadShape::Hollow)
    };
    let original = drawing(Some(decoration));
    for format in ["cdxml", "cdx"] {
        let bytes = formats::write(original.clone(), format).await.unwrap();
        let restored = formats::read(bytes, format).await.unwrap();
        assert!(restored.arrows.is_empty());
        assert_eq!(restored.graphics.len(), 1);
        assert_eq!(restored.graphics[0].heads, Some(decoration), "{format}");
    }
}

#[tokio::test]
async fn path_heads_roundtrip_at_the_notch_limit_in_cdxml_and_cdx() {
    for (length, rounded_length, notch) in [
        (0.5, 0.5, 0.9),
        (0.54, 0.54, 1. - 6. / 54.),
        (0.549, 0.55, 1. - 6. / 55.),
        (24., 24., 0.9),
    ] {
        let decoration = PathHeads {
            head_length_pt: length,
            head_notch: 0.9,
            ..heads(Head::Full, Head::None, HeadShape::Hollow)
        };
        let original = drawing(Some(decoration));
        for format in ["cdxml", "cdx"] {
            let bytes = formats::write(original.clone(), format).await.unwrap();
            let restored = formats::read(bytes, format).await.unwrap();
            assert!(restored.arrows.is_empty());
            assert_eq!(restored.graphics.len(), 1);
            let actual = restored.graphics[0].heads.unwrap();
            actual.validate().unwrap();
            assert_eq!(actual.head, decoration.head);
            assert_eq!(actual.tail, decoration.tail);
            assert_eq!(actual.shape, decoration.shape);
            assert_eq!(actual.head_length_pt, rounded_length);
            assert_eq!(actual.head_width_pt, decoration.head_width_pt);
            assert!((actual.head_notch - notch).abs() < 0.0001);
        }
    }
}

fn curve_xml(points: &str, attributes: &str) -> String {
    format!(
        "<CDXML BondLength='30'><page><curve ArrowheadHead='Full' CurvePoints='{points}' {attributes}/></page></CDXML>"
    )
}

const CUBIC: &str = "0 0 0 0 0 -20 40 20 40 0 40 0";
const ELBOW: &str = "0 0 0 0 10 0 20 0 30 0 30 10 30 20 30 30 30 30";
const SPLINE: &str = "0 0 0 0 0 -20 20 20 30 0 40 -20 60 20 60 0 60 0";

#[tokio::test]
async fn curves_keep_explicit_head_settings_without_visible_heads() {
    for endpoints in [
        "",
        "ArrowheadHead='None' ArrowheadTail='None'",
        "ArrowheadHead='Unspecified' ArrowheadTail='Unspecified'",
    ] {
        for attributes in ["", "Closed='yes'", "FillType='Solid'"] {
            let xml = format!(
                "<CDXML BondLength='30'><page><curve CurvePoints='{SPLINE}' {endpoints} ArrowheadType='Hollow' HeadSize='900' {attributes}/></page></CDXML>"
            );
            let document = formats::read(xml.into_bytes(), "cdxml").await.unwrap();
            assert!(document.arrows.is_empty());
            assert_eq!(document.graphics.len(), 1);
            assert_eq!(
                document.graphics[0].heads,
                Some(PathHeads {
                    shape: HeadShape::Hollow,
                    head_length_pt: 9.,
                    head_width_pt: 2.5,
                    head_notch: 0.,
                    ..Default::default()
                })
            );
        }
    }
}

#[tokio::test]
async fn cdxml_keeps_legacy_cubic_and_elbow_arrows_and_imports_curved_splines_as_paths() {
    for (points, kind) in [(CUBIC, "curved"), (ELBOW, "bent"), (SPLINE, "path")] {
        let document = formats::read(curve_xml(points, "").into_bytes(), "cdxml")
            .await
            .unwrap();
        if kind == "path" {
            assert!(document.arrows.is_empty());
            assert_eq!(document.graphics.len(), 1);
            let graphic = &document.graphics[0];
            assert_eq!(graphic.path.len(), 3);
            assert_eq!(
                graphic.heads,
                Some(PathHeads {
                    head: Head::Full,
                    tail: Head::None,
                    shape: HeadShape::Solid,
                    head_length_pt: 10.,
                    head_width_pt: 2.5,
                    head_notch: 0.,
                })
            );
        } else {
            assert!(document.graphics.is_empty());
            assert_eq!(document.arrows.len(), 1);
            assert_eq!(document.arrows[0].kind, kind);
        }
    }
}

#[tokio::test]
async fn path_heads_use_arrow_import_ranges_and_plain_closed_or_filled_splines() {
    let engine = reshiki_io::engine::LocalEngine::in_process();
    let read = |points: &'static str, attributes: &'static str| {
        formats::read_with_warnings(curve_xml(points, attributes).into_bytes(), "cdxml", &engine)
    };
    // A head size outside the range arrows take imports at the nearest one.
    for (attribute, length, width, notch) in [
        ("HeadSize='49'", 0.5, 2.5, 0.),
        ("HeadSize='2401'", 24., 2.5, 0.),
        ("ArrowheadWidth='24'", 10., 0.25, 0.),
        ("ArrowheadWidth='1601'", 10., 16., 0.),
        ("ArrowheadCenterSize='-1'", 10., 2.5, 0.9),
        ("ArrowheadCenterSize='1001'", 10., 2.5, 0.),
        ("ArrowheadCenterSize='99'", 10., 2.5, 0.9),
    ] {
        let (arrow, warnings) = read(CUBIC, attribute).await.unwrap();
        let (path, path_warnings) = read(SPLINE, attribute).await.unwrap();
        assert_eq!(warnings, path_warnings, "{attribute}");
        assert!(
            warnings[0]
                .contains("1 line, arrowhead or shadow size at the nearest size ReShiki draws"),
            "{attribute}: {warnings:?}"
        );
        let style = arrow.arrows[0].style.clone().unwrap();
        let heads = path.graphics[0].heads.unwrap();
        for (name, expected, values) in [
            (
                "length",
                length,
                [style.head_length_pt, heads.head_length_pt],
            ),
            ("width", width, [style.head_width_pt, heads.head_width_pt]),
            ("notch", notch, [style.head_notch, heads.head_notch]),
        ] {
            for value in values {
                assert!(
                    (value - expected).abs() < 1e-4,
                    "{attribute} {name}: {value}"
                );
            }
        }
    }
    // A head ChemDraw does not have is still refused, the same way for both.
    let unknown = "ArrowheadType='unsupported'";
    assert_eq!(
        read(CUBIC, unknown).await.unwrap_err(),
        read(SPLINE, unknown).await.unwrap_err()
    );
    // A closed or filled spline has no heads; a longer spline has no marks.
    for (attributes, notice, heads) in [
        (
            "Closed='yes'",
            "1 closed or filled curve without arrowheads",
            false,
        ),
        (
            "FillType='Solid'",
            "1 closed or filled curve without arrowheads",
            false,
        ),
        (
            "CurveType='1'",
            "1 closed or filled curve without arrowheads",
            false,
        ),
        (
            "Dipole='yes'",
            "1 curved arrow without the double shaft, no-go or dipole mark",
            true,
        ),
        (
            "NoGo='Cross'",
            "1 curved arrow without the double shaft, no-go or dipole mark",
            true,
        ),
    ] {
        let (document, warnings) = read(SPLINE, attributes).await.unwrap();
        assert!(warnings[0].contains(notice), "{attributes}: {warnings:?}");
        assert_eq!(document.graphics[0].heads.is_some(), heads, "{attributes}");
    }
    // A one-segment curved arrow and a bent arrow keep their marks.
    for points in [CUBIC, ELBOW] {
        let (document, warnings) = read(points, "NoGo='Cross'").await.unwrap();
        assert_eq!(warnings, Vec::<String>::new());
        assert_eq!(
            document.arrows[0].style.as_ref().unwrap().no_go,
            reshiki_model::arrows::NoGo::Cross
        );
    }
}

#[tokio::test]
async fn parametric_curve_heads_export_on_the_editable_curve() {
    let mut document = Document::default();
    let mut graphic = Graphic::dragged(
        1,
        GraphicKind::Curve,
        Point::default(),
        Point::new(80., 20.),
        GraphicStyle::default(),
        BracketSides::Both,
        false,
    );
    graphic.heads = Some(heads(Head::Left, Head::Right, HeadShape::Open));
    document.graphics.push(graphic);
    let bytes = formats::write(document, "cdxml").await.unwrap();
    let xml = String::from_utf8(bytes.clone()).unwrap();
    let tree = roxmltree::Document::parse(&xml).unwrap();
    let curves: Vec<_> = tree
        .descendants()
        .filter(|n| n.has_tag_name("curve"))
        .collect();
    assert_eq!(curves.len(), 1);
    assert_eq!(curves[0].attribute("ArrowheadHead"), Some("HalfLeft"));
    assert_eq!(curves[0].attribute("ArrowheadTail"), Some("HalfRight"));
    assert_eq!(curves[0].attribute("ArrowheadType"), Some("Angle"));
    // Curved arrows exchange only solid heads, so angled ones come back as a path.
    let restored = formats::read(bytes, "cdxml").await.unwrap();
    assert!(restored.arrows.is_empty());
    assert_eq!(
        restored.graphics[0].heads,
        Some(heads(Head::Left, Head::Right, HeadShape::Open))
    );
    formats::write(restored, "cdxml").await.unwrap();
}

#[tokio::test]
async fn closed_and_compound_paths_export_without_endpoint_decorations() {
    for compound in [false, true] {
        let mut document = drawing(Some(heads(Head::Full, Head::Full, HeadShape::Solid)));
        if compound {
            document.graphics[0].path.extend([
                PathCommand::Move(Point::new(140., 0.)),
                PathCommand::Line(Point::new(180., 20.)),
            ]);
        } else {
            document.graphics[0].path.push(PathCommand::Close);
        }
        let bytes = formats::write(document, "cdxml").await.unwrap();
        let xml = String::from_utf8(bytes).unwrap();
        let tree = roxmltree::Document::parse(&xml).unwrap();
        let curves: Vec<_> = tree
            .descendants()
            .filter(|n| n.has_tag_name("curve"))
            .collect();
        assert_eq!(curves.len(), if compound { 2 } else { 1 });
        for curve in curves {
            assert_eq!(
                curve.attribute("Closed"),
                Some(if compound { "no" } else { "yes" })
            );
            assert!(curve.attribute("ArrowheadHead").is_none());
            assert!(curve.attribute("ArrowheadTail").is_none());
        }
    }
}

#[test]
fn svg_export_draws_both_path_heads() {
    for (decoration, count) in [
        (None, 1),
        (Some(heads(Head::Full, Head::Full, HeadShape::Solid)), 3),
    ] {
        let bytes = export::drawing(&drawing(decoration), "svg").unwrap();
        let xml = std::str::from_utf8(&bytes).unwrap();
        let tree = roxmltree::Document::parse(xml).unwrap();
        let paths: Vec<_> = tree
            .descendants()
            .filter(|n| n.has_tag_name("path"))
            .collect();
        assert_eq!(paths.len(), count);
        assert_eq!(
            paths
                .iter()
                .filter(|n| n.attribute("fill") != Some("none"))
                .count(),
            count - 1
        );
    }
}
