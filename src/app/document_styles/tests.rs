use super::*;

#[test]
fn background_style_loads_and_save_status_stay_with_their_editor() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let _ = app.update(Message::DrawingStyle(Action::Open));
    let (id, epoch, serial) = (app.tab.id, app.tab.file_epoch, app.tab.styles.serial);
    let front = super::super::tabs::tests::Front::new(&mut app);
    let style = Preset::Nature.style();
    let _ = app.update(Message::Tab(
        id,
        Box::new(Message::DrawingStyle(Action::Loaded(
            serial,
            epoch,
            Ok(Some(style.clone())),
        ))),
    ));
    front.assert_unchanged(&app);
    assert_eq!(
        app.tabs.background[0]
            .styles
            .editor
            .as_ref()
            .unwrap()
            .candidate()
            .unwrap(),
        style
    );
    let _ = app.update(Message::Tab(
        id,
        Box::new(Message::DrawingStyle(Action::Saved(Ok(true)))),
    ));
    front.assert_unchanged(&app);
    assert_eq!(app.tabs.background[0].status, "Drawing style saved");
}

#[test]
fn element_tile_contrast_covers_all_themes_modes_and_states() {
    use iced::{Background, widget::button};
    use reshiki::{
        canvas_theme::{CanvasTheme, ColorTheme},
        color_contrast::{OUTLINE_TARGET, TEXT_TARGET, contrast},
    };
    let (mut app, _) = super::super::App::new();
    let mut pairs = 0;
    let mut text_min = 21_f64;
    let mut outline_min = 21_f64;
    for canvas in CanvasTheme::ALL {
        app.tab.doc.canvas_theme = canvas;
        for mode in crate::appearance::Mode::ALL {
            app.appearance.mode = mode;
            let theme = app.theme();
            let palettes = ColorTheme::ALL
                .into_iter()
                .map(|p| (p.to_string(), p, None))
                .chain(
                    reshiki::theme_files::bundled()
                        .unwrap()
                        .into_iter()
                        .map(|t| (t.name.clone(), t.base, Some(t))),
                );
            for (palette, base, custom) in palettes {
                base.apply(&mut app.tab.doc);
                if let Some(custom) = custom {
                    custom.apply(&mut app.tab.doc).unwrap();
                }
                for element in reshiki::editing::ELEMENTS {
                    for selected in [false, true] {
                        for state in [
                            button::Status::Active,
                            button::Status::Hovered,
                            button::Status::Pressed,
                        ] {
                            let style = super::super::workspace::element_control(
                                selected,
                                &app.tab.doc,
                                element,
                            )(&theme, state);
                            let outer = theme.palette().background;
                            let inner = match style.background {
                                Some(Background::Color(c)) if c.a == 1. => c,
                                _ => outer,
                            };
                            let ratio = contrast(
                                crate::appearance::rgb(style.text_color),
                                crate::appearance::rgb(inner),
                            );
                            text_min = text_min.min(ratio);
                            pairs += 1;
                            assert!(
                                ratio >= TEXT_TARGET,
                                "{palette}/{canvas}/{mode}/{element}/{selected}/{state:?}: {ratio}"
                            );
                            if selected {
                                for background in [inner, outer] {
                                    let ratio = contrast(
                                        crate::appearance::rgb(style.border.color),
                                        crate::appearance::rgb(background),
                                    );
                                    outline_min = outline_min.min(ratio);
                                    assert!(
                                        ratio >= OUTLINE_TARGET,
                                        "outline {palette}/{canvas}/{mode}/{element}: {ratio}"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    eprintln!(
        "Tiles: {pairs} text pairs, minimum {text_min:.4}:1; selected outline minimum {outline_min:.4}:1"
    );
}

#[test]
fn custom_keeps_current_dimensions_and_can_be_saved() {
    let (mut app, _) = App::new();
    let _ = app.update(Message::DrawingStyle(Action::Open));
    let _ = app.update(Message::DrawingStyle(Action::Preset(Preset::Nature)));
    let _ = app.update(Message::DrawingStyle(Action::Custom));
    let custom = app.tab.styles.editor.as_ref().unwrap().candidate().unwrap();
    let mut expected = Preset::Nature.style();
    expected.name = "Custom".into();
    assert_eq!(custom, expected);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("custom.reshiki-style");
    std::fs::write(&path, serde_json::to_vec(&custom).unwrap()).unwrap();
    assert_eq!(reshiki::document_styles::load(&path).unwrap(), custom);
    let _ = app.update(Message::DrawingStyle(Action::Apply));
    assert_eq!(app.tab.doc.drawing_style, custom);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc.drawing_style, DrawingStyle::default());
}

#[test]
fn interface_mode_never_changes_canvas_or_exports() {
    use reshiki::canvas_theme::CanvasTheme;
    let (mut app, _) = App::new();
    app.tab.doc = reshiki::rings::Preset::Regular.document(42., false);
    for (canvas, palette) in CanvasTheme::ALL
        .into_iter()
        .flat_map(|canvas| reshiki::canvas_theme::ColorTheme::ALL.map(|palette| (canvas, palette)))
    {
        app.tab.doc.canvas_theme = canvas;
        app.tab.doc.color_theme = palette;
        for preset in Preset::ALL {
            app.tab.doc.drawing_style = preset.style();
            let _ = app.update(Message::DrawingStyle(Action::Open));
            let before = app.tab.doc.clone();
            let saved = app.tab.saved.clone();
            let export = reshiki::export::figure(&app.tab.doc, "svg").unwrap().bytes;
            for mode in crate::appearance::Mode::ALL {
                let _ = app.update(Message::Appearance(mode));
                assert_eq!(
                    crate::appearance::is_dark(&app.theme()),
                    mode.is_dark(canvas)
                );
                assert_eq!(app.tab.doc, before);
                assert_eq!(app.tab.saved, saved);
                assert_eq!(
                    app.tab.styles.editor.as_ref().unwrap().candidate().unwrap(),
                    preset.style()
                );
                assert_eq!(
                    reshiki::export::figure(&app.tab.doc, "svg").unwrap().bytes,
                    export
                );
            }
        }
    }
}

#[test]
fn canvas_colors_and_quick_presets_are_independent_undo_steps() {
    use reshiki::canvas_theme::CanvasTheme;
    let (mut app, _) = App::new();
    app.tab.doc = reshiki::rings::Preset::Regular.document(42., false);
    let original = app.tab.doc.clone();
    let _ = app.update(Message::ColorTheme(
        reshiki::canvas_theme::ColorTheme::Presentation,
    ));
    let colored = app.tab.doc.clone();
    let _ = app.update(Message::CanvasTheme(CanvasTheme::Dark));
    assert_eq!(app.tab.doc.drawing_style, original.drawing_style);
    assert_eq!(app.tab.doc.atoms, original.atoms);
    let dark = app.tab.doc.clone();
    let _ = app.update(Message::QuickDrawingStyle(Choice::Journal(Preset::Nature)));
    assert_eq!(app.tab.doc.canvas_theme, CanvasTheme::Dark);
    assert_eq!(app.tab.doc.drawing_style, Preset::Nature.style());
    assert!(app.tab.styles.editor.is_none());
    assert_ne!(
        app.tab.doc.atoms, dark.atoms,
        "quick switching also scales bond geometry"
    );
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, dark);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, colored);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, colored);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, dark);
    let _ = app.update(Message::QuickDrawingStyle(Choice::Details));
    assert!(app.tab.styles.editor.is_some());
    assert_eq!(app.tab.doc, dark);
}

#[test]
fn every_preset_survives_editor_fields_without_becoming_custom() {
    for preset in Preset::ALL {
        let style = preset.style();
        let mut editor = Editor::new(&DrawingStyle::default(), 0, Unit::Points);
        editor.set(&style);
        assert_eq!(editor.candidate().unwrap(), style, "{preset}");
    }
}

#[test]
fn preview_cancel_apply_and_history_preserve_the_document() {
    let (mut app, _) = App::new();
    app.tab.doc = reshiki::rings::Preset::Regular.document(42., false);
    app.tab.busy = false;
    app.tab.history = Default::default();
    let before = app.tab.doc.clone();
    app.tab.saved = before.clone();
    let send = |app: &mut App, action| {
        let _ = app.update(Message::DrawingStyle(action));
    };
    send(&mut app, Action::Open);
    send(&mut app, Action::Preset(Preset::Presentation));
    assert_eq!(app.tab.doc, before);
    send(&mut app, Action::Cancel);
    assert_eq!(app.tab.doc, before);
    send(&mut app, Action::Open);
    send(&mut app, Action::Preset(Preset::Presentation));
    send(&mut app, Action::Input(Field::Line, "NaN".into()));
    send(&mut app, Action::Apply);
    assert_eq!(app.tab.doc, before);
    assert!(app.error);
    send(&mut app, Action::Preset(Preset::Presentation));
    send(&mut app, Action::Apply);
    assert!(!app.error);
    let after = app.tab.doc.clone();
    assert!(app.dirty(), "A style-only edit must be saved");
    assert_eq!(app.tab.drawing_length_input, "24");
    assert_eq!(app.tab.caption_format.style.size_pt, 16.);
    assert_eq!(app.tab.doc.atoms, before.atoms);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.drawing_length_input, "14.4");
    assert!(!app.dirty());
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, after);
    let _ = app.update(Message::ArrowStyle(reshiki::arrows::Preset::Fishhook));
    assert_eq!(app.tab.arrows.style.width_pt, 1.);
    assert_eq!(
        app.tab.arrows.numbers.first().map(String::as_str),
        Some("1")
    );
    send(&mut app, Action::Open);
    app.tab.file_epoch = app.tab.file_epoch.wrapping_add(1);
    send(&mut app, Action::Preset(Preset::Jacs));
    send(&mut app, Action::Apply);
    assert_eq!(app.tab.doc, after);
    assert!(app.error);
    let _ = app.update(Message::New);
    assert!(app.tab.doc.drawing_style.is_default());
    assert_eq!(app.tab.caption_format.style.size_pt, 10.);
    assert_eq!(app.tab.drawing_length_input, "14.4");
}

fn send(app: &mut App, action: Action) {
    let _ = app.update(Message::DrawingStyle(action));
}

fn input(editor: &Editor, field: Field) -> &Input {
    &editor.inputs.iter().find(|(f, _)| *f == field).unwrap().1
}

fn legacy_style_app() -> App {
    use reshiki::{
        document::Point,
        graphics::{BracketSides, Graphic, GraphicKind, GraphicStyle},
        typography::TextStyle,
    };
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let id = app.tab.doc.add_atom("N", Point::new(17.125, 31.75));
    app.tab.doc.atom_mut(id).unwrap().text_style = Some(TextStyle {
        size_pt: 10.0005,
        ..Default::default()
    });
    app.tab.doc.graphics.push(Graphic::dragged(
        id + 1,
        GraphicKind::Line,
        Point::new(90.25, 11.),
        Point::new(132.5, 11.),
        GraphicStyle {
            width_pt: 0.6005,
            ..Default::default()
        },
        BracketSides::Both,
        false,
    ));
    app.tab.doc.version = 14;
    app.tab.doc.validate().unwrap();
    app.tab.saved = app.tab.doc.clone();
    app
}

#[test]
fn display_only_apply_preserves_legacy_document_and_near_matching_overrides() {
    let mut app = legacy_style_app();
    let before = app.tab.doc.clone();
    let revision = app.tab.revision;
    let bond_settings = app.tab.drawing_length_input.clone();
    let graphic_settings = app.tab.graphic_width_input.clone();
    send(&mut app, Action::Open);
    for _ in 0..100 {
        for unit in Unit::ALL {
            send(&mut app, Action::DisplayUnit(unit));
            assert_eq!(
                app.tab.styles.editor.as_ref().unwrap().candidate().unwrap(),
                before.drawing_style
            );
            assert_eq!(app.tab.doc, before);
            assert!(!app.dirty());
            assert!(!app.tab.history.can_undo());
            assert!(!app.tab.history.can_redo());
        }
    }
    send(&mut app, Action::Advanced(true));
    let text = input(app.tab.styles.editor.as_ref().unwrap(), Field::Bond)
        .text
        .clone();
    send(&mut app, Action::Input(Field::Bond, text));
    send(&mut app, Action::CommitField(Field::Bond));
    assert!(
        !app.tab
            .styles
            .editor
            .as_ref()
            .unwrap()
            .apply_semantics_requested
    );
    send(&mut app, Action::Apply);
    assert!(app.tab.styles.editor.is_none());
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.saved, before);
    assert_eq!(app.tab.revision, revision);
    assert_eq!(app.tab.drawing_length_input, bond_settings);
    assert_eq!(app.tab.graphic_width_input, graphic_settings);
    assert!(!app.dirty());
    assert!(!app.tab.history.can_undo());
    let reopened =
        reshiki::document::Document::from_json(&serde_json::to_vec(&app.tab.doc).unwrap()).unwrap();
    assert_eq!(reopened.version, 14);
    assert_eq!(reopened.drawing_style, before.drawing_style);
    assert_eq!(reopened.graphics, before.graphics);
}

#[test]
fn display_only_apply_preserves_imported_style_string_whitespace() {
    let mut app = legacy_style_app();
    app.tab.doc.drawing_style.name = " Imported ".into();
    app.tab.doc.drawing_style.font_family = " Arial ".into();
    app.tab.doc.validate().unwrap();
    app.tab.saved = app.tab.doc.clone();
    let before = app.tab.doc.clone();
    let revision = app.tab.revision;
    send(&mut app, Action::Open);
    send(&mut app, Action::DisplayUnit(Unit::Millimetres));
    assert_eq!(
        app.tab.styles.editor.as_ref().unwrap().candidate().unwrap(),
        before.drawing_style
    );
    send(&mut app, Action::Apply);
    assert!(app.tab.styles.editor.is_none());
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.saved, before);
    assert_eq!(app.tab.revision, revision);
    assert!(!app.dirty());
    assert!(!app.tab.history.can_undo());

    send(&mut app, Action::Open);
    send(&mut app, Action::Name(" Renamed ".into()));
    send(&mut app, Action::Font(" Helvetica ".into()));
    let edited = app.tab.styles.editor.as_ref().unwrap().candidate().unwrap();
    assert_eq!(edited.name, "Renamed");
    assert_eq!(edited.font_family, "Helvetica");
    send(&mut app, Action::Apply);
    assert_eq!(app.tab.doc.drawing_style, edited);
    assert!(app.tab.history.can_undo());
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
}

