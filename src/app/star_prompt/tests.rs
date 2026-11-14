use super::*;
use crate::app::{clipboard::CopyAsKey, figure_export::Saved, nmr, workspace::layout_snapshots};
use chrono::Days;
use reshiki::clipboard::{CopyFormat, CopyOutcome};

fn day(text: &str) -> NaiveDate {
    text.parse().unwrap()
}

fn asking(first_launch_day: NaiveDate) -> Record {
    Record {
        first_launch_day,
        uses: USES,
        state: Answer::Asking,
    }
}

/// A test app whose record lives in a temporary data folder.
fn app_with(record: Record) -> (App, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("star-prompt.json");
    record.save(&path).unwrap();
    let (mut app, _) = App::new();
    app.star = State {
        path: Some(path),
        record: Some(record),
        ..State::load()
    };
    (app, dir)
}

fn benzene(app: &mut App) {
    app.tab.doc = reshiki::rings::Preset::Benzene.document(42., false);
    app.tab.saved = app.tab.doc.clone();
}

fn saved_record(app: &App) -> Option<Record> {
    Record::parse(&std::fs::read(app.star.path.as_ref().unwrap()).unwrap())
}

/// Input stopped `IDLE` ago.
fn idle(app: &mut App) {
    app.star.last_input = Instant::now().checked_sub(IDLE).unwrap();
}

#[test]
fn asks_only_when_every_rule_holds() {
    let first = day("2026-11-01");
    let ready = Moment {
        today: day("2026-11-02"),
        version: "0.16.0",
        idle: IDLE,
        focused: true,
        busy: false,
    };
    let record = asking(first);
    assert!(should_ask(Some(&record), &ready));
    // An unreadable or unwritable file never asks.
    assert!(!should_ask(None, &ready));
    let fewer = Record {
        uses: USES - 1,
        ..record.clone()
    };
    assert!(!should_ask(Some(&fewer), &ready));
    for today in [first, day("2026-10-31")] {
        assert!(!should_ask(Some(&record), &Moment { today, ..ready }));
    }
    let recent = Moment {
        idle: IDLE - Duration::from_secs(1),
        ..ready
    };
    assert!(!should_ask(Some(&record), &recent));
    let unfocused = Moment {
        focused: false,
        ..ready
    };
    assert!(!should_ask(Some(&record), &unfocused));
    let busy = Moment {
        busy: true,
        ..ready
    };
    assert!(!should_ask(Some(&record), &busy));

    // Starred and done never ask again, even years later in a new major version.
    let years = Moment {
        today: day("2030-01-01"),
        version: "2.0.0",
        ..ready
    };
    for state in [Answer::Starred, Answer::Done] {
        let record = Record {
            state,
            ..record.clone()
        };
        assert!(!should_ask(Some(&record), &years));
    }

    // The re-ask: a feature release at least 90 days after "Not now".
    let dismissed_on = day("2026-11-02");
    for (dismissed, running, days, expected) in [
        ("0.15.0", "0.16.0", 90, true),
        ("0.15.0", "0.16.0", 89, false),
        ("0.15.0", "1.0.0", 90, true),
        ("0.15.0", "0.15.1", 365, false),
        ("0.15.0", "0.15.0", 365, false),
        ("0.15.0", "0.16.0-nightly.20270201.1.1", 365, false),
        ("0.16.0-nightly.20261101.1.1", "0.16.0", 365, false),
        ("0.16.0-nightly.20261101.1.1", "0.17.0", 365, true),
        ("not a version", "0.16.0", 365, false),
    ] {
        let record = Record {
            state: Answer::Dismissed {
                version: dismissed.into(),
                day: dismissed_on,
            },
            ..record.clone()
        };
        let moment = Moment {
            today: dismissed_on + Days::new(days),
            version: running,
            ..ready
        };
        assert_eq!(
            should_ask(Some(&record), &moment),
            expected,
            "{dismissed} → {running} after {days} days"
        );
    }
}

