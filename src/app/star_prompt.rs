//! The "Like ReShiki?" toast, which asks once for a GitHub star after real
//! use, and again at most once after a later feature release. Its record is
//! `star-prompt.json` in the data folder; nothing is sent anywhere.
use super::{App, Message, workspace::muted_text};
use chrono::NaiveDate;
use iced::time::{Duration, Instant};
use iced::widget::{canvas, column, container, opaque, row, text};
use iced::{Alignment, Border, Color, Element, Length, Subscription, Task, Theme};
use reshiki::updates::CURRENT_VERSION;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub(super) const REPOSITORY: &str = "https://github.com/Ameyanagi/ReShiki";
#[cfg(not(target_arch = "wasm32"))]
const OWNER: &str = "Ameyanagi";
#[cfg(not(target_arch = "wasm32"))]
const NAME: &str = "ReShiki";
const THANKS: &str = "Opened GitHub in your browser. Thank you!";
#[cfg(not(target_arch = "wasm32"))]
/// Use after the first GitHub sign-in before the card asks once more.
#[cfg(not(target_arch = "wasm32"))]
const AFTER_SIGN_IN: Duration = Duration::from_secs(5 * 60);
/// Exports and copies before the first ask.
const USES: u32 = 5;
const IDLE: Duration = Duration::from_secs(30);
const TICK: Duration = Duration::from_secs(5);
/// Days after the first ask before a feature release may ask again.
const REASK_DAYS: i64 = 90;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Answer {
    Asking,
    Starred,
    /// Asked once, in this version on this day, without a star: closed, "Not
    /// now", or still open when ReShiki quit.
    Asked {
        version: String,
        day: NaiveDate,
    },
    /// Asked twice: never again.
    Done,
}

/// The one extra ask after the first GitHub sign-in, apart from the two
/// asks after use.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum SignedInAsk {
    #[default]
    NotYet,
    /// Signed in: ask after a few minutes of use.
    Due,
    Asked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Record {
    pub first_launch_day: NaiveDate,
    /// Successful exports and copies, counted up to [`USES`].
    pub uses: u32,
    pub state: Answer,
    #[serde(default)]
    pub signed_in_ask: SignedInAsk,
}

impl Record {
    fn path() -> Result<PathBuf, String> {
        Ok(reshiki::compatibility::data_directory()?.join("star-prompt.json"))
    }
    fn new(today: NaiveDate) -> Self {
        Self {
            first_launch_day: today,
            uses: 0,
            state: Answer::Asking,
            signed_in_ask: SignedInAsk::NotYet,
        }
    }
    /// The saved record, or a new one from today. `None`, for a file this
    /// build cannot read or a data folder it cannot write, never asks.
    fn load(path: &Path, today: NaiveDate) -> Option<Self> {
        match reshiki::storage::read(path) {
            Ok(bytes) => Self::parse(&bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let record = Self::new(today);
                record.save(path).map(|()| record)
            }
            Err(_) => None,
        }
    }
    /// `None` for a file this build cannot read, such as a newer build's.
    fn parse(bytes: &[u8]) -> Option<Self> {
        serde_json::from_slice(bytes).ok()
    }
    fn save(&self, path: &Path) -> Option<()> {
        reshiki::storage::create_dir_all(path.parent()?).ok()?;
        reshiki::storage::write_atomic(path, &serde_json::to_vec(self).ok()?).ok()
    }
    /// Everything that the record, the day and the version decide.
    pub(super) fn may_ask(&self, today: NaiveDate, version: &str) -> bool {
        self.uses >= USES
            && today > self.first_launch_day
            && match &self.state {
                Answer::Asking => true,
                Answer::Asked { version: old, day } => {
                    feature_release(old, version) && (today - *day).num_days() >= REASK_DAYS
                }
                Answer::Starred | Answer::Done => false,
            }
    }
    /// Records an ask, if one is due: the first notes the version and day,
    /// the second ends the asks.
    fn ask(&mut self, today: NaiveDate, version: &str) -> bool {
        if !self.may_ask(today, version) {
            return false;
        }
        self.state = match self.state {
            Answer::Asking => Answer::Asked {
                version: version.into(),
                day: today,
            },
            _ => Answer::Done,
        };
        true
    }
}

