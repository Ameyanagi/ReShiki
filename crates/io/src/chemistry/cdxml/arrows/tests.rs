use super::*;

fn read_straight(xml: &str) -> NativeArrow {
    let source = presentation::parse(xml)
        .unwrap()
        .root_element()
        .descendants()
        .filter(|node| node.is_element())
        .position(|node| node.has_tag_name("arrow"))
        .unwrap();
    ArrowReader::new(xml)
        .unwrap()
        .read(source, 1., &[NativeColor([0.; 3]); 4], 42)
        .unwrap()
}

#[test]
fn retro_839_spacing_requires_a_full_open_head_without_a_tail() {
    for (head, tail, shape, gap, expected) in [
        ("Full", "None", "Angle", "140", Preset::Retro),
        ("Full", "Unspecified", "Angle", "140", Preset::Retro),
        ("Full", "None", "Angle", "0", Preset::Forward),
        ("Full", "None", "Angle", "-0", Preset::Forward),
        ("Full", "None", "Hollow", "140", Preset::Forward),
        ("Full", "None", "Solid", "140", Preset::Forward),
        ("HalfLeft", "HalfLeft", "Angle", "140", Preset::Equilibrium),
        (
            "HalfLeft",
            "HalfRight",
            "Hollow",
            "140",
            Preset::Equilibrium,
        ),
        ("HalfRight", "HalfLeft", "Solid", "140", Preset::Equilibrium),
        (
            "HalfRight",
            "HalfRight",
            "Angle",
            "140",
            Preset::Equilibrium,
        ),
        ("HalfLeft", "None", "Angle", "140", Preset::Forward),
        ("None", "HalfRight", "Angle", "140", Preset::Forward),
        ("None", "None", "Angle", "140", Preset::Forward),
        ("Full", "Full", "Angle", "140", Preset::Resonance),
        ("Full", "HalfLeft", "Angle", "140", Preset::Resonance),
        ("HalfRight", "Full", "Angle", "140", Preset::Resonance),
        ("Full", "Full", "Angle", "0", Preset::Resonance),
        ("HalfLeft", "HalfRight", "Angle", "0", Preset::Resonance),
    ] {
        let xml = format!(
            "<CDXML><arrow ArrowheadHead='{head}' ArrowheadTail='{tail}' \
             ArrowheadType='{shape}' ArrowShaftSpacing='{gap}' Tail3D='0 0' Head3D='30 0'/></CDXML>"
        );
        let actual = read_straight(&xml);
        assert_eq!(actual.kind, expected, "{xml}");
        assert_eq!(actual.style.head, super::head(head).unwrap(), "{xml}");
        assert_eq!(actual.style.tail, super::head(tail).unwrap(), "{xml}");
        assert_eq!(
            actual.style.shape,
            match shape {
                "Angle" => HeadShape::Open,
                "Hollow" => HeadShape::Hollow,
                _ => HeadShape::Solid,
            },
            "{xml}"
        );
        let gap: f64 = gap.parse().unwrap();
        assert_eq!(
            actual.style.gap_pt,
            if gap == 0. { 2. } else { gap / 100. },
            "{xml}"
        );
    }
}

#[test]
fn retro_839_legacy_graphics_match_anywhere_in_either_order() {
    let arrow = "<arrow id='21' ArrowheadHead='HalfLeft' ArrowheadTail='HalfLeft' \
                 ArrowheadType='Hollow' ArrowShaftSpacing='0' Tail3D='0 0' Head3D='30 0'/>";
    let legacy = "<graphic ArrowType='RetroSynthetic' SupersededBy='21'/>";
    for xml in [
        format!("<CDXML>{legacy}<page><group>{arrow}</group></page></CDXML>"),
        format!(
            "<CDXML><page><group>{arrow}</group></page><page><group>{legacy}</group></page></CDXML>"
        ),
        format!("<CDXML><graphic ArrowType='FullHead' SupersededBy='21'/>{arrow}{legacy}</CDXML>"),
    ] {
        let actual = read_straight(&xml);
        assert_eq!(actual.kind, Preset::Retro, "{xml}");
        assert_eq!(actual.style.head, Head::Left);
        assert_eq!(actual.style.tail, Head::Left);
        assert_eq!(actual.style.shape, HeadShape::Hollow);
    }
}

#[test]
fn retro_839_legacy_graphics_require_an_exact_link_and_type() {
    for legacy in [
        "<graphic ArrowType='RetroSynthetic' SupersededBy='other'/>",
        "<graphic ArrowType='RetroSynthetic'/>",
        "<graphic ArrowType='FullHead' SupersededBy='21'/>",
        "<graphic ArrowType='retrosynthetic' SupersededBy='21'/>",
        "<group ArrowType='RetroSynthetic' SupersededBy='21'/>",
    ] {
        let xml = format!(
            "<CDXML>{legacy}<arrow id='21' ArrowheadHead='Full' Tail3D='0 0' Head3D='30 0'/></CDXML>"
        );
        assert_eq!(read_straight(&xml).kind, Preset::Forward, "{xml}");
    }
    let xml = "<CDXML><graphic ArrowType='RetroSynthetic'/><arrow ArrowheadHead='Full' \
               Tail3D='0 0' Head3D='30 0'/></CDXML>";
    assert_eq!(read_straight(xml).kind, Preset::Forward);
}

#[test]
fn retro_839_classification_preserves_native_geometry_and_style() {
    let actual = read_straight(
        "<CDXML><arrow id='21' ArrowheadHead='Full' ArrowheadType='Angle' \
         ArrowShaftSpacing='123.45678901234567' HeadSize='345.6789012345678' \
         ArrowheadWidth='234.5678901234567' LineWidth='0.67890123456789' \
         Tail3D='0.12345678901234567 2.345678901234567' \
         Head3D='30.123456789012345 4.567890123456789'/></CDXML>",
    );
    assert_eq!(actual.kind, Preset::Retro);
    assert_eq!(actual.id, 42);
    assert_eq!(actual.start.x, 0.12345678901234567);
    assert_eq!(actual.start.y, 2.345678901234567);
    assert_eq!(actual.end.x, 30.123456789012345);
    assert_eq!(actual.end.y, 4.567890123456789);
    assert_eq!(actual.style.gap_pt, 123.45678901234567 / 100.);
    assert_eq!(actual.style.head_length_pt, 345.6789012345678 / 100.);
    assert_eq!(actual.style.head_width_pt, 234.5678901234567 / 100.);
    assert_eq!(actual.style.width_pt, 0.67890123456789);
}

#[test]
fn retro_839_curves_with_spacing_remain_rejected() {
    let reader = ArrowReader::new(
        "<CDXML><curve ArrowheadHead='Full' ArrowheadType='Angle' ArrowShaftSpacing='140' \
         CurvePoints='0 0 0 0 10 5 20 5 30 0 30 0'/></CDXML>",
    )
    .unwrap();
    assert_eq!(
        reader
            .read(1, 1., &[NativeColor([0.; 3]); 4], 1)
            .unwrap_err()
            .to_string(),
        "This arrowed CDXML spline is not supported yet"
    );
}

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
