use super::*;
use crate::color_contrast::{TEXT_MIN, TEXT_TARGET, contrast};

#[test]
fn legacy_colors_migrate_and_current_colors_round_trip() {
    use {Hue::*, Row::*};
    for (rgb, color) in [
        ([0, 0, 0], Color::Ink),
        ([32, 80, 145], Color::Palette(Blue, Strong)),
        ([17, 126, 108], Color::Palette(Teal, Strong)),
        ([180, 50, 55], Color::Palette(Red, Strong)),
        ([116, 65, 147], Color::Palette(Purple, Strong)),
        ([220, 239, 233], Color::Palette(Teal, Tint)),
        ([221, 232, 248], Color::Palette(Blue, Tint)),
        ([253, 239, 203], Color::Palette(Amber, Tint)),
        ([249, 223, 225], Color::Palette(Red, Tint)),
        ([31, 78, 121], Color::Custom([31, 78, 121])),
        ([255, 255, 255], Color::Custom([255, 255, 255])),
    ] {
        let read: Color = serde_json::from_value(serde_json::json!(rgb)).unwrap();
        assert_eq!(read, color, "{rgb:?}");
    }
    for (color, json) in [
        // Earlier versions can still read Ink and ordinary custom colors.
        (Color::Ink, serde_json::json!([0, 0, 0])),
        (
            Color::Custom([31, 78, 121]),
            serde_json::json!([31, 78, 121]),
        ),
        (
            Color::Palette(Indigo, Tint),
            serde_json::json!("indigo.tint"),
        ),
        (
            Color::Palette(Blue, Strong),
            serde_json::json!("blue.strong"),
        ),
        // Custom colors equal to an old swatch must not migrate on reload.
        (Color::Custom([32, 80, 145]), serde_json::json!("#205091")),
        (Color::Custom([0, 0, 0]), serde_json::json!("#000000")),
    ] {
        assert_eq!(serde_json::to_value(color).unwrap(), json);
        assert_eq!(serde_json::from_value::<Color>(json).unwrap(), color);
    }
    for bad in [
        "\"blue\"",
        "\"cyan.strong\"",
        "\"#12345\"",
        "[256,0,0]",
        "\"blue.pale\"",
    ] {
        assert!(serde_json::from_str::<Color>(bad).is_err(), "{bad}");
    }
    assert_eq!(Color::imported([0; 3]), Color::Ink);
    assert_eq!(Color::imported([32, 80, 145]), Color::Custom([32, 80, 145]));
}

#[test]
fn every_theme_resolves_rows_at_one_lightness_on_both_canvases() {
    for theme in ColorTheme::ALL {
        for canvas in CanvasTheme::ALL {
            let tones = theme.tones(canvas);
            let palette = Palette::new(tones, Hues::default(), canvas);
            assert_eq!(palette.rgb(Color::Ink), canvas.color([0; 3]));
            assert_eq!(palette.rgb(Color::Custom([31, 78, 121])), [31, 78, 121]);
            for hue in Hue::ALL {
                for (row, [l, c]) in [(Row::Strong, tones.strong), (Row::Tint, tones.tint)] {
                    let rgb = palette.rgb(Color::Palette(hue, row));
                    let back = Oklch::from_rgb(rgb);
                    // Gamut mapping keeps lightness and hue; only chroma drops.
                    assert!(
                        (back.l - l).abs() < 0.006,
                        "{theme}/{canvas}/{hue:?}/{row:?}"
                    );
                    assert!(back.c <= c + 0.004, "{theme}/{canvas}/{hue:?}/{row:?}");
                    let degrees = back.h.to_degrees().rem_euclid(360.);
                    let delta = (degrees - f64::from(hue.default_degrees())).abs();
                    assert!(
                        delta.min(360. - delta) < 3.,
                        "{theme}/{canvas}/{hue:?}/{row:?}: {degrees}"
                    );
                    assert_eq!(
                        canvas.color(palette.canonical(Color::Palette(hue, row))),
                        rgb
                    );
                }
            }
            // Strong labels read on the canvas; ink and bonds read on every tint.
            let bg = canvas.background();
            let ink = canvas.color([0; 3]);
            for hue in Hue::ALL {
                let strong = palette.swatch(hue, Row::Strong);
                let tint = palette.swatch(hue, Row::Tint);
                assert!(contrast(strong, bg) >= TEXT_MIN, "{theme}/{canvas}/{hue:?}");
                assert!(
                    contrast(tint, ink) >= TEXT_TARGET,
                    "{theme}/{canvas}/{hue:?}"
                );
            }
        }
    }
    let publication = Palette::new(
        ColorTheme::Publication.tones(CanvasTheme::Light),
        Hues::default(),
        CanvasTheme::Light,
    );
    assert_eq!(publication.swatch(Hue::Blue, Row::Strong), [40, 99, 171]);
    assert_eq!(publication.swatch(Hue::Red, Row::Tint), [255, 223, 220]);
}

