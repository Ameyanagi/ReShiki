//! Help: a searchable reference of every shortcut, with a live before/after
//! drawing for contextual keys. Selecting a row only shows it; it never runs
//! the command.
use super::{
    App, Message,
    workspace::{control, muted_text},
};
use iced::keyboard::{Key, Modifiers, key::Named};
use iced::widget::{
    Space, column, container, mouse_area, opaque, rich_text, row, scrollable, stack, text,
};
use iced::{Alignment, Border, Color, Element, Length, Task};
use std::sync::LazyLock;

mod examples;
pub(super) mod registry;
pub(super) mod search;

use registry::{Entry, Example};

const SEARCH: &str = "help-search";
const RESULTS: &str = "help-results";

static ENTRIES: LazyLock<Vec<Entry>> = LazyLock::new(registry::entries);

pub(super) fn entries() -> &'static [Entry] {
    &ENTRIES
}

/// F1, or Command+? (Command+Shift+/). Command+/ is Fit.
pub(super) fn is_shortcut(key: &Key, modifiers: Modifiers) -> bool {
    (modifiers.is_empty() && matches!(key, Key::Named(Named::F1)))
        || (modifiers == Modifiers::COMMAND | Modifiers::SHIFT
            && matches!(key, Key::Character(c) if c == "/" || c == "?"))
}

/// Keys Help takes before its search field: closing and moving the selection.
pub(super) fn modal_key(key: &Key, modifiers: Modifiers) -> Option<Message> {
    if matches!(key, Key::Named(Named::Escape)) || is_shortcut(key, modifiers) {
        return Some(Message::ToggleHelp);
    }
    if !modifiers.is_empty() {
        return None;
    }
    let step = match key {
        Key::Named(Named::ArrowUp) => -1,
        Key::Named(Named::ArrowDown) => 1,
        Key::Named(Named::PageUp) => -8,
        Key::Named(Named::PageDown) => 8,
        _ => return None,
    };
    Some(Message::Help(Action::Move(step)))
}

#[derive(Debug, Clone)]
pub enum Action {
    Query(String),
    /// Move the selection by this many rows, stopping at either end.
    Move(isize),
    /// Select a registry entry; it is only shown, never run.
    Select(usize),
}

#[derive(Debug, Default)]
pub(super) struct State {
    pub(super) query: String,
    /// A registry index; None selects the best match.
    selected: Option<usize>,
}

impl State {
    pub(super) fn results(&self) -> Vec<usize> {
        search::rank(entries(), &self.query)
    }
    /// The selected row's position among `results`.
    pub(super) fn position(&self, results: &[usize]) -> Option<usize> {
        self.selected
            .and_then(|selected| results.iter().position(|i| *i == selected))
            .or((!results.is_empty()).then_some(0))
    }
    /// The selected registry index.
    fn current(&self, results: &[usize]) -> Option<usize> {
        self.position(results).and_then(|p| results.get(p).copied())
    }
    #[cfg(test)]
    pub(super) fn selected(&self) -> Option<&'static Entry> {
        self.current(&self.results()).and_then(|i| entries().get(i))
    }
}

impl App {
    pub(super) fn toggle_help(&mut self) -> Task<Message> {
        self.help_open = !self.help_open;
        if !self.help_open {
            return Task::none();
        }
        self.palette = None;
        self.commands.open = false;
        self.help = State::default();
        iced::widget::operation::focus(SEARCH)
    }

    pub(super) fn help_action(&mut self, action: Action) -> Task<Message> {
        if !self.help_open {
            return Task::none();
        }
        match action {
            Action::Query(query) => {
                self.help.query = query;
                self.help.selected = None;
                iced::widget::operation::snap_to(
                    RESULTS,
                    iced::widget::operation::RelativeOffset::START,
                )
            }
            Action::Select(index) => {
                if index < entries().len() {
                    self.help.selected = Some(index);
                }
                Task::none()
            }
            Action::Move(step) => {
                let results = self.help.results();
                let Some(position) = self.help.position(&results) else {
                    return Task::none();
                };
                let last = results.len().saturating_sub(1);
                let position = position.saturating_add_signed(step).min(last);
                self.help.selected = results.get(position).copied();
                iced::widget::operation::snap_to(
                    RESULTS,
                    iced::widget::operation::RelativeOffset {
                        x: 0.,
                        y: position as f32 / last.max(1) as f32,
                    },
                )
            }
        }
    }

