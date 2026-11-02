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

#[test]
fn appearance_preferences_survive_file_reload_independently() {
    use reshiki::canvas_theme::CanvasTheme;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings/appearance.json");
    for mode in Mode::ALL {
        for canvas_theme in CanvasTheme::ALL {
            let settings = Settings {
                mode,
                canvas_theme,
                arrange_controls: false,
                smart_guides: false,
                iupac_style_notes: true,
                drawing_style_unit: Unit::Millimetres,
            };
            settings.save_to(&path).unwrap();
            let loaded = Settings::load_from(&path);
            assert_eq!(loaded.mode, mode);
            assert_eq!(loaded.canvas_theme, canvas_theme);
            assert!(!loaded.arrange_controls && !loaded.smart_guides);
            assert!(loaded.iupac_style_notes);
            assert_eq!(loaded.drawing_style_unit, Unit::Millimetres);
        }
    }
}

#[test]
fn older_preferences_keep_interface_choice_and_default_to_light_canvas() {
    use reshiki::canvas_theme::CanvasTheme;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("appearance.json");
    for (json, expected) in [
        (r#"{"dark":true}"#, Mode::Dark),
        (r#"{"dark":false}"#, Mode::Light),
        (r#"{"mode":"match_canvas"}"#, Mode::MatchCanvas),
        (r#"{"mode":"light","dark":true}"#, Mode::Light),
        (r#"{"mode":"dark","smart_guides":false}"#, Mode::Dark),
    ] {
        std::fs::write(&path, json).unwrap();
        let loaded = Settings::load_from(&path);
        assert_eq!(loaded.mode, expected, "{json}");
        assert_eq!(loaded.canvas_theme, CanvasTheme::Light);
        assert!(!loaded.iupac_style_notes);
    }
    for invalid in ["", "not JSON", r#"{"canvas_theme":"future-mode"}"#] {
        std::fs::write(&path, invalid).unwrap();
        let loaded = Settings::load_from(&path);
        assert_eq!(loaded.mode, Mode::MatchCanvas);
        assert_eq!(loaded.canvas_theme, CanvasTheme::Light);
    }
    std::fs::remove_file(&path).unwrap();
    assert_eq!(Settings::load_from(&path).canvas_theme, CanvasTheme::Light);
}

#[test]
fn an_unwritable_preference_destination_reports_failure() {
    let dir = tempfile::tempdir().unwrap();
    let parent = dir.path().join("file-not-directory");
    std::fs::write(&parent, "keep this file").unwrap();
    let settings = Settings {
        canvas_theme: reshiki::canvas_theme::CanvasTheme::Dark,
        ..Default::default()
    };
    let path = parent.join("appearance.json");
    assert!(settings.save_to(&path).is_err());
    assert_eq!(std::fs::read_to_string(&parent).unwrap(), "keep this file");
    std::fs::remove_file(&parent).unwrap();
    settings.save_to(&path).unwrap();
    assert_eq!(
        Settings::load_from(&path).canvas_theme,
        settings.canvas_theme
    );
}
