//! The Ω symbol picker in the Style bar. A symbol goes in at the text cursor;
//! without a text draft it starts one, on the selected text box or at the
//! center of the view.
use super::color_popover::{Action, Menu, surface};
use super::command_palette::catalog::Run;
use super::popover::popover;
use super::workspace::{control, hover_hint, muted_text};
use super::{App, Message, inline_text};
use iced::widget::{Column, Row, column, container, mouse_area, row, scrollable, text, tooltip};
use iced::{Alignment, Element, Length, Task};
use reshiki::symbols::{GROUPS, Symbol, find};

const CELL: f32 = 30.;
const COLUMNS: usize = 14;
const RECENT: usize = 14;
const EDITOR: &str = "inline-caption";

impl App {
    pub(super) fn symbol_button(&self) -> Element<'_, Message> {
        let open = matches!(self.style_menu, Some(Menu::Symbols { .. }));
        let anchor = hover_hint(
            reshiki::accessibility::button(
                "text-style-symbols",
                "Insert symbol",
                text("Ω").size(14),
            )
            .padding([5, 8])
            .style(control(open))
            .expanded(open)
            .on_press(Message::StyleMenu(Action::Symbols)),
            "Insert symbol",
            tooltip::Position::Bottom,
        );
        let popup = match self.style_menu {
            Some(Menu::Symbols { hovered }) => Some(self.symbol_popup(hovered)),
            _ => None,
        };
        Element::new(popover(anchor, popup, Message::StyleMenu(Action::Close)))
    }

    fn symbol_popup(&self, hovered: Option<&'static Symbol>) -> Element<'_, Message> {
        let info: Element<'_, Message> = match hovered {
            Some(symbol) => row![
                text(symbol.text).size(22).width(44).center(),
                column![
                    text(symbol.name).size(12),
                    text(symbol.shortcut.map_or_else(
                        || "No shortcut".to_string(),
                        |shortcut| format!("Type {shortcut}")
                    ))
                    .size(11)
                    .style(muted_text),
                ]
                .spacing(1),
            ]
            .spacing(6)
            .align_y(Alignment::Center)
            .height(34)
            .into(),
            None => container(
                text("Click to insert, or type a shortcut: -> for →, +- for ±, \\alpha for α")
                    .size(11)
                    .style(muted_text),
            )
            .height(34)
            .align_y(Alignment::Center)
            .into(),
        };
        let mut body = column![info].spacing(6);
        let recent: Vec<_> = self
            .appearance
            .recent_symbols
            .iter()
            .filter_map(|text| find(text))
            .collect();
        if !recent.is_empty() {
            body = body.push(section("Recent", &recent));
        }
        let group_section = |group: &'static reshiki::symbols::Group| {
            let symbols: Vec<_> = group.symbols.iter().collect();
            section(group.name, &symbols)
        };
        // Short groups share a row, so the picker covers less of the drawing.
        let (long, short) = GROUPS.split_at(3);
        let groups = Column::with_children(long.iter().map(group_section))
            .push(Row::with_children(short.iter().map(group_section)).spacing(16))
            .spacing(6);
        body = body.push(scrollable(groups).height(Length::Shrink));
        container(body).padding(8).style(surface).into()
    }

    pub(super) fn open_symbols(&mut self) -> Task<Message> {
        let open = matches!(self.style_menu, Some(Menu::Symbols { .. }));
        self.close_style_menu();
        if open {
            return self.focus_draft();
        }
        self.style_menu = Some(Menu::Symbols { hovered: None });
        // Typing goes on in the caption; it closes the picker.
        self.focus_draft()
    }

    /// The palette's Insert symbol… keeps a text draft open, to insert into it.
    pub(super) fn opens_symbols(&self, message: &Message) -> bool {
        let Message::Commands(action) = message else {
            return false;
        };
        self.commands.to_run(action).is_some_and(|command| {
            matches!(&command.run, Run::Send(messages)
                if matches!(messages.as_slice(), [Message::StyleMenu(Action::Symbols)]))
        })
    }

    /// Keyboard focus goes back to the text draft, if there is one.
    pub(super) fn focus_draft(&self) -> Task<Message> {
        if self.tab.inline_text.is_some() {
            iced::widget::operation::focus(EDITOR)
        } else {
            Task::none()
        }
    }

    pub(super) fn insert_symbol(&mut self, symbol: &'static str) -> Task<Message> {
        use iced::widget::text_editor::{Action as Input, Edit, Motion};
        let task = if self.tab.inline_text.is_some() {
            iced::widget::operation::focus(EDITOR)
        } else {
            let selected = self
                .tab
                .doc
                .annotations
                .iter()
                .find(|a| self.tab.selected == [a.id]);
            let (id, position) = selected.map_or((None, self.tab.camera.center), |label| {
                (Some(label.id), label.position)
            });
            let task = self.inline_action(inline_text::Action::Begin(id, position));
            if self.tab.inline_text.is_none() {
                return task;
            }
            // Add, not replace: a linked name opens selected, for renaming.
            // Its line end keeps a property caption's later lines intact.
            self.tab.caption_editor.perform(Input::Move(Motion::End));
            // The picker would hide where the symbol went.
            self.close_style_menu();
            task
        };
        self.caption_action(Input::Edit(Edit::Paste(std::sync::Arc::new(symbol.into()))));
        let recent = &mut self.appearance.recent_symbols;
        recent.retain(|text| text != symbol);
        recent.insert(0, symbol.into());
        recent.truncate(RECENT);
        // Recents are a convenience: a failed save only forgets them.
        let _ = self.appearance.save();
        task
    }
}

fn section<'a>(name: &'a str, symbols: &[&'static Symbol]) -> Element<'a, Message> {
    // Each button keeps its id when recents reorder.
    let id = |symbol: &Symbol| {
        format!("symbol-{name}-{}", symbol.name)
            .to_lowercase()
            .replace(' ', "-")
    };
    let rows = symbols.chunks(COLUMNS).map(|chunk| {
        Row::with_children(chunk.iter().map(|&symbol| cell(id(symbol), symbol)))
            .spacing(2)
            .into()
    });
    column![text(name.to_uppercase()).size(10).style(muted_text)]
        .extend(rows)
        .spacing(2)
        .into()
}

fn cell(id: String, symbol: &'static Symbol) -> Element<'static, Message> {
    let name = match symbol.shortcut {
        Some(shortcut) => format!("{} {}, type {shortcut}", symbol.text, symbol.name),
        None => format!("{} {}", symbol.text, symbol.name),
    };
    mouse_area(
        reshiki::accessibility::button(
            id,
            name,
            container(text(symbol.text).size(16)).center(CELL),
        )
        .width(CELL)
        .height(CELL)
        .padding(0)
        .style(control(false))
        .on_press(Message::StyleMenu(Action::Insert(symbol.text))),
    )
    .on_enter(Message::StyleMenu(Action::Hover(symbol)))
    .into()
}

#[cfg(test)]
mod tests;
