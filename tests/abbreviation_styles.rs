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
