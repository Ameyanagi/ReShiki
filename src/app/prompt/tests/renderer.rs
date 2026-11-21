//! Actual Prompt layout, widget dispatch and specialized resolution.
use super::*;
use iced::advanced::{
    renderer::Headless,
    widget::{Id, Operation, operation},
};
use iced::keyboard::{self, key::Code};
use iced::{Event, Rectangle, Size, Vector, mouse};
use iced_runtime::{UserInterface, user_interface::Cache};

struct Ui {
    renderer: iced::Renderer,
    cache: Cache,
    size: Size,
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
            .expect("headless renderer"),
            cache: Cache::new(),
            size,
        }
    }

    fn inspect(&mut self, app: &App, operation: &mut dyn Operation) {
        let mut ui = UserInterface::build(
            app.view(),
            self.size,
            std::mem::take(&mut self.cache),
            &mut self.renderer,
        );
        ui.operate(&self.renderer, operation);
        self.cache = ui.into_cache();
    }

    fn snapshot(&mut self, app: &App) -> reshiki::accessibility::Snapshot {
        let mut collect = reshiki::accessibility::Collect::new(Rectangle::with_size(self.size));
        self.inspect(app, &mut operation::black_box(&mut collect));
        let snapshot = collect.snapshot().clone();
        assert!(snapshot.duplicate_ids.is_empty());
        snapshot
    }

    fn geometry(&mut self, app: &App) -> (Rectangle, Rectangle, Rectangle) {
        #[derive(Default)]
        struct Bounds {
            content: Option<Rectangle>,
            scroll: Option<(Rectangle, Rectangle)>,
        }
        impl Operation for Bounds {
            fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
                operate(self);
            }
            fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
                if id == Some(&Id::new("prompt-content")) {
                    self.content = Some(bounds);
                }
            }
            fn scrollable(
                &mut self,
                id: Option<&Id>,
                bounds: Rectangle,
                content: Rectangle,
                _: Vector,
                _: &mut dyn operation::Scrollable,
            ) {
                if id == Some(&Id::new("prompt-scroll")) {
                    self.scroll = Some((bounds, content));
                }
            }
        }
        let mut bounds = Bounds::default();
        self.inspect(app, &mut bounds);
        let (viewport, content) = bounds.scroll.expect("real prompt scroll area");
        // The existing modal surface adds 24px padding around this content.
        (bounds.content.unwrap().expand(24.), viewport, content)
    }

    fn focus(&mut self, app: &App, id: &str) {
        let mut focus: Box<dyn Operation> = Box::new(reshiki::accessibility::FocusControl::new(id));
        loop {
            self.inspect(app, focus.as_mut());
            match focus.finish() {
                operation::Outcome::Chain(next) => focus = next,
                _ => break,
            }
        }
    }

    fn event(&mut self, app: &App, event: Event, cursor: mouse::Cursor) -> Vec<Message> {
        let mut ui = UserInterface::build(
            app.view(),
            self.size,
            std::mem::take(&mut self.cache),
            &mut self.renderer,
        );
        let mut messages = Vec::new();
        let _ = ui.update(
            &[event],
            cursor,
            &mut self.renderer,
            &mut iced::advanced::clipboard::Null,
            &mut messages,
        );
        self.cache = ui.into_cache();
        messages
    }

    fn key(&mut self, app: &App, named: Named) -> Vec<Message> {
        let code = match named {
            Named::Enter => Code::Enter,
            Named::Escape => Code::Escape,
            _ => unreachable!(),
        };
        self.event(
            app,
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: Key::Named(named),
                modified_key: Key::Named(named),
                physical_key: keyboard::key::Physical::Code(code),
                location: keyboard::Location::Standard,
                modifiers: Modifiers::empty(),
                text: None,
                repeat: false,
            }),
            mouse::Cursor::Unavailable,
        )
    }

    fn click(&mut self, app: &App, id: &str) -> Vec<Message> {
        let snapshot = self.snapshot(app);
        let at = node(&snapshot, id).bounds.center();
        let mut messages = Vec::new();
        for event in [
            mouse::Event::CursorMoved { position: at },
            mouse::Event::ButtonPressed(mouse::Button::Left),
            mouse::Event::ButtonReleased(mouse::Button::Left),
        ] {
            messages.extend(self.event(app, Event::Mouse(event), mouse::Cursor::Available(at)));
        }
        messages
    }
}

fn node<'a>(
    snapshot: &'a reshiki::accessibility::Snapshot,
    id: &str,
) -> &'a reshiki::accessibility::Node {
    snapshot.nodes.iter().find(|node| node.id == id).unwrap()
}

fn visible(node: &reshiki::accessibility::Node) {
    let visible = node.visible_bounds.expect("visible prompt control");
    assert!((visible.width - node.bounds.width).abs() < 0.02, "{node:?}");
    assert!(
        (visible.height - node.bounds.height).abs() < 0.02,
        "{node:?}"
    );
}

fn answer(messages: Vec<Message>) -> Answer {
    match messages.as_slice() {
        [Message::Prompt(answer)] => *answer,
        _ => panic!("one prompt answer: {messages:?}"),
    }
}

