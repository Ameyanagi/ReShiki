use super::{
    App, Message,
    workspace::{control, muted_text},
};
use iced::widget::{Space, column, container, mouse_area, opaque, row, stack, text};
use iced::{Element, Length, Task};
use reshiki::abbreviations::LabelAlignment;
use reshiki::atom_text::{self, Mode};

#[derive(Debug, Clone)]
pub enum Action {
    Begin(Option<u64>),
    Input(String),
    Mode(Mode),
    ContractSelection,
    ReverseInput(String),
    Apply,
    Cancel,
}
pub struct State {
    id: u64,
    epoch: u64,
    input: String,
    mode: Mode,
    label_edited: bool,
    alignment: LabelAlignment,
    members: Option<Vec<u64>>,
    reverse: String,
    reverse_edited: bool,
    error: Option<String>,
}
impl App {
    pub(super) fn atom_text_target(&self) -> Option<u64> {
        if let [id] = self.tab.selected.as_slice()
            && self.tab.doc.atom(*id).is_some()
        {
            return Some(*id);
        }
        self.tab
            .doc
            .abbreviations
            .iter()
            .find(|g| {
                g.members.len() == self.tab.selected.len()
                    && g.members.iter().all(|id| self.tab.selected.contains(id))
            })
            .map(|g| g.anchor)
    }
    pub(super) fn atom_text_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Begin(id) => {
                if self.tab.cleanup.is_some() || self.tab.busy || !self.finish_inline(true) {
                    return Task::none();
                }
                let Some(id) = id.or_else(|| self.atom_text_target()) else {
                    self.status = "Select one atom, then press Enter to edit its label".into();
                    return Task::none();
                };
                let Some(atom) = self.tab.doc.atom(id) else {
                    return Task::none();
                };
                if !atom.centroid.is_empty() && self.tab.doc.abbreviation(id).is_none() {
                    self.status =
                        "This is a tracked attachment point. Edit the atom bonded to it instead."
                            .into();
                    return Task::none();
                }
                let group = self.tab.doc.abbreviation(id);
                self.tab.atom_text = Some(State {
                    id,
                    epoch: self.tab.file_epoch,
                    input: group
                        .map(|g| g.label.clone())
                        .unwrap_or_else(|| atom_text::entry(atom)),
                    mode: if group.is_some() {
                        Mode::Group
                    } else if atom.display.variable.is_some() {
                        Mode::Text
                    } else {
                        Mode::Auto
                    },
                    error: None,
                    label_edited: false,
                    alignment: group.map(|g| g.alignment).unwrap_or_default(),
                    members: None,
                    reverse: group.map(|g| g.reverse_label.clone()).unwrap_or_default(),
                    reverse_edited: false,
                });
                self.context_menu = None;
                return Task::batch([
                    iced::widget::operation::focus("atom-text"),
                    iced::widget::operation::select_all("atom-text"),
                ]);
            }
            Action::Input(input) => {
                if let Some(state) = &mut self.tab.atom_text {
                    state.label_edited |= state.input != input;
                    state.input = input;
                    state.error = None;
                }
            }
            Action::ContractSelection => {
                if self.tab.cleanup.is_some() || self.tab.busy || !self.finish_inline(true) {
                    return Task::none();
                }
                let mut candidate = self.tab.doc.clone();
                match candidate.contract(&self.tab.selected, "Group", "") {
                    Ok(()) => {
                        if let Some(group) = candidate.abbreviations.last() {
                            self.tab.atom_text = Some(State {
                                id: group.anchor,
                                epoch: self.tab.file_epoch,
                                input: String::new(),
                                mode: Mode::Group,
                                label_edited: true,
                                alignment: LabelAlignment::Auto,
                                members: Some(group.members.clone()),
                                reverse: String::new(),
                                reverse_edited: false,
                                error: None,
                            });
                            self.context_menu = None;
                            return iced::widget::operation::focus("atom-text");
                        }
                    }
                    Err(error) => {
                        self.status = error;
                        self.error = true;
                    }
                }
            }
            Action::ReverseInput(value) => {
                if let Some(state) = &mut self.tab.atom_text {
                    state.reverse = value;
                    state.reverse_edited = true;
                    state.error = None;
                }
            }
            Action::Mode(mode) => {
                if let Some(state) = &mut self.tab.atom_text {
                    state.label_edited |= state.mode != mode;
                    state.mode = mode;
                    state.error = None;
                }
            }
            Action::Cancel => self.tab.atom_text = None,
            Action::Apply => {
                let Some(state) = &self.tab.atom_text else {
                    return Task::none();
                };
                let result = if state.epoch != self.tab.file_epoch {
                    Err("The drawing changed. Cancel and select the atom again.".into())
                } else if !state.label_edited {
                    Ok(self.tab.doc.clone())
                } else if let Some(members) = &state.members {
                    let mut doc = self.tab.doc.clone();
                    doc.contract(members, &state.input, &state.reverse)
                        .map(|()| doc)
                } else {
                    atom_text::apply(&self.tab.doc, state.id, &state.input, state.mode)
                };
                match result {
                    Ok(mut doc) => {
                        if let Some(group) =
                            doc.abbreviations.iter_mut().find(|g| g.anchor == state.id)
                        {
                            group.alignment = state.alignment;
                            if state.members.is_some()
                                || state.reverse_edited
                                || self
                                    .tab
                                    .doc
                                    .abbreviation(state.id)
                                    .is_some_and(|g| g.label == group.label)
                            {
                                group.reverse_label = state.reverse.trim().into();
                            }
                        }
                        if let Err(error) = doc.validate() {
                            if let Some(state) = &mut self.tab.atom_text {
                                state.error = Some(error);
                            }
                            return Task::none();
                        }
                        let before = std::mem::replace(&mut self.tab.doc, doc);
                        self.tab.atom_text = None;
                        self.changed(before);
                    }
                    Err(error) => {
                        if let Some(state) = &mut self.tab.atom_text {
                            state.error = Some(error);
                        }
                    }
                }
            }
        }
        Task::none()
    }
    pub(super) fn with_atom_text<'a>(&'a self, base: Element<'a, Message>) -> Element<'a, Message> {
        let Some(state) = &self.tab.atom_text else {
            return base;
        };
        let base = reshiki::accessibility::inert(base);
        let mut content = column![
            text(if state.members.is_some() {
                "Create group label"
            } else {
                "Edit atom label"
            })
            .size(20),
            reshiki::accessibility::text_input(
                "atom-text",
                "Atom or group label",
                "N, NH3, C2H5, Boc, Cp*, M…",
                &state.input
            )
            .style(crate::appearance::input_style)
            .padding(10)
            .size(18)
            .on_input(|s| Message::AtomText(Action::Input(s)))
            .on_submit(Message::AtomText(Action::Apply)),
        ]
        .spacing(12);
        if state.members.is_none() {
            content = content.push(
                crate::appearance::pick_list(
                    [Mode::Auto, Mode::Text, Mode::Group],
                    Some(state.mode),
                    |mode| Message::AtomText(Action::Mode(mode)),
                )
                .width(Length::Fill),
            );
        }
        content = content.push(text(if state.members.is_some() {
                "The selected atoms define this group. Its formula and bonds are retained; expand it to edit the structure."
            } else { atom_text::description(&state.input, state.mode) })
                .size(12)
                .style(muted_text));
        if state.members.is_some() || self.tab.doc.abbreviation(state.id).is_some() {
            content = content.push(
                reshiki::accessibility::text_input(
                    "atom-text-reverse",
                    "Label when facing left (optional)",
                    "Label when facing left (optional)",
                    &state.reverse,
                )
                .style(crate::appearance::input_style)
                .on_input(|s| Message::AtomText(Action::ReverseInput(s)))
                .on_submit(Message::AtomText(Action::Apply))
                .padding(8),
            );
        }
        if let Some(error) = &state.error {
            content = content.push(text(error).size(12).style(crate::appearance::text_color(
                iced::Color::from_rgb8(175, 54, 54),
            )));
        }
        let popup = container(
            content.push(
                row![
                    Space::new().width(Length::Fill),
                    reshiki::accessibility::button(
                        "atom-text-cancel",
                        "Cancel atom label edit",
                        "Cancel · Esc"
                    )
                    .on_press(Message::AtomText(Action::Cancel))
                    .style(control(false)),
                    reshiki::accessibility::button(
                        "atom-text-apply",
                        "Apply atom label",
                        "Apply · Enter"
                    )
                    .on_press(Message::AtomText(Action::Apply))
                    .style(crate::appearance::primary),
                ]
                .spacing(8),
            ),
        )
        .padding(22)
        .width(430)
        .style(|theme| {
            crate::appearance::container(
                theme,
                container::Style {
                    background: Some(iced::Color::WHITE.into()),
                    border: iced::Border {
                        radius: 12.into(),
                        width: 1.,
                        color: iced::Color::from_rgb8(213, 224, 220),
                    },
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
                                background: Some(iced::Color::from_rgba8(20, 30, 30, 0.3).into()),
                                ..Default::default()
                            }
                        ))
                )
                .on_press(Message::AtomText(Action::Cancel))
            ),
            container(opaque(popup)).center(Length::Fill)
        ]
        .into()
    }
}

// A modal draft must not let unhandled keyboard shortcuts edit the canvas or
// replace the document. Background work and close/save completion still run.
pub(super) fn background(message: &Message) -> bool {
    matches!(
        message,
        Message::Tick
            | Message::EngineDone { .. }
            | Message::Viewport(_)
            | Message::Assistant(_)
            | Message::Updates(_)
            | Message::Close(_)
            | Message::Cancel
            | Message::Discard
            | Message::Saved(..)
            | Message::Opened(_)
            | Message::Exported(_)
            | Message::FigureExported(_)
            | Message::ClipboardRead { .. }
            | Message::ClipboardWritten { .. }
            | Message::CopyAsPrepared(..)
            | Message::CopyAsWritten(..)
            | Message::Pictures(super::pictures::Action::Loaded(..))
            | Message::Imports(super::import::Action::Loaded(..))
            | Message::Printing(
                super::printing::Action::Prepared(..) | super::printing::Action::Finished(..)
            )
    )
}

#[cfg(test)]
mod tests;
