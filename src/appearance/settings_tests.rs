use super::*;
use reshiki::style::units::Unit;

#[test]
fn drawing_units_preserve_legacy_and_unrelated_preferences() {
    for json in [
        r#"{"mode":"dark","arrange_controls":false}"#,
        r#"{"mode":"dark","arrange_controls":false,"drawing_style_unit":"future-unit"}"#,
    ] {
        let settings: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.drawing_style_unit, Unit::Points);
        assert_eq!(settings.mode, Mode::Dark);
        assert!(!settings.arrange_controls);
    }
    for unit in Unit::ALL {
        let settings = Settings {
            drawing_style_unit: unit,
            ..Default::default()
        };
        let saved = serde_json::to_vec(&settings).unwrap();
        let loaded: Settings = serde_json::from_slice(&saved).unwrap();
        assert_eq!(loaded.drawing_style_unit, unit);
    }
}
