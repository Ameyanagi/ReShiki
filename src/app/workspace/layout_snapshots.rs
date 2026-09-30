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

/// The Insert ▾ menu open over typed SMILES.
fn import_menu(app: &mut App) {
    use crate::app::import::Action;
    import(app);
    let paste = text_editor::Edit::Paste(std::sync::Arc::new("CCO".into()));
    let _ = app.update(Message::Imports(Action::Edit(text_editor::Action::Edit(
        paste,
    ))));
    let _ = app.update(Message::Imports(Action::Menu(true)));
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

/// The widest inspector tab.
fn assistant(app: &mut App) {
    app.inspector_open = true;
    app.inspector_tab = InspectorTab::Assistant;
}

/// A snaking chain with changed constraints, so that Reset shows.
fn chain(app: &mut App) {
    let _ = app.update(Message::Tool(Tool::Chain(
        reshiki::chains::ChainMode::Snaking,
    )));
    app.bond_drawing.fixed_angles = false;
}

/// One selected atom moves its bonded neighbors: constraints and Reset show.
fn atom(app: &mut App) {
    open(app, benzene());
    let id = app.doc.atoms[0].id;
    let _ = app.update(Message::Canvas(Edit::Select(vec![id])));
    app.bond_drawing.fixed_angles = false;
}

/// A reaction scheme in one row with a molecule waiting below it, selected,
/// with the grid and View panel shown. Returns the drag from the molecule to
/// its place in the row, and a point on the molecule to drag by.
fn smart_guides_scheme(app: &mut App) -> (World, World) {
    let arrow = {
        let mut doc = Document::default();
        doc.arrows.push(Arrow::new(
            doc.next_id(),
            World::new(0., 0.),
            World::new(80., 0.),
            Default::default(),
            Default::default(),
        ));
        doc
    };
    let mut doc = Document::default();
    let mut place = |part: &Document, left: f32, middle: f32| {
        let (lo, hi) = reshiki::scene::selection_bounds(part, &part.all_ids()).unwrap();
        let offset = World::new(left - lo.x, middle - (lo.y + hi.y) / 2.);
        let ids = reshiki::editing::append(&mut doc, part, offset);
        (ids, left + hi.x - lo.x + 30.)
    };
    // Equal 30-unit gaps, with the middle molecule 25 right and 170 below its place.
    let (_, x) = place(&benzene(), -300., 0.);
    let (_, x) = place(&arrow, x, 0.);
    let (ids, x) = place(&benzene(), x + 25., 170.);
    let (_, x) = place(&arrow, x - 25., 0.);
    place(&benzene(), x, 0.);
    open(app, doc);
    let grab = app.doc.atom(ids[0]).unwrap().position;
    let _ = app.update(Message::Canvas(Edit::Select(ids)));
    app.grid = true;
    app.view_open = true;
    app.camera.center = World::new(-40., 70.);
    (World::new(-25., -170.), grab)
}

/// The canvas widget: the leaf the size of the drawing viewport.
fn canvas_bounds(layout: Layout<'_>, size: Size) -> Option<Rectangle> {
    if layout.bounds().size() == size && layout.children().next().is_none() {
        return Some(layout.bounds());
    }
    layout
        .children()
        .find_map(|child| canvas_bounds(child, size))
}

/// Mid-drag: the molecule moved to 3 px right of and 4 px below its place in the
/// row, where smart guides center it and even out the gaps.
fn smart_guides(renderer: &mut iced::Renderer, size: Size, output: &Path) {
    let (mut app, _) = App::new();
    app.appearance.mode = crate::appearance::Mode::Light;
    let (offset, grab) = smart_guides_scheme(&mut app);
    let mut tree = Tree::empty();
    for _ in 0..3 {
        for message in pass(&app, renderer, &mut tree, size, None) {
            let _ = app.update(message);
        }
    }
    // Sends one pointer event; returns what it publishes and where the canvas is.
    let mut send = |app: &App, event, point| {
        let mut messages = Vec::new();
        let mut view = app.view();
        tree.diff(view.as_widget());
        let node =
            view.as_widget_mut()
                .layout(&mut tree, renderer, &layout::Limits::new(size, size));
        view.as_widget_mut().update(
            &mut tree,
            &Event::Mouse(event),
            Layout::new(&node),
            mouse::Cursor::Available(point),
            renderer,
            &mut iced::advanced::clipboard::Null,
            &mut iced::advanced::Shell::new(&mut messages),
            &Rectangle::with_size(size),
        );
        let canvas = canvas_bounds(Layout::new(&node), app.viewport);
        (messages, canvas)
    };
    let (_, canvas) = send(&app, mouse::Event::CursorLeft, iced::Point::ORIGIN);
    let canvas = canvas.expect("Canvas layout");
    let camera = app.camera;
    let screen = |p: World| {
        iced::Point::new(
            canvas.x + (p.x - camera.center.x) * camera.zoom + canvas.width / 2.,
            canvas.y + (p.y - camera.center.y) * camera.zoom + canvas.height / 2.,
        )
    };
    let pixel = 1. / camera.zoom;
    let to = grab.offset(offset.x + 3. * pixel, offset.y + 4. * pixel);
    for (event, point) in [
        (
            mouse::Event::CursorMoved {
                position: screen(grab),
            },
            grab,
        ),
        (mouse::Event::ButtonPressed(mouse::Button::Left), grab),
        (
            mouse::Event::CursorMoved {
                position: screen(to),
            },
            to,
        ),
    ] {
        let (messages, _) = send(&app, event, screen(point));
        for message in messages {
            let _ = app.update(message);
        }
    }
    pass(&app, renderer, &mut tree, size, Some(output));
}

type Setup = fn(&mut App);

const STATES: [(&str, Setup); 17] = [
    ("default", |_| {}),
    ("molecule", molecule),
    ("mixed", mixed),
    ("ring-tool", ring_tool),
    ("arc", arc),
    ("import", import),
    ("import-menu", import_menu),
    ("help", |app| app.help_open = true),
    ("transform", transform),
    ("unsaved", unsaved),
    ("recovery", recovery),
    ("color-popover", color_popover),
    ("edit-hues", edit_hues),
    ("ring-tool-assistant", |app| {
        ring_tool(app);
        assistant(app);
    }),
    ("molecule-assistant", |app| {
        molecule(app);
        assistant(app);
    }),
    ("chain-assistant", |app| {
        chain(app);
        assistant(app);
    }),
    ("atom-assistant", |app| {
        atom(app);
        assistant(app);
    }),
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
        let output = directory.join(format!("smart-guides-{width}.png"));
        smart_guides(
            &mut renderer,
            Size::new(width as f32, height as f32),
            &output,
        );
        eprintln!("{}", output.display());
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

/// The Import tab's Insert ▾ menu floats like the other menus: a press outside
/// or Escape closes it, and choosing its item runs the command.
#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn the_insert_menu_closes_like_the_other_menus() {
    use iced::keyboard::{self, Key, Modifiers, key};
    let renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .unwrap();
    let size = Size::new(1040., 680.);
    let (mut app, _) = App::new();
    import_menu(&mut app);
    assert!(app.imports.menu);
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
        .expect("The menu is open");
    let mut menu = overlay::Nested::new(element);
    let node = menu.layout(&renderer, size);
    fn find(layout: Layout<'_>) -> Option<Rectangle> {
        let bounds = layout.bounds();
        if bounds.width == 170. {
            return Some(bounds);
        }
        layout.children().find_map(find)
    }
    let popup = find(Layout::new(&node)).expect("Menu layout");
    assert!(popup.x + popup.width <= size.width, "{popup:?}");
    let item = mouse::Cursor::Available(popup.center());
    let outside = mouse::Cursor::Available(iced::Point::new(popup.x - 40., popup.center_y()));
    let escape = Event::Keyboard(keyboard::Event::KeyPressed {
        key: Key::Named(key::Named::Escape),
        modified_key: Key::Named(key::Named::Escape),
        physical_key: key::Physical::Code(key::Code::Escape),
        location: keyboard::Location::Standard,
        modifiers: Modifiers::empty(),
        text: None,
        repeat: false,
    });
    let press = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
    let release = Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
    for (event, cursor, published) in [
        (&press, outside, Some("Imports(Menu(false))")),
        (&escape, item, Some("Imports(Menu(false))")),
        (&press, item, None),
        (&release, item, Some("Import")),
    ] {
        let mut messages = Vec::new();
        let mut shell = iced::advanced::Shell::new(&mut messages);
        menu.update(
            event,
            Layout::new(&node),
            cursor,
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut shell,
        );
        assert_eq!(
            shell.event_status(),
            iced::event::Status::Captured,
            "{event:?}"
        );
        let messages: Vec<_> = messages.iter().map(|m| format!("{m:?}")).collect();
        assert_eq!(messages, Vec::from_iter(published), "{event:?}");
    }
    drop(menu);
    drop(view);
    // Choosing the item closes the menu, as the style bar and Export menus do.
    let _ = app.update(Message::Import);
    assert!(!app.imports.menu);
}

/// Every tool's context row fits the canvas of a 1040 px window with any
/// inspector tab or none, including Reset and bonded-movement controls.
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
    let tabs = [
        None,
        Some(InspectorTab::Templates),
        Some(InspectorTab::Properties),
        Some(InspectorTab::DrawingStyle),
        Some(InspectorTab::Assistant),
    ];
    for tool in tools {
        for (tab, (selected, reset)) in tabs.into_iter().flat_map(|tab| {
            [(0, false), (0, true), (1, true), (2, true), (6, true)].map(|case| (tab, case))
        }) {
            let (mut app, _) = App::new();
            open(&mut app, benzene());
            app.tool = tool;
            app.selected = app.doc.all_ids().into_iter().take(selected).collect();
            app.bond_drawing.fixed_angles = !reset;
            app.inspector_open = tab.is_some();
            app.inspector_tab = tab.unwrap_or(app.inspector_tab);
            let width = 1040. - PALETTE_WIDTH - app.inspector_width() - 2. * CONTEXT_PADDING;
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
                "{tool:?} with {selected} selected, reset {reset}, {tab:?}: {natural} > {width}"
            );
        }
    }
}