#[test]
fn hues_default_serialize_by_name_and_edit_a_custom_copy() {
    let mut hues = Hues::default();
    assert!(hues.is_default());
    hues.set(Hue::Blue, 240);
    let json = serde_json::to_value(hues).unwrap();
    assert_eq!(json["blue"], 240);
    assert_eq!(json["red"], 25);
    let partial: Hues = serde_json::from_value(serde_json::json!({"teal": 190})).unwrap();
    assert_eq!(partial.get(Hue::Teal), 190);
    assert_eq!(partial.get(Hue::Blue), 255);
    assert!(serde_json::from_value::<Hues>(serde_json::json!({"cyan": 190})).is_err());
    assert!(Hues([360; 8]).validate().is_err());

    let mut doc = Document {
        color_theme: ColorTheme::Pastel,
        ..Default::default()
    };
    let before = Palette::of(&doc);
    set_hues(&mut doc, hues);
    let theme = doc.custom_theme.as_ref().unwrap();
    assert_eq!(theme.name, "Pastel · custom hues");
    assert_eq!(theme.id, "pastel-custom-hues");
    theme.validate().unwrap();
    let after = Palette::of(&doc);
    assert_ne!(
        after.swatch(Hue::Blue, Row::Strong),
        before.swatch(Hue::Blue, Row::Strong)
    );
    assert_eq!(
        after.swatch(Hue::Red, Row::Tint),
        before.swatch(Hue::Red, Row::Tint)
    );
    let json = serde_json::to_vec(&doc).unwrap();
    let reopened: Document = serde_json::from_slice(&json).unwrap();
    assert_eq!(reopened, doc);
    assert_eq!(Palette::of(&reopened), after);
    // A second edit changes the embedded copy instead of stacking names.
    set_hues(&mut doc, Hues::default());
    assert_eq!(
        doc.custom_theme.as_ref().unwrap().name,
        "Pastel · custom hues"
    );
    assert_eq!(Palette::of(&doc), before);
}

#[test]
fn both_color_visitors_see_every_stored_color() {
    use crate::{document::Point, graphics::*, typography::*};
    let mut doc = Document::default();
    let a = doc.add_atom("N", Point::default());
    let b = doc.add_atom("C", Point::new(42., 0.));
    doc.add_bond(a, b, 1, "plain");
    let atom = doc.atom_mut(a).unwrap();
    atom.text_style = Some(TextStyle::default());
    atom.display.hydrogen_color = Some(Color::Ink);
    atom.display.number = Some(crate::atom_labels::Number {
        text: "1".into(),
        offset: None,
        style: crate::atom_labels::number_style(),
    });
    let mut format = TextFormat::default();
    format.spans.push(TextSpan {
        start: 0,
        end: 1,
        style: TextStyle::default(),
    });
    doc.annotations.push(crate::document::Annotation {
        id: 3,
        position: Point::default(),
        text: "x".into(),
        format,
    });
    doc.arrows.push(crate::document::Arrow::new(
        4,
        Point::default(),
        Point::new(40., 0.),
        Default::default(),
        Default::default(),
    ));
    doc.graphics.push(Graphic::dragged(
        5,
        GraphicKind::Rectangle,
        Point::default(),
        Point::new(10., 10.),
        GraphicStyle {
            fill: Some(Color::Ink),
            ..Default::default()
        },
        BracketSides::Both,
        false,
    ));
    doc.ring_fills.push(crate::ring_fills::RingFill {
        atoms: vec![a],
        color: Color::Ink,
    });
    let mut visited = 0;
    crate::palette::for_each_color_mut(&mut doc, |c| {
        visited += 1;
        *c = Color::Palette(Hue::ALL[visited % 8], Row::Strong);
    });
    // Atom 5 (style, H, stereo, mapping, number) + second atom stereo/mapping,
    // bond 2, text 2, arrow 1, graphic 2, ring fill 1.
    assert_eq!(visited, 15);
    let seen = std::cell::Cell::new(0);
    assert!(!any_color(&doc, |c| {
        seen.set(seen.get() + 1);
        c == Color::Ink
    }));
    assert_eq!(seen.get(), visited);
}

