//! Opt-in renderer/input regression, also used for matched visual evidence.
use super::*;
use iced::advanced::Renderer as _;
use iced::advanced::{Layout, layout, mouse, renderer::Headless, widget::Tree};
use iced::{Event, Point, Rectangle, Size};
use std::path::Path;

fn canvas_bounds(layout: Layout<'_>, size: Size) -> Option<Rectangle> {
    if layout.bounds().size() == size && layout.children().next().is_none() {
        return Some(layout.bounds());
    }
    layout
        .children()
        .find_map(|child| canvas_bounds(child, size))
}

fn frame(
    app: &mut App,
    renderer: &mut iced::Renderer,
    tree: &mut Tree,
    size: Size,
    input: Option<(mouse::Event, Point)>,
    output: Option<&Path>,
) -> Rectangle {
    let mut messages = Vec::new();
    let bounds;
    {
        let theme = app.theme();
        let mut view = app.view();
        tree.diff(view.as_widget());
        let node = view
            .as_widget_mut()
            .layout(tree, renderer, &layout::Limits::new(size, size));
        let cursor = input.map_or(mouse::Cursor::Unavailable, |(_, point)| {
            mouse::Cursor::Available(point)
        });
        let event = input.map_or_else(
            || {
                Event::Window(iced::window::Event::RedrawRequested(
                    std::time::Instant::now(),
                ))
            },
            |(event, _)| Event::Mouse(event),
        );
        view.as_widget_mut().update(
            tree,
            &event,
            Layout::new(&node),
            cursor,
            renderer,
            &mut iced::advanced::clipboard::Null,
            &mut iced::advanced::Shell::new(&mut messages),
            &Rectangle::with_size(size),
        );
        let viewport = messages
            .iter()
            .find_map(|message| match message {
                Message::Viewport(size) => Some(*size),
                _ => None,
            })
            .unwrap_or(app.viewport);
        bounds = canvas_bounds(Layout::new(&node), viewport).expect("Canvas layout");
        if let Some(output) = output {
            renderer.reset(Rectangle::with_size(size));
            view.as_widget().draw(
                tree,
                renderer,
                &theme,
                &iced::advanced::renderer::Style::default(),
                Layout::new(&node),
                mouse::Cursor::Unavailable,
                &Rectangle::with_size(size),
            );
            let pixels = Headless::screenshot(
                renderer,
                Size::new(size.width as u32, size.height as u32),
                1.,
                theme.palette().background,
            );
            image::save_buffer(
                output,
                &pixels,
                size.width as u32,
                size.height as u32,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
    }
    for message in messages {
        let _ = app.update(message);
    }
    bounds
}

fn click(app: &mut App, renderer: &mut iced::Renderer, tree: &mut Tree, size: Size, point: Point) {
    for event in [
        mouse::Event::CursorMoved { position: point },
        mouse::Event::ButtonPressed(mouse::Button::Left),
        mouse::Event::ButtonReleased(mouse::Button::Left),
    ] {
        frame(app, renderer, tree, size, Some((event, point)), None);
    }
}

#[tokio::test]
#[ignore = "Opt-in GPU layout/input regression and matched snapshots"]
async fn selection_layout_and_double_click_regression() {
    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .unwrap();
    let directory = std::env::var_os("RESHIKI_CANVAS_QA_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("reshiki-selection-canvas-qa"));
    std::fs::create_dir_all(&directory).unwrap();
    let baseline = std::env::var_os("RESHIKI_CANVAS_QA_BASELINE").is_some();
    for (width, height) in [(1040, 680), (1280, 820), (1600, 1000)] {
        for inspector in [false, true] {
            for zoom in [0.7, 1., 1.7] {
                let (mut app, _) = App::new();
                app.doc = reshiki::rings::Preset::Regular.document(42., false);
                app.doc.atoms[0].element = "N".into();
                app.saved = app.doc.clone();
                app.busy = false;
                app.status = "Ready".into();
                app.appearance.mode = crate::appearance::Mode::Light;
                app.inspector_open = inspector;
                app.camera.zoom = zoom;
                app.camera.center = reshiki::document::Point::new(-20., 10.);
                let atom = app.doc.atoms[0].clone();
                let size = Size::new(width as f32, height as f32);
                let mut tree = Tree::empty();
                let before = frame(&mut app, &mut renderer, &mut tree, size, None, None);
                let position = |app: &App, bounds: Rectangle| {
                    app.camera.screen(atom.position, bounds) + iced::Vector::new(bounds.x, bounds.y)
                };
                let point = position(&app, before);
                let capture = inspector && zoom == 1. && width <= 1280;
                let prefix = if baseline { "before" } else { "after" };
                if capture {
                    std::fs::write(
                        directory.join("selection-fixture.rsk"),
                        serde_json::to_vec_pretty(&app.doc).unwrap(),
                    )
                    .unwrap();
                    frame(
                        &mut app,
                        &mut renderer,
                        &mut tree,
                        size,
                        None,
                        Some(&directory.join(format!("{prefix}-{width}-unselected.png"))),
                    );
                }
                click(&mut app, &mut renderer, &mut tree, size, point);
                assert_eq!(app.selected, [atom.id]);
                let after = frame(&mut app, &mut renderer, &mut tree, size, None, None);
                let after_point = position(&app, after);
                if !baseline {
                    assert_eq!(
                        after, before,
                        "Canvas bounds at {width}, inspector {inspector}"
                    );
                    assert_eq!(after_point, point, "Selected atom must stay under cursor");
                }
                if capture {
                    eprintln!("{prefix} {width}×{height}: target {point:?} -> {after_point:?}");
                }
                click(&mut app, &mut renderer, &mut tree, size, point);
                if !baseline {
                    assert_eq!(
                        app.selected.len(),
                        app.doc.atoms.len(),
                        "Second click selects the molecule at {width}, zoom {zoom}"
                    );
                    assert_eq!(app.camera.zoom, zoom);
                    assert_eq!(app.doc, app.saved);
                    assert!(!app.history.can_undo());
                }
                if capture {
                    frame(
                        &mut app,
                        &mut renderer,
                        &mut tree,
                        size,
                        None,
                        Some(&directory.join(format!("{prefix}-{width}-double-click.png"))),
                    );
                    // Capture the single-click state separately: GPU readback
                    // between the two input clicks can exceed their 450 ms
                    // deadline on a busy machine.
                    let _ = app.update(Message::Canvas(Edit::Select(vec![])));
                    tree = Tree::empty();
                    frame(&mut app, &mut renderer, &mut tree, size, None, None);
                    click(&mut app, &mut renderer, &mut tree, size, point);
                    frame(
                        &mut app,
                        &mut renderer,
                        &mut tree,
                        size,
                        None,
                        Some(&directory.join(format!("{prefix}-{width}-selected.png"))),
                    );
                }
            }
        }
    }
    // A different visible defect: selecting a caption automatically opens the
    // inspector. Compare the actual object position across the width change.
    for (width, height) in [(1040, 680), (1280, 820)] {
        let (mut app, _) = App::new();
        app.doc = reshiki::rings::Preset::Regular.document(42., false);
        let id = app.doc.next_id();
        let at = reshiki::document::Point::new(-120., 100.);
        app.doc.annotations.push(reshiki::document::Annotation {
            id,
            position: at,
            text: "Reaction conditions".into(),
            format: Default::default(),
        });
        app.saved = app.doc.clone();
        app.busy = false;
        app.status = "Ready".into();
        app.appearance.mode = crate::appearance::Mode::Light;
        app.inspector_open = false;
        app.camera.center = reshiki::document::Point::new(-20., 10.);
        let size = Size::new(width as f32, height as f32);
        let mut tree = Tree::empty();
        let bounds = frame(&mut app, &mut renderer, &mut tree, size, None, None);
        let point = app.camera.screen(at, bounds) + iced::Vector::new(bounds.x, bounds.y);
        let prefix = if baseline { "before" } else { "after" };
        std::fs::write(
            directory.join("inspector-fixture.rsk"),
            serde_json::to_vec_pretty(&app.doc).unwrap(),
        )
        .unwrap();
        click(&mut app, &mut renderer, &mut tree, size, point);
        assert_eq!(app.selected, [id]);
        assert!(app.inspector_open);
        let bounds = frame(
            &mut app,
            &mut renderer,
            &mut tree,
            size,
            None,
            Some(&directory.join(format!("{prefix}-inspector-{width}-selected.png"))),
        );
        let after_point = app.camera.screen(at, bounds) + iced::Vector::new(bounds.x, bounds.y);
        eprintln!("{prefix} inspector {width}×{height}: target {point:?} -> {after_point:?}");
        if !baseline {
            assert_eq!(
                point, after_point,
                "Auto-revealing Properties keeps the caption in place"
            );
            assert_eq!(app.doc, app.saved);
            assert!(!app.history.can_undo());
        }
    }
}
