//! Matched captures from the actual widget tree and renderer; opt-in only.
use super::layout_snapshots::pass;
use super::*;
use iced::advanced::{Layout, layout, renderer::Headless, widget::Operation, widget::Tree};
use iced::futures::StreamExt;
use iced::{Rectangle, Size, Task};
use reshiki::document::Point;
use reshiki::engine::ChemistryEngine;
use std::path::Path;

fn operate<T: 'static>(
    app: &App,
    renderer: &mut iced::Renderer,
    tree: &mut Tree,
    size: Size,
    operation: &mut dyn Operation<T>,
) {
    let mut view = app.view();
    tree.diff(view.as_widget());
    let node = view
        .as_widget_mut()
        .layout(tree, renderer, &layout::Limits::new(size, size));
    view.as_widget_mut().operate(
        tree,
        Layout::new(&node),
        renderer,
        &mut iced::advanced::widget::operation::black_box(operation),
    );
}

async fn run(
    app: &mut App,
    renderer: &mut iced::Renderer,
    tree: &mut Tree,
    size: Size,
    task: Task<Message>,
) {
    let mut pending = vec![task];
    while let Some(task) = pending.pop() {
        let Some(mut stream) = iced_runtime::task::into_stream(task) else {
            continue;
        };
        while let Some(action) = stream.next().await {
            match action {
                iced_runtime::Action::Widget(mut operation) => {
                    operate(app, renderer, tree, size, operation.as_mut());
                }
                iced_runtime::Action::Output(message) => pending.push(app.update(message)),
                _ => panic!("Unexpected external action in inspector QA"),
            }
        }
    }
}

fn settle(app: &mut App, renderer: &mut iced::Renderer, tree: &mut Tree, size: Size) {
    for _ in 0..4 {
        let messages = pass(app, renderer, tree, size, None);
        if messages.is_empty() {
            break;
        }
        for message in messages {
            let _ = app.update(message);
        }
    }
}

fn text_bounds(
    app: &App,
    renderer: &mut iced::Renderer,
    tree: &mut Tree,
    size: Size,
    target: &str,
) -> (Rectangle, Option<Rectangle>) {
    use iced::Vector;
    use iced::advanced::widget::{Id, operation::Scrollable};
    struct Find<'a> {
        target: &'a str,
        context: (Vector, Option<Rectangle>),
        pending: Option<(Vector, Option<Rectangle>)>,
        found: Option<(Rectangle, Option<Rectangle>)>,
    }
    impl Operation for Find<'_> {
        fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation)) {
            let previous = self.context;
            if let Some(context) = self.pending.take() {
                self.context = context;
            }
            visit(self);
            self.context = previous;
        }
        fn scrollable(
            &mut self,
            _: Option<&Id>,
            bounds: Rectangle,
            _: Rectangle,
            translation: Vector,
            _: &mut dyn Scrollable,
        ) {
            self.pending = Some((
                self.context.0 - translation,
                self.context
                    .1
                    .and_then(|clip| clip.intersection(&(bounds + self.context.0))),
            ));
        }
        fn text(&mut self, _: Option<&Id>, bounds: Rectangle, text: &str) {
            if text == self.target {
                let bounds = bounds + self.context.0;
                self.found = Some((
                    bounds,
                    self.context.1.and_then(|clip| clip.intersection(&bounds)),
                ));
            }
        }
    }
    let mut find = Find {
        target,
        context: (Vector::ZERO, Some(Rectangle::with_size(size))),
        pending: None,
        found: None,
    };
    operate(app, renderer, tree, size, &mut find);
    find.found.expect("Text exists in the real widget tree")
}

async fn click(
    app: &mut App,
    renderer: &mut iced::Renderer,
    tree: &mut Tree,
    size: Size,
    at: iced::Point,
) {
    use iced::{Event, advanced::mouse};
    for event in [
        mouse::Event::CursorMoved { position: at },
        mouse::Event::ButtonPressed(mouse::Button::Left),
        mouse::Event::ButtonReleased(mouse::Button::Left),
    ] {
        let mut messages = vec![];
        {
            let mut view = app.view();
            tree.diff(view.as_widget());
            let node =
                view.as_widget_mut()
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
        }
        for message in messages {
            let task = app.update(message);
            run(app, renderer, tree, size, task).await;
        }
    }
}

async fn drawing(smiles: &str) -> App {
    let (mut app, _) = App::new();
    let response = app
        .engine
        .execute(super::super::Request::import_smiles(smiles))
        .await
        .unwrap();
    app.tab.doc = response.document.unwrap();
    app.tab.analysis = response.analysis;
    app.tab.busy = false;
    app.tab.labels_dirty = false;
    app.status = "Ready".into();
    app.appearance.mode = crate::appearance::Mode::Light;
    app.inspector_open = true;
    app.inspector_tab = InspectorTab::Properties;
    app
}

fn save_input(app: &mut App, output: &Path, name: &str) {
    app.tab.saved = app.tab.doc.clone();
    std::fs::write(output.join(name), app.tab.doc.file_json().unwrap()).unwrap();
}

