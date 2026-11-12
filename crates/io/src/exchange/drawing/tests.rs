use super::*;

#[tokio::test]
async fn multi_node_arrow_exports_all_control_triples_to_cdxml_and_cdx() {
    use crate::{
        arrows::{ArrowStyle, Preset},
        document::{Arrow, ArrowNode},
        graphics::PathNodeType,
    };

    let points = [
        Point::new(0., 0.),
        Point::new(0., 0.),
        Point::new(10., -15.),
        Point::new(20., -30.),
        Point::new(30., -25.),
        Point::new(40., -20.),
        Point::new(50., 5.),
        Point::new(60., 10.),
        Point::new(70., 15.),
        Point::new(80., -10.),
        Point::new(90., -5.),
        Point::new(100., 0.),
        Point::new(110., 15.),
        Point::new(120., 20.),
        Point::new(120., 20.),
    ];
    for (node_count, point_count) in [(2, 12), (3, 15)] {
        let mut arrow = Arrow::new(
            1,
            points[0],
            points[13],
            Preset::Curved,
            ArrowStyle::preset(Preset::Curved),
        );
        arrow.cubic = Some([points[2], points[12]]);
        arrow.nodes = points[3..12]
            .as_chunks::<3>()
            .0
            .iter()
            .take(node_count)
            .map(|&[arriving, point, leaving]| ArrowNode {
                point,
                controls: [arriving, leaving],
                kind: PathNodeType::Corner,
            })
            .collect();
        let document = Document {
            arrows: vec![arrow],
            ..Default::default()
        };
        let scale = f64::from(document.drawing_style.bond_length_pt)
            / f64::from(document.drawing_style.bond_length_world);
        let expected = points[..3 * (node_count + 1)].iter().chain(&points[12..]);
        for format in ["cdxml", "cdx"] {
            let bytes = crate::formats::write(document.clone(), format)
                .await
                .unwrap();
            let xml = if format == "cdx" {
                crate::exchange::from_cdx(&bytes).unwrap()
            } else {
                String::from_utf8(bytes).unwrap()
            };
            let tree = roxmltree::Document::parse(&xml).unwrap();
            assert!(!tree.descendants().any(|node| node.has_tag_name("arrow")));
            let curve = tree
                .descendants()
                .find(|node| node.has_tag_name("curve"))
                .unwrap();
            let coordinates: Vec<f64> = curve
                .attribute("CurvePoints")
                .unwrap()
                .split_whitespace()
                .map(|value| value.parse().unwrap())
                .collect();
            assert_eq!(coordinates.len(), 2 * point_count);
            assert_eq!(curve.attribute("Closed").unwrap_or("no"), "no");
            for (actual, expected) in coordinates.as_chunks::<2>().0.iter().zip(expected.clone()) {
                // Editable exchange translates the page and CDX quantizes points.
                assert!(
                    (actual[0] - coordinates[0] - f64::from(expected.x) * scale).abs() < 0.0001
                );
                assert!(
                    (actual[1] - coordinates[1] - f64::from(expected.y) * scale).abs() < 0.0001
                );
            }
        }
    }
}

