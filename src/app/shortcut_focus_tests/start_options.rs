use super::*;
use crate::app::start;

#[tokio::test]
#[ignore = "Opt-in actual start-option dismissal and drawing keyboard ownership"]
async fn start_options_dismiss_by_mouse_keyboard_and_native_action_then_allow_drawing() {
    let mut ui = Ui::new().await;
    ui.viewport = Rectangle::with_size(Size::new(900., 640.));
    for method in ["mouse", "keyboard", "native"] {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        app.tab.fit_to_view = false;
        app.inspector_open = false;
        app.start = start::State::default();
        let drawing = app.tab.doc.clone();
        let revision = app.tab.revision;
        let snapshot = ui.semantics(&app);
        assert!(snapshot.duplicate_ids.is_empty());
        assert!(!snapshot.nodes.iter().any(|node| node.id == "start-input"));
        let dismiss = snapshot
            .nodes
            .iter()
            .find(|node| node.id == "start-dismiss")
            .expect("visible start dismissal");
        assert_eq!(dismiss.name, "Start with a blank page");
        assert_eq!(dismiss.role, reshiki::accessibility::Role::Button);
        assert!(dismiss.enabled);
        assert_eq!(dismiss.visible_bounds, Some(dismiss.bounds));

        match method {
            "mouse" => click_running_tasks(&mut ui, &mut app, "start-dismiss").await,
            "keyboard" => {
                ui.focus_control(&mut app, "start-dismiss").await;
                assert_eq!(ui.focused(&app), ["start-dismiss"]);
                ui.key(
                    &mut app,
                    Key::Named(Named::Enter),
                    Code::Enter,
                    Modifiers::empty(),
                    None,
                )
                .await;
            }
            "native" => {
                let mut activate =
                    reshiki::accessibility::Activate::<Message>::new("start-dismiss");
                ui.inspect(
                    &app,
                    &mut iced::advanced::widget::operation::black_box(&mut activate),
                );
                let message = activate.message().cloned().expect("native dismiss action");
                assert!(matches!(message, Message::Start(start::Action::Dismiss)));
                let task = app.update(message);
                ui.run_task(&mut app, task).await;
            }
            _ => unreachable!(),
        }

        assert!(!app.start_visible(), "{method}");
        assert!(
            !ui.semantics(&app)
                .nodes
                .iter()
                .any(|node| node.id.starts_with("start-")),
            "Dismissed options leave no hidden controls"
        );
        ui.assert_unfocused(&app);
        assert_eq!(app.tab.doc, drawing);
        assert_eq!(app.tab.revision, revision);
        assert!(!app.tab.history.can_undo());
        assert!(!app.tab.dirty());

        ui.key(
            &mut app,
            Key::Character("n".into()),
            Code::KeyN,
            Modifiers::empty(),
            Some("n"),
        )
        .await;
        assert_eq!(app.tab.doc.atoms.len(), 1, "{method}");
        assert_eq!(app.tab.doc.atoms[0].element, "N");
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, drawing);
        assert!(!app.start_visible());
        assert!(!app.tab.history.can_undo());
    }
}
