use super::*;
use crate::app::status_style::Action;

struct Paper(Option<Rectangle>);
impl Operation for Paper {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }
    fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
        if id == Some(&Id::new("atom-text-canvas")) {
            self.0 = Some(bounds);
        }
    }
}
fn paper(ui: &mut Ui, app: &App) -> Rectangle {
    let mut bounds = Paper(None);
    ui.inspect(app, &mut bounds);
    bounds.0.expect("actual drawing container")
}

#[tokio::test]
#[ignore = "Opt-in real status chooser geometry, focus and selection check"]
async fn style_chooser_keeps_footer_and_canvas_geometry_and_keyboard_focus() {
    use reshiki::document_styles::Preset;
    for size in [Size::new(1280., 820.), Size::new(1040., 680.)] {
        let mut ui = Ui::new().await;
        ui.viewport = Rectangle::with_size(size);
        let (mut app, _) = App::new();
        app.tab.busy = false;
        app.tab.fit_to_view = false;
        app.tab.doc = reshiki::rings::Preset::Benzene.document(42., false);
        app.tab.doc.drawing_style = Preset::Jacs.style();
        app.tab.doc.color_theme = reshiki::canvas_theme::ColorTheme::Publication;
        let drawing = app.tab.doc.clone();
        let canvas = paper(&mut ui, &app);
        app.viewport = canvas.size();
        let revision = app.tab.revision;
        let camera = app.tab.camera;
        let fit = ui.bounds(&app, "status-fit");
        let anchor = ui.bounds(&app, "status-style");
        let snapshot = ui.semantics(&app);
        let summary = snapshot
            .nodes
            .iter()
            .find(|n| n.id == "status-style")
            .unwrap();
        assert_eq!(summary.expanded, Some(false));
        assert!(summary.name.contains("JACS / ACS · Publication"));
        assert!(ui.text(&app, "JACS · Publication").height > 10.);
        assert!(!snapshot.nodes.iter().any(|n| n.id == "journal-preset"));
        ui.focus_control(&mut app, "status-style").await;
        ui.key(
            &mut app,
            Key::Named(Named::Enter),
            Code::Enter,
            Modifiers::empty(),
            None,
        )
        .await;
        assert!(app.status_style_open);
        assert_eq!(ui.focused(&app), ["journal-preset"]);
        assert_eq!(paper(&mut ui, &app), canvas);
        for (key, code) in [(Named::Enter, Code::Enter), (Named::Delete, Code::Delete)] {
            let (status, messages) = ui.event(
                &app,
                press(Key::Named(key), code, Modifiers::empty(), None),
                mouse::Cursor::Unavailable,
            );
            assert_eq!(status, iced::event::Status::Captured);
            assert!(
                messages.is_empty(),
                "Drawing shortcuts stay out of the chooser"
            );
        }
        let snapshot = ui.semantics(&app);
        assert!(snapshot.duplicate_ids.is_empty());
        for id in ["journal-preset", "color-theme"] {
            let node = snapshot.nodes.iter().find(|n| n.id == id).unwrap();
            assert_eq!(node.role, reshiki::accessibility::Role::ComboBox);
            let visible = node.visible_bounds.expect("visible complete selector");
            assert!(node.bounds.width > 240. && node.bounds.height >= 25.);
            assert!((visible.width - node.bounds.width).abs() < 0.02);
            assert!((visible.height - node.bounds.height).abs() < 0.02);
            assert!(node.bounds.y + node.bounds.height <= anchor.y - 3.);
            assert!(node.bounds.x >= 0. && node.bounds.x + node.bounds.width <= size.width);
        }
        for expected in [
            "color-theme",
            "status-manage-styles",
            "status-manage-themes",
            "journal-preset",
        ] {
            ui.key(
                &mut app,
                Key::Named(Named::Tab),
                Code::Tab,
                Modifiers::empty(),
                None,
            )
            .await;
            assert_eq!(ui.focused(&app), [expected]);
        }
        ui.key(
            &mut app,
            Key::Named(Named::Tab),
            Code::Tab,
            Modifiers::SHIFT,
            None,
        )
        .await;
        assert_eq!(ui.focused(&app), ["status-manage-themes"]);
        ui.key(
            &mut app,
            Key::Named(Named::Escape),
            Code::Escape,
            Modifiers::empty(),
            None,
        )
        .await;
        assert!(!app.status_style_open);
        assert_eq!(ui.focused(&app), ["status-style"]);
        assert_eq!(paper(&mut ui, &app), canvas);
        assert_eq!(ui.bounds(&app, "status-style"), anchor);
        assert_eq!(ui.bounds(&app, "status-fit"), fit);
        assert_eq!(app.tab.doc, drawing);
        assert_eq!(app.tab.revision, revision);
        assert!(!app.tab.history.can_undo());
        assert_eq!(app.tab.camera.center, camera.center);
        assert_eq!(app.tab.camera.zoom, camera.zoom);

        // Assistive technology opens the same chooser and edits either value.
        let mut activate = reshiki::accessibility::Activate::<Message>::new("status-style");
        ui.inspect(
            &app,
            &mut iced::advanced::widget::operation::black_box(&mut activate),
        );
        let iced::advanced::widget::operation::Outcome::Some(message) = activate.finish() else {
            panic!("live status button activation");
        };
        let task = app.update(message);
        ui.run_task(&mut app, task).await;
        let mut set =
            reshiki::accessibility::SetValue::<Message>::new("journal-preset", "Nature".into());
        ui.inspect(
            &app,
            &mut iced::advanced::widget::operation::black_box(&mut set),
        );
        let iced::advanced::widget::operation::Outcome::Some(message) = set.finish() else {
            panic!("live journal choice");
        };
        let task = app.update(message);
        ui.run_task(&mut app, task).await;
        assert_eq!(app.tab.doc.drawing_style, Preset::Nature.style());
        assert!(app.status_style_open);
        assert_eq!(ui.focused(&app), ["journal-preset"]);
        // Mouse focus still belongs to the separate theme list.
        let theme = ui.bounds(&app, "color-theme").center();
        ui.drag(&mut app, theme, theme).await;
        assert_eq!(ui.focused(&app), ["color-theme"]);
        // Escape first closes its nested choice list, then the outer chooser.
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
            Key::Named(Named::Escape),
            Code::Escape,
            Modifiers::empty(),
            None,
        )
        .await;
        assert!(!app.status_style_open);
        assert_eq!(ui.focused(&app), ["status-style"]);
        assert_eq!(paper(&mut ui, &app), canvas);

        let at = ui.bounds(&app, "status-style").center();
        ui.drag(&mut app, at, at).await;
        let document = app.tab.doc.clone();
        let point = canvas.position() + iced::Vector::new(20., 20.);
        let messages = ui.drag(&mut app, point, point).await;
        assert!(
            messages
                .iter()
                .any(|message| matches!(message, Message::StatusStyle(Action::Close)))
        );
        assert!(!app.status_style_open);
        assert_eq!(
            app.tab.doc, document,
            "Dismissal cannot draw through the popup"
        );
    }
}

