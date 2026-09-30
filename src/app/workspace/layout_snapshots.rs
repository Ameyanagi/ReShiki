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
    let redraw = Event::Window(iced::window::Event::RedrawRequested(
        std::time::Instant::now(),
    ));
    view.as_widget_mut().update(
        tree,
        &redraw,
        Layout::new(&node),
        mouse::Cursor::Unavailable,
        renderer,
        &mut iced::advanced::clipboard::Null,
        &mut iced::advanced::Shell::new(&mut messages),
        &viewport,
    );
    // Overlay widgets, such as popover buttons, settle their status here too.
    if let Some(element) =
        view.as_widget_mut()
            .overlay(tree, Layout::new(&node), renderer, &viewport, Vector::ZERO)
    {
        let mut nested = overlay::Nested::new(element);
        let node = nested.layout(renderer, size);
        nested.update(
            &redraw,
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            renderer,
            &mut iced::advanced::clipboard::Null,
            &mut iced::advanced::Shell::new(&mut messages),
        );
    }
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
    let _ = app.update(Message::Inspector(InspectorTab::Import));
}

/// The Transform section with tilt shown under More.
fn transform(app: &mut App) {
    use crate::app::inspector::{Action, Section};
    molecule(app);
    for (section, expanded) in [(Section::Bonds, false), (Section::Transform, true)] {
        let _ = app.update(Message::InspectorAction(Action::Section(section, expanded)));
    }
    let _ = app.update(Message::NumericTransform(
        crate::app::numeric_transforms::Action::More(true),
    ));
}

/// New on an edited drawing waits for the native save dialog, which the
/// headless renderer cannot show; nothing may appear above the canvas.
fn unsaved(app: &mut App) {
    app.doc = benzene();
    let _ = app.update(Message::New);
    assert!(app.pending.is_some());
}

/// A launch that found drafts from a session that closed unexpectedly.
fn recovery(app: &mut App) {
    use reshiki::recovery::{Candidate, Snapshot};
    let snapshot = Snapshot {
        document: benzene(),
        source: None,
        saved_at: 0,
    };
    let path = std::path::PathBuf::from("draft.json");
    app.recovered = vec![Candidate { path, snapshot }];
    // As at launch, where the offer replaces the ready message.
    app.status.clear();
}

/// The color popover for a selection in a custom color that is faint on the
/// light canvas, with two recent custom colors.
fn color_popover(app: &mut App) {
    use reshiki::palette::Color as Paint;
    mixed(app);
    app.doc.remember_color([31, 78, 121]);
    let _ = app.update(Message::TextStyle(StyleChange::Color(Paint::Custom([
        232, 119, 34,
    ]))));
    let _ = app.update(Message::StyleMenu(
        super::super::color_popover::Action::Color,
    ));
}

/// Edit hues with Blue moved from 255° to 225°, recoloring the drawing.
fn edit_hues(app: &mut App) {
    use super::super::color_popover::Action;
    use reshiki::palette::{Color as Paint, Hue, Row};
    mixed(app);
    let _ = app.update(Message::TextStyle(StyleChange::Color(Paint::Palette(
        Hue::Blue,
        Row::Strong,
    ))));
    for action in [Action::Color, Action::EditHues, Action::Chip(-3)] {
        let _ = app.update(Message::StyleMenu(action));
    }
}

type Setup = fn(&mut App);

