use super::*;
use crate::{
    arrows::{ArrowStyle, Body, Preset},
    document::Arrow,
};

fn arrow(body: Body) -> Arrow {
    let mut style = ArrowStyle::preset(Preset::Forward);
    style.set_body(body);
    Arrow::new(
        1,
        Point::new(0., 0.),
        Point::new(100., 0.),
        Preset::Forward,
        style,
    )
}

async fn exported(document: &Document, format: &str) -> String {
    let bytes = crate::formats::write(document.clone(), format)
        .await
        .unwrap();
    if format == "cdx" {
        crate::exchange::from_cdx(&bytes).unwrap()
    } else {
        String::from_utf8(bytes).unwrap()
    }
}

#[tokio::test]
async fn block_hollow_and_gradient_bodies_export_as_chemdraw_block_arrows() {
    for (body, fill) in [
        (Body::Block, "Solid"),
        (Body::Hollow, "None"),
        (Body::Gradient, "Shaded"),
    ] {
        let document = Document {
            arrows: vec![arrow(body)],
            ..Default::default()
        };
        for format in ["cdxml", "cdx"] {
            let xml = exported(&document, format).await;
            let tree = roxmltree::Document::parse(&xml).unwrap();
            let node = tree
                .descendants()
                .find(|n| n.has_tag_name("arrow"))
                .unwrap();
            assert_eq!(node.attribute("FillType"), Some(fill), "{body} {format}");
            assert_eq!(node.attribute("ArrowheadType"), Some("Hollow"));
            // Hundredths of the line width: a 5 pt body, a head 1 pt beyond
            // its edge and 8 pt long; a filled body draws with a 1 pt line.
            let (spacing, beyond, head) = if body == Body::Hollow {
                ("833", "167", "1333")
            } else {
                ("500", "100", "800")
            };
            assert_eq!(node.attribute("ArrowShaftSpacing"), Some(spacing), "{body}");
            assert_eq!(node.attribute("ArrowheadWidth"), Some(beyond));
            assert_eq!(node.attribute("HeadSize"), Some(head));
        }
    }
}

#[tokio::test]
async fn arc_arrows_export_as_chemdraw_arcs_running_the_same_way() {
    for sweep in [90_f32, -180., 270.] {
        let mut a = arrow(Body::Line);
        a.set_arc(sweep);
        let document = Document {
            arrows: vec![a.clone()],
            ..Default::default()
        };
        let xml = exported(&document, "cdxml").await;
        let tree = roxmltree::Document::parse(&xml).unwrap();
        let node = tree
            .descendants()
            .find(|n| n.has_tag_name("arrow"))
            .unwrap();
        let read = |key: &str| -> Vec<f64> {
            node.attribute(key)
                .unwrap()
                .split_whitespace()
                .map(|v| v.parse().unwrap())
                .collect()
        };
        // A clockwise sweep is a negative ChemDraw size.
        let size = read("AngularSize")[0];
        assert_eq!(size, f64::from(-sweep));
        let (c, m, n, tail, head) = (
            read("Center3D"),
            read("MajorAxisEnd3D"),
            read("MinorAxisEnd3D"),
            read("Tail3D"),
            read("Head3D"),
        );
        // ChemDraw walks from the tail through -AngularSize in its frame.
        let (u, v) = ([m[0] - c[0], m[1] - c[1]], [n[0] - c[0], n[1] - c[1]]);
        let r2 = u[0] * u[0] + u[1] * u[1];
        let angle = |p: &[f64]| {
            let w = [p[0] - c[0], p[1] - c[1]];
            ((w[0] * v[0] + w[1] * v[1]) / r2).atan2((w[0] * u[0] + w[1] * u[1]) / r2)
        };
        let end = angle(&tail) - size.to_radians();
        let expected = angle(&head);
        let difference = (end - expected).rem_euclid(std::f64::consts::TAU);
        assert!(
            difference < 1e-3 || difference > std::f64::consts::TAU - 1e-3,
            "{sweep}"
        );
        // The middle of ChemDraw's arc is where ReShiki draws it.
        let middle = angle(&tail) - size.to_radians() * 0.5;
        let scale = f64::from(document.drawing_style.bond_length_pt)
            / f64::from(document.drawing_style.bond_length_world);
        let drawn = a.point(0.5);
        let shown = [
            c[0] + u[0] * middle.cos() + v[0] * middle.sin(),
            c[1] + u[1] * middle.cos() + v[1] * middle.sin(),
        ];
        let start = a.start;
        let first = [tail[0], tail[1]];
        assert!(
            ((shown[0] - first[0]) - f64::from(drawn.x - start.x) * scale).abs() < 0.05
                && ((shown[1] - first[1]) - f64::from(drawn.y - start.y) * scale).abs() < 0.05,
            "{sweep}"
        );
    }
}

#[tokio::test]
async fn chevrons_and_callouts_travel_as_grouped_shapes_with_a_notice() {
    let mut line = arrow(Body::Line);
    line.id = 2;
    line.start = Point::new(0., 60.);
    line.end = Point::new(100., 60.);
    let mut document = Document {
        arrows: vec![arrow(Body::Chevron), line],
        ..Default::default()
    };
    let a = document.add_atom("C", Point::new(0., 150.));
    let b = document.add_atom("C", Point::new(20., 150.));
    document.add_bond(a, b, 1, "plain");
    crate::leaders::add_callout(
        &mut document,
        crate::leaders::Target::Bond {
            a,
            b,
            fraction: 0.5,
        },
        "New bond",
    )
    .unwrap();
    document.validate().unwrap();
    let notice = crate::arrows::export_notice(&document).unwrap();
    assert!(
        notice.contains("1 chevron or curved block arrow"),
        "{notice}"
    );
    assert!(notice.contains("1 callout leader"), "{notice}");
    for format in ["cdxml", "cdx"] {
        let xml = exported(&document, format).await;
        let tree = roxmltree::Document::parse(&xml).unwrap();
        // Only the line arrow stays an arrow; the chevron is shapes.
        assert_eq!(
            tree.descendants()
                .filter(|n| n.has_tag_name("arrow"))
                .count(),
            1,
            "{format}"
        );
        // The chevron and the callout each travel as one group.
        let groups: Vec<_> = tree
            .descendants()
            .filter(|n| n.has_tag_name("group"))
            .collect();
        assert!(groups.len() >= 2, "{format}");
        assert!(groups.iter().any(|g| {
            g.descendants()
                .any(|n| n.has_tag_name("s") && n.text() == Some("New bond"))
        }));
    }
}