    pub(super) fn with_help<'a>(&'a self, base: Element<'a, Message>) -> Element<'a, Message> {
        if !self.help_open {
            // Keep the workspace at the same child position while Help opens
            // and closes, so its inert focus and text selection survive.
            return stack![base].into();
        }
        let results = self.help.results();
        let current = self.help.current(&results);
        let selected = current.and_then(|i| entries().get(i));
        let name = match selected {
            Some(entry) => format!(
                "Search shortcuts; {} results; selected {}, {}, {}: {}",
                results.len(),
                entry.name,
                entry.keys,
                entry.scope.label(),
                entry.description
            ),
            None => "Search shortcuts; no results".into(),
        };
        let search = reshiki::accessibility::text_input(
            SEARCH,
            name,
            "Search by action, key or description (e.g. wedge, ⇧R, ring)",
            &self.help.query,
        )
        .style(crate::appearance::input_style)
        .on_input(|query| Message::Help(Action::Query(query)))
        .on_submit_maybe(current.map(|i| Message::Help(Action::Select(i))))
        .size(14)
        .padding(8);
        let mut list = column![].spacing(2);
        for &index in &results {
            if let Some(entry) = entries().get(index) {
                list = list.push(result_row(index, entry, Some(index) == current));
            }
        }
        if results.is_empty() {
            list = list.push(
                text("No shortcut matches. Try a key such as r, a command such as copy, or a group such as Boc.")
                    .size(12)
                    .style(muted_text),
            );
        }
        let popup = column![
                row![
                    column![
                        text("Help & shortcuts").size(22),
                        text("Search keys and commands; select one to see what it does")
                            .size(12)
                            .style(muted_text)
                    ]
                    .spacing(5),
                    Space::new().width(Length::Fill),
                    reshiki::accessibility::button(
                        "help-close",
                        "Close Help and shortcuts",
                        text("×").size(25).center()
                    )
                    .width(32)
                    .height(32)
                    .padding(0)
                    .on_press(Message::ToggleHelp)
                    .style(control(false)),
                ]
                .align_y(Alignment::Center)
                .spacing(12),
                search,
                row![
                    scrollable(list.padding([0, 12]))
                        .id(RESULTS)
                        .width(Length::Fill)
                        .height(Length::Fill),
                    preview(selected),
                ]
                .spacing(16)
                .height(Length::Fill),
                row![
                    text("↑ ↓ move · Esc closes · Uppercase means Shift-letter · Reference only: nothing runs")
                        .size(12)
                        .style(muted_text)
                        .width(Length::Fill),
                    reshiki::accessibility::button(
                        "help-examples",
                        "Open shortcut examples: one editable drawing with labeled examples, in a new tab",
                        text("Open shortcut examples").size(13)
                    )
                    .padding([7, 14])
                    .on_press(Message::OpenShortcutExamples)
                    .style(control(false)),
                    reshiki::accessibility::button(
                        "help-done",
                        "Done; close Help and shortcuts",
                        text("Done").size(13)
                    )
                    .padding([7, 18])
                    .on_press(Message::ToggleHelp)
                    .style(control(true))
                ]
                .spacing(12)
                .align_y(Alignment::Center),
            ]
            .spacing(16);
        modal(base, popup.into(), 900., 650., Message::ToggleHelp)
    }
}

/// A centered dialog over a dimmed, inert workspace; clicking outside it
/// sends `dismiss`. Help and the command palette share it.
pub(super) fn modal<'a>(
    base: Element<'a, Message>,
    content: Element<'a, Message>,
    width: f32,
    height: f32,
    dismiss: Message,
) -> Element<'a, Message> {
    let base = reshiki::accessibility::inert(base);
    let popup = container(content)
        .padding(24)
        .width(width)
        .height(height)
        .max_height(height)
        .style(|theme| {
            crate::appearance::container(
                theme,
                container::Style {
                    background: Some(Color::WHITE.into()),
                    border: Border {
                        radius: 14.into(),
                        width: 1.,
                        color: Color::from_rgb8(207, 216, 216),
                    },
                    shadow: crate::appearance::surface_shadow(iced::Shadow {
                        color: Color::from_rgba8(20, 40, 35, 0.18),
                        offset: iced::Vector::new(0., 8.),
                        blur_radius: 30.,
                    }),
                    ..Default::default()
                },
            )
        });
    stack![
        base,
        opaque(
            mouse_area(
                container(Space::new())
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(|theme| crate::appearance::container(
                        theme,
                        container::Style {
                            background: Some(Color::from_rgba8(25, 35, 40, 0.18).into()),
                            ..Default::default()
                        }
                    ))
            )
            .on_press(dismiss)
        ),
        container(opaque(popup))
            .padding(24)
            .center_x(Length::Fill)
            .center_y(Length::Fill),
    ]
    .into()
}