const STATES: [(&str, Setup); 11] = [
    ("default", |_| {}),
    ("molecule", molecule),
    ("mixed", mixed),
    ("ring-tool", ring_tool),
    ("arc", arc),
    ("import", import),
    ("transform", transform),
    ("unsaved", unsaved),
    ("recovery", recovery),
    ("color-popover", color_popover),
    ("edit-hues", edit_hues),
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

/// The color popover over the canvas swallows presses and scrolling on its
/// bare areas (the canvas tracks the pointer itself), and a press outside only
/// closes it.
#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn the_color_popover_keeps_pointer_input_from_the_canvas() {
    let renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .unwrap();
    let size = Size::new(1280., 820.);
    let (mut app, _) = App::new();
    color_popover(&mut app);
    let mut view = app.view();
    let mut tree = Tree::new(view.as_widget());
    let node = view
        .as_widget_mut()
        .layout(&mut tree, &renderer, &layout::Limits::new(size, size));
    let element = view
        .as_widget_mut()
        .overlay(
            &mut tree,
            Layout::new(&node),
            &renderer,
            &Rectangle::with_size(size),
            Vector::ZERO,
        )
        .expect("The popover is open");
    let mut popover = overlay::Nested::new(element);
    let node = popover.layout(&renderer, size);
    // The view's overlays form a group; find the popup by its width.
    fn find(layout: Layout<'_>) -> Option<Rectangle> {
        let bounds = layout.bounds();
        if bounds.width == 336. {
            return Some(bounds);
        }
        layout.children().find_map(find)
    }
    let popup = find(Layout::new(&node)).expect("Popup layout");
    // The bottom-left padding corner holds no widget.
    let bare = iced::Point::new(popup.x + 4., popup.y + popup.height - 4.);
    let outside = iced::Point::new(popup.x - 40., popup.y + popup.height - 4.);
    for (point, event, published) in [
        (
            bare,
            mouse::Event::ButtonPressed(mouse::Button::Left),
            vec![],
        ),
        (
            bare,
            mouse::Event::ButtonReleased(mouse::Button::Left),
            vec![],
        ),
        (
            bare,
            mouse::Event::WheelScrolled {
                delta: mouse::ScrollDelta::Lines { x: 0., y: 1. },
            },
            vec![],
        ),
        (
            outside,
            mouse::Event::ButtonPressed(mouse::Button::Left),
            vec!["StyleMenu(Close)"],
        ),
    ] {
        let mut messages = Vec::new();
        let mut shell = iced::advanced::Shell::new(&mut messages);
        popover.update(
            &Event::Mouse(event),
            Layout::new(&node),
            mouse::Cursor::Available(point),
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut shell,
        );
        assert_eq!(
            shell.event_status(),
            iced::event::Status::Captured,
            "{event:?}"
        );
        let published: Vec<_> = published.into_iter().map(String::from).collect();
        assert_eq!(
            messages
                .iter()
                .map(|m| format!("{m:?}"))
                .collect::<Vec<_>>(),
            published,
            "{event:?}"
        );
    }
}

/// Every tool's context row fits the canvas of a 1040 px window with the
/// default inspector, including Reset and bonded-movement controls.
#[tokio::test]
#[ignore = "Opt-in renderer layout check"]
async fn every_context_row_fits_the_minimum_window() {
    use reshiki::{bonds::BondPreset, chains::ChainMode, rings::Preset, scientific};
    let renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .unwrap();
    let mut tools = vec![
        Tool::Select,
        Tool::Lasso,
        Tool::Tilt,
        Tool::Wedge,
        Tool::Hash,
        Tool::Wavy,
        Tool::Atom,
        Tool::Ring,
        Tool::Template,
        Tool::Arrow,
        Tool::Text,
        Tool::Erase,
        Tool::EditPoints,
        Tool::Graphic(GraphicKind::Symbol(scientific::SymbolKind::ALL[0])),
        Tool::Graphic(GraphicKind::Orbital(scientific::OrbitalKind::ALL[0])),
    ];
    tools.extend([ChainMode::Straight, ChainMode::Snaking].map(Tool::Chain));
    tools.extend((1..=3).map(Tool::Bond));
    tools.extend(BondPreset::ALL.map(Tool::StyledBond));
    tools.extend(Preset::ALL.iter().map(|&p| Tool::RingPreset(p)));
    tools.extend(GraphicKind::DRAWABLE.map(Tool::Graphic));
    let width = 636. - 2. * CONTEXT_PADDING;
    for tool in tools {
        for (selected, reset) in [(0, false), (0, true), (1, true), (6, true)] {
            let (mut app, _) = App::new();
            open(&mut app, benzene());
            app.tool = tool;
            app.selected = app.doc.all_ids().into_iter().take(selected).collect();
            app.bond_drawing.fixed_angles = !reset;
            // Lay out without a width limit; the arrange group sits in a
            // filling container, so count the group itself.
            let mut row = app.context_row(width);
            let mut tree = Tree::new(row.as_widget());
            let node = row.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(Size::ZERO, Size::new(10_000., 36.)),
            );
            let children = node.children();
            let arrange = tool.selects() && app.appearance.arrange_controls;
            let natural = children
                .iter()
                .enumerate()
                .map(|(i, child)| match child.children() {
                    [group] if arrange && i + 1 == children.len() => group.bounds().width,
                    _ => child.bounds().width,
                })
                .sum::<f32>()
                + CONTEXT_GAP * (children.len() - 1) as f32;
            assert!(
                natural <= width,
                "{tool:?} with {selected} selected, reset {reset}: {natural} > {width}"
            );
        }
    }
}