#[test]
fn equal_style_deliberate_actions_keep_existing_apply_semantics() {
    for action in 0..5 {
        let mut app = legacy_style_app();
        let before = app.tab.doc.clone();
        send(&mut app, Action::Open);
        match action {
            0 => send(&mut app, Action::Preset(Preset::Jacs)),
            1 => {
                let (serial, epoch) = (app.tab.styles.serial, app.tab.file_epoch);
                send(
                    &mut app,
                    Action::Loaded(serial, epoch, Ok(Some(before.drawing_style.clone()))),
                );
            }
            2 => {
                send(&mut app, Action::Matching(false));
                send(&mut app, Action::Matching(true));
            }
            3 => {
                send(&mut app, Action::Scale(true));
                send(&mut app, Action::Scale(false));
            }
            _ => {
                send(&mut app, Action::Input(Field::Bond, "14.4 pt".into()));
                send(&mut app, Action::Name(before.drawing_style.name.clone()));
            }
        }
        send(&mut app, Action::DisplayUnit(Unit::Millimetres));
        let editor = app.tab.styles.editor.as_ref().unwrap();
        assert!(editor.apply_semantics_requested);
        assert_eq!(editor.candidate().unwrap(), before.drawing_style);
        send(&mut app, Action::Apply);
        assert_eq!(app.tab.doc.version, 15);
        assert_eq!(
            app.tab.doc.atoms[0].text_style.as_ref().unwrap().size_pt,
            10.
        );
        assert_eq!(app.tab.doc.graphics[0].style.width_pt, 0.6);
        assert!(app.dirty());
        assert!(app.tab.history.can_undo());
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
        assert!(
            !app.tab.history.can_undo(),
            "one Apply is one history entry"
        );
    }
}

