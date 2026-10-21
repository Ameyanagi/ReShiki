//! The reaction inspector edits chemical map numbers and reviews detached fits.
use super::inspector::Section;
use super::workspace::{
    accessible_checkbox, accessible_command, horizontal_line, hover_hint, muted_text,
};
use super::{App, InspectorTab, Message, Tool};
use iced::widget::{Column, column, row, text, tooltip};
use iced::{Alignment, Element, Length, Task};
use reshiki::{
    document::Document,
    reaction_mapping::{self as mapping, Budget, Proposal},
};
use std::sync::Arc;

const EDITOR_HINT: &str = "Auto-map keeps maps you set. Reagents are not mapped.";
const ALIGN_HINT: &str = "Moves complete product molecules";
const MOVE_HINT: &str = "Drag the map numbers on the canvas · Escape finishes";
const PROPOSAL_HINT: &str =
    "Graph matching suggests correspondence; it does not determine a reaction mechanism.";

#[derive(Default)]
pub(super) struct State {
    atom: Option<u64>,
    text: String,
    serial: u64,
    pending: Option<(u64, u64)>,
    proposal: Option<Arc<Proposal>>,
    preview: Option<Preview>,
}

struct Preview {
    key: (u64, u64),
    current: bool,
    document: Option<Document>,
    // Hue previews and Cancel write these without recording history/revision.
    version: u32,
    theme: Option<Box<reshiki::theme_files::ThemeFile>>,
}

impl Preview {
    fn new(key: (u64, u64), proposal: &Proposal, doc: &Document) -> Self {
        let current = proposal.current(doc);
        let document = proposal.apply(doc, true).ok().map(|mut document| {
            document.atom_labels.maps = true;
            for atom in &mut document.atoms {
                atom.display.mapping.show = Some(true);
            }
            document
        });
        Self {
            key,
            current,
            document,
            version: doc.version,
            theme: doc.custom_theme.clone(),
        }
    }
}

impl State {
    fn set_proposal(&mut self, proposal: Option<Arc<Proposal>>) {
        self.proposal = proposal;
        self.preview = None;
    }

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
    reshiki::task::spawn_blocking(move || mapping::propose(&source, arrow, Budget::default()))
        .await
        .map_err(|e| format!("Atom mapping failed: {e}"))?
        .map(Arc::new)
}
impl App {
    pub(super) fn sync_mapping_preview(&mut self) {
        let Some(proposal) = &self.tab.mapping.proposal else {
            self.tab.mapping.preview = None;
            return;
        };
        let key = (self.tab.revision, self.tab.label_generation);
        if let Some(preview) = &self.tab.mapping.preview
            && preview.key == key
            && preview.version == self.tab.doc.version
            && preview.theme == self.tab.doc.custom_theme
        {
            #[cfg(test)]
            {
                let fresh = Preview::new(key, proposal, &self.tab.doc);
                debug_assert_eq!(preview.current, fresh.current);
                debug_assert_eq!(preview.document, fresh.document);
            }
            return;
        }
        self.tab.mapping.preview = Some(Preview::new(key, proposal, &self.tab.doc));
    }