#[test]
fn record_round_trips_and_keeps_a_newer_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data/star-prompt.json");
    let today = day("2026-11-15");
    let new = Record::load(&path, today).unwrap();
    assert_eq!(
        new,
        Record {
            uses: 0,
            ..asking(today)
        }
    );
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        r#"{"first_launch_day":"2026-11-15","uses":0,"state":"asking"}"#
    );
    // A later launch keeps the first day.
    assert_eq!(Record::load(&path, day("2026-12-01")), Some(new.clone()));
    for state in [
        Answer::Starred,
        Answer::Done,
        Answer::Dismissed {
            version: "0.15.0".into(),
            day: today,
        },
    ] {
        let record = Record {
            uses: 3,
            state,
            ..new.clone()
        };
        record.save(&path).unwrap();
        assert_eq!(Record::load(&path, today), Some(record));
    }

    // A newer build's file is unreadable here: never ask, and leave it alone.
    let newer = br#"{"first_launch_day":"2026-11-01","uses":5,"state":"later"}"#;
    std::fs::write(&path, newer).unwrap();
    assert_eq!(Record::load(&path, today), None);
    assert_eq!(std::fs::read(&path).unwrap(), newer);
    let bad = path.with_extension("json.bad");
    assert!(!bad.exists());
    // Replacing it keeps a copy first.
    new.save(&path).unwrap();
    assert_eq!(std::fs::read(&bad).unwrap(), newer);
    assert_eq!(Record::load(&path, today), Some(new));

    // A file that cannot be read, or a folder that cannot be written, never asks.
    let folder = dir.path().join("folder.json");
    std::fs::create_dir(&folder).unwrap();
    assert_eq!(Record::load(&folder, today), None);
    assert!(folder.is_dir());
    let file = dir.path().join("file");
    std::fs::write(&file, b"").unwrap();
    assert_eq!(Record::load(&file.join("star-prompt.json"), today), None);
}

#[test]
fn exports_and_picture_copies_count_up_to_the_threshold() {
    let (mut app, _dir) = app_with(Record {
        uses: 0,
        ..asking(day("2026-11-01"))
    });
    let uses = |app: &App| app.star.record.as_ref().unwrap().uses;
    let saved = || {
        Some(Saved {
            path: "Molecule.png".into(),
            details: vec![],
        })
    };
    let _ = app.update(Message::FigureExported(Ok(saved())));
    assert_eq!(uses(&app), 1);
    let _ = app.update(Message::FigureExported(Ok(None)));
    let _ = app.update(Message::FigureExported(Err("Disk full".into())));
    assert_eq!(uses(&app), 1, "Canceled and failed exports do not count");
    let _ = app.update(Message::Exported(Ok(saved())));
    assert_eq!(uses(&app), 2);
    let _ = app.update(Message::Nmr(nmr::Action::Exported(Ok(Some(
        "Shifts.csv".into(),
    )))));
    assert_eq!(uses(&app), 3);

    let key = |app: &App, format| CopyAsKey {
        epoch: app.tab.file_epoch,
        revision: app.tab.revision,
        format,
        selected: vec![],
    };
    let _ = app.update(Message::CopyAsWritten(
        key(&app, CopyFormat::Smiles),
        Ok(vec![]),
    ));
    let _ = app.update(Message::CopyAsWritten(
        key(&app, CopyFormat::Png),
        Err("Busy".into()),
    ));
    assert_eq!(uses(&app), 3, "Text copies and failed copies do not count");
    let _ = app.update(Message::CopyAsWritten(
        key(&app, CopyFormat::Svg),
        Ok(vec![]),
    ));
    assert_eq!(uses(&app), 4);
    let written = |image_only| Message::ClipboardWritten {
        epoch: 0,
        revision: 0,
        cut_ids: vec![],
        result: Ok(CopyOutcome {
            external_editable: false,
            image_only,
            notices: vec![],
        }),
    };
    let _ = app.update(written(true));
    assert_eq!(uses(&app), USES);
    assert_eq!(saved_record(&app).unwrap().uses, USES);
    // The count stops at the threshold, so later copies write nothing.
    let _ = app.update(Message::FigureExported(Ok(saved())));
    assert_eq!(uses(&app), USES);

    // Copy puts a picture on the clipboard, except on Windows.
    let (mut app, _dir) = app_with(Record {
        uses: 0,
        ..asking(day("2026-11-01"))
    });
    let _ = app.update(written(false));
    assert_eq!(uses(&app), u32::from(!cfg!(windows)));
}

