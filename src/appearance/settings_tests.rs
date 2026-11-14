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
                two_decimal_weight: true,
                name_typography: false,
                write_unknown_stereo: true,
                export_editable_drawing: true,
                export_svg_live_text: true,
                drawing_style_unit: Unit::Millimetres,
            };
            settings.save_to(&path).unwrap();
            let loaded = Settings::load_from(&path);
            assert_eq!(loaded.mode, mode);
            assert_eq!(loaded.canvas_theme, canvas_theme);
            assert!(!loaded.arrange_controls && !loaded.smart_guides);
            assert!(loaded.iupac_style_notes);
            assert!(loaded.two_decimal_weight);
            assert!(!loaded.name_typography);
            assert!(loaded.write_unknown_stereo);
            assert!(loaded.export_editable_drawing);
            assert!(loaded.export_svg_live_text);
            assert_eq!(loaded.drawing_style_unit, Unit::Millimetres);
        }
    }
}

#[test]
fn molecular_weight_precision_defaults_to_three_decimals_and_survives_reload() {
    assert!(!Settings::default().two_decimal_weight);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("appearance.json");
    std::fs::write(&path, r#"{"mode":"dark","smart_guides":false}"#).unwrap();
    let legacy = Settings::load_from(&path);
    assert!(!legacy.two_decimal_weight);
    assert!(!legacy.smart_guides);
    assert_eq!(legacy.mode, Mode::Dark);

    let mut settings = legacy;
    for two_decimal_weight in [true, false] {
        settings.two_decimal_weight = two_decimal_weight;
        settings.save_to(&path).unwrap();
        let loaded = Settings::load_from(&path);
        assert_eq!(loaded.two_decimal_weight, two_decimal_weight);
        assert!(!loaded.smart_guides);
        assert_eq!(loaded.mode, Mode::Dark);
    }
}

#[test]
fn editable_png_export_defaults_on_and_survives_reload() {
    assert!(Settings::default().export_editable_drawing);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("appearance.json");
    std::fs::write(&path, r#"{"mode":"dark","smart_guides":false}"#).unwrap();
    let mut settings = Settings::load_from(&path);
    assert!(settings.export_editable_drawing);
    for editable in [false, true] {
        settings.export_editable_drawing = editable;
        settings.save_to(&path).unwrap();
        let loaded = Settings::load_from(&path);
        assert_eq!(loaded.export_editable_drawing, editable);
        assert_eq!(loaded.mode, Mode::Dark);
        assert!(!loaded.smart_guides);
    }
}

#[test]
fn svg_live_text_defaults_off_and_survives_reload() {
    assert!(!Settings::default().export_svg_live_text);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("appearance.json");
    std::fs::write(&path, r#"{"mode":"dark","smart_guides":false}"#).unwrap();
    let mut settings = Settings::load_from(&path);
    assert!(!settings.export_svg_live_text);
    for live_text in [true, false] {
        settings.export_svg_live_text = live_text;
        settings.save_to(&path).unwrap();
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved["export_svg_live_text"], live_text);
        let loaded = Settings::load_from(&path);
        assert_eq!(loaded.export_svg_live_text, live_text);
        assert_eq!(loaded.mode, Mode::Dark);
        assert!(!loaded.smart_guides);
        assert!(loaded.export_editable_drawing);
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
        assert!(loaded.name_typography);
        assert!(!loaded.write_unknown_stereo);
    }
    for invalid in ["", "not JSON", r#"{"canvas_theme":"future-mode"}"#] {
        std::fs::write(&path, invalid).unwrap();
        let loaded = Settings::load_from(&path);
        assert_eq!(loaded.mode, Mode::MatchCanvas);
        assert_eq!(loaded.canvas_theme, CanvasTheme::Light);
        // The unreadable file was set aside, so there is none left to read.
        assert!(!path.exists());
    }
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

#[test]
fn unreadable_preferences_are_set_aside_before_a_save_replaces_them() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("appearance.json");
    let newer = br#"{"mode":"a-mode-from-a-newer-build","smart_guides":false}"#;
    std::fs::write(&path, newer).unwrap();
    let loaded = Settings::load_from(&path);
    assert_eq!(loaded.mode, Mode::default());
    loaded.save_to(&path).unwrap();
    assert_eq!(
        std::fs::read(dir.path().join("appearance.json.bad")).unwrap(),
        newer
    );

    // A readable file and a missing one leave nothing behind.
    std::fs::remove_file(dir.path().join("appearance.json.bad")).unwrap();
    assert_eq!(Settings::load_from(&path).mode, Mode::default());
    std::fs::remove_file(&path).unwrap();
    let _ = Settings::load_from(&path);
    assert!(!dir.path().join("appearance.json.bad").exists());
}
