//! Opt-in headless snapshots of key UI states at the default and minimum window sizes.
use super::*;
use iced::advanced::Renderer as _;
use iced::advanced::{Layout, layout, mouse, overlay, renderer, renderer::Headless, widget::Tree};
use iced::{Event, Rectangle, Size, Vector};
use reshiki::document::{Arrow, Document, Point as World};
use reshiki::graphics::{Graphic, GraphicKind};
use std::path::Path;

/// Lays out and updates the view once, optionally saving a screenshot; returns published messages.
fn pass(
    app: &App,
    renderer: &mut iced::Renderer,
    tree: &mut Tree,
    size: Size,
    output: Option<&Path>,
) -> Vec<Message> {
    let mut messages = Vec::new();
    let viewport = Rectangle::with_size(size);
    let theme = app.theme();
    let mut view = app.view();
    tree.diff(view.as_widget());
    let node = view
        .as_widget_mut()
        .layout(tree, renderer, &layout::Limits::new(size, size));
    view.as_widget_mut().update(
        tree,
        &Event::Window(iced::window::Event::RedrawRequested(
            std::time::Instant::now(),
        )),
        Layout::new(&node),
        mouse::Cursor::Unavailable,
        renderer,
        &mut iced::advanced::clipboard::Null,
        &mut iced::advanced::Shell::new(&mut messages),
        &viewport,
    );
    let Some(output) = output else {
        return messages;
    };
    let style = renderer::Style {
        text_color: theme.palette().text,
    };
    renderer.reset(viewport);
    view.as_widget().draw(
        tree,
        renderer,
        &theme,
        &style,
        Layout::new(&node),
        mouse::Cursor::Unavailable,
        &viewport,
    );
    if let Some(element) =
        view.as_widget_mut()
            .overlay(tree, Layout::new(&node), renderer, &viewport, Vector::ZERO)
    {
        let mut nested = overlay::Nested::new(element);
        let node = nested.layout(renderer, size);
        nested.draw(
            renderer,
            &theme,
            &style,
            Layout::new(&node),
            mouse::Cursor::Unavailable,
        );
    }
    let (width, height) = (size.width as u32, size.height as u32);
    let pixels = Headless::screenshot(
        renderer,
        Size::new(width, height),
        1.,
        theme.palette().background,
    );
    image::save_buffer(output, &pixels, width, height, image::ColorType::Rgba8).unwrap();
    messages
}

/// Lets the view settle (canvas viewport etc.), then saves the screenshot.
fn snapshot(app: &mut App, renderer: &mut iced::Renderer, size: Size, output: &Path) {
    let mut tree = Tree::empty();
    for _ in 0..3 {
        let messages = pass(app, renderer, &mut tree, size, None);
        if messages.is_empty() {
            break;
        }
        for message in messages {
            let _ = app.update(message);
        }
    }
    pass(app, renderer, &mut tree, size, Some(output));
}

fn benzene() -> Document {
    reshiki::rings::Preset::Benzene.document(42., false)
}

fn open(app: &mut App, doc: Document) {
    app.doc = doc;
    app.saved = app.doc.clone();
}

fn select_all(app: &mut App) {
    let _ = app.update(Message::Canvas(Edit::Select(app.doc.all_ids())));
}

fn molecule(app: &mut App) {
    open(app, benzene());
    select_all(app);
}

fn mixed(app: &mut App) {
    let mut doc = benzene();
    doc.arrows.push(Arrow::new(
        doc.next_id(),
        World::new(70., 0.),
        World::new(160., 0.),
        Default::default(),
        Default::default(),
    ));
    doc.graphics.push(Graphic::dragged(
        doc.next_id(),
        GraphicKind::Rectangle,
        World::new(190., -40.),
        World::new(270., 40.),
        Default::default(),
        Default::default(),
        false,
    ));
    open(app, doc);
    app.camera.center = World::new(110., 0.);
    select_all(app);
}

fn ring_tool(app: &mut App) {
    let _ = app.update(Message::Tool(Tool::Ring));
}

fn arc(app: &mut App) {
    let _ = app.update(Message::Tool(Tool::Graphic(GraphicKind::Arc)));
    let _ = app.update(Message::Canvas(Edit::Graphic(
        World::new(-80., -50.),
        World::new(80., 50.),
        false,
    )));
}

fn import(app: &mut App) {
    let _ = app.update(Message::ToggleImport);
}

fn transform(app: &mut App) {
    use crate::app::inspector::{Action, Section};
    molecule(app);
    for (section, expanded) in [(Section::Bonds, false), (Section::Arrange, true)] {
        let _ = app.update(Message::InspectorAction(Action::Section(section, expanded)));
    }
}

fn unsaved(app: &mut App) {
    app.doc = benzene();
    let _ = app.update(Message::New);
}

type Setup = fn(&mut App);

const STATES: [(&str, Setup); 8] = [
    ("default", |_| {}),
    ("molecule", molecule),
    ("mixed", mixed),
    ("ring-tool", ring_tool),
    ("arc", arc),
    ("import", import),
    ("transform", transform),
    ("unsaved", unsaved),
];

#[tokio::test]
#[ignore = "Opt-in GPU snapshots of key UI states; set RESHIKI_UI_QA_DIR"]
async fn ui_layout_snapshots() {
    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .unwrap();
    let directory = std::env::var_os("RESHIKI_UI_QA_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("reshiki-ui-qa"));
    std::fs::create_dir_all(&directory).unwrap();
    for (width, height) in [(1280, 820), (1040, 680)] {
        for (name, setup) in STATES {
            let (mut app, _) = App::new();
            app.appearance.mode = crate::appearance::Mode::Light;
            // Arrange commands visible (issue #78 compares the object toolbar).
            app.appearance.object_toolbar = true;
            setup(&mut app);
            let output = directory.join(format!("{name}-{width}.png"));
            snapshot(
                &mut app,
                &mut renderer,
                Size::new(width as f32, height as f32),
                &output,
            );
            eprintln!("{}", output.display());
        }
    }
}