#[test]
fn the_toast_waits_for_a_quiet_moment_and_asks_once_per_run() {
    let today = today();
    let (mut app, _dir) = app_with(asking(today - Days::new(1)));
    assert!(app.star.possible());
    let tick = |app: &mut App| {
        let _ = app.update(Message::Star(Action::Tick));
        app.star.open
    };
    let _ = app.update(Message::Star(Action::Input(None)));
    assert!(!tick(&mut app), "Input within 30 seconds");
    idle(&mut app);
    app.help_open = true;
    assert!(!tick(&mut app), "Help is open");
    app.help_open = false;
    app.tab.office = Some(crate::session::Binding::standalone(
        "Drawing in Word.rsk".into(),
        crate::session::Host::LibreOffice,
    ));
    assert!(!tick(&mut app), "The drawing belongs to LibreOffice");
    app.tab.office = None;
    let _ = app.update(Message::Star(Action::Focused(false)));
    idle(&mut app);
    assert!(!tick(&mut app), "The window is in the background");
    let _ = app.update(Message::Star(Action::Focused(true)));
    let _ = app.update(Message::Star(Action::Input(Some(true))));
    idle(&mut app);
    assert!(!tick(&mut app), "A mouse button is held, as in a drag");
    let _ = app.update(Message::Star(Action::Input(Some(false))));
    idle(&mut app);
    assert!(tick(&mut app));
    assert!(!app.star.possible(), "No timer once it has asked");

    let _ = app.update(Message::Star(Action::Dismiss));
    assert!(!app.star.open);
    let dismissed = Answer::Dismissed {
        version: reshiki::updates::CURRENT_VERSION.into(),
        day: today,
    };
    assert_eq!(app.star.record.as_ref().unwrap().state, dismissed);
    assert_eq!(saved_record(&app).unwrap().state, dismissed);
    idle(&mut app);
    assert!(!tick(&mut app), "At most once per run");
}

#[test]
fn the_second_close_or_a_star_ends_the_asks() {
    let first = day("2026-01-01");
    let (mut app, _dir) = app_with(Record {
        state: Answer::Dismissed {
            version: "0.1.0".into(),
            day: first,
        },
        ..asking(first)
    });
    app.star.open = true;
    let _ = app.update(Message::Star(Action::Dismiss));
    assert_eq!(saved_record(&app).unwrap().state, Answer::Done);

    for toast in [true, false] {
        let (mut app, _dir) = app_with(asking(first));
        app.star.open = toast;
        let _ = app.update(Message::Star(Action::Star { toast }));
        assert!(!app.star.open);
        assert_eq!(saved_record(&app).unwrap().state, Answer::Starred);
        let _ = app.update(Message::Star(Action::Opened(Ok(()))));
        assert_eq!(app.status, THANKS);
        assert!(!app.error);
        let _ = app.update(Message::Star(Action::Opened(Err(
            "Could not open the browser".into(),
        ))));
        assert_eq!(app.status, "Could not open the browser");
        assert!(app.error);
    }
}

/// The real view: event dispatch, focus and 2× pictures.
struct Ui {
    renderer: iced::Renderer,
    tree: iced::advanced::widget::Tree,
    size: iced::Size,
}

impl Ui {
    async fn new() -> Self {
        use iced::advanced::renderer::Headless;
        Self {
            renderer: <iced::Renderer as Headless>::new(
                iced::Font::with_name(reshiki::style::ui_font_family()),
                iced::Pixels(16.),
                None,
            )
            .await
            .unwrap(),
            tree: iced::advanced::widget::Tree::empty(),
            size: iced::Size::new(1280., 800.),
        }
    }

