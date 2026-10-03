use reshiki::{
    document::{Document, Point},
    palette::{self, Color, Hue, Row},
    typography::TextStyle,
};

fn grouped() -> (Document, u64) {
    let mut doc = Document::default();
    let anchor = doc.add_atom("O", Point::default());
    let carbon = doc.add_atom("C", Point::new(42., 0.));
    doc.add_bond(anchor, carbon, 1, "plain");
    doc.atom_mut(anchor).unwrap().text_style = Some(TextStyle {
        color: Color::Custom([0; 3]),
        size_pt: 11.,
        ..Default::default()
    });
    doc.atom_mut(anchor).unwrap().display.color_override = true;
    doc.contract(&[anchor, carbon], "OMe", "MeO").unwrap();
    (doc, anchor)
}

#[test]
fn independent_label_style_survives_native_save_and_expansion_keeps_internal_typography() {
    let (mut doc, anchor) = grouped();
    let internal = doc.atoms.clone();
    let group = doc.abbreviations.first_mut().unwrap();
    group.label_style = Some(TextStyle {
        color: Color::Custom([124; 3]),
        size_pt: 17.,
        bold: true,
        ..Default::default()
    });
    group.label_color_override = true;
    let bytes = doc.file_json().unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["version"], 18);
    assert!(
        json["abbreviations"][0]["label_color_override"]
            .as_bool()
            .unwrap()
    );
    let mut reopened = Document::from_json(&bytes).unwrap();
    assert_eq!(reopened, doc.current());
    let group = reopened.abbreviation(anchor).unwrap();
    assert_eq!(group.text_style(&reopened).color, Color::Custom([124; 3]));
    assert_eq!(group.text_style(&reopened).size_pt, 17.);
    assert!(group.color_override(&reopened));
    reopened.expand_abbreviations(&[anchor]);
    assert_eq!(reopened.atoms, internal);
}

#[test]
fn older_groups_use_the_anchor_style_without_serializing_new_fields() {
    let (mut doc, anchor) = grouped();
    doc.version = 17;
    let bytes = serde_json::to_vec(&doc).unwrap();
    let text = String::from_utf8_lossy(&bytes);
    assert!(!text.contains("label_style"));
    assert!(!text.contains("label_color_override"));
    let reopened = Document::from_json(&bytes).unwrap();
    let group = reopened.abbreviation(anchor).unwrap();
    assert_eq!(
        group.text_style(&reopened),
        reopened.atom(anchor).unwrap().text_style.clone().unwrap()
    );
    assert!(group.color_override(&reopened));
}

#[test]
fn label_style_colors_participate_in_both_palette_visitors() {
    let (mut doc, _) = grouped();
    let internal = doc.atoms.clone();
    let chosen = Color::Palette(Hue::Teal, Row::Strong);
    doc.abbreviations[0].label_style = Some(TextStyle {
        color: chosen,
        ..Default::default()
    });
    assert!(palette::any_color(&doc, |color| color == chosen));
    palette::for_each_color_mut(&mut doc, |color| {
        if *color == chosen {
            *color = Color::Custom([10, 20, 30]);
        }
    });
    assert!(!palette::any_color(&doc, |color| color == chosen));
    assert_eq!(
        doc.abbreviations[0].label_style.as_ref().unwrap().color,
        Color::Custom([10, 20, 30])
    );
    assert_eq!(doc.atoms, internal);
}

#[test]
fn invalid_label_typography_and_override_without_style_are_rejected() {
    let (mut doc, _) = grouped();
    doc.abbreviations[0].label_color_override = true;
    assert!(doc.validate().is_err());
    doc.abbreviations[0].label_style = Some(TextStyle {
        size_pt: 0.,
        ..Default::default()
    });
    assert!(doc.validate().is_err());
    doc.abbreviations[0].label_style.as_mut().unwrap().size_pt = 12.;
    doc.validate().unwrap();
}