#[test]
fn all_presets_and_native_precision_survive_display_cycles() {
    for mut style in Preset::ALL.into_iter().map(Preset::style) {
        // Older native files can retain a derived length within tolerance.
        style.bond_length_world += 0.0001;
        style.validate().unwrap();
        let mut editor = Editor::new(&style, 0, Unit::Points);
        for _ in 0..100 {
            for unit in Unit::ALL {
                editor.change_unit(unit).unwrap();
                assert_eq!(editor.candidate().unwrap(), style);
            }
        }
        assert!(!editor.apply_semantics_requested);
    }
}

#[test]
fn non_round_dimensions_survive_a_hundred_switches_and_untouched_apply() {
    let mut app = legacy_style_app();
    let style = &mut app.tab.doc.drawing_style;
    style.name = "Precise imported dimensions".into();
    style.set_bond_length(17.123_457);
    style.line_width_pt = 0.712_345_66;
    style.bold_width_pt = 2.456_789;
    style.margin_width_pt = 0.123_456_79;
    style.hash_spacing_pt = 3.456_789;
    app.tab.doc.validate().unwrap();
    let before = app.tab.doc.clone();
    app.tab.saved = before.clone();
    let revision = app.tab.revision;
    send(&mut app, Action::Open);
    for _ in 0..100 {
        for unit in [Unit::Millimetres, Unit::Centimetres, Unit::Points] {
            send(&mut app, Action::DisplayUnit(unit));
            let candidate = app.tab.styles.editor.as_ref().unwrap().candidate().unwrap();
            assert_eq!(candidate, before.drawing_style);
            for dimension in Dimension::ALL {
                assert_eq!(
                    dimension.get(&candidate).to_bits(),
                    dimension.get(&before.drawing_style).to_bits(),
                );
            }
        }
    }
    send(&mut app, Action::Apply);
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.saved, before);
    assert_eq!(app.tab.revision, revision);
    assert!(!app.dirty());
    assert!(!app.tab.history.can_undo());
    assert!(!app.tab.history.can_redo());
}

