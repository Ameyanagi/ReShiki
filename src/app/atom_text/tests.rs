use super::*;
use crate::canvas::{Edit, Tool};
use iced::keyboard::{
    Key, Modifiers,
    key::{Named, Physical},
};
use iced::widget::text_editor::{Action as Input, Edit as TextEdit, Motion};
use reshiki::document::Point;

fn rich_label(text: &str) -> (App, u64) {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let id = app.tab.doc.add_atom("*", Point::default());
    app.tab.doc.atom_mut(id).unwrap().display.variable = Some(text.into());
    let _ = app.update(Message::AtomText(Action::Begin(Some(id))));
    (app, id)
}

fn editor_action(app: &mut App, action: Input) {
    let _ = app.update(Message::AtomText(Action::Editor(action)));
}

fn key_press(key: Key, modifiers: Modifiers) -> text_editor::KeyPress {
    text_editor::KeyPress {
        modified_key: key.clone(),
        key,
        physical_key: Physical::Unidentified(iced::keyboard::key::NativeCode::Unidentified),
        modifiers,
        text: None,
        status: text_editor::Status::Focused { is_hovered: false },
    }
}

#[test]
fn label_keys_share_caption_script_aliases_and_keep_plain_local() {
    for (key, script) in [
        ("-", Script::Subscript),
        ("+", Script::Superscript),
        ("=", Script::Superscript),
        ("t", Script::Normal),
        ("T", Script::Normal),
    ] {
        let key = Key::Character(key.into());
        for modifiers in [Modifiers::COMMAND, Modifiers::COMMAND | Modifiers::SHIFT] {
            assert!(matches!(
                key_binding(key_press(key.clone(), modifiers)),
                Some(text_editor::Binding::Custom(Message::AtomText(Action::Script(value)))) if value == script
            ));
            if script != Script::Normal {
                assert_eq!(script_key(&key, modifiers), Some(script));
            } else {
                assert_eq!(script_key(&key, modifiers), None);
            }
        }
        assert!(!matches!(
            key_binding(key_press(key, Modifiers::empty())),
            Some(text_editor::Binding::Custom(_))
        ));
    }
    for modifiers in [Modifiers::empty(), Modifiers::COMMAND, Modifiers::SHIFT] {
        assert!(matches!(
            key_binding(key_press(Key::Named(Named::Enter), modifiers)),
            Some(text_editor::Binding::Custom(Message::AtomText(
                Action::Apply
            )))
        ));
    }
    assert!(matches!(
        key_binding(key_press(Key::Named(Named::Escape), Modifiers::empty())),
        Some(text_editor::Binding::Custom(Message::AtomText(
            Action::Cancel
        )))
    ));
    let mut unfocused = key_press(Key::Character("t".into()), Modifiers::COMMAND);
    unfocused.status = text_editor::Status::Active;
    assert!(key_binding(unfocused).is_none());
}

#[test]
fn selection_script_buttons_toggle_only_the_selected_utf8_range() {
    let (mut app, _) = rich_label("触媒aa");
    let state = app.tab.atom_text.as_ref().unwrap();
    assert_eq!(state.editor.selection().as_deref(), Some("触媒aa"));
    editor_action(&mut app, Input::Move(Motion::DocumentEnd));
    editor_action(&mut app, Input::Select(Motion::Left));
    let _ = app.update(Message::AtomText(Action::Script(Script::Subscript)));
    let state = app.tab.atom_text.as_ref().unwrap();
    assert!(!state.label_edited);
    assert!(state.scripts_edited);
    assert_eq!(
        state.scripts(),
        vec![ScriptRun {
            start: "触媒a".len(),
            end: "触媒aa".len(),
            script: Script::Subscript
        }]
    );
    let _ = app.update(Message::AtomText(Action::Script(Script::Subscript)));
    let state = app.tab.atom_text.as_ref().unwrap();
    assert_eq!(
        state.scripts(),
        vec![ScriptRun {
            start: 0,
            end: "触媒aa".len(),
            script: Script::Normal
        }]
    );
    let _ = app.update(Message::AtomText(Action::Script(Script::Superscript)));
    let _ = app.update(Message::AtomText(Action::Script(Script::Normal)));
    assert!(
        app.tab
            .atom_text
            .as_ref()
            .unwrap()
            .selected_script(Script::Normal)
    );
}

#[test]
fn caret_only_script_control_formats_the_entire_label_and_empty_is_safe() {
    let (mut app, _) = rich_label("R1");
    editor_action(&mut app, Input::Move(Motion::DocumentEnd));
    let _ = app.update(Message::AtomText(Action::Script(Script::Superscript)));
    assert_eq!(
        app.tab.atom_text.as_ref().unwrap().scripts(),
        vec![ScriptRun {
            start: 0,
            end: 2,
            script: Script::Superscript
        }]
    );
    let _ = app.update(Message::AtomText(Action::Script(Script::Superscript)));
    assert_eq!(
        app.tab.atom_text.as_ref().unwrap().scripts(),
        vec![ScriptRun {
            start: 0,
            end: 2,
            script: Script::Normal
        }]
    );
    let _ = app.update(Message::AtomText(Action::Input(String::new())));
    let _ = app.update(Message::AtomText(Action::Script(Script::Subscript)));
    assert!(app.tab.atom_text.as_ref().unwrap().scripts().is_empty());
    let _ = app.update(Message::AtomText(Action::Apply));
    assert!(app.tab.atom_text.as_ref().unwrap().error.is_some());
}