#[tokio::test]
async fn retrosynthesis_arrows_roundtrip_through_cdxml_and_cdx() {
    use crate::{
        arrows::{ArrowStyle, Head, HeadShape, Preset},
        document::Arrow,
        graphics::LinePattern,
        grouping::Group,
        palette::Color,
    };

    for style in [
        None,
        Some(ArrowStyle {
            width_pt: 1.25,
            head_length_pt: 8.,
            head_width_pt: 3.,
            head_notch: 0.25,
            gap_pt: 3.25,
            pattern: LinePattern::Dashed,
            color: Color::Custom([32, 80, 145]),
            ..ArrowStyle::preset(Preset::Retro)
        }),
        Some(ArrowStyle {
            shape: HeadShape::Hollow,
            ..ArrowStyle::preset(Preset::Retro)
        }),
        Some(ArrowStyle {
            head: Head::Right,
            tail: Head::Full,
            shape: HeadShape::Solid,
            ..ArrowStyle::preset(Preset::Retro)
        }),
    ] {
        let mut document = Document::default();
        document.arrows.push(Arrow {
            id: 1,
            kind: Preset::Retro.kind().into(),
            start: Point::new(-30., 20.),
            end: Point::new(150., -40.),
            start_anchor: None,
            end_anchor: None,
            control: None,
            cubic: None,
            nodes: Vec::new(),
            style,
        });
        document.arrows.push(Arrow {
            id: 2,
            kind: "forward".into(),
            start: Point::new(-30., 80.),
            end: Point::new(150., 80.),
            style: None,
            ..document.arrows[0].clone()
        });
        document.groups.push(Group {
            id: 3,
            members: vec![1, 2],
            integral: true,
        });
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
            let arrow = tree
                .descendants()
                .find(|n| n.has_tag_name("arrow"))
                .unwrap();
            if document.arrows[0].style.is_none() {
                assert_eq!(arrow.attribute("ArrowheadHead"), Some("Full"));
                assert_eq!(arrow.attribute("ArrowheadTail"), Some("None"));
                assert_eq!(arrow.attribute("ArrowheadType"), Some("Angle"));
            }
            assert_eq!(
                arrow.attribute("ArrowShaftSpacing"),
                Some(if document.arrows[0].appearance().gap_pt == 3.25 {
                    "325"
                } else {
                    "140"
                })
            );
            let legacy = tree
                .descendants()
                .find(|n| n.has_tag_name("graphic"))
                .unwrap();
            assert_eq!(legacy.attribute("GraphicType"), Some("Line"));
            assert_eq!(legacy.attribute("ArrowType"), Some("RetroSynthetic"));
            assert_eq!(legacy.attribute("SupersededBy"), arrow.attribute("id"));
            for attribute in ["Z", "HeadSize", "color"] {
                assert_eq!(legacy.attribute(attribute), arrow.attribute(attribute));
            }
            let head = arrow
                .attribute("Head3D")
                .unwrap()
                .split_whitespace()
                .take(2);
            let tail = arrow
                .attribute("Tail3D")
                .unwrap()
                .split_whitespace()
                .take(2);
            assert_eq!(
                legacy
                    .attribute("BoundingBox")
                    .unwrap()
                    .split_whitespace()
                    .collect::<Vec<_>>(),
                head.chain(tail).collect::<Vec<_>>()
            );
            assert_eq!(legacy.parent(), arrow.parent());

            let restored = crate::formats::read(bytes, format).await.unwrap();
            assert_eq!(restored.arrows.len(), 2);
            assert!(restored.graphics.is_empty());
            assert_eq!(restored.groups.len(), 1);
            assert_eq!(
                restored.groups[0].members,
                restored.arrows.iter().map(|a| a.id).collect::<Vec<_>>()
            );
            let actual = &restored.arrows[0];
            let expected = &document.arrows[0];
            assert_eq!(actual.kind, "retro");
            let mut actual_style = actual.appearance();
            let expected_style = expected.appearance();
            assert!((actual_style.width_pt - expected_style.width_pt).abs() < 1. / 65536.);
            actual_style.width_pt = expected_style.width_pt;
            assert_eq!(actual_style, expected_style);
            // Editable exchange translates the page; preserve relative geometry.
            assert!((actual.end.x - actual.start.x - 180.).abs() < 0.001);
            assert!((actual.end.y - actual.start.y + 60.).abs() < 0.001);
            assert_eq!(actual.control, None);
            assert_eq!(actual.cubic, None);
        }
    }
}

#[test]
fn retrosynthesis_exchange_refuses_curves() {
    use crate::{
        arrows::{ArrowStyle, Preset},
        document::Arrow,
    };
    for (control, cubic) in [
        (Some(Point::new(60., -30.)), None),
        (Some(Point::new(180., 0.)), None),
        (None, Some([Point::new(30., -30.), Point::new(90., -30.)])),
        (None, Some([Point::new(40., 0.), Point::new(80., 0.)])),
    ] {
        let document = Document {
            arrows: vec![Arrow {
                id: 1,
                kind: "retro".into(),
                start: Point::default(),
                end: Point::new(120., 0.),
                start_anchor: None,
                end_anchor: None,
                control,
                cubic,
                nodes: Vec::new(),
                style: Some(ArrowStyle::preset(Preset::Retro)),
            }],
            ..Default::default()
        };
        let message = write(&document, Options::default())
            .unwrap_err()
            .to_string();
        assert!(message.contains("Curved retrosynthesis"), "{message}");
    }
}

#[tokio::test]
async fn straightened_retrosynthesis_exports_as_a_straight_arrow() {
    use crate::{
        arrows::{ArrowStyle, Preset},
        document::Arrow,
    };
    let mut arrow = Arrow::new(
        1,
        Point::new(-30., 20.),
        Point::new(150., -40.),
        Preset::Retro,
        ArrowStyle::preset(Preset::Retro),
    );
    arrow.control = Some(Point::new(60., -80.));
    arrow.straighten();
    assert_eq!(arrow.control, Some(arrow.start.lerp(arrow.end, 0.5)));
    for control in [
        arrow.control.unwrap(),
        arrow.start.lerp(arrow.end, 0.3).offset(0., 0.0005),
    ] {
        arrow.control = Some(control);
        let document = Document {
            arrows: vec![arrow.clone()],
            ..Default::default()
        };
        for format in ["cdxml", "cdx"] {
            let bytes = crate::formats::write(document.clone(), format)
                .await
                .unwrap();
            let restored = crate::formats::read(bytes, format).await.unwrap();
            assert_eq!(restored.arrows.len(), 1);
            assert!(restored.graphics.is_empty());
            let actual = &restored.arrows[0];
            assert_eq!(actual.kind, "retro");
            assert_eq!(actual.control, None);
            assert_eq!(actual.cubic, None);
            assert!((actual.end.x - actual.start.x - 180.).abs() < 0.001);
            assert!((actual.end.y - actual.start.y + 60.).abs() < 0.001);
        }
    }
}

