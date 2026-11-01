use super::*;

fn substituted_double_xml(position: &str) -> String {
    format!(
        "<CDXML BondLength='30'><page><fragment id='1'>\
         <n id='2' p='0 0'/><n id='3' p='25.9808 -15'/>\
         <n id='4' p='51.9616 0'/><n id='5' p='77.9423 -15'/>\
         <b id='6' B='2' E='3'/><b id='7' B='3' E='4' Order='2' {position}/>\
         <b id='8' B='4' E='5'/></fragment></page></CDXML>"
    )
}

#[test]
fn omitted_cdxml_double_position_stays_auto_and_uses_a_chain_offset() {
    use crate::bonds::DoublePosition;

    let imported = import_cdxml(&substituted_double_xml("")).unwrap();
    let doc = &imported.document;
    let bond = doc.bonds.iter().find(|bond| bond.order == 2).unwrap();
    assert_eq!(bond.double_position, DoublePosition::Auto);
    assert!(matches!(
        crate::scene::effective_double_position(doc, bond),
        DoublePosition::Left | DoublePosition::Right
    ));
    let request = crate::engine::Request::molecule("export", doc.clone());
    let exported = crate::exchange::drawing::write(doc, (&request).into()).unwrap();
    let xml = roxmltree::Document::parse(&exported).unwrap();
    let double = xml
        .descendants()
        .find(|node| node.has_tag_name("b") && node.attribute("Order") == Some("2"))
        .unwrap();
    assert_eq!(double.attribute("DoublePosition"), None);
    let reopened = import_cdxml(&exported).unwrap().document;
    let bond = reopened.bonds.iter().find(|bond| bond.order == 2).unwrap();
    assert_eq!(bond.double_position, DoublePosition::Auto);
    assert!(matches!(
        crate::scene::effective_double_position(&reopened, bond),
        DoublePosition::Left | DoublePosition::Right
    ));
}

#[test]
fn omitted_cdxml_terminal_double_position_offsets_carbon_and_centers_heteroatoms() {
    use crate::bonds::DoublePosition;

    let xml = substituted_double_xml("").replace("<b id='8' B='4' E='5'/>", "");
    for (element, expected) in [
        ("6", DoublePosition::Right),
        ("8", DoublePosition::Center),
        ("7", DoublePosition::Center),
    ] {
        let xml = xml.replace(
            "<n id='4' p='51.9616 0'/>",
            &format!("<n id='4' p='51.9616 0' Element='{element}'/>"),
        );
        let imported = import_cdxml(&xml).unwrap();
        let doc = &imported.document;
        let bond = doc.bonds.iter().find(|bond| bond.order == 2).unwrap();
        assert_eq!(bond.double_position, DoublePosition::Auto);
        assert_eq!(
            crate::scene::effective_double_position(doc, bond),
            expected,
            "terminal element {element}"
        );
    }
}