    /// Lays out the view and runs `visit` on it.
    fn with_view<T>(
        &mut self,
        app: &App,
        visit: impl FnOnce(
            &mut Element<'_, Message>,
            &mut iced::advanced::widget::Tree,
            &iced::advanced::layout::Node,
            &mut iced::Renderer,
        ) -> T,
    ) -> T {
        let mut view = app.view();
        self.tree.diff(view.as_widget());
        let node = view.as_widget_mut().layout(
            &mut self.tree,
            &self.renderer,
            &iced::advanced::layout::Limits::new(self.size, self.size),
        );
        visit(&mut view, &mut self.tree, &node, &mut self.renderer)
    }

    fn key(&mut self, app: &App, named: iced::keyboard::key::Named) -> Vec<Message> {
        use iced::keyboard::{Event, Key, Location, Modifiers, key::Physical};
        let code = match named {
            iced::keyboard::key::Named::Tab => iced::keyboard::key::Code::Tab,
            _ => iced::keyboard::key::Code::Enter,
        };
        let size = self.size;
        self.with_view(app, |view, tree, node, renderer| {
            let mut messages = Vec::new();
            view.as_widget_mut().update(
                tree,
                &iced::Event::Keyboard(Event::KeyPressed {
                    key: Key::Named(named),
                    modified_key: Key::Named(named),
                    physical_key: Physical::Code(code),
                    location: Location::Standard,
                    modifiers: Modifiers::empty(),
                    text: None,
                    repeat: false,
                }),
                iced::advanced::Layout::new(node),
                iced::mouse::Cursor::Unavailable,
                renderer,
                &mut iced::advanced::clipboard::Null,
                &mut iced::advanced::Shell::new(&mut messages),
                &iced::Rectangle::with_size(size),
            );
            messages
        })
    }

    fn operate(&mut self, app: &App, operation: &mut dyn iced::advanced::widget::Operation) {
        self.with_view(app, |view, tree, node, renderer| {
            view.as_widget_mut().operate(
                tree,
                iced::advanced::Layout::new(node),
                renderer,
                operation,
            );
        });
    }

    fn focused(&mut self, app: &App) -> Vec<String> {
        let mut collect =
            reshiki::accessibility::Collect::new(iced::Rectangle::with_size(self.size));
        self.operate(
            app,
            &mut iced::advanced::widget::operation::black_box(&mut collect),
        );
        collect
            .snapshot()
            .nodes
            .iter()
            .filter(|node| node.focused)
            .map(|node| node.id.clone())
            .collect()
    }

    fn ids(&mut self, app: &App) -> Vec<String> {
        let mut collect =
            reshiki::accessibility::Collect::new(iced::Rectangle::with_size(self.size));
        self.operate(
            app,
            &mut iced::advanced::widget::operation::black_box(&mut collect),
        );
        collect
            .snapshot()
            .nodes
            .iter()
            .map(|n| n.id.clone())
            .collect()
    }

    /// Updates the app, then applies the widget operations its task returns.
    async fn update(&mut self, app: &mut App, message: Message) {
        use iced::advanced::widget::operation::Outcome;
        use iced::futures::StreamExt;
        let Some(mut stream) = iced_runtime::task::into_stream(app.update(message)) else {
            return;
        };
        while let Some(action) = stream.next().await {
            if let iced_runtime::Action::Widget(operation) = action {
                let mut current = Some(operation);
                while let Some(mut operation) = current.take() {
                    self.operate(app, operation.as_mut());
                    if let Outcome::Chain(next) = operation.finish() {
                        current = Some(next);
                    }
                }
            }
        }
    }

