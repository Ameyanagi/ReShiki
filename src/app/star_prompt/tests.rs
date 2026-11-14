use super::*;
use crate::app::{
    clipboard::CopyAsKey, document_styles, figure_export::Saved, import, nmr, theme_files,
    workspace::layout_snapshots,
};
use chrono::Days;
use iced::{Event, mouse, touch, window};
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

fn save(path: &Path, record: &Record) {
    std::fs::write(path, serde_json::to_vec(record).unwrap()).unwrap();
}

/// A test app whose record lives in a temporary data folder.
fn app_with(record: Record) -> (App, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("star-prompt.json");
    save(&path, &record);
    let (mut app, _) = App::new();
    app.star = State::at(Some(path));
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

fn event(app: &mut App, event: Event) {
    let _ = app.update(Message::Star(Action::Event(event)));
}

/// A timer tick after `IDLE` without input, with the drawing holding the
/// keyboard; whether the toast is open afterwards.
fn tick(app: &mut App) -> bool {
    idle(app);
    let _ = app.update(Message::Star(Action::Tick));
    let _ = app.update(Message::Star(Action::Checked { focused: false }));
    app.star.open
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

    // The re-ask: a stable x.y.0 feature release at least 90 days after the first.
    let asked_on = day("2026-11-02");
    for (asked, running, days, expected) in [
        ("0.15.0", "0.16.0", 90, true),
        ("0.15.0", "0.16.0", 89, false),
        ("0.15.0", "1.0.0", 90, true),
        ("0.15.0", "0.15.1", 365, false),
        ("0.15.0", "0.16.1", 365, false),
        ("0.15.0", "1.0.1", 365, false),
        ("0.15.0", "0.15.0", 365, false),
        ("0.15.0", "0.16.0-nightly.20270201.1.1", 365, false),
        ("0.16.0-nightly.20261101.1.1", "0.16.0", 365, false),
        ("0.16.0-nightly.20261101.1.1", "0.17.0", 365, true),
        ("not a version", "0.16.0", 365, false),
    ] {
        let record = Record {
            state: Answer::Asked {
                version: asked.into(),
                day: asked_on,
            },
            ..record.clone()
        };
        let moment = Moment {
            today: asked_on + Days::new(days),
            version: running,
            ..ready
        };
        assert_eq!(
            should_ask(Some(&record), &moment),
            expected,
            "{asked} → {running} after {days} days"
        );
    }
}

#[test]
fn record_round_trips_and_leaves_a_newer_file_alone() {
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
        Answer::Asked {
            version: "0.15.0".into(),
            day: today,
        },
    ] {
        let record = Record {
            uses: 3,
            state,
            ..new.clone()
        };
        assert_eq!(record.save(&path), Some(()));
        assert_eq!(Record::load(&path, today), Some(record));
    }

    // A newer build's file is unreadable here: never ask, and leave it alone.
    let newer = br#"{"first_launch_day":"2026-11-01","uses":5,"state":"later"}"#;
    std::fs::write(&path, newer).unwrap();
    assert_eq!(Record::load(&path, today), None);
    assert_eq!(std::fs::read(&path).unwrap(), newer);

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
fn a_record_that_stops_reading_or_saving_ends_the_asks_and_is_left_alone() {
    // Read after the first load fails: nothing is written over it.
    let (mut app, dir) = app_with(Record {
        uses: 0,
        ..asking(day("2026-11-01"))
    });
    let path = dir.path().join("star-prompt.json");
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    app.star.used();
    assert_eq!(app.star.record, None);
    assert!(path.is_dir());

    // A valid record in a folder that cannot save the ask never shows the toast.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let (mut app, dir) = app_with(asking(today() - Days::new(1)));
        let path = dir.path().join("star-prompt.json");
        let before = std::fs::read(&path).unwrap();
        let mode = |mode| std::fs::Permissions::from_mode(mode);
        std::fs::set_permissions(dir.path(), mode(0o555)).unwrap();
        assert!(app.star.record.is_some(), "Loading only reads");
        assert!(!tick(&mut app));
        std::fs::set_permissions(dir.path(), mode(0o755)).unwrap();
        assert_eq!(app.star.record, None);
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
}

#[test]
fn two_running_copies_keep_each_others_counts_and_answers() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("star-prompt.json");
    let saved = || Record::parse(&std::fs::read(&path).unwrap()).unwrap();
    let running = || {
        let (mut app, _) = App::new();
        app.star = State::at(Some(path.clone()));
        app
    };
    let export = || {
        Message::FigureExported(Ok(Some(Saved {
            path: "Molecule.png".into(),
            details: vec![],
        })))
    };
    save(
        &path,
        &Record {
            uses: 3,
            ..asking(today() - Days::new(1))
        },
    );
    let (mut a, mut b) = (running(), running());
    let _ = a.receive(export());
    let _ = b.receive(export());
    assert_eq!(saved().uses, USES, "Both uses count");
    let _ = a.update(Message::Star(Action::Star { toast: false }));
    // B starts from A's answer, so it never asks.
    assert!(!tick(&mut b));
    assert_eq!(b.star.record.as_ref().unwrap().state, Answer::Starred);
    assert_eq!(saved().state, Answer::Starred);
    assert!(!b.star.waiting());

    // An ask that the other copy already made is not made again.
    save(&path, &asking(today() - Days::new(1)));
    let (mut a, mut b) = (running(), running());
    assert!(tick(&mut a));
    assert!(!tick(&mut b));
    assert!(matches!(saved().state, Answer::Asked { .. }));
}

