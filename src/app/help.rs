//! The Keys filter: a searchable reference of every shortcut, with a live before/after
//! drawing for contextual keys. Selecting a row only shows it; Enter or Run
//! runs a row that names a command palette command, through the palette.
use super::{
    App, Message,
    command_palette::catalog::{Command, Run},
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

use registry::Entry;
#[cfg(test)]
use registry::Example;

pub(super) const SEARCH: &str = "help-search";
pub(super) const RESULTS: &str = "help-results";

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
    if super::command_palette::is_shortcut(key, modifiers) {
        return Some(Message::Commands(super::command_palette::Action::Open));
    }
    if matches!(key, Key::Named(Named::Tab)) {
        return super::command_palette::modal_key(key, modifiers);
    }
    if !modifiers.is_empty() {
        return None;
    }
    let step = match key {
        Key::Named(Named::ArrowUp) => -1,
        Key::Named(Named::ArrowDown) => 1,
        Key::Named(Named::PageUp) => -8,
        Key::Named(Named::PageDown) => 8,
        Key::Named(Named::Home) => isize::MIN,
        Key::Named(Named::End) => isize::MAX,
        _ => return None,
    };
    Some(Message::Help(Action::Move(step)))
}

#[derive(Debug, Clone)]
pub enum Action {
    Query(String),
    /// Move the selection by this many rows, stopping at either end.
    Move(isize),
    /// Select a registry entry; it is only shown.
    Select(usize),
    /// Run a registry entry's palette command and close Help; an entry
    /// without one, or a command that cannot run now, does nothing.
    Run(usize),
}

#[derive(Debug, Default)]
pub(super) struct State {
    pub(super) query: String,
    /// A registry index; None selects the best match.
    selected: Option<usize>,
    /// The palette command each runnable registry entry names, found when
    /// Help opens.
    commands: Vec<(usize, Command)>,
}

impl State {
    pub(super) fn results(&self, contextual: bool) -> Vec<usize> {
        search::rank(entries(), &self.query, contextual)
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
    /// The palette command registry entry `index` names, if any.
    pub(super) fn command(&self, index: usize) -> Option<&Command> {
        (self.commands.iter())
            .find(|(entry, _)| *entry == index)
            .map(|(_, command)| command)
    }
    #[cfg(test)]
    pub(super) fn selected(&self, contextual: bool) -> Option<&'static Entry> {
        self.current(&self.results(contextual))
            .and_then(|i| entries().get(i))
    }
}

impl App {
    /// Help blocks drawing input, but its ranking uses the tab's stored mode.
    pub(super) fn help_contextual(&self) -> bool {
        self.tab.keyboard_drawing.enabled() && self.tool.selects()
    }

    pub(super) fn toggle_help(&mut self) -> Task<Message> {
        if self.help_open {
            self.help_open = false;
            return Task::none();
        }
        // Build the same catalog once for both filters. Keys keeps registry
        // indexes, while command choices still use their original run gates.
        let _ = self.commands_action(super::command_palette::Action::Open);
        self.commands.open = false;
        self.help_open = true;
        let catalog = self.commands.catalog();
        let commands = (entries().iter().enumerate())
            .filter_map(|(index, entry)| {
                catalog
                    .iter()
                    .find(|command| {
                        !matches!(command.run, Run::Template(..))
                            && command.name.eq_ignore_ascii_case(entry.name)
                    })
                    .map(|command| (index, command.clone()))
            })
            .collect();
        self.help = State {
            commands,
            ..State::default()
        };
        iced::widget::operation::focus(SEARCH)
    }