#[test]
fn unfinished_input_blocks_switch_and_apply_without_accepting_a_prefix() {
    let mut app = legacy_style_app();
    let before = app.tab.doc.clone();
    send(&mut app, Action::Open);
    for raw in ["", "-", ".", "5.", "1e", "5 m", "0,5 cm", "1e999", "-0"] {
        send(&mut app, Action::Input(Field::Bond, raw.into()));
        let accepted = input(app.tab.styles.editor.as_ref().unwrap(), Field::Bond).accepted;
        send(&mut app, Action::DisplayUnit(Unit::Millimetres));
        send(&mut app, Action::CommitField(Field::Bond));
        send(&mut app, Action::Apply);
        let editor = app.tab.styles.editor.as_ref().unwrap();
        assert_eq!(input(editor, Field::Bond).text, raw);
        assert_eq!(input(editor, Field::Bond).accepted, accepted);
        assert!(editor.candidate().is_err());
        assert_eq!(editor.display_unit, Unit::Points);
        assert_eq!(app.appearance.drawing_style_unit, Unit::Points);
        assert_eq!(app.tab.doc, before);
        assert!(!app.dirty());
    }
    send(&mut app, Action::Input(Field::Bond, "0.5 cm".into()));
    let candidate = app.tab.styles.editor.as_ref().unwrap().candidate().unwrap();
    assert_eq!(
        input(app.tab.styles.editor.as_ref().unwrap(), Field::Bond).accepted,
        before.drawing_style.bond_length_pt
    );
    send(&mut app, Action::DisplayUnit(Unit::Millimetres));
    let editor = app.tab.styles.editor.as_ref().unwrap();
    assert_eq!(editor.candidate().unwrap(), candidate);
    assert_eq!(input(editor, Field::Bond).text, "5");
    assert_eq!(app.tab.doc, before);
    send(&mut app, Action::CommitField(Field::Bond));
    assert_eq!(app.tab.doc, before);
    send(&mut app, Action::Cancel);
    assert_eq!(app.appearance.drawing_style_unit, Unit::Millimetres);
    assert_eq!(app.tab.doc, before);
    send(&mut app, Action::Open);
    assert_eq!(
        app.tab.styles.editor.as_ref().unwrap().display_unit,
        Unit::Millimetres
    );
}

#[test]
fn font_and_percentage_keep_their_units_and_do_not_block_dimension_switches() {
    let style = DrawingStyle::default();
    let mut editor = Editor::new(&style, 0, Unit::Points);
    editor
        .inputs
        .iter_mut()
        .find(|(f, _)| *f == Field::FontSize)
        .unwrap()
        .1
        .text = "10 mm".into();
    assert!(
        editor
            .candidate()
            .unwrap_err()
            .contains("Label size uses pt")
    );
    editor.change_unit(Unit::Centimetres).unwrap();
    editor
        .inputs
        .iter_mut()
        .find(|(f, _)| *f == Field::FontSize)
        .unwrap()
        .1
        .text = "10 pt".into();
    assert_eq!(editor.candidate().unwrap(), style);
    assert_eq!(input(&editor, Field::Spacing).text, "18");
    editor
        .inputs
        .iter_mut()
        .find(|(f, _)| *f == Field::Line)
        .unwrap()
        .1
        .text = "3 pt".into();
    assert!(editor.candidate().unwrap_err().contains("Bold width"));
    editor.change_unit(Unit::Millimetres).unwrap();
    assert!(
        editor.candidate().is_err(),
        "switching units must not hide cross-field validation"
    );
}

