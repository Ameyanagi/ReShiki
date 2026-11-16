use super::*;

#[test]
fn legacy_drawing_style_defaults_chain_centering_to_false_without_changing_json() {
    let legacy: serde_json::Value =
        serde_json::from_str(include_str!("../../../../assets/drawing_style.json")).unwrap();
    let style: DrawingStyle = serde_json::from_value(legacy.clone()).unwrap();
    assert!(!style.center_chain_double_bonds);
    assert_eq!(style, DrawingStyle::default());
    let saved: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&style).unwrap()).unwrap();
    assert_eq!(saved, legacy);
}

#[test]
fn centered_chain_double_bonds_round_trip_and_false_is_omitted() {
    let mut style = DrawingStyle {
        center_chain_double_bonds: true,
        ..Default::default()
    };
    let mut value = serde_json::to_value(&style).unwrap();
    assert_eq!(value["center_chain_double_bonds"], true);
    assert_eq!(
        serde_json::from_value::<DrawingStyle>(value.clone()).unwrap(),
        style
    );
    value["center_chain_double_bonds"] = false.into();
    style.center_chain_double_bonds = false;
    assert_eq!(
        serde_json::from_value::<DrawingStyle>(value).unwrap(),
        style
    );
    assert!(
        serde_json::to_value(style)
            .unwrap()
            .get("center_chain_double_bonds")
            .is_none()
    );
}

/// Japanese fonts center "…" between the lines, so "Export PDF…" read as
/// "Export PDF···"; the interface font sets it on the baseline.
#[test]
fn interface_font_sets_the_ellipsis_on_the_baseline() {
    use resvg::usvg::fontdb::{Family, Query};
    let family = ui_font_family();
    let fonts = fonts();
    let id = (fonts.db)
        .query(&Query {
            families: &[Family::Name(family)],
            ..Query::default()
        })
        .expect("the interface font is installed");
    let top = (fonts.db)
        .with_face_data(id, |bytes, index| {
            let face = font::face(bytes, index, 400)?;
            let bounds = face.glyph_bounding_box(face.glyph_index('…')?)?;
            Some(f32::from(bounds.y_max) / f32::from(face.units_per_em()))
        })
        .flatten()
        .expect("the interface font has an ellipsis");
    assert!(top < 0.2, "{family} draws … {top} em above the baseline");
}
