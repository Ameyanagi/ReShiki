use super::{
    App, InspectorTab, Message,
    icons::{Glyph, Icon},
};
use crate::canvas::{DrawingThumbnail, layered::canvas};
use iced::widget::{
    Space, button, checkbox, column, combo_box, container, row, scrollable, text, tooltip,
};
use iced::{Alignment, Element, Length, Task};
use reshiki::{
    document_styles::Preset,
    style::{
        DrawingStyle,
        units::{self, Dimension, Unit},
    },
};

fn command(label: &str) -> iced::widget::Button<'_, Message> {
    button(text(label).size(12)).padding([7, 9])
}

#[cfg(test)]
mod tests {
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
        for (canvas, palette) in CanvasTheme::ALL.into_iter().flat_map(|canvas| {
            reshiki::canvas_theme::ColorTheme::ALL.map(|palette| (canvas, palette))
        }) {
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
            reshiki::document::Document::from_json(&serde_json::to_vec(&app.tab.doc).unwrap())
                .unwrap();
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Bond,
    FontSize,
    Line,
    Bold,
    Margin,
    Hash,
    Spacing,
}
impl Field {
    fn dimension(self) -> Option<Dimension> {
        Some(match self {
            Self::Bond => Dimension::Bond,
            Self::Line => Dimension::Line,
            Self::Bold => Dimension::Bold,
            Self::Margin => Dimension::Margin,
            Self::Hash => Dimension::Hash,
            Self::FontSize | Self::Spacing => return None,
        })
    }
    fn label(self, unit: Unit) -> String {
        if let Some(dimension) = self.dimension() {
            format!("{} ({unit})", dimension.name())
        } else {
            match self {
                Self::FontSize => "Label size (pt)",
                _ => "Bond spacing (%)",
            }
            .into()
        }
    }
    fn format(self, value: f32, unit: Unit) -> String {
        if self.dimension().is_some() {
            units::format(value, unit)
        } else {
            value.to_string()
        }
    }
    fn parse(self, text: &str, unit: Unit) -> Result<f32, String> {
        let value = if self == Self::Spacing {
            text.trim()
                .parse::<f32>()
                .map_err(|_| "Enter a number for bond spacing (%).".to_string())?
        } else {
            let parsed = units::parse(
                text,
                if self == Self::FontSize {
                    Unit::Points
                } else {
                    unit
                },
            )
            .map_err(|error| format!("{}: {error}", self.label(unit)))?;
            if self == Self::FontSize && parsed.unit != Unit::Points {
                return Err("Label size uses pt; enter a bare number or add pt.".into());
            }
            parsed.points
        };
        self.validate(value, unit)?;
        Ok(value)
    }
    fn validate(self, value: f32, unit: Unit) -> Result<(), String> {
        if let Some(dimension) = self.dimension() {
            return dimension.validate(value, unit);
        }
        let (min, max, suffix) = if self == Self::FontSize {
            (4., 144., "pt")
        } else {
            (5., 40., "%")
        };
        if value.is_finite() && (min..=max).contains(&value) {
            Ok(())
        } else {
            Err(format!(
                "{} must be between {min} and {max} {suffix}.",
                self.label(unit)
            ))
        }
    }
}

/// `accepted` is never reconstructed from rounded display text. While typing,
/// candidate() reads a provisional value without accepting or applying it.
struct Input {
    text: String,
    rendered: String,
    accepted: f32,
}
impl Input {
    fn new(field: Field, value: f32, unit: Unit) -> Self {
        let text = field.format(value, unit);
        Self {
            rendered: text.clone(),
            text,
            accepted: value,
        }
    }
    fn value(&self, field: Field, unit: Unit) -> Result<f32, String> {
        if self.text == self.rendered {
            field.validate(self.accepted, unit)?;
            Ok(self.accepted)
        } else {
            field.parse(&self.text, unit)
        }
    }
    fn accept(&mut self, field: Field, value: f32, unit: Unit) {
        *self = Self::new(field, value, unit);
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    Journal(Preset),
    Custom,
    Details,
}
impl std::fmt::Display for Choice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Journal(preset) => preset.fmt(f),
            Self::Custom => f.write_str("Custom"),
            Self::Details => f.write_str("Manage styles…"),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveFormat {
    Native,
    Cds,
}
impl SaveFormat {
    fn extension(self) -> &'static str {
        match self {
            Self::Native => "reshiki-style",
            Self::Cds => "cds",
        }
    }
}
impl std::fmt::Display for SaveFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Native => "ReShiki style",
            Self::Cds => "ChemDraw CDS",
        })
    }
}
#[derive(Debug, Clone)]
pub enum Action {
    Open,
    Cancel,
    Apply,
    Preset(Preset),
    Custom,
    Source(Preset),
    Name(String),
    Font(String),
    Input(Field, String),
    CommitField(Field),
    DisplayUnit(Unit),
    Advanced(bool),
    Matching(bool),
    Scale(bool),
    Load,
    Save(SaveFormat),
    ExportMenu(bool),
    Loaded(u64, u64, Result<Option<DrawingStyle>, String>),
    Saved(Result<bool, String>),
}
#[derive(Default)]
pub struct State {
    pub editor: Option<Editor>,
    serial: u64,
}
pub struct Editor {
    font_options: iced::widget::combo_box::State<String>,
    original: DrawingStyle,
    base: DrawingStyle,
    epoch: u64,
    name: String,
    font: String,
    inputs: Vec<(Field, Input)>,
    display_unit: Unit,
    unit_notice: Option<String>,
    /// Preserve deliberate Apply semantics even if the candidate equals the
    /// original (matching overrides can still be normalized).
    apply_semantics_requested: bool,
    advanced: bool,
    export_menu: bool,
    matching: bool,
    scale: bool,
}
impl Editor {
    fn new(style: &DrawingStyle, epoch: u64, unit: Unit) -> Self {
        let mut editor = Self {
            font_options: iced::widget::combo_box::State::new(
                reshiki::style::font_families()
                    .iter()
                    .map(|name| (*name).to_owned())
                    .collect(),
            ),
            original: style.clone(),
            base: style.clone(),
            epoch,
            name: String::new(),
            font: String::new(),
            inputs: vec![],
            display_unit: unit,
            unit_notice: None,
            apply_semantics_requested: false,
            advanced: false,
            export_menu: false,
            matching: true,
            scale: false,
        };
        editor.set(style);
        editor
    }
    fn set(&mut self, style: &DrawingStyle) {
        self.base = style.clone();
        self.name = style.name.clone();
        self.font = style.font_family.clone();
        self.unit_notice = None;
        self.inputs = [
            (Field::FontSize, style.font_size_pt),
            (Field::Bond, style.bond_length_pt),
            (Field::Line, style.line_width_pt),
            (Field::Bold, style.bold_width_pt),
            (Field::Margin, style.margin_width_pt),
            (Field::Hash, style.hash_spacing_pt),
            (Field::Spacing, style.bond_spacing_ratio * 100.),
        ]
        .into_iter()
        .map(|(field, n)| (field, Input::new(field, n, self.display_unit)))
        .collect();
    }
    fn candidate(&self) -> Result<DrawingStyle, String> {
        let mut style = self.base.clone();
        // Imported styles can contain surrounding whitespace. Preserve an
        // untouched string exactly, just as we preserve untouched dimensions.
        if self.name != self.base.name {
            style.name = self.name.trim().into();
        }
        if self.font != self.base.font_family {
            style.font_family = self.font.trim().into();
        }
        for (field, input) in &self.inputs {
            let value = input.value(*field, self.display_unit)?;
            if let Some(dimension) = field.dimension() {
                // Preserve imported/native derived coordinates exactly for an
                // unchanged point value, even within validation tolerance.
                if value.to_bits() != dimension.get(&style).to_bits() {
                    dimension.set(&mut style, value);
                }
                continue;
            }
            match field {
                Field::FontSize => style.font_size_pt = value,
                Field::Spacing if value != style.bond_spacing_ratio * 100. => {
                    style.bond_spacing_ratio = value / 100.
                }
                _ => {}
            }
        }
        style.validate()?;
        Ok(style)
    }

