//! Opt-in real-widget evidence for the exact large-gallery pointer report.
use super::*;
use iced::advanced::{Layout, Shell, layout, mouse, renderer, renderer::Headless, widget::Tree};
use iced::{Event, Rectangle, Size};
use std::path::PathBuf;

struct Ui {
    renderer: iced::Renderer,
    tree: Tree,
    size: Size,
    cursor: mouse::Cursor,
    directory: PathBuf,
}

impl Ui {
    fn event(&mut self, app: &mut App, event: Event) -> Vec<Edit> {
        if let Event::Mouse(mouse::Event::CursorMoved { position }) = event {
            self.cursor = mouse::Cursor::Available(position);
        }
        let mut view = app.view();
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
            &mut iced::advanced::clipboard::Null,
            &mut Shell::new(&mut messages),
            &Rectangle::with_size(self.size),
        );
        drop(view);
        let mut edits = Vec::new();
        for message in messages {
            if let Message::Canvas(edit) = &message {
                edits.push(edit.clone());
            }
            let _ = app.update(message);
        }
        edits
    }

    fn capture(&mut self, app: &App, name: &str) {
        use iced::advanced::Renderer as _;
        let viewport = Rectangle::with_size(self.size);
        let theme = app.theme();
        let mut view = app.view();
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
        let pixels = Headless::screenshot(
            &mut self.renderer,
            Size::new(self.size.width as u32, self.size.height as u32),
            1.,
            theme.palette().background,
        );
        image::save_buffer(
            self.directory.join(format!("{name}.png")),
            &pixels,
            self.size.width as u32,
            self.size.height as u32,
            image::ColorType::Rgba8,
        )
        .unwrap();
        println!(
            "capture,{name},zoom={},center={:?},target={:?}",
            app.tab.camera.zoom,
            app.tab.camera.center,
            app.tab.keyboard_drawing.target()
        );
    }
}

#[tokio::test]
#[ignore = "Exact-gallery real widget pan/zoom and 151-position hover evidence"]
async fn large_gallery_pointer_renderer() {
    let fixture = std::env::var_os("RESHIKI_HOVER_QA_DRAWING").expect("Exact issue fixture path");
    let doc = Document::from_json(&std::fs::read(fixture).unwrap()).unwrap();
    assert_eq!(doc.atoms.len(), 7728);
    assert_eq!(doc.bonds.len(), 6864);
    let (mut app, _) = App::new();
    app.recovered.clear();
    app.tab.doc = doc.clone();
    app.tab.saved = doc.clone();
    app.tab.busy = false;
    app.tab.labels_dirty = false;
    app.inspector_open = false;
    app.appearance.mode = crate::appearance::Mode::Light;
    // The actual canvas crosshair makes the pointer coordinate visible in a
    // headless screenshot; it is the same setting in both builds.
    app.guides.crosshair = true;
    app.sync_keyboard_drawing();
    let directory = PathBuf::from(std::env::var_os("RESHIKI_HOVER_QA_DIR").unwrap());
    std::fs::create_dir_all(&directory).unwrap();
    let mut ui = Ui {
        renderer: <iced::Renderer as Headless>::new(
            iced::Font::with_name(reshiki::style::ui_font_family()),
            iced::Pixels(16.),
            None,
        )
        .await
        .unwrap(),
        tree: Tree::empty(),
        size: Size::new(1600., 968.),
        cursor: mouse::Cursor::Unavailable,
        directory,
    };
    for _ in 0..2 {
        ui.event(
            &mut app,
            Event::Window(iced::window::Event::RedrawRequested(
                std::time::Instant::now(),
            )),
        );
    }
    let _ = app.update(Message::Fit);
    let pointer = iced::Point::new(439., 209.); // outer-window y241 minus 32pt title bar
    ui.event(
        &mut app,
        Event::Mouse(mouse::Event::CursorMoved { position: pointer }),
    );
    let original_target = app.tab.keyboard_drawing.target();
    ui.capture(&app, "fit-hover");
    // Calibrate Iced's logical wheel delta to the report's final 104% rather
    // than equating a macOS script's physical scroll pixels with this input.
    let zoom_delta = (1.04 / app.tab.camera.zoom).ln() / (12. * 0.003);
    println!("zoom-calibration,12 ticks,{zoom_delta} logical pixels per tick");
    ui.event(
        &mut app,
        Event::Keyboard(iced::keyboard::Event::ModifiersChanged(
            iced::keyboard::Modifiers::COMMAND,
        )),
    );
    for _ in 0..12 {
        ui.event(
            &mut app,
            Event::Mouse(mouse::Event::WheelScrolled {
                delta: mouse::ScrollDelta::Pixels {
                    x: 0.,
                    y: zoom_delta,
                },
            }),
        );
    }
    ui.capture(&app, "after-wheel-zoom");
    ui.event(
        &mut app,
        Event::Keyboard(iced::keyboard::Event::ModifiersChanged(
            iced::keyboard::Modifiers::empty(),
        )),
    );
    for _ in 0..20 {
        ui.event(
            &mut app,
            Event::Mouse(mouse::Event::WheelScrolled {
                delta: mouse::ScrollDelta::Pixels { x: 12., y: 6. },
            }),
        );
    }
    ui.capture(&app, "during-wheel-pan");
    if std::env::var_os("RESHIKI_HOVER_QA_BASELINE").is_some() {
        assert_eq!(app.tab.keyboard_drawing.target(), original_target);
    } else {
        assert_ne!(app.tab.keyboard_drawing.target(), original_target);
        assert!(app.tab.hover.is_some());
    }
    // This is a synchronous renderer/hit-parity trace, not an OS latency take.
    // The separately recorded native 5-second replay establishes visible lag.
    let mut trace = String::new();
    for step in 0..=150 {
        let angle = step as f32 / 150. * std::f32::consts::TAU;
        let position = iced::Point::new(700. - 400. * angle.cos(), 368. + 160. * angle.sin());
        ui.event(
            &mut app,
            Event::Mouse(mouse::Event::CursorMoved { position }),
        );
        trace.push_str(&format!("{step},{:?}\n", app.tab.keyboard_drawing.target()));
        if [0, 38, 75, 113, 150].contains(&step) {
            ui.capture(&app, &format!("hover-{step:03}"));
        }
    }
    std::fs::write(ui.directory.join("hover-targets.txt"), trace).unwrap();
    assert_eq!(app.tab.doc, doc);
    assert!(app.tab.selected.is_empty());
    assert!(!app.tab.history.can_undo());
}