pub(super) fn chord(keys: &str, size: u32) -> Element<'static, Message> {
    container(rich_text(super::shortcuts::spans(keys)).size(size))
        .padding([3, 6])
        .style(|theme| {
            crate::appearance::container(
                theme,
                container::Style {
                    background: Some(Color::from_rgb8(245, 247, 249).into()),
                    border: Border {
                        radius: 4.into(),
                        width: 1.,
                        color: Color::from_rgb8(224, 228, 233),
                    },
                    ..Default::default()
                },
            )
        })
        .into()
}

fn result_row(index: usize, entry: &'static Entry, selected: bool) -> Element<'static, Message> {
    list_row(
        &entry.keys,
        entry.name.into(),
        entry.description.into(),
        entry.scope.label(),
        selected,
        true,
        Message::Help(Action::Select(index)),
    )
}

/// One result row, shared with the command palette. A disabled row is muted
/// and its description says why.
pub(super) fn list_row(
    keys: &str,
    name: String,
    description: String,
    scope: &str,
    selected: bool,
    enabled: bool,
    on_press: Message,
) -> Element<'static, Message> {
    let name = text(name).size(13);
    let body = container(
        row![
            container(if keys.is_empty() {
                Space::new().into()
            } else {
                chord(keys, 11)
            })
            .width(150),
            column![
                if enabled {
                    name
                } else {
                    name.style(muted_text)
                },
                text(description).size(11).style(muted_text),
            ]
            .spacing(2)
            .width(Length::Fill),
            text(scope.to_owned()).size(11).style(muted_text).width(96),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .padding([6, 8])
    .width(Length::Fill)
    .style(move |theme| {
        crate::appearance::container(
            theme,
            container::Style {
                background: selected.then(|| Color::from_rgb8(225, 240, 236).into()),
                border: Border {
                    radius: 6.into(),
                    width: if selected { 1. } else { 0. },
                    color: Color::from_rgb8(17, 126, 108),
                },
                ..Default::default()
            },
        )
    });
    mouse_area(body)
        .on_press(on_press)
        .interaction(iced::mouse::Interaction::Pointer)
        .into()
}

fn drawing(label: &'static str, doc: reshiki::document::Document) -> Element<'static, Message> {
    column![
        text(label).size(11).style(muted_text),
        crate::canvas::layered::canvas(crate::canvas::OwnedDrawingPreview(doc))
            .width(Length::Fill)
            .height(100),
    ]
    .spacing(3)
    .into()
}

fn preview(entry: Option<&'static Entry>) -> Element<'static, Message> {
    let mut pane = column![].spacing(10).width(300);
    if let Some(entry) = entry {
        pane = pane.push(
            column![
                chord(&entry.keys, 14),
                text(entry.name).size(17),
                text(entry.description).size(12),
                text(format!("Applies to: {}", entry.scope.label()))
                    .size(11)
                    .style(muted_text),
            ]
            .spacing(6),
        );
        pane = pane.push(match entry.example {
            Some(example) => match examples::render(example) {
                Ok((before, after)) => {
                    let (target, key) = match example {
                        Example::Atom(key) => ("highlighted atom", key),
                        Example::Bond(key) => ("highlighted bond", key),
                    };
                    column![
                        drawing("Before", before),
                        text(format!(
                            "Point at the {target} (or move the keyboard hotspot there) and press {key}"
                        ))
                        .size(11)
                        .style(muted_text),
                        drawing("After", after),
                    ]
                    .spacing(6)
                    .into()
                }
                Err(error) => text(format!("Example unavailable: {error}"))
                    .size(11)
                    .style(muted_text)
                    .into(),
            },
            None => Element::from(
                text("This shortcut chooses a tool or acts on the view, a file or the selection; there is no drawing to preview.")
                    .size(11)
                    .style(muted_text),
            ),
        });
    }
    // Clip rather than squeeze: the footer keeps the examples action.
    pane.clip(true).into()
}

#[cfg(test)]
mod tests;