#[test]
fn manual_script_only_apply_reopens_and_text_edits_preserve_runs() {
    let (mut app, id) = rich_label("Rab");
    let original = app.tab.doc.clone();
    editor_action(&mut app, Input::Move(Motion::DocumentEnd));
    editor_action(&mut app, Input::Select(Motion::Left));
    let _ = app.update(Message::AtomText(Action::Script(Script::Superscript)));
    let _ = app.update(Message::AtomText(Action::Apply));
    assert!(app.tab.atom_text.is_none());
    assert_eq!(
        app.tab.doc.atom(id).unwrap().display.label_scripts,
        vec![ScriptRun {
            start: 2,
            end: 3,
            script: Script::Superscript
        }]
    );
    let formatted = app.tab.doc.clone();
    let _ = app.update(Message::AtomText(Action::Begin(Some(id))));
    let state = app.tab.atom_text.as_ref().unwrap();
    assert!(state.manual_scripts);
    assert!(!state.scripts_edited);
    assert_eq!(
        state.scripts(),
        formatted.atom(id).unwrap().display.label_scripts
    );
    editor_action(&mut app, Input::Move(Motion::DocumentEnd));
    editor_action(
        &mut app,
        Input::Edit(TextEdit::Paste("c".to_owned().into())),
    );
    let _ = app.update(Message::AtomText(Action::Apply));
    let display = &app.tab.doc.atom(id).unwrap().display;
    assert_eq!(display.variable.as_deref(), Some("Rabc"));
    assert_eq!(
        display.label_scripts,
        vec![ScriptRun {
            start: 2,
            end: 4,
            script: Script::Superscript
        }]
    );
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, formatted);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
}

#[test]
fn first_script_key_sets_an_automatic_script_and_the_second_toggles_it_off() {
    let (mut app, _) = rich_label("R1");
    editor_action(&mut app, Input::Move(Motion::DocumentEnd));
    editor_action(&mut app, Input::Select(Motion::Left));
    assert!(
        app.tab
            .atom_text
            .as_ref()
            .unwrap()
            .selected_script(Script::Subscript)
    );
    let _ = app.update(Message::AtomText(Action::Script(Script::Subscript)));
    let one = ScriptRun {
        start: 1,
        end: 2,
        script: Script::Subscript,
    };
    let state = app.tab.atom_text.as_ref().unwrap();
    assert!(state.scripts().contains(&one));
    let _ = app.update(Message::AtomText(Action::Script(Script::Subscript)));
    let state = app.tab.atom_text.as_ref().unwrap();
    assert!(!state.scripts().contains(&one));
    assert!(state.selected_script(Script::Normal));
}

#[test]
fn automatic_runs_remain_automatic_until_plain_is_chosen() {
    let (mut app, id) = rich_label("R1");
    let original = app.tab.doc.clone();
    let _ = app.update(Message::AtomText(Action::Apply));
    assert_eq!(app.tab.doc, original);
    let _ = app.update(Message::AtomText(Action::Begin(Some(id))));
    let _ = app.update(Message::AtomText(Action::Input("R2".into())));
    let _ = app.update(Message::AtomText(Action::Apply));
    assert!(
        app.tab
            .doc
            .atom(id)
            .unwrap()
            .display
            .label_scripts
            .is_empty()
    );
    let _ = app.update(Message::AtomText(Action::Begin(Some(id))));
    let _ = app.update(Message::AtomText(Action::Script(Script::Normal)));
    let _ = app.update(Message::AtomText(Action::Apply));
    assert_eq!(
        app.tab.doc.atom(id).unwrap().display.label_scripts,
        vec![ScriptRun {
            start: 0,
            end: 2,
            script: Script::Normal
        }]
    );
    let plain = app.tab.doc.clone();
    let _ = app.update(Message::AtomText(Action::Begin(Some(id))));
    let _ = app.update(Message::AtomText(Action::Apply));
    assert_eq!(app.tab.doc, plain);
}

