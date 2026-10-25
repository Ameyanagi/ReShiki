//! Opt-in checks use the real application view, renderer, menus and pointer dispatch.
use super::*;
use iced::advanced::{
    Layout, layout,
    renderer::Headless,
    widget::{Id, Operation, Tree, operation},
};
use iced::{Event, Point, Rectangle, Size, mouse};
use iced_runtime::{UserInterface, user_interface::Cache};
use reshiki::accessibility::{Collect, Snapshot};
use std::path::Path;

struct Ui {
    renderer: iced::Renderer,
    cache: Cache,
    size: Size,
}
#[derive(Default)]
struct Bounds(Vec<(String, Rectangle)>);
impl Operation for Bounds {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }
    fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
        for name in ["atom-text-dialog", "atom-text-mode", "atom-text-alignment"] {
            if id == Some(&Id::from(name)) {
                self.0.push((name.into(), bounds));
            }
        }
    }
}
impl Ui {
    async fn new(size: Size) -> Self {
        Self {
            renderer: <iced::Renderer as Headless>::new(
                iced::Font::with_name(reshiki::style::ui_font_family()),
                iced::Pixels(16.),
                None,
            )
            .await
            .unwrap(),
            cache: Cache::new(),
            size,
        }
    }
    fn frame(&mut self, app: &mut App, event: Event, point: Option<Point>, output: Option<&Path>) {
        let mut ui = UserInterface::build(
            app.view(),
            self.size,
            std::mem::take(&mut self.cache),
            &mut self.renderer,
        );
        let cursor = point.map_or(mouse::Cursor::Unavailable, mouse::Cursor::Available);
        let mut messages = vec![];
        ui.update(
            &[event],
            cursor,
            &mut self.renderer,
            &mut iced::advanced::clipboard::Null,
            &mut messages,
        );
        if let Some(output) = output {
            iced::advanced::Renderer::reset(&mut self.renderer, Rectangle::with_size(self.size));
            ui.draw(
                &mut self.renderer,
                &app.theme(),
                &iced::advanced::renderer::Style {
                    text_color: app.theme().palette().text,
                },
                cursor,
            );
            let pixels = Headless::screenshot(
                &mut self.renderer,
                Size::new(self.size.width as u32, self.size.height as u32),
                1.,
                app.theme().palette().background,
            );
            image::save_buffer(
                output,
                &pixels,
                self.size.width as u32,
                self.size.height as u32,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
        self.cache = ui.into_cache();
        for message in messages {
            let _ = app.update(message);
        }
    }
    fn settle(&mut self, app: &mut App, pointer: Option<Point>) {
        for _ in 0..3 {
            self.frame(
                app,
                Event::Window(iced::window::Event::RedrawRequested(
                    std::time::Instant::now(),
                )),
                pointer,
                None,
            );
        }
    }
    fn capture(&mut self, app: &mut App, directory: &Path, name: &str, pointer: Option<Point>) {
        self.frame(
            app,
            Event::Window(iced::window::Event::RedrawRequested(
                std::time::Instant::now(),
            )),
            pointer,
            Some(&directory.join(format!("{name}.png"))),
        );
    }
    fn inspect(&mut self, app: &App) -> (Snapshot, Bounds) {
        let mut ui = UserInterface::build(
            app.view(),
            self.size,
            std::mem::take(&mut self.cache),
            &mut self.renderer,
        );
        let mut collect = Collect::new(Rectangle::with_size(self.size));
        ui.operate(&self.renderer, &mut operation::black_box(&mut collect));
        let mut bounds = Bounds::default();
        ui.operate(&self.renderer, &mut bounds);
        self.cache = ui.into_cache();
        (collect.snapshot().clone(), bounds)
    }
    fn click(&mut self, app: &mut App, point: Point) {
        for event in [
            mouse::Event::CursorMoved { position: point },
            mouse::Event::ButtonPressed(mouse::Button::Left),
            mouse::Event::ButtonReleased(mouse::Button::Left),
        ] {
            self.frame(app, Event::Mouse(event), Some(point), None);
        }
    }
    fn canvas(&mut self, app: &App) -> Rectangle {
        fn find(layout: Layout<'_>, size: Size) -> Option<Rectangle> {
            if layout.bounds().size() == size && layout.children().next().is_none() {
                return Some(layout.bounds());
            }
            layout.children().find_map(|child| find(child, size))
        }
        let mut view = app.view();
        let mut tree = Tree::new(view.as_widget());
        let node = view.as_widget_mut().layout(
            &mut tree,
            &self.renderer,
            &layout::Limits::new(self.size, self.size),
        );
        find(Layout::new(&node), app.viewport).expect("actual canvas layout")
    }
}
fn new_app() -> (App, u64) {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.grid = false;
    app.status = "Ready".into();
    app.appearance.mode = crate::appearance::Mode::Light;
    app.tab.camera.zoom = 2.;
    let other = app
        .tab
        .doc
        .add_atom("C", reshiki::document::Point::new(-42., 0.));
    let id = app
        .tab
        .doc
        .add_atom("C", reshiki::document::Point::default());
    app.tab.doc.add_bond(other, id, 1, "plain");
    (app, id)
}
fn node<'a>(snapshot: &'a Snapshot, id: &str) -> &'a reshiki::accessibility::Node {
    snapshot
        .nodes
        .iter()
        .find(|node| node.id == id)
        .unwrap_or_else(|| panic!("Missing {id}"))
}
fn bounds(bounds: &Bounds, id: &str) -> Rectangle {
    bounds
        .0
        .iter()
        .find(|(name, _)| name == id)
        .unwrap_or_else(|| panic!("Missing {id}"))
        .1
}
fn begin(app: &mut App, id: u64, input: &str) {
    app.tab.selected = vec![id];
    let _ = app.update(Message::AtomText(Action::Begin(Some(id))));
    let _ = app.update(Message::AtomText(Action::Input(input.into())));
}

