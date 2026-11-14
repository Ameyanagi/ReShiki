//! The "Enjoying ReShiki?" toast, which asks once for a GitHub star after real
//! use, and again at most once after a later feature release. Its record is
//! `star-prompt.json` in the data folder; nothing is sent anywhere.
use super::{App, Message, workspace::muted_text};
use chrono::NaiveDate;
use iced::time::{Duration, Instant};
use iced::widget::{canvas, column, container, opaque, row, text};
use iced::{Alignment, Border, Color, Element, Length, Subscription, Task, Theme};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub(super) const REPOSITORY: &str = "https://github.com/Ameyanagi/ReShiki";
const THANKS: &str = "Opened GitHub in your browser. Thank you!";
/// Exports and picture copies before the first ask.
const USES: u32 = 5;
const IDLE: Duration = Duration::from_secs(30);
const TICK: Duration = Duration::from_secs(5);
/// Days after a "Not now" before a feature release may ask again.
const REASK_DAYS: i64 = 90;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Answer {
    Asking,
    Starred,
    /// "Not now" or × on the first ask, in this version on this day.
    Dismissed {
        version: String,
        day: NaiveDate,
    },
    /// Closed on the second ask: never again.
    Done,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Record {
    pub first_launch_day: NaiveDate,
    /// Successful exports and picture copies, counted up to [`USES`].
    pub uses: u32,
    pub state: Answer,
}

impl Record {
    fn path() -> Result<PathBuf, String> {
        Ok(reshiki::compatibility::data_directory()?.join("star-prompt.json"))
    }
    /// The record, a new one from today, or `None` for a file this build
    /// cannot read or a data folder it cannot write: then it never asks.
    fn load(path: &Path, today: NaiveDate) -> Option<Self> {
        match reshiki::storage::read(path) {
            Ok(bytes) => Self::parse(&bytes),
            Err(_) if path.exists() => None,
            Err(_) => {
                let record = Self {
                    first_launch_day: today,
                    uses: 0,
                    state: Answer::Asking,
                };
                record.save(path).ok().map(|()| record)
            }
        }
    }
    /// `None` for a file this build cannot read, such as a newer build's.
    fn parse(bytes: &[u8]) -> Option<Self> {
        serde_json::from_slice(bytes).ok()
    }
    fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            reshiki::storage::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        // A file this build cannot read is replaced only once a copy of it is kept.
        if let Ok(old) = reshiki::storage::read(path)
            && Self::parse(&old).is_none()
        {
            reshiki::storage::write_atomic(&path.with_extension("json.bad"), &old)?;
        }
        reshiki::storage::write_atomic(path, &serde_json::to_vec(self).map_err(|e| e.to_string())?)
    }
    /// Everything that the record, the day and the version decide.
    pub(super) fn may_ask(&self, today: NaiveDate, version: &str) -> bool {
        self.uses >= USES
            && today > self.first_launch_day
            && match &self.state {
                Answer::Asking => true,
                Answer::Dismissed { version: old, day } => {
                    feature_release(old, version) && (today - *day).num_days() >= REASK_DAYS
                }
                Answer::Starred | Answer::Done => false,
            }
    }
}