#[test]
fn chemdraw_acs1996_auto_chain_doubles_draw_the_observed_second_line_side() {
    use crate::{bonds::DoublePosition, document::Point, graphics::PathCommand, scene::Primitive};
    use std::collections::HashMap;

    const SOURCE: &str =
        include_str!("../../../../../tests/fixtures/chemdraw-acs1996-chain-alkenes.cdxml");
    // These signs were observed in ChemDraw 26.1's ACS1996 rendering, relative
    // to the original CDXML B -> E direction in its screen (Y-down) coordinates.
    // Zero denotes a centered pair, with neither rail on the atom-to-atom axis.
    const CASES: [(&str, &str, &str, f32); 9] = [
        ("7", "2", "3", -1.),
        ("17", "13", "14", -1.),
        ("26", "22", "23", 1.),
        ("34", "29", "30", -1.),
        ("36", "31", "5", 0.),
        ("61", "46", "47", -1.),
        ("80", "73", "75", -1.),
        ("91", "85", "86", -1.),
        ("93", "87", "88", -1.),
    ];

    // Slice the actual painted rails halfway along B -> E. Centerlines may be
    // joined polygons collected into a compound filled path, while second
    // rails remain Line primitives. Measure both rather than infer ink from
    // DoublePosition or rely on primitive order / imported bond indices.
    fn rail_centers(drawing: &[Primitive], a: Point, b: Point, spacing: f32) -> Vec<f32> {
        let length = a.distance(b);
        let (ux, uy) = ((b.x - a.x) / length, (b.y - a.y) / length);
        let along = |p: Point| (p.x - a.x) * ux + (p.y - a.y) * uy - length / 2.;
        let across = |p: Point| ux * (p.y - a.y) - uy * (p.x - a.x);
        let intersection = |p: Point, q: Point| {
            let (from, to) = (along(p), along(q));
            let delta = to - from;
            if delta.abs() < 0.001 {
                return None;
            }
            let t = -from / delta;
            (0. ..=1.).contains(&t).then(|| across(p.lerp(q, t)))
        };
        let polygon_center = |points: &[Point]| {
            let mut intersections: Vec<_> = points
                .iter()
                .zip(points.iter().cycle().skip(1))
                .filter_map(|(&p, &q)| intersection(p, q))
                .collect();
            intersections.sort_by(f32::total_cmp);
            let (&first, &last) = intersections.first().zip(intersections.last())?;
            // Exclude unrelated large filled shapes crossing this slice.
            (last - first < spacing / 2.).then_some((first + last) / 2.)
        };
        let mut centers = Vec::new();
        for primitive in drawing {
            match primitive {
                Primitive::Line(p, q, _) => {
                    if (across(*q) - across(*p)).abs() < 0.001
                        && let Some(center) = intersection(*p, *q)
                    {
                        centers.push(center);
                    }
                }
                Primitive::Polygon(points) => centers.extend(polygon_center(points)),
                Primitive::Path {
                    commands,
                    filled: true,
                    ..
                } => {
                    let mut points = Vec::new();
                    for command in commands {
                        match command {
                            PathCommand::Move(p) => {
                                points.clear();
                                points.push(*p);
                            }
                            PathCommand::Line(p) => points.push(*p),
                            PathCommand::Close => {
                                centers.extend(polygon_center(&points));
                                points.clear();
                            }
                            _ => points.clear(),
                        }
                    }
                }
                _ => {}
            }
        }
        centers.retain(|center| center.abs() <= spacing * 1.1);
        centers.sort_by(f32::total_cmp);
        centers
    }

    let xml = roxmltree::Document::parse_with_options(
        SOURCE,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        xml.root_element().attribute("CreationProgram"),
        Some("ChemDraw 26.1.0.6801")
    );
    let doc = import_cdxml(SOURCE).unwrap().document;
    assert!(!doc.drawing_style.center_chain_double_bonds);
    let scale = 42. / 14.4;
    let source_atoms: HashMap<_, _> = xml
        .descendants()
        .filter(|node| node.has_tag_name("n"))
        .map(|node| {
            let xy: Vec<f32> = node
                .attribute("p")
                .unwrap()
                .split_whitespace()
                .map(|value| value.parse().unwrap())
                .collect();
            let expected = Point::new(xy[0] * scale, xy[1] * scale);
            let matches: Vec<_> = doc
                .atoms
                .iter()
                .filter(|atom| atom.position.distance(expected) < 0.001)
                .collect();
            assert_eq!(matches.len(), 1, "source atom {:?}", node.attribute("id"));
            // Original Y-down screen coordinates survive import: the chemical
            // graph's temporary Y inversion must not reverse this test's sign.
            assert_eq!(
                matches[0].element,
                if node.attribute("Element") == Some("8") {
                    "O"
                } else {
                    "C"
                }
            );
            (node.attribute("id").unwrap(), matches[0].id)
        })
        .collect();
    assert_eq!(source_atoms.len(), doc.atoms.len());
    let drawing = crate::scene::primitives(&doc);
    let spacing = doc.drawing_style.bond_length_world * doc.drawing_style.bond_spacing_ratio;
    for (source_id, begin, end, side) in CASES {
        let source_bond = xml
            .descendants()
            .find(|node| node.has_tag_name("b") && node.attribute("id") == Some(source_id))
            .unwrap();
        assert_eq!(source_bond.attribute("B"), Some(begin));
        assert_eq!(source_bond.attribute("E"), Some(end));
        assert_eq!(source_bond.attribute("DoublePosition"), None);
        let a = source_atoms[begin];
        let b = source_atoms[end];
        let (_, bond) = doc.bond_between(a, b).unwrap();
        assert_eq!((bond.a, bond.b), (a, b), "CDXML bond {source_id}");
        assert_eq!(bond.order, 2, "CDXML bond {source_id}");
        assert_eq!(bond.double_position, DoublePosition::Auto);
        let centers = rail_centers(
            &drawing,
            doc.atom(a).unwrap().position,
            doc.atom(b).unwrap().position,
            spacing,
        );
        let mut expected = if side == 0. {
            [-spacing / 2., spacing / 2.]
        } else {
            [0., side * spacing]
        };
        expected.sort_by(f32::total_cmp);
        assert_eq!(centers.len(), 2, "CDXML bond {source_id}: {centers:?}");
        for (actual, expected) in centers.iter().zip(expected) {
            assert!(
                (actual - expected).abs() < 0.01,
                "CDXML bond {source_id}: rail centers {centers:?}, expected {expected}"
            );
        }
    }
}