#[test]
fn an_unanswered_toast_counts_as_an_ask_after_a_restart() {
    let (mut app, dir) = app_with(asking(today() - Days::new(1)));
    assert!(tick(&mut app));
    let asked = Answer::Asked {
        version: CURRENT_VERSION.into(),
        day: today(),
    };
    assert_eq!(
        saved_record(&app).unwrap().state,
        asked,
        "Saved before it shows"
    );
    // Quit without an answer, then start again.
    let (mut app, _) = App::new();
    app.star = State::at(Some(dir.path().join("star-prompt.json")));
    assert!(!app.star.waiting());
    assert!(!tick(&mut app));
}

#[test]
fn every_export_and_picture_copy_counts_once_wherever_its_drawing_went() {
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
    let _ = app.receive(Message::FigureExported(Ok(saved())));
    assert_eq!(uses(&app), 1);
    let _ = app.receive(Message::FigureExported(Ok(None)));
    let _ = app.receive(Message::FigureExported(Err("Disk full".into())));
    let _ = app.receive(Message::ThemeFile(theme_files::Action::Saved(Ok(false))));
    assert_eq!(uses(&app), 1, "Canceled and failed exports do not count");
    let _ = app.receive(Message::Exported(Ok(saved())));
    assert_eq!(uses(&app), 2);
    let _ = app.receive(Message::Nmr(nmr::Action::Exported(Ok(Some(
        "Shifts.csv".into(),
    )))));
    assert_eq!(uses(&app), 3);
    let _ = app.receive(Message::ThemeFile(theme_files::Action::Saved(Ok(true))));
    assert_eq!(uses(&app), 4);
    let _ = app.receive(Message::DrawingStyle(document_styles::Action::Saved(Ok(
        true,
    ))));
    assert_eq!(uses(&app), USES);
    assert_eq!(saved_record(&app).unwrap().uses, USES);
    // The count stops at the threshold.
    let _ = app.receive(Message::FigureExported(Ok(saved())));
    assert_eq!(uses(&app), USES);

    let (mut app, _dir) = app_with(Record {
        uses: 0,
        ..asking(day("2026-11-01"))
    });
    let key = |app: &App, format| CopyAsKey {
        epoch: app.tab.file_epoch,
        revision: app.tab.revision,
        format,
        selected: vec![],
    };
    let _ = app.receive(Message::CopyAsWritten(
        key(&app, CopyFormat::Smiles),
        false,
        Ok(vec![]),
    ));
    let _ = app.receive(Message::CopyAsWritten(
        key(&app, CopyFormat::Png),
        true,
        Err("Busy".into()),
    ));
    // Without native clipboard support, Copy As SVG puts its source on the
    // clipboard as text.
    let _ = app.receive(Message::CopyAsWritten(
        key(&app, CopyFormat::Svg),
        false,
        Ok(vec![]),
    ));
    assert_eq!(uses(&app), 0, "Text copies and failed copies do not count");
    let _ = app.receive(Message::CopyAsWritten(
        key(&app, CopyFormat::Svg),
        true,
        Ok(vec![]),
    ));
    assert_eq!(uses(&app), 1);
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
    let _ = app.receive(written(true));
    assert_eq!(uses(&app), 2);
    // Copy puts a picture on the clipboard, except on Windows.
    let _ = app.receive(written(false));
    assert_eq!(uses(&app), 2 + u32::from(!cfg!(windows)));
    let before = uses(&app);

    // A copy that finishes after its drawing was replaced, or an export after
    // its tab closed, still counts, once.
    let mut replaced = key(&app, CopyFormat::Png);
    replaced.epoch += 1;
    let id = app.tab.id;
    let _ = app.receive(Message::Tab(
        id,
        Box::new(Message::CopyAsWritten(replaced, true, Ok(vec![]))),
    ));
    assert_eq!(uses(&app), before + 1);
    let _ = app.close_active_tab();
    assert_ne!(app.tab.id, id);
    let _ = app.receive(Message::Tab(
        id,
        Box::new(Message::FigureExported(Ok(saved()))),
    ));
    assert_eq!(uses(&app), before + 2);
}