#[tokio::test]
#[ignore = "Opt-in GPU evidence: RESHIKI_QA_INSPECTOR_OUTPUT required"]
async fn inspector_motion_renderer_evidence() {
    let output = std::path::PathBuf::from(std::env::var_os("RESHIKI_QA_INSPECTOR_OUTPUT").unwrap());
    let baseline = std::env::var_os("RESHIKI_QA_INSPECTOR_BASELINE").is_some();
    std::fs::create_dir_all(&output).unwrap();
    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .unwrap();
    let size = Size::new(1600., 1000.);
    let mut app = drawing("CN1CCN(c2ccccc2)CC1").await;
    save_input(&mut app, &output, "phenylpiperazine.rsk");
    app.inspector_tab = InspectorTab::Abbreviations;
    let mut tree = Tree::empty();
    settle(&mut app, &mut renderer, &mut tree, size);
    app.tab.camera.zoom = 2.;
    app.tab.fit_to_view = false;
    let at = app.tab.doc.atoms[8].position;
    app.tab.camera.center = Point::new(at.x + 150., at.y);
    let before = app
        .tab
        .camera
        .screen(at, Rectangle::with_size(app.viewport));
    pass(
        &app,
        &mut renderer,
        &mut tree,
        size,
        Some(&output.join("page-before.png")),
    );
    let task = app.update(Message::Inspector(InspectorTab::Properties));
    run(&mut app, &mut renderer, &mut tree, size, task).await;
    settle(&mut app, &mut renderer, &mut tree, size);
    let after = app
        .tab
        .camera
        .screen(at, Rectangle::with_size(app.viewport));
    pass(
        &app,
        &mut renderer,
        &mut tree,
        size,
        Some(&output.join("page-after.png")),
    );
    assert_eq!(app.tab.camera.zoom, 2.);
    assert!(!app.tab.fit_to_view);
    assert_eq!(after.x - before.x, if baseline { -22. } else { 0. });
    std::fs::write(output.join("page-receipt.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "before": [before.x, before.y], "after": [after.x, after.y], "zoom": app.tab.camera.zoom,
        "viewport": [app.viewport.width, app.viewport.height], "fit": app.tab.fit_to_view,
    })).unwrap()).unwrap();

    let mut app = drawing("CC(C)=O").await;
    let oxygen = app
        .tab
        .doc
        .atoms
        .iter_mut()
        .find(|atom| atom.element == "O")
        .unwrap();
    for offset in [Point::new(-10., -18.), Point::new(12., -16.)] {
        reshiki::scientific::attach(oxygen, reshiki::scientific::SymbolKind::LonePair, offset)
            .unwrap();
    }
    app.tab.selected = vec![oxygen.id];
    save_input(&mut app, &output, "acetone-marks.rsk");
    app.tab
        .inspector_ui
        .update(super::super::inspector::Action::Section(
            super::super::inspector::Section::Molecule,
            true,
        ));
    let mut tree = Tree::empty();
    let task = app.update(Message::InspectorAction(
        super::super::inspector::Action::RefreshProperties,
    ));
    run(&mut app, &mut renderer, &mut tree, size, task).await;
    settle(&mut app, &mut renderer, &mut tree, size);
    app.tab.camera.zoom = 2.5;
    app.tab.camera.center = app.tab.doc.atoms.last().unwrap().position;
    let mut scroll = iced::advanced::widget::operation::scrollable::scroll_to::<()>(
        "inspector-content".into(),
        iced::widget::operation::AbsoluteOffset {
            x: None,
            y: Some(520.),
        },
    );
    operate(&app, &mut renderer, &mut tree, size, &mut scroll);
    settle(&mut app, &mut renderer, &mut tree, size);
    let position = text_bounds(&app, &mut renderer, &mut tree, size, "Position atom marks");
    assert!(
        position.1.is_some(),
        "Position row must be visible at the starting scroll"
    );
    let start_y = position.0.y;
    pass(
        &app,
        &mut renderer,
        &mut tree,
        size,
        Some(&output.join("marks-before.png")),
    );
    click(
        &mut app,
        &mut renderer,
        &mut tree,
        size,
        position.1.unwrap().center(),
    )
    .await;
    settle(&mut app, &mut renderer, &mut tree, size);
    let finish = text_bounds(
        &app,
        &mut renderer,
        &mut tree,
        size,
        "Finish positioning marks",
    );
    assert_eq!(app.tool, Tool::EditPoints);
    if baseline {
        assert!(
            finish.1.is_none(),
            "Original scroll reset should hide the row"
        );
    } else {
        assert!(finish.1.is_some());
        assert!((finish.0.y - start_y).abs() <= 1.);
    }
    std::fs::write(
        output.join("marks-receipt.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "before_y": start_y, "after_y": finish.0.y, "finish_visible": finish.1.is_some(),
            "zoom": app.tab.camera.zoom, "tool": format!("{:?}", app.tool),
        }))
        .unwrap(),
    )
    .unwrap();
    pass(
        &app,
        &mut renderer,
        &mut tree,
        size,
        Some(&output.join("marks-after.png")),
    );
}
