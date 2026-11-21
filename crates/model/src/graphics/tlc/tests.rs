use super::*;
use crate::{
    document::{Document, History},
    graphics::{BracketSides, GraphicKind},
    palette::{self, Color},
    scene::{self, Primitive},
    typography::{Script, TextSpan},
};
fn plate() -> Graphic {
    let mut g = Graphic::dragged(
        1,
        GraphicKind::Rectangle,
        Point::new(330., 383.),
        Point::new(370., 583.),
        GraphicStyle::default(),
        BracketSides::Both,
        false,
    );
    g.kind = GraphicKind::TlcPlate;
    let label = TlcRfLabel {
        text: "Rf = 0.28".into(),
        format: TextFormat {
            spans: vec![TextSpan {
                start: 1,
                end: 2,
                style: crate::typography::TextStyle {
                    script: Script::Subscript,
                    ..Default::default()
                },
            }],
            ..Default::default()
        },
        offset: Point::new(4., -8.),
        angle: 0.,
    };
    g.tlc = Some(Box::new(TlcPlate {
        lanes: [0.28, 0.51, 0.77]
            .into_iter()
            .enumerate()
            .map(|(i, rf)| TlcLane {
                spots: vec![TlcSpot {
                    rf,
                    width_pt: 325095. / 65536.,
                    height_pt: 325095. / 65536.,
                    style: GraphicStyle {
                        fill: (i != 2).then_some(Color::Ink),
                        ..Default::default()
                    },
                    show_rf: i == 0,
                    label: (i == 0).then(|| label.clone()),
                }],
            })
            .collect(),
        ..Default::default()
    }));
    g
}
#[test]
fn native_tlc_keeps_lanes_spot_fraction_physical_size_and_owned_rich_label() {
    let g = plate();
    g.validate().unwrap();
    let t = g.tlc.as_deref().unwrap();
    assert!(t.spot_center(&g, 0, 0.28).distance(Point::new(340., 518.2)) < 0.001);
    assert!(t.spot_center(&g, 1, 0.51).distance(Point::new(350., 481.4)) < 0.001);
    assert!(t.spot_center(&g, 2, 0.77).distance(Point::new(360., 439.8)) < 0.001);
    let parts = g.parts();
    assert_eq!(parts.len(), 9); // border, two reference lines, three ticks and spots
    assert!(parts[6].filled && parts[7].filled && !parts[8].filled);
    assert_eq!(parts[6].style.width_pt, 0.);
    assert_eq!(parts[1].style.pattern, LinePattern::Dashed);
    assert!(parts[3].filled && parts[3].style.width_pt == 0.);
    let tick: Vec<_> = parts[3]
        .commands
        .iter()
        .flat_map(PathCommand::iter_points)
        .collect();
    assert!((tick[2].y - tick[0].y - g.style.width() * 6.).abs() < 0.001);
    let labels = g.tlc_labels();
    assert_eq!(labels.len(), 1);
    assert_eq!(labels[0].format.spans[0].style.script, Script::Subscript);
    assert!(g.hit(labels[0].centre(), 0.));
    assert!(g.hit(t.spot_center(&g, 0, 0.28), 0.));
    assert!(!g.hit(t.spot_center(&g, 2, 0.77), 0.));
    assert!(g.edit_points().is_empty());
}
#[test]
fn native_tlc_geometry_and_text_use_the_shared_scene() {
    let g = plate();
    let doc = Document {
        graphics: vec![g.clone()],
        ..Default::default()
    };
    let primitives = scene::primitives(&doc);
    assert_eq!(
        primitives
            .iter()
            .filter(|p| matches!(p, Primitive::Path { .. }))
            .count(),
        g.parts().len()
    );
    assert!(primitives.iter().any(|p| matches!(p, Primitive::Text {text, style, ..} if text == "f" && style.script == Script::Subscript)));
    let (_, hi) = g.bounds();
    let (_, label_hi) = g.tlc_labels()[0].bounds();
    assert!(hi.x >= label_hi.x && hi.y >= label_hi.y);
}
#[test]
fn native_tlc_affine_move_and_reflection_preserve_semantics_and_physical_spot_sizes() {
    let mut g = plate();
    g.tlc.as_deref_mut().unwrap().bottom_right = Point::new(1.2, 0.9);
    let before = g.clone();
    let t = before.tlc.as_deref().unwrap();
    let corners = [(0., 0.), (1., 0.), (1., 1.), (0., 1.)].map(|(u, v)| t.point(&before, u, v));
    let transform = |p: Point| Point::new(1000. - p.x * 2. + p.y * 0.2, p.y * 1.5 + 20.);
    g.map_positions(transform);
    g.validate().unwrap();
    let t = g.tlc.as_deref().unwrap();
    for ((u, v), expected) in [(0., 0.), (1., 0.), (1., 1.), (0., 1.)]
        .into_iter()
        .zip(corners.map(transform))
    {
        assert!(t.point(&g, u, v).distance(expected) < 0.001);
    }
    assert_eq!(g.tlc, before.tlc);
    assert_eq!(g.tlc_labels()[0].format, before.tlc_labels()[0].format);
    assert_eq!(g.tlc_labels()[0].angle, 0.);
}
#[test]
fn native_tlc_version40_roundtrip_and_one_undo_keep_the_whole_plate() {
    let mut doc = Document {
        graphics: vec![plate()],
        ..Default::default()
    };
    assert_eq!(doc.file_version(), crate::document::DRAWING_ELEMENTS);
    let bytes = serde_json::to_vec(&doc.current()).unwrap();
    let restored: Document = serde_json::from_slice(&bytes).unwrap();
    restored.validate().unwrap();
    assert_eq!(restored.graphics, doc.graphics);
    let before = doc.clone();
    let mut history = History::default();
    doc.graphics[0].map_positions(|p| p.offset(30., 40.));
    assert!(history.commit(before.clone(), &doc));
    assert_eq!(history.frames(), 1);
    assert_eq!(history.peek(false).unwrap().graphics, before.graphics);
    let json = serde_json::to_value(Graphic::dragged(
        2,
        GraphicKind::Line,
        Point::default(),
        Point::new(10., 0.),
        GraphicStyle::default(),
        BracketSides::Both,
        false,
    ))
    .unwrap();
    assert!(json.get("tlc").is_none());
    assert!(
        serde_json::from_value::<Graphic>(json)
            .unwrap()
            .tlc
            .is_none()
    );
}
#[test]
fn native_tlc_validation_rejects_folded_nonfinite_and_unbounded_data() {
    for br in [
        Point::new(0., 1.),
        Point::new(0.3, 0.3),
        Point::new(f32::NAN, 1.),
    ] {
        let mut g = plate();
        g.tlc.as_deref_mut().unwrap().bottom_right = br;
        assert!(g.validate().is_err());
    }
    let mut g = plate();
    g.axis_y = g.axis_x;
    assert!(g.validate().is_err());
    let mut g = plate();
    g.tlc.as_deref_mut().unwrap().lanes[0].spots[0].rf = f64::NAN;
    assert!(g.validate().is_err());
    let mut g = plate();
    g.tlc.as_deref_mut().unwrap().lanes[0].spots[0].width_pt = 32768.;
    assert!(g.validate().is_err());
    let mut g = plate();
    g.tlc.as_deref_mut().unwrap().lanes = vec![TlcLane { spots: vec![] }; MAX_TLC_LANES + 1];
    assert!(g.validate().is_err());
}
#[test]
fn native_tlc_nested_colors_follow_palette_and_document_theme_visitors() {
    let mut doc = Document {
        graphics: vec![plate()],
        ..Default::default()
    };
    let c = Color::Custom([3, 71, 119]);
    let spot = &mut doc.graphics[0].tlc.as_deref_mut().unwrap().lanes[0].spots[0];
    spot.style.stroke = c;
    spot.style.fill = Some(c);
    spot.label.as_mut().unwrap().format.spans[0].style.color = c;
    assert!(palette::any_color(&doc, |v| v == c));
    palette::for_each_color_mut(&mut doc, |v| {
        if *v == c {
            *v = Color::Ink
        }
    });
    assert!(!palette::any_color(&doc, |v| v == c));
}