#[test]
fn full_value_updates_preserve_reopened_manual_formatting() {
    let (mut app, id) = rich_label("Rab");
    editor_action(&mut app, Input::Move(Motion::DocumentEnd));
    editor_action(&mut app, Input::Select(Motion::Left));
    let _ = app.update(Message::AtomText(Action::Script(Script::Superscript)));
    let _ = app.update(Message::AtomText(Action::Apply));
    let _ = app.update(Message::AtomText(Action::Begin(Some(id))));
    let _ = app.update(Message::AtomText(Action::Input("Rabc".into())));
    assert_eq!(
        app.tab.atom_text.as_ref().unwrap().scripts(),
        vec![ScriptRun {
            start: 2,
            end: 4,
            script: Script::Superscript
        }]
    );
    let _ = app.update(Message::AtomText(Action::Input("酸Rabc".into())));
    assert_eq!(
        app.tab.atom_text.as_ref().unwrap().scripts(),
        vec![ScriptRun {
            start: "酸Ra".len(),
            end: "酸Rabc".len(),
            script: Script::Superscript
        }]
    );
    let _ = app.update(Message::AtomText(Action::Apply));
    let display = &app.tab.doc.atom(id).unwrap().display;
    assert_eq!(display.variable.as_deref(), Some("酸Rabc"));
    assert_eq!(
        display.label_scripts,
        vec![ScriptRun {
            start: "酸Ra".len(),
            end: "酸Rabc".len(),
            script: Script::Superscript
        }]
    );
}

#[test]
fn label_paste_strips_newlines_and_preserves_manual_script_offsets() {
    let (mut app, _) = rich_label("触媒a");
    let _ = app.update(Message::AtomText(Action::Script(Script::Superscript)));
    editor_action(&mut app, Input::Move(Motion::DocumentEnd));
    editor_action(
        &mut app,
        Input::Edit(TextEdit::Paste("\r\nb\nc\rd".to_owned().into())),
    );
    let state = app.tab.atom_text.as_ref().unwrap();
    assert_eq!(state.input, "触媒abcd");
    assert_eq!(state.editor.text(), state.input);
    assert_eq!(
        state.scripts(),
        vec![ScriptRun {
            start: 0,
            end: state.input.len(),
            script: Script::Superscript
        }]
    );
    editor_action(&mut app, Input::Edit(TextEdit::Enter));
    assert_eq!(app.tab.atom_text.as_ref().unwrap().input, "触媒abcd");
    let _ = app.update(Message::AtomText(Action::Input("M\r\n2\n+".into())));
    assert_eq!(app.tab.atom_text.as_ref().unwrap().input, "M2+");
    assert_eq!(app.tab.atom_text.as_ref().unwrap().editor.text(), "M2+");
}

