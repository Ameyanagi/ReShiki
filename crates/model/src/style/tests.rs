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
