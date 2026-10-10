//! The reaction inspector edits chemical map numbers and reviews detached fits.
use super::{App, InspectorTab, Message, Tool};
use iced::widget::{button, checkbox, column, container, row, text};
use iced::{Element, Length, Task};
use reshiki::{
    document::Document,
    reaction_mapping::{self as mapping, Budget, Proposal},
};
use std::sync::Arc;

#[derive(Default)]
pub(super) struct State {
    atom: Option<u64>,
    text: String,
    serial: u64,
    pending: Option<(u64, u64)>,
    proposal: Option<Arc<Proposal>>,
}
#[derive(Debug, Clone)]
pub enum Action {
    Open,
    Text(String),
    Apply,
    Clear,
    Pair,
    Show(bool),
    Position,
    ResetPositions,
    Auto,
    Ready(u64, u64, Result<Arc<Proposal>, String>),
    ApplyProposal,
    Dismiss,
    ReviewPair(usize),
    Align,
}
async fn calculate(source: Document, arrow: u64) -> Result<Arc<Proposal>, String> {
    tokio::task::spawn_blocking(move || mapping::propose(&source, arrow, Budget::default()))
        .await
        .map_err(|e| format!("Atom mapping failed: {e}"))?
        .map(Arc::new)
}
impl App {
    fn mapping_atom(&self) -> Option<u64> {
        let ids: Vec<_> = self
            .tab
            .selected
            .iter()
            .copied()
            .filter(|id| self.tab.doc.atom(*id).is_some())
            .collect();
        if let [id] = ids.as_slice() {
            Some(*id)
        } else {
            None
        }
    }
    pub(super) fn mapping_action(&mut self, action: Action) -> Task<Message> {
        if let Action::Ready(epoch, serial, result) = action {
            if self.tab.mapping.pending != Some((epoch, serial)) {
                return Task::none();
            }
            self.tab.mapping.pending = None;
            if epoch != self.tab.file_epoch {
                return Task::none();
            }
            match result {
                Ok(proposal) if proposal.current(&self.tab.doc) => {
                    self.status = if proposal.complete {
                        "Atom-map proposal ready to review"
                    } else {
                        "Search budget reached · Review the feasible proposal or add manual anchors"
                    }
                    .into();
                    self.error = false;
                    self.tab.mapping.proposal = Some(proposal);
                }
                Ok(_) => {
                    self.status = "The drawing changed; run Auto-map again".into();
                }
                Err(error) => {
                    self.status = error;
                    self.error = true;
                }
            }
            return Task::none();
        }
        if matches!(action, Action::Open) {
            self.inspector_open = true;
            self.inspector_tab = InspectorTab::Reactions;
            self.tool = Tool::Select;
            self.tab.mapping.atom = self.mapping_atom();
            self.tab.mapping.text = self
                .mapping_atom()
                .and_then(|id| self.tab.doc.atom(id))
                .map(|a| a.map_num.to_string())
                .unwrap_or_default();
            return Task::none();
        }
        if let Action::Text(value) = action {
            self.tab.mapping.atom = self.mapping_atom();
            self.tab.mapping.text = value;
            return Task::none();
        }
        if matches!(action, Action::Auto) {
            if self.tab.mapping.pending.is_some() || self.tab.busy {
                return Task::none();
            }
            let Some(arrow) = self.reaction_arrow() else {
                self.status = "Assign reactants and products first".into();
                self.error = true;
                return Task::none();
            };
            self.tab.mapping.serial = self.tab.mapping.serial.wrapping_add(1);
            let serial = self.tab.mapping.serial;
            let epoch = self.tab.file_epoch;
            self.tab.mapping.pending = Some((epoch, serial));
            self.tab.mapping.proposal = None;
            self.status = "Finding atom correspondence…".into();
            self.error = false;
            return Task::perform(calculate(self.tab.doc.clone(), arrow), move |result| {
                Message::Mapping(Action::Ready(epoch, serial, result))
            });
        }
        if matches!(action, Action::Dismiss) {
            self.tab.mapping.proposal = None;
            return Task::none();
        }
        if let Action::ReviewPair(index) = action {
            if let Some((a, b)) = self
                .tab
                .mapping
                .proposal
                .as_ref()
                .and_then(|p| p.pairs.get(index))
            {
                self.tab.selected = vec![*a, *b];
                self.tool = Tool::Select;
            }
            return Task::none();
        }
        if matches!(action, Action::Position) {
            self.tab.selected = self
                .tab
                .doc
                .atoms
                .iter()
                .filter(|a| a.map_num != 0)
                .map(|a| a.id)
                .collect();
            self.tool = Tool::EditPoints;
            self.status = "Drag atom-map handles · Escape finishes".into();
            return Task::none();
        }
        let before = self.tab.doc.clone();
        let result = (|| -> Result<Document, String> {
            match action {
                Action::Apply | Action::Clear => {
                    let id = self
                        .mapping_atom()
                        .ok_or("Select one atom to edit its map")?;
                    let number =
                        if matches!(action, Action::Clear) {
                            0
                        } else {
                            if self.tab.mapping.atom != Some(id) {
                                return Err("Enter the atom map for this atom first".into());
                            }
                            self.tab.mapping.text.trim().parse::<u32>().map_err(
                                |_| "Enter an atom map from 1 to 2147483647, or 0 to clear",
                            )?
                        };
                    mapping::set_map(&before, id, number)
                }
                Action::Pair => mapping::pair(
                    &before,
                    self.reaction_arrow()
                        .ok_or("Assign reactants and products first")?,
                    &self.tab.selected,
                ),
                Action::Align => mapping::align(
                    &before,
                    self.reaction_arrow()
                        .ok_or("Assign reactants and products first")?,
                ),
                Action::ApplyProposal => self
                    .tab
                    .mapping
                    .proposal
                    .as_ref()
                    .ok_or("Run Auto-map first")?
                    .apply(&before, true),
                Action::Show(show) => {
                    let mut candidate = before.clone();
                    candidate.atom_labels.maps = show;
                    for a in &mut candidate.atoms {
                        a.display.mapping.show = None;
                    }
                    Ok(candidate)
                }
                Action::ResetPositions => {
                    let mut candidate = before.clone();
                    for a in &mut candidate.atoms {
                        a.display.mapping.offset = None;
                    }
                    Ok(candidate)
                }
                _ => Ok(before.clone()),
            }
        })();
        match result {
            Ok(candidate) => {
                self.tab.doc = candidate;
                self.changed(before);
                self.tab.mapping.proposal = None;
                self.status = "Atom mapping updated".into();
                self.error = false;
            }
            Err(error) => {
                self.status = error;
                self.error = true;
            }
        }
        Task::none()
    }
    pub(super) fn mapping_panel(&self) -> Element<'_, Message> {
        use super::workspace::{command, horizontal_line, muted_text};
        let ready = self
            .reaction_arrow()
            .and_then(|arrow| mapping::reaction(&self.tab.doc, arrow).ok())
            .is_some_and(|r| r.ready());
        let atom = self.mapping_atom().and_then(|id| self.tab.doc.atom(id));
        let value = if atom.is_some_and(|a| self.tab.mapping.atom == Some(a.id)) {
            self.tab.mapping.text.clone()
        } else {
            atom.map(|a| a.map_num.to_string()).unwrap_or_default()
        };
        let mut body=column![
            text("Atom mapping").size(16),
            checkbox(self.tab.doc.atom_labels.maps).label("Show maps").text_size(12).on_toggle(|value|Message::Mapping(Action::Show(value))),
            text("Atom map").size(12),
            row![
                crate::appearance::text_input("Select one atom",&value).on_input(|value|Message::Mapping(Action::Text(value))).on_submit(Message::Mapping(Action::Apply)).size(12).padding(7),
                command("Set",Message::Mapping(Action::Apply)).on_press_maybe(atom.map(|_|Message::Mapping(Action::Apply))),
                command("Clear",Message::Mapping(Action::Clear)).on_press_maybe(atom.filter(|a|a.map_num!=0).map(|_|Message::Mapping(Action::Clear))),
            ].spacing(4),
            command("Pair selected atoms",Message::Mapping(Action::Pair)).on_press_maybe((ready&&self.tab.selected.iter().filter(|id|self.tab.doc.atom(**id).is_some()).count()==2).then_some(Message::Mapping(Action::Pair))),
            row![
                command(if self.tab.mapping.pending.is_some() {"Mapping…"}else{"Auto-map"},Message::Mapping(Action::Auto)).on_press_maybe((ready&&self.tab.mapping.pending.is_none()&&!self.tab.busy).then_some(Message::Mapping(Action::Auto))).width(Length::Fill),
                command("Move map labels",Message::Mapping(Action::Position)).width(Length::Fill),
            ].spacing(4),
            command("Align mapped structures",Message::Mapping(Action::Align)).on_press_maybe(ready.then_some(Message::Mapping(Action::Align))),
            command("Reset map label positions",Message::Mapping(Action::ResetPositions)),
            text("Existing maps are anchors. Agents are excluded. Alignment moves complete product molecules.").size(11).style(muted_text),
        ].spacing(7);
        if let Some(proposal) = &self.tab.mapping.proposal {
            let current = proposal.current(&self.tab.doc);
            let certainty = if !proposal.complete {
                "Budget reached · optimality and uniqueness unknown"
            } else if proposal.multiple_best {
                "Several mappings tie under the graph score"
            } else {
                "One best mapping under the graph score"
            };
            body=body.push(horizontal_line()).push(text("Review atom maps").size(14)).push(text(certainty).size(11).style(muted_text))
                .push(text(format!("{} pairs · {} reactant / {} product atoms unmatched",proposal.pairs.len(),proposal.unmatched_reactants,proposal.unmatched_products)).size(11))
                .push(text("Graph matching suggests correspondence; it does not determine a reaction mechanism.").size(11).style(muted_text));
            if let Ok(mut preview) = proposal.apply(&self.tab.doc, true) {
                preview.atom_labels.maps = true;
                for a in &mut preview.atoms {
                    a.display.mapping.show = Some(true);
                }
                body = body.push(
                    crate::canvas::layered::canvas(crate::canvas::DrawingThumbnail(preview))
                        .width(Length::Fill)
                        .height(120),
                );
            }
            for (index, (a, b)) in proposal.pairs.iter().enumerate() {
                let element = self.tab.doc.atom(*a).map_or("?", |a| a.element.as_str());
                let number = proposal.number(*a).unwrap_or_default();
                body = body.push(
                    button(text(format!("Map {number} · {element} → {element}")).size(12))
                        .padding([7, 9])
                        .style(super::workspace::control(false))
                        .on_press_maybe(
                            current.then_some(Message::Mapping(Action::ReviewPair(index))),
                        ),
                );
                let _ = b;
            }
            if !current {
                body = body.push(text("Drawing changed · Run Auto-map again").size(11));
            }
            body = body.push(
                row![
                    command(
                        "Apply reviewed proposal",
                        Message::Mapping(Action::ApplyProposal)
                    )
                    .on_press_maybe(current.then_some(Message::Mapping(Action::ApplyProposal))),
                    command("Discard", Message::Mapping(Action::Dismiss)),
                ]
                .spacing(4),
            );
        }
        container(body).padding([0, 0]).width(Length::Fill).into()
    }
}

#[cfg(test)]
mod tests;