#[test]
fn native_tlc_owned_label_participates_in_region_selection_and_erase_strokes() {
    let mut g = plate();
    g.tlc.as_deref_mut().unwrap().lanes[0].spots[0]
        .label
        .as_mut()
        .unwrap()
        .offset = Point::new(200., -8.);
    let doc = Document {
        graphics: vec![g.clone()],
        ..Default::default()
    };
    let just_frame = [
        Point::new(300., 350.),
        Point::new(400., 350.),
        Point::new(400., 620.),
        Point::new(300., 620.),
    ];
    assert!(crate::selection_region::objects(&doc, &just_frame).is_empty());
    let label = g.tlc_labels().remove(0);
    let (lo, hi) = label.bounds();
    let all = [
        Point::new(300., 350.),
        Point::new(hi.x + 10., 350.),
        Point::new(hi.x + 10., 620.),
        Point::new(300., 620.),
    ];
    assert_eq!(crate::selection_region::objects(&doc, &all), vec![g.id]);
    let mut doc = doc;
    let mid = (lo.y + hi.y) / 2.;
    crate::erasing::stroke(
        &mut doc,
        Point::new(lo.x - 20., mid),
        Point::new(hi.x + 20., mid),
        0.,
    );
    assert!(doc.graphics.is_empty());
    let mut g = plate();
    let t = g.tlc.as_deref_mut().unwrap();
    t.lanes.clear();
    t.show_origin = false;
    t.show_solvent_front = false;
    t.show_borders = false;
    let doc = Document {
        graphics: vec![g],
        ..Default::default()
    };
    assert!(crate::selection_region::objects(&doc, &just_frame).is_empty());
}

#[test]
fn native_tlc_normal_graphic_opacity_and_wavy_border_have_shared_rendering() {
    let mut g = plate();
    g.style.pattern = LinePattern::Wavy;
    g.style.effects.opacity = Some(0.5);
    g.validate().unwrap();
    assert!(
        g.parts()[0]
            .commands
            .iter()
            .any(|c| matches!(c, PathCommand::Cubic(..)))
    );
    let doc = Document {
        graphics: vec![g],
        ..Default::default()
    };
    assert!(
        scene::primitives(&doc)
            .iter()
            .all(|p| matches!(p,Primitive::Opacity {alpha, ..} if *alpha==0.5))
    );
}

#[test]
fn native_tlc_rotation_center_follows_its_convex_four_corner_frame() {
    let mut g = plate();
    g.tlc.as_deref_mut().unwrap().bottom_right = Point::new(1.2, 0.9);
    let expected = g.tlc.as_deref().unwrap().point(&g, 0.5, 0.5);
    let mut doc = Document {
        graphics: vec![g],
        ..Default::default()
    };
    let centre = crate::editing::rotation_center(&doc, &[1]).unwrap();
    assert!(centre.distance(expected) < 0.001);
    for _ in 0..8 {
        doc.graphics[0].map_positions(|p| p.turned(centre, 17.));
    }
    assert!(
        crate::editing::rotation_center(&doc, &[1])
            .unwrap()
            .distance(centre)
            < 0.001
    );
}
