//! Search opens the existing palette without moving the drawing or taking modal input.
use super::*;

fn aliases() -> [(Key, Code, Modifiers, &'static str); 2] {
    [
        (
            Key::Character("k".into()),
            Code::KeyK,
            Modifiers::COMMAND,
            "k",
        ),
        (
            Key::Character("P".into()),
            Code::KeyP,
            Modifiers::COMMAND | Modifiers::SHIFT,
            "P",
        ),
    ]
}

fn assert_fully_visible(node: &reshiki::accessibility::Node) {
    let visible = node.visible_bounds.expect("The control is visible");
    for (actual, expected) in [
        (visible.x, node.bounds.x),
        (visible.y, node.bounds.y),
        (visible.width, node.bounds.width),
        (visible.height, node.bounds.height),
    ] {
        assert!((actual - expected).abs() < 0.01, "{node:?}");
    }
}

#[tokio::test]
#[ignore = "Opt-in real header layout and pointer dispatch check"]
async fn search_header_fits_and_opens_a_focused_palette() {
    use crate::appearance::Mode;
    for size in [Size::new(1040., 680.), Size::new(1280., 820.)] {
        for mode in [Mode::Light, Mode::Dark] {
            let mut ui = Ui::new().await;
            ui.viewport = Rectangle::with_size(size);
            let mut app = drawing_app();
            app.appearance.mode = mode;
            for name in [
                "Long reaction scheme.rsk",
                "Detailed supporting information.rsk",
            ] {
                app.add_tab();
                app.tab.path = Some(name.into());
            }
            app.inspector_open = true;
            let (_, messages) = ui.event(
                &app,
                Event::Window(iced::window::Event::RedrawRequested(
                    std::time::Instant::now(),
                )),
                mouse::Cursor::Unavailable,
            );
            apply(&mut app, messages);
            let before = app.tab.doc.clone();
            let camera = app.tab.camera;
            let snapshot = ui.semantics(&app);
            assert!(snapshot.duplicate_ids.is_empty());
            let tabs = snapshot
                .nodes
                .iter()
                .filter(|node| {
                    node.id
                        .strip_prefix("document-tab-")
                        .is_some_and(|suffix| suffix.parse::<u64>().is_ok())
                })
                .count();
            assert_eq!(
                tabs,
                3,
                "{size:?}: {:?}",
                snapshot
                    .nodes
                    .iter()
                    .filter(|node| node.id.starts_with("header-")
                        || node.id.starts_with("document-tab-"))
                    .map(|node| (&node.id, node.bounds))
                    .collect::<Vec<_>>()
            );
            let search_left = snapshot
                .nodes
                .iter()
                .find(|node| node.id == "header-search")
                .unwrap()
                .bounds
                .x;
            for tab in snapshot
                .nodes
                .iter()
                .filter(|node| node.id.starts_with("document-tab"))
            {
                assert_fully_visible(tab);
                assert!(
                    tab.bounds.x + tab.bounds.width <= search_left + 0.01,
                    "{tab:?}"
                );
            }
            assert!(
                snapshot
                    .nodes
                    .iter()
                    .find(|node| node.id == "header-search")
                    .unwrap()
                    .name
                    .contains(&shortcuts::keys(Modifiers::COMMAND, "K"))
            );
            assert!(
                !snapshot
                    .nodes
                    .iter()
                    .any(|node| { matches!(node.id.as_str(), "header-import" | "header-export") })
            );
            let mut previous_right = 0.;
            for id in [
                "header-search",
                "header-check",
                "header-cleanup",
                "header-cleanup-options",
                "header-assistant",
                "header-settings",
                "header-inspector",
            ] {
                let node = snapshot.nodes.iter().find(|node| node.id == id).unwrap();
                assert_fully_visible(node);
                assert!(node.bounds.x + 0.01 >= previous_right, "{id}: {node:?}");
                assert!(
                    node.bounds.x + node.bounds.width <= size.width + 0.01,
                    "{id}"
                );
                previous_right = node.bounds.x + node.bounds.width;
            }
            for id in ["inspector-tab-Templates", "inspector-tab-Export"] {
                let node = snapshot.nodes.iter().find(|node| node.id == id).unwrap();
                assert_fully_visible(node);
            }
            assert!(
                snapshot
                    .nodes
                    .iter()
                    .any(|node| node.id == "inspector-tab-Templates" && node.name == "Add tab")
            );
            assert!(
                !snapshot
                    .nodes
                    .iter()
                    .any(|node| node.id == "inspector-tab-Import")
            );
            click_running_tasks(&mut ui, &mut app, "inspector-tab-Templates").await;
            assert!(
                ui.semantics(&app)
                    .nodes
                    .iter()
                    .any(|node| node.id == "add-search"
                        && node.role == reshiki::accessibility::Role::TextInput)
            );
            click_running_tasks(&mut ui, &mut app, "add-more").await;
            click_running_tasks(&mut ui, &mut app, "add-advanced-import").await;
            assert_eq!(app.inspector_tab, InspectorTab::Import);
            assert!(
                ui.semantics(&app)
                    .nodes
                    .iter()
                    .any(|node| node.id == "import-input"
                        && node.role == reshiki::accessibility::Role::TextArea)
            );
            click_running_tasks(&mut ui, &mut app, "header-search").await;
            assert!(app.commands.open);
            assert!(ui.input(&app, "command-palette-search").1);
            ui.key(
                &mut app,
                Key::Character("b".into()),
                Code::KeyB,
                Modifiers::empty(),
                Some("b"),
            )
            .await;
            assert_eq!(app.commands.query, "b");
            assert_eq!(app.tab.doc, before);
            assert_eq!(app.tab.camera.center, camera.center);
            assert_eq!(app.tab.camera.zoom, camera.zoom);
            assert!(!app.tab.history.can_undo());
        }
    }
}

#[tokio::test]
#[ignore = "Opt-in real text-field shortcut routing check"]
async fn search_aliases_leave_text_intact_and_focus_the_palette() {
    let mut ui = Ui::new().await;
    for (key, code, modifiers, text) in aliases() {
        let mut app = drawing_app();
        let drawing = app.tab.doc.clone();
        let draft = app.tab.font_size_input.clone();
        ui.focus_control(&mut app, "text-style-size").await;
        assert!(ui.input(&app, "text-style-size").1);
        ui.key(&mut app, key, code, modifiers, Some(text)).await;
        assert!(app.commands.open);
        assert!(ui.input(&app, "command-palette-search").1);
        assert_eq!(app.tab.font_size_input, draft);
        assert_eq!(app.tab.doc, drawing);
        assert!(!app.tab.history.can_undo());
        let _ = app.update(Message::Commands(
            crate::app::command_palette::Action::Query("benzene".into()),
        ));
        for (key, code, modifiers, text) in aliases() {
            ui.key(&mut app, key, code, modifiers, Some(text)).await;
            assert!(app.commands.open);
            assert_eq!(app.commands.query, "benzene");
            assert!(ui.input(&app, "command-palette-search").1);
        }
    }
    let mut app = drawing_app();
    ui.focus_control(&mut app, "text-style-size").await;
    ui.key(
        &mut app,
        Key::Character(":".into()),
        Code::Semicolon,
        Modifiers::SHIFT,
        Some(":"),
    )
    .await;
    assert!(
        !app.commands.open,
        "A colon still belongs to the text field"
    );
    assert!(app.tab.font_size_input.contains(':'));
}

#[tokio::test]
#[ignore = "Opt-in real modal keyboard ownership check"]
async fn search_aliases_respect_foreground_dialogs() {
    for dialog in ["Help", "Settings", "About", "Save", "Atom label"] {
        let mut ui = Ui::new().await;
        let mut app = drawing_app();
        let drawing = app.tab.doc.clone();
        let task = match dialog {
            "Help" => app.update(Message::ToggleHelp),
            "Settings" => app.update(Message::Settings(crate::app::settings_page::Action::Open(
                None,
            ))),
            "About" => app.update(Message::Updates(updates::Action::Show(true))),
            "Save" => {
                app.prompt = Some(crate::app::prompt::Prompt::Save("Reaction".into()));
                iced::Task::none()
            }
            _ => app.update(Message::AtomText(crate::app::atom_text::Action::Begin(
                Some(app.tab.doc.atoms[0].id),
            ))),
        };
        ui.run_task(&mut app, task).await;
        for (key, code, modifiers, text) in aliases() {
            ui.key(&mut app, key, code, modifiers, Some(text)).await;
            assert!(!app.commands.open, "{dialog} keeps the keyboard");
            assert_eq!(app.tab.doc, drawing, "{dialog}");
            assert!(match dialog {
                "Help" => app.help_open,
                "Settings" => app.settings.open,
                "About" => app.updates.open,
                "Save" => app.prompt.is_some(),
                _ => app.tab.atom_text.is_some(),
            });
        }
    }
}
