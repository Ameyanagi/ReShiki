//! Opt-in real-renderer captures at the issue's 1600 × 1000 logical pixels.
//! Inputs come from `reshiki-io/examples/model_controls_qa.rs`; no drawing or
//! screenshot pixels are synthesized by this harness.
use super::layout_snapshots::pass;
use super::*;
use iced::advanced::{renderer::Headless, widget::Operation, widget::Tree, widget::operation};
use iced::{Rectangle, Size};
use reshiki::document::{Document, Point};
use reshiki::models3d::{self, Display};
use std::path::Path;

fn setup(input: &Path, display: Display) -> App {
    let (mut app, _) = App::new();
    app.tab.doc = Document::from_json(&std::fs::read(input).unwrap()).unwrap();
    models3d::set_display(&mut app.tab.doc, 0, display);
    app.tab.saved = app.tab.doc.clone();
    app.tab.selected = app.tab.doc.all_ids();
    app.tab.busy = false;
    app.status = "Ready".into();
    app.appearance.mode = crate::appearance::Mode::Light;
    app.inspector_open = true;
    app.inspector_tab = InspectorTab::Properties;
    app
}

fn settle(app: &mut App, renderer: &mut iced::Renderer, tree: &mut Tree, size: Size) {
    for _ in 0..3 {
        let messages = pass(app, renderer, tree, size, None);
        if messages.is_empty() {
            break;
        }
        for message in messages {
            let _ = app.update(message);
        }
    }
}

fn semantics(
    app: &App,
    renderer: &mut iced::Renderer,
    size: Size,
) -> reshiki::accessibility::Snapshot {
    let mut ui = iced_runtime::UserInterface::build(app.view(), size, Default::default(), renderer);
    let mut collect = reshiki::accessibility::Collect::new(Rectangle::with_size(size));
    ui.operate(renderer, &mut operation::black_box(&mut collect));
    collect.snapshot().clone()
}

#[tokio::test]
#[ignore = "Opt-in GPU evidence: RESHIKI_QA_MODEL_INPUTS and RESHIKI_QA_MODEL_OUTPUT required"]
async fn model_controls_renderer_evidence() {
    let input = std::path::PathBuf::from(std::env::var_os("RESHIKI_QA_MODEL_INPUTS").unwrap());
    let output = std::path::PathBuf::from(std::env::var_os("RESHIKI_QA_MODEL_OUTPUT").unwrap());
    let baseline = std::env::var_os("RESHIKI_QA_MODELS_BASELINE").is_some();
    std::fs::create_dir_all(&output).unwrap();
    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .unwrap();
    let size = Size::new(1600., 1000.);
    for (name, display) in [
        ("properties-3d", Display::ThreeD),
        ("properties-2d", Display::TwoD),
        ("properties-both", Display::Both),
    ] {
        let mut app = setup(&input.join("caffeine.rsk"), display);
        // Exercise the remembered collapsed state that a default-section change
        // alone cannot repair. Other preferences remain the ordinary defaults.
        let _ = app.update(Message::InspectorAction(
            super::super::inspector::Action::Section(
                super::super::inspector::Section::Model3d,
                false,
            ),
        ));
        let mut tree = Tree::empty();
        settle(&mut app, &mut renderer, &mut tree, size);
        app.tab.camera.zoom = 2.;
        let (lo, hi) = reshiki::scene::selection_bounds(&app.tab.doc, &app.tab.selected).unwrap();
        app.tab.camera.center = Point::new((lo.x + hi.x) / 2., (lo.y + hi.y) / 2.);
        pass(
            &app,
            &mut renderer,
            &mut tree,
            size,
            Some(&output.join(format!("{name}.png"))),
        );
        if display == Display::ThreeD {
            let nodes = semantics(&app, &mut renderer, size);
            let link = nodes
                .nodes
                .iter()
                .find(|node| node.id == "model-controls-open");
            assert_eq!(link.is_some(), !baseline);
            if !baseline {
                assert!(link.unwrap().visible_bounds.is_some());
                let message = {
                    let mut ui = iced_runtime::UserInterface::build(
                        app.view(),
                        size,
                        Default::default(),
                        &mut renderer,
                    );
                    let mut activate =
                        reshiki::accessibility::Activate::<Message>::new("model-controls-open");
                    ui.operate(&renderer, &mut operation::black_box(&mut activate));
                    match activate.finish() {
                        operation::Outcome::Some(message) => message,
                        _ => panic!("Visible model controls activate"),
                    }
                };
                let _ = app.update(message);
                let nodes = semantics(&app, &mut renderer, size);
                for id in ["model-style-ball-stick", "model-style-space-filling"] {
                    let node = nodes
                        .nodes
                        .iter()
                        .find(|node| node.id == id)
                        .unwrap_or_else(|| panic!("{id}"));
                    assert!(
                        node.visible_bounds.is_some(),
                        "{id} is visible after one click"
                    );
                }
                pass(
                    &app,
                    &mut renderer,
                    &mut Tree::empty(),
                    size,
                    Some(&output.join("model-controls.png")),
                );
            }
        }
    }
    let mut app = setup(&input.join("caffeine.rsk"), Display::ThreeD);
    let mut tree = Tree::empty();
    settle(&mut app, &mut renderer, &mut tree, size);
    app.tab.camera.zoom = 2.;
    let _ = app.update(Message::Commands(
        super::super::command_palette::Action::Open,
    ));
    let _ = app.update(Message::Commands(
        super::super::command_palette::Action::Query("scale 150".into()),
    ));
    let _ = app.update(Message::Commands(
        super::super::command_palette::Action::Submit,
    ));
    pass(
        &app,
        &mut renderer,
        &mut tree,
        size,
        Some(&output.join("scale-recovery.png")),
    );

    let mut app = setup(&input.join("aspirin.rsk"), Display::ThreeD);
    let mut tree = Tree::empty();
    settle(&mut app, &mut renderer, &mut tree, size);
    app.tab.camera.zoom = 2.5;
    // The same pre-command camera and viewport put the stored Both model past
    // the right edge. No document, model pose or input geometry differs.
    let mut both = app.tab.doc.clone();
    models3d::set_display(&mut both, 0, Display::Both);
    let ids = app.tab.selected.clone();
    let (lo, hi) = reshiki::scene::selection_bounds(&both, &ids).unwrap();
    app.tab.camera.center = Point::new(lo.x + 30., (lo.y + hi.y) / 2.);
    let _ = app.update(Message::Models3d(super::super::models3d::Action::Show(
        Display::Both,
    )));
    app.tab.model_morph = None;
    let paper = app.guides.paper(Rectangle::with_size(app.viewport));
    let visible = Rectangle::with_size(paper.size());
    let fits = [lo, hi]
        .iter()
        .all(|p| visible.contains(app.tab.camera.screen(*p, paper)));
    assert_eq!(fits, !baseline);
    std::fs::write(
        output.join("both-reveal-state.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "document": app.tab.doc,
            "selected": app.tab.selected,
            "viewport": [app.viewport.width, app.viewport.height],
            "camera": [app.tab.camera.center.x, app.tab.camera.center.y, app.tab.camera.zoom],
        }))
        .unwrap(),
    )
    .unwrap();
    pass(
        &app,
        &mut renderer,
        &mut tree,
        size,
        Some(&output.join("both-reveal.png")),
    );
}