#[test]
fn the_toast_waits_for_a_quiet_moment_and_asks_once_per_run() {
    let today = today();
    let (mut app, _dir) = app_with(asking(today - Days::new(1)));
    assert!(app.star.waiting());
    let _ = app.update(Message::Star(Action::Event(Event::Keyboard(
        iced::keyboard::Event::ModifiersChanged(iced::keyboard::Modifiers::empty()),
    ))));
    let _ = app.update(Message::Star(Action::Checked { focused: false }));
    assert!(!app.star.open, "Input within 30 seconds");
    app.help_open = true;
    assert!(!tick(&mut app), "Help is open");
    app.help_open = false;
    idle(&mut app);
    let _ = app.update(Message::Star(Action::Checked { focused: true }));
    assert!(!app.star.open, "A field or control has the keyboard");
    app.tab.office = Some(crate::session::Binding::standalone(
        "Drawing in Word.rsk".into(),
        crate::session::Host::LibreOffice,
    ));
    assert!(!tick(&mut app), "The drawing belongs to LibreOffice");
    app.tab.office = None;
    event(&mut app, Event::Window(window::Event::Unfocused));
    assert!(!tick(&mut app), "The window is in the background");
    event(&mut app, Event::Window(window::Event::Focused));
    event(
        &mut app,
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
    );
    assert!(!tick(&mut app), "A mouse button is held, as in a drag");
    event(
        &mut app,
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
    );
    assert!(tick(&mut app));
    assert!(
        !app.star.waiting(),
        "No listener or timer once it has asked"
    );

    let _ = app.update(Message::Star(Action::Dismiss));
    assert!(!app.star.open);
    let asked = Answer::Asked {
        version: CURRENT_VERSION.into(),
        day: today,
    };
    assert_eq!(app.star.record.as_ref().unwrap().state, asked);
    assert_eq!(saved_record(&app).unwrap().state, asked);
    assert!(!tick(&mut app), "At most once per run");
}

#[test]
fn focus_and_held_input_are_followed_before_the_count_is_reached() {
    let (mut app, _dir) = app_with(Record {
        uses: USES - 1,
        ..asking(today() - Days::new(1))
    });
    let moved = Event::Mouse(mouse::Event::CursorMoved {
        position: iced::Point::ORIGIN,
    });
    let touched = Event::Touch(touch::Event::FingerPressed {
        id: touch::Finger(1),
        position: iced::Point::ORIGIN,
    });
    // Until the count is reached, only focus and held buttons.
    for (event, held) in [
        (Event::Window(window::Event::Unfocused), true),
        (
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            true,
        ),
        (touched, false),
        (moved, false),
    ] {
        assert_eq!(input(event.clone(), false).is_some(), held, "{event:?}");
        assert!(input(event, true).is_some());
    }
    assert!(
        input(
            Event::Window(window::Event::Moved(iced::Point::ORIGIN)),
            true
        )
        .is_none()
    );

    // The window goes to the background, then an export reaches the count.
    event(&mut app, Event::Window(window::Event::Unfocused));
    let _ = app.receive(Message::FigureExported(Ok(Some(Saved {
        path: "Molecule.png".into(),
        details: vec![],
    }))));
    assert!(
        app.star
            .record
            .as_ref()
            .unwrap()
            .may_ask(today(), CURRENT_VERSION)
    );
    assert!(!tick(&mut app), "Never in the background");
    event(&mut app, Event::Window(window::Event::Focused));
    assert!(tick(&mut app));
}

