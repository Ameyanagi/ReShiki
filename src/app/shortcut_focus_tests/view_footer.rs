use super::*;
use reshiki::{accessibility::Role, canvas_theme::CanvasTheme, document::Point};

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
#[ignore = "Opt-in real narrow footer click and native canvas-theme activation"]
async fn bottom_canvas_toggle_is_visible_and_operates_without_opening_view() {
    for size in [Size::new(1040., 680.), Size::new(700., 680.)] {
        let mut ui = Ui::new().await;
        ui.viewport = Rectangle::with_size(size);
        let (mut app, _) = App::new();
        app.tab.busy = false;
        app.tab.fit_to_view = false;
        app.appearance = crate::appearance::Settings::default();
        app.tab.doc.canvas_theme = CanvasTheme::Light;
        app.tab.doc.add_atom("C", Point::new(80., 25.));
        app.tab.camera.center = Point::new(85., -23.);
        app.tab.camera.zoom = 1.4;
        let canvas = paper(&mut ui, &app);
        app.viewport = canvas.size();
        let camera = app.tab.camera;
        let drawing = app.tab.doc.clone();
        let frames = app.tab.history.frames();
        let dirty = app.tab.dirty();
        let snapshot = ui.semantics(&app);
        assert!(snapshot.duplicate_ids.is_empty());
        let toggle = snapshot
            .nodes
            .iter()
            .find(|node| node.id == "canvas-theme")
            .unwrap();
        assert_eq!(toggle.name, "Dark canvas");
        assert_eq!(toggle.role, Role::ToggleButton);
        assert_eq!(toggle.checked, Some(false));
        assert!(toggle.enabled);
        let visible = toggle.visible_bounds.expect("fully visible footer toggle");
        assert!((visible.width - toggle.bounds.width).abs() < 0.02);
        assert!((visible.height - toggle.bounds.height).abs() < 0.02);
        assert!(toggle.bounds.y >= size.height - 50.);
        assert!(toggle.bounds.x >= 0. && toggle.bounds.x + toggle.bounds.width <= size.width);
        assert!(ui.bounds(&app, "status-style").x < toggle.bounds.x);
        assert!(toggle.bounds.x + toggle.bounds.width <= ui.bounds(&app, "status-view").x);
        assert!(!snapshot.nodes.iter().any(|node| node.id == "view-grid"));
        assert!(!app.view_open && !app.status_style_open);

        let at = toggle.bounds.center();
        let messages = ui.drag(&mut app, at, at).await;
        assert_eq!(
            messages
                .iter()
                .filter(|message| matches!(message, Message::CanvasTheme(CanvasTheme::Dark)))
                .count(),
            1,
            "one bottom-button click toggles the canvas once"
        );
        assert!(
            !messages
                .iter()
                .any(|message| matches!(message, Message::View(_)))
        );
        assert_eq!(app.tab.doc.canvas_theme, CanvasTheme::Dark);
        assert_eq!(app.appearance.canvas_theme, CanvasTheme::Dark);
        assert!(!app.view_open && !app.status_style_open);
        assert_eq!(paper(&mut ui, &app), canvas);
        assert_eq!(ui.bounds(&app, "canvas-theme"), toggle.bounds);
        assert_eq!(
            ui.semantics(&app)
                .nodes
                .iter()
                .find(|node| node.id == "canvas-theme")
                .unwrap()
                .checked,
            Some(true)
        );

        let mut activate = reshiki::accessibility::Activate::<Message>::new("canvas-theme");
        ui.inspect(
            &app,
            &mut iced::advanced::widget::operation::black_box(&mut activate),
        );
        let message = activate
            .message()
            .cloned()
            .expect("live native footer activation");
        assert!(matches!(message, Message::CanvasTheme(CanvasTheme::Light)));
        let task = app.update(message);
        ui.run_task(&mut app, task).await;
        assert_eq!(app.tab.doc, drawing);
        assert_eq!(app.appearance.canvas_theme, CanvasTheme::Light);
        assert_eq!(app.tab.history.frames(), frames);
        assert_eq!(app.tab.dirty(), dirty);
        assert_eq!(app.tab.camera.center, camera.center);
        assert_eq!(app.tab.camera.zoom, camera.zoom);
        assert_eq!(paper(&mut ui, &app), canvas);
        assert!(!app.view_open && !app.status_style_open);
        let snapshot = ui.semantics(&app);
        assert!(snapshot.duplicate_ids.is_empty());
        assert_eq!(
            snapshot
                .nodes
                .iter()
                .filter(|node| node.id == "canvas-theme")
                .count(),
            1
        );
        assert!(!snapshot.nodes.iter().any(|node| node.id == "view-grid"));
        assert!(snapshot.nodes.iter().any(|node| node.id == "status-style"));
        assert!(snapshot.nodes.iter().any(|node| node.id == "status-view"));
    }
}