#[test]
fn invalid_name_or_percentage_allows_unit_choice_but_never_applies() {
    for action in [
        Action::Name(" ".into()),
        Action::Input(Field::Spacing, "1e".into()),
        Action::Input(Field::Spacing, "41".into()),
    ] {
        let mut app = legacy_style_app();
        let before = app.tab.doc.clone();
        let revision = app.tab.revision;
        send(&mut app, Action::Open);
        send(&mut app, Action::Input(Field::Bond, "5 mm".into()));
        send(&mut app, action);
        let editor = app.tab.styles.editor.as_ref().unwrap();
        let name = editor.name.clone();
        let spacing = input(editor, Field::Spacing).text.clone();
        assert!(editor.candidate().is_err());
        send(&mut app, Action::DisplayUnit(Unit::Centimetres));
        let editor = app.tab.styles.editor.as_ref().unwrap();
        assert_eq!(editor.display_unit, Unit::Centimetres);
        assert_eq!(input(editor, Field::Bond).text, "0.5");
        assert_eq!(editor.name, name);
        assert_eq!(input(editor, Field::Spacing).text, spacing);
        assert!(editor.candidate().is_err());
        assert_eq!(app.appearance.drawing_style_unit, Unit::Centimetres);
        send(&mut app, Action::Apply);
        assert!(app.error);
        assert!(app.tab.styles.editor.is_some());
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tab.saved, before);
        assert_eq!(app.tab.revision, revision);
        assert!(!app.dirty());
        assert!(!app.tab.history.can_undo());
    }
}

#[test]
fn invalid_style_export_schedules_no_dialog_and_keeps_the_entire_draft() {
    for action in [
        Action::Name(" ".into()),
        Action::Input(Field::FontSize, "10 mm".into()),
        Action::Input(Field::Spacing, "1e".into()),
        Action::Input(Field::Bond, "5 m".into()),
        Action::Input(Field::Line, "3 pt".into()),
    ] {
        let mut app = legacy_style_app();
        let before = app.tab.doc.clone();
        let revision = app.tab.revision;
        send(&mut app, Action::Open);
        send(&mut app, Action::DisplayUnit(Unit::Millimetres));
        send(&mut app, action);
        let editor = app.tab.styles.editor.as_ref().unwrap();
        let expected_error = editor.candidate().unwrap_err();
        let name = editor.name.clone();
        let fields = |editor: &Editor| {
            editor
                .inputs
                .iter()
                .map(|(field, input)| {
                    (
                        *field,
                        input.text.clone(),
                        input.rendered.clone(),
                        input.accepted.to_bits(),
                    )
                })
                .collect::<Vec<_>>()
        };
        let before_fields = fields(editor);
        for format in [SaveFormat::Native, SaveFormat::Cds] {
            let task = app.drawing_style_action(Action::Save(format));
            assert_eq!(
                task.units(),
                0,
                "invalid export must not open a file dialog"
            );
            assert!(app.error);
            assert_eq!(app.status, expected_error);
            let editor = app.tab.styles.editor.as_ref().unwrap();
            assert_eq!(editor.name, name);
            assert_eq!(fields(editor), before_fields);
            assert_eq!(editor.display_unit, Unit::Millimetres);
            assert_eq!(app.tab.doc, before);
            assert_eq!(app.tab.saved, before);
            assert_eq!(app.tab.revision, revision);
            assert!(!app.dirty());
            assert!(!app.tab.history.can_undo());
            assert!(!app.tab.history.can_redo());
        }
    }
}

#[test]
fn background_style_drafts_keep_incomplete_units_until_a_commit_boundary() {
    let mut app = legacy_style_app();
    let before = app.tab.doc.clone();
    send(&mut app, Action::Open);
    send(&mut app, Action::Input(Field::Bond, "5 m".into()));
    app.add_tab();
    app.tab.busy = false;
    send(&mut app, Action::Open);
    send(&mut app, Action::DisplayUnit(Unit::Millimetres));
    let hidden = app.tabs.background[0].styles.editor.as_ref().unwrap();
    assert_eq!(hidden.display_unit, Unit::Points);
    assert_eq!(input(hidden, Field::Bond).text, "5 m");
    app.select_tab(0);
    let editor = app.tab.styles.editor.as_ref().unwrap();
    assert_eq!(editor.display_unit, Unit::Points);
    assert!(
        editor
            .unit_notice
            .as_ref()
            .unwrap()
            .contains("still uses pt")
    );
    assert_eq!(input(editor, Field::Bond).text, "5 m");
    send(&mut app, Action::Input(Field::Bond, "5 mm".into()));
    assert_eq!(
        app.tab.styles.editor.as_ref().unwrap().display_unit,
        Unit::Points
    );
    send(&mut app, Action::CommitField(Field::Bond));
    let editor = app.tab.styles.editor.as_ref().unwrap();
    assert_eq!(editor.display_unit, Unit::Millimetres);
    assert!(editor.unit_notice.is_none());
    let candidate = editor.candidate().unwrap();
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    app.select_tab(1);
    send(&mut app, Action::DisplayUnit(Unit::Centimetres));
    app.select_tab(0);
    let editor = app.tab.styles.editor.as_ref().unwrap();
    assert_eq!(editor.display_unit, Unit::Centimetres);
    assert_eq!(editor.candidate().unwrap(), candidate);
    assert_eq!(app.tab.doc, before);
}

