use super::*;
use crate::app::{command_palette, help};

struct Region(&'static str, Option<Rectangle>);
impl Operation for Region {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }
    fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
        if id == Some(&Id::new(self.0)) {
            self.1 = Some(bounds);
        }
    }
}

fn region(ui: &mut Ui, app: &App, id: &'static str) -> Rectangle {
    let mut find = Region(id, None);
    ui.inspect(app, &mut find);
    find.1.expect("container in the actual palette layout")
}

fn assert_filters(ui: &mut Ui, app: &App, active: &str) {
    let snapshot = ui.semantics(app);
    assert!(snapshot.duplicate_ids.is_empty());
    let mut ids = snapshot
        .nodes
        .iter()
        .filter(|node| node.id.starts_with("command-palette-filter-"))
        .map(|node| {
            let visible = node.visible_bounds.expect("visible filter");
            assert!((visible.width - node.bounds.width).abs() < 0.02);
            assert!((visible.height - node.bounds.height).abs() < 0.02);
            assert_eq!(node.checked, Some(node.id.ends_with(active)));
            node.id.as_str()
        })
        .collect::<Vec<_>>();
    ids.sort_unstable();
    assert_eq!(
        ids,
        [
            "command-palette-filter-all",
            "command-palette-filter-edit",
            "command-palette-filter-file",
            "command-palette-filter-insert",
            "command-palette-filter-keys",
            "command-palette-filter-view",
        ]
    );
    assert_eq!(
        snapshot
            .nodes
            .iter()
            .filter(|node| node.id == "command-palette-close")
            .count(),
        1,
        "Commands and Keys share one live dialog"
    );
    assert!(!snapshot.nodes.iter().any(|node| node.id == "help-close"));
}

#[tokio::test]
#[ignore = "Opt-in real unified Commands/Keys entry, focus and narrow layout check"]
async fn keys_and_commands_share_one_palette_and_keep_keyboard_ownership() {
    for size in [Size::new(1040., 680.), Size::new(700., 680.)] {
        let mut ui = Ui::new().await;
        ui.viewport = Rectangle::with_size(size);
        let (mut app, _) = App::new();
        app.tab.busy = false;
        app.tab.fit_to_view = false;
        app.tab.doc = reshiki::rings::Preset::Benzene.document(42., false);
        let atom = app.tab.doc.atoms[0].id;
        app.tab.selected = vec![atom];
        let drawing = app.tab.doc.clone();
        let camera = app.tab.camera;
        let paper = region(&mut ui, &app, "atom-text-canvas");
        app.viewport = paper.size();

        let help_button = ui.bounds(&app, "help-open").center();
        let _ = ui.drag(&mut app, help_button, help_button).await;
        assert!(app.help_open && !app.commands.open);
        assert_filters(&mut ui, &app, "keys");
        ui.key(
            &mut app,
            Key::Named(Named::Escape),
            Code::Escape,
            Modifiers::empty(),
            None,
        )
        .await;
        ui.key(
            &mut app,
            Key::Named(Named::F1),
            Code::F1,
            Modifiers::empty(),
            None,
        )
        .await;
        assert!(app.help_open && !app.commands.open);
        assert!(ui.input(&app, "help-search").1);
        assert_filters(&mut ui, &app, "keys");
        let task = app.update(Message::Help(help::Action::Query("wedge".into())));
        ui.run_task(&mut app, task).await;
        let detail = region(&mut ui, &app, "command-palette-detail");
        assert!(detail.y > ui.input(&app, "help-search").0.y);
        for label in ["Before", "After"] {
            let bounds = ui.text(&app, label);
            assert!(detail.contains(bounds.center()));
        }
        let (status, messages) = ui.event(
            &app,
            press(
                Key::Named(Named::Delete),
                Code::Delete,
                Modifiers::empty(),
                None,
            ),
            mouse::Cursor::Unavailable,
        );
        assert_eq!(status, iced::event::Status::Captured);
        for message in messages {
            let task = app.update(message);
            ui.run_task(&mut app, task).await;
        }
        assert_eq!(app.tab.doc, drawing);
        assert_eq!(app.tab.selected, [atom]);

        ui.key(
            &mut app,
            Key::Named(Named::Tab),
            Code::Tab,
            Modifiers::empty(),
            None,
        )
        .await;
        assert!(app.commands.open && !app.help_open);
        assert_eq!(app.commands.query, "wedge");
        assert!(ui.input(&app, command_palette::SEARCH).1);
        assert_filters(&mut ui, &app, "all");
        ui.key(
            &mut app,
            Key::Named(Named::Tab),
            Code::Tab,
            Modifiers::SHIFT,
            None,
        )
        .await;
        assert!(app.help_open && !app.commands.open);
        assert_eq!(app.help.query, "wedge");
        assert!(ui.input(&app, "help-search").1);

        // Search's global chord always returns to ordinary Commands mode.
        ui.key(
            &mut app,
            Key::Character("k".into()),
            Code::KeyK,
            Modifiers::COMMAND,
            None,
        )
        .await;
        assert!(app.commands.open && !app.help_open);
        assert_eq!(app.commands.filter, None);
        assert!(app.commands.query.is_empty());
        assert_filters(&mut ui, &app, "all");
        ui.key(
            &mut app,
            Key::Named(Named::F1),
            Code::F1,
            Modifiers::empty(),
            None,
        )
        .await;
        assert!(app.help_open && !app.commands.open);
        let task = app.update(Message::Help(help::Action::Query("select all".into())));
        ui.run_task(&mut app, task).await;
        ui.key(
            &mut app,
            Key::Named(Named::Enter),
            Code::Enter,
            Modifiers::empty(),
            None,
        )
        .await;
        assert!(!app.help_open && !app.commands.open);
        assert_eq!(app.tab.selected.len(), drawing.atoms.len());
        assert_eq!(app.tab.doc, drawing);
        assert!(!app.tab.history.can_undo());

        // Escape closes the same surface without changing the drawing or view.
        let task = app.update(Message::ToggleHelp);
        ui.run_task(&mut app, task).await;
        ui.key(
            &mut app,
            Key::Named(Named::Escape),
            Code::Escape,
            Modifiers::empty(),
            None,
        )
        .await;
        assert!(!app.help_open && !app.commands.open);
        assert_eq!(region(&mut ui, &app, "atom-text-canvas"), paper);
        assert_eq!(app.tab.camera.center, camera.center);
        assert_eq!(app.tab.camera.zoom, camera.zoom);
    }
}

