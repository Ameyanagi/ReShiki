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
