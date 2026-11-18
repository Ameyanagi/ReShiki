use reshiki_io::{chemistry::cdxml, engine::LocalEngine, exchange, formats};
use reshiki_model::{
    document::{Document, Point},
    graphics::PathCommand,
};
use std::path::Path;

const PAPER_POINTS: [(f32, f32); 16] = [
    (10., 12.),
    (21., 15.),
    (38., 20.),
    (113., 47.),
    (119., 63.),
    (143., 51.),
    (55., 22.),
    (94., 27.),
    (62., 9.),
    (68., 42.),
    (49., 20.),
    (52., 23.),
    (46., 23.),
    (11., 35.),
    (18., 35.),
    (11., 40.),
];

fn fixture(name: &str, format: &str) -> Vec<u8> {
    let extension = if format == "cds" {
        "drawing.cds"
    } else {
        format
    };
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/chemdraw-clipboard-layout")
            .join(format!("{name}.{extension}")),
    )
    .unwrap()
}

fn check_geometry(document: &Document, source_frames: bool) {
    assert_eq!(document.atoms.len(), 6);
    assert_eq!(document.bonds.len(), 4);
    assert_eq!(document.arrows.len(), 1);
    assert_eq!(document.annotations.len(), 2);
    assert_eq!(
        document
            .atoms
            .iter()
            .map(|a| a.element.as_str())
            .collect::<Vec<_>>(),
        ["C", "C", "O", "C", "C", "N"]
    );
    let mut points = document
        .atoms
        .iter()
        .map(|a| a.position)
        .collect::<Vec<_>>();
    points.extend([document.arrows[0].start, document.arrows[0].end]);
    for text in ["NaOH", "rt, 5 h"] {
        points.push(
            document
                .annotations
                .iter()
                .find(|a| a.text == text)
                .unwrap()
                .position,
        );
    }
    if source_frames {
        assert_eq!(document.graphics.len(), 2);
        for kind in ["Plus", "Rectangle"] {
            let graphic = document
                .graphics
                .iter()
                .find(|g| g.kind.to_string() == kind)
                .unwrap();
            points.extend([
                graphic.origin,
                graphic.origin.offset(graphic.axis_x.x, graphic.axis_x.y),
                graphic.origin.offset(graphic.axis_y.x, graphic.axis_y.y),
            ]);
        }
    }
    let anchor = points[0];
    let scale = 42. / 14.4;
    for (index, (actual, &(x, y))) in points.iter().zip(&PAPER_POINTS).enumerate() {
        let expected = Point::new(
            (x - PAPER_POINTS[0].0) * scale,
            (y - PAPER_POINTS[0].1) * scale,
        );
        let relative = actual.offset(-anchor.x, -anchor.y);
        // This is below 0.00035 paper pt and covers signed16.16 plus f32
        // transport rounding. No fitting of scales, angles or components.
        assert!(
            relative.distance(expected) < 0.001,
            "point {index}: {relative:?} / {expected:?}"
        );
    }
}

fn graphic_segments(document: &Document) -> Vec<[Point; 2]> {
    let anchor = document.atoms[0].position;
    let mut segments = Vec::new();
    for graphic in &document.graphics {
        let (mut first, mut previous) = (None, None);
        for command in graphic.commands() {
            let point = match command {
                PathCommand::Move(point) => {
                    first = Some(point);
                    previous = Some(point);
                    continue;
                }
                PathCommand::Line(point) => point,
                PathCommand::Cubic(a, b, point) => {
                    // These straight stamps may be serialized as degenerate
                    // cubics, but must not acquire a bend or lose an edge.
                    assert!(a.distance(previous.unwrap()) < 0.001);
                    assert!(b.distance(point) < 0.001);
                    point
                }
                PathCommand::Close => first.unwrap(),
            };
            let start = previous.unwrap();
            if start.distance(point) > 0.001 {
                segments.push([
                    start.offset(-anchor.x, -anchor.y),
                    point.offset(-anchor.x, -anchor.y),
                ]);
            }
            previous = Some(point);
        }
    }
    assert_eq!(
        segments.len(),
        6,
        "two plus strokes and four rectangle edges"
    );
    segments
}