    /// Saves the window at twice its size, as on a high-DPI screen.
    fn picture(&mut self, app: &mut App, path: &std::path::Path) {
        use iced::advanced::{Layout, Renderer as _, overlay, renderer::Headless};
        for _ in 0..3 {
            let messages =
                layout_snapshots::pass(app, &mut self.renderer, &mut self.tree, self.size, None);
            for message in messages {
                let _ = app.update(message);
            }
        }
        let theme = app.theme();
        let style = iced::advanced::renderer::Style {
            text_color: theme.palette().text,
        };
        let viewport = iced::Rectangle::with_size(self.size);
        let size = self.size;
        self.with_view(app, |view, tree, node, renderer| {
            // Buttons settle their status on a redraw, as in the app.
            view.as_widget_mut().update(
                tree,
                &iced::Event::Window(iced::window::Event::RedrawRequested(Instant::now())),
                Layout::new(node),
                iced::mouse::Cursor::Unavailable,
                renderer,
                &mut iced::advanced::clipboard::Null,
                &mut iced::advanced::Shell::new(&mut Vec::new()),
                &viewport,
            );
            renderer.reset(viewport);
            view.as_widget().draw(
                tree,
                renderer,
                &theme,
                &style,
                Layout::new(node),
                iced::mouse::Cursor::Unavailable,
                &viewport,
            );
            if let Some(element) = view.as_widget_mut().overlay(
                tree,
                Layout::new(node),
                renderer,
                &viewport,
                iced::Vector::ZERO,
            ) {
                let mut nested = overlay::Nested::new(element);
                let node = nested.layout(renderer, size);
                nested.draw(
                    renderer,
                    &theme,
                    &style,
                    Layout::new(&node),
                    iced::mouse::Cursor::Unavailable,
                );
            }
            let (width, height) = (size.width as u32 * 2, size.height as u32 * 2);
            let pixels = Headless::screenshot(
                renderer,
                iced::Size::new(width, height),
                2.,
                theme.palette().background,
            );
            image::save_buffer(path, &pixels, width, height, image::ColorType::Rgba8).unwrap();
        });
    }
}

#[tokio::test]
#[ignore = "Opt-in renderer check"]
async fn the_toast_takes_no_focus_is_reached_by_tab_and_returns_focus_to_the_drawing() {
    use iced::keyboard::key::Named;
    let mut ui = Ui::new().await;
    let (mut app, _dir) = app_with(asking(today() - Days::new(1)));
    benzene(&mut app);
    idle(&mut app);
    ui.update(&mut app, Message::Star(Action::Tick)).await;
    assert!(app.star.open);
    let ids = ui.ids(&app);
    for id in ["star-close", "star-not-now", "star-github"] {
        assert!(ids.iter().any(|node| node == id), "{id} in {ids:?}");
    }
    assert!(ui.focused(&app).is_empty(), "The toast takes no focus");
    let mut reached = false;
    for _ in 0..ids.len() {
        assert!(ui.key(&app, Named::Tab).is_empty());
        if ui.focused(&app) == ["star-not-now"] {
            reached = true;
            break;
        }
    }
    assert!(reached, "Tab reaches Not now");
    let messages = ui.key(&app, Named::Enter);
    assert!(matches!(
        messages.as_slice(),
        [Message::Star(Action::Dismiss)]
    ));
    for message in messages {
        ui.update(&mut app, message).await;
    }
    assert!(!app.star.open);
    assert!(ui.focused(&app).is_empty(), "Focus returns to the drawing");
    assert!(!ui.ids(&app).iter().any(|id| id.starts_with("star-")));
}

#[tokio::test]
#[ignore = "Opt-in renderer evidence; set RESHIKI_STAR_EVIDENCE_DIR"]
async fn star_prompt_evidence() {
    let Some(directory) = std::env::var_os("RESHIKI_STAR_EVIDENCE_DIR") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    std::fs::create_dir_all(&directory).unwrap();
    let mut ui = Ui::new().await;
    for (mode, theme) in [
        (crate::appearance::Mode::Light, "light"),
        (crate::appearance::Mode::Dark, "dark"),
    ] {
        let (mut app, _dir) = app_with(asking(today() - Days::new(1)));
        app.appearance.mode = mode;
        benzene(&mut app);
        ui.picture(&mut app, &directory.join(format!("canvas-{theme}.png")));
        idle(&mut app);
        let _ = app.update(Message::Star(Action::Tick));
        assert!(app.star.open);
        ui.picture(&mut app, &directory.join(format!("toast-{theme}.png")));
        let _ = app.update(Message::Star(Action::Dismiss));
        app.updates.open = true;
        ui.picture(&mut app, &directory.join(format!("about-{theme}.png")));
    }
}