/// A stable release with a higher major or minor version than `old`.
fn feature_release(old: &str, version: &str) -> bool {
    let (Ok(old), Ok(new)) = (semver::Version::parse(old), semver::Version::parse(version)) else {
        return false;
    };
    new.pre.is_empty() && (new.major, new.minor) > (old.major, old.minor)
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

/// Whether to show the toast now.
pub(super) fn should_ask(record: Option<&Record>, moment: &Moment<'_>) -> bool {
    record.is_some_and(|record| record.may_ask(moment.today, moment.version))
        && moment.focused
        && !moment.busy
        && moment.idle >= IDLE
}

fn today() -> NaiveDate {
    chrono::Local::now().date_naive()
}

#[derive(Debug, Clone)]
pub enum Action {
    /// Keyboard, pointer or scroll input; `Some` for a mouse button press or release.
    Input(Option<bool>),
    Focused(bool),
    Tick,
    /// Star on GitHub, in the toast or in About.
    Star {
        toast: bool,
    },
    /// Not now or ×.
    Dismiss,
    Opened(Result<(), String>),
}

pub(super) struct State {
    path: Option<PathBuf>,
    record: Option<Record>,
    /// Asked in this run: never twice.
    asked: bool,
    pub open: bool,
    last_input: Instant,
    focused: bool,
    pressed: bool,
}

impl State {
    /// The browser editor, tests and an unreadable or unwritable file never ask.
    pub(super) fn load() -> Self {
        let path = if cfg!(any(test, target_arch = "wasm32")) {
            None
        } else {
            Record::path().ok()
        };
        let record = path.as_deref().and_then(|path| Record::load(path, today()));
        Self {
            path,
            record,
            asked: false,
            open: false,
            last_input: Instant::now(),
            focused: true,
            pressed: false,
        }
    }

    /// Whether a later tick may still show the toast in this run.
    fn possible(&self) -> bool {
        !self.asked
            && self
                .record
                .as_ref()
                .is_some_and(|record| record.may_ask(today(), reshiki::updates::CURRENT_VERSION))
    }

    pub(super) fn subscription(&self) -> Subscription<Message> {
        if !self.possible() {
            return Subscription::none();
        }
        Subscription::batch([
            iced::time::every(TICK).map(|_| Message::Star(Action::Tick)),
            iced::event::listen_with(|event, _, _| {
                use iced::{Event, mouse, window};
                Some(Message::Star(match event {
                    Event::Window(window::Event::Focused) => Action::Focused(true),
                    Event::Window(window::Event::Unfocused) => Action::Focused(false),
                    Event::Mouse(mouse::Event::ButtonPressed(_)) => Action::Input(Some(true)),
                    Event::Mouse(mouse::Event::ButtonReleased(_)) => Action::Input(Some(false)),
                    Event::Mouse(_) | Event::Keyboard(_) | Event::Touch(_) => Action::Input(None),
                    _ => return None,
                }))
            }),
        ])
    }

    /// A successful export or picture copy.
    pub(super) fn used(&mut self) {
        self.last_input = Instant::now();
        if self
            .record
            .as_ref()
            .is_some_and(|record| record.uses < USES)
        {
            self.change(|record| record.uses += 1);
        }
    }

    /// Applies `edit` and saves. A record that cannot be saved is dropped, so
    /// this run asks no more.
    fn change(&mut self, edit: impl FnOnce(&mut Record)) {
        let (Some(record), Some(path)) = (self.record.as_mut(), self.path.as_deref()) else {
            return;
        };
        edit(record);
        if record.save(path).is_err() {
            self.record = None;
        }
    }
}

impl App {
    pub(super) fn star_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Input(pressed) => {
                self.star.last_input = Instant::now();
                if let Some(pressed) = pressed {
                    self.star.pressed = pressed;
                }
            }
            Action::Focused(focused) => {
                self.star.last_input = Instant::now();
                self.star.focused = focused;
            }
            Action::Tick => {
                let moment = Moment {
                    today: today(),
                    version: reshiki::updates::CURRENT_VERSION,
                    idle: self.star.last_input.elapsed(),
                    focused: self.star.focused,
                    busy: self.star_busy(),
                };
                if !self.star.asked && should_ask(self.star.record.as_ref(), &moment) {
                    self.star.asked = true;
                    self.star.open = true;
                    return self.accessibility_refresh();
                }
            }
            Action::Star { toast } => {
                self.star.open = false;
                self.star.change(|record| record.state = Answer::Starred);
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
                let today = today();
                self.star.open = false;
                self.star.change(|record| {
                    record.state = match record.state {
                        Answer::Asking => Answer::Dismissed {
                            version: reshiki::updates::CURRENT_VERSION.into(),
                            day: today,
                        },
                        _ => Answer::Done,
                    }
                });
                return Task::batch([super::focus_drawing(), self.accessibility_refresh()]);
            }
            Action::Opened(result) => {
                self.error = result.is_err();
                self.status = result.err().unwrap_or_else(|| THANKS.into());
            }
        }
        Task::none()
    }

    /// Anything the toast must not interrupt.
    fn star_busy(&self) -> bool {
        self.star.pressed
            || self.tab.office.is_some()
            || self.prompt.is_some()
            || self.pending.is_some()
            || self.recovery_manager.open()
            || self.updates.open
            || self.help_open
            || self.commands.open
            || self.view_open
            || self.palette.is_some()
            || self.context_menu.is_some()
            || self.style_menu.is_some()
            || self.assistant.viewed_image.is_some()
            || self.theme_library.editor.is_some()
            || self.imports.menu
            || self.imports.examples_menu
            || self.tab.inspector_ui.menu_open()
            || self.tab.atom_text.is_some()
            || self.tab.inline_text.is_some()
            || self.tab.joining.is_some()
            || self.tab.cleanup.is_some()
    }

    /// The toast floats at the canvas's bottom right. The stack stays when it
    /// closes, so the canvas keeps its state.
    pub(super) fn with_star_prompt<'a>(
        &'a self,
        base: Element<'a, Message>,
    ) -> Element<'a, Message> {
        let toast = self.star.open.then(|| {
            container(opaque(toast()))
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

fn toast<'a>() -> Element<'a, Message> {
    let button = |id, label: &'static str, content: Element<'a, Message>, action| {
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
    let title = text("Enjoying ReShiki?").size(14).font(iced::Font {
        weight: iced::font::Weight::Bold,
        ..iced::Font::with_name(reshiki::style::ui_font_family())
    });
    let body = text(
        "A star on GitHub helps other chemists find it. ReShiki is free and built in the open.",
    )
    .size(13)
    .style(muted_text);
    let close = reshiki::accessibility::button("star-close", "Close", text("×").size(16))
        .on_press(Message::Star(Action::Dismiss))
        .padding([0, 6])
        .style(super::workspace::control(false));
    let actions = row![
        button(
            "star-not-now",
            "Not now",
            text("Not now").size(12).into(),
            Action::Dismiss
        )
        .style(crate::appearance::secondary),
        button(
            "star-github",
            "Star on GitHub",
            row![star(14., true), text("Star on GitHub").size(12)]
                .spacing(6)
                .align_y(Alignment::Center)
                .into(),
            Action::Star { toast: true }
        )
        .style(crate::appearance::primary),
    ]
    .spacing(8);
    container(
        column![
            row![
                badge,
                column![title, body].spacing(3).width(Length::Fill),
                close
            ]
            .spacing(12),
            container(actions).align_right(Length::Fill),
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