#[test]
fn choosing_the_retained_unit_cancels_pending_adoption_without_changing_either_tab() {
    for raw in ["1e", "5 m"] {
        let mut app = legacy_style_app();
        let before = app.tab.doc.clone();
        let revision = app.tab.revision;
        send(&mut app, Action::Open);
        send(&mut app, Action::Input(Field::Bond, raw.into()));
        app.add_tab();
        app.tab.busy = false;
        let other = app.tab.doc.clone();
        let other_revision = app.tab.revision;
        send(&mut app, Action::Open);
        send(&mut app, Action::DisplayUnit(Unit::Millimetres));
        app.select_tab(0);
        let editor = app.tab.styles.editor.as_ref().unwrap();
        assert!(editor.unit_notice.is_some());
        assert_eq!(editor.display_unit, Unit::Points);
        let original_inputs = editor
            .inputs
            .iter()
            .map(|(field, input)| {
                (
                    *field,
                    input.text.clone(),
                    input.rendered.clone(),
                    input.accepted.to_bits(),
                )
            })
            .collect::<Vec<_>>();
        send(&mut app, Action::DisplayUnit(Unit::Points));
        let editor = app.tab.styles.editor.as_ref().unwrap();
        assert!(editor.unit_notice.is_none());
        assert_eq!(app.appearance.drawing_style_unit, Unit::Points);
        assert_eq!(input(editor, Field::Bond).text, raw);
        assert_eq!(
            editor
                .inputs
                .iter()
                .map(|(field, input)| {
                    (
                        *field,
                        input.text.clone(),
                        input.rendered.clone(),
                        input.accepted.to_bits(),
                    )
                })
                .collect::<Vec<_>>(),
            original_inputs
        );
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tab.saved, before);
        assert_eq!(app.tab.revision, revision);
        assert!(!app.dirty());
        assert!(!app.tab.history.can_undo());
        assert_eq!(
            app.tabs.background[0]
                .styles
                .editor
                .as_ref()
                .unwrap()
                .display_unit,
            Unit::Millimetres
        );
        send(&mut app, Action::Cancel);
        send(&mut app, Action::Open);
        assert_eq!(
            app.tab.styles.editor.as_ref().unwrap().display_unit,
            Unit::Points
        );
        app.select_tab(1);
        assert_eq!(
            app.tab.styles.editor.as_ref().unwrap().display_unit,
            Unit::Points
        );
        assert_eq!(app.tab.doc, other);
        assert_eq!(app.tab.saved, other);
        assert_eq!(app.tab.revision, other_revision);
        assert!(!app.dirty());
        assert!(!app.tab.history.can_undo());
    }
}

#[test]
fn a_real_unit_input_applies_once_and_history_restores_the_exact_document() {
    let mut app = legacy_style_app();
    let before = app.tab.doc.clone();
    send(&mut app, Action::Open);
    send(&mut app, Action::Input(Field::Bond, "0.5 cm".into()));
    send(&mut app, Action::Input(Field::Line, "0.25 mm".into()));
    send(&mut app, Action::CommitField(Field::Line));
    assert_eq!(app.tab.doc, before);
    send(&mut app, Action::DisplayUnit(Unit::Centimetres));
    send(&mut app, Action::Apply);
    let after = app.tab.doc.clone();
    assert_eq!(
        after.drawing_style.bond_length_pt,
        units::parse("5 mm", Unit::Points).unwrap().points
    );
    assert_eq!(after.atoms[0].position, before.atoms[0].position);
    assert!(app.dirty());
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, after);
    assert_eq!(app.appearance.drawing_style_unit, Unit::Centimetres);
}

