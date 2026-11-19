//! Catalytic cycles: making them, dropping parts in, and their properties.
use super::workspace::{control, hover_hint, muted_text, panel_action, section};
use super::{App, Message};
use iced::widget::{button, checkbox, column, row, text};
use iced::{Alignment, Element, Length};
use reshiki::cycles::{Cycle, Role};

#[derive(Debug, Clone)]
pub enum Action {
    Make,
    Insert,
    Remove,
    Clockwise(bool),
    Edit(Field, String),
    Apply(Field),
    Step(StepChoice),
    Reversible(bool),
    Hidden(bool),
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

/// The cycle card's text fields and chosen step.
#[derive(Debug, Clone, Default)]
pub(super) struct Inputs {
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

fn points(world: f32, doc: &reshiki::document::Document) -> String {
    let pt = world * doc.drawing_style.points_per_world();
    format!("{}", (pt * 10.).round() / 10.)
}

impl App {
    pub(super) fn cycle_action(&mut self, action: Action) {
        match action {
            Action::Make => self.make_cycle(),
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
                    return;
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
        }
    }

    fn make_cycle(&mut self) {
        let before = self.tab.doc.clone();
        let selected = self.tab.selected.clone();
        match reshiki::cycles::make(&mut self.tab.doc, &selected) {
            Ok(id) => {
                let members = self
                    .tab
                    .doc
                    .groups
                    .iter()
                    .find(|g| g.members.contains(&id))
                    .map_or_else(|| vec![id], |g| g.members.clone());
                self.changed(before);
                if self.error {
                    return;
                }
                self.set_selection(members);
                self.tool = crate::canvas::Tool::Select;
                self.status = format!(
                    "Made a cycle · {}-click moves one part",
                    crate::platform::alt_key()
                );
            }
            Err(error) => {
                self.tab.doc = before;
                self.error = true;
                self.status = error;
            }
        }
    }

    /// The selected cycle: the one whose circle is selected.
    pub(super) fn selected_cycle(&self) -> Option<(u64, &Cycle)> {
        self.tab
            .doc
            .graphics
            .iter()
            .filter(|g| self.tab.selected.contains(&g.id))
            .find_map(|g| Some((g.id, g.cycle.as_deref()?)))
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
                );
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