    /// The one selected atom that atom-map commands edit. Shared with the
    /// selection menu.
    pub(super) fn mapping_atom(&self) -> Option<u64> {
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
            Action::Open => return self.open_mapping(),
            Action::Text(value) => {
                self.tab.mapping.atom = self.mapping_atom();
                self.tab.mapping.text = value;
            }
            Action::Auto => return self.start_auto_map(),
            Action::Dismiss => self.tab.mapping.set_proposal(None),
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
                self.tab.mapping.set_proposal(Some(proposal));
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
    fn open_mapping(&mut self) -> Task<Message> {
        self.inspector_open = true;
        self.inspector_tab = InspectorTab::Reactions;
        self.tool = Tool::Select;
        self.tab.mapping.atom = self.mapping_atom();
        self.tab.mapping.text = self
            .mapping_atom()
            .and_then(|id| self.tab.doc.atom(id))
            .map(|a| a.map_num.to_string())
            .unwrap_or_default();
        // The Atom maps section ends the page, after the role list: open it
        // and scroll to it, then reveal its field, which an open review can
        // push above the end of the page.
        self.tab
            .inspector_ui
            .update(super::inspector::Action::Section(Section::AtomMaps, true));
        iced::widget::operation::snap_to(
            "inspector-content",
            iced::widget::operation::RelativeOffset::END,
        )
        .chain(
            iced::advanced::widget::operate(reshiki::accessibility::FocusControl::reveal(
                "mapping.atom",
            ))
            .discard(),
        )
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
        self.tab.mapping.set_proposal(None);
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
                self.tab.mapping.set_proposal(None);
                self.status = "Atom mapping updated".into();
                self.error = false;
            }
            Err(error) => {
                self.status = error;
                self.error = true;
            }
        }
    }
    /// The Atom maps section, after the role list it depends on.
    pub(super) fn mapping_section(&self) -> Element<'_, Message> {
        let ready = self
            .reaction_arrow()
            .and_then(|arrow| mapping::reaction(&self.tab.doc, arrow).ok())
            .is_some_and(|r| r.ready());
        let mapped = self.tab.doc.atoms.iter().filter(|a| a.map_num != 0).count();
        let summary = match mapped {
            _ if !ready => "Assign reactants and products first".to_owned(),
            0 => "No atoms mapped".to_owned(),
            1 => "1 atom mapped".to_owned(),
            n => format!("{n} atoms mapped"),
        };
        let mut body = self.map_editor(ready);
        if let Some(proposal) = &self.tab.mapping.proposal {
            body = self.proposal_review(body, proposal);
        }
        self.inspector_section(Section::AtomMaps, "Atom maps", summary, true, body)
    }
    fn map_editor(&self, ready: bool) -> Column<'_, Message> {
        let atom = self.mapping_atom().and_then(|id| self.tab.doc.atom(id));
        let value = if atom.is_some_and(|a| self.tab.mapping.atom == Some(a.id)) {
            self.tab.mapping.text.clone()
        } else {
            atom.map(|a| a.map_num.to_string()).unwrap_or_default()
        };
        let shown = self.tab.doc.atom_labels.maps;
        let pair = ready
            && self
                .tab
                .selected
                .iter()
                .filter(|id| self.tab.doc.atom(**id).is_some())
                .count()
                == 2;
        let auto = if self.tab.mapping.pending.is_some() {
            "Mapping…"
        } else {
            "Auto-map"
        };
        column![
            accessible_checkbox(
                "mapping.show",
                "Show maps",
                shown,
                Some(Message::Mapping(Action::Show(!shown)))
            ),
            row![
                text("Atom map").size(12),
                reshiki::accessibility::text_input(
                    "mapping.atom",
                    "Atom map",
                    "Select one atom",
                    &value
                )
                .style(crate::appearance::input_style)
                .on_input(|value| Message::Mapping(Action::Text(value)))
                .on_submit(Message::Mapping(Action::Apply))
                .size(12)
                .padding(5)
                .width(Length::Fill),
                // Narrower padding leaves the field room for its placeholder.
                accessible_command("mapping.set", "Set atom map", "Set")
                    .padding([7, 6])
                    .on_press_maybe(atom.map(|_| Message::Mapping(Action::Apply))),
                accessible_command("mapping.clear", "Clear atom map", "Clear")
                    .padding([7, 6])
                    .on_press_maybe(
                        atom.filter(|a| a.map_num != 0)
                            .map(|_| Message::Mapping(Action::Clear))
                    ),
            ]
            .spacing(4)
            .align_y(Alignment::Center),
            row![
                accessible_command("mapping.auto", auto, auto).on_press_maybe(
                    (ready && self.tab.mapping.pending.is_none() && !self.tab.busy)
                        .then_some(Message::Mapping(Action::Auto))
                ),
                accessible_command("mapping.pair", "Pair selected atoms", "Pair selected atoms")
                    .on_press_maybe(pair.then_some(Message::Mapping(Action::Pair))),
            ]
            .spacing(4),
            hover_hint(
                accessible_command(
                    "mapping.align",
                    "Align mapped structures",
                    "Align mapped structures"
                )
                .on_press_maybe(ready.then_some(Message::Mapping(Action::Align))),
                ALIGN_HINT,
                tooltip::Position::Top,
            ),
            row![
                hover_hint(
                    accessible_command("mapping.labels.move", "Move labels", "Move labels")
                        .on_press(Message::Mapping(Action::Position)),
                    MOVE_HINT,
                    tooltip::Position::Top,
                ),
                accessible_command(
                    "mapping.labels.reset",
                    "Reset label positions",
                    "Reset label positions"
                )
                .on_press(Message::Mapping(Action::ResetPositions)),
            ]
            .spacing(4),
            text(EDITOR_HINT).size(11).style(muted_text),
        ]
        .spacing(6)
    }
    fn proposal_review<'a>(
        &'a self,
        mut body: Column<'a, Message>,
        proposal: &Proposal,
    ) -> Column<'a, Message> {
        // A test may install a proposal without updating; no preview means stale.
        let current = self.tab.mapping.preview.as_ref().is_some_and(|p| p.current);
        let certainty = if !proposal.complete {
            "Budget reached · optimality and uniqueness unknown"
        } else if proposal.multiple_best {
            "Several mappings tie under the graph score"
        } else {
            "One best mapping under the graph score"
        };
        body = body
            .push(horizontal_line())
            .push(text("Review atom maps").size(13))
            .push(text(certainty).size(11).style(muted_text))
            .push(
                text(format!(
                    "{} pairs · {} reactant / {} product atoms unmatched",
                    proposal.pairs.len(),
                    proposal.unmatched_reactants,
                    proposal.unmatched_products,
                ))
                .size(11),
            );
        if let Some(document) = self
            .tab
            .mapping
            .preview
            .as_ref()
            .and_then(|preview| preview.document.as_ref())
        {
            body = body.push(
                crate::canvas::layered::canvas(crate::canvas::TemplateThumbnail(document))
                    .width(Length::Fill)
                    .height(120),
            );
        }
        let atom = |id: u64| {
            let element = self.tab.doc.atom(id).map_or("?", |a| a.element.as_str());
            format!("{element} #{id}")
        };
        for (index, (a, b)) in proposal.pairs.iter().enumerate() {
            let number = proposal.number(*a).unwrap_or_default();
            let label = format!("Map {number} · {} → {}", atom(*a), atom(*b));
            body = body.push(
                accessible_command(format!("mapping.review.{index}"), label.clone(), label)
                    .on_press_maybe(current.then_some(Message::Mapping(Action::ReviewPair(index)))),
            );
        }
        if !current {
            body = body.push(text("Drawing changed · Run Auto-map again").size(11));
        }
        body.push(
            row![
                hover_hint(
                    reshiki::accessibility::button(
                        "mapping.apply",
                        "Apply maps",
                        text("Apply maps").size(12)
                    )
                    .padding([6, 14])
                    .style(crate::appearance::primary)
                    .on_press_maybe(current.then_some(Message::Mapping(Action::ApplyProposal))),
                    PROPOSAL_HINT,
                    tooltip::Position::Top,
                ),
                accessible_command("mapping.discard", "Discard", "Discard")
                    .on_press(Message::Mapping(Action::Dismiss)),
            ]
            .spacing(4)
            .align_y(Alignment::Center),
        )
    }
}

#[cfg(test)]
mod tests;