/// A stable feature release (x.y.0) with a higher major or minor version than
/// `old`.
fn feature_release(old: &str, version: &str) -> bool {
    let (Ok(old), Ok(new)) = (semver::Version::parse(old), semver::Version::parse(version)) else {
        return false;
    };
    new.pre.is_empty() && new.patch == 0 && (new.major, new.minor) > (old.major, old.minor)
}

/// What the window is doing when the timer fires.
pub(super) struct Moment<'a> {
    pub today: NaiveDate,
    pub version: &'a str,
    pub idle: Duration,
    pub focused: bool,
    /// A drag, text edit, menu, dialog or other overlay, or a drawing that
    /// Office or LibreOffice hosts.
    pub busy: bool,
}

/// Whether to show the toast now, once no field or control has keyboard focus.
pub(super) fn should_ask(record: Option<&Record>, moment: &Moment<'_>) -> bool {
    record.is_some_and(|record| record.may_ask(moment.today, moment.version))
        && moment.focused
        && !moment.busy
        && moment.idle >= IDLE
}

/// Whether `message` reports a file exported or a drawing copied, on every
/// platform and in any format. [`App::receive`] counts each once, before any
/// check of the drawing it came from.
pub(super) fn counts(message: &Message) -> bool {
    match message {
        Message::Tab(_, message) => counts(message),
        Message::Exported(Ok(Some(_)))
        | Message::FigureExported(Ok(Some(_)))
        | Message::Nmr(super::nmr::Action::Exported(Ok(Some(_))))
        | Message::ThemeFile(super::theme_files::Action::Saved(Ok(true)))
        | Message::DrawingStyle(super::document_styles::Action::Saved(Ok(true)))
        | Message::CopyAsWritten(_, Ok(_))
        | Message::ClipboardWritten { result: Ok(_), .. } => true,
        _ => false,
    }
}

fn today() -> NaiveDate {
    chrono::Local::now().date_naive()
}

#[derive(Debug, Clone)]
pub enum Action {
    /// Window focus, or pointer, keyboard or touch input.
    Event(iced::Event),
    Tick,
    /// Whether a field or control had keyboard focus when the timer fired.
    Checked {
        focused: bool,
    },
    /// Star on GitHub, in the toast or in About.
    Star {
        toast: bool,
    },
    /// Not now or ×.
    Dismiss,
    Opened(Result<(), String>),
    /// Whether GitHub says the user starred ReShiki; `None` when it could
    /// not tell.
    #[cfg(not(target_arch = "wasm32"))]
    IsStarred(Option<bool>),
    /// The one-click star through GitHub, from the toast or not.
    #[cfg(not(target_arch = "wasm32"))]
    Starred(Result<(), String>),
}

pub(super) struct State {
    path: Option<PathBuf>,
    /// The record as last read or saved; `None` asks no more in this run.
    record: Option<Record>,
    pub open: bool,
    last_input: Instant,
    focused: bool,
    /// A mouse button held down, as in a drag.
    pressed: bool,
    /// Signed in to GitHub since, while the sign-in ask is due.
    signed_in: Option<Instant>,
    /// Asking GitHub whether the user starred ReShiki.
    #[cfg(not(target_arch = "wasm32"))]
    checking: bool,
}

impl State {
    /// The browser editor, tests and an unreadable or unwritable file never ask.
    pub(super) fn load() -> Self {
        let path = if cfg!(any(test, target_arch = "wasm32")) {
            None
        } else {
            Record::path().ok()
        };
        Self::at(path)
    }

    fn at(path: Option<PathBuf>) -> Self {
        let record = path.as_deref().and_then(|path| Record::load(path, today()));
        Self {
            path,
            record,
            open: false,
            last_input: Instant::now(),
            focused: true,
            pressed: false,
            signed_in: None,
            #[cfg(not(target_arch = "wasm32"))]
            checking: false,
        }
    }

    /// Whether this run may still ask, perhaps after more uses or on a later day.
    fn waiting(&self) -> bool {
        self.signed_in.is_some()
            || self
                .record
                .as_ref()
                .is_some_and(|record| match &record.state {
                    Answer::Asking => true,
                    Answer::Asked { version, .. } => feature_release(version, CURRENT_VERSION),
                    Answer::Starred | Answer::Done => false,
                })
    }

