//! The reaction inspector edits chemical map numbers and reviews detached fits.
use super::{App, InspectorTab, Message, Tool};
use iced::widget::{Column, button, checkbox, column, container, row, text};
use iced::{Element, Length, Task};
use reshiki::{
    document::Document,
    reaction_mapping::{self as mapping, Budget, Proposal},
};
use std::sync::Arc;

const EDITOR_HINT: &str =
    "Existing maps are anchors. Agents are excluded. Alignment moves complete product molecules.";
const PROPOSAL_HINT: &str =
    "Graph matching suggests correspondence; it does not determine a reaction mechanism.";

#[derive(Default)]
pub(super) struct State {
    atom: Option<u64>,
    text: String,
    serial: u64,
    pending: Option<(u64, u64)>,
    proposal: Option<Arc<Proposal>>,
}
impl State {
    pub(super) fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    pub(super) fn clear_pending(&mut self) {
        self.pending = None;
    }
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
        match action {
            Action::Ready(epoch, serial, result) => self.mapping_ready(epoch, serial, result),
            Action::Open => self.open_mapping(),
            Action::Text(value) => {
                self.tab.mapping.atom = self.mapping_atom();
                self.tab.mapping.text = value;
            }
            Action::Auto => return self.start_auto_map(),
            Action::Dismiss => self.tab.mapping.proposal = None,
            Action::ReviewPair(index) => self.review_pair(index),
            Action::Position => self.position_labels(),
            Action::Apply => {
                let target = self
                    .mapping_atom()
                    .ok_or_else(|| "Select one atom to edit its map".to_string())
                    .and_then(|id| {
                        if self.tab.mapping.atom != Some(id) {
                            return Err("Enter the atom map for this atom first".into());
                        }
                        let number = self
                            .tab
                            .mapping
                            .text
                            .trim()
                            .parse::<u32>()
                            .map_err(|_| mapping::map_range_error())?;
                        Ok((id, number))
                    });
                self.commit_mapping(|before| {
                    let (id, number) = target?;
                    mapping::set_map(before, id, number)
                });
            }
            Action::Clear => {
                let atom = self.mapping_atom().ok_or("Select one atom to edit its map");
                self.commit_mapping(|before| mapping::set_map(before, atom?, 0));
            }
            Action::Pair => {
                let arrow = self.reaction_arrow();
                let selected = self.tab.selected.clone();
                self.commit_mapping(|before| {
                    mapping::pair(
                        before,
                        arrow.ok_or("Assign reactants and products first")?,
                        &selected,
                    )
                });
            }
            Action::Align => {
                let arrow = self.reaction_arrow();
                self.commit_mapping(|before| {
                    mapping::align(before, arrow.ok_or("Assign reactants and products first")?)
                });
            }
            Action::ApplyProposal => {
                let proposal = self.tab.mapping.proposal.clone();
                self.commit_mapping(|before| {
                    proposal
                        .as_ref()
                        .ok_or("Run Auto-map first")?
                        .apply(before, true)
                });
            }
            Action::Show(show) => {
                self.commit_mapping(|before| Ok(mapping::set_maps_visible(before, show)));
            }
            Action::ResetPositions => {
                self.commit_mapping(|before| Ok(mapping::reset_label_offsets(before)));
            }
        }
        Task::none()
    }
    fn mapping_ready(&mut self, epoch: u64, serial: u64, result: Result<Arc<Proposal>, String>) {
        if self.tab.mapping.pending != Some((epoch, serial)) {
            return;
        }
        self.tab.mapping.pending = None;
        if epoch != self.tab.file_epoch {
            return;
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
    }
    fn open_mapping(&mut self) {
        self.inspector_open = true;
        self.inspector_tab = InspectorTab::Reactions;
        self.tool = Tool::Select;
        self.tab.mapping.atom = self.mapping_atom();
        self.tab.mapping.text = self
            .mapping_atom()
            .and_then(|id| self.tab.doc.atom(id))
            .map(|a| a.map_num.to_string())
            .unwrap_or_default();
    }
    fn start_auto_map(&mut self) -> Task<Message> {
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
        Task::perform(calculate(self.tab.doc.clone(), arrow), move |result| {
            Message::Mapping(Action::Ready(epoch, serial, result))
        })
    }
    fn review_pair(&mut self, index: usize) {
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
    }
    fn position_labels(&mut self) {
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
    }
    fn commit_mapping(&mut self, edit: impl FnOnce(&Document) -> Result<Document, String>) {
        let before = self.tab.doc.clone();
        match edit(&before) {
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
    }
    pub(super) fn mapping_panel(&self) -> Element<'_, Message> {
        let mut body = self.map_editor();
        if let Some(proposal) = &self.tab.mapping.proposal {
            body = self.proposal_review(body, proposal);
        }
        container(body).padding([0, 0]).width(Length::Fill).into()
    }
    fn map_editor(&self) -> Column<'_, Message> {
        use super::workspace::{command, muted_text};
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
        column![
            text("Atom mapping").size(16),
            checkbox(self.tab.doc.atom_labels.maps)
                .label("Show maps")
                .text_size(12)
                .on_toggle(|value| Message::Mapping(Action::Show(value))),
            text("Atom map").size(12),
            row![
                crate::appearance::text_input("Select one atom", &value)
                    .on_input(|value| Message::Mapping(Action::Text(value)))
                    .on_submit(Message::Mapping(Action::Apply))
                    .size(12)
                    .padding(7),
                command("Set", Message::Mapping(Action::Apply))
                    .on_press_maybe(atom.map(|_| Message::Mapping(Action::Apply))),
                command("Clear", Message::Mapping(Action::Clear)).on_press_maybe(
                    atom.filter(|a| a.map_num != 0)
                        .map(|_| Message::Mapping(Action::Clear))
                ),
            ]
            .spacing(4),
            command("Pair selected atoms", Message::Mapping(Action::Pair)).on_press_maybe(
                (ready
                    && self
                        .tab
                        .selected
                        .iter()
                        .filter(|id| self.tab.doc.atom(**id).is_some())
                        .count()
                        == 2)
                    .then_some(Message::Mapping(Action::Pair))
            ),
            row![
                command(
                    if self.tab.mapping.pending.is_some() {
                        "Mapping…"
                    } else {
                        "Auto-map"
                    },
                    Message::Mapping(Action::Auto)
                )
                .on_press_maybe(
                    (ready && self.tab.mapping.pending.is_none() && !self.tab.busy)
                        .then_some(Message::Mapping(Action::Auto))
                )
                .width(Length::Fill),
                command("Move map labels", Message::Mapping(Action::Position)).width(Length::Fill),
            ]
            .spacing(4),
            command("Align mapped structures", Message::Mapping(Action::Align))
                .on_press_maybe(ready.then_some(Message::Mapping(Action::Align))),
            command(
                "Reset map label positions",
                Message::Mapping(Action::ResetPositions)
            ),
            text(EDITOR_HINT).size(11).style(muted_text),
        ]
        .spacing(7)
    }
    fn proposal_review<'a>(
        &'a self,
        mut body: Column<'a, Message>,
        proposal: &Proposal,
    ) -> Column<'a, Message> {
        use super::workspace::{command, horizontal_line, muted_text};
        let current = proposal.current(&self.tab.doc);
        let certainty = if !proposal.complete {
            "Budget reached · optimality and uniqueness unknown"
        } else if proposal.multiple_best {
            "Several mappings tie under the graph score"
        } else {
            "One best mapping under the graph score"
        };
        body = body
            .push(horizontal_line())
            .push(text("Review atom maps").size(14))
            .push(text(certainty).size(11).style(muted_text))
            .push(
                text(format!(
                    "{} pairs · {} reactant / {} product atoms unmatched",
                    proposal.pairs.len(),
                    proposal.unmatched_reactants,
                    proposal.unmatched_products,
                ))
                .size(11),
            )
            .push(text(PROPOSAL_HINT).size(11).style(muted_text));
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
        for (index, (a, _)) in proposal.pairs.iter().enumerate() {
            let element = self.tab.doc.atom(*a).map_or("?", |a| a.element.as_str());
            let number = proposal.number(*a).unwrap_or_default();
            body = body.push(
                button(text(format!("Map {number} · {element} → {element}")).size(12))
                    .padding([7, 9])
                    .style(super::workspace::control(false))
                    .on_press_maybe(current.then_some(Message::Mapping(Action::ReviewPair(index)))),
            );
        }
        if !current {
            body = body.push(text("Drawing changed · Run Auto-map again").size(11));
        }
        body.push(
            row![
                command(
                    "Apply reviewed proposal",
                    Message::Mapping(Action::ApplyProposal)
                )
                .on_press_maybe(current.then_some(Message::Mapping(Action::ApplyProposal))),
                command("Discard", Message::Mapping(Action::Dismiss)),
            ]
            .spacing(4),
        )
    }
}

#[cfg(test)]
mod tests;
