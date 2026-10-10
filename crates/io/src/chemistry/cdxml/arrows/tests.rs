use super::*;

#[test]
fn mechanism_curvature_91_keeps_independent_cubic_controls_exactly() {
    let xml = include_str!(
        "../../../../../../tests/fixtures/mechanism-curvature-91/independent-controls.cdxml"
    );
    let reader = ArrowReader::new(xml).unwrap();
    let tree = presentation::parse(xml).unwrap();
    let colors = presentation::palette(tree.root_element()).unwrap().colors;
    let arrow = reader
        .read(5, 1., &colors, 1)
        .unwrap()
        .into_document()
        .unwrap();
    assert_eq!(arrow.start, Point::new(100., 100.));
    assert_eq!(arrow.end, Point::new(136., 100.));
    assert_eq!(arrow.control, None);
    assert_eq!(
        arrow.cubic,
        Some([Point::new(100., 76.), Point::new(136., 124.)])
    );
    assert_eq!(arrow.appearance().head, Head::Full);
    assert_eq!(arrow.appearance().color, crate::palette::Color::Ink);
    arrow.validate().unwrap();
}

#[test]
fn mechanism_attachment_92_cdxml_and_cdx_preserve_curve_without_editing_links() {
    use crate::{
        arrow_anchors::{self, Pick},
        arrows::ArrowStyle,
        document::Document,
    };
    let mut d = Document::from_native_file(include_bytes!(
        "../../../../../../tests/fixtures/mechanism-attachments-92/before.rsk"
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
    let xml = crate::exchange::drawing::write_preserving(&d, Default::default()).unwrap();
    let binary = crate::exchange::to_cdx(&xml).unwrap();
    let decoded = crate::exchange::from_cdx(&binary).unwrap();
    for text in [&xml, &decoded] {
        let tree = presentation::parse(text).unwrap();
        let colors = presentation::palette(tree.root_element()).unwrap().colors;
        let ordinal = tree
            .root_element()
            .descendants()
            .filter(|n| n.is_element())
            .position(|n| n.has_tag_name("curve"))
            .unwrap();
        let reader = ArrowReader::new(text).unwrap();
        let arrow = reader
            .read(
                ordinal,
                f64::from(d.drawing_style.bond_length_world / d.drawing_style.bond_length_pt),
                &colors,
                1,
            )
            .unwrap()
            .into_document()
            .unwrap();
        let original = &d.arrows[0];
        let relative = |p: Point, origin: Point| Point::new(p.x - origin.x, p.y - origin.y);
        assert!(
            relative(arrow.end, arrow.start).distance(relative(original.end, original.start))
                < 0.001
        );
        for (a, b) in arrow
            .cubic
            .unwrap()
            .into_iter()
            .zip(original.cubic.unwrap())
        {
            assert!(relative(a, arrow.start).distance(relative(b, original.start)) < 0.001);
        }
        assert!(arrow.start_anchor.is_none() && arrow.end_anchor.is_none());
    }
}