#[test]
fn visible_group_and_expanded_atom_resolve_against_their_own_highlights() {
    use reshiki::canvas_theme::{self, CanvasTheme};
    use reshiki::scene::{self, Primitive};
    for canvas in CanvasTheme::ALL {
        for explicit_group_color in [false, true] {
            let (mut doc, anchor) = grouped();
            doc.canvas_theme = canvas;
            let atom = doc.atom_mut(anchor).unwrap();
            atom.text_style.as_mut().unwrap().color = Color::Ink;
            atom.display.color_override = false;
            atom.display.highlight = Some(Color::Custom([255; 3]));
            let group = doc.abbreviations.first_mut().unwrap();
            group.highlight = Some(Color::Custom([0; 3]));
            group.label_style = Some(TextStyle {
                size_pt: 17.,
                bold: true,
                color: if explicit_group_color {
                    Color::Custom([124; 3])
                } else {
                    Color::Ink
                },
                ..Default::default()
            });
            group.label_color_override = explicit_group_color;
            let before = doc.clone();
            let visible = canvas.color(canvas_theme::atom_color(&doc, doc.atom(anchor).unwrap()));
            if explicit_group_color {
                assert_eq!(visible, [124; 3]);
            } else {
                assert!(reshiki::color_contrast::contrast(visible, [0; 3]) >= 4.5);
            }
            let resolved = canvas_theme::resolved_document(&doc).into_owned();
            assert_eq!(doc, before);
            assert_eq!(
                canvas_theme::resolved_document(&resolved).as_ref(),
                &resolved
            );
            let group_ink = reshiki::palette::Palette::of(&resolved).rgb(
                resolved
                    .abbreviation(anchor)
                    .unwrap()
                    .text_style(&resolved)
                    .color,
            );
            assert_eq!(group_ink, visible);
            let inner_ink = reshiki::palette::Palette::of(&resolved).rgb(
                resolved
                    .atom(anchor)
                    .unwrap()
                    .text_style
                    .as_ref()
                    .unwrap()
                    .color,
            );
            assert!(reshiki::color_contrast::contrast(inner_ink, [255; 3]) >= 4.5);
            let drawing = scene::primitives(&doc);
            let label: Vec<_> = drawing
                .iter()
                .filter_map(|primitive| match primitive {
                    Primitive::Text { style, color, .. } => Some((style, *color)),
                    _ => None,
                })
                .collect();
            assert!(!label.is_empty());
            assert!(label.iter().all(|(style, color)| style.size_pt == 17.
                && style.bold
                && canvas.color(*color) == visible));
            let mut expanded = resolved.clone();
            expanded.expand_abbreviations(&[anchor]);
            assert_eq!(expanded.atom(anchor), resolved.atom(anchor));
            assert_eq!(
                canvas.color(canvas_theme::atom_color(
                    &expanded,
                    expanded.atom(anchor).unwrap()
                )),
                inner_ink
            );
        }
    }
}

#[test]
fn themes_reset_wrapper_color_overrides_without_changing_independent_typography_or_paint() {
    use reshiki::canvas_theme::ColorTheme;
    let (mut doc, anchor) = grouped();
    let group = doc.abbreviations.first_mut().unwrap();
    group.highlight = Some(Color::Custom([190, 230, 240]));
    group.label_style = Some(TextStyle {
        size_pt: 18.,
        italic: true,
        color: Color::Custom([180, 40, 70]),
        ..Default::default()
    });
    group.label_color_override = true;
    ColorTheme::Pastel.apply(&mut doc);
    let group = doc.abbreviation(anchor).unwrap();
    assert_eq!(group.text_style(&doc).size_pt, 18.);
    assert!(group.text_style(&doc).italic);
    assert_eq!(group.text_style(&doc).color, Color::Ink);
    assert!(!group.color_override(&doc));
    assert_eq!(group.highlight, Some(Color::Custom([190, 230, 240])));
    assert_eq!(
        doc.atom(anchor)
            .unwrap()
            .text_style
            .as_ref()
            .unwrap()
            .size_pt,
        11.
    );
}

