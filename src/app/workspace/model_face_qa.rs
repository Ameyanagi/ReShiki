//! Matched real-renderer and pointer evidence for model faces, frames and hints.
use super::layout_snapshots::pass;
use super::*;
use iced::advanced::{Layout, layout, mouse, renderer::Headless, widget::Tree};
use iced::{Event, Point, Rectangle, Size, Vector};
use reshiki::{
    document::Document,
    models3d::{self, Display, Look},
};
use std::path::Path;

fn canvas_bounds(layout: Layout<'_>, size: Size) -> Option<Rectangle> {
    if layout.bounds().size() == size && layout.children().next().is_none() {
        return Some(layout.bounds());
    }
    layout
        .children()
        .find_map(|child| canvas_bounds(child, size))
}

fn input(
    app: &mut App,
    renderer: &mut iced::Renderer,
    tree: &mut Tree,
    size: Size,
    event: mouse::Event,
    at: Point,
) -> Rectangle {
    let mut messages = vec![];
    let bounds;
    {
        let mut view = app.view();
        tree.diff(view.as_widget());
        let node = view
            .as_widget_mut()
            .layout(tree, renderer, &layout::Limits::new(size, size));
        view.as_widget_mut().update(
            tree,
            &Event::Mouse(event),
            Layout::new(&node),
            mouse::Cursor::Available(at),
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
        bounds = canvas_bounds(Layout::new(&node), viewport).unwrap();
    }
    for message in messages {
        let _ = app.update(message);
    }
    bounds
}

fn settle(app: &mut App, renderer: &mut iced::Renderer, tree: &mut Tree, size: Size) -> Rectangle {
    for _ in 0..3 {
        let messages = pass(app, renderer, tree, size, None);
        for message in messages {
            let _ = app.update(message);
        }
    }
    input(
        app,
        renderer,
        tree,
        size,
        mouse::Event::CursorMoved {
            position: Point::new(20., 200.),
        },
        Point::new(20., 200.),
    )
}

fn setup(input: &Path, reset: bool) -> App {
    let (mut app, _) = App::new();
    let _ = app.update(Message::KeyboardDrawing(
        super::super::keyboard_drawing::Action::Leave,
    ));
    app.tab.doc = Document::from_json(&std::fs::read(input).unwrap()).unwrap();
    let chemistry: serde_json::Value = serde_json::from_slice(
        &std::fs::read(input.parent().unwrap().join("chemistry-receipts.json")).unwrap(),
    )
    .unwrap();
    let name = if input
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("caffeine")
    {
        "caffeine"
    } else {
        "β-D-glucopyranose"
    };
    app.tab.analysis = Some(
        serde_json::from_value(
            chemistry
                .as_array()
                .unwrap()
                .iter()
                .find(|receipt| receipt["name"] == name)
                .unwrap()["before"]
                .clone(),
        )
        .unwrap(),
    );
    if reset {
        // Reuse one actual worker conformer, creating its opening view through
        // each source's production constructor rather than generating a new
        // random pose for the comparison.
        let old = &app.tab.doc.models[0];
        let opening = models3d::Model3d::new(
            &app.tab.doc,
            old.atoms.clone(),
            old.hydrogens.clone(),
            old.force_field,
        );
        app.tab.doc.models[0].rotation = opening.rotation;
    }
    app.tab.selected.clear();
    app.tab.saved = app.tab.doc.clone();
    app.tab.busy = false;
    app.tab.labels_dirty = false;
    app.tab.fit_to_view = false;
    app.appearance.mode = crate::appearance::Mode::Light;
    app.status = "Ready".into();
    app.inspector_open = true;
    app.inspector_tab = InspectorTab::Properties;
    app.tab.forget_unsaved();
    app
}

fn center(app: &mut App, zoom: f32) {
    let (lo, hi) = reshiki::scene::selection_bounds(&app.tab.doc, &app.tab.doc.all_ids()).unwrap();
    app.tab.camera.center = reshiki::document::Point::new((lo.x + hi.x) / 2., (lo.y + hi.y) / 2.);
    app.tab.camera.zoom = zoom;
}

fn show_only(app: &mut App) {
    let before = app.tab.doc.clone();
    models3d::set_display(&mut app.tab.doc, 0, Display::ThreeD);
    let _ = reshiki::transaction::reconcile(&mut app.tab.doc, before).unwrap();
    app.tab.saved = app.tab.doc.clone();
    app.tab.forget_unsaved();
}

fn screen(app: &App, at: reshiki::document::Point, bounds: Rectangle) -> Point {
    app.tab.camera.screen(at, bounds) + Vector::new(bounds.x, bounds.y)
}

fn hint(path: &Path, bounds: Rectangle) -> bool {
    let image = image::open(path).unwrap().into_rgba8();
    let pixel = image
        .get_pixel(
            (bounds.x + 12.) as u32,
            (bounds.y + bounds.height - 31.) as u32,
        )
        .0;
    pixel[1] > pixel[0].saturating_add(8) && pixel[2] > pixel[0].saturating_add(4)
}

fn correlations(doc: &Document) -> [f32; 2] {
    let model = &doc.models[0];
    let n = model.atoms.len() as f32;
    let center = model
        .atoms
        .iter()
        .map(|placed| doc.atom(placed.id).unwrap().position)
        .fold(reshiki::document::Point::default(), |sum, p| {
            sum.offset(p.x / n, p.y / n)
        });
    models3d::viewed(doc, model)
        .iter()
        .fold([0.; 2], |[x, y], (id, p)| {
            let drawing = doc.atom(*id).unwrap().position;
            [
                x + (drawing.x - center.x) * p[0],
                y - (drawing.y - center.y) * p[1],
            ]
        })
}

#[tokio::test]
#[ignore = "Opt-in GPU evidence: RESHIKI_QA_MODEL_FACE_INPUTS and RESHIKI_QA_MODEL_FACE_OUTPUT"]
async fn model_face_frames_and_hints_renderer_evidence() {
    let inputs =
        std::path::PathBuf::from(std::env::var_os("RESHIKI_QA_MODEL_FACE_INPUTS").unwrap());
    let output =
        std::path::PathBuf::from(std::env::var_os("RESHIKI_QA_MODEL_FACE_OUTPUT").unwrap());
    let baseline = std::env::var_os("RESHIKI_QA_MODEL_FACE_BASELINE").is_some();
    std::fs::create_dir_all(&output).unwrap();
    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .unwrap();
    let size = Size::new(1600., 1000.);
    let mut receipts = vec![];
    for (name, zoom) in [("caffeine", 1.75), ("glucose", 2.)] {
        let mut app = setup(&inputs.join(format!("{name}-model.rsk")), true);
        if name == "glucose" {
            app.inspector_open = false;
            app.tab.doc.models[0].look = Look::Glossy;
            app.tab.saved = app.tab.doc.clone();
            app.tab.forget_unsaved();
        }
        let mut tree = Tree::empty();
        settle(&mut app, &mut renderer, &mut tree, size);
        center(&mut app, zoom);
        pass(
            &app,
            &mut renderer,
            &mut tree,
            size,
            Some(&output.join(format!("{name}-both.png"))),
        );
        let correlations = correlations(&app.tab.doc);
        if !baseline {
            assert!(
                correlations.into_iter().all(|value| value > 0.),
                "{name}: {correlations:?}"
            );
        }
        receipts.push(serde_json::json!({"case":name,"correlations":correlations,"rotation":app.tab.doc.models[0].rotation,"zoom":zoom}));
        if name == "caffeine" {
            app.tab.doc.models[0].morph = Some(0.4 / 0.75);
            app.tab.saved = app.tab.doc.clone();
            app.tab.forget_unsaved();
            pass(
                &app,
                &mut renderer,
                &mut tree,
                size,
                Some(&output.join("caffeine-morph-400ms.png")),
            );
        }
    }

    // The interaction comparison retains exactly the same saved pose on both
    // sources, so the face-alignment change cannot obscure frame/hint results.
    let mut app = setup(&inputs.join("glucose-model.rsk"), false);
    show_only(&mut app);
    let original = app.tab.doc.clone();
    let atoms: Vec<_> = app.tab.doc.models[0]
        .atoms
        .iter()
        .map(|atom| atom.id)
        .collect();
    let mut tree = Tree::empty();
    let bounds = settle(&mut app, &mut renderer, &mut tree, size);
    center(&mut app, 2.);
    let (lo, hi) = models3d::frame(&app.tab.doc, &app.tab.doc.models[0]).unwrap();
    let middle = screen(
        &app,
        reshiki::document::Point::new((lo.x + hi.x) / 2., (lo.y + hi.y) / 2.),
        bounds,
    );
    for event in [
        mouse::Event::CursorMoved { position: middle },
        mouse::Event::ButtonPressed(mouse::Button::Left),
        mouse::Event::ButtonReleased(mouse::Button::Left),
    ] {
        input(&mut app, &mut renderer, &mut tree, size, event, middle);
    }
    assert_eq!(
        app.tab.selected,
        reshiki::molecule_names::include_annotations(&app.tab.doc, &atoms)
    );
    let outside = Point::new(
        bounds.x + bounds.width - 50.,
        bounds.y + bounds.height - 70.,
    );
    input(
        &mut app,
        &mut renderer,
        &mut tree,
        size,
        mouse::Event::CursorMoved { position: outside },
        outside,
    );
    let rest = output.join("selected-rest-empty.png");
    pass(&app, &mut renderer, &mut tree, size, Some(&rest));
    assert_eq!(hint(&rest, bounds), !baseline);
    input(
        &mut app,
        &mut renderer,
        &mut tree,
        size,
        mouse::Event::CursorMoved { position: middle },
        middle,
    );
    let inside = output.join("selected-rest-inside.png");
    pass(&app, &mut renderer, &mut tree, size, Some(&inside));
    assert_eq!(hint(&inside, bounds), !baseline);

    let corner = screen(&app, hi, bounds) + Vector::new(12., 12.);
    let end = corner + Vector::new(45., 45.);
    for (event, at) in [
        (mouse::Event::CursorMoved { position: corner }, corner),
        (mouse::Event::ButtonPressed(mouse::Button::Left), corner),
        (mouse::Event::CursorMoved { position: end }, end),
    ] {
        input(&mut app, &mut renderer, &mut tree, size, event, at);
    }
    assert_eq!(
        app.tab.doc, original,
        "A held drag only changes the preview"
    );
    pass(
        &app,
        &mut renderer,
        &mut tree,
        size,
        Some(&output.join("held-corner-drag.png")),
    );
    input(
        &mut app,
        &mut renderer,
        &mut tree,
        size,
        mouse::Event::ButtonReleased(mouse::Button::Left),
        end,
    );
    let resized = app.tab.doc.models[0].size > original.models[0].size;
    assert_eq!(resized, !baseline);
    pass(
        &app,
        &mut renderer,
        &mut tree,
        size,
        Some(&output.join("after-corner-drag.png")),
    );
    if !baseline {
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, original, "One Undo restores model and caption");
    }
    receipts.push(serde_json::json!({"case":"caption-corner-drag","resized":resized,"selection":app.tab.selected}));

    let mut app = setup(&inputs.join("glucose-model.rsk"), false);
    show_only(&mut app);
    app.tab.doc.annotations.push(reshiki::document::Annotation {
        id: app.tab.doc.next_id(),
        position: reshiki::document::Point::new(-120., -60.),
        text: "Unrelated note".into(),
        format: Default::default(),
    });
    app.tab.selected = app.tab.doc.all_ids();
    app.tab.saved = app.tab.doc.clone();
    app.tab.forget_unsaved();
    let mut tree = Tree::empty();
    let bounds = settle(&mut app, &mut renderer, &mut tree, size);
    center(&mut app, 2.);
    let p = Point::new(bounds.x + 20., bounds.y + bounds.height - 70.);
    input(
        &mut app,
        &mut renderer,
        &mut tree,
        size,
        mouse::Event::CursorMoved { position: p },
        p,
    );
    let path = output.join("control-unrelated-selection.png");
    pass(&app, &mut renderer, &mut tree, size, Some(&path));
    assert!(!hint(&path, bounds));
    std::fs::write(
        output.join("receipts.json"),
        serde_json::to_vec_pretty(&receipts).unwrap(),
    )
    .unwrap();
}