#[test]
fn group_drafts_ignore_scripts_without_changing_selection_or_format() {
    let (mut app, _) = rich_label("Rab");
    let _ = app.update(Message::AtomText(Action::Mode(Mode::Group)));
    let state = app.tab.atom_text.as_ref().unwrap();
    let cursor = state.editor.cursor();
    let format = state.format.clone();
    for script in [Script::Subscript, Script::Superscript, Script::Normal] {
        let _ = app.update(Message::AtomText(Action::Script(script)));
    }
    let state = app.tab.atom_text.as_ref().unwrap();
    assert_eq!(state.editor.cursor(), cursor);
    assert_eq!(state.format, format);
    assert!(!state.scripts_edited);
    assert!(state.scripts().is_empty());
    let _ = app.update(Message::AtomText(Action::Cancel));
    let a = app.tab.doc.add_atom("C", Point::new(42., 0.));
    let b = app.tab.doc.add_atom("O", Point::new(84., 0.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    app.tab.selected = vec![a, b];
    let _ = app.update(Message::AtomText(Action::ContractSelection));
    let _ = app.update(Message::AtomText(Action::Input("Group".into())));
    let _ = app.update(Message::AtomText(Action::Script(Script::Subscript)));
    let state = app.tab.atom_text.as_ref().unwrap();
    assert!(state.members.is_some());
    assert!(!state.scripts_enabled());
    assert!(!state.scripts_edited);
}

#[test]
fn label_modal_blocks_global_typography_and_tool_shortcuts() {
    let (mut app, _) = rich_label("R1");
    let document = app.tab.doc.clone();
    let format = app.tab.caption_format.clone();
    let tool = app.tool;
    for message in [
        Message::ContextKey("T".into()),
        Message::TextStyle(StyleChange::Script(Script::Superscript)),
    ] {
        assert!(!background(&message));
        let _ = app.update(message);
    }
    assert_eq!(app.tab.doc, document);
    assert_eq!(app.tab.caption_format, format);
    assert_eq!(app.tool, tool);
    assert!(!app.tab.atom_text.as_ref().unwrap().scripts_edited);
    let (mut empty, _) = App::new();
    empty.tab.busy = false;
    let _ = empty.update(Message::AtomText(Action::Begin(None)));
    assert!(empty.tab.atom_text.is_none());
}

#[tokio::test]
#[ignore = "Opt-in renderer check for label editor semantics and control actions"]
async fn rich_label_editor_and_script_controls_expose_accessible_actions() {
    use iced::advanced::{mouse, renderer::Headless, widget::operation};
    use iced_runtime::{UserInterface, user_interface::Cache};
    use reshiki::canvas_theme::CanvasTheme;
    let mut renderer =
        <iced::Renderer as Headless>::new(iced::Font::default(), iced::Pixels(16.), None)
            .await
            .unwrap();
    let (mut app, _) = rich_label("Rab");
    let _ = app.update(Message::AtomText(Action::Script(Script::Superscript)));
    assert!(app.tab.atom_text.as_ref().unwrap().manual_scripts);
    let size = iced::Size::new(1040., 680.);
    app.viewport = size;
    app.appearance.mode = crate::appearance::Mode::MatchCanvas;
    for theme in [CanvasTheme::Light, CanvasTheme::Dark] {
        app.tab.doc.canvas_theme = theme;
        for mode in [Mode::Text, Mode::Group] {
            let _ = app.update(Message::AtomText(Action::Mode(mode)));
            let mut ui = UserInterface::build(
                app.with_atom_text(Space::new().width(Length::Fill).height(Length::Fill).into()),
                size,
                Cache::new(),
                &mut renderer,
            );
            // Draw as well as lay out, so the manual Appearance preview and
            // themed editor/control styles take their real renderer path.
            ui.draw(
                &mut renderer,
                &app.theme(),
                &iced::advanced::renderer::Style::default(),
                mouse::Cursor::Unavailable,
            );
            let mut collect =
                reshiki::accessibility::Collect::new(iced::Rectangle::with_size(size));
            ui.operate(&renderer, &mut operation::black_box(&mut collect));
            let snapshot = collect.snapshot();
            assert!(snapshot.duplicate_ids.is_empty());
            let editor = snapshot
                .nodes
                .iter()
                .find(|node| node.id == "atom-text")
                .unwrap();
            assert_eq!(editor.role, reshiki::accessibility::Role::TextArea);
            assert_eq!(editor.name, "Atom or group label");
            assert_eq!(editor.value.as_deref(), Some("Rab"));
            for id in [
                "atom-text",
                "atom-text-subscript",
                "atom-text-superscript",
                "atom-text-plain",
                "atom-text-apply",
                "atom-text-cancel",
            ] {
                let node = snapshot.nodes.iter().find(|node| node.id == id).unwrap();
                let bounds = node.bounds;
                assert!(
                    bounds.x >= 0.
                        && bounds.y >= 0.
                        && bounds.width > 0.
                        && bounds.height > 0.
                        && bounds.x + bounds.width <= size.width
                        && bounds.y + bounds.height <= size.height,
                    "{theme:?} {mode:?} {id}: {bounds:?} must fit inside {size:?}"
                );
                let visible = node.visible_bounds.unwrap_or_else(|| {
                    panic!(
                        "{theme:?} {mode:?} {id}: {bounds:?} has no visible bounds inside {size:?}"
                    )
                });
                assert!(
                    (visible.width - bounds.width).abs() < 0.01
                        && (visible.height - bounds.height).abs() < 0.01,
                    "{id} must remain fully visible"
                );
            }
            for (id, name, script) in [
                ("atom-text-subscript", "Subscript", Script::Subscript),
                ("atom-text-superscript", "Superscript", Script::Superscript),
                ("atom-text-plain", "Plain text", Script::Normal),
            ] {
                let button = snapshot.nodes.iter().find(|node| node.id == id).unwrap();
                assert_eq!(button.name, name);
                assert_eq!(button.enabled, mode != Mode::Group);
                assert_eq!(
                    button.checked,
                    Some(mode != Mode::Group && script == Script::Superscript)
                );
                let mut activate = reshiki::accessibility::Activate::<Message>::new(id);
                ui.operate(&renderer, &mut operation::black_box(&mut activate));
                if mode == Mode::Group {
                    assert!(activate.message().is_none());
                } else {
                    assert!(
                        matches!(activate.message(), Some(Message::AtomText(Action::Script(value))) if *value == script)
                    );
                }
            }
        }
    }
}

#[test]
fn accepting_an_unchanged_label_preserves_imported_atom_properties() -> Result<(), String> {
    for element in ["H", "N", "Pt"] {
        let (mut app, _) = App::new();
        let id = app.tab.doc.add_atom(element, Point::default());
        let atom = app.tab.doc.atom_mut(id).ok_or("Atom")?;
        atom.no_implicit = true;
        atom.charge = 1;
        atom.isotope = 2;
        let before = app.tab.doc.clone();
        let _ = app.update(Message::AtomText(Action::Begin(Some(id))));
        assert_eq!(
            app.tab.atom_text.as_ref().ok_or("Draft")?.input,
            format!("{element}+")
        );
        let _ = app.update(Message::AtomText(Action::Apply));
        assert_eq!(app.tab.doc, before);
    }
    Ok(())
}

#[test]
fn explicit_label_draft_and_history_retain_the_entered_count() -> Result<(), String> {
    let (mut app, _) = App::new();
    let pt = app.tab.doc.add_atom("Pt", Point::default());
    let n = app.tab.doc.add_atom("N", Point::new(42., 0.));
    app.tab.doc.add_bond(n, pt, 1, "plain");
    let _ = app.update(Message::AtomText(Action::Begin(Some(n))));
    let _ = app.update(Message::AtomText(Action::Input("NH3".into())));
    let _ = app.update(Message::AtomText(Action::Apply));
    assert_eq!(app.tab.doc.atom(n).ok_or("N")?.explicit_h, 3);
    let _ = app.update(Message::AtomText(Action::Begin(Some(n))));
    assert_eq!(app.tab.atom_text.as_ref().ok_or("Draft")?.input, "NH3");
    let _ = app.update(Message::AtomText(Action::Cancel));
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc.atom(n).ok_or("N")?.explicit_h, 0);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc.atom(n).ok_or("N")?.explicit_h, 3);
    Ok(())
}