#[test]
fn menus_and_panels_keep_the_toast_away() {
    type Open = fn(&mut App);
    let cases: [(&str, Open); 5] = [
        ("Tabs ▾ menu", |app| app.tabs.menu = true),
        ("Assistant menu", |app| {
            app.assistant.menu = Some(crate::app::assistant::Menu::Models);
        }),
        ("Floating NMR palette", |app| app.tab.nmr.open = true),
        ("Insert menu", |app| app.imports.menu = true),
        ("Updates", |app| app.updates.open = true),
    ];
    for (name, open) in cases {
        let (mut app, _dir) = app_with(asking(today() - Days::new(1)));
        assert!(!app.star_busy());
        open(&mut app);
        assert!(app.star_busy(), "{name}");
        assert!(!tick(&mut app), "{name}");
    }
}

#[test]
fn the_second_ask_or_a_star_ends_the_asks() {
    let first = day("2026-01-01");
    let mut record = asking(first);
    assert!(record.ask(first + Days::new(1), "0.1.0"));
    let asked = Answer::Asked {
        version: "0.1.0".into(),
        day: first + Days::new(1),
    };
    assert_eq!(record.state, asked);
    assert!(!record.ask(first + Days::new(2), "0.1.0"));
    assert_eq!(record.state, asked);
    assert!(record.ask(first + Days::new(91), "0.2.0"));
    assert_eq!(record.state, Answer::Done);
    assert!(!record.ask(first + Days::new(900), "9.0.0"));

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

    fn focus(&mut self, app: &App, id: &str) {
        let mut focus: Box<dyn iced::advanced::widget::Operation> =
            Box::new(reshiki::accessibility::FocusControl::new(id));
        loop {
            self.operate(app, focus.as_mut());
            match focus.finish() {
                iced::advanced::widget::operation::Outcome::Chain(next) => focus = next,
                _ => break,
            }
        }
        assert_eq!(self.focused(app), [id]);
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

    /// Updates the app, then applies the widget operations its task returns
    /// and the toast's own messages that they produce.
    async fn update(&mut self, app: &mut App, message: Message) {
        use iced::advanced::widget::operation::Outcome;
        use iced::futures::StreamExt;
        let mut queue = vec![message];
        while let Some(message) = queue.pop() {
            let Some(mut stream) = iced_runtime::task::into_stream(app.update(message)) else {
                continue;
            };
            while let Some(action) = stream.next().await {
                match action {
                    iced_runtime::Action::Widget(operation) => {
                        let mut current = Some(operation);
                        while let Some(mut operation) = current.take() {
                            self.operate(app, operation.as_mut());
                            if let Outcome::Chain(next) = operation.finish() {
                                current = Some(next);
                            }
                        }
                    }
                    iced_runtime::Action::Output(
                        message @ Message::Star(Action::Checked { .. }),
                    ) => {
                        queue.push(message);
                    }
                    _ => {}
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
    // Not while a field has the keyboard.
    let _ = app.update(Message::Inspector(crate::app::InspectorTab::Import));
    ui.focus(&app, import::INPUT);
    idle(&mut app);
    ui.update(&mut app, Message::Star(Action::Tick)).await;
    assert!(!app.star.open, "The Import field has the keyboard");
    ui.operate(
        &app,
        &mut iced::advanced::widget::operation::focusable::unfocus::<()>(),
    );
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
#[ignore = "Opt-in renderer check"]
async fn the_updates_link_stars_reshiki_on_github() {
    let mut ui = Ui::new().await;
    let (mut app, _dir) = app_with(asking(today() - Days::new(1)));
    app.updates.open = true;
    ui.focus(&app, "updates-star");
    let messages = ui.key(&app, iced::keyboard::key::Named::Enter);
    assert!(matches!(
        messages.as_slice(),
        [Message::Star(Action::Star { toast: false })]
    ));
    // The task that opens the browser is dropped here, not run.
    for message in messages {
        let _ = app.update(message);
    }
    assert_eq!(saved_record(&app).unwrap().state, Answer::Starred);
    assert!(!app.star.waiting(), "The toast never asks");
    assert!(app.updates.open, "About stays open");
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
        ui.update(&mut app, Message::Star(Action::Tick)).await;
        assert!(app.star.open);
        ui.picture(&mut app, &directory.join(format!("toast-{theme}.png")));
        let _ = app.update(Message::Star(Action::Dismiss));
        app.updates.open = true;
        ui.picture(&mut app, &directory.join(format!("about-{theme}.png")));
    }
}