    /// A successful export or copy.
    pub(super) fn used(&mut self) {
        self.last_input = Instant::now();
        if self
            .record
            .as_ref()
            .is_some_and(|record| record.uses < USES)
        {
            self.change(|record| {
                let counted = record.uses < USES;
                record.uses += u32::from(counted);
                counted
            });
        }
    }

    /// Reads the saved record again, so that another running ReShiki's count
    /// or answer stays, applies `edit` and saves it if `edit` returns true. A
    /// record that cannot be read or saved is dropped, so this run asks no
    /// more. Returns whether `edit` changed it.
    fn change(&mut self, edit: impl FnOnce(&mut Record) -> bool) -> bool {
        let (Some(_), Some(path)) = (self.record.take(), self.path.as_deref()) else {
            return false;
        };
        let Some(mut record) = Record::load(path, today()) else {
            return false;
        };
        let changed = edit(&mut record);
        if changed && record.save(path).is_none() {
            return false;
        }
        self.record = Some(record);
        changed
    }
}

/// The events the toast's timing needs: focus and held buttons always, and
/// with `all` every other input too.
fn input(event: iced::Event, all: bool) -> Option<Message> {
    use iced::{Event, mouse, window};
    let held = matches!(
        event,
        Event::Window(window::Event::Focused | window::Event::Unfocused)
            | Event::Mouse(mouse::Event::ButtonPressed(_) | mouse::Event::ButtonReleased(_))
    );
    let other = matches!(
        event,
        Event::Mouse(_) | Event::Keyboard(_) | Event::Touch(_)
    );
    (held || all && other).then_some(Message::Star(Action::Event(event)))
}

impl App {
    /// Focus and held buttons are followed while this run may still ask, so
    /// that the toast never opens in the background or mid-drag; every input
    /// and the timer once the uses are reached.
    pub(super) fn star_subscription(&self) -> Subscription<Message> {
        if !self.star.waiting() {
            return Subscription::none();
        }
        if self.star.signed_in.is_some()
            || self
                .star
                .record
                .as_ref()
                .is_some_and(|record| record.uses >= USES)
        {
            Subscription::batch([
                iced::event::listen_with(|event, _, _| input(event, true)),
                iced::time::every(TICK).map(|_| Message::Star(Action::Tick)),
            ])
        } else {
            iced::event::listen_with(|event, _, _| input(event, false))
        }
    }