#[test]
fn charged_atom_label_draft_and_undo_retain_real_atom_properties() -> Result<(), String> {
    let (mut app, _) = App::new();
    let metal = app.tab.doc.add_atom("Pt", Point::default());
    let ligand = app.tab.doc.add_atom("N", Point::new(42., 0.));
    app.tab.doc.add_bond(ligand, metal, 1, "plain");
    let original = app.tab.doc.clone();
    let _ = app.update(Message::AtomText(Action::Begin(Some(metal))));
    let _ = app.update(Message::AtomText(Action::Input("Pd2+".into())));
    let _ = app.update(Message::AtomText(Action::Apply));
    assert!(app.tab.atom_text.is_none());
    let atom = app.tab.doc.atom(metal).ok_or("Missing palladium")?;
    assert_eq!(atom.element, "Pd");
    assert_eq!(atom.charge, 2);
    assert_eq!(app.tab.doc.bonds, original.bonds);
    let charged = app.tab.doc.clone();
    let _ = app.update(Message::AtomText(Action::Begin(Some(metal))));
    assert_eq!(
        app.tab.atom_text.as_ref().ok_or("Missing draft")?.input,
        "Pd2+"
    );
    let _ = app.update(Message::AtomText(Action::Cancel));
    assert!(app.tab.atom_text.is_none());
    assert_eq!(app.tab.doc, charged);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc.atom(metal), original.atom(metal));
    assert_eq!(app.tab.doc, original);
    Ok(())
}

#[test]
fn defining_and_renaming_a_group_keeps_chemistry_and_invalid_drafts() -> Result<(), String> {
    let (mut app, _) = App::new();
    // This exercises classic selection-based Enter; hybrid Enter edits
    // its explicit atom/bond hotspot instead of contracting a selection.
    let _ = app.update(Message::KeyboardDrawing(
        crate::app::keyboard_drawing::Action::Leave,
    ));
    assert!(!app.tab.keyboard_drawing.enabled());
    let n = app.tab.doc.add_atom("N", Point::default());
    let a = app.tab.doc.add_atom("C", Point::new(42., 0.));
    let b = app.tab.doc.add_atom("O", Point::new(84., 0.));
    app.tab.doc.add_bond(n, a, 1, "plain");
    app.tab.doc.add_bond(a, b, 1, "plain");
    let original = app.tab.doc.clone();
    app.tab.selected = vec![a, b];
    let _ = app.update(Message::ContextKey("Enter".into()));
    assert!(
        app.tab
            .atom_text
            .as_ref()
            .ok_or("Group draft")?
            .members
            .is_some()
    );
    let _ = app.update(Message::AtomText(Action::Input("Custom".into())));
    let _ = app.update(Message::AtomText(Action::ReverseInput("LeftCustom".into())));
    let _ = app.update(Message::AtomText(Action::Apply));
    assert!(app.tab.atom_text.is_none());
    assert_eq!(app.tab.doc.atoms, original.atoms);
    assert_eq!(app.tab.doc.bonds, original.bonds);
    assert_eq!(
        app.tab.doc.abbreviation(a).ok_or("Group")?.reverse_label,
        "LeftCustom"
    );
    let _ = app.update(Message::AtomText(Action::Begin(None)));
    let _ = app.update(Message::AtomText(Action::ReverseInput("a".repeat(33))));
    let _ = app.update(Message::AtomText(Action::Apply));
    assert!(
        app.tab
            .atom_text
            .as_ref()
            .ok_or("Invalid draft must remain")?
            .error
            .is_some()
    );
    assert_eq!(
        app.tab.doc.abbreviation(a).ok_or("Group")?.reverse_label,
        "LeftCustom"
    );
    let _ = app.update(Message::AtomText(Action::Cancel));
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc.abbreviations.len(), 1);
    Ok(())
}

