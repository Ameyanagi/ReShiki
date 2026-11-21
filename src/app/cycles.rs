//! Catalytic cycles: making them, dropping parts in, and their properties.
use super::workspace::{
    accessible_command, control, hover_hint, muted_text, panel_action, section,
};
use super::{App, Message};
use iced::widget::{button, checkbox, column, container, row, text};
use iced::{Alignment, Element, Length, Padding, Task};
use reshiki::cycles::{Cycle, Role, Shape, steps};

mod layout;

#[derive(Debug, Clone)]
pub enum Action {
    /// Make a cycle of the selection immediately, as one Undo step.
    Make,
    /// Apply the proposed cycle layout edit, as one Undo step.
    LayoutApply,
    /// Drop the proposed cycle layout edit; the drawing stays as it was.
    LayoutDiscard,
    Insert,
    Remove,
    /// Select every intermediate, reagent and product of the selected cycle.
    SelectStructures,
    Clockwise(bool),
    Shape(Shape),
    /// Space the selected cycle evenly and align its side parts, as one Undo step.
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

/// A cycle layout edit on a copy, shown until it is applied or discarded.
#[derive(Debug, Clone)]
pub(super) struct Proposal {
    /// What the drawing shows meanwhile.
    doc: reshiki::document::Document,
    cycle: u64,
    source: ProposalSource,
    baseline: Box<reshiki::document::Document>,
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

/// Match the label and caption policy of committing a drawing edit, without
/// adding history. The layout review must show exactly what Apply keeps.
fn reconcile_preview(
    doc: &mut reshiki::document::Document,
    before: &reshiki::document::Document,
) -> Result<(), String> {
    // Geometry edits preserve each existing caption's precision, including an
    // imported default and explicitly chosen mass-line decimal places.
    reshiki::atom_labels::refresh::refresh_edited(before, doc);
    let reconciled = reshiki::transaction::reconcile(doc, before.clone())
        .map_err(|rejection| rejection.message().to_owned())?;
    if reconciled.outcome(doc).chemistry_changed {
        reshiki::atom_labels::clear_computed(doc);
    }
    Ok(())
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
            Action::Make => return self.make_cycle(),
            Action::LayoutApply => return self.apply_cycle_proposal(),
            Action::LayoutDiscard => {
                if self.tab.cycle_inputs.proposal.take().is_some() {
                    self.error = false;
                    self.status = "Cycle layout discarded".into();
                    self.sync_cycle();
                }
                return super::focus_drawing();
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
            Action::Tidy => return self.tidy_cycle(),
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

    /// Make the selected structures and texts into a cycle, as one Undo step.
    fn make_cycle(&mut self) -> Task<Message> {
        if self.tab.busy || self.tab.drawing_edit_active() {
            self.error = true;
            self.status = "Wait for the current task to finish".into();
            return super::focus_drawing();
        }
        let before = self.tab.doc.clone();
        let selected = self.tab.selected.clone();
        let (cycle, left) = match reshiki::cycles::make(&mut self.tab.doc, &selected) {
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
        self.error = false;
        self.changed(before);
        if self.error {
            return super::focus_drawing();
        }
        self.set_selection(members);
        self.tool = crate::canvas::Tool::Select;
        self.inspector_open = true;
        self.inspector_tab = super::InspectorTab::Properties;
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

    pub(super) fn can_tidy_cycle(&self) -> bool {
        !self.tab.busy
            && !self.tab.drawing_edit_active()
            && self
                .selected_cycle()
                .is_some_and(|(_, cycle)| !cycle.steps.is_empty())
    }

    /// Tidy the selected cycle immediately, as one Undo step.
    fn tidy_cycle(&mut self) -> Task<Message> {
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
        let before = self.tab.doc.clone();
        self.error = false;
        if reshiki::cycles::tidy(&mut self.tab.doc, cycle) {
            self.changed(before);
            if !self.error {
                self.status = "Tidied the cycle".into();
            }
        } else {
            self.status = "The cycle is already tidy".into();
        }
        super::focus_drawing()
    }

    fn cycle_proposal_source(&self) -> ProposalSource {
        ProposalSource {
            epoch: self.tab.file_epoch,
            revision: self.tab.revision,
            labels: self.tab.label_generation,
            two_decimal_weight: self.appearance.two_decimal_weight,
        }
    }

    /// Apply the proposed cycle layout edit in the drawing, as one Undo step.
    fn apply_cycle_proposal(&mut self) -> Task<Message> {
        let Some(proposal) = self.tab.cycle_inputs.proposal.take() else {
            return Task::none();
        };
        if proposal.source != self.cycle_proposal_source() || *proposal.baseline != self.tab.doc {
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
        super::focus_drawing()
    }

    /// The drawing with the proposed cycle, while one is under review.
    pub(super) fn cycle_proposal_document(&self) -> Option<&reshiki::document::Document> {
        self.tab.cycle_inputs.proposal.as_ref().map(|p| &p.doc)
    }

    /// While a proposed cycle layout is under review, Enter applies it, Esc
    /// discards it and edits wait.
    pub(super) fn cycle_proposal_gate(&mut self, message: &Message) -> Option<Task<Message>> {
        self.tab.cycle_inputs.proposal.as_ref()?;
        if matches!(message, Message::Escape) {
            return Some(self.cycle_action(Action::LayoutDiscard));
        }
        if matches!(message, Message::ContextKey(key) if key == "Enter") {
            return Some(self.cycle_action(Action::LayoutApply));
        }
        // Saving keeps the drawing as it is, without the proposed layout edit.
        let allowed = matches!(
            message,
            Message::Cycle(Action::LayoutApply | Action::LayoutDiscard)
                | Message::Save
                | Message::SaveAs
                | Message::SaveFormat(_)
                | Message::KeepFormat
                // A completed import invalidates the layout baseline.
                | Message::Imports(super::import::Action::Loaded(..))
                | Message::Saved(..)
        ) || super::gates::preview_passthrough(message)
            || self.answers_save_dialog(message);
        if allowed {
            None
        } else {
            self.status = "Review cycle layout · Enter applies · Esc discards".into();
            Some(Task::none())
        }
    }

    /// The proposed cycle layout, to apply or discard.
    pub(super) fn cycle_proposal_card(&self) -> Option<Element<'_, Message>> {
        self.tab.cycle_inputs.proposal.as_ref()?;
        Some(
            column![
                section("Cycle layout"),
                text("Review the moved structures and cycle arrows.")
                    .size(11)
                    .style(muted_text),
                row![
                    accessible_command("cycle.layout.apply", "Apply", "Apply")
                        .padding([6, 14])
                        .style(crate::appearance::primary)
                        .on_press(Message::Cycle(Action::LayoutApply)),
                    accessible_command("cycle.layout.discard", "Discard", "Discard")
                        .on_press(Message::Cycle(Action::LayoutDiscard)),
                ]
                .spacing(4)
                .align_y(Alignment::Center),
                text("Enter applies · Esc discards")
                    .size(11)
                    .style(muted_text),
            ]
            .spacing(6)
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
                if let Err(error) = reconcile_preview(&mut candidate, &self.tab.doc) {
                    self.error = true;
                    self.status = error;
                    return;
                }
                self.tab.cycle_inputs.proposal = Some(Box::new(Proposal {
                    doc: candidate,
                    cycle,
                    source: self.cycle_proposal_source(),
                    baseline: Box::new(self.tab.doc.clone()),
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
