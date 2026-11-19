//! Catalytic cycles: making them, dropping parts in, and their properties.
use super::workspace::{control, hover_hint, muted_text, panel_action, section};
use super::{App, Message};
use iced::widget::{button, checkbox, column, row, text};
use iced::{Alignment, Element, Length, Task};
use reshiki::cycles::{Cycle, Role, steps};

#[derive(Debug, Clone)]
pub enum Action {
    /// Propose a cycle of the selection to review before it is made.
    Make,
    /// Make the proposed cycle, as one Undo step.
    MakeApply,
    /// Drop the proposed cycle; the drawing stays as it was.
    MakeDiscard,
    /// Run the proposed cycle the other way.
    MakeReverse,
    Insert,
    Remove,
    Clockwise(bool),
    Edit(Field, String),
    Apply(Field),
    Step(StepChoice),
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
    Gap,
    Label,
    Name,
    Bow,
    Lead,
    Trail,
}

/// A cycle made on a copy of the drawing, shown until it is applied or
/// discarded.
#[derive(Debug, Clone)]
pub(super) struct Proposal {
    /// What the drawing shows meanwhile.
    doc: reshiki::document::Document,
    cycle: u64,
    /// Whether its parts leave ordinary reactions.
    left: bool,
    /// What it is made of, again on Apply from the drawing as it is then.
    selected: Vec<u64>,
    reversed: bool,
}

/// The cycle card's text fields and chosen step.
#[derive(Debug, Clone, Default)]
pub(super) struct Inputs {
    /// Make catalytic cycle's proposal under review.
    pub(super) proposal: Option<Box<Proposal>>,
    /// The cycle right-clicked last, which the card and step tools use
    /// while it is selected, also among other selected cycles.
    cycle: Option<u64>,
    step: usize,
    radius: String,
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
            Field::Gap => &mut self.gap,
            Field::Label => &mut self.label,
            Field::Name => &mut self.name,
            Field::Bow => &mut self.bow,
            Field::Lead => &mut self.lead,
            Field::Trail => &mut self.trail,
        }
    }
}