#[tokio::test]
#[ignore = "Opt-in real imported-theme footer and chooser layout check"]
async fn long_embedded_theme_keeps_full_value_inside_a_bounded_status_chooser() {
    let mut ui = Ui::new().await;
    ui.viewport = Rectangle::with_size(Size::new(1040., 680.));
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let mut theme = reshiki::theme_files::bundled().unwrap().remove(0);
    theme.name = "A deliberately long embedded theme name retained from another drawing".into();
    app.tab.doc.custom_theme = Some(Box::new(theme.clone()));
    let anchor = ui.bounds(&app, "status-style");
    assert!(anchor.width <= 214.);
    for id in [
        "status-style",
        "status-view",
        "status-fit",
        "status-zoom-in",
        "status-zoom-out",
    ] {
        let snapshot = ui.semantics(&app);
        let node = snapshot.nodes.iter().find(|node| node.id == id).unwrap();
        let visible = node.visible_bounds.unwrap();
        assert!((visible.width - node.bounds.width).abs() < 0.02);
    }
    let task = app.update(Message::StatusStyle(Action::Toggle));
    ui.run_task(&mut app, task).await;
    let snapshot = ui.semantics(&app);
    let colors = snapshot
        .nodes
        .iter()
        .find(|node| node.id == "color-theme")
        .unwrap();
    assert_eq!(colors.value.as_deref(), Some(theme.name.as_str()));
    assert_eq!(app.theme_library.themes().len(), 0);
}

#[tokio::test]
#[ignore = "Opt-in real status chooser keyboard manager activation"]
async fn style_chooser_managers_open_from_the_keyboard_without_changing_the_drawing() {
    for (id, tab) in [
        ("status-manage-styles", InspectorTab::DrawingStyle),
        ("status-manage-themes", InspectorTab::ThemeGenerator),
    ] {
        let mut ui = Ui::new().await;
        let (mut app, _) = App::new();
        app.tab.busy = false;
        let drawing = app.tab.doc.clone();
        let task = app.update(Message::StatusStyle(Action::Toggle));
        ui.run_task(&mut app, task).await;
        ui.focus_control(&mut app, id).await;
        assert_eq!(ui.focused(&app), [id]);
        ui.key(
            &mut app,
            Key::Named(Named::Enter),
            Code::Enter,
            Modifiers::empty(),
            None,
        )
        .await;
        assert!(!app.status_style_open);
        assert!(app.inspector_open);
        assert_eq!(app.inspector_tab, tab);
        assert_eq!(app.tab.doc, drawing);
        assert!(!app.tab.history.can_undo());
        assert!(!ui.semantics(&app).nodes.iter().any(|node| node.id == id));
    }
}