fn text_document(label: &str) -> Document {
    let mut document = Document::default();
    let left = document.add_atom("*", Point::default());
    document.atom_mut(left).unwrap().display.variable = Some(label.into());
    let right = document.add_atom("C", Point::new(42., 0.));
    document.add_bond(left, right, 1, "plain");
    document
}

#[test]
fn strict_text_label_refusal_names_the_label_without_calling_the_drawing_invalid() {
    let document = text_document("MeO");
    let before = document.clone();
    for preserving in [false, true] {
        let error = write_impl(&document, Options::default(), preserving, false).unwrap_err();
        assert!(matches!(error, Error::Unsupported(_)));
        let message = error.to_string();
        assert!(message.contains("text label “MeO”"), "{message}");
        assert!(message.ends_with(TEXT_LABEL_COPY_ADVICE));
        assert!(!message.contains("Invalid") && !message.contains("ring"));
    }
    let (xml, _) = write_clipboard(&document).unwrap();
    assert!(xml.contains("MeO"), "Ordinary Copy still retains atom text");
    assert_eq!(document, before);
}

#[test]
fn strict_ring_curve_refusal_does_not_claim_a_text_label_is_present() {
    let mut document = crate::rings::Preset::Benzene.document(42., true);
    let selected = document
        .atoms
        .iter()
        .take(3)
        .map(|a| a.id)
        .collect::<Vec<_>>();
    crate::ring_arcs::toggle(&mut document, &selected).unwrap();
    let before = document.clone();
    let error = write(&document, Options::default()).unwrap_err();
    assert!(matches!(error, Error::Unsupported(_)));
    let message = error.to_string();
    assert!(message.contains("inner ring curves"), "{message}");
    assert!(!message.contains("text label") && !message.contains("Invalid"));
    assert_eq!(document, before);
}

#[test]
fn refusal_excerpts_are_bounded_and_cannot_inject_lines_quotes_or_direction_changes() {
    assert_eq!(label_excerpt("\t MeO\n  label\r"), "MeO label");
    assert_eq!(label_excerpt("“MeO”\u{202e}\"\u{0007}"), "'MeO''");
    assert_eq!(label_excerpt("\u{2067}\n\t"), "(empty)");
    let long = "酸".repeat(200);
    assert_eq!(label_excerpt(&long), format!("{}…", "酸".repeat(24)));
    let mut document = text_document(&"酸".repeat(32));
    for label in ["second", "third"] {
        let id = document.add_atom("*", Point::new(100., 0.));
        document.atom_mut(id).unwrap().display.variable = Some(label.into());
    }
    let message = write(&document, Options::default())
        .unwrap_err()
        .to_string();
    assert!(message.contains("and 2 more"), "{message}");
    assert!(message.chars().count() < 190, "{message}");
}

#[test]
fn malformed_documents_still_report_invalid() {
    let mut document = text_document("MeO");
    document.atoms[1].id = document.atoms[0].id;
    assert!(matches!(
        write(&document, Options::default()),
        Err(Error::Invalid(_))
    ));
}

#[test]
fn centered_aromatic_charge_export_retains_atom_charge_without_extra_center_text() {
    let mut doc = Document::default();
    let ids = crate::editing::ring(&mut doc, Point::default(), 7, false, 5.);
    doc.atom_mut(ids[0]).unwrap().charge = 1;
    for (i, bond) in doc.bonds.iter_mut().enumerate() {
        bond.order = if i % 2 == 1 { 2 } else { 1 };
    }
    let mut doc = crate::chemistry::document::aromatic_display(&doc, &ids)
        .unwrap()
        .finish()
        .unwrap()
        .document;
    assert!(crate::aromatic::toggle_center_charge(&mut doc, &ids).unwrap());
    let before = doc.clone();
    let xml = write(&doc, Options::default()).unwrap();
    let tree = roxmltree::Document::parse(&xml).unwrap();
    let charges: Vec<_> = tree
        .descendants()
        .filter(|node| node.has_tag_name("n"))
        .filter_map(|node| node.attribute("Charge"))
        .collect();
    assert_eq!(charges, ["1"]);
    assert!(!tree.descendants().any(|node| node.has_tag_name("t")));
    assert_eq!(doc, before);
}
