use super::*;
use reshiki::document::{Annotation, Point};

#[test]
fn short_rows_fold_commands_then_arrange_then_summary_then_name() {
    let commands = [120., 56.];
    let row = Fold {
        name: (80., 40.),
        options: 70.,
        summary: 50.,
        commands: &commands,
        arrange: Some((227., 80.)),
    };
    let steps = |width| {
        let fit = fit(width, &row);
        (fit.folded, fit.compact, fit.summary, fit.short)
    };
    // Fixed part, two commands with gaps, and the group after one gap.
    let full = 200. + 130. + 66. + 237.;
    assert_eq!(steps(full), (0, false, true, false));
    assert_eq!(steps(full - 1.), (1, false, true, false));
    assert_eq!(steps(542.), (2, false, true, false));
    assert_eq!(steps(476.), (2, true, true, false));
    assert_eq!(steps(330.), (2, true, true, false));
    assert_eq!(steps(329.), (2, true, false, false));
    assert_eq!(steps(279.), (2, true, false, true));
    assert_eq!(steps(100.), (2, true, false, true));
    assert_eq!(fit(329., &row).fixed, 150.);
    assert_eq!(fit(279., &row).fixed, 110.);
    // Rows without an arrange group, summary or short name.
    let plain = |commands: &[f32], width| {
        let fit = fit(
            width,
            &Fold {
                name: (80., 80.),
                options: 120.,
                summary: 0.,
                commands,
                arrange: None,
            },
        );
        (fit.folded, fit.compact, fit.short)
    };
    assert_eq!(plain(&[56.], 300.), (0, false, false));
    assert_eq!(plain(&[56.], 250.), (1, false, false));
    assert_eq!(plain(&[], 100.), (0, false, false));
}

#[test]
fn row_commands_leave_clipboard_to_menus_and_shortcuts() {
    let (mut app, _) = App::new();
    app.tab.doc = reshiki::rings::Preset::Regular.document(42., false);
    let labels: Vec<_> = app.context_commands().iter().map(|c| c.label).collect();
    assert_eq!(
        labels,
        ["3D optimize…", "Mark [", "Connect ]", "Coordinate }"]
    );
    assert!(app.context_commands().iter().all(|command| !matches!(
        command.message,
        Message::Copy(_) | Message::CopyImage | Message::CopyAs(_) | Message::Paste
    )));
    let _ = app.update(Message::KeyboardDrawing(
        super::super::keyboard_drawing::Action::Leave,
    ));
    assert!(!app.keyboard_drawing_active());
    let labels: Vec<_> = app.context_commands().iter().map(|c| c.label).collect();
    assert_eq!(labels, ["3D optimize…", "Keyboard drawing"]);
    app.tab.selected = app.tab.doc.all_ids();
    let labels: Vec<_> = app.context_commands().iter().map(|c| c.label).collect();
    assert_eq!(
        labels,
        [
            "3D optimize…",
            "Keyboard drawing",
            "Move & attach…",
            "Group"
        ]
    );
    let _ = app.update(Message::Group);
    let labels: Vec<_> = app.context_commands().iter().map(|c| c.label).collect();
    assert_eq!(
        labels,
        [
            "3D optimize…",
            "Keyboard drawing",
            "Move & attach…",
            "Group",
            "Ungroup"
        ]
    );
    assert!(!app.context_commands()[3].enabled);
}

#[test]
fn selection_revealing_properties_keeps_targets_at_the_same_screen_position() {
    for zoom in [0.5, 1., 2.] {
        for (open, tab) in [
            (false, InspectorTab::Properties),
            (true, InspectorTab::Labels),
            (true, InspectorTab::DrawingStyle),
            (true, InspectorTab::Properties),
            (true, InspectorTab::Templates),
        ] {
            let (mut app, _) = App::new();
            app.inspector_open = open;
            app.inspector_tab = tab;
            app.viewport = iced::Size::new(1000. - app.inspector_width(), 600.);
            app.tab.camera.zoom = zoom;
            app.tab.camera.center = Point::new(42., -20.);
            app.tab.fit_to_view = true;
            let at = Point::new(-120., 30.);
            app.tab.doc.annotations.push(Annotation {
                id: 1,
                position: at,
                text: "Conditions".into(),
                format: Default::default(),
            });
            let before = app.tab.doc.clone();
            let screen = app
                .tab
                .camera
                .screen(at, iced::Rectangle::with_size(app.viewport));
            let _ = app.update(Message::Canvas(Edit::Select(vec![1])));
            let size = app.viewport;
            let _ = app.update(Message::Viewport(size));
            assert_eq!(app.tab.camera.zoom, zoom, "Selection must not refit");
            assert_eq!(
                app.tab.camera.screen(at, iced::Rectangle::with_size(size)),
                screen,
                "Inspector {open:?}/{tab:?}, zoom {zoom}"
            );
            assert_eq!(app.tab.doc, before);
            assert!(!app.tab.history.can_undo());
            assert!(
                app.tab.fit_to_view,
                "A real resize should still refit later"
            );
            let _ = app.update(Message::Viewport(iced::Size::new(500., 400.)));
            assert_ne!(app.tab.camera.zoom, zoom, "A real resize still refits");
        }
    }
}

#[test]
fn selection_preserves_manual_pan_and_zoom_then_inspector_toggle_resizes_normally() {
    let (mut app, _) = App::new();
    app.inspector_open = false;
    app.viewport = iced::Size::new(1000., 600.);
    app.tab.doc.arrows.push(reshiki::document::Arrow::new(
        1,
        Point::new(-100., 0.),
        Point::new(100., 0.),
        Default::default(),
        Default::default(),
    ));
    app.edit(Edit::Pan(60., -20.));
    app.edit(Edit::Zoom(1.5, Point::new(-40., 10.)));
    let at = app.tab.doc.arrows[0].start;
    let before = app
        .tab
        .camera
        .screen(at, iced::Rectangle::with_size(app.viewport));
    app.edit(Edit::Select(vec![1]));
    assert_eq!(
        app.tab
            .camera
            .screen(at, iced::Rectangle::with_size(app.viewport)),
        before
    );
    let camera = app.tab.camera;
    let _ = app.update(Message::Viewport(app.viewport));
    assert_eq!(app.tab.camera.center, camera.center);
    assert_eq!(app.tab.camera.zoom, camera.zoom);
    app.edit(Edit::Pan(10., 20.));
    assert_eq!(app.tab.camera.center, camera.center.offset(-10., -20.));
    let _ = app.update(Message::Fit);
    let zoom = app.tab.camera.zoom;
    let _ = app.update(Message::ToggleInspector);
    let _ = app.update(Message::Viewport(iced::Size::new(1000., 600.)));
    assert!(
        app.tab.camera.zoom > zoom,
        "Explicit inspector toggle still refits"
    );
}
