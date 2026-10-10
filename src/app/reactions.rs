use super::{App, InspectorTab, Job, Message, Request, Tool};
use crate::canvas::{DrawingThumbnail, layered::canvas};
use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Element, Length, Task};
use reshiki::reactions::{Reaction, Role};

#[derive(Default)]
pub struct State {
    arrow: Option<u64>,
    epoch: u64,
}

#[cfg(test)]
mod tests;
#[derive(Debug, Clone)]
pub enum Action {
    Open,
    Choose(u64),
    Assign(Role),
    ClearSelected,
    Unlink,
    SelectAll,
    SelectParticipant(Role, usize),
    Export(&'static str),
}
impl App {
    pub(super) fn reaction_arrow(&self) -> Option<u64> {
        let selected: Vec<_> = self
            .tab
            .doc
            .arrows
            .iter()
            .filter(|a| self.tab.selected.contains(&a.id))
            .collect();
        if let [arrow] = selected.as_slice() {
            return Some(arrow.id);
        }
        if self.tab.reactions.epoch == self.tab.file_epoch
            && let Some(id) = self.tab.reactions.arrow
            && self.tab.doc.arrows.iter().any(|a| a.id == id)
        {
            return Some(id);
        }
        if let [reaction] = self.tab.doc.reactions.as_slice() {
            return Some(reaction.arrow);
        }
        if let [arrow] = self.tab.doc.arrows.as_slice() {
            return Some(arrow.id);
        }
        None
    }
    pub(super) fn reaction_action(&mut self, action: Action) -> Task<Message> {
        self.inspector_open = true;
        self.inspector_tab = InspectorTab::Reactions;
        self.tool = Tool::Select;
        if let Action::Choose(id) = action {
            self.tab.reactions = State {
                arrow: Some(id),
                epoch: self.tab.file_epoch,
            };
            self.tab.selected = vec![id];
            return Task::none();
        }
        let Some(arrow) = self.reaction_arrow() else {
            return Task::none();
        };
        self.tab.reactions = State {
            arrow: Some(arrow),
            epoch: self.tab.file_epoch,
        };
        if let Action::Export(format) = action {
            if self.tab.busy {
                return Task::none();
            }
            let mut request = Request::molecule("export", self.tab.doc.clone());
            request.selected_ids = Some(vec![arrow]);
            request.format = Some(format.into());
            return self.run(request, Job::Export(format));
        }
        let before = self.tab.doc.clone();
        match action {
            Action::Assign(role) => {
                if let Err(error) =
                    reshiki::reactions::assign(&mut self.tab.doc, arrow, &self.tab.selected, role)
                {
                    self.error = true;
                    self.status = error;
                    return Task::none();
                }
            }
            Action::ClearSelected => {
                if let Some(reaction) = self.tab.doc.reactions.iter_mut().find(|r| r.arrow == arrow)
                {
                    for role in Role::ALL {
                        reaction
                            .participants_mut(role)
                            .retain(|p| !p.atoms.iter().any(|id| self.tab.selected.contains(id)));
                    }
                }
            }
            Action::Unlink => self.tab.doc.reactions.retain(|r| r.arrow != arrow),
            Action::SelectAll => {
                if let Some(reaction) = self.tab.doc.reactions.iter().find(|r| r.arrow == arrow) {
                    self.tab.selected = reaction.ids();
                }
            }
            Action::SelectParticipant(role, index) => {
                if let Some(participant) = self
                    .tab
                    .doc
                    .reactions
                    .iter()
                    .find(|r| r.arrow == arrow)
                    .and_then(|r| r.participants(role).get(index))
                {
                    self.tab.selected = participant.atoms.clone();
                }
            }
            _ => {}
        }
        self.changed(before);
        Task::none()
    }
    pub(super) fn reactions_inspector(&self) -> Element<'_, Message> {
        use super::workspace::{command, muted_text, panel};
        let ready = self
            .reaction_arrow()
            .and_then(|arrow| self.tab.doc.reactions.iter().find(|r| r.arrow == arrow))
            .is_some_and(Reaction::ready);
        let mut exports = row![].spacing(8);
        for (label, format) in [("RXN · V3000", "rxn"), ("Reaction SMILES", "rsmi")] {
            exports = exports.push(
                command(label, Message::Reaction(Action::Export(format)))
                    .on_press_maybe(
                        (ready && !self.tab.busy)
                            .then_some(Message::Reaction(Action::Export(format))),
                    )
                    .width(Length::Fill),
            );
        }
        container(column![
            container(
                column![
                    command("‹ Properties", Message::Inspector(InspectorTab::Properties)),
                    text("Reaction roles").size(20)
                ]
                .spacing(8)
            )
            .padding([12, 16]),
            scrollable(
                container(column![self.mapping_panel(), self.reactions_panel()].spacing(16))
                    .padding([0, 16])
            )
            .id("inspector-content")
            .height(Length::Fill),
            container(
                column![
                    text(if ready {
                        "Export reaction data"
                    } else {
                        "Assign reactants and products to export"
                    })
                    .size(12),
                    exports,
                    text("Save .rsk to retain the complete scheme and captions.")
                        .size(11)
                        .style(muted_text)
                ]
                .spacing(7)
            )
            .padding(16),
        ])
        .width(320)
        .height(Length::Fill)
        .style(panel)
        .into()
    }
    pub(super) fn reactions_panel(&self) -> Element<'_, Message> {
        use super::workspace::{command, control, muted_text};
        let mut body = column![
            text("Choose an arrow, then assign molecules from the canvas.")
                .size(12)
                .style(muted_text),
        ]
        .spacing(12);
        let active = self.reaction_arrow();
        if self.tab.doc.arrows.is_empty() {
            return body
                .push(text("Add a reaction arrow with A to begin.").size(13))
                .into();
        }
        for (index, arrow) in self.tab.doc.arrows.iter().enumerate() {
            let reaction = self.tab.doc.reactions.iter().find(|r| r.arrow == arrow.id);
            let summary = reaction
                .map(|r| format!("{} → {}", r.reactants.len(), r.products.len()))
                .unwrap_or_else(|| "Assign roles".into());
            body = body.push(
                button(row![
                    text(format!("Arrow {}", index + 1))
                        .size(13)
                        .width(Length::Fill),
                    text(summary).size(12)
                ])
                .padding([9, 10])
                .width(Length::Fill)
                .style(control(active == Some(arrow.id)))
                .on_press(Message::Reaction(Action::Choose(arrow.id))),
            );
        }
        let Some(arrow) = active else {
            return body.into();
        };
        let empty = Reaction::new(arrow);
        let reaction = self
            .tab
            .doc
            .reactions
            .iter()
            .find(|r| r.arrow == arrow)
            .unwrap_or(&empty);
        let has_atoms = self
            .tab
            .selected
            .iter()
            .any(|id| self.tab.doc.atom(*id).is_some());
        for role in Role::ALL {
            let parts = reaction.participants(role);
            let mut card = column![row![
                text(role.label()).size(13).width(Length::Fill),
                text(parts.len().to_string()).size(12).style(muted_text)
            ]]
            .spacing(5);
            for (index, part) in parts.iter().enumerate() {
                let selected = part.atoms.iter().all(|id| self.tab.selected.contains(id));
                let mut preview = reshiki::editing::selection(&self.tab.doc, &part.atoms);
                for atom in &mut preview.atoms {
                    if let Some(source) = self.tab.doc.atom(atom.id) {
                        atom.label_h = source.label_h;
                    }
                }
                card = card.push(
                    button(
                        row![
                            canvas(DrawingThumbnail(preview)).width(92).height(28),
                            text(format!(
                                "{}{} {}",
                                if part.coefficient > 1 {
                                    format!("{} × ", part.coefficient)
                                } else {
                                    String::new()
                                },
                                part.atoms.len(),
                                if part.atoms.len() == 1 {
                                    "atom"
                                } else {
                                    "atoms"
                                }
                            ))
                            .size(12),
                        ]
                        .align_y(iced::Alignment::Center),
                    )
                    .padding([3, 8])
                    .width(Length::Fill)
                    .style(control(selected))
                    .on_press(Message::Reaction(Action::SelectParticipant(role, index))),
                );
            }
            if parts.is_empty() {
                card = card.push(text("No molecules assigned").size(11).style(muted_text));
            }
            card = card.push(
                command(
                    "+ Assign selected molecules",
                    Message::Reaction(Action::Assign(role)),
                )
                .on_press_maybe(has_atoms.then_some(Message::Reaction(Action::Assign(role))))
                .width(Length::Fill),
            );
            body = body.push(
                container(card)
                    .padding(10)
                    .width(Length::Fill)
                    .style(|theme| {
                        crate::appearance::container(
                            theme,
                            container::Style {
                                background: Some(iced::Color::WHITE.into()),
                                border: iced::Border {
                                    color: iced::Color::from_rgb8(218, 228, 225),
                                    width: 1.,
                                    radius: 8.into(),
                                },
                                ..Default::default()
                            },
                        )
                    }),
            );
        }
        body = body.push(text("Selecting any atom assigns its whole molecule. Reassigning moves it to the new role.").size(11).style(muted_text));
        body = body.push(
            row![
                command("Select reaction", Message::Reaction(Action::SelectAll)),
                command(
                    "Clear selected roles",
                    Message::Reaction(Action::ClearSelected)
                )
                .on_press_maybe(has_atoms.then_some(Message::Reaction(Action::ClearSelected)))
            ]
            .spacing(4),
        );
        if !self.tab.doc.reactions.is_empty() {
            body = body.push(command(
                "Remove reaction roles",
                Message::Reaction(Action::Unlink),
            ));
        }
        body.into()
    }
}