#[tokio::test]
#[ignore = "Opt-in actual Prompt sheet layout and scrolling"]
async fn prompt_sheets_fit_short_long_and_narrow_windows() {
    for size in [Size::new(1040., 680.), Size::new(320., 480.)] {
        let mut ui = Ui::new(size).await;
        let (mut app, _) = App::new();
        app.tab.busy = false;
        app.prompt = Some(Prompt::Notice {
            title: "Notice".into(),
            text: "The operation finished.".into(),
            then: Box::new(Message::Fit),
        });
        let (short, viewport, content) = ui.geometry(&app);
        assert!(short.width <= 480. && short.width <= size.width);
        assert!(
            short.height < 240.,
            "Short questions have no fixed-height gap"
        );
        assert!(content.height <= viewport.height + 0.02);
        assert!(short.x >= 0. && short.y >= 0.);
        if cfg!(target_os = "macos") {
            assert!((short.y - 24.).abs() < 0.02);
        } else {
            assert!((short.center_y() - size.height / 2.).abs() < 0.02);
        }
        let snapshot = ui.snapshot(&app);
        visible(node(&snapshot, "prompt-close"));
        visible(node(&snapshot, "prompt-ok"));
        assert!(!snapshot.nodes.iter().any(|node| node.name == "Done"));

        app.prompt = Some(Prompt::Save("Scheme.rsk".into()));
        let snapshot = ui.snapshot(&app);
        for id in [
            "prompt-close",
            "prompt-dont-save",
            "prompt-cancel",
            "prompt-save",
        ] {
            let control = node(&snapshot, id);
            visible(control);
            assert!(control.bounds.x >= 0.);
            assert!(control.bounds.x + control.bounds.width <= size.width);
        }

        app.prompt = Some(Prompt::Notice {
            title: format!("A detailed export warning {}", "X".repeat(80)),
            text: "The drawing contains information this format cannot keep. ".repeat(100),
            then: Box::new(Message::Fit),
        });
        let (long, viewport, content) = ui.geometry(&app);
        assert!(long.height > short.height);
        assert!(long.y >= 0. && long.y + long.height <= size.height + 0.02);
        assert!(content.height > viewport.height);
        visible(node(&ui.snapshot(&app), "prompt-close"));
        ui.focus(&app, "prompt-ok");
        let snapshot = ui.snapshot(&app);
        let ok = node(&snapshot, "prompt-ok");
        assert!(
            ok.focused,
            "The final action is reachable through the scroll area"
        );
        visible(ok);
    }
}

#[tokio::test]
#[ignore = "Opt-in actual Prompt dismissal and primary/secondary answers"]
async fn prompt_widgets_keep_existing_answers_and_export_ownership() {
    let mut ui = Ui::new(Size::new(640., 480.)).await;
    let mut app = edited();
    let drawing = app.tab.doc.clone();
    let tab = app.tab.id;
    for close in [false, true] {
        close_asking(&mut app);
        let messages = if close {
            ui.click(&app, "prompt-close")
        } else {
            ui.key(&app, Named::Escape)
        };
        let answer = answer(messages);
        assert_eq!(answer, Answer::Cancel);
        let _ = app.update(Message::Prompt(answer));
        assert!(app.prompt.is_none() && app.pending.is_none());
        assert_eq!(app.tab.id, tab);
        assert_eq!(app.tab.doc, drawing);
    }

    for prompt in [
        Prompt::Save("Scheme.rsk".into()),
        Prompt::KeepFormat("mol"),
        Prompt::SaveAs,
    ] {
        app.prompt = Some(prompt.clone());
        let primary = answer(ui.key(&app, Named::Enter));
        assert_eq!(primary, Answer::Save);
        let resolved = prompt.answer(primary);
        if matches!(app.prompt, Some(Prompt::Save(_))) {
            assert!(matches!(resolved, Message::Save));
        } else {
            assert!(matches!(resolved, Message::SaveFormat(format)
                if format == reshiki::compatibility::NATIVE_EXTENSION));
        }
    }
    app.prompt = Some(Prompt::KeepFormat("mol"));
    let keep = answer(ui.click(&app, "prompt-keep-format"));
    assert!(matches!(
        app.prompt.clone().unwrap().answer(keep),
        Message::KeepFormat
    ));
    app.prompt = Some(Prompt::SaveAs);
    let format = answer(ui.click(&app, "prompt-format-cdxml"));
    assert!(matches!(
        app.prompt.clone().unwrap().answer(format),
        Message::SaveFormat("cdxml")
    ));

    // Closing a notice retains its acknowledgement continuation.
    let grid = app.grid;
    app.prompt = Some(Prompt::Notice {
        title: "Notice".into(),
        text: "Acknowledge this operation.".into(),
        then: Box::new(Message::View(crate::app::view_settings::Action::Grid)),
    });
    let close = answer(ui.click(&app, "prompt-close"));
    let _ = app.update(Message::Prompt(close));
    assert!(app.prompt.is_none());
    assert_eq!(app.grid, !grid);

    let export = crate::app::figure_export::Prepared::new(
        vec![1, 2, 3],
        "png",
        None,
        vec![],
        crate::app::figure_export::Kind::Figure,
    );
    let download = Prompt::Download {
        name: "scheme.png".into(),
        action: Download::Export { tab, export },
    };
    app.prompt = Some(download.clone());
    let primary = answer(ui.key(&app, Named::Enter));
    let Message::Tab(owner, resolved) = download.answer(primary) else {
        panic!("Download retains its originating tab");
    };
    assert_eq!(owner, tab);
    assert!(matches!(*resolved, Message::SaveExport(ref name, _) if name == "scheme.png"));
    app.figure_exporting = true;
    let close = answer(ui.click(&app, "prompt-close"));
    let task = app.update(Message::Prompt(close));
    assert!(app.prompt.is_none() && !app.figure_exporting);
    assert_eq!(
        task.units(),
        0,
        "Closing does not start the prepared download"
    );
    assert_eq!(app.tab.doc, drawing);
}