#[test]
fn label_dialog_preserves_toolbar_alignment_and_chemical_graph() -> Result<(), String> {
    for label in ["Boc", "Cp*"] {
        let (mut app, _) = App::new();
        let id = app.tab.doc.add_atom("C", Point::default());
        app.tab.doc = reshiki::atom_text::apply(&app.tab.doc, id, label, Mode::Group)?;
        app.tab.selected = vec![id];
        let before = app.tab.doc.clone();
        for alignment in LabelAlignment::ALL {
            let _ = app.update(Message::GroupLabelAlign(alignment));
            let _ = app.update(Message::AtomText(Action::Begin(None)));
            let _ = app.update(Message::AtomText(Action::Apply));
            assert_eq!(
                app.tab.doc.abbreviation(id).ok_or("group")?.alignment,
                alignment
            );
            assert_eq!(app.tab.doc.atoms, before.atoms);
            assert_eq!(app.tab.doc.bonds, before.bonds);
        }
        let _ = app.update(Message::Undo);
        assert_eq!(
            app.tab.doc.abbreviation(id).ok_or("group")?.alignment,
            LabelAlignment::Right
        );
        let _ = app.update(Message::Redo);
        assert_eq!(
            app.tab.doc.abbreviation(id).ok_or("group")?.alignment,
            LabelAlignment::Above
        );
        assert_eq!(app.tab.doc.atoms, before.atoms);
    }
    Ok(())
}

#[test]
fn collapsed_haptic_groups_can_be_edited_and_undone() -> Result<(), String> {
    let (mut app, _) = App::new();
    let end = app.tab.doc.add_atom("C", Point::default());
    app.tab.doc = reshiki::atom_text::apply(&app.tab.doc, end, "Cp*", Mode::Auto)?;
    app.tab.selected = app
        .tab
        .doc
        .abbreviation(end)
        .ok_or("Missing group")?
        .members
        .clone();
    let before = app.tab.doc.clone();
    let _ = app.update(Message::AtomText(Action::Begin(None)));
    assert_eq!(
        app.tab.atom_text.as_ref().ok_or("Missing editor")?.input,
        "Cp*"
    );
    let _ = app.update(Message::AtomText(Action::Input("Cp".into())));
    let _ = app.update(Message::AtomText(Action::Apply));
    assert!(app.tab.atom_text.is_none());
    assert_eq!(
        app.tab
            .doc
            .atoms
            .iter()
            .filter(|a| a.element == "C")
            .count(),
        5
    );
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(
        app.tab
            .doc
            .atoms
            .iter()
            .filter(|a| a.element == "C")
            .count(),
        5
    );
    Ok(())
}

#[test]
fn atom_text_entry_preserves_bonds_is_undoable_and_cancel_is_nonmutating() -> Result<(), String> {
    let (mut app, _) = App::new();
    let a = app.tab.doc.add_atom("C", Point::new(0., 0.));
    let b = app.tab.doc.add_atom("N", Point::new(80., 0.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    app.tab.selected = vec![a];
    let before = app.tab.doc.clone();
    let _ = app.update(Message::AtomText(Action::Begin(None)));
    let _ = app.update(Message::AtomText(Action::Input("M".into())));
    let _ = app.update(Message::Delete);
    let _ = app.update(Message::New);
    assert_eq!(
        app.tab.doc, before,
        "Shortcuts cannot mutate the drawing behind the editor"
    );
    let _ = app.update(Message::AtomText(Action::Apply));
    assert_eq!(
        app.tab
            .doc
            .atom(a)
            .ok_or("Missing atom")?
            .display
            .variable
            .as_deref(),
        Some("M")
    );
    assert_eq!(app.tab.doc.bonds, before.bonds);
    let named = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, named);
    app.tool = Tool::Text;
    let _ = app.update(Message::Canvas(Edit::Click(Point::new(0., 0.))));
    assert!(app.tab.atom_text.is_some());
    assert!(app.tab.inline_text.is_none());
    let _ = app.update(Message::AtomText(Action::Input("L".into())));
    let _ = app.update(Message::Escape);
    assert_eq!(app.tab.doc, named);
    assert!(app.tab.atom_text.is_none());
    let _ = app.update(Message::Canvas(Edit::Click(Point::new(300., 300.))));
    assert!(
        app.tab.inline_text.is_some(),
        "Empty space still creates a caption"
    );
    Ok(())
}

#[test]
fn label_errors_and_stale_documents_retain_the_draft() -> Result<(), String> {
    let (mut app, _) = App::new();
    let a = app.tab.doc.add_atom("C", Point::default());
    app.tab.selected = vec![a];
    let _ = app.update(Message::AtomText(Action::Begin(None)));
    let before = app.tab.doc.clone();
    let _ = app.update(Message::AtomText(Action::Input("".into())));
    let _ = app.update(Message::AtomText(Action::Apply));
    assert!(
        app.tab
            .atom_text
            .as_ref()
            .ok_or("Missing draft")?
            .error
            .is_some()
    );
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::AtomText(Action::Input("X".into())));
    app.tab.file_epoch += 1;
    let _ = app.update(Message::AtomText(Action::Apply));
    assert!(
        app.tab
            .atom_text
            .as_ref()
            .ok_or("Missing draft")?
            .error
            .is_some()
    );
    assert_eq!(app.tab.doc, before);
    Ok(())
}