    pub(super) fn star_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Event(event) => {
                use iced::{Event, mouse, window};
                let star = &mut self.star;
                star.last_input = Instant::now();
                match event {
                    Event::Window(window::Event::Focused) => star.focused = true,
                    Event::Window(window::Event::Unfocused) => star.focused = false,
                    Event::Mouse(mouse::Event::ButtonPressed(_)) => star.pressed = true,
                    Event::Mouse(mouse::Event::ButtonReleased(_)) => star.pressed = false,
                    _ => {}
                }
            }
            Action::Tick => {
                if self.star_ready() {
                    use iced::advanced::widget::{operate, operation::focusable};
                    return operate(focusable::count()).map(|count| {
                        Message::Star(Action::Checked {
                            focused: count.focused.is_some(),
                        })
                    });
                }
            }
            Action::Checked { focused } => {
                if focused || !self.star_ready() {
                    return Task::none();
                }
                // Signed in: never ask someone who already starred.
                #[cfg(not(target_arch = "wasm32"))]
                if let Some(client) = self.github.client() {
                    if !std::mem::replace(&mut self.star.checking, true) {
                        return Task::perform(
                            async move { reshiki::github::api::starred(&client, OWNER, NAME).await },
                            |result| Message::Star(Action::IsStarred(result.ok())),
                        );
                    }
                    return Task::none();
                }
                return self.star_show();
            }
            #[cfg(not(target_arch = "wasm32"))]
            Action::IsStarred(starred) => {
                // GitHub could not tell: no ask in this run, as `checking`
                // stays set; a later launch asks GitHub again.
                let Some(starred) = starred else {
                    return Task::none();
                };
                self.star.checking = false;
                if starred {
                    self.star.signed_in = None;
                    self.star.change(|record| {
                        record.state = Answer::Starred;
                        true
                    });
                } else if self.star_ready() {
                    return self.star_show();
                }
            }
            #[cfg(not(target_arch = "wasm32"))]
            Action::Starred(result) => match result {
                Ok(()) => {
                    self.github.starred = true;
                    self.status = starred(self.star_login().as_deref());
                    self.error = false;
                    // About's link is named as it now shows.
                    return self.accessibility_refresh();
                }
                // As when signed out: the repository page in the browser.
                Err(_) => {
                    return Task::perform(
                        reshiki::updates::open_url(REPOSITORY.into()),
                        |result| Message::Star(Action::Opened(result)),
                    );
                }
            },
            Action::Star { toast } => {
                self.star.open = false;
                self.star.signed_in = None;
                self.star.change(|record| {
                    record.state = Answer::Starred;
                    true
                });
                // Signed in: one click stars it, with no browser.
                #[cfg(not(target_arch = "wasm32"))]
                if let Some(client) = self.github.client() {
                    let star = Task::perform(
                        async move {
                            reshiki::github::api::star(&client, OWNER, NAME)
                                .await
                                .map_err(|error| error.to_string())
                        },
                        |result| Message::Star(Action::Starred(result)),
                    );
                    return if toast {
                        Task::batch([star, super::focus_drawing(), self.accessibility_refresh()])
                    } else {
                        star
                    };
                }
                let open = Task::perform(reshiki::updates::open_url(REPOSITORY.into()), |result| {
                    Message::Star(Action::Opened(result))
                });
                return if toast {
                    Task::batch([open, super::focus_drawing(), self.accessibility_refresh()])
                } else {
                    open
                };
            }
            Action::Dismiss => {
                self.star.open = false;
                return Task::batch([super::focus_drawing(), self.accessibility_refresh()]);
            }
            Action::Opened(result) => {
                self.error = result.is_err();
                self.status = result.err().unwrap_or_else(|| THANKS.into());
            }
        }
        Task::none()
    }

    /// Every rule but keyboard focus, which a tick checks in the view.
    fn star_ready(&self) -> bool {
        let moment = Moment {
            today: today(),
            version: CURRENT_VERSION,
            idle: self.star.last_input.elapsed(),
            focused: self.star.focused,
            busy: self.star_busy(),
        };
        should_ask(self.star.record.as_ref(), &moment) || self.star_sign_in_due(&moment)
    }

    /// The ask a few minutes after the first GitHub sign-in: once, even
    /// after Not now, never after a star.
    fn star_sign_in_due(&self, moment: &Moment<'_>) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        return self
            .star
            .signed_in
            .is_some_and(|since| since.elapsed() >= AFTER_SIGN_IN)
            && self.star.record.as_ref().is_some_and(|record| {
                record.signed_in_ask == SignedInAsk::Due && record.state != Answer::Starred
            })
            && moment.focused
            && !moment.busy
            && moment.idle >= IDLE;
        #[cfg(target_arch = "wasm32")]
        {
            let _ = moment;
            false
        }
    }

    /// Shows the card, saving the ask first, so that quitting without an
    /// answer still counts and a folder that cannot keep the answer is never
    /// asked. The asks after use come first; the sign-in ask counts apart.
    fn star_show(&mut self) -> Task<Message> {
        if self.star.open {
            return Task::none();
        }
        let today = today();
        let used = self
            .star
            .change(|record| record.ask(today, CURRENT_VERSION));
        let asked = used
            || self.star.signed_in.is_some()
                && self.star.change(|record| {
                    let due = record.signed_in_ask == SignedInAsk::Due;
                    record.signed_in_ask = SignedInAsk::Asked;
                    due
                });
        if !asked {
            return Task::none();
        }
        // A sign-in ask still due waits its few minutes again after this card.
        self.star.signed_in = self.star.signed_in.filter(|_| used).map(|_| Instant::now());
        self.star.open = true;
        self.accessibility_refresh()
    }

    /// After the first GitHub sign-in on this computer, the card may ask once
    /// more; see [`Self::star_sign_in_due`].
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn star_github_signed_in(&mut self) {
        self.star.change(|record| {
            let first = record.signed_in_ask == SignedInAsk::NotYet;
            if first {
                record.signed_in_ask = SignedInAsk::Due;
            }
            first
        });
        self.star_github_ready();
    }

    /// Signed in, now or at launch: start the few minutes before the
    /// sign-in ask, if it is still due.
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn star_github_ready(&mut self) {
        let due = self.star.record.as_ref().is_some_and(|record| {
            record.signed_in_ask == SignedInAsk::Due && record.state != Answer::Starred
        });
        if due && self.star.signed_in.is_none() {
            self.star.signed_in = Some(Instant::now());
        }
    }

    /// The signed-in GitHub account, which a star is made with.
    fn star_login(&self) -> Option<String> {
        #[cfg(not(target_arch = "wasm32"))]
        return self.github.login().map(str::to_owned);
        #[cfg(target_arch = "wasm32")]
        None
    }

    /// The Star link in About, which is also its accessible name, and its
    /// tooltip: signed in, one click stars it with that account and no
    /// browser opens.
    pub(super) fn star_link(&self) -> (String, Option<String>) {
        #[cfg(not(target_arch = "wasm32"))]
        if self.github.starred {
            return ("★ Starred on GitHub".into(), None);
        }
        match self.star_login() {
            Some(login) => (
                "★ Star on GitHub".into(),
                Some(format!("Stars ReShiki on GitHub as @{login}")),
            ),
            None => ("★ Star on GitHub ↗".into(), None),
        }
    }

    /// Anything the toast must not interrupt, besides a focused field.
    fn star_busy(&self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        if self.github.menu {
            return true;
        }
        self.star.pressed
            || self.tab.office.is_some()
            || self.prompt.is_some()
            || self.pending.is_some()
            || self.recovery_manager.open()
            || self.updates.open
            || self.github_dialog()
            || self.help_open
            || self.commands.open
            || self.view_open
            || self.palette.is_some()
            || self.context_menu.is_some()
            || self.style_menu.is_some()
            || self.tabs.menu
            || self.tabs.color_menu.is_some()
            || self.assistant.menu.is_some()
            || self.assistant.viewed_image.is_some()
            || self.theme_library.editor.is_some()
            || self.imports.menu
            || self.imports.examples_menu
            || self.tab.nmr.overlay_open()
            || self.tab.inspector_ui.menu_open()
            || self.tab.atom_text.is_some()
            || self.tab.inline_text.is_some()
            || self.tab.joining.is_some()
    }

    /// The toast floats at the canvas's bottom right. The stack stays when it
    /// closes, so the canvas keeps its state.
    pub(super) fn with_star_prompt<'a>(
        &'a self,
        base: Element<'a, Message>,
    ) -> Element<'a, Message> {
        // Hidden while a GitHub dialog or popover is open; it comes back after.
        let toast = (self.star.open && !self.github_dialog()).then(|| {
            container(opaque(toast(self.star_login())))
                .align_right(Length::Fill)
                .align_bottom(Length::Fill)
                .padding(18)
                .into()
        });
        iced::widget::Stack::with_children([base])
            .extend(toast)
            .into()
    }
}