    pub(super) fn help_action(&mut self, action: Action) -> Task<Message> {
        if !self.help_open {
            return Task::none();
        }
        match action {
            Action::Query(query) => {
                self.commands.query = query.clone();
                self.rank_commands();
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
                // A clicked row takes focus; typing and Enter stay in the search.
                iced::widget::operation::focus(SEARCH)
            }
            Action::Run(index) => {
                // The update gates refuse a command that cannot run now,
                // before they commit a caption draft, as for the palette.
                let Some(command) = self.help.command(index).cloned() else {
                    return Task::none();
                };
                self.help_open = false;
                self.run_command(command)
            }
            Action::Move(step) => {
                let results = self.help.results(self.help_contextual());
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

    pub(super) fn with_keys<'a>(&'a self, base: Element<'a, Message>) -> Element<'a, Message> {
        let results = self.help.results(self.help_contextual());
        let current = self.help.current(&results);
        let selected = current.and_then(|i| entries().get(i));
        let command = current.and_then(|i| self.help.command(i));
        let unavailable = command.and_then(|command| self.command_unavailable(command));
        let name = match selected {
            Some(entry) => format!(
                "Search shortcuts; {} results; selected {}, {}, {}: {}{}",
                results.len(),
                entry.name,
                entry.keys,
                entry.scope.label(),
                entry.description,
                match (command, unavailable) {
                    (None, _) => String::new(),
                    (Some(_), None) => "; Enter runs it".into(),
                    (Some(_), Some(reason)) => format!("; cannot run now: {reason}"),
                }
            ),
            None => "Search shortcuts; no results".into(),
        };
        let search = reshiki::accessibility::text_input(
            SEARCH,
            name,
            "Search keys, actions or descriptions",
            &self.help.query,
        )
        .style(crate::appearance::input_style)
        .on_input(|query| Message::Help(Action::Query(query)))
        .on_submit_maybe(
            current
                .filter(|_| command.is_some())
                .map(|i| Message::Help(Action::Run(i))),
        )
        .size(14)
        .padding(8);
        let mut list = column![].spacing(2);
        for &index in &results {
            if let Some(entry) = entries().get(index) {
                list = list.push(result_row(
                    index,
                    entry,
                    Some(index) == current,
                    self.help_contextual(),
                    self.help.command(index).is_some(),
                ));
            }
        }
        if results.is_empty() {
            list = list.push(
                text("No shortcut matches. Try a key, an action, or a group such as Boc.")
                    .size(12)
                    .style(muted_text),
            );
        }
        let run = current.filter(|_| command.is_some()).map(|index| {
            Element::from(
                reshiki::accessibility::button(
                    format!("help-run-{index}"),
                    format!("Run {}", selected.map_or("", |entry| entry.name)),
                    row![text("Run").size(13), enter_chord()]
                        .spacing(8)
                        .align_y(Alignment::Center),
                )
                .padding([5, 12])
                .style(control(true))
                .on_press_maybe(
                    unavailable
                        .is_none()
                        .then_some(Message::Help(Action::Run(index))),
                ),
            )
        });
        let detail = selected
            .map(|entry| {
                selected_detail(Some(entry), entry.name, entry.description, unavailable, run)
            })
            .unwrap_or_else(|| Space::new().into());
        self.palette_surface(base, search.into(), list.into(), detail)
    }
}

pub(super) enum Placement {
    Center,
    /// Centered, and only as tall as its content.
    #[cfg_attr(
        target_arch = "wasm32",
        expect(dead_code, reason = "Only the GitHub dialogs fit their content")
    )]
    Fit,
}

/// A dialog over a dimmed, inert workspace; clicking outside it sends
/// `dismiss`. Fit placement lets a short dialog shrink to its content.
pub(super) fn modal<'a>(
    base: Element<'a, Message>,
    content: Element<'a, Message>,
    width: f32,
    height: f32,
    placement: Placement,
    dismiss: Message,
) -> Element<'a, Message> {
    let base = reshiki::accessibility::inert(base);
    let popup = container(content)
        .padding(24)
        .width(width)
        .height(match placement {
            Placement::Center => Length::Fixed(height),
            Placement::Fit => Length::Shrink,
        })
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
    let popup = container(opaque(popup))
        .padding(24)
        .center_x(Length::Fill)
        .height(Length::Fill);
    let popup = popup.align_y(Alignment::Center);
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
        popup,
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