#[test]
fn apply_keeps_the_side_the_model_chose_for_a_new_group() -> Result<(), String> {
    use reshiki::{
        abbreviations::LabelAlignment,
        haworth::{Anomer, Sugar, sugar_document},
    };
    // The glucose template's C4 hydroxyl, drawn HO on its vertical bond.
    const C4_OXYGEN: u64 = 10;
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = sugar_document(Sugar::Glucose, Anomer::Alpha, 42.)?;
    let mut retype = |text: &str| {
        let _ = app.update(Message::AtomText(Action::Begin(Some(C4_OXYGEN))));
        let _ = app.update(Message::AtomText(Action::Input(text.into())));
        let _ = app.update(Message::AtomText(Action::Apply));
        assert!(app.tab.atom_text.is_none(), "{text}");
        let group = app.tab.doc.abbreviation(C4_OXYGEN).ok_or("Group")?;
        Ok::<_, String>((group.text(&app.tab.doc).to_owned(), group.alignment))
    };
    assert_eq!(retype("OBn")?, ("BnO".into(), LabelAlignment::Right));
    // Retyping on the group keeps its alignment.
    assert_eq!(retype("OMe")?, ("MeO".into(), LabelAlignment::Right));
    Ok(())
}

#[test]
fn apply_keeps_placement_a_group_without_a_reverse_spelling_can_take() -> Result<(), String> {
    use reshiki::{
        abbreviations::LabelAlignment,
        haworth::{Anomer, Sugar, sugar_document},
    };
    const C4_OXYGEN: u64 = 10;
    let (mut app, _) = App::new();
    app.tab.busy = false;
    for (placed, kept) in [
        (LabelAlignment::Above, LabelAlignment::Above),
        (LabelAlignment::Center, LabelAlignment::Center),
        (LabelAlignment::Left, LabelAlignment::Left),
        // Right would turn ONp, which has no reverse spelling.
        (LabelAlignment::Right, LabelAlignment::Auto),
    ] {
        let mut doc = sugar_document(Sugar::Glucose, Anomer::Alpha, 42.)?;
        doc = reshiki::atom_text::apply(&doc, C4_OXYGEN, "OBn", Mode::Auto)?;
        if let Some(group) = doc.abbreviations.iter_mut().find(|g| g.anchor == C4_OXYGEN) {
            group.alignment = placed;
        }
        app.tab.doc = doc;
        let _ = app.update(Message::AtomText(Action::Begin(Some(C4_OXYGEN))));
        let _ = app.update(Message::AtomText(Action::Input("ONp".into())));
        let _ = app.update(Message::AtomText(Action::Apply));
        assert!(app.tab.atom_text.is_none(), "{placed:?}");
        let group = app.tab.doc.abbreviation(C4_OXYGEN).ok_or("Group")?;
        assert_eq!(
            (group.text(&app.tab.doc), group.alignment),
            ("ONp", kept),
            "{placed:?}"
        );
    }
    Ok(())
}

#[test]
fn clicking_another_atom_commits_the_open_label_as_one_undo_step() -> Result<(), String> {
    for clicks in [1, 2] {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        let a = app.tab.doc.add_atom("C", Point::new(0., 0.));
        let b = app.tab.doc.add_atom("C", Point::new(80., 0.));
        app.tab.doc.add_bond(a, b, 1, "plain");
        let before = app.tab.doc.clone();
        app.tool = Tool::Text;
        let _ = app.update(Message::Canvas(Edit::Click(Point::new(0., 0.))));
        let _ = app.update(Message::AtomText(Action::Input("OMe".into())));
        for _ in 0..clicks {
            let _ = app.update(Message::Canvas(Edit::Click(Point::new(80., 0.))));
        }
        let draft = app.tab.atom_text.as_ref().ok_or("Draft")?;
        assert_eq!((draft.id, draft.input.as_str()), (b, "C"));
        assert_eq!(app.tab.doc.atom(a).ok_or("A")?.element, "O");
        assert_eq!(app.tab.history.undo_frames().len(), 1, "{clicks} clicks");
        let _ = app.update(Message::AtomText(Action::Cancel));
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
    }
    Ok(())
}

#[test]
fn switching_atoms_from_an_unchanged_or_empty_label_adds_no_undo_step() -> Result<(), String> {
    for input in [None, Some(""), Some("  ")] {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        app.tab.doc.add_atom("N", Point::new(0., 0.));
        let b = app.tab.doc.add_atom("C", Point::new(80., 0.));
        let before = app.tab.doc.clone();
        app.tool = Tool::Text;
        let _ = app.update(Message::Canvas(Edit::Click(Point::new(0., 0.))));
        if let Some(input) = input {
            let _ = app.update(Message::AtomText(Action::Input(input.into())));
        }
        let _ = app.update(Message::Canvas(Edit::Click(Point::new(80., 0.))));
        assert_eq!(app.tab.atom_text.as_ref().ok_or("Draft")?.id, b);
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tab.history.undo_frames().len(), 0, "{input:?}");
        assert_eq!(app.last_label, None);
    }
    Ok(())
}