    fn change_unit(&mut self, unit: Unit) -> Result<(), String> {
        if self.display_unit == unit {
            self.unit_notice = None;
            return Ok(());
        }
        // Preflight the whole conversion before touching any text or value.
        let values = self
            .inputs
            .iter()
            .enumerate()
            .filter(|(_, (field, _))| field.dimension().is_some())
            .map(|(index, (field, input))| {
                input
                    .value(*field, self.display_unit)
                    .map(|value| (index, value))
            })
            .collect::<Result<Vec<_>, _>>()?;
        for (index, value) in values {
            let (field, input) = &mut self.inputs[index];
            input.accept(*field, value, unit);
        }
        self.display_unit = unit;
        self.unit_notice = None;
        Ok(())
    }

    fn commit_field(&mut self, field: Field) -> Result<(), String> {
        if let Some((_, input)) = self.inputs.iter_mut().find(|(f, _)| *f == field) {
            let value = input.value(field, self.display_unit)?;
            input.accept(field, value, self.display_unit);
        }
        Ok(())
    }

    fn mark_custom(&mut self) {
        self.apply_semantics_requested = true;
        if Preset::ALL.iter().any(|p| self.name == p.to_string()) || self.name == "Presentation" {
            self.name = "Custom".into();
        }
    }
}

