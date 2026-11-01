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

fn curve_xml(points: &str, attributes: &str) -> String {
    format!(
        "<CDXML BondLength='30'><page><curve ArrowheadHead='Full' CurvePoints='{points}' {attributes}/></page></CDXML>"
    )
}

const CUBIC: &str = "0 0 0 0 0 -20 40 20 40 0 40 0";
const ELBOW: &str = "0 0 0 0 10 0 20 0 30 0 30 10 30 20 30 30 30 30";
const SPLINE: &str = "0 0 0 0 0 -20 20 20 30 0 40 -20 60 20 60 0 60 0";

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
async fn path_heads_use_arrow_import_ranges_and_reject_closed_or_filled_splines() {
    for attribute in [
        "HeadSize='49'",
        "HeadSize='2401'",
        "ArrowheadWidth='24'",
        "ArrowheadWidth='1601'",
        "ArrowheadCenterSize='-1'",
        "ArrowheadCenterSize='1001'",
        "ArrowheadCenterSize='99'",
        "ArrowheadType='unsupported'",
    ] {
        let mut messages = vec![];
        for points in [CUBIC, SPLINE] {
            messages.push(
                formats::read(curve_xml(points, attribute).into_bytes(), "cdxml")
                    .await
                    .unwrap_err(),
            );
        }
        assert_eq!(messages[0], messages[1], "{attribute}");
    }
    for attributes in [
        "Closed='yes'",
        "FillType='Solid'",
        "CurveType='1'",
        "Dipole='yes'",
        "NoGo='Cross'",
    ] {
        assert!(
            formats::read(curve_xml(SPLINE, attributes).into_bytes(), "cdxml")
                .await
                .is_err(),
            "{attributes}"
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