/// The status after a one-click star.
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn starred(login: Option<&str>) -> String {
    match login {
        Some(login) => format!("Starred ReShiki on GitHub as @{login}. Thank you!"),
        None => "Starred ReShiki on GitHub. Thank you!".into(),
    }
}

/// The card says plainly what Star does: signed in, it stars with that
/// account; signed out, it opens GitHub in the browser.
fn toast<'a>(login: Option<String>) -> Element<'a, Message> {
    let button = |id, label: String, content: Element<'a, Message>, action| {
        reshiki::accessibility::button(id, label, content)
            .on_press(Message::Star(action))
            .padding([7, 12])
    };
    let badge = container(star(18., false)).center(34).style(|theme| {
        crate::appearance::container(
            theme,
            container::Style {
                background: Some(Color::from_rgb8(255, 244, 214).into()),
                border: Border {
                    radius: 9.0.into(),
                    ..Border::default()
                },
                ..container::Style::default()
            },
        )
    });
    let title = text("Like ReShiki?").size(14).font(iced::Font {
        weight: iced::font::Weight::Bold,
        ..iced::Font::with_name(reshiki::style::ui_font_family())
    });
    // Signed in, a click stars with that account; signed out, a browser opens.
    let (label, note) = match &login {
        Some(login) => (
            "Star on GitHub".to_owned(),
            format!("Stars ReShiki on GitHub as @{login}."),
        ),
        None => (
            "Star on GitHub ↗".to_owned(),
            "Opens GitHub in your browser.".to_owned(),
        ),
    };
    let body = text("A star on GitHub helps other chemists find it.")
        .size(13)
        .style(muted_text);
    let close = reshiki::accessibility::button("star-close", "Close", text("×").size(16))
        .on_press(Message::Star(Action::Dismiss))
        .padding([0, 6])
        .style(super::workspace::control(false));
    let actions = row![
        button(
            "star-not-now",
            "Not now".into(),
            text("Not now").size(12).into(),
            Action::Dismiss
        )
        .style(crate::appearance::secondary),
        button(
            "star-github",
            label.clone(),
            row![star(14., true), text(label).size(12)]
                .spacing(6)
                .align_y(Alignment::Center)
                .into(),
            Action::Star { toast: true }
        )
        .style(crate::appearance::primary),
    ]
    .spacing(8);
    let actions = column![
        container(actions).align_right(Length::Fill),
        container(text(note).size(11).style(muted_text)).align_right(Length::Fill),
    ]
    .spacing(6);
    container(
        column![
            row![
                badge,
                column![title, body].spacing(3).width(Length::Fill),
                close
            ]
            .spacing(12),
            actions,
        ]
        .spacing(12),
    )
    .padding([14, 16])
    .width(330)
    .style(|theme| {
        crate::appearance::container(
            theme,
            container::Style {
                background: Some(Color::WHITE.into()),
                border: Border {
                    radius: 12.0.into(),
                    width: 1.,
                    color: Color::from_rgb8(207, 216, 216),
                },
                shadow: crate::appearance::surface_shadow(iced::Shadow {
                    color: Color::from_rgba8(20, 40, 35, 0.18),
                    offset: iced::Vector::new(0., 8.),
                    blur_radius: 30.,
                }),
                ..container::Style::default()
            },
        )
    })
    .into()
}