impl App {
    /// Hidden invalid drafts retain their original unit context. Retry only at
    /// a deliberate boundary, never halfway through typing a number.
    pub(super) fn sync_drawing_style_unit(&mut self) {
        let preferred = self.appearance.drawing_style_unit;
        if let Some(editor) = &mut self.tab.styles.editor
            && editor.change_unit(preferred).is_err()
        {
            editor.unit_notice = Some(format!(
                "This draft still uses {}. Finish the dimension and press Enter to use {preferred}.",
                editor.display_unit
            ));
        }
    }

    pub(super) fn quick_drawing_style(&mut self, choice: Choice) -> Task<Message> {
        match choice {
            Choice::Details | Choice::Custom => {
                let task = self.drawing_style_action(Action::Open);
                if choice == Choice::Custom
                    && let Some(editor) = &mut self.tab.styles.editor
                {
                    editor.apply_semantics_requested = true;
                    editor.name = "Custom".into();
                }
                task
            }
            Choice::Journal(preset) => {
                self.cancel_join();
                if self.tab.cleanup.is_some() || !self.finish_inline(true) {
                    return Task::none();
                }
                match reshiki::document_styles::apply(&self.tab.doc, preset.style(), true, true) {
                    Ok(doc) => {
                        let before = self.tab.doc.clone();
                        self.tab.doc = doc;
                        self.changed(before);
                        self.tab.styles.editor = None;
                        if self.inspector_tab == InspectorTab::DrawingStyle {
                            self.inspector_tab = InspectorTab::Properties;
                        }
                        if !self.error {
                            self.status = format!(
                                "{preset} applied · Undo restores the previous style and layout"
                            );
                        }
                    }
                    Err(error) => {
                        self.status = error;
                        self.error = true;
                    }
                }
                Task::none()
            }
        }
    }