#[test]
fn typed_colors_parse_hex_rgb_and_oklch() {
    for (text, rgb) in [
        ("#1F4E79", [31, 78, 121]),
        ("1f4e79", [31, 78, 121]),
        (" #17B ", [17, 119, 187]),
        ("31, 78, 121", [31, 78, 121]),
        ("31 78 121", [31, 78, 121]),
        ("rgb(31 78 121)", [31, 78, 121]),
        ("RGB(31, 78, 121)", [31, 78, 121]),
    ] {
        assert_eq!(parse_color(text), Some(rgb), "{text}");
    }
    let oklch = parse_color("oklch(0.42 0.09 250)").unwrap();
    let back = Oklch::from_rgb(oklch);
    assert!((back.l - 0.42).abs() < 0.01 && (back.c - 0.09).abs() < 0.01);
    assert_eq!(parse_color("oklch(42% 0.09 250deg)"), Some(oklch));
    // Out-of-gamut chroma drops at the typed lightness.
    let vivid = Oklch::from_rgb(parse_color("oklch(0.9 0.4 145)").unwrap());
    assert!((vivid.l - 0.9).abs() < 0.01);
    for bad in [
        "",
        "blue",
        "#GG0000",
        "12345",
        "#1234567",
        "αβγ",
        "💚AB",
        "256, 0, 0",
        "1, 2",
        "rgb(1 2 3 4)",
        "oklch(1.2 0.1 30)",
        "oklch(0.5 0.1)",
        "oklch(nan 0.1 30)",
    ] {
        assert_eq!(parse_color(bad), None, "{bad}");
    }
}

#[test]
fn the_closest_palette_color_uses_oklab_distance() {
    let palette = Palette::new(
        ColorTheme::Publication.tones(CanvasTheme::Light),
        Hues::default(),
        CanvasTheme::Light,
    );
    for color in [Color::Ink, Color::Palette(Hue::Teal, Row::Tint)] {
        assert_eq!(palette.closest(palette.rgb(color)), (color, 0.));
    }
    let (near, distance) = palette.closest([31, 78, 121]);
    assert_eq!(near, Color::Palette(Hue::Blue, Row::Strong));
    assert!(distance > 0. && distance < 15., "{distance}");
    assert!((crate::color_contrast::delta_e([0; 3], [255; 3]) - 100.).abs() < 0.1);
    // Any hue angle resolves like the swatches.
    let tones = ColorTheme::Publication.tones(CanvasTheme::Light);
    assert_eq!(
        tones.rgb(Row::Strong, 255),
        palette.swatch(Hue::Blue, Row::Strong)
    );
}

#[test]
fn recent_custom_colors_keep_the_newest_eight() {
    let mut doc = Document::default();
    for i in 0..10 {
        doc.remember_color([i, 0, 0]);
    }
    doc.remember_color([3, 0, 0]);
    assert_eq!(doc.recent_colors.len(), RECENT_LIMIT);
    assert_eq!(doc.recent_colors[0], [3, 0, 0]);
    assert_eq!(doc.recent_colors[1], [9, 0, 0]);
    let json = serde_json::to_value(&doc).unwrap();
    assert_eq!(json["recent_colors"][0], serde_json::json!([3, 0, 0]));
    assert_eq!(serde_json::from_value::<Document>(json).unwrap(), doc);
    assert!(
        !serde_json::to_string(&Document::default())
            .unwrap()
            .contains("recent_colors")
    );
}

fn mapping_color_sample(color: Color) -> (Document, u64) {
    use crate::{atom_labels::Number, document::Point, typography::TextStyle};
    let mut doc = Document::default();
    let id = doc.add_atom("N", Point::new(12., -8.));
    let atom = doc.atom_mut(id).unwrap();
    atom.map_num = 17;
    atom.isotope = 15;
    atom.charge = 1;
    atom.explicit_h = 2;
    atom.no_implicit = true;
    atom.depth = 7.;
    atom.display.mapping.show = Some(false);
    atom.display.mapping.offset = Some(Point::new(9., -6.));
    atom.display.mapping.style = TextStyle {
        family: "Times New Roman".into(),
        size_pt: 9.5,
        bold: true,
        italic: true,
        underline: true,
        color,
        ..Default::default()
    };
    atom.display.number = Some(Number {
        text: "A-42".into(),
        offset: Some(Point::new(-11., 5.)),
        style: TextStyle {
            family: "Courier New".into(),
            size_pt: 12.,
            ..Default::default()
        },
    });
    (doc, id)
}

