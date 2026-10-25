use super::*;
use reshiki::document::{Document, Point};

fn ready(page: InspectorTab, zoom: f32) -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.inspector_open = true;
    app.inspector_tab = page;
    app.viewport = iced::Size::new(1500. - app.inspector_width(), 780.);
    app.tab.doc = reshiki::rings::Preset::Benzene.document(42., false);
    app.tab.selected = vec![app.tab.doc.atoms[0].id];
    app.tab.camera.zoom = zoom;
    app.tab.camera.center = Point::new(42., -20.);
    app
}

fn screen(app: &App, at: Point) -> iced::Point {
    app.tab
        .camera
        .screen(at, iced::Rectangle::with_size(app.viewport))
}

#[test]
fn inspector_pages_keep_screen_coordinates_zoom_fit_and_selection() {
    let pages = [
        InspectorTab::Abbreviations,
        InspectorTab::Properties,
        InspectorTab::Templates,
        InspectorTab::DrawingStyle,
        InspectorTab::Reactions,
        InspectorTab::Assistant,
        InspectorTab::Labels,
        InspectorTab::Import,
        InspectorTab::Export,
        InspectorTab::Names,
        InspectorTab::Pages,
        InspectorTab::ThemeGenerator,
        InspectorTab::Predict,
    ];
    for from in pages {
        for to in pages {
            for zoom in [0.5, 2., 2.5] {
                let mut app = ready(from, zoom);
                app.tab.fit_to_view = true;
                let at = app.tab.doc.atoms[0].position;
                let before = screen(&app, at);
                let document = app.tab.doc.clone();
                let selected = app.tab.selected.clone();
                let _ = app.update(Message::Inspector(to));
                let size = app.viewport;
                let _ = app.update(Message::Viewport(size));
                assert!(
                    screen(&app, at).distance(before) <= 1.,
                    "{from:?} → {to:?} at {zoom}: {before:?} → {:?}",
                    screen(&app, at)
                );
                assert_eq!(app.tab.camera.zoom, zoom, "Page changes must not Fit");
                assert!(app.tab.fit_to_view);
                assert_eq!(app.viewport.width + app.inspector_width(), 1500.);
                assert_eq!(app.tab.selected, selected);
                assert_eq!(app.tab.doc, document);
                assert!(!app.tab.history.can_undo());
            }
        }
    }
}

#[test]
fn direct_page_handlers_and_nested_context_commands_compensate_once() {
    use crate::app::{context_menu, optimization, reactions};
    for message in [
        Message::Reaction(reactions::Action::Open),
        Message::Optimization(optimization::Action::Begin),
        Message::ContextMenu(context_menu::Action::Run(Box::new(Message::Inspector(
            InspectorTab::Properties,
        )))),
    ] {
        let mut app = ready(InspectorTab::Templates, 2.);
        let before = screen(&app, app.tab.doc.atoms[0].position);
        let _ = app.update(message);
        let size = app.viewport;
        let _ = app.update(Message::Viewport(size));
        assert!(screen(&app, app.tab.doc.atoms[0].position).distance(before) <= 1.);
        assert_eq!(app.tab.camera.zoom, 2.);
        assert_eq!(app.viewport.width + app.inspector_width(), 1500.);
    }
}

#[test]
fn page_compensation_preserves_a_handlers_intentional_camera_change() {
    let mut app = ready(InspectorTab::Templates, 2.);
    let before = app.viewport;
    let old_width = app.inspector_width();
    app.inspector_tab = InspectorTab::Properties;
    // A reveal or Fit is allowed to change the camera while opening a page.
    // Compensate only the remaining layout delta, using the new zoom.
    app.tab.camera.center = Point::new(400., -50.);
    app.tab.camera.zoom = 0.75;
    let intended = screen(&app, Point::new(410., -40.));
    app.preserve_inspector_page_position(old_width, before);
    assert!(screen(&app, Point::new(410., -40.)).distance(intended) <= 1.);
    assert_eq!(app.tab.camera.zoom, 0.75);
    let compensated = app.tab.camera.center;
    app.preserve_inspector_page_position(old_width, before);
    assert_eq!(
        app.tab.camera.center, compensated,
        "Nested updates must not repeat it"
    );
    let size = app.viewport;
    let _ = app.update(Message::Viewport(size));
    assert_eq!(app.tab.camera.center, compensated);
    let _ = app.update(Message::Fit);
    assert_ne!(
        app.tab.camera.center, compensated,
        "Explicit Fit still works"
    );
    let fit_zoom = app.tab.camera.zoom;
    let _ = app.update(Message::Viewport(iced::Size::new(600., 400.)));
    assert_ne!(app.tab.camera.zoom, fit_zoom, "A real resize still Fits");
}