async fn open_palette(ui: &mut Ui, app: &mut App, keys: bool, query: &str) -> &'static str {
    let task = app.update(if keys {
        Message::ToggleHelp
    } else {
        Message::Commands(command_palette::Action::Open)
    });
    ui.run_task(app, task).await;
    let task = app.update(if keys {
        Message::Help(help::Action::Query(query.into()))
    } else {
        Message::Commands(command_palette::Action::Query(query.into()))
    });
    ui.run_task(app, task).await;
    if keys {
        "help-search"
    } else {
        command_palette::SEARCH
    }
}

async fn f6(ui: &mut Ui, app: &mut App, backwards: bool) {
    ui.key(
        app,
        Key::Named(Named::F6),
        Code::F6,
        if backwards {
            Modifiers::SHIFT
        } else {
            Modifiers::empty()
        },
        None,
    )
    .await;
}

fn visible_focus(ui: &mut Ui, app: &App, id: &str) {
    assert_eq!(ui.focused(app), [id]);
    let node = ui
        .semantics(app)
        .nodes
        .into_iter()
        .find(|node| node.id == id)
        .unwrap();
    assert!(node.enabled);
    let shown = node.visible_bounds.expect("focused action is visible");
    assert!((shown.width - node.bounds.width).abs() < 0.02, "{node:?}");
    assert!((shown.height - node.bounds.height).abs() < 0.02, "{node:?}");
}

#[tokio::test]
#[ignore = "Opt-in palette F6 action traversal and search/filter ownership"]
async fn f6_reaches_palette_actions_and_returns_to_search() {
    for size in [Size::new(1040., 680.), Size::new(700., 680.)] {
        let mut ui = Ui::new().await;
        ui.viewport = Rectangle::with_size(size);
        for keys in [false, true] {
            let (mut app, _) = App::new();
            app.tab.busy = false;
            app.tab.fit_to_view = false;
            app.tab.doc = reshiki::rings::Preset::Benzene.document(42., false);
            app.tab.selected = vec![app.tab.doc.atoms[0].id];
            let drawing = app.tab.doc.clone();
            let selected = app.tab.selected.clone();
            let camera = app.tab.camera;
            let search = open_palette(&mut ui, &mut app, keys, "select all").await;
            visible_focus(&mut ui, &app, search);
            assert!(
                ui.text(&app, "↑↓ Move · ↵ Run · Tab Filter · F6 Actions · Esc")
                    .width
                    > 0.
            );
            let run = ui
                .semantics(&app)
                .nodes
                .into_iter()
                .find(|node| node.id.starts_with("help-run-"))
                .map(|node| node.id);
            assert_eq!(run.is_some(), keys);
            let mut forward = Vec::new();
            if let Some(id) = &run {
                forward.push(id.as_str());
            }
            forward.extend(["help-about", "help-examples", search]);
            for id in forward {
                f6(&mut ui, &mut app, false).await;
                visible_focus(&mut ui, &app, id);
            }
            let mut reverse = vec!["help-examples", "help-about"];
            if let Some(id) = &run {
                reverse.push(id.as_str());
            }
            reverse.push(search);
            for id in reverse {
                f6(&mut ui, &mut app, true).await;
                visible_focus(&mut ui, &app, id);
            }
            ui.key(
                &mut app,
                Key::Character("z".into()),
                Code::KeyZ,
                Modifiers::empty(),
                Some("z"),
            )
            .await;
            assert_eq!(ui.input_value(&app, search), "select allz");
            for modifiers in [Modifiers::empty(), Modifiers::SHIFT] {
                ui.key(&mut app, Key::Named(Named::Tab), Code::Tab, modifiers, None)
                    .await;
            }
            assert_eq!(app.help_open, keys);
            assert_eq!(app.commands.open, !keys);
            visible_focus(&mut ui, &app, search);
            assert_eq!(ui.input_value(&app, search), "select allz");
            f6(&mut ui, &mut app, true).await;
            visible_focus(&mut ui, &app, "help-examples");
            ui.key(
                &mut app,
                Key::Named(Named::Escape),
                Code::Escape,
                Modifiers::empty(),
                None,
            )
            .await;
            assert!(!app.help_open && !app.commands.open);
            let search = open_palette(&mut ui, &mut app, keys, "wedge").await;
            visible_focus(&mut ui, &app, search);
            f6(&mut ui, &mut app, false).await;
            visible_focus(&mut ui, &app, "help-about");
            assert_eq!(app.tab.doc, drawing);
            assert_eq!(app.tab.selected, selected);
            assert_eq!(app.tab.camera.center, camera.center);
            assert_eq!(app.tab.camera.zoom, camera.zoom);
            assert!(!app.tab.history.can_undo());
            if keys {
                app.tab.doc = Default::default();
                app.tab.selected.clear();
                let task = app.update(Message::Help(help::Action::Query("delete".into())));
                ui.run_task(&mut app, task).await;
                assert!(
                    ui.semantics(&app)
                        .nodes
                        .iter()
                        .any(|node| node.id.starts_with("help-run-") && !node.enabled)
                );
                f6(&mut ui, &mut app, true).await;
                visible_focus(&mut ui, &app, search);
                f6(&mut ui, &mut app, false).await;
                visible_focus(&mut ui, &app, "help-about");
            }
        }
    }
}