#[test]
fn mapping_only_color_is_found_and_mutated_without_changing_its_owner() {
    let source = Color::Custom([31, 78, 121]);
    let replacement = Color::Palette(Hue::Teal, Row::Strong);
    let (mut doc, id) = mapping_color_sample(source);
    assert!(any_color(&doc, |color| color == source));
    let mut expected = doc.clone();
    expected.atom_mut(id).unwrap().display.mapping.style.color = replacement;
    for_each_color_mut(&mut doc, |color| {
        if *color == source {
            *color = replacement;
        }
    });
    assert_eq!(doc, expected);
    assert!(!any_color(&doc, |color| color == source));
    assert!(any_color(&doc, |color| color == replacement));
}

#[test]
fn mapping_only_palette_resolves_on_both_canvases_with_embedded_hues() {
    for canvas in CanvasTheme::ALL {
        for custom_hues in [false, true] {
            let color = Color::Palette(Hue::Blue, Row::Strong);
            let (mut doc, id) = mapping_color_sample(color);
            doc.canvas_theme = canvas;
            if custom_hues {
                let mut hues = Hues::default();
                hues.set(Hue::Blue, 145);
                set_hues(&mut doc, hues);
            }
            let original = doc.clone();
            let visible = Palette::of(&doc).rgb(color);
            let resolved = crate::canvas_theme::resolved_document(&doc);
            assert!(matches!(resolved, std::borrow::Cow::Owned(_)));
            let mut expected = original.atom(id).unwrap().display.mapping.clone();
            expected.style.color = Color::Custom(visible);
            assert_eq!(resolved.atom(id).unwrap().display.mapping, expected);
            let canonical = crate::canvas_theme::canonical_document(&doc);
            assert_eq!(
                canvas.color(
                    canonical
                        .atom(id)
                        .unwrap()
                        .display
                        .mapping
                        .style
                        .color
                        .rgb()
                ),
                visible
            );
            let pasted = crate::canvas_theme::for_paste(doc.clone(), canvas.toggled());
            assert_eq!(pasted.atom(id).unwrap().display.mapping, expected);
            assert_eq!(doc, original);
        }
    }
}

#[test]
fn applying_a_theme_resets_mapping_ink_without_changing_labels_or_chemistry() {
    for theme in ColorTheme::ALL {
        let (mut doc, id) = mapping_color_sample(Color::Custom([31, 78, 121]));
        set_hues(&mut doc, Hues::default());
        let mut expected = doc.clone();
        expected.color_theme = theme;
        expected.custom_theme = None;
        expected.atom_mut(id).unwrap().display.mapping.style.color = Color::Ink;
        theme.apply(&mut doc);
        assert_eq!(doc, expected);
    }
}

#[test]
fn mapping_rgb_fade_matches_existing_indicators_and_is_a_terminal_snapshot() {
    use crate::depth_appearance as depth;
    for canvas in CanvasTheme::ALL {
        for color in [
            Color::Custom([20, 80, 140]),
            Color::Palette(Hue::Blue, Row::Strong),
        ] {
            let (mut doc, id) = mapping_color_sample(color);
            doc.canvas_theme = canvas;
            let atom = doc.atom_mut(id).unwrap();
            atom.display.stereo.style.color = color;
            atom.display.number.as_mut().unwrap().style.color = color;
            depth::enable(&mut doc, &[id], 1.).unwrap();
            depth::override_fade(&mut doc, &[id], Some(0.5)).unwrap();
            let original = doc.clone();
            let faded = depth::materialize(&doc);
            let atom = faded.atom(id).unwrap();
            let ink = atom.display.stereo.style.color;
            assert_eq!(atom.display.mapping.style.color, ink);
            assert_eq!(atom.display.number.as_ref().unwrap().style.color, ink);
            if matches!(color, Color::Custom(_)) {
                let golden = if canvas.is_light() {
                    [138, 168, 198]
                } else {
                    [10, 40, 70]
                };
                assert_eq!(ink, Color::Custom(golden));
            }
            let before = original.atom(id).unwrap();
            let mut mapping = before.display.mapping.clone();
            mapping.style.color = ink;
            assert_eq!(atom.display.mapping, mapping);
            let mut number = before.display.number.clone().unwrap();
            number.style.color = ink;
            assert_eq!(atom.display.number.as_ref(), Some(&number));
            let mut chemical_owner = atom.clone();
            chemical_owner.display = before.display.clone();
            chemical_owner.text_style = before.text_style.clone();
            assert_eq!(&chemical_owner, before);
            assert!(faded.depth_appearance.is_empty());
            assert_eq!(depth::materialize(&faded).as_ref(), faded.as_ref());
            assert_eq!(doc, original);
        }
    }
}