/// A step in the step picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepChoice {
    index: usize,
    label: String,
}
impl std::fmt::Display for StepChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
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
) -> Result<(reshiki::document::Document, u64, bool), String> {
    let mut made = doc.clone();
    let (cycle, left) = reshiki::cycles::make(&mut made, selected)?;
    if reversed {
        reverse(&mut made, cycle);
    }
    reshiki::atom_labels::refresh::refresh_edited(doc, &mut made);
    let _ = reshiki::transaction::reconcile(&mut made, doc.clone());
    Ok((made, cycle, left))
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
                if self.tab.cycle_inputs.proposal.take().is_some() {
                    self.error = false;
                    self.status = "Catalytic cycle discarded".into();
                }
                return super::focus_drawing();
            }
            Action::MakeReverse => {
                if let Some(mut p) = self.tab.cycle_inputs.proposal.take() {
                    p.reversed = !p.reversed;
                    if let Ok((doc, cycle, left)) = proposed(&self.tab.doc, &p.selected, p.reversed)
                    {
                        (p.doc, p.cycle, p.left) = (doc, cycle, left);
                    }
                    self.tab.cycle_inputs.proposal = Some(p);
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
                self.status = "Drag structures or text onto the circle to add them".into();
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
            Action::Clockwise(clockwise) => {
                if self
                    .selected_cycle_graphic()
                    .is_some_and(|g| g.cycle_counterclockwise() == clockwise)
                {
                    self.edit_cycle(reshiki::cycles::reverse);
                }
            }
            Action::Edit(field, value) => *self.tab.cycle_inputs.get_mut(field) = value,
            Action::Apply(field) => self.apply_cycle_field(field),
            Action::Step(choice) => {
                self.tab.cycle_inputs.step = choice.index;
                self.sync_cycle();
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
                        // Map the step first; applying its maps colors it.
                        Err(reshiki::reaction_colors::Refusal::Maps) => {}
                        Err(refusal) => return fail(self, refusal.message().into()),
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
        match proposed(&self.tab.doc, &selected, false) {
            Ok((doc, cycle, left)) => {
                self.tab.cycle_inputs.proposal = Some(Box::new(Proposal {
                    doc,
                    cycle,
                    left,
                    selected,
                    reversed: false,
                }));
                self.inspector_open = true;
                self.inspector_tab = super::InspectorTab::Properties;
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

    /// Make the proposed cycle in the drawing, as one Undo step.
    fn apply_cycle_proposal(&mut self) -> Task<Message> {
        let Some(proposal) = self.tab.cycle_inputs.proposal.take() else {
            return Task::none();
        };
        // Made again, so results that reached the drawing meanwhile stay.
        let before = self.tab.doc.clone();
        let (cycle, left) = match reshiki::cycles::make(&mut self.tab.doc, &proposal.selected) {
            Ok(made) => made,
            Err(error) => {
                self.tab.doc = before;
                self.error = true;
                self.status = error;
                return super::focus_drawing();
            }
        };
        if proposal.reversed {
            reverse(&mut self.tab.doc, cycle);
        }
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
        self.tab.cycle_inputs.proposal.as_ref()?;
        if matches!(message, Message::Escape) {
            return Some(self.cycle_action(Action::MakeDiscard));
        }
        if matches!(message, Message::ContextKey(key) if key == "Enter") {
            return Some(self.cycle_action(Action::MakeApply));
        }
        // Saving keeps the drawing as it is, without the proposed cycle.
        let allowed = matches!(
            message,
            Message::Cycle(Action::MakeApply | Action::MakeDiscard | Action::MakeReverse)
                | Message::Save
                | Message::SaveAs
                | Message::SaveFormat(_)
                | Message::KeepFormat
                // An import already running finishes; Apply makes the cycle
                // from the drawing as it is then.
                | Message::Imports(super::import::Action::Loaded(..))
                | Message::Saved(..)
        ) || super::gates::preview_passthrough(message)
            || self.answers_save_dialog(message);
        if allowed {
            None
        } else {
            self.status = "Review the catalytic cycle · Enter applies · Esc discards".into();
            Some(Task::none())
        }
    }

    /// The proposed cycle's order, to apply or discard.
    pub(super) fn cycle_proposal_card(&self) -> Option<Element<'_, Message>> {
        let proposal = self.tab.cycle_inputs.proposal.as_deref()?;
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
            text(way).size(11).style(muted_text),
        ]
        .spacing(6);
        for (i, ids) in cycle.intermediates.iter().enumerate() {
            body = body.push(text(format!("{} · {}", i + 1, self.part_name(ids))).size(12));
        }
        if proposal.left {
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
                    text("Enter makes it · Esc discards")
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
        let Some((id, _)) = self.selected_cycle() else {
            return;
        };
        let before = self.tab.doc.clone();
        if let Some(cycle) = self
            .tab
            .doc
            .graphics
            .iter_mut()
            .find(|g| g.id == id)
            .and_then(|g| g.cycle.as_deref_mut())
        {
            edit(cycle);
        }
        self.changed(before);
        self.sync_cycle();
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
        match field {
            Field::Label => self.edit_cycle(|c| c.label = value),
            Field::Name => self.edit_step(|s| s.name = value),
            Field::Radius => match number {
                Some(pt) if (10. ..=2000.).contains(&pt) => {
                    let Some((id, _)) = self.selected_cycle() else {
                        return;
                    };
                    let before = self.tab.doc.clone();
                    reshiki::cycles::set_radius(&mut self.tab.doc, id, world(pt));
                    self.changed(before);
                    self.sync_cycle();
                }
                _ => invalid(self, "Enter a radius of 10–2000 pt"),
            },
            Field::Gap => match number {
                Some(pt) if (0. ..=72.).contains(&pt) => self.edit_cycle(|c| c.gap_pt = pt),
                _ => invalid(self, "Enter a gap of 0–72 pt"),
            },
            Field::Bow => match number.or(value.is_empty().then_some(0.)) {
                Some(pt) if (-720. ..=720.).contains(&pt) => self.edit_step(|s| s.bow_pt = pt),
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

    /// What a part shows in the parts list: its text, or its size.
    fn part_name(&self, ids: &[u64]) -> String {
        let doc = &self.tab.doc;
        if let [id] = ids
            && let Some(a) = doc.annotations.iter().find(|a| a.id == *id)
        {
            return a.text.lines().next().unwrap_or_default().to_owned();
        }
        let atoms = ids.iter().filter(|id| doc.atom(**id).is_some()).count();
        format!("Structure, {atoms} atoms")
    }

    /// Properties of the selected cycle.
    pub(super) fn cycle_card(&self) -> Option<Element<'_, Message>> {
        let (_, cycle) = self.selected_cycle()?;
        let counterclockwise = self.selected_cycle_graphic()?.cycle_counterclockwise();
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
            let value = match field {
                Field::Radius => &inputs.radius,
                Field::Gap => &inputs.gap,
                Field::Label => &inputs.label,
                Field::Name => &inputs.name,
                Field::Bow => &inputs.bow,
                Field::Lead => &inputs.lead,
                Field::Trail => &inputs.trail,
            };
            reshiki::accessibility::text_input(id, name, "", value)
                .style(crate::appearance::input_style)
                .on_input(move |v| send(Action::Edit(field, v)))
                .on_submit(send(Action::Apply(field)))
                .size(12)
                .padding(5)
                .width(width)
        };
        let mut body = column![
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
            row![
                labelled(
                    "Radius (pt)",
                    input("cycle-radius", "Cycle radius (pt)", Field::Radius, 52.)
                ),
                labelled(
                    "Gap (pt)",
                    input(
                        "cycle-gap",
                        "Gap around intermediates (pt)",
                        Field::Gap,
                        40.
                    )
                ),
            ]
            .spacing(10),
            labelled(
                "Center",
                input(
                    "cycle-label",
                    "Text in the cycle's center",
                    Field::Label,
                    150.
                )
            ),
        ]
        .spacing(8);
        let names: Vec<String> = cycle
            .intermediates
            .iter()
            .map(|ids| self.part_name(ids))
            .collect();
        let n = names.len();
        if n > 0 {
            let next = names.iter().cycle().skip(1);
            let choices: Vec<StepChoice> = names
                .iter()
                .zip(next)
                .zip(&cycle.steps)
                .enumerate()
                .map(|(i, ((from, to), step))| StepChoice {
                    index: i,
                    label: match step.name.trim() {
                        "" => format!("Step {}: {from} → {to}", i + 1),
                        name => format!("Step {}: {name}", i + 1),
                    },
                })
                .collect();
            let current = choices.get(inputs.step).cloned();
            let step = cycle.steps.get(inputs.step).cloned().unwrap_or_default();
            body = body
                .push(
                    crate::appearance::pick_list(choices, current, |c| {
                        Message::Cycle(Action::Step(c))
                    })
                    .text_size(12)
                    .padding(6)
                    .width(Length::Fill),
                )
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
        }
        let mut parts = column![text("Parts").size(11).style(muted_text)].spacing(3);
        for (role, ids) in cycle.parts() {
            parts = parts.push(text(format!("{role} · {}", self.part_name(ids))).size(12));
        }
        if n == 0 {
            parts = parts.push(
                text("Drag a structure or text onto the circle to add it")
                    .size(12)
                    .style(muted_text),
            );
        }
        body = body.push(parts).push(
            text(format!(
                "{}-click a part to move it alone; the arrows follow",
                crate::platform::alt_key()
            ))
            .size(11)
            .style(muted_text),
        );
        Some(body.into())
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
            text("Step as a reaction").size(11).style(muted_text),
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
mod tests;
