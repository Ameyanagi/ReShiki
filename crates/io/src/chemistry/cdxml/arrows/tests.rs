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

fn read_drawing(xml: &str) -> Arrow {
    let source = presentation::parse(xml)
        .unwrap()
        .root_element()
        .descendants()
        .filter(|node| node.is_element())
        .position(|node| node.has_tag_name("arrow"))
        .unwrap();
    let reader = ArrowReader::new(xml).unwrap();
    let colors = [NativeColor([0.; 3]); 4];
    reader
        .read_drawing(source, 1., &colors, 42)
        .unwrap()
        .into_document()
        .unwrap()
}

#[test]
fn chemdraw_block_arrows_read_as_bodies_and_the_reference_reader_still_refuses_them() {
    // The Hollow FillType=Solid and FillType=Shaded cells of ChemDraw's own p5 probe.
    for (fill, body) in [
        ("Solid", Body::Block),
        ("Shaded", Body::Gradient),
        ("None", Body::Hollow),
    ] {
        let xml = format!(
            r#"<CDXML LineWidth="1"><page><arrow id="227" FillType="{fill}" ArrowheadHead="Full" ArrowheadType="Hollow" HeadSize="2000" ArrowheadCenterSize="1500" ArrowheadWidth="600" ArrowShaftSpacing="800" Head3D="105 307 0" Tail3D="30 307 0"/></page></CDXML>"#
        );
        let arrow = read_drawing(&xml);
        let style = arrow.appearance();
        assert_eq!((arrow.kind.as_str(), style.body), ("forward", body));
        assert_eq!(style.gap_pt, 8.);
        assert_eq!(style.head_width_pt, 14.);
        assert_eq!(style.head_length_pt, 20.);
        assert_eq!(style.head_notch, 0.25);
        if fill != "None" {
            let source = 2;
            assert!(
                ArrowReader::new(&xml)
                    .unwrap()
                    .read(source, 1., &[NativeColor([0.; 3]); 4], 1)
                    .is_err()
            );
        }
    }
}

#[test]
fn chemdraw_arc_arrows_read_as_arcs_running_the_same_way() {
    // ChemDraw's 90°, 180° and 270° arc arrows from its p5 probe render.
    for (angle, head, tail, center, middle) in [
        (
            "90",
            "375.61 301.39",
            "375.61 322.61",
            "365 312",
            (380., 312.),
        ),
        ("180", "480 312", "450 312", "465 312", (465., 327.)),
        ("270", "75.61 364.61", "75.61 343.39", "65 354", (50., 354.)),
    ] {
        let (cx, cy) = {
            let mut it = center.split(' ').map(|v| v.parse::<f32>().unwrap());
            (it.next().unwrap(), it.next().unwrap())
        };
        let xml = format!(
            r#"<CDXML><page><arrow id="230" FillType="None" ArrowheadHead="Full" ArrowheadType="Solid" AngularSize="{angle}" Head3D="{head} 0" Tail3D="{tail} 0" Center3D="{center} 0" MajorAxisEnd3D="{} {cy} 0" MinorAxisEnd3D="{cx} {} 0"/></page></CDXML>"#,
            cx + 15.,
            cy + 15.,
        );
        let arrow = read_drawing(&xml);
        let sweep: f32 = angle.parse().unwrap();
        assert_eq!(arrow.arc, Some(-sweep), "{angle}");
        let p = arrow.point(0.5);
        assert!(
            p.distance(Point::new(middle.0, middle.1)) < 0.05,
            "{angle}: {p:?}"
        );
        arrow.validate().unwrap();
    }
}

#[test]
fn an_elliptical_chemdraw_arc_keeps_its_shape_as_a_curve() {
    let xml = r#"<CDXML><page><arrow id="1" ArrowheadHead="Full" AngularSize="180" Tail3D="20 0 0" Head3D="-20 0 0" Center3D="0 0 0" MajorAxisEnd3D="20 0 0" MinorAxisEnd3D="0 10 0"/></page></CDXML>"#;
    let arrow = read_drawing(xml);
    assert!(arrow.arc.is_none());
    assert!(arrow.point(0.5).distance(Point::new(0., -10.)) < 0.05);
    assert_eq!(arrow.end, Point::new(-20., 0.));
    arrow.validate().unwrap();
}

#[test]
fn a_negative_chemdraw_arc_size_runs_clockwise_whatever_the_minor_axis() {
    // ChemDraw draws both over the top: the sign of AngularSize sets the way.
    for minor in ["100 120", "100 80"] {
        let xml = format!(
            r#"<CDXML><page><arrow id="2" ArrowheadHead="Full" AngularSize="-180" Tail3D="80 100 0" Head3D="120 100 0" Center3D="100 100 0" MajorAxisEnd3D="120 100 0" MinorAxisEnd3D="{minor} 0"/></page></CDXML>"#
        );
        let arrow = read_drawing(&xml);
        assert_eq!(arrow.arc, Some(180.), "{minor}");
        assert!(arrow.point(0.5).distance(Point::new(100., 80.)) < 0.05);
    }
}
