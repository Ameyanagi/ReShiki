//! Catalytic cycles: making them, dropping parts in, and their properties.
use super::workspace::{
    accessible_command, control, hover_hint, muted_text, panel_action, section,
};
use super::{App, Message};
use iced::widget::{button, checkbox, column, container, row, text};
use iced::{Alignment, Element, Length, Padding, Task};
use reshiki::cycles::{Cycle, Role, Shape, steps};

mod layout;
use layout::MakeLayout;

#[derive(Debug, Clone)]
pub enum Action {
    /// Propose a cycle of the selection to review before it is made.
    Make,
    /// Apply the proposed cycle edit, as one Undo step.
    MakeApply,
    /// Drop the proposed cycle edit; the drawing stays as it was.
    MakeDiscard,
    /// Run the proposed cycle the other way.
    MakeReverse,
    Insert,
    Remove,
    /// Select every intermediate, reagent and product of the selected cycle.
    SelectStructures,
    Clockwise(bool),
    Shape(Shape),
    /// Preview even spacing of the selected cycle and alignment of its side parts.
    Tidy,
    Edit(Field, String),
    Apply(Field),
    Step(usize),
    ToggleLayout,
    TogglePurpose,
    Reversible(bool),
    Hidden(bool),
    /// The chosen step as a reaction: copy its reaction SMILES.
    CopySmiles,
    /// Propose atom maps for the chosen step.
    AutoMap,
    /// Color the cycle by origin from the chosen step.
    Color,
    /// Check that the chosen step keeps its atoms and charge.
    Check,
}

/// A text field of the cycle card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Radius,
    Width,
    Height,
    Corner,
    Gap,
    Label,
    Name,
    Bow,
    Lead,
    Trail,
}

/// A cycle edit on a copy of the drawing, shown until it is applied or discarded.
#[derive(Debug, Clone)]
pub(super) struct Proposal {
    /// What the drawing shows meanwhile.
    doc: reshiki::document::Document,
    cycle: u64,
    kind: ProposalKind,
}

#[derive(Debug, Clone)]
enum ProposalKind {
    Make {
        /// Whether its parts leave ordinary reactions.
        left: bool,
        /// What it is made of, again on Apply from the drawing as it is then.
        selected: Vec<u64>,
        reversed: bool,
        layout: MakeLayout,
    },
    Tidy {
        source: ProposalSource,
    },
    Refit {
        source: ProposalSource,
        baseline: Box<reshiki::document::Document>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ProposalSource {
    epoch: u64,
    revision: u64,
    labels: u64,
    two_decimal_weight: bool,
}

/// Part names keyed by part, at a drawing revision and computed-label generation.
type PartNames = ((u64, u64), std::collections::HashMap<Vec<u64>, String>);

/// The cycle card's text fields and disclosures.
#[derive(Debug, Clone, Default)]
pub(super) struct Inputs {
    /// The cycle edit under review.
    pub(super) proposal: Option<Box<Proposal>>,
    /// The cycle right-clicked last, which the card and step tools use
    /// while it is selected, also among other selected cycles.
    cycle: Option<u64>,
    step: usize,
    step_expanded: bool,
    layout_open: bool,
    pub(super) purpose_open: bool,
    /// Part names at a drawing revision: a formula costs a chemistry pass.
    names: std::cell::RefCell<PartNames>,
    radius: String,
    width: String,
    height: String,
    corner: String,
    gap: String,
    label: String,
    name: String,
    bow: String,
    lead: String,
    trail: String,
}
impl Inputs {
    fn get_mut(&mut self, field: Field) -> &mut String {
        match field {
            Field::Radius => &mut self.radius,
            Field::Width => &mut self.width,
            Field::Height => &mut self.height,
            Field::Corner => &mut self.corner,
            Field::Gap => &mut self.gap,
            Field::Label => &mut self.label,
            Field::Name => &mut self.name,
            Field::Bow => &mut self.bow,
            Field::Lead => &mut self.lead,
            Field::Trail => &mut self.trail,
        }
    }

    fn get(&self, field: Field) -> &str {
        match field {
            Field::Radius => &self.radius,
            Field::Width => &self.width,
            Field::Height => &self.height,
            Field::Corner => &self.corner,
            Field::Gap => &self.gap,
            Field::Label => &self.label,
            Field::Name => &self.name,
            Field::Bow => &self.bow,
            Field::Lead => &self.lead,
            Field::Trail => &self.trail,
        }
    }
}

pub(super) fn disclosure(
    id: &'static str,
    label: &'static str,
    open: bool,
    action: Action,
) -> reshiki::accessibility::Button<'static, Message> {
    let marker = if open { '▾' } else { '▸' };
    accessible_command(id, label, format!("{label} {marker}"))
        .expanded(open)
        .padding([4, 8])
        .width(Length::Fill)
        .on_press(Message::Cycle(action))
}

fn labelled<'a>(
    label: &'a str,
    field: impl Into<Element<'a, Message>>,
) -> iced::widget::Row<'a, Message> {
    row![text(label).size(11), field.into()]
        .spacing(6)
        .align_y(Alignment::Center)
}