#[test]
fn label_typography_survives_native_copy_paste_and_typed_group_replacement() {
    use reshiki::{
        atom_text::{self, Mode},
        canvas_theme::{self, CanvasTheme},
        editing,
    };
    let (mut doc, anchor) = grouped();
    let chosen = TextStyle {
        size_pt: 18.,
        italic: true,
        color: Color::Custom([180, 40, 70]),
        ..Default::default()
    };
    doc.abbreviations[0].label_style = Some(chosen.clone());
    doc.abbreviations[0].label_color_override = true;
    let part = editing::selection(&doc, &[anchor]);
    for canvas in CanvasTheme::ALL {
        let mut destination = Document::default();
        destination.canvas_theme = canvas;
        let pasted = canvas_theme::for_native_paste(part.clone(), canvas);
        let ids = editing::append(&mut destination, &pasted, Point::new(150., 90.));
        let group = destination.abbreviations.first().unwrap();
        assert!(ids.contains(&group.anchor));
        assert_eq!(group.label_style.as_ref(), Some(&chosen));
        assert!(group.label_color_override);
    }
    let replaced = atom_text::apply(&doc, anchor, "OEt", Mode::Group).unwrap();
    let group = replaced.abbreviation(anchor).unwrap();
    assert_eq!(group.label_style.as_ref(), Some(&chosen));
    assert!(group.label_color_override);
    assert_eq!(
        replaced.atom(anchor).unwrap().text_style,
        doc.atom(anchor).unwrap().text_style
    );
}

#[test]
fn drawing_style_updates_matching_wrapper_fonts_without_overwriting_internal_overrides() {
    let (mut doc, anchor) = grouped();
    doc.abbreviations[0].label_style = Some(doc.drawing_style.text_style());
    let mut style = doc.drawing_style.clone();
    style.font_size_pt = 16.;
    let updated = reshiki::document_styles::apply(&doc, style, true, false).unwrap();
    assert_eq!(
        updated
            .abbreviation(anchor)
            .unwrap()
            .text_style(&updated)
            .size_pt,
        16.
    );
    assert_eq!(
        updated
            .atom(anchor)
            .unwrap()
            .text_style
            .as_ref()
            .unwrap()
            .size_pt,
        11.
    );
}

#[test]
fn contrast_checks_use_visible_wrapper_ink_and_ignore_hidden_atom_styles() {
    let mut doc = Document::default();
    let anchor = doc.add_atom("C", Point::default());
    let oxygen = doc.add_atom("O", Point::new(42., 0.));
    doc.add_bond(anchor, oxygen, 1, "plain");
    doc.atom_mut(oxygen).unwrap().text_style = Some(TextStyle {
        color: Color::Custom([255; 3]),
        ..Default::default()
    });
    doc.atom_mut(oxygen).unwrap().display.color_override = true;
    doc.contract(&[anchor, oxygen], "Group", "Group").unwrap();
    doc.abbreviations[0].label_style = Some(TextStyle::default());
    doc.abbreviations[0].label_color_override = true;
    assert!(reshiki::canvas_theme::label_contrast_issues(&doc).is_empty());
    doc.abbreviations[0].label_style.as_mut().unwrap().color = Color::Custom([255; 3]);
    assert_eq!(
        reshiki::canvas_theme::label_contrast_issues(&doc),
        vec![anchor]
    );
    doc.expand_abbreviations(&[anchor]);
    assert_eq!(
        reshiki::canvas_theme::label_contrast_issues(&doc),
        vec![oxygen]
    );
}

#[test]
fn an_internal_condensed_group_uses_its_own_font_and_color() {
    let mut doc = Document::default();
    let anchor = doc.add_atom("C", Point::default());
    let left = doc.add_atom("C", Point::new(-42., 0.));
    let right = doc.add_atom("C", Point::new(42., 0.));
    doc.add_bond(anchor, left, 1, "plain");
    doc.add_bond(anchor, right, 1, "plain");
    doc.contract(&[anchor], "CH2", "H2C").unwrap();
    doc.abbreviations[0].label_style = Some(TextStyle {
        size_pt: 21.,
        color: Color::Custom([110, 40, 80]),
        bold: true,
        ..Default::default()
    });
    doc.abbreviations[0].label_color_override = true;
    let drawing = reshiki::scene::primitives(&doc);
    let core = drawing
        .iter()
        .find_map(|primitive| match primitive {
            reshiki::scene::Primitive::Text {
                text, style, color, ..
            } if text == "C" => Some((style, color)),
            _ => None,
        })
        .unwrap();
    assert_eq!(core.0.size_pt, 21.);
    assert!(core.0.bold);
    assert_eq!(*core.1, [110, 40, 80]);
}