/// Command keys never type into a field, Undo and Redo do nothing while one
/// is focused, and Enter applies a field and leaves it, so that ⌘Z then
/// reaches the drawing. Overlay fields, like the color popover's, too.
#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn command_keys_never_type_into_fields_and_enter_leaves_them() {
    use iced::advanced::widget::{
        Id, Operation,
        operation::{Focusable, TextInput},
    };
    use iced::keyboard::{self, Key, Modifiers, key};
    /// The bounds of the field showing `text` (any field if empty), and
    /// whether some field is focused.
    struct Find(&'static str, Option<Rectangle>, bool);
    impl Operation for Find {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
            operate(self);
        }
        fn text_input(&mut self, _: Option<&Id>, bounds: Rectangle, state: &mut dyn TextInput) {
            if self.1.is_none() && (self.0.is_empty() || state.text() == self.0) {
                self.1 = Some(bounds);
            }
        }
        fn focusable(&mut self, _: Option<&Id>, _: Rectangle, state: &mut dyn Focusable) {
            self.2 |= state.is_focused();
        }
    }
    let renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .unwrap();
    let size = Size::new(1280., 820.);
    let viewport = Rectangle::with_size(size);
    let command = if cfg!(target_os = "macos") {
        Modifiers::LOGO
    } else {
        Modifiers::CTRL
    };
    let press = |key: Key, code, modifiers, text: Option<&str>| {
        Event::Keyboard(keyboard::Event::KeyPressed {
            modified_key: key.clone(),
            key,
            physical_key: key::Physical::Code(code),
            location: keyboard::Location::Standard,
            modifiers,
            text: text.map(Into::into),
            repeat: false,
        })
    };
    // macOS reports the letter as text for ⌘Z.
    let undo = press(
        Key::Character("z".into()),
        key::Code::KeyZ,
        command,
        Some("z"),
    );
    let enter = press(
        Key::Named(key::Named::Enter),
        key::Code::Enter,
        Modifiers::empty(),
        None,
    );
    let click = |at: Rectangle| {
        let cursor = mouse::Cursor::Available(at.center());
        [
            (
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                cursor,
            ),
            (
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                cursor,
            ),
        ]
    };
    let modifiers = |modifiers| {
        (
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)),
            mouse::Cursor::Unavailable,
        )
    };
    // Sends events to the view, or its overlay, and applies the messages;
    // returns them with the last event's status and whether a field is focused.
    fn send(
        app: &mut App,
        renderer: &iced::Renderer,
        tree: &mut Tree,
        viewport: Rectangle,
        overlay_only: bool,
        events: &[(Event, mouse::Cursor)],
    ) -> (Vec<String>, iced::event::Status, bool) {
        let mut messages = Vec::new();
        let mut status = iced::event::Status::Ignored;
        let mut view = app.view();
        tree.diff(view.as_widget());
        let node = view.as_widget_mut().layout(
            tree,
            renderer,
            &layout::Limits::new(viewport.size(), viewport.size()),
        );
        for (event, cursor) in events {
            let mut shell = iced::advanced::Shell::new(&mut messages);
            if overlay_only {
                let element = view
                    .as_widget_mut()
                    .overlay(tree, Layout::new(&node), renderer, &viewport, Vector::ZERO)
                    .expect("An overlay");
                let mut nested = overlay::Nested::new(element);
                let node = nested.layout(renderer, viewport.size());
                nested.update(
                    event,
                    Layout::new(&node),
                    *cursor,
                    renderer,
                    &mut iced::advanced::clipboard::Null,
                    &mut shell,
                );
            } else {
                view.as_widget_mut().update(
                    tree,
                    event,
                    Layout::new(&node),
                    *cursor,
                    renderer,
                    &mut iced::advanced::clipboard::Null,
                    &mut shell,
                    &viewport,
                );
            }
            status = shell.event_status();
        }
        let mut find = Find("", None, false);
        if overlay_only {
            let element = view
                .as_widget_mut()
                .overlay(tree, Layout::new(&node), renderer, &viewport, Vector::ZERO)
                .expect("An overlay");
            let mut nested = overlay::Nested::new(element);
            let node = nested.layout(renderer, viewport.size());
            nested.operate(Layout::new(&node), renderer, &mut find);
        } else {
            view.as_widget_mut()
                .operate(tree, Layout::new(&node), renderer, &mut find);
        }
        drop(view);
        let names = messages.iter().map(|m| format!("{m:?}")).collect();
        for message in messages {
            let _ = app.update(message);
        }
        (names, status, find.2)
    }
    // Locates a field by its text in the view or its overlay.
    fn locate(
        app: &App,
        renderer: &iced::Renderer,
        tree: &mut Tree,
        viewport: Rectangle,
        overlay_only: bool,
        text: &'static str,
    ) -> Rectangle {
        let mut view = app.view();
        tree.diff(view.as_widget());
        let node = view.as_widget_mut().layout(
            tree,
            renderer,
            &layout::Limits::new(viewport.size(), viewport.size()),
        );
        let mut find = Find(text, None, false);
        if overlay_only {
            let element = view
                .as_widget_mut()
                .overlay(tree, Layout::new(&node), renderer, &viewport, Vector::ZERO)
                .expect("An overlay");
            let mut nested = overlay::Nested::new(element);
            let node = nested.layout(renderer, viewport.size());
            nested.operate(Layout::new(&node), renderer, &mut find);
        } else {
            view.as_widget_mut()
                .operate(tree, Layout::new(&node), renderer, &mut find);
        }
        find.1.expect("The field")
    }

    // The Transform panel's Rotate field.
    let (mut app, _) = App::new();
    transform(&mut app);
    let _ = app.update(Message::NumericTransform(
        crate::app::numeric_transforms::Action::Input(
            crate::app::numeric_transforms::Field::Rotation,
            "72".into(),
        ),
    ));
    let before = app.doc.clone();
    let mut tree = Tree::empty();
    let field = locate(&app, &renderer, &mut tree, viewport, false, "72");
    let (_, _, focused) = send(
        &mut app,
        &renderer,
        &mut tree,
        viewport,
        false,
        &click(field),
    );
    assert!(focused);
    let (messages, status, focused) = send(
        &mut app,
        &renderer,
        &mut tree,
        viewport,
        false,
        &[
            modifiers(command),
            (undo.clone(), mouse::Cursor::Unavailable),
        ],
    );
    assert!(messages.is_empty(), "{messages:?}");
    assert_eq!(status, iced::event::Status::Captured, "Undo does nothing");
    assert!(focused);
    let (messages, _, focused) = send(
        &mut app,
        &renderer,
        &mut tree,
        viewport,
        false,
        &[
            modifiers(Modifiers::empty()),
            (enter.clone(), mouse::Cursor::Unavailable),
        ],
    );
    assert_eq!(messages.len(), 1, "{messages:?}");
    assert!(messages[0].contains("Apply(Rotation)"), "{messages:?}");
    assert!(!focused, "Enter leaves the field");
    assert_ne!(app.doc, before);
    let (messages, status, _) = send(
        &mut app,
        &renderer,
        &mut tree,
        viewport,
        false,
        &[
            modifiers(command),
            (undo.clone(), mouse::Cursor::Unavailable),
        ],
    );
    assert!(messages.is_empty(), "{messages:?}");
    assert_eq!(
        status,
        iced::event::Status::Ignored,
        "⌘Z reaches the drawing's Undo"
    );

    // The color popover's Color field, in an overlay.
    let (mut app, _) = App::new();
    color_popover(&mut app);
    let mut tree = Tree::empty();
    let field = locate(&app, &renderer, &mut tree, viewport, true, "");
    let (_, _, focused) = send(
        &mut app,
        &renderer,
        &mut tree,
        viewport,
        true,
        &click(field),
    );
    assert!(focused);
    let (messages, status, _) = send(
        &mut app,
        &renderer,
        &mut tree,
        viewport,
        true,
        &[modifiers(command), (undo, mouse::Cursor::Unavailable)],
    );
    assert!(messages.is_empty(), "{messages:?}");
    assert_eq!(status, iced::event::Status::Captured);
    let (messages, _, focused) = send(
        &mut app,
        &renderer,
        &mut tree,
        viewport,
        true,
        &[
            modifiers(Modifiers::empty()),
            (enter, mouse::Cursor::Unavailable),
        ],
    );
    assert!(
        messages.iter().any(|m| m.contains("ApplyTextColor")),
        "{messages:?}"
    );
    assert!(!focused);
}