    pub(super) fn sync_drawing_defaults(&mut self) {
        let style = &self.tab.doc.drawing_style;
        self.tab.bond_drawing.length = style.bond_length_world;
        self.tab.drawing_length_input = style.bond_length_pt.to_string();
        self.tab.caption_format = reshiki::typography::TextFormat {
            style: style.text_style(),
            ..Default::default()
        };
        self.tab.caption_target = None;
        self.tab.graphic_style.width_pt = style.line_width_pt;
        self.tab.graphic_width_input = style.line_width_pt.to_string();
        self.tab.arrows.style.width_pt = style.line_width_pt;
        self.tab
            .arrows
            .refresh_inputs(&reshiki::palette::Palette::of(&self.tab.doc));
        self.sync_style_inputs();
    }

    pub(super) fn drawing_style_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Source(preset) => {
                if let Some(url) = preset.source_url()
                    && let Err(error) = open::that(url)
                {
                    self.status = format!("Could not open publisher instructions: {error}");
                    self.error = true;
                }
            }
            Action::Open => {
                self.cancel_join();
                if self.tab.cleanup.is_some() || !self.finish_inline(true) {
                    return Task::none();
                }
                self.tab.styles.serial = self.tab.styles.serial.wrapping_add(1);
                self.tab.styles.editor = Some(Editor::new(
                    &self.tab.doc.drawing_style,
                    self.tab.file_epoch,
                    self.appearance.drawing_style_unit,
                ));
                self.inspector_tab = InspectorTab::DrawingStyle;
                self.inspector_open = true;
                self.palette = None;
            }
            Action::Cancel => {
                self.tab.styles.editor = None;
                self.inspector_tab = InspectorTab::Properties;
            }
            Action::Apply => {
                let Some(editor) = &self.tab.styles.editor else {
                    return Task::none();
                };
                let result = if editor.epoch != self.tab.file_epoch
                    || editor.original != self.tab.doc.drawing_style
                {
                    Err("The document style changed. Reopen Drawing style before applying.".into())
                } else {
                    editor.candidate().and_then(|style| {
                        if !editor.apply_semantics_requested && style == self.tab.doc.drawing_style
                        {
                            return Ok(None);
                        }
                        reshiki::document_styles::apply(
                            &self.tab.doc,
                            style,
                            editor.matching,
                            editor.scale,
                        )
                        .map(Some)
                    })
                };
                match result {
                    Ok(None) => {
                        self.tab.styles.editor = None;
                        self.inspector_tab = InspectorTab::Properties;
                        self.status = "Drawing style unchanged".into();
                        self.error = false;
                    }
                    Ok(Some(doc)) => {
                        let before = self.tab.doc.clone();
                        self.tab.doc = doc;
                        self.changed(before);
                        if !self.error {
                            self.tab.styles.editor = None;
                            self.inspector_tab = InspectorTab::Properties;
                            self.status = "Drawing style applied · Undo restores the previous style and layout".into();
                        }
                    }
                    Err(error) => {
                        self.status = error;
                        self.error = true;
                    }
                }
            }
            Action::DisplayUnit(unit) => {
                let Some(editor) = &mut self.tab.styles.editor else {
                    return Task::none();
                };
                if let Err(error) = editor.change_unit(unit) {
                    editor.unit_notice = Some(format!("Units unchanged. {error}"));
                    return Task::none();
                }
                self.appearance.drawing_style_unit = unit;
                if let Err(error) = self.appearance.save() {
                    self.status = format!("Could not save dimension unit preference: {error}");
                    self.error = true;
                }
            }
            Action::CommitField(field) => {
                let Some(editor) = &mut self.tab.styles.editor else {
                    return Task::none();
                };
                if let Err(error) = editor.commit_field(field) {
                    editor.unit_notice = Some(error);
                } else {
                    self.sync_drawing_style_unit();
                }
            }
            Action::Load => {
                let serial = self.tab.styles.serial;
                let epoch = self.tab.file_epoch;
                return Task::perform(
                    async {
                        let Some(file) = rfd::AsyncFileDialog::new()
                            .set_title("Load drawing style")
                            .add_filter(
                                "Drawing styles and ChemDraw stationery",
                                &[
                                    "reshiki-style",
                                    "moruno-style",
                                    "json",
                                    "cds",
                                    "cdx",
                                    "cdxml",
                                ],
                            )
                            .pick_file()
                            .await
                        else {
                            return Ok(None);
                        };
                        let path = file.path().to_path_buf();
                        tokio::task::spawn_blocking(move || reshiki::document_styles::load(&path))
                            .await
                            .map_err(|e| e.to_string())?
                            .map(Some)
                    },
                    move |result| Message::DrawingStyle(Action::Loaded(serial, epoch, result)),
                );
            }
            Action::Loaded(serial, epoch, result) => {
                if serial != self.tab.styles.serial
                    || epoch != self.tab.file_epoch
                    || self.tab.styles.editor.is_none()
                {
                    return Task::none();
                }
                match result {
                    Ok(Some(style)) => {
                        if let Some(editor) = &mut self.tab.styles.editor {
                            editor.apply_semantics_requested = true;
                            editor.set(&style);
                        }
                    }
                    Ok(None) => {}
                    Err(error) => {
                        self.status = error;
                        self.error = true;
                    }
                }
            }
            Action::Save(format) => {
                let Some(editor) = &mut self.tab.styles.editor else {
                    return Task::none();
                };
                editor.export_menu = false;
                let result = editor.candidate();
                match result {
                    Ok(style) => {
                        return Task::perform(
                            async move {
                                let Some(path) = super::files::save_path(
                                    "Save drawing style",
                                    &format!("Drawing.{}", format.extension()),
                                    format.extension(),
                                )
                                .await
                                else {
                                    return Ok(false);
                                };
                                tokio::task::spawn_blocking(move || {
                                    reshiki::document_styles::save(&path, &style)
                                })
                                .await
                                .map_err(|e| e.to_string())??;
                                Ok(true)
                            },
                            |result| Message::DrawingStyle(Action::Saved(result)),
                        );
                    }
                    Err(error) => {
                        self.status = error;
                        self.error = true;
                    }
                }
            }
            Action::Saved(result) => match result {
                Ok(true) => {
                    self.status = "Drawing style saved".into();
                    self.error = false;
                }
                Ok(false) => {}
                Err(error) => {
                    self.status = error;
                    self.error = true;
                }
            },
            action => {
                if let Some(editor) = &mut self.tab.styles.editor {
                    match action {
                        Action::Preset(preset) => {
                            editor.apply_semantics_requested = true;
                            editor.set(&preset.style());
                        }
                        Action::Custom => {
                            editor.apply_semantics_requested = true;
                            editor.name = "Custom".into();
                        }
                        Action::Name(name) => {
                            editor.apply_semantics_requested |= editor.name != name;
                            editor.name = name;
                        }
                        Action::Font(font) => {
                            if editor.font != font {
                                editor.font = font;
                                editor.mark_custom();
                            }
                        }
                        Action::Input(field, value) => {
                            if let Some((_, input)) =
                                editor.inputs.iter_mut().find(|(f, _)| *f == field)
                                && input.text != value
                            {
                                input.text = value;
                                editor.mark_custom();
                                if editor.display_unit == self.appearance.drawing_style_unit {
                                    editor.unit_notice = None;
                                }
                            }
                        }
                        Action::Advanced(value) => editor.advanced = value,
                        Action::ExportMenu(value) => editor.export_menu = value,
                        Action::Matching(value) => {
                            editor.apply_semantics_requested = true;
                            editor.matching = value;
                        }
                        Action::Scale(value) => {
                            editor.apply_semantics_requested = true;
                            editor.scale = value;
                        }
                        _ => {}
                    }
                }
            }
        }
        Task::none()
    }

    pub(super) fn drawing_style_panel(&self) -> Element<'_, Message> {
        let action = Message::DrawingStyle;
        let Some(editor) = &self.tab.styles.editor else {
            return command("Edit drawing style")
                .on_press(action(Action::Open))
                .into();
        };
        let candidate = editor.candidate();
        let preset = candidate
            .as_ref()
            .ok()
            .and_then(|style| Preset::ALL.into_iter().find(|p| p.style() == *style));
        let mut header = column![
            command("‹ Properties")
                .on_press(action(Action::Cancel))
                .style(button::text),
            text("Drawing style").size(19),
            text("Physical sizes for this document")
                .size(12)
                .style(super::workspace::muted_text),
            crate::appearance::pick_list(
                Preset::ALL
                    .into_iter()
                    .map(Choice::Journal)
                    .collect::<Vec<_>>(),
                Some(preset.map(Choice::Journal).unwrap_or(Choice::Custom)),
                move |choice| action(match choice {
                    Choice::Journal(p) => Action::Preset(p),
                    Choice::Custom => Action::Custom,
                    Choice::Details => Action::Open,
                }),
            )
            .width(Length::Fill)
            .text_size(13),
        ]
        .spacing(10);
        if let Some(preset) = preset {
            header = header
                .push(
                    text(preset.description())
                        .size(12)
                        .style(super::workspace::muted_text),
                )
                .push(
                    command("Publisher instructions ↗")
                        .on_press(action(Action::Source(preset)))
                        .style(button::text),
                );
        } else {
            header = header.push(
                text("Customize the current settings, then save a reusable style file.")
                    .size(12)
                    .style(super::workspace::muted_text),
            );
        }
        if let Ok(style) = &candidate {
            let mut preview =
                reshiki::rings::Preset::Regular.document(style.bond_length_world, false);
            if let Some(atom) = preview.atoms.first() {
                let (id, p) = (atom.id, atom.position);
                let oxygen = preview.add_atom("O", p.offset(0., -style.bond_length_world));
                preview.add_bond(id, oxygen, 2, "plain");
            }
            let ids = preview.all_ids();
            reshiki::editing::transform_about(
                &mut preview,
                &ids,
                reshiki::document::Point::default(),
                1.,
                90.,
            );
            preview.drawing_style = style.clone();
            preview.canvas_theme = self.tab.doc.canvas_theme;
            preview.color_theme = self.tab.doc.color_theme;
            preview.custom_theme = self.tab.doc.custom_theme.clone();
            header = header.push(
                container(
                    canvas(DrawingThumbnail(preview))
                        .height(85)
                        .width(Length::Fill),
                )
                .style(super::workspace::panel),
            );
        }
        // Keep conditional preset guidance and preview in one subtree. Iced
        // reconciles column children by position: removing either before an
        // input would otherwise move its state to a different widget and lose
        // focus in the middle of an incomplete number or unit suffix.
        let mut body = column![header].spacing(10);
        body = body.push(
            crate::appearance::text_input("Style name", &editor.name)
                .on_input(move |s| action(Action::Name(s)))
                .size(13)
                .padding(7),
        );
        body = body.push(text("Label font").size(12));
        body = body.push(
            combo_box(
                &editor.font_options,
                "Search fonts…",
                Some(&editor.font),
                move |s| action(Action::Font(s)),
            )
            .input_style(crate::appearance::input_style)
            .menu_style(crate::appearance::dropdown_menu)
            .size(13)
            .padding(7)
            .width(Length::Fill),
        );
        body = body.push(
            row![
                text("Dimension units").size(12).width(Length::Fill),
                super::workspace::hover_hint(
                    crate::appearance::pick_list(Unit::ALL, Some(editor.display_unit), move |unit| action(Action::DisplayUnit(unit)))
                        .text_size(12).padding(6).width(104),
                    "Bare dimensions use this unit. Add cm, mm or pt to override. Label size always uses pt. Other tools keep their labeled units.",
                    tooltip::Position::Top,
                ),
            ].align_y(Alignment::Center).spacing(8),
        );
        for (field, input) in &editor.inputs {
            if !editor.advanced && !matches!(field, Field::FontSize | Field::Bond | Field::Line) {
                continue;
            }
            let field = *field;
            body = body.push(
                row![
                    text(field.label(editor.display_unit))
                        .size(12)
                        .width(Length::Fill),
                    crate::appearance::text_input("", &input.text)
                        .on_input(move |s| action(Action::Input(field, s)))
                        .on_submit(action(Action::CommitField(field)))
                        .size(13)
                        .padding(6)
                        .width(104)
                ]
                .align_y(Alignment::Center)
                .spacing(8),
            );
        }
        body = body.push(
            command(if editor.advanced {
                "▾ Advanced stroke settings"
            } else {
                "▸ Advanced stroke settings"
            })
            .on_press(action(Action::Advanced(!editor.advanced)))
            .style(button::text),
        );
        body = body
            .push(super::workspace::horizontal_line())
            .push(
                checkbox(editor.matching)
                    .label("Update matching text and strokes")
                    .on_toggle(move |v| action(Action::Matching(v)))
                    .text_size(12),
            )
            .push(
                text("Preserve custom fonts, sizes and widths.")
                    .size(11)
                    .style(super::workspace::muted_text),
            )
            .push(
                checkbox(editor.scale)
                    .label("Scale layout with bond length")
                    .on_toggle(move |v| action(Action::Scale(v)))
                    .text_size(12),
            )
            .push(
                text("Off: keep positions. On: scale object geometry; keep paper size.")
                    .size(11)
                    .style(super::workspace::muted_text),
            );
        body = body.push(text("Load .cds, .cdx or .cdxml for label fonts and bond settings. Template artwork and page layout are not imported.")
            .size(11).style(super::workspace::muted_text));
        let mut footer = column![super::workspace::horizontal_line()].spacing(8);
        if let Some(notice) = &editor.unit_notice {
            footer = footer.push(text(notice).size(11).style(super::workspace::muted_text));
        }
        if let Err(error) = &candidate {
            footer = footer.push(text(error.clone()).size(12).style(
                crate::appearance::text_color(iced::Color::from_rgb8(164, 54, 47)),
            ));
        }
        if editor.export_menu {
            footer = footer.push(
                container(
                    column![
                        command("ReShiki style (.reshiki-style)")
                            .on_press(action(Action::Save(SaveFormat::Native)))
                            .style(crate::appearance::secondary)
                            .width(Length::Fill),
                        command("ChemDraw stationery (.cds)")
                            .on_press(action(Action::Save(SaveFormat::Cds)))
                            .style(crate::appearance::secondary)
                            .width(Length::Fill),
                    ]
                    .spacing(5),
                )
                .padding(5),
            );
        }
        let icon = |glyph, hint, message, enabled, active| {
            super::workspace::hover_hint(
                button(canvas(Glyph(glyph, enabled)).width(24).height(24))
                    .padding(7)
                    .width(40)
                    .height(38)
                    .style(super::workspace::control(active))
                    .on_press_maybe(enabled.then_some(action(message))),
                hint,
                tooltip::Position::Top,
            )
        };
        footer = footer.push(
            row![
                icon(
                    Icon::Import,
                    "Import drawing style…",
                    Action::Load,
                    true,
                    false
                ),
                icon(
                    Icon::Export,
                    "Export as ReShiki style or ChemDraw CDS",
                    Action::ExportMenu(!editor.export_menu),
                    candidate.is_ok(),
                    editor.export_menu
                ),
                Space::new().width(Length::Fill),
                icon(
                    Icon::Save,
                    "Save style to this drawing",
                    Action::Apply,
                    candidate.is_ok(),
                    true
                ),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        );
        container(
            column![
                scrollable(container(body).padding(iced::Padding {
                    right: 12.,
                    ..Default::default()
                }))
                .id("inspector-content")
                .height(Length::Fill),
                footer
            ]
            .spacing(12),
        )
        .padding(16)
        .height(Length::Fill)
        .into()
    }
}