#[test]
fn explicit_cdxml_double_positions_survive_import_and_export() {
    use crate::bonds::DoublePosition;

    for (name, expected) in [
        ("Left", DoublePosition::Left),
        ("Right", DoublePosition::Right),
        ("Center", DoublePosition::Center),
    ] {
        let imported =
            import_cdxml(&substituted_double_xml(&format!("DoublePosition='{name}'"))).unwrap();
        let doc = &imported.document;
        let bond = doc.bonds.iter().find(|bond| bond.order == 2).unwrap();
        assert_eq!(bond.double_position, expected, "{name}");
        assert_eq!(crate::scene::effective_double_position(doc, bond), expected);
        let request = crate::engine::Request::molecule("export", doc.clone());
        let exported = crate::exchange::drawing::write(doc, (&request).into()).unwrap();
        let xml = roxmltree::Document::parse(&exported).unwrap();
        let double = xml
            .descendants()
            .find(|node| node.has_tag_name("b") && node.attribute("Order") == Some("2"))
            .unwrap();
        assert_eq!(double.attribute("DoublePosition"), Some(name));
        let reopened = import_cdxml(&exported).unwrap().document;
        let bond = reopened.bonds.iter().find(|bond| bond.order == 2).unwrap();
        assert_eq!(bond.double_position, expected, "{name}");
    }
}

#[test]
fn predicate_defaults_and_preparation_error_precedence_are_preserved() {
    for &(name, defaults) in PREDICATES {
        let xml = |value| {
            format!(
                "<CDXML><page id=\"1\"><fragment id=\"2\"><n id=\"3\" p=\"0 0\" {name}=\"{value}\"/></fragment></page></CDXML>"
            )
        };
        assert!(matches!(
            read(&xml("query")),
            Err(Error::Unsupported("query or reaction predicate"))
        ));
        let error = prepare_cdxml(&xml("query")).unwrap_err();
        assert_eq!(error.stage, PreparationStage::Validation);
        assert!(matches!(error.cause, PreparationCause::Predicate(value) if value == name));
        for &default in defaults {
            read(&xml(default)).unwrap();
            assert!(!matches!(
                prepare_cdxml(&xml(default)),
                Err(PreparationError {
                    cause: PreparationCause::Predicate(_),
                    ..
                })
            ));
        }
    }
    // Predicate priority follows the fixed table, not XML attribute order, and
    // precedes unsupported objects even when the object comes first.
    let xml =
        "<CDXML><page><unsupported/><n RxnChange=\"yes\" RingBondCount=\"2\"/></page></CDXML>";
    let error = prepare_cdxml(xml).unwrap_err();
    assert_eq!(error.stage, PreparationStage::Validation);
    assert!(matches!(
        error.cause,
        PreparationCause::Predicate("RingBondCount")
    ));
}