/// Unbonded atoms, so that every one of them can take a substituent label.
fn text_atoms(len: usize) -> (App, Vec<u64>) {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let atoms: Vec<u64> = (0..len)
        .map(|i| app.tab.doc.add_atom("C", Point::new(80. * i as f32, 0.)))
        .collect();
    app.tool = Tool::Text;
    (app, atoms)
}

fn element(app: &App, id: u64) -> Option<&str> {
    Some(app.tab.doc.atom(id)?.element.as_str())
}

/// A double-click is a Click then a RepeatLabel on the same atom.
fn double_click(app: &mut App, x: f32) {
    let p = Point::new(x, 0.);
    let _ = app.update(Message::Canvas(Edit::Click(p)));
    let _ = app.update(Message::Canvas(Edit::RepeatLabel(p)));
}

#[test]
fn double_clicking_on_carries_the_label_atom_by_atom() -> Result<(), String> {
    let (mut app, atoms) = text_atoms(3);
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Canvas(Edit::Click(Point::new(0., 0.))));
    let _ = app.update(Message::AtomText(Action::Input("OMe".into())));
    double_click(&mut app, 80.);
    let draft = app.tab.atom_text.as_ref().ok_or("Draft")?;
    assert_eq!((draft.id, draft.input.as_str()), (atoms[1], "OMe"));
    assert_eq!(
        editor_range(&draft.editor, &draft.input),
        Some(0..3),
        "The carried label is selected to type over"
    );
    double_click(&mut app, 160.);
    let _ = app.update(Message::AtomText(Action::Apply));
    assert_eq!(app.tool, Tool::Text, "Enter keeps the Text tool");
    for id in &atoms {
        assert_eq!(element(&app, *id), Some("O"));
    }
    assert_eq!(app.tab.doc.atoms.len(), 6, "Each OMe adds its methyl");
    assert_eq!(app.tab.history.undo_frames().len(), 3);
    let _ = app.update(Message::Undo);
    assert_eq!(element(&app, atoms[2]), Some("C"));
    assert_eq!(element(&app, atoms[1]), Some("O"));
    let _ = app.update(Message::Undo);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    Ok(())
}

#[test]
fn a_repeat_after_enter_carries_but_typing_over_replaces_it() -> Result<(), String> {
    let (mut app, atoms) = text_atoms(3);
    let _ = app.update(Message::Canvas(Edit::Click(Point::new(0., 0.))));
    let _ = app.update(Message::AtomText(Action::Input("OMe".into())));
    let _ = app.update(Message::AtomText(Action::Apply));
    double_click(&mut app, 80.);
    let _ = app.update(Message::AtomText(Action::Input("NH2".into())));
    // Option/Alt-click is a RepeatLabel without the first click.
    let _ = app.update(Message::Canvas(Edit::RepeatLabel(Point::new(160., 0.))));
    let _ = app.update(Message::AtomText(Action::Apply));
    assert_eq!(
        atoms
            .iter()
            .map(|id| element(&app, *id))
            .collect::<Vec<_>>(),
        [Some("O"), Some("N"), Some("N")]
    );
    assert_eq!(app.tab.history.undo_frames().len(), 3);
    Ok(())
}

#[test]
fn escape_and_tool_changes_forget_the_carried_label() -> Result<(), String> {
    for forget in [
        Message::AtomText(Action::Cancel),
        Message::Tool(Tool::Select),
    ] {
        let (mut app, atoms) = text_atoms(2);
        let _ = app.update(Message::Canvas(Edit::Click(Point::new(0., 0.))));
        let _ = app.update(Message::AtomText(Action::Input("OMe".into())));
        let _ = app.update(Message::AtomText(Action::Apply));
        assert_eq!(app.last_label.as_deref(), Some("OMe"));
        if matches!(forget, Message::AtomText(_)) {
            let _ = app.update(Message::Canvas(Edit::Click(Point::new(80., 0.))));
        }
        let _ = app.update(forget.clone());
        app.tool = Tool::Text;
        double_click(&mut app, 80.);
        let draft = app.tab.atom_text.as_ref().ok_or("Draft")?;
        assert_eq!((draft.id, draft.input.as_str()), (atoms[1], "C"));
        assert_eq!(app.last_label, None, "{forget:?}");
    }
    Ok(())
}

#[test]
fn an_invalid_label_stays_open_when_another_atom_is_clicked() -> Result<(), String> {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let a = app.tab.doc.add_atom("C", Point::new(0., 0.));
    app.tab.doc.add_atom("C", Point::new(80., 0.));
    let before = app.tab.doc.clone();
    app.tool = Tool::Text;
    let _ = app.update(Message::Canvas(Edit::Click(Point::new(0., 0.))));
    let _ = app.update(Message::AtomText(Action::Input("O\u{7}".into())));
    let _ = app.update(Message::Canvas(Edit::Click(Point::new(80., 0.))));
    let draft = app.tab.atom_text.as_ref().ok_or("Draft")?;
    assert_eq!(draft.id, a);
    assert!(draft.error.is_some());
    assert_eq!(app.tab.doc, before);
    Ok(())
}
