//! Focused native-renderer and interaction evidence for drawing polish.
use super::*;
use iced::advanced::{
    Clipboard, Layout, Shell, layout, mouse, overlay, renderer,
    renderer::Headless,
    widget::{Operation, Tree, operation},
};
use iced::{Event, Rectangle, Size, Vector};
use std::path::{Path, PathBuf};

fn directory() -> PathBuf {
    let path = std::env::var_os("RESHIKI_DRAWING_QA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("reshiki-drawing-polish"));
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn baseline() -> bool {
    std::env::var_os("RESHIKI_DRAWING_QA_BASELINE").is_some()
}

struct Paste(String);
impl Clipboard for Paste {
    fn read(&self, _: iced::advanced::clipboard::Kind) -> Option<String> {
        Some(self.0.clone())
    }
    fn write(&mut self, _: iced::advanced::clipboard::Kind, value: String) {
        self.0 = value;
    }
}

struct Ui {
    renderer: iced::Renderer,
    tree: Tree,
    size: Size,
    cursor: mouse::Cursor,
    panel: bool,
}
impl Ui {
    async fn new(size: Size, panel: bool) -> Self {
        Self {
            renderer: <iced::Renderer as Headless>::new(
                iced::Font::with_name(reshiki::style::ui_font_family()),
                iced::Pixels(16.),
                None,
            )
            .await
            .unwrap(),
            tree: Tree::empty(),
            size,
            cursor: mouse::Cursor::Unavailable,
            panel,
        }
    }
    fn view<'a>(&self, app: &'a App) -> Element<'a, Message> {
        if self.panel {
            app.numeric_transform_panel()
        } else {
            app.view()
        }
    }
    fn event(&mut self, app: &mut App, event: Event, clipboard: &mut dyn Clipboard) {
        if let Event::Keyboard(iced::keyboard::Event::KeyPressed { modifiers, .. }) = &event {
            self.event(
                app,
                Event::Keyboard(iced::keyboard::Event::ModifiersChanged(*modifiers)),
                clipboard,
            );
        }
        let mut view = self.view(app);
        self.tree.diff(view.as_widget());
        let node = view.as_widget_mut().layout(
            &mut self.tree,
            &self.renderer,
            &layout::Limits::new(self.size, self.size),
        );
        let mut messages = Vec::new();
        view.as_widget_mut().update(
            &mut self.tree,
            &event,
            Layout::new(&node),
            self.cursor,
            &self.renderer,
            clipboard,
            &mut Shell::new(&mut messages),
            &Rectangle::with_size(self.size),
        );
        drop(view);
        for m in messages {
            let _ = app.update(m);
        }
    }
    fn operate(&mut self, app: &App, op: &mut dyn Operation) {
        let mut view = self.view(app);
        self.tree.diff(view.as_widget());
        let node = view.as_widget_mut().layout(
            &mut self.tree,
            &self.renderer,
            &layout::Limits::new(self.size, self.size),
        );
        view.as_widget_mut()
            .operate(&mut self.tree, Layout::new(&node), &self.renderer, op);
    }
    fn semantics(&mut self, app: &App) -> reshiki::accessibility::Snapshot {
        let mut collect = reshiki::accessibility::Collect::new(Rectangle::with_size(self.size));
        self.operate(app, &mut operation::black_box(&mut collect));
        collect.snapshot().clone()
    }
    fn node(&mut self, app: &App, id: &str) -> reshiki::accessibility::Node {
        self.semantics(app)
            .nodes
            .into_iter()
            .find(|n| n.id == id)
            .unwrap_or_else(|| panic!("Missing {id}"))
    }
    fn focus(&mut self, app: &App, id: &str) {
        let mut op: Box<dyn Operation> = Box::new(reshiki::accessibility::FocusControl::new(id));
        loop {
            self.operate(app, op.as_mut());
            if let operation::Outcome::Chain(next) = op.finish() {
                op = next;
            } else {
                break;
            }
        }
    }
    fn activate(&mut self, app: &mut App, id: &str) {
        let mut activate = reshiki::accessibility::Activate::<Message>::new(id);
        self.operate(app, &mut operation::black_box(&mut activate));
        if let operation::Outcome::Some(m) = activate.finish() {
            let _ = app.update(m);
        } else {
            panic!("No action for {id}");
        }
    }
    fn capture(&mut self, app: &App, path: Option<&Path>) -> bool {
        use iced::advanced::Renderer as _;
        let viewport = Rectangle::with_size(self.size);
        let theme = app.theme();
        let mut view = self.view(app);
        self.tree.diff(view.as_widget());
        let node = view.as_widget_mut().layout(
            &mut self.tree,
            &self.renderer,
            &layout::Limits::new(self.size, self.size),
        );
        self.renderer.reset(viewport);
        view.as_widget().draw(
            &self.tree,
            &mut self.renderer,
            &theme,
            &renderer::Style {
                text_color: theme.palette().text,
            },
            Layout::new(&node),
            self.cursor,
            &viewport,
        );
        let mut shown = false;
        if let Some(o) = view.as_widget_mut().overlay(
            &mut self.tree,
            Layout::new(&node),
            &self.renderer,
            &viewport,
            Vector::ZERO,
        ) {
            shown = true;
            let mut nested = overlay::Nested::new(o);
            let layout = nested.layout(&self.renderer, self.size);
            nested.draw(
                &mut self.renderer,
                &theme,
                &renderer::Style {
                    text_color: theme.palette().text,
                },
                Layout::new(&layout),
                self.cursor,
            );
        }
        if let Some(path) = path {
            let pixels = Headless::screenshot(
                &mut self.renderer,
                Size::new(self.size.width as u32, self.size.height as u32),
                1.,
                theme.palette().background,
            );
            image::save_buffer(
                path,
                &pixels,
                self.size.width as u32,
                self.size.height as u32,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
        shown
    }
}

fn press(
    key: iced::keyboard::Key,
    code: iced::keyboard::key::Code,
    modifiers: iced::keyboard::Modifiers,
    text: Option<&str>,
) -> Event {
    Event::Keyboard(iced::keyboard::Event::KeyPressed {
        modified_key: key.clone(),
        key,
        physical_key: iced::keyboard::key::Physical::Code(code),
        location: iced::keyboard::Location::Standard,
        modifiers,
        text: text.map(Into::into),
        repeat: false,
    })
}

fn drawing() -> App {
    let (mut app, _) = App::new();
    app.recovered.clear();
    app.appearance.mode = crate::appearance::Mode::Light;
    app.tab.doc = Document::from_json(include_bytes!(
        "../../tests/fixtures/phenylalanine-rotation.rsk"
    ))
    .unwrap();
    app.tab.saved = app.tab.doc.clone();
    app.tab.selected = app.tab.doc.all_ids();
    app.tab.camera.zoom = 2.;
    app.tab.camera.center = reshiki::editing::center(&app.tab.doc, &app.tab.selected);
    app.tab.labels_dirty = false;
    let _ = app.update(Message::KeyboardDrawing(keyboard_drawing::Action::Leave));
    app.sync_numeric_transforms();
    app
}

#[tokio::test]
#[ignore = "Real renderer palette geometry, native actions and matched 1600/default captures"]
async fn drawing_polish_renderer_palette() {
    // A 1000 pt outer macOS window has a 32 pt native title bar. The
    // default 1280 × 820 setting names the client size in main.rs.
    for size in [Size::new(1600., 968.), Size::new(1280., 820.)] {
        let (mut app, _) = App::new();
        app.recovered.clear();
        app.appearance.mode = crate::appearance::Mode::Light;
        let mut ui = Ui::new(size, false).await;
        ui.event(
            &mut app,
            Event::Window(iced::window::Event::RedrawRequested(
                iced::time::Instant::now(),
            )),
            &mut iced::advanced::clipboard::Null,
        );
        let nodes = ui.semantics(&app);
        assert!(nodes.duplicate_ids.is_empty());
        let elements: Vec<_> = nodes
            .nodes
            .iter()
            .filter(|n| n.id.starts_with("element-"))
            .collect();
        assert_eq!(elements.len(), 11);
        let h = elements.iter().find(|n| n.id == "element-H").unwrap();
        let c = elements.iter().find(|n| n.id == "element-C").unwrap();
        assert!(h.bounds.x < c.bounds.x);
        assert_eq!(h.bounds.y, c.bounds.y);
        let star = elements.iter().find(|n| n.id == "element-*").unwrap();
        if size.width == 1600. && !baseline() {
            assert_eq!(
                star.visible_bounds,
                Some(star.bounds),
                "* must be fully visible without scrolling"
            );
            for node in &elements {
                assert_eq!(
                    node.visible_bounds,
                    Some(node.bounds),
                    "{} is clipped",
                    node.id
                );
            }
        }
        if size.width == 1600. && baseline() {
            assert_ne!(
                star.visible_bounds,
                Some(star.bounds),
                "The retained baseline must reproduce the clipped last row"
            );
        }
        ui.capture(
            &app,
            Some(&directory().join(format!("palette-{}.png", size.width))),
        );
        if !baseline() && size.width == 1600. {
            for symbol in ["H", "C", "N", "O", "S", "P", "F", "Cl", "Br", "Fe", "*"] {
                ui.activate(&mut app, &format!("element-{symbol}"));
                assert_eq!(app.element, symbol);
            }
        }
    }
}

#[tokio::test]
#[ignore = "Real renderer focus, typing and one-step rotation with idle-only tooltips"]
async fn drawing_polish_renderer_rotation_field() {
    use iced::keyboard::{
        Key, Modifiers,
        key::{Code, Named},
    };
    for panel in [false, true] {
        let mut app = drawing();
        app.inspector_open = false;
        let before = app.tab.doc.clone();
        let mut ui = Ui::new(
            if panel {
                Size::new(300., 360.)
            } else {
                Size::new(1600., 1000.)
            },
            panel,
        )
        .await;
        let id = if panel {
            "transform-rotation"
        } else {
            "arrange-rotation"
        };
        let field = ui.node(&app, id);
        let point = field.bounds.center();
        ui.cursor = mouse::Cursor::Available(point);
        ui.event(
            &mut app,
            Event::Mouse(mouse::Event::CursorMoved { position: point }),
            &mut iced::advanced::clipboard::Null,
        );
        tokio::time::sleep(std::time::Duration::from_millis(550)).await;
        ui.event(
            &mut app,
            Event::Window(iced::window::Event::RedrawRequested(
                iced::time::Instant::now(),
            )),
            &mut iced::advanced::clipboard::Null,
        );
        assert!(
            ui.capture(
                &app,
                Some(&directory().join(format!(
                    "rotation-{}-idle.png",
                    if panel { "inspector" } else { "context" }
                )))
            ),
            "Idle hover keeps the hint"
        );
        ui.focus(&app, id);
        ui.event(
            &mut app,
            press(
                Key::Character("a".into()),
                Code::KeyA,
                Modifiers::COMMAND,
                None,
            ),
            &mut iced::advanced::clipboard::Null,
        );
        for (ch, code) in [("-", Code::Minus), ("6", Code::Digit6), ("0", Code::Digit0)] {
            ui.event(
                &mut app,
                press(
                    Key::Character(ch.into()),
                    code,
                    Modifiers::empty(),
                    Some(ch),
                ),
                &mut iced::advanced::clipboard::Null,
            );
            assert!(ui.node(&app, id).focused, "Typing retains focus");
        }
        assert_eq!(ui.node(&app, id).value.as_deref(), Some("-60"));
        assert_eq!(app.tab.doc, before);
        let shown = ui.capture(
            &app,
            Some(&directory().join(format!(
                "rotation-{}-editing.png",
                if panel { "inspector" } else { "context" }
            ))),
        );
        assert_eq!(shown, baseline(), "Only an idle field shows its hint");
        ui.event(
            &mut app,
            press(
                Key::Named(Named::Enter),
                Code::Enter,
                Modifiers::empty(),
                None,
            ),
            &mut iced::advanced::clipboard::Null,
        );
        assert_eq!(ui.node(&app, id).value.as_deref(), Some("0"));
        assert_eq!(app.tab.history.frames(), 1);
        assert!(!app.error, "{}", app.status);
        if !baseline() && ui.node(&app, id).focused {
            assert!(
                !ui.capture(&app, None),
                "Enter must not revive the hint while focused"
            );
        }
        let _ = app.update(Message::Undo);
        assert_eq!(
            app.tab
                .doc
                .atoms
                .iter()
                .map(|a| (a.id, a.position))
                .collect::<Vec<_>>(),
            before
                .atoms
                .iter()
                .map(|a| (a.id, a.position))
                .collect::<Vec<_>>(),
            "Undo restores every coordinate"
        );
        assert_eq!(app.tab.doc.bonds, before.bonds);
    }
}

#[tokio::test]
#[ignore = "Real Structure editor focus, replacing paste, accessible Clear and insert shortcut"]
async fn drawing_polish_renderer_structure_input() {
    use iced::keyboard::{
        Key, Modifiers,
        key::{Code, Named},
    };
    const OLD: &str = "Cn1cnc2c1c(=O)n(C)c(=O)n2C";
    let mut ui = Ui::new(Size::new(1600., 1000.), false).await;
    for from_name in [false, true] {
        let (mut app, _) = App::new();
        app.recovered.clear();
        app.appearance.mode = crate::appearance::Mode::Light;
        app.imports.set_text(OLD);
        let _ = app.update(Message::Inspector(InspectorTab::Import));
        if from_name {
            let _ = app.update(Message::Imports(import::Action::InputKind(
                import::InputKind::Name,
            )));
            let _ = app.update(Message::Imports(import::Action::InputKind(
                import::InputKind::Structure,
            )));
        } else {
            let _ = app.update(Message::Inspector(InspectorTab::Import));
        }
        ui.focus(&app, "import-input");
        if !baseline() {
            assert_eq!(app.imports.input.selection().as_deref(), Some(OLD));
        }
        ui.capture(
            &app,
            Some(&directory().join(format!(
                "structure-{}-opened.png",
                if from_name { "switch" } else { "reopen" }
            ))),
        );
        assert!(
            ui.node(&app, "import-input").focused,
            "Import editor focus must survive capture"
        );
        ui.event(
            &mut app,
            press(
                Key::Character("v".into()),
                Code::KeyV,
                Modifiers::COMMAND,
                None,
            ),
            &mut Paste("CCO".into()),
        );
        if !baseline() {
            assert_eq!(app.imports.input.text(), "CCO");
            ui.activate(&mut app, "import-clear");
            assert!(app.imports.input.text().is_empty());
            assert!(app.imports.is_blank());
            ui.focus(&app, "import-input");
            ui.event(
                &mut app,
                press(
                    Key::Character("c".into()),
                    Code::KeyC,
                    Modifiers::empty(),
                    Some("C"),
                ),
                &mut iced::advanced::clipboard::Null,
            );
            assert_eq!(app.imports.input.text(), "C");
            ui.event(
                &mut app,
                press(
                    Key::Named(Named::Enter),
                    Code::Enter,
                    Modifiers::COMMAND,
                    None,
                ),
                &mut iced::advanced::clipboard::Null,
            );
            assert!(
                app.tab.busy,
                "The original insert shortcut still launches the job"
            );
        }
    }
}

#[tokio::test]
#[ignore = "Prepare the real import → cleanup fixture for rotation-overlay QA"]
async fn prepare_rotation_polish_fixture() -> Result<(), String> {
    use reshiki::engine::ChemistryEngine;
    let engine = LocalEngine::default();
    let imported = engine
        .execute(Request::import_smiles("N[C@@H](Cc1ccccc1)C(=O)O"))
        .await?;
    let doc = imported.document.ok_or("No imported document")?;
    let mut request = Request::molecule("clean", doc.clone());
    request.selected_ids = Some(doc.all_ids());
    request.cleanup = Some(reshiki::cleanup::Options {
        scope: reshiki::cleanup::Scope::Drawing,
        ..Default::default()
    });
    let doc = engine
        .execute(request)
        .await?
        .document
        .ok_or("No cleaned document")?;
    doc.validate()?;
    std::fs::write(
        "tests/fixtures/phenylalanine-rotation.rsk",
        serde_json::to_vec_pretty(&doc).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
