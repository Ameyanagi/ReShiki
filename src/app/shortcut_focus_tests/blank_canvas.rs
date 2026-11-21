use super::*;

struct CanvasBounds(Option<Rectangle>);
impl Operation for CanvasBounds {
    fn traverse(&mut self, children: &mut dyn FnMut(&mut dyn Operation)) {
        children(self);
    }
    fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
        if id == Some(&Id::new("atom-text-canvas")) {
            self.0 = Some(bounds);
        }
    }
}

struct CanvasText {
    bounds: Rectangle,
    labels: Vec<String>,
}
impl Operation for CanvasText {
    fn traverse(&mut self, children: &mut dyn FnMut(&mut dyn Operation)) {
        children(self);
    }
    fn text(&mut self, _: Option<&Id>, bounds: Rectangle, text: &str) {
        if bounds.intersection(&self.bounds).is_some() && !text.is_empty() {
            self.labels.push(text.into());
        }
    }
}

fn clear_canvas(ui: &mut Ui, app: &mut App) -> Rectangle {
    assert!(app.tab.doc.object_ids().next().is_none());
    let mut canvas = CanvasBounds(None);
    ui.inspect(app, &mut canvas);
    let bounds = canvas.0.expect("actual drawing bounds");
    app.viewport = bounds.size();
    let inside = bounds.shrink(2.);
    let snapshot = ui.semantics(app);
    assert!(snapshot.duplicate_ids.is_empty());
    assert!(
        snapshot.nodes.iter().all(|node| node
            .visible_bounds
            .is_none_or(|node| node.intersection(&inside).is_none())),
        "An empty canvas has no startup controls: {:?}",
        snapshot.nodes
    );
    let mut text = CanvasText {
        bounds: inside,
        labels: vec![],
    };
    ui.inspect(app, &mut text);
    assert!(text.labels.is_empty(), "{:?}", text.labels);
    assert!(!app.tab.dirty());
    bounds
}

#[tokio::test]
#[ignore = "Opt-in actual blank canvas, drawing input, tab lifecycle and drag feedback"]
async fn empty_canvas_keeps_drawing_tabs_and_drop_feedback_without_startup_widgets() {
    let mut ui = Ui::new().await;
    for size in [Size::new(900., 640.), Size::new(700., 680.)] {
        ui.viewport = Rectangle::with_size(size);
        let (mut app, _) = App::new();
        app.tab.busy = false;
        app.tab.fit_to_view = false;
        app.inspector_open = false;
        let drawing = app.tab.doc.clone();
        let original = app.tab.id;
        clear_canvas(&mut ui, &mut app);
        assert!(!app.tab.history.can_undo());
        assert!(
            ui.semantics(&app)
                .nodes
                .iter()
                .any(|node| node.id == "header-open" && node.enabled)
        );

        ui.key(
            &mut app,
            Key::Character("n".into()),
            Code::KeyN,
            Modifiers::empty(),
            Some("n"),
        )
        .await;
        assert_eq!(app.tab.doc.atoms.len(), 1, "Typing reaches the drawing");
        assert_eq!(app.tab.doc.atoms[0].element, "N");
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, drawing);
        clear_canvas(&mut ui, &mut app);
        assert!(!app.tab.history.can_undo());

        let task = app.update(Message::New);
        ui.run_task(&mut app, task).await;
        clear_canvas(&mut ui, &mut app);
        assert_ne!(app.tab.id, original);
        app.add_tab();
        clear_canvas(&mut ui, &mut app);
        let _ = app.update(Message::Tabs(crate::app::tabs::Action::Select(original)));
        clear_canvas(&mut ui, &mut app);
        app.show_opened(reshiki::document::Document::default(), None);
        let bounds = clear_canvas(&mut ui, &mut app);

        let revision = app.tab.revision;
        let _ = app.update(Message::Imports(crate::app::import::Action::Hovered(
            "sample.rsk".into(),
        )));
        let label = ui.text(&app, "Drop to open · sample.rsk");
        assert!(bounds.contains(label.center()));
        let _ = app.update(Message::Imports(crate::app::import::Action::Left));
        clear_canvas(&mut ui, &mut app);
        app.browser_drag(1);
        let label = ui.text(&app, "Drop to open");
        assert!(bounds.contains(label.center()));
        app.browser_drag(0);
        clear_canvas(&mut ui, &mut app);
        assert_eq!(app.tab.revision, revision);
        assert!(!app.tab.history.can_undo());
    }
}