/// The five-pointed star: gold in the badge, or an outline in the button's
/// text color.
fn star<'a>(size: f32, outline: bool) -> Element<'a, Message> {
    canvas(Star(outline)).width(size).height(size).into()
}

struct Star(bool);

impl<M> canvas::Program<M> for Star {
    type State = ();
    fn draw(
        &self,
        _: &(),
        renderer: &iced::Renderer,
        theme: &Theme,
        bounds: iced::Rectangle,
        _: iced::mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        // The mockup's 24-unit star.
        const POINTS: [(f32, f32); 10] = [
            (12., 3.),
            (14.8, 8.8),
            (21., 9.7),
            (16.5, 14.1),
            (17.6, 20.4),
            (12., 17.4),
            (6.4, 20.4),
            (7.5, 14.1),
            (3., 9.7),
            (9.2, 8.8),
        ];
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let scale = bounds.width / 24.;
        let path = canvas::Path::new(|p| {
            for (i, (x, y)) in POINTS.into_iter().enumerate() {
                let point = iced::Point::new(x * scale, y * scale);
                if i == 0 {
                    p.move_to(point);
                } else {
                    p.line_to(point);
                }
            }
            p.close();
        });
        let stroke = |color, width| {
            canvas::Stroke::default()
                .with_color(color)
                .with_width(width * scale)
                .with_line_join(canvas::LineJoin::Round)
        };
        if self.0 {
            let ink = theme.extended_palette().primary.base.text;
            frame.stroke(&path, stroke(ink, 2.));
        } else {
            let themed = |c| crate::appearance::themed(theme, c);
            frame.fill(&path, themed(Color::from_rgb8(224, 161, 0)));
            frame.stroke(&path, stroke(themed(Color::from_rgb8(196, 138, 0)), 1.2));
        }
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests;