#[tokio::test]
#[ignore = "Opt-in application renderer, actual menu/pointer dispatch and matched evidence"]
async fn atom_label_editor_layout_and_interaction_evidence() {
    let directory = std::env::var_os("RESHIKI_ATOM_LABEL_QA_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("reshiki-atom-label-qa"));
    std::fs::create_dir_all(&directory).unwrap();
    let baseline = std::env::var_os("RESHIKI_ATOM_LABEL_QA_BASELINE").is_some();
    let prefix = if baseline { "before" } else { "after" };
    let mut ui = Ui::new(Size::new(1040., 680.)).await;
    // Same literal input and operation in each build, including the actual open menu.
    for dark in [false, true] {
        let (mut app, id) = new_app();
        if dark {
            app.appearance.mode = crate::appearance::Mode::Dark;
        }
        ui.cache = Cache::new();
        ui.settle(&mut app, None);
        begin(&mut app, id, "MeO");
        ui.settle(&mut app, None);
        let (snapshot, geometry) = ui.inspect(&app);
        let control = if baseline {
            let editor = node(&snapshot, "atom-text").bounds;
            Point::new(
                editor.x + 40.,
                editor.y + editor.height + 12. + 28. + 12. + 15. + 17.,
            )
        } else {
            let control = bounds(&geometry, "atom-text-mode");
            for mode in [Mode::Auto, Mode::Text, Mode::Group] {
                assert!(
                    super::super::workspace::text_width(&mode.to_string(), 16.) + 42.
                        < control.width,
                    "{mode} fits fully in {control:?}"
                );
            }
            let alignment = bounds(&geometry, "atom-text-alignment");
            for value in LabelAlignment::ALL {
                assert!(
                    super::super::workspace::text_width(&value.to_string(), 16.) + 42.
                        < alignment.width
                );
            }
            Point::new(control.center_x(), control.y + control.height - 15.)
        };
        ui.click(&mut app, control);
        ui.capture(
            &mut app,
            &directory,
            &format!("{prefix}-choices-{}", if dark { "dark" } else { "light" }),
            None,
        );
        let _ = app.update(Message::AtomText(Action::Cancel));
    }
    let (mut app, id) = new_app();
    ui.cache = Cache::new();
    ui.settle(&mut app, None);
    for atom in &mut app.tab.doc.atoms {
        atom.position.x = -atom.position.x;
    }
    app.tab.doc = reshiki::atom_text::apply(&app.tab.doc, id, "OMe", Mode::Auto).unwrap();
    begin(&mut app, id, "OMe");
    ui.settle(&mut app, None);
    ui.capture(
        &mut app,
        &directory,
        &format!("{prefix}-group-captions"),
        None,
    );
    let group_origin = if baseline {
        None
    } else {
        Some(bounds(&ui.inspect(&app).1, "atom-text-dialog").position())
    };
    let _ = app.update(Message::AtomText(Action::Mode(Mode::Text)));
    ui.settle(&mut app, None);
    let (snapshot, _) = ui.inspect(&app);
    if !baseline {
        assert!(
            !snapshot
                .nodes
                .iter()
                .any(|node| node.id == "atom-text-reverse")
        );
        assert_eq!(
            bounds(&ui.inspect(&app).1, "atom-text-dialog").position(),
            group_origin.unwrap()
        );
    }
    ui.capture(
        &mut app,
        &directory,
        &format!("{prefix}-text-caption"),
        None,
    );
    let _ = app.update(Message::AtomText(Action::Mode(Mode::Group)));
    ui.settle(&mut app, None);
    if !baseline {
        assert_eq!(
            bounds(&ui.inspect(&app).1, "atom-text-dialog").position(),
            group_origin.unwrap()
        );
    }
    assert_eq!(
        node(&ui.inspect(&app).0, "atom-text-reverse")
            .value
            .as_deref(),
        Some("MeO")
    );
    let _ = app.update(Message::AtomText(Action::Cancel));
    let (mut app, id) = new_app();
    ui.cache = Cache::new();
    ui.settle(&mut app, None);
    begin(&mut app, id, "morpholino");
    ui.settle(&mut app, None);
    ui.capture(
        &mut app,
        &directory,
        &format!("{prefix}-morpholino-suggestion"),
        None,
    );
    if !baseline {
        let suggestion_origin = bounds(&ui.inspect(&app).1, "atom-text-dialog").position();
        let point = node(&ui.inspect(&app).0, "atom-text-suggest-Morph")
            .bounds
            .center();
        ui.click(&mut app, point);
        assert_eq!(app.tab.atom_text.as_ref().unwrap().input, "Morph");
        assert_eq!(app.tab.atom_text.as_ref().unwrap().mode, Mode::Group);
        ui.settle(&mut app, None);
        assert_eq!(
            bounds(&ui.inspect(&app).1, "atom-text-dialog").position(),
            suggestion_origin
        );
        ui.capture(&mut app, &directory, "after-suggestion-accepted", None);
    }
    ui.size = Size::new(1600., 1000.);
    for fixture in ["glucose-c3", "p-xylene-end"] {
        let (mut app, _) = new_app();
        let id = if fixture == "glucose-c3" {
            app.tab.doc = reshiki::haworth::sugar_document(
                reshiki::haworth::Sugar::Glucose,
                reshiki::haworth::Anomer::Alpha,
                42.,
            )
            .unwrap();
            app.tab.doc.expand_abbreviations(&app.tab.doc.all_ids());
            9
        } else {
            app.tab.doc = reshiki::rings::Preset::Benzene.document(42., false);
            let left = app
                .tab
                .doc
                .atoms
                .iter()
                .min_by(|a, b| a.position.x.total_cmp(&b.position.x))
                .unwrap()
                .clone();
            let right = app
                .tab
                .doc
                .atoms
                .iter()
                .max_by(|a, b| a.position.x.total_cmp(&b.position.x))
                .unwrap()
                .clone();
            let left_end = app.tab.doc.add_atom("C", left.position.offset(-42., 0.));
            let right_end = app.tab.doc.add_atom("C", right.position.offset(42., 0.));
            app.tab.doc.add_bond(left.id, left_end, 1, "plain");
            app.tab.doc.add_bond(right.id, right_end, 1, "plain");
            app.tab.doc =
                reshiki::atom_text::apply(&app.tab.doc, left_end, "MeO", Mode::Auto).unwrap();
            right_end
        };
        app.tab.camera.center = app.tab.doc.atom(id).unwrap().position;
        ui.cache = Cache::new();
        ui.settle(&mut app, None);
        let paper = app.guides.paper(ui.canvas(&app));
        let target = app
            .tab
            .camera
            .screen(app.tab.doc.atom(id).unwrap().position, paper)
            + iced::Vector::new(paper.x, paper.y);
        std::fs::write(
            directory.join(format!("{fixture}.rsk")),
            serde_json::to_vec_pretty(&app.tab.doc).unwrap(),
        )
        .unwrap();
        begin(
            &mut app,
            id,
            if fixture == "glucose-c3" {
                "OBn"
            } else {
                "O2N"
            },
        );
        ui.settle(&mut app, Some(target));
        ui.capture(
            &mut app,
            &directory,
            &format!("{prefix}-{fixture}"),
            Some(target),
        );
        if !baseline {
            assert!(!bounds(&ui.inspect(&app).1, "atom-text-dialog").contains(target));
        }
    }
    // The target and pointer remain outside the measured popup at centre and
    // every corner, with actual workspace offsets, rulers, inspector and sizing.
    if !baseline {
        for size in [
            Size::new(1040., 680.),
            Size::new(1280., 820.),
            Size::new(1600., 1000.),
        ] {
            ui.size = size;
            for inspector in [false, true] {
                for tool in [crate::canvas::Tool::Select, crate::canvas::Tool::Text] {
                    for fraction in [
                        (0.5, 0.5),
                        (0.03, 0.03),
                        (0.97, 0.03),
                        (0.03, 0.97),
                        (0.97, 0.97),
                    ] {
                        let (mut app, id) = new_app();
                        app.inspector_open = inspector;
                        app.tool = tool;
                        app.guides.rulers = true;
                        ui.cache = Cache::new();
                        ui.settle(&mut app, None);
                        let paper = app.guides.paper(ui.canvas(&app));
                        let target = Point::new(
                            paper.x + paper.width * fraction.0,
                            paper.y + paper.height * fraction.1,
                        );
                        app.tab.camera.center = reshiki::document::Point::new(
                            (paper.width / 2. - (target.x - paper.x)) / app.tab.camera.zoom,
                            (paper.height / 2. - (target.y - paper.y)) / app.tab.camera.zoom,
                        );
                        begin(&mut app, id, "OBn");
                        ui.settle(&mut app, Some(target));
                        let (snapshot, geometry) = ui.inspect(&app);
                        let popup = bounds(&geometry, "atom-text-dialog");
                        assert!(
                            !popup.contains(target),
                            "target covered: {size:?} {inspector} {tool:?} {fraction:?}: {target:?} {popup:?}"
                        );
                        for id in ["atom-text", "atom-text-apply", "atom-text-cancel"] {
                            let control = node(&snapshot, id);
                            let visible = control.visible_bounds.expect("visible control");
                            assert!(
                                (visible.width - control.bounds.width).abs() < 0.01
                                    && (visible.height - control.bounds.height).abs() < 0.01,
                                "{id} fully visible"
                            );
                        }
                        assert!(
                            popup.x >= 0.
                                && popup.y >= 0.
                                && popup.x + popup.width <= size.width
                                && popup.y + popup.height <= size.height
                        );
                    }
                }
            }
        }
    }
}