/// The Return key as menus show it, in a key cap.
fn enter_chord() -> Element<'static, Message> {
    chord(
        &super::shortcuts::keys(iced::keyboard::Modifiers::empty(), "Enter"),
        11,
    )
}

/// One result row: chord, name and description, scope, and Run ↩ when Enter
/// runs it.
fn result_row(
    index: usize,
    entry: &'static Entry,
    selected: bool,
    contextual: bool,
    runs: bool,
) -> Element<'static, Message> {
    let name = if contextual && !entry.classic_keys.is_empty() {
        format!("{} · {}: F8 off", entry.name, entry.classic_keys.join("/"))
    } else {
        entry.name.into()
    };
    let label = format!(
        "{name}, {}, {}{}",
        entry.keys,
        entry.scope.label(),
        if runs {
            ", runs with Enter"
        } else {
            ", shortcut reference"
        }
    );
    let body = container(
        row![
            container(
                row![
                    text(name)
                        .size(13)
                        .wrapping(iced::widget::text::Wrapping::None),
                    text(entry.scope.label())
                        .size(11)
                        .style(muted_text)
                        .wrapping(iced::widget::text::Wrapping::None),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
            )
            .width(Length::Fill)
            .clip(true),
            container(chord(&entry.keys, 11)).max_width(240).clip(true),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .padding([0, 8])
    .height(28)
    .align_y(Alignment::Center);
    row_button(
        format!("help-row-{index}"),
        label,
        body,
        selected,
        Message::Help(Action::Select(index)),
    )
    .into()
}

/// A result row's selection highlight and named button, shared with the
/// command palette's compact rows.
pub(super) fn row_button(
    id: String,
    label: String,
    body: iced::widget::Container<'static, Message>,
    selected: bool,
    on_press: Message,
) -> reshiki::accessibility::Button<'static, Message> {
    let body = body.width(Length::Fill).style(move |theme| {
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
    // A named button, so assistive technology can reach and press every row;
    // arrow keys, not Tab, move between rows. The container above draws it.
    reshiki::accessibility::button(id, label, body)
        .padding(0)
        .width(Length::Fill)
        .style(|theme, _| iced::widget::button::Style {
            text_color: theme.palette().text,
            ..Default::default()
        })
        .on_press(on_press)
        .tab_stop(false)
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

/// The shared bottom strip; examples come from the shortcut's actual edit.
pub(super) fn selected_detail<'a>(
    entry: Option<&'static Entry>,
    name: &str,
    description: &str,
    unavailable: Option<&str>,
    run: Option<Element<'a, Message>>,
) -> Element<'a, Message> {
    let mut summary = column![
        text(name.to_owned()).size(15),
        text(description.to_owned()).size(12)
    ]
    .spacing(5)
    .width(Length::FillPortion(3));
    if let Some(entry) = entry {
        summary = summary.push(
            text(format!("Applies to: {}", entry.scope.label()))
                .size(11)
                .style(muted_text),
        );
    }
    if let Some(reason) = unavailable {
        summary = summary.push(text(reason.to_owned()).size(11).style(muted_text));
    }
    summary = summary.extend(run);
    let mut strip = row![
        scrollable(summary)
            .width(Length::FillPortion(3))
            .height(Length::Fill)
    ]
    .spacing(16)
    .height(Length::Fill);
    if let Some(example) = entry.and_then(|entry| entry.example) {
        strip = strip.push(match examples::render(example) {
            Ok((before, after)) => row![drawing("Before", before), drawing("After", after)]
                .spacing(10)
                .width(Length::FillPortion(4))
                .into(),
            Err(error) => Element::from(
                text(format!("Example unavailable: {error}"))
                    .size(11)
                    .style(muted_text),
            ),
        });
    }
    container(strip)
        .height(144)
        .width(Length::Fill)
        .clip(true)
        .into()
}

#[cfg(test)]
mod tests;