#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn drawing_style_fields_keep_focus_through_incomplete_units() {
    use iced::advanced::{
        Layout, layout, mouse,
        renderer::Headless,
        widget::{
            Id, Operation, Tree,
            operation::{Focusable, TextInput},
        },
    };
    use iced::keyboard::{self, Key, Modifiers, key};
    use iced::{Event, Rectangle, Size};

    struct Find<'a> {
        text: &'a str,
        bounds: Option<Rectangle>,
        matching: bool,
        focused: bool,
    }
    impl Operation for Find<'_> {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
            operate(self);
        }
        fn text_input(&mut self, _: Option<&Id>, bounds: Rectangle, state: &mut dyn TextInput) {
            self.matching = state.text() == self.text;
            if self.matching {
                self.bounds = Some(bounds);
            }
        }
        fn focusable(&mut self, _: Option<&Id>, _: Rectangle, state: &mut dyn Focusable) {
            if self.matching {
                self.focused |= state.is_focused();
            }
            self.matching = false;
        }
    }
    fn inspect(
        app: &App,
        renderer: &iced::Renderer,
        tree: &mut Tree,
        viewport: Rectangle,
        text: &str,
    ) -> (Rectangle, bool) {
        let mut view = app.view();
        tree.diff(view.as_widget());
        let node = view.as_widget_mut().layout(
            tree,
            renderer,
            &layout::Limits::new(viewport.size(), viewport.size()),
        );
        let mut find = Find {
            text,
            bounds: None,
            matching: false,
            focused: false,
        };
        view.as_widget_mut()
            .operate(tree, Layout::new(&node), renderer, &mut find);
        (find.bounds.expect("the Bond length field"), find.focused)
    }
    fn event(
        app: &mut App,
        renderer: &iced::Renderer,
        tree: &mut Tree,
        viewport: Rectangle,
        event: Event,
        cursor: mouse::Cursor,
    ) -> (iced::event::Status, bool) {
        let mut view = app.view();
        tree.diff(view.as_widget());
        let node = view.as_widget_mut().layout(
            tree,
            renderer,
            &layout::Limits::new(viewport.size(), viewport.size()),
        );
        let mut messages = Vec::new();
        let mut shell = iced::advanced::Shell::new(&mut messages);
        view.as_widget_mut().update(
            tree,
            &event,
            Layout::new(&node),
            cursor,
            renderer,
            &mut iced::advanced::clipboard::Null,
            &mut shell,
            &viewport,
        );
        let status = shell.event_status();
        drop(view);
        let committed = messages.iter().any(|message| {
            matches!(
                message,
                Message::DrawingStyle(Action::CommitField(Field::Bond))
            )
        });
        for message in messages {
            let _ = app.update(message);
        }
        (status, committed)
    }
    let press = |key: Key, code, modifiers, text: Option<&str>| {
        Event::Keyboard(keyboard::Event::KeyPressed {
            modified_key: key.clone(),
            key,
            physical_key: key::Physical::Code(code),
            location: keyboard::Location::Standard,
            modifiers,
            text: text.map(Into::into),
            repeat: false,
        })
    };
    let character = |c: char| {
        let code = match c {
            ' ' => key::Code::Space,
            '.' => key::Code::Period,
            '0' => key::Code::Digit0,
            '1' => key::Code::Digit1,
            '5' => key::Code::Digit5,
            'e' => key::Code::KeyE,
            'm' => key::Code::KeyM,
            _ => unreachable!(),
        };
        press(
            Key::Character(c.to_string().into()),
            code,
            Modifiers::empty(),
            Some(&c.to_string()),
        )
    };
    let renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .unwrap();
    let mut app = legacy_style_app();
    let before = app.tab.doc.clone();
    send(&mut app, Action::Open);
    send(&mut app, Action::DisplayUnit(Unit::Millimetres));
    let viewport = Rectangle::with_size(Size::new(1280., 1000.));
    let mut tree = Tree::empty();
    let command = if cfg!(target_os = "macos") {
        Modifiers::LOGO
    } else {
        Modifiers::CTRL
    };
    for (incomplete, completion, normalized) in [
        ("5 m", "m", "5"),
        ("", "5 mm", "5"),
        ("5.", "0 mm", "5"),
        ("5e", "0 mm", "5"),
    ] {
        let current = input(app.tab.styles.editor.as_ref().unwrap(), Field::Bond)
            .text
            .clone();
        let (bounds, _) = inspect(&app, &renderer, &mut tree, viewport, &current);
        for mouse_event in [
            mouse::Event::ButtonPressed(mouse::Button::Left),
            mouse::Event::ButtonReleased(mouse::Button::Left),
        ] {
            event(
                &mut app,
                &renderer,
                &mut tree,
                viewport,
                Event::Mouse(mouse_event),
                mouse::Cursor::Available(bounds.center()),
            );
        }
        for key_event in [
            Event::Keyboard(keyboard::Event::ModifiersChanged(command)),
            press(Key::Character("a".into()), key::Code::KeyA, command, None),
            Event::Keyboard(keyboard::Event::ModifiersChanged(Modifiers::empty())),
        ] {
            event(
                &mut app,
                &renderer,
                &mut tree,
                viewport,
                key_event,
                mouse::Cursor::Unavailable,
            );
        }
        let mut raw = String::new();
        if incomplete.is_empty() {
            event(
                &mut app,
                &renderer,
                &mut tree,
                viewport,
                press(
                    Key::Named(key::Named::Backspace),
                    key::Code::Backspace,
                    Modifiers::empty(),
                    None,
                ),
                mouse::Cursor::Unavailable,
            );
            assert!(inspect(&app, &renderer, &mut tree, viewport, "").1);
        }
        for (text, valid) in [(incomplete, false), (completion, true)] {
            for c in text.chars() {
                event(
                    &mut app,
                    &renderer,
                    &mut tree,
                    viewport,
                    character(c),
                    mouse::Cursor::Unavailable,
                );
                raw.push(c);
                assert_eq!(
                    input(app.tab.styles.editor.as_ref().unwrap(), Field::Bond).text,
                    raw
                );
                // Reconcile the changed view before checking focus, exactly
                // where removing the header's children used to lose it.
                assert!(
                    inspect(&app, &renderer, &mut tree, viewport, &raw).1,
                    "focus after {raw:?}"
                );
            }
            let editor = app.tab.styles.editor.as_ref().unwrap();
            let candidate = editor.candidate();
            assert_eq!(
                candidate.is_ok(),
                valid,
                "Bond length {raw:?} in {} produced {candidate:?}",
                editor.display_unit,
            );
        }
        let (status, committed) = event(
            &mut app,
            &renderer,
            &mut tree,
            viewport,
            press(
                Key::Named(key::Named::Enter),
                key::Code::Enter,
                Modifiers::empty(),
                None,
            ),
            mouse::Cursor::Unavailable,
        );
        assert_eq!(status, iced::event::Status::Captured);
        assert!(
            committed,
            "Enter must reach the field, not a global shortcut"
        );
        assert_eq!(app.inspector_tab, InspectorTab::DrawingStyle);
        assert_eq!(
            input(app.tab.styles.editor.as_ref().unwrap(), Field::Bond).text,
            normalized
        );
        assert!(!inspect(&app, &renderer, &mut tree, viewport, normalized).1);
        assert_eq!(app.tab.doc, before);
        assert!(!app.dirty());
        assert!(!app.tab.history.can_undo());
    }
}