/// What Check step balance found for step `s`, in one line.
fn balance_status(s: usize, balance: &steps::Balance) -> String {
    if balance.balanced() {
        return format!("Step {} is balanced", s + 1);
    }
    let list = |counts: &[(String, u32)]| {
        counts
            .iter()
            .map(|(symbol, n)| match n {
                1 => symbol.clone(),
                n => format!("{n} {symbol}"),
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut parts = vec![format!("Step {} not balanced", s + 1)];
    if !balance.before.is_empty() {
        parts.push(format!("extra on the left: {}", list(&balance.before)));
    }
    if !balance.after.is_empty() {
        parts.push(format!("extra on the right: {}", list(&balance.after)));
    }
    let [before, after] = balance.charge;
    if before != after {
        parts.push(format!("charge {before:+} → {after:+}"));
    }
    parts.join(" · ")
}

/// The drawing with a cycle of `selected` made, as Apply leaves it: parts
/// moved, their captions with them.
fn proposed(
    doc: &reshiki::document::Document,
    selected: &[u64],
    reversed: bool,
    layout: MakeLayout,
    two_decimal_weight: bool,
) -> Result<(reshiki::document::Document, u64, bool), String> {
    let mut made = doc.clone();
    let (cycle, left) = make_configured(&mut made, selected, reversed, layout)?;
    reconcile_preview(&mut made, doc, two_decimal_weight)?;
    Ok((made, cycle, left))
}

/// Match the label and caption policy of committing a drawing edit, without
/// adding history. Both cycle reviews must show exactly what Apply keeps.
fn reconcile_preview(
    doc: &mut reshiki::document::Document,
    before: &reshiki::document::Document,
    two_decimal_weight: bool,
) -> Result<(), String> {
    for link in &mut doc.property_captions {
        link.two_decimal_weight = two_decimal_weight;
    }
    reshiki::atom_labels::refresh::refresh_edited(before, doc);
    let reconciled = reshiki::transaction::reconcile(doc, before.clone())
        .map_err(|rejection| rejection.message().to_owned())?;
    if reconciled.outcome(doc).chemistry_changed {
        reshiki::atom_labels::clear_computed(doc);
    }
    Ok(())
}

fn make_configured(
    doc: &mut reshiki::document::Document,
    selected: &[u64],
    reversed: bool,
    layout: MakeLayout,
) -> Result<(u64, bool), String> {
    let (cycle, left) = reshiki::cycles::make(doc, selected)?;
    if reversed {
        reverse(doc, cycle);
    }
    layout.apply(doc, cycle)?;
    Ok((cycle, left))
}

/// Run cycle `id` the other way.
fn reverse(doc: &mut reshiki::document::Document, id: u64) {
    if let Some(cycle) = doc
        .graphics
        .iter_mut()
        .find(|g| g.id == id)
        .and_then(|g| g.cycle.as_deref_mut())
    {
        reshiki::cycles::reverse(cycle);
    }
}

fn points(world: f32, doc: &reshiki::document::Document) -> String {
    let pt = world * doc.drawing_style.points_per_world();
    format!("{}", (pt * 10.).round() / 10.)
}

impl App {
    pub(super) fn cycle_action(&mut self, action: Action) -> Task<Message> {
        let step_action = matches!(
            action,
            Action::CopySmiles | Action::AutoMap | Action::Color | Action::Check
        );
        if step_action {
            let Some((id, s)) = self.cycle_step() else {
                self.error = true;
                self.status = "Select a catalytic cycle first".into();
                return Task::none();
            };
            return self.step_action(action, id, s);
        }
        match action {
            Action::Make => return self.propose_cycle(),
            Action::MakeApply => return self.apply_cycle_proposal(),
            Action::MakeDiscard => {
                if let Some(proposal) = self.tab.cycle_inputs.proposal.take() {
                    self.error = false;
                    self.status = match proposal.kind {
                        ProposalKind::Make { .. } => "Catalytic cycle discarded",
                        ProposalKind::Tidy { .. } => "Cycle tidy discarded",
                        ProposalKind::Refit { .. } => "Cycle layout discarded",
                    }
                    .into();
                    self.sync_cycle();
                }
                return super::focus_drawing();
            }
            Action::MakeReverse => {
                if let Some(mut p) = self.tab.cycle_inputs.proposal.take() {
                    if let ProposalKind::Make {
                        left,
                        selected,
                        reversed,
                        layout,
                    } = &mut p.kind
                    {
                        match proposed(
                            &self.tab.doc,
                            selected,
                            !*reversed,
                            *layout,
                            self.appearance.two_decimal_weight,
                        ) {
                            Ok((doc, cycle, leaves)) => {
                                (p.doc, p.cycle, *left) = (doc, cycle, leaves);
                                *reversed = !*reversed;
                                self.error = false;
                                self.status =
                                    "Review the catalytic cycle · Enter applies · Esc discards"
                                        .into();
                            }
                            Err(error) => {
                                self.error = true;
                                self.status = error;
                            }
                        }
                    }
                    self.tab.cycle_inputs.proposal = Some(p);
                    self.sync_cycle_proposal();
                }
            }
            Action::Insert => {
                let before = self.tab.doc.clone();
                let bond = self.tab.doc.drawing_style.bond_length_world;
                let center = self.tab.camera.center;
                let id = reshiki::cycles::insert(
                    &mut self.tab.doc,
                    reshiki::document::Point::new(center.x, center.y),
                    bond * 3.,
                );
                self.changed(before);
                self.set_selection(vec![id]);
                self.tool = crate::canvas::Tool::Select;
                self.status = "Choose a shape in Layout or add structures and text".into();
            }
            Action::Remove => {
                let before = self.tab.doc.clone();
                let selected = self.tab.selected.clone();
                let freed = reshiki::cycles::remove(&mut self.tab.doc, &selected);
                if freed == 0 {
                    self.error = true;
                    self.status = "Select a part of a cycle first".into();
                    return Task::none();
                }
                self.changed(before);
                self.status = "Removed from the cycle · it stays in the drawing".into();
            }
            Action::SelectStructures => {
                let Some((_, cycle)) = self.selected_cycle() else {
                    return Task::none();
                };
                let parts: Vec<u64> = cycle.parts().flat_map(|(_, ids)| ids.clone()).collect();
                let ids = reshiki::molecule_names::include_annotations(&self.tab.doc, &parts);
                self.set_selection(ids);
                self.status = "Selected the cycle's structures and texts".into();
                return super::focus_drawing();
            }
            Action::Clockwise(clockwise) => {
                if self
                    .selected_cycle_graphic()
                    .is_some_and(|g| g.cycle_counterclockwise() == clockwise)
                {
                    self.edit_cycle(reshiki::cycles::reverse);
                }
            }
            Action::Shape(shape) => self.change_cycle_shape(shape),
            Action::Tidy => return self.propose_cycle_tidy(),
            Action::Edit(field, value) => *self.tab.cycle_inputs.get_mut(field) = value,
            Action::Apply(field) => self.apply_cycle_field(field),
            Action::Step(step) => {
                let inputs = &mut self.tab.cycle_inputs;
                inputs.step_expanded = inputs.step != step || !inputs.step_expanded;
                if inputs.step != step {
                    inputs.step = step;
                    self.sync_cycle();
                }
            }
            Action::ToggleLayout => {
                self.tab.cycle_inputs.layout_open = !self.tab.cycle_inputs.layout_open;
            }
            Action::TogglePurpose => {
                self.tab.cycle_inputs.purpose_open = !self.tab.cycle_inputs.purpose_open;
            }
            Action::Reversible(on) => self.edit_step(|step| step.reversible = on),
            Action::Hidden(on) => self.edit_step(|step| step.hidden = on),
            Action::CopySmiles | Action::AutoMap | Action::Color | Action::Check => {}
        }
        Task::none()
    }

    /// The selected cycle and its chosen step.
    pub(super) fn cycle_step(&self) -> Option<(u64, usize)> {
        let (id, cycle) = self.selected_cycle()?;
        let last = cycle.steps.len().checked_sub(1)?;
        Some((id, self.tab.cycle_inputs.step.min(last)))
    }

    pub(super) fn cycle_part_selected(&self) -> bool {
        self.tab
            .selected
            .iter()
            .any(|id| reshiki::cycles::role_of(&self.tab.doc, *id).is_some())
    }

    /// Choose the step of the selected cycle that lies toward `p`, as a
    /// right-click on its arc does.
    pub(super) fn choose_step_at(&mut self, p: reshiki::document::Point, hit: &[u64]) {
        let doc = &self.tab.doc;
        let Some(g) = doc
            .graphics
            .iter()
            .find(|g| g.cycle.is_some() && hit.contains(&g.id))
        else {
            return;
        };
        self.tab.cycle_inputs.cycle = Some(g.id);
        if let Some(step) = reshiki::cycles::step_at(doc, g, p) {
            self.tab.cycle_inputs.step = step;
        }
        self.sync_cycle();
    }

    /// Run a reaction tool on step `s` of cycle `id`.
    fn step_action(&mut self, action: Action, id: u64, s: usize) -> Task<Message> {
        let fail = |app: &mut Self, error: String| {
            app.error = true;
            app.status = error;
            Task::none()
        };
        match action {
            Action::CopySmiles => match steps::step(&self.tab.doc, id, s) {
                Ok((copy, _)) => self.copy_made(
                    copy,
                    reshiki::clipboard::CopyFormat::ReactionSmiles,
                    format!("Step {}", s + 1),
                ),
                Err(error) => fail(self, error),
            },
            Action::AutoMap | Action::Color => {
                if let Err(error) = steps::step(&self.tab.doc, id, s) {
                    return fail(self, error);
                }
                if matches!(action, Action::Color) {
                    let before = self.tab.doc.clone();
                    self.tab.doc = super::reactions::maps_hidden(&before);
                    match steps::color(&mut self.tab.doc, id, s) {
                        Ok(()) => {
                            self.changed(before);
                            if !self.error {
                                self.status = super::reactions::COLORED.into();
                            }
                            return Task::none();
                        }
                        Err(refusal) => {
                            self.tab.doc = before;
                            // Map the step first; applying its maps colors it.
                            if refusal != reshiki::reaction_colors::Refusal::Maps {
                                return fail(self, refusal.message().into());
                            }
                        }
                    }
                }
                // The proposal to review shows in Properties.
                self.inspector_open = true;
                self.inspector_tab = super::InspectorTab::Properties;
                self.start_step_auto_map(id, s, matches!(action, Action::Color))
            }
            Action::Check => {
                match steps::balance(&self.tab.doc, id, s) {
                    Ok(balance) => {
                        self.error = false;
                        self.status = balance_status(s, &balance);
                    }
                    Err(error) => return fail(self, error),
                }
                Task::none()
            }
            _ => Task::none(),
        }
    }

    /// Make a cycle of the selection on a copy, to review before applying.
    fn propose_cycle(&mut self) -> Task<Message> {
        // A job still running would finish into a drawing under review.
        if self.tab.busy || self.tab.drawing_edit_active() {
            self.error = true;
            self.status = "Wait for the current task to finish".into();
            return Task::none();
        }
        let selected = self.tab.selected.clone();
        match proposed(
            &self.tab.doc,
            &selected,
            false,
            MakeLayout::default(),
            self.appearance.two_decimal_weight,
        ) {
            Ok((doc, cycle, left)) => {
                self.tab.cycle_inputs.proposal = Some(Box::new(Proposal {
                    doc,
                    cycle,
                    kind: ProposalKind::Make {
                        left,
                        selected,
                        reversed: false,
                        layout: MakeLayout::default(),
                    },
                }));
                self.inspector_open = true;
                self.inspector_tab = super::InspectorTab::Properties;
                self.sync_cycle_proposal();
                self.error = false;
                self.status = "Review the catalytic cycle · Enter applies · Esc discards".into();
            }
            Err(error) => {
                self.error = true;
                self.status = error;
            }
        }
        super::focus_drawing()
    }

    /// Tidy a copy so the drawing and its history stay untouched during review.
    fn propose_cycle_tidy(&mut self) -> Task<Message> {
        if self.tab.busy || self.tab.drawing_edit_active() {
            self.error = true;
            self.status = "Wait for the current task to finish".into();
            return super::focus_drawing();
        }
        let Some((cycle, _)) = self.selected_cycle() else {
            self.error = true;
            self.status = "Select a catalytic cycle first".into();
            return super::focus_drawing();
        };
        self.propose_cycle_tidy_for(cycle)
    }

    fn cycle_proposal_source(&self) -> ProposalSource {
        ProposalSource {
            epoch: self.tab.file_epoch,
            revision: self.tab.revision,
            labels: self.tab.label_generation,
            two_decimal_weight: self.appearance.two_decimal_weight,
        }
    }

    /// Refresh the same target even if a completed import changed selection.
    fn propose_cycle_tidy_for(&mut self, cycle: u64) -> Task<Message> {
        if !self
            .tab
            .doc
            .graphics
            .iter()
            .any(|g| g.id == cycle && g.cycle.is_some())
        {
            self.error = true;
            self.status = "The catalytic cycle is no longer available".into();
            return super::focus_drawing();
        }
        let mut doc = self.tab.doc.clone();
        self.error = false;
        if reshiki::cycles::tidy(&mut doc, cycle) {
            if let Err(error) =
                reconcile_preview(&mut doc, &self.tab.doc, self.appearance.two_decimal_weight)
            {
                self.error = true;
                self.status = error;
                return super::focus_drawing();
            }
            self.tab.cycle_inputs.proposal = Some(Box::new(Proposal {
                doc,
                cycle,
                kind: ProposalKind::Tidy {
                    source: self.cycle_proposal_source(),
                },
            }));
            self.inspector_open = true;
            self.inspector_tab = super::InspectorTab::Properties;
            self.status = "Review cycle tidy · Enter applies · Esc discards".into();
        } else {
            self.status = "The cycle is already tidy".into();
        }
        super::focus_drawing()
    }

    /// Apply the proposed cycle edit in the drawing, as one Undo step.
    fn apply_cycle_proposal(&mut self) -> Task<Message> {
        let Some(proposal) = self.tab.cycle_inputs.proposal.take() else {
            return Task::none();
        };
        if let ProposalKind::Tidy { source } = &proposal.kind
            && *source != self.cycle_proposal_source()
        {
            // Background labels and names may change ink without a revision.
            // Show their newly tidied result before accepting another Apply.
            return self.propose_cycle_tidy_for(proposal.cycle);
        }
        if let ProposalKind::Refit { source, baseline } = &proposal.kind {
            if *source != self.cycle_proposal_source() || **baseline != self.tab.doc {
                self.sync_cycle();
                self.error = true;
                self.status = "Drawing changed · apply the cycle setting again".into();
                return super::focus_drawing();
            }
            self.error = false;
            self.commit_prepared_cycle(proposal.doc);
            self.sync_cycle();
            if !self.error {
                self.status = "Updated the cycle layout".into();
            }
            return super::focus_drawing();
        }
        // Made again, so results that reached the drawing meanwhile stay.
        let before = self.tab.doc.clone();
        let ProposalKind::Make {
            left: _,
            selected,
            reversed,
            layout,
        } = proposal.kind
        else {
            self.error = false;
            if reshiki::cycles::tidy(&mut self.tab.doc, proposal.cycle) {
                self.changed(before);
                if !self.error {
                    self.status = "Tidied the cycle".into();
                }
            } else {
                self.status = "The cycle is already tidy".into();
            }
            return super::focus_drawing();
        };
        let (cycle, left) = match make_configured(&mut self.tab.doc, &selected, reversed, layout) {
            Ok(made) => made,
            Err(error) => {
                self.tab.doc = before;
                self.error = true;
                self.status = error;
                return super::focus_drawing();
            }
        };
        let members = self
            .tab
            .doc
            .groups
            .iter()
            .find(|g| g.members.contains(&cycle))
            .map_or_else(|| vec![cycle], |g| g.members.clone());
        self.changed(before);
        if self.error {
            return super::focus_drawing();
        }
        self.set_selection(members);
        self.tool = crate::canvas::Tool::Select;
        self.status = if left {
            "Made a cycle · its parts left their reactions".into()
        } else {
            format!(
                "Made a cycle · {}-click moves one part",
                crate::platform::alt_key()
            )
        };
        super::focus_drawing()
    }

    /// The drawing with the proposed cycle, while one is under review.
    pub(super) fn cycle_proposal_document(&self) -> Option<&reshiki::document::Document> {
        self.tab.cycle_inputs.proposal.as_ref().map(|p| &p.doc)
    }

    /// While a proposed cycle is under review, Enter applies it, Esc
    /// discards it and edits wait.
    pub(super) fn cycle_proposal_gate(&mut self, message: &Message) -> Option<Task<Message>> {
        let proposal = self.tab.cycle_inputs.proposal.as_ref()?;
        if matches!(message, Message::Escape) {
            return Some(self.cycle_action(Action::MakeDiscard));
        }
        if matches!(message, Message::ContextKey(key) if key == "Enter") {
            return Some(self.cycle_action(Action::MakeApply));
        }
        // Saving keeps the drawing as it is, without the proposed cycle.
        let allowed = matches!(
            message,
            Message::Cycle(Action::MakeApply | Action::MakeDiscard)
                | Message::Save
                | Message::SaveAs
                | Message::SaveFormat(_)
                | Message::KeepFormat
                // An import already running finishes; Apply makes the cycle
                // from the drawing as it is then.
                | Message::Imports(super::import::Action::Loaded(..))
                | Message::Saved(..)
        ) || matches!(message, Message::Cycle(Action::MakeReverse))
            && matches!(proposal.kind, ProposalKind::Make { .. })
            || matches!(
                message,
                Message::Cycle(
                    Action::Shape(_)
                        | Action::Edit(
                            Field::Radius | Field::Width | Field::Height | Field::Corner,
                            _
                        )
                        | Action::Apply(
                            Field::Radius | Field::Width | Field::Height | Field::Corner
                        )
                )
            ) && matches!(proposal.kind, ProposalKind::Make { .. })
            || super::gates::preview_passthrough(message)
            || self.answers_save_dialog(message);
        if allowed {
            None
        } else {
            self.status = match proposal.kind {
                ProposalKind::Make { .. } => {
                    "Review the catalytic cycle · Enter applies · Esc discards"
                }
                ProposalKind::Tidy { .. } => "Review cycle tidy · Enter applies · Esc discards",
                ProposalKind::Refit { .. } => "Review cycle layout · Enter applies · Esc discards",
            }
            .into();
            Some(Task::none())
        }
    }

    /// The proposed cycle's order, to apply or discard.
    pub(super) fn cycle_proposal_card(&self) -> Option<Element<'_, Message>> {
        let proposal = self.tab.cycle_inputs.proposal.as_deref()?;
        if matches!(proposal.kind, ProposalKind::Refit { .. }) {
            return Some(
                column![
                    section("Cycle layout"),
                    text("Review the moved structures and cycle arrows.")
                        .size(11)
                        .style(muted_text),
                    row![
                        accessible_command("cycle.layout.apply", "Apply", "Apply")
                            .padding([6, 14])
                            .style(crate::appearance::primary)
                            .on_press(Message::Cycle(Action::MakeApply)),
                        accessible_command("cycle.layout.discard", "Discard", "Discard")
                            .on_press(Message::Cycle(Action::MakeDiscard)),
                    ]
                    .spacing(4)
                    .align_y(Alignment::Center),
                    text("Enter applies · Esc discards")
                        .size(11)
                        .style(muted_text),
                ]
                .spacing(6)
                .into(),
            );
        }
        if matches!(proposal.kind, ProposalKind::Tidy { .. }) {
            return Some(
                column![
                    section("Tidy cycle"),
                    text("Review the spacing and positions.")
                        .size(11)
                        .style(muted_text),
                    row![
                        accessible_command("cycle.tidy.apply", "Apply", "Apply")
                            .padding([6, 14])
                            .style(crate::appearance::primary)
                            .on_press(Message::Cycle(Action::MakeApply)),
                        accessible_command("cycle.tidy.discard", "Discard", "Discard")
                            .on_press(Message::Cycle(Action::MakeDiscard)),
                    ]
                    .spacing(4)
                    .align_y(Alignment::Center),
                    text("Enter applies · Esc discards")
                        .size(11)
                        .style(muted_text),
                ]
                .spacing(6)
                .into(),
            );
        }
        let g = proposal
            .doc
            .graphics
            .iter()
            .find(|g| g.id == proposal.cycle)?;
        let cycle = g.cycle.as_deref()?;
        let way = if g.cycle_counterclockwise() {
            "Steps run counterclockwise"
        } else {
            "Steps run clockwise"
        };
        let mut body = column![
            section("Make catalytic cycle"),
            layout::controls(g, &self.tab.cycle_inputs),
            text(way).size(11).style(muted_text),
        ]
        .spacing(6);
        for (i, ids) in cycle.intermediates.iter().enumerate() {
            body = body.push(text(format!("{} · {}", i + 1, self.part_name(ids))).size(12));
        }
        if matches!(proposal.kind, ProposalKind::Make { left: true, .. }) {
            body = body.push(
                text("They leave their ordinary reactions")
                    .size(11)
                    .style(muted_text),
            );
        }
        let send = |action| Message::Cycle(action);
        Some(
            body.push(panel_action("Reverse order", send(Action::MakeReverse)))
                .push(
                    row![
                        reshiki::accessibility::button(
                            "cycle.make.apply",
                            "Make catalytic cycle",
                            text("Make catalytic cycle").size(12)
                        )
                        .padding([6, 14])
                        .style(crate::appearance::primary)
                        .on_press(send(Action::MakeApply)),
                        super::workspace::accessible_command(
                            "cycle.make.discard",
                            "Discard",
                            "Discard"
                        )
                        .on_press(send(Action::MakeDiscard)),
                    ]
                    .spacing(4)
                    .align_y(Alignment::Center),
                )
                .push(
                    text("Enter from the drawing makes it · Esc discards")
                        .size(11)
                        .style(muted_text),
                )
                .into(),
        )
    }

    /// The selected cycle: the one whose circle is selected.
    pub(super) fn selected_cycle(&self) -> Option<(u64, &Cycle)> {
        let selected = |id: &u64| self.tab.selected.contains(id);
        let mut cycles = self
            .tab
            .doc
            .graphics
            .iter()
            .filter(|g| selected(&g.id))
            .filter_map(|g| Some((g.id, g.cycle.as_deref()?)));
        let first = cycles.next()?;
        let chosen = self.tab.cycle_inputs.cycle;
        Some(
            std::iter::once(first)
                .chain(cycles)
                .find(|(id, _)| Some(*id) == chosen)
                .unwrap_or(first),
        )
    }

    fn selected_cycle_graphic(&self) -> Option<&reshiki::graphics::Graphic> {
        let (id, _) = self.selected_cycle()?;
        self.tab.doc.graphics.iter().find(|g| g.id == id)
    }

    fn edit_cycle(&mut self, edit: impl FnOnce(&mut Cycle)) {
        self.edit_cycle_with_bow(None, edit);
    }

    fn edit_cycle_with_bow(&mut self, bow_step: Option<usize>, edit: impl FnOnce(&mut Cycle)) {
        let Some((id, _)) = self.selected_cycle() else {
            return;
        };
        let mut candidate = self.tab.doc.clone();
        if let Some(cycle) = candidate
            .graphics
            .iter_mut()
            .find(|g| g.id == id)
            .and_then(|g| g.cycle.as_deref_mut())
        {
            edit(cycle);
        }
        self.finish_cycle_edit(candidate, id, bow_step);
    }

    /// Review edits that transport unselected cycle members before committing them.
    fn finish_cycle_edit(
        &mut self,
        mut candidate: reshiki::document::Document,
        cycle: u64,
        bow_step: Option<usize>,
    ) {
        match reshiki::cycles::fit_edited(&self.tab.doc, &mut candidate, cycle, bow_step) {
            Ok(true) => {
                if let Err(error) = reconcile_preview(
                    &mut candidate,
                    &self.tab.doc,
                    self.appearance.two_decimal_weight,
                ) {
                    self.error = true;
                    self.status = error;
                    return;
                }
                self.tab.cycle_inputs.proposal = Some(Box::new(Proposal {
                    doc: candidate,
                    cycle,
                    kind: ProposalKind::Refit {
                        source: self.cycle_proposal_source(),
                        baseline: Box::new(self.tab.doc.clone()),
                    },
                }));
                self.inspector_open = true;
                self.inspector_tab = super::InspectorTab::Properties;
                self.sync_cycle_proposal();
                self.error = false;
                self.status = "Review cycle layout · Enter applies · Esc discards".into();
            }
            Ok(false) => {
                let before = std::mem::replace(&mut self.tab.doc, candidate);
                self.changed(before);
                self.sync_cycle();
            }
            Err(error) => {
                self.error = true;
                self.status = error;
            }
        }
    }

    /// Install a cycle candidate already normalized and validated against the live drawing.
    pub(super) fn commit_prepared_cycle(&mut self, candidate: reshiki::document::Document) {
        let changed = candidate != self.tab.doc;
        if changed {
            self.tab.erase_stroke = false;
        }
        self.prepare_document_change(changed);
        let replacement = reshiki::transaction::replace(&mut self.tab.doc, candidate);
        self.commit_changed(replacement, false);
    }

    fn edit_step(&mut self, edit: impl FnOnce(&mut reshiki::cycles::Step)) {
        let step = self.tab.cycle_inputs.step;
        self.edit_cycle(|cycle| {
            if let Some(s) = cycle.steps.get_mut(step) {
                edit(s);
            }
        });
    }

    fn apply_cycle_field(&mut self, field: Field) {
        let value = self.tab.cycle_inputs.get_mut(field).trim().to_owned();
        let number = value.parse::<f32>().ok().filter(|v| v.is_finite());
        let world = |pt: f32| reshiki::style::DEFAULT.world(pt);
        let invalid = |app: &mut Self, message: &str| {
            app.error = true;
            app.status = message.into();
        };
        if matches!(
            field,
            Field::Radius | Field::Width | Field::Height | Field::Corner
        ) && self.apply_cycle_layout_field(field, &value, number)
        {
            return;
        }
        match field {
            Field::Label => self.edit_cycle(|c| c.label = value),
            Field::Name => self.edit_step(|s| s.name = value),
            Field::Radius => match number {
                Some(pt) if (10. ..=2000.).contains(&pt) => {
                    let Some((id, _)) = self.selected_cycle() else {
                        return;
                    };
                    let mut candidate = self.tab.doc.clone();
                    reshiki::cycles::set_radius(&mut candidate, id, world(pt));
                    self.finish_cycle_edit(candidate, id, None);
                }
                _ => invalid(self, "Enter a radius of 10–2000 pt"),
            },
            Field::Width | Field::Height | Field::Corner => {}
            Field::Gap => match number {
                Some(pt) if (0. ..=72.).contains(&pt) => self.edit_cycle(|c| c.gap_pt = pt),
                _ => invalid(self, "Enter a gap of 0–72 pt"),
            },
            Field::Bow => match number.or(value.is_empty().then_some(0.)) {
                Some(pt) if (-720. ..=720.).contains(&pt) => {
                    let step = self.tab.cycle_inputs.step;
                    self.edit_cycle_with_bow(Some(step), |cycle| {
                        if let Some(s) = cycle.steps.get_mut(step) {
                            s.bow_pt = pt;
                        }
                    });
                }
                _ => invalid(self, "Enter a bow of −720 to 720 pt"),
            },
            Field::Lead | Field::Trail => match number.or(value.is_empty().then_some(0.)) {
                Some(pt) if (-72. ..=720.).contains(&pt) => self.edit_step(|s| {
                    if field == Field::Lead {
                        s.lead_pt = pt;
                    } else {
                        s.trail_pt = pt;
                    }
                }),
                _ => invalid(self, "Enter extra space of −72 to 720 pt"),
            },
        }
    }

    /// Refresh the cycle card's fields from the selected cycle.
    pub(super) fn sync_cycle(&mut self) {
        let Some((id, cycle)) = self.selected_cycle() else {
            return;
        };
        let cycle = cycle.clone();
        let doc = &self.tab.doc;
        let Some(g) = doc.graphics.iter().find(|g| g.id == id) else {
            return;
        };
        let (_, radius) = g.cycle_circle();
        let inputs = &mut self.tab.cycle_inputs;
        inputs.step = inputs.step.min(cycle.steps.len().saturating_sub(1));
        inputs.radius = points(radius, doc);
        let [width, height] = layout::dimensions(g);
        inputs.width = points(width, doc);
        inputs.height = points(height, doc);
        inputs.corner = g.style.corner_pt.map(|v| v.to_string()).unwrap_or_default();
        inputs.gap = format!("{}", cycle.gap_pt);
        inputs.label = cycle.label.clone();
        let step = cycle.steps.get(inputs.step).cloned().unwrap_or_default();
        inputs.name = step.name;
        let pt = |v: f32| {
            if v == 0. {
                String::new()
            } else {
                format!("{v}")
            }
        };
        inputs.bow = pt(step.bow_pt);
        inputs.lead = pt(step.lead_pt);
        inputs.trail = pt(step.trail_pt);
    }

    /// A short label for a cycle part or reaction participant.
    pub(super) fn part_name(&self, ids: &[u64]) -> String {
        let mut names = self.tab.cycle_inputs.names.borrow_mut();
        // Computed names change without a new revision.
        let at = (self.tab.revision, self.tab.label_generation);
        if names.0 != at {
            *names = (at, Default::default());
        }
        names
            .1
            .entry(ids.to_vec())
            .or_insert_with(|| self.uncached_part_name(ids))
            .clone()
    }

    fn uncached_part_name(&self, ids: &[u64]) -> String {
        let doc = &self.tab.doc;
        let name = if let [id] = ids
            && let Some(a) = doc.annotations.iter().find(|a| a.id == *id)
        {
            a.text.clone()
        } else {
            let atoms = reshiki::editing::analysis_atoms(doc, ids);
            if let Some(link) = reshiki::molecule_names::label(doc, &atoms) {
                link.text.clone()
            } else {
                self.molecular_composition(&atoms, ids).map_or_else(
                    |_| "Structure".into(),
                    |properties| self.shown_formula(&properties.display_formula),
                )
            }
        };
        let line = name.lines().find(|l| !l.trim().is_empty());
        let mut chars = line.unwrap_or_default().trim().chars();
        let mut label: String = chars.by_ref().take(32).collect();
        if chars.next().is_some() {
            label.pop();
            label.push('…');
        }
        label
    }

    fn step_labels(&self, cycle: &Cycle) -> Vec<String> {
        let names: Vec<_> = cycle
            .intermediates
            .iter()
            .map(|ids| self.part_name(ids))
            .collect();
        cycle
            .steps
            .iter()
            .enumerate()
            .map(|(i, step)| match step.name.trim().lines().next() {
                Some(name) => format!("{}  {name}", i + 1),
                None => {
                    let name = |k: usize| names.get(k % names.len()).map_or("", String::as_str);
                    format!("{}  {} → {}", i + 1, name(i), name(i + 1))
                }
            })
            .collect()
    }

    /// Properties of the selected cycle.
    pub(super) fn cycle_card(&self) -> Option<Element<'_, Message>> {
        let (_, cycle) = self.selected_cycle()?;
        let graphic = self.selected_cycle_graphic()?;
        let counterclockwise = graphic.cycle_counterclockwise();
        let inputs = &self.tab.cycle_inputs;
        let send = |action| Message::Cycle(action);
        let toggle = |label, active: bool, action| {
            button(text(label).size(12).width(Length::Fill).center())
                .padding([6, 8])
                .width(Length::Fill)
                .style(control(active))
                .on_press(send(action))
        };
        let input = |id: &'static str, name: &'static str, field: Field, width: f32| {
            let value = inputs.get(field);
            reshiki::accessibility::text_input(id, name, "", value)
                .style(crate::appearance::input_style)
                .on_input(move |v| send(Action::Edit(field, v)))
                .on_submit(send(Action::Apply(field)))
                .size(12)
                .padding(5)
                .width(width)
        };
        let mut layout = column![disclosure(
            "cycle-layout",
            "Layout",
            inputs.layout_open,
            Action::ToggleLayout,
        )]
        .spacing(5);
        if inputs.layout_open {
            layout = layout
                .push(layout::controls(graphic, inputs))
                .push(
                    row![labelled(
                        "Gap (pt)",
                        input(
                            "cycle-gap",
                            "Gap around intermediates (pt)",
                            Field::Gap,
                            40.
                        )
                    ),]
                    .spacing(10),
                )
                .push(labelled(
                    "Center",
                    input(
                        "cycle-label",
                        "Text in the cycle's center",
                        Field::Label,
                        150.,
                    ),
                ));
        }
        let mut rows = column![].spacing(2);
        for (i, (step, label)) in cycle.steps.iter().zip(self.step_labels(cycle)).enumerate() {
            let active = i == inputs.step;
            let expanded = active && inputs.step_expanded;
            let mut entry = column![
                reshiki::accessibility::button(
                    format!("cycle-step-{i}"),
                    label.clone(),
                    container(text(label).size(12).wrapping(text::Wrapping::None)).clip(true),
                )
                .padding([4, 8])
                .width(Length::Fill)
                .style(control(active))
                .expanded(expanded)
                .on_press(send(Action::Step(i))),
            ]
            .spacing(5);
            if expanded {
                let mut details = column![].spacing(6);
                for (role, parts) in [("In", &step.reagents), ("Out", &step.products)] {
                    if !parts.is_empty() {
                        let names = parts
                            .iter()
                            .map(|ids| self.part_name(ids))
                            .collect::<Vec<_>>()
                            .join(", ");
                        details = details.push(text(format!("{role}: {names}")).size(12));
                    }
                }
                details = details
                    .push(labelled(
                        "Name",
                        input("cycle-step-name", "Step name", Field::Name, 170.),
                    ))
                    .push(
                        row![
                            checkbox(step.reversible)
                                .label("Reversible")
                                .on_toggle(|on| Message::Cycle(Action::Reversible(on)))
                                .size(14)
                                .text_size(12),
                            checkbox(step.hidden)
                                .label("Hide arrow")
                                .on_toggle(|on| Message::Cycle(Action::Hidden(on)))
                                .size(14)
                                .text_size(12),
                        ]
                        .spacing(14),
                    )
                    .push(
                        row![
                            labelled(
                                "Bow",
                                input("cycle-step-bow", "Arc bow (pt)", Field::Bow, 40.)
                            ),
                            labelled(
                                "Space in",
                                input(
                                    "cycle-step-lead",
                                    "Extra space after the step's first intermediate (pt)",
                                    Field::Lead,
                                    36.
                                )
                            ),
                            labelled(
                                "out",
                                input(
                                    "cycle-step-trail",
                                    "Extra space before the step's next intermediate (pt)",
                                    Field::Trail,
                                    36.
                                )
                            ),
                        ]
                        .spacing(8),
                    )
                    .push(self.step_reaction_controls());
                entry = entry.push(container(details).padding(Padding::ZERO.left(10.)));
            }
            rows = rows.push(entry);
        }
        let hint = if cycle.steps.is_empty() {
            "Drag a structure or text onto the cycle to add it"
        } else {
            "Drop a structure on an arc to add a step · Click a part twice to move it alone"
        };
        Some(
            column![
                section("Catalytic cycle"),
                hover_hint(
                    row![
                        toggle("Clockwise", !counterclockwise, Action::Clockwise(true)),
                        toggle(
                            "Counterclockwise",
                            counterclockwise,
                            Action::Clockwise(false)
                        ),
                    ]
                    .spacing(4),
                    "Switching direction runs the steps the other way: reagents become products",
                    iced::widget::tooltip::Position::Top,
                ),
                (!cycle.steps.is_empty()).then(|| panel_action("Tidy cycle", send(Action::Tidy))),
                layout,
                (!cycle.steps.is_empty()).then_some(rows),
                text(hint).size(11).style(muted_text),
            ]
            .spacing(8)
            .into(),
        )
    }

    /// The chosen step's reaction tools.
    fn step_reaction_controls(&self) -> Element<'_, Message> {
        let send = |action| Message::Cycle(action);
        let mapping = self.tab.mapping.is_pending();
        let auto = if mapping {
            "Mapping…"
        } else {
            "Auto-map step"
        };
        let free = !mapping && !self.tab.busy;
        column![
            panel_action("Copy step as reaction SMILES", send(Action::CopySmiles)),
            row![
                panel_action(auto, send(Action::AutoMap))
                    .on_press_maybe(free.then_some(send(Action::AutoMap))),
                panel_action("Check step balance", send(Action::Check)),
            ]
            .spacing(4),
            panel_action("Color cycle by origin", send(Action::Color))
                .on_press_maybe(free.then_some(send(Action::Color))),
        ]
        .spacing(4)
        .into()
    }

    /// The atom maps proposed for a cycle step, to review and apply. It
    /// stays while a reviewed pair of atoms is selected.
    pub(super) fn step_review_card(&self) -> Option<Element<'_, Message>> {
        let (_, s) = self.tab.mapping.step?;
        let proposal = self.tab.mapping.proposal.as_ref()?;
        let body = column![
            section("Atom maps"),
            text(format!("Step {}", s + 1)).size(13)
        ]
        .spacing(6);
        Some(self.proposal_review(body, proposal).into())
    }

    /// The role of a selected part that is not its whole cycle.
    pub(super) fn cycle_part_card(&self) -> Option<Element<'_, Message>> {
        if self.selected_cycle().is_some()
            || !self.tab.doc.graphics.iter().any(|g| g.cycle.is_some())
        {
            return None;
        }
        let roles: Vec<Role> = self
            .tab
            .selected
            .iter()
            .filter_map(|id| reshiki::cycles::role_of(&self.tab.doc, *id).map(|(_, r)| r))
            .fold(vec![], |mut roles, role| {
                if !roles.contains(&role) {
                    roles.push(role);
                }
                roles
            });
        let [role] = roles.as_slice() else {
            return None;
        };
        Some(
            column![
                section("Catalytic cycle"),
                text(role.to_string()).size(13),
                panel_action("Remove from cycle", Message::Cycle(Action::Remove)),
            ]
            .spacing(8)
            .into(),
        )
    }
}

#[cfg(test)]
mod refit_tests;
#[cfg(test)]
mod tests;