#[tokio::test]
#[ignore = "Opt-in keyboard-only palette About, Examples and Run activation"]
async fn palette_actions_run_from_the_keyboard_in_both_modes() {
    let mut ui = Ui::new().await;
    for keys in [false, true] {
        for action in ["about", "examples", "run"] {
            let (mut app, _) = App::new();
            app.tab.busy = false;
            app.tab.fit_to_view = false;
            app.tab.doc = reshiki::rings::Preset::Benzene.document(42., false);
            app.tab.selected = vec![app.tab.doc.atoms[0].id];
            let original = app.tab.id;
            let drawing = app.tab.doc.clone();
            let selected = app.tab.selected.clone();
            let search = open_palette(&mut ui, &mut app, keys, "select all").await;
            if action == "examples" {
                f6(&mut ui, &mut app, true).await;
                visible_focus(&mut ui, &app, "help-examples");
            } else if action == "about" {
                for _ in 0..if keys { 2 } else { 1 } {
                    f6(&mut ui, &mut app, false).await;
                }
                visible_focus(&mut ui, &app, "help-about");
            } else if keys {
                f6(&mut ui, &mut app, false).await;
                let focused = ui.focused(&app);
                assert!(matches!(focused.as_slice(), [id] if id.starts_with("help-run-")));
                visible_focus(&mut ui, &app, &focused[0]);
            } else {
                // Commands run from search; F6 must let the user return there.
                f6(&mut ui, &mut app, false).await;
                f6(&mut ui, &mut app, true).await;
                visible_focus(&mut ui, &app, search);
            }
            assert_eq!(app.tab.doc, drawing);
            assert_eq!(app.tab.selected, selected);
            let (status, messages) = ui.event(
                &app,
                press(
                    Key::Named(Named::Enter),
                    Code::Enter,
                    Modifiers::empty(),
                    None,
                ),
                mouse::Cursor::Unavailable,
            );
            assert_eq!(status, iced::event::Status::Captured);
            match action {
                "about" => assert!(matches!(
                    messages.as_slice(),
                    [Message::Updates(updates::Action::Show(true))]
                )),
                "examples" => assert!(matches!(
                    messages.as_slice(),
                    [Message::OpenShortcutExamples]
                )),
                _ if keys => assert!(matches!(
                    messages.as_slice(),
                    [Message::Help(help::Action::Run(_))]
                )),
                _ => assert!(matches!(
                    messages.as_slice(),
                    [Message::Commands(command_palette::Action::Submit)]
                )),
            }
            // Resolve the real button/search action without polling unrelated
            // About account/network work that its handler can return.
            apply(&mut app, messages);
            assert!(!app.help_open && !app.commands.open);
            match action {
                "about" => {
                    assert!(app.updates.open);
                    assert_eq!(app.tab.doc, drawing);
                }
                "examples" => {
                    assert_eq!(app.document_name(), "Shortcut examples");
                    assert_eq!(
                        app.tabs
                            .background
                            .iter()
                            .find(|tab| tab.id == original)
                            .unwrap()
                            .doc,
                        drawing
                    );
                }
                _ => {
                    assert_eq!(app.tab.doc, drawing);
                    assert_eq!(app.tab.selected.len(), drawing.atoms.len());
                }
            }
            assert!(!app.tab.history.can_undo());
        }
    }
}