#[test]
fn opening_a_drawing_uses_its_own_fit_instead_of_the_previous_camera() {
    use crate::app::files;
    for reusable in [false, true] {
        let mut app = ready(InspectorTab::Abbreviations, 2.5);
        if reusable {
            app.tab.doc = Document::default();
            app.tab.selected.clear();
        }
        let mut document = Document::default();
        document.add_atom("O", Point::new(1500., 80.));
        let mut reference = ready(InspectorTab::Abbreviations, 2.5);
        reference.show_opened(document.clone(), None);
        let old_epoch = app.tab.file_epoch;
        let _ = app.update(Message::FilePrepared(Some((
            "inspector-motion.rsk".into(),
            Ok(files::Prepared::Native(Box::new(document))),
        ))));
        assert_ne!(app.tab.file_epoch, old_epoch);
        assert_eq!(app.tab.camera.center, reference.tab.camera.center);
        assert_eq!(app.tab.camera.zoom, reference.tab.camera.zoom);
        assert!(app.tab.selected.is_empty());
    }
}

#[test]
fn background_properties_preserve_the_front_camera_and_layout() {
    use crate::app::inspector;
    let mut app = ready(InspectorTab::Properties, 2.);
    let id = app.tab.id;
    let key = app.property_request_key();
    let _ = app.update(Message::InspectorAction(
        inspector::Action::RefreshProperties,
    ));
    let front = crate::app::tabs::tests::Front::new(&mut app);
    app.inspector_open = true;
    app.inspector_tab = InspectorTab::Abbreviations;
    let viewport = app.viewport;
    let camera = app.tab.camera;
    let _ = app.update(Message::Tab(
        id,
        Box::new(Message::InspectorAction(
            inspector::Action::PropertiesCalculated(
                key,
                Box::new(Err("Background property notice".into())),
            ),
        )),
    ));
    assert_eq!(app.tab.camera.center, camera.center);
    assert_eq!(app.tab.camera.zoom, camera.zoom);
    assert_eq!(app.viewport, viewport);
    assert_eq!(app.inspector_tab, InspectorTab::Abbreviations);
    app.inspector_open = false;
    app.inspector_tab = InspectorTab::Import;
    front.assert_unchanged(&app);
}

#[test]
fn positioning_marks_keeps_properties_scroll_but_reveals_other_pages() {
    use reshiki::graphics::GraphicKind;
    for (open, page, reveal) in [
        (true, InspectorTab::Properties, false),
        (false, InspectorTab::Properties, true),
        (true, InspectorTab::Abbreviations, true),
    ] {
        let mut app = ready(page, 2.5);
        app.inspector_open = open;
        assert_eq!(
            app.reveals_inspector(&Message::Tool(Tool::EditPoints)),
            reveal
        );
        let before = app.tab.doc.clone();
        let selected = app.tab.selected.clone();
        let task = app.update(Message::Tool(Tool::EditPoints));
        assert_eq!(task.units() > 0, reveal);
        assert_eq!(app.tool, Tool::EditPoints);
        assert!(app.inspector_open);
        assert_eq!(app.inspector_tab, InspectorTab::Properties);
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tab.selected, selected);
        assert_eq!(app.update(Message::Tool(Tool::Select)).units(), 0);
        assert!(app.reveals_inspector(&Message::Tool(Tool::Arrow)));
        assert!(app.reveals_inspector(&Message::Tool(Tool::Graphic(GraphicKind::Ellipse))));
    }
}