#[tokio::test]
#[ignore = "Manual GPU snapshots without opening or controlling desktop windows"]
async fn drawing_style_headless_snapshot() {
    use iced::advanced::{layout, mouse, renderer::Headless, widget::Tree};
    let (mut app, _) = App::new();
    app.tab.doc = reshiki::rings::Preset::Regular.document(42., false);
    app.tab.busy = false;
    app.status = "Ready".into();
    let _ = app.update(Message::DrawingStyle(Action::Open));
    let _ = app.update(Message::DrawingStyle(Action::Preset(Preset::Nature)));
    let carbon = app.tab.doc.atoms[0].id;
    let p = app.tab.doc.atoms[0].position;
    // The first ring atom is the top vertex. Point its carbonyl outward,
    // leaving 120-degree bond angles instead of crowding a ring edge.
    let oxygen = app.tab.doc.add_atom("O", p.offset(0., -42.));
    app.tab.doc.add_bond(carbon, oxygen, 2, "plain");
    app.tab.doc.atoms[3].element = "N".into();
    app.tab.doc.atoms[3].label_h = 1;
    let directory = std::env::temp_dir().join("reshiki-document-style-qa");
    std::fs::create_dir_all(&directory).unwrap();
    for (name, width, height, dark, mode) in [
        (
            "desktop",
            1280,
            820,
            false,
            crate::appearance::Mode::MatchCanvas,
        ),
        (
            "compact",
            1040,
            680,
            false,
            crate::appearance::Mode::MatchCanvas,
        ),
        (
            "dark-desktop",
            1280,
            820,
            true,
            crate::appearance::Mode::MatchCanvas,
        ),
        (
            "dark-compact",
            1040,
            680,
            true,
            crate::appearance::Mode::MatchCanvas,
        ),
        (
            "dark-canvas-light-ui",
            1280,
            820,
            true,
            crate::appearance::Mode::Light,
        ),
        (
            "light-canvas-dark-ui",
            1280,
            820,
            false,
            crate::appearance::Mode::Dark,
        ),
        (
            "presentation-light",
            1280,
            820,
            false,
            crate::appearance::Mode::Light,
        ),
        (
            "presentation-dark",
            1280,
            820,
            true,
            crate::appearance::Mode::Dark,
        ),
        (
            "pastel-light",
            1280,
            820,
            false,
            crate::appearance::Mode::Dark,
        ),
        (
            "pastel-dark",
            1280,
            820,
            true,
            crate::appearance::Mode::Light,
        ),
        (
            "selected-compact",
            1040,
            680,
            true,
            crate::appearance::Mode::MatchCanvas,
        ),
    ] {
        app.tab.doc.canvas_theme = if dark {
            reshiki::canvas_theme::CanvasTheme::Dark
        } else {
            reshiki::canvas_theme::CanvasTheme::Light
        };
        app.tab.doc.color_theme = if name.starts_with("presentation") {
            reshiki::canvas_theme::ColorTheme::Presentation
        } else if name.starts_with("pastel") {
            reshiki::canvas_theme::ColorTheme::Pastel
        } else {
            reshiki::canvas_theme::ColorTheme::Publication
        };
        app.appearance.mode = mode;
        app.sync_color_input();
        app.tab.selected = if name == "selected-compact" {
            app.tab.doc.all_ids()
        } else {
            vec![]
        };
        app.grid = dark;
        app.guides.rulers = dark;
        app.view_open = dark;
        let mut renderer = <iced::Renderer as Headless>::new(
            iced::Font::with_name(reshiki::style::ui_font_family()),
            iced::Pixels(16.),
            None,
        )
        .await
        .unwrap();
        let size = iced::Size::new(width as f32, height as f32);
        let theme = app.theme();
        let mut view = app.view();
        let mut tree = Tree::new(view.as_widget());
        let node =
            view.as_widget_mut()
                .layout(&mut tree, &renderer, &layout::Limits::new(size, size));
        let mut messages = Vec::new();
        view.as_widget_mut().update(
            &mut tree,
            &iced::Event::Window(iced::window::Event::RedrawRequested(
                std::time::Instant::now(),
            )),
            iced::advanced::Layout::new(&node),
            mouse::Cursor::Unavailable,
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut iced::advanced::Shell::new(&mut messages),
            &iced::Rectangle::with_size(size),
        );
        view.as_widget().draw(
            &tree,
            &mut renderer,
            &theme,
            &iced::advanced::renderer::Style::default(),
            iced::advanced::Layout::new(&node),
            mouse::Cursor::Unavailable,
            &iced::Rectangle::with_size(size),
        );
        let pixels = Headless::screenshot(
            &mut renderer,
            iced::Size::new(width, height),
            1.,
            theme.palette().background,
        );
        image::save_buffer(
            directory.join(format!("{name}.png")),
            &pixels,
            width,
            height,
            image::ColorType::Rgba8,
        )
        .unwrap();
        let cursor = mouse::Cursor::Available(iced::Point::new(
            width as f32 - 36.,
            height as f32 - 73. - if dark { 44. } else { 0. },
        ));
        for event in [
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        ] {
            view.as_widget_mut().update(
                &mut tree,
                &event,
                iced::advanced::Layout::new(&node),
                cursor,
                &renderer,
                &mut iced::advanced::clipboard::Null,
                &mut iced::advanced::Shell::new(&mut messages),
                &iced::Rectangle::with_size(size),
            );
        }
        assert!(
            messages
                .iter()
                .any(|m| matches!(m, Message::DrawingStyle(Action::Apply))),
            "Save icon stays visible and clickable at {width}×{height}"
        );
    }
}
