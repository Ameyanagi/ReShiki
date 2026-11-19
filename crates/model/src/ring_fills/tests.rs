use super::*;
#[test]
fn legacy_ring_fills_migrate_by_key_and_fixed_colors_stay_exact() {
    let read = |json: serde_json::Value| serde_json::from_value::<RingFill>(json);
    for (key, hue) in PALETTE.map(|(_, key, hue)| (key, hue)) {
        let fill = read(serde_json::json!({"atoms": [1, 2, 3], "color": key})).unwrap();
        assert_eq!(fill.color, Color::Palette(hue, Row::Tint));
        let fixed = serde_json::json!({"atoms": [1, 2, 3], "color": key, "fixed_color": true});
        assert_eq!(read(fixed).unwrap().color, Color::Custom(key));
    }
    let other = read(serde_json::json!({"atoms": [1, 2, 3], "color": [245, 221, 165]}));
    assert_eq!(other.unwrap().color, Color::Custom([245, 221, 165]));
    let current = RingFill {
        atoms: vec![1, 2, 3],
        color: Color::Palette(Hue::Indigo, Row::Tint),
        effect: Default::default(),
    };
    let json = serde_json::to_value(&current).unwrap();
    assert_eq!(
        json,
        serde_json::json!({"atoms": [1, 2, 3], "color": "indigo.tint"})
    );
    assert_eq!(read(json).unwrap(), current);
    // A custom color equal to an old key stays exact after saving.
    let exact = RingFill {
        atoms: vec![1, 2, 3],
        color: Color::Custom([201, 224, 248]),
        effect: Default::default(),
    };
    let json = serde_json::to_value(&exact).unwrap();
    assert_eq!(json["fixed_color"], true);
    assert_eq!(read(json).unwrap(), exact);
    let ink = RingFill {
        atoms: vec![1, 2, 3],
        color: Color::Ink,
        effect: Default::default(),
    };
    assert_eq!(read(serde_json::to_value(&ink).unwrap()).unwrap(), ink);
    let black = RingFill {
        color: Color::Custom([0; 3]),
        ..ink
    };
    assert_eq!(read(serde_json::to_value(&black).unwrap()).unwrap(), black);
    assert!(read(serde_json::json!({"atoms": [1], "color": "sky"})).is_err());
}

#[test]
fn ring_fill_effects_paint_save_and_need_version_40() {
    use crate::scene::Primitive;
    let mut doc = Document::default();
    let ring = crate::editing::ring(&mut doc, Point::new(0., 0.), 6, false, 1.);
    let color = Color::Custom([0, 0, 255]);
    assert_eq!(apply(&mut doc, &ring, Some(color)), 1);
    assert_eq!(doc.file_version(), Document::default().file_version());
    let fill = |doc: &Document| doc.ring_fills[0].primitive(doc).unwrap();
    assert!(matches!(fill(&doc), Primitive::Path { filled: true, .. }));

    assert_eq!(set_effect(&mut doc, &ring, RingEffect::Gradient), 1);
    assert_eq!(set_effect(&mut doc, &ring, RingEffect::Gradient), 0);
    let Primitive::Gradient {
        radial: true,
        stops,
        ..
    } = fill(&doc)
    else {
        panic!("a radial gradient");
    };
    // The paper color at the middle of the ring into its color at the bonds.
    assert_eq!(stops, [(0., [255; 3], 1.), (1., [0, 0, 255], 1.)]);
    let mut dark = doc.clone();
    dark.canvas_theme = crate::canvas_theme::CanvasTheme::Dark;
    let Primitive::Gradient { stops, .. } = fill(&dark) else {
        panic!("a gradient on the dark canvas");
    };
    assert_eq!(
        stops.first().map(|s| s.1),
        Some(dark.canvas_theme.background())
    );
    assert_eq!(doc.file_version(), crate::document::DRAWING_ELEMENTS);

    set_effect(&mut doc, &ring, RingEffect::SeeThrough);
    assert!(matches!(
        fill(&doc),
        Primitive::Opacity { alpha, .. } if alpha == crate::graphics::Effects::SEE_THROUGH
    ));
    let json = serde_json::to_value(&doc.ring_fills[0]).unwrap();
    assert_eq!(json["effect"], "see_through");
    assert_eq!(
        serde_json::from_value::<RingFill>(json).unwrap(),
        doc.ring_fills[0]
    );
    // A flat fill saves exactly as earlier versions wrote it.
    set_effect(&mut doc, &ring, RingEffect::Flat);
    let json = serde_json::to_value(&doc.ring_fills[0]).unwrap();
    assert!(json.get("effect").is_none(), "{json}");
}

#[test]
fn a_ring_glows_or_sits_on_a_disc_past_its_bonds() {
    use crate::scene::Primitive;
    let mut doc = Document::default();
    let ring = crate::editing::ring(&mut doc, Point::new(0., 0.), 6, false, 1.);
    apply(&mut doc, &ring, Some(Color::Custom([0, 0, 255])));
    let reach = |p: &Primitive| {
        let (lo, hi) = crate::scene::bounds(std::slice::from_ref(p));
        (hi.x - lo.x) / 2.
    };
    let ring_radius = reach(&doc.ring_fills[0].primitive(&doc).unwrap());
    set_effect(&mut doc, &ring, RingEffect::Glow);
    let glow = doc.ring_fills[0].primitive(&doc).unwrap();
    assert!(matches!(
        glow,
        Primitive::Glow {
            color: [0, 0, 255],
            ..
        }
    ));
    assert!(reach(&glow) > ring_radius * 1.2);
    set_effect(&mut doc, &ring, RingEffect::Disc);
    let disc = doc.ring_fills[0].primitive(&doc).unwrap();
    assert!(matches!(disc, Primitive::Path { filled: true, .. }));
    // Around the farthest atom, a little wider.
    assert!(
        reach(&disc) > ring_radius * 1.1,
        "{} {ring_radius}",
        reach(&disc)
    );
    assert_eq!(doc.file_version(), crate::document::DRAWING_ELEMENTS);
}