#[test]
fn missing_bond_length_defaults_only_the_detached_preparation_copy() {
    let text = String::from_utf8(fixture("missing-bond-length", "cdxml")).unwrap();
    let before = text.clone();
    let raw = cdxml::read(&text).unwrap();
    assert_eq!(raw.fragments[0].positions[0].x, 10. * 65536.);
    let prepared = cdxml::prepare_cdxml(&text).unwrap();
    assert_eq!(prepared.drawing_style.bond_length_pt, 30.);
    assert_eq!(text, before);
    let expanded = roxmltree::Document::parse(&prepared.expanded_xml).unwrap();
    assert_eq!(expanded.root_element().attribute("BondLength"), None);
    assert_eq!(
        expanded
            .descendants()
            .find(|n| n.has_tag_name("n"))
            .unwrap()
            .attribute("p"),
        Some("10 12")
    );
    // The public native-reader contract still supplies raw coordinates when
    // no native normalization length was supplied by its caller.
    assert_eq!(
        cdxml::read(&text).unwrap().fragments[0].positions,
        raw.fragments[0].positions
    );
}

#[tokio::test]
async fn foreign_drawings_keep_one_common_geometry_frame_through_editable_roundtrips() {
    let engine = LocalEngine::in_process();
    for name in ["valid-layout", "explicit-30", "missing-bond-length"] {
        for format in ["cdxml", "cdx", "cds"] {
            let source_format = if format == "cds" { "cdx" } else { format };
            let (document, warnings) =
                formats::read_with_warnings(fixture(name, format), source_format, &engine)
                    .await
                    .unwrap_or_else(|error| panic!("{name}.{format}: {error}"));
            assert!(warnings.is_empty(), "{name}.{format}: {warnings:?}");
            check_geometry(&document, true);
            let source_segments = graphic_segments(&document);
            for target in ["cdxml", "cdx", "cds"] {
                let bytes = if target == "cds" {
                    let xml = formats::write_with_engine(document.clone(), "cdxml", &engine)
                        .await
                        .unwrap();
                    exchange::to_cds(std::str::from_utf8(&xml).unwrap()).unwrap()
                } else {
                    formats::write_with_engine(document.clone(), target, &engine)
                        .await
                        .unwrap()
                };
                let (restored, warnings) = formats::read_with_warnings(
                    bytes,
                    if target == "cds" { "cdx" } else { target },
                    &engine,
                )
                .await
                .unwrap();
                assert!(
                    warnings.is_empty(),
                    "{name}.{format} → {target}: {warnings:?}"
                );
                check_geometry(&restored, false);
                let mut remaining = source_segments.clone();
                for [a, b] in graphic_segments(&restored) {
                    let matching = remaining.iter().position(|[c, d]| {
                        (a.distance(*c) < 0.001 && b.distance(*d) < 0.001)
                            || (a.distance(*d) < 0.001 && b.distance(*c) < 0.001)
                    }).unwrap_or_else(|| panic!("{name}.{format} → {target}: displaced graphic segment {a:?} / {b:?}"));
                    remaining.remove(matching);
                }
                assert!(remaining.is_empty());
            }
        }
    }
}

#[tokio::test]
async fn unsupported_rich_drawing_remains_an_atomic_rejection() {
    let engine = LocalEngine::in_process();
    for format in ["cdxml", "cdx", "cds"] {
        let error = formats::read_with_engine(
            fixture("unsupported-graphic", format),
            if format == "cds" { "cdx" } else { format },
            &engine,
        )
        .await
        .unwrap_err();
        assert_eq!(
            error,
            "Gradient-shaded CDXML orbitals are not supported yet"
        );
    }
}
