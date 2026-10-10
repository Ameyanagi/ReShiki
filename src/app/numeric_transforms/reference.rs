//! Reference-edge operations use their own small draft, leaving the existing
//! relative numeric fields and shortcuts unchanged.
use super::{Action as NumericAction, App, Message, control, hover_hint};
use crate::canvas::Tool;
use iced::widget::{column, row, text, tooltip};
use iced::{Element, Length};
use reshiki::{
    document::Point,
    editing::{
        self,
        reference::{self, Edge, Stretch},
    },
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Scope {
    #[default]
    Connected,
    Selected,
}
impl std::fmt::Display for Scope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Connected => "Connected fragment",
            Self::Selected => "Selected objects",
        })
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Pivot {
    #[default]
    Center,
    Start,
    Middle,
    Pinned,
}
impl std::fmt::Display for Pivot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Center => "Center of moved objects",
            Self::Start => "Reference start",
            Self::Middle => "Reference midpoint",
            Self::Pinned => "Pinned point",
        })
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    edge: Edge,
    label: String,
}
impl std::fmt::Display for Choice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}
#[derive(Debug, Clone)]
pub enum Action {
    Open(bool),
    Reference(Choice),
    Scope(Scope),
    Pivot(Pivot),
    PinCenter,
    X(String),
    Y(String),
    Target(String),
    Turn(String),
    Length(String),
    Align(f64),
    AlignTarget,
    Rotate(bool),
    Stretch,
    StretchDrag,
    SwapFixed,
}
pub(super) struct State {
    epoch: Option<u64>,
    open: bool,
    edge: Option<Edge>,
    scope: Scope,
    pivot: Pivot,
    x: String,
    y: String,
    target: String,
    turn: String,
    length: String,
    swap_fixed: bool,
    drag_epoch: Option<u64>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            epoch: None,
            open: false,
            edge: None,
            scope: Default::default(),
            pivot: Default::default(),
            x: "0".into(),
            y: "0".into(),
            target: "0".into(),
            turn: "90".into(),
            length: String::new(),
            swap_fixed: false,
            drag_epoch: None,
        }
    }
}
impl State {
    pub(super) fn stale_stretch(&self, epoch: u64) -> bool {
        self.drag_epoch.is_some_and(|owned| owned != epoch)
    }
    pub(super) fn take_for_epoch(&mut self, epoch: u64) -> Self {
        let mut state = std::mem::take(self);
        if state.epoch.is_some_and(|e| e != epoch) {
            state = Self::default();
        }
        state.epoch = Some(epoch);
        state
    }
}
fn message(action: Action) -> Message {
    Message::NumericTransform(NumericAction::Reference(action))
}
fn number(input: &str, label: &str) -> Result<f32, String> {
    input
        .trim()
        .parse::<f32>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("Enter a finite {label}"))
}

impl App {
    /// A stretch gesture belongs to this tab and drawing epoch, even though the
    /// ordinary palette tool is app-wide. Background tab callbacks have no owner.
    pub(in crate::app) fn cancel_reference_stretch(&mut self) {
        let owned = self
            .tab
            .numeric_transforms
            .reference
            .drag_epoch
            .take()
            .is_some();
        if owned && matches!(self.tool, Tool::StretchBond { .. }) {
            self.select_tool(Tool::Select);
        }
    }
    pub(in crate::app) fn reference_stretch_active(&self, fixed: u64, moving: u64) -> bool {
        self.tool == Tool::StretchBond { fixed, moving }
            && self.tab.numeric_transforms.reference.drag_epoch == Some(self.tab.file_epoch)
    }
    fn reference_choices(&self) -> Vec<Choice> {
        let doc = &self.tab.doc;
        reference::edges(doc, &self.tab.selected)
            .into_iter()
            .map(|edge| {
                let atom_name = |id| {
                    doc.atoms
                        .iter()
                        .position(|a| a.id == id)
                        .and_then(|i| doc.atoms.get(i).map(|a| format!("{} {}", a.element, i + 1)))
                        .unwrap_or_else(|| "Atom".into())
                };
                let label = match edge {
                    Edge::Bond(a, b) => format!("{} → {}", atom_name(a), atom_name(b)),
                    Edge::Graphic(id, index) => format!(
                        "Shape {} · edge {}",
                        doc.graphics
                            .iter()
                            .position(|g| g.id == id)
                            .map_or(1, |i| i + 1),
                        index
                    ),
                };
                Choice { edge, label }
            })
            .collect()
    }
    fn chosen_reference(&self) -> Option<Choice> {
        let choices = self.reference_choices();
        choices
            .iter()
            .find(|c| Some(c.edge) == self.tab.numeric_transforms.reference.edge)
            .cloned()
            .or_else(|| choices.first().cloned())
    }
    fn reference_pivot(&self, ids: &[u64], edge: Edge) -> Result<Point, String> {
        let state = &self.tab.numeric_transforms.reference;
        let (a, b) = edge.endpoints(&self.tab.doc)?;
        Ok(match state.pivot {
            Pivot::Center => {
                editing::rotation_center(&self.tab.doc, ids).ok_or("Select objects to rotate")?
            }
            Pivot::Start => a,
            Pivot::Middle => Point::new(
                ((f64::from(a.x) + f64::from(b.x)) * 0.5) as f32,
                ((f64::from(a.y) + f64::from(b.y)) * 0.5) as f32,
            ),
            Pivot::Pinned => Point::new(
                self.tab
                    .doc
                    .drawing_style
                    .world(number(&state.x, "pivot X coordinate")?),
                self.tab
                    .doc
                    .drawing_style
                    .world(number(&state.y, "pivot Y coordinate")?),
            ),
        })
    }
    fn reference_stretch(&self) -> Result<Stretch, String> {
        let edge = self
            .chosen_reference()
            .ok_or("Select a bond to stretch")?
            .edge;
        let Edge::Bond(a, b) = edge else {
            return Err("Choose a molecular bond to stretch".into());
        };
        if self.tab.numeric_transforms.reference.swap_fixed {
            Stretch::new(&self.tab.doc, b, a)
        } else {
            Stretch::new(&self.tab.doc, a, b)
        }
    }
    fn commit_reference(
        &mut self,
        mut candidate: reshiki::document::Document,
        selected: Vec<u64>,
    ) -> Result<(), String> {
        // Preflight the same reaction/group reconciliation as ordinary edits.
        // A rejected edit must keep its error and must not replace selection.
        let _ = reshiki::transaction::reconcile(&mut candidate, self.tab.doc.clone())
            .map_err(|error| error.message().to_owned())?;
        let before = std::mem::replace(&mut self.tab.doc, candidate);
        self.tab.selected = selected;
        self.changed(before);
        self.sync_numeric_transforms();
        self.error = false;
        self.status = "Drawing geometry updated · Undo restores the operation".into();
        Ok(())
    }
    pub(super) fn reference_transform_action(&mut self, action: Action) {
        if self.tab.cleanup.is_some()
            || self.tab.joining.is_some()
            || self.tab.optimization.is_some()
            || self.tab.inline_text.is_some()
            || self.tab.atom_text.is_some()
        {
            self.status =
                "Finish the active drawing edit before changing reference geometry".into();
            self.error = true;
            return;
        }
        let result = (|| -> Result<(), String> {
            match action {
                Action::Open(value) => self.tab.numeric_transforms.reference.open = value,
                Action::Reference(choice) => {
                    let state = &mut self.tab.numeric_transforms.reference;
                    state.edge = Some(choice.edge);
                    state.length.clear();
                    state.swap_fixed = false;
                }
                Action::Scope(value) => self.tab.numeric_transforms.reference.scope = value,
                Action::Pivot(value) => self.tab.numeric_transforms.reference.pivot = value,
                Action::X(value) => self.tab.numeric_transforms.reference.x = value,
                Action::Y(value) => self.tab.numeric_transforms.reference.y = value,
                Action::Target(value) => self.tab.numeric_transforms.reference.target = value,
                Action::Turn(value) => self.tab.numeric_transforms.reference.turn = value,
                Action::Length(value) => self.tab.numeric_transforms.reference.length = value,
                Action::SwapFixed => {
                    let state = &mut self.tab.numeric_transforms.reference;
                    state.swap_fixed = !state.swap_fixed;
                    state.length.clear();
                }
                Action::PinCenter => {
                    let p = editing::rotation_center(&self.tab.doc, &self.tab.selected)
                        .ok_or("Select atoms or objects whose center you want to pin")?;
                    let scale = self.tab.doc.drawing_style.points_per_world();
                    let state = &mut self.tab.numeric_transforms.reference;
                    state.pivot = Pivot::Pinned;
                    state.x = (p.x * scale).to_string();
                    state.y = (p.y * scale).to_string();
                }
                align @ (Action::Align(_) | Action::AlignTarget) => {
                    let target = match align {
                        Action::Align(target) => target,
                        _ => f64::from(number(
                            &self.tab.numeric_transforms.reference.target,
                            "target direction",
                        )?),
                    };
                    let edge = self
                        .chosen_reference()
                        .ok_or("Select both endpoints of a bond, or a shape with a straight edge")?
                        .edge;
                    let ids = reference::scope(
                        &self.tab.doc,
                        &self.tab.selected,
                        self.tab.numeric_transforms.reference.scope == Scope::Connected,
                    )?;
                    let pivot = self.reference_pivot(&ids, edge)?;
                    let angle = reference::alignment_degrees(
                        &self.tab.doc,
                        edge,
                        target,
                        matches!(align, Action::AlignTarget),
                    )?;
                    let (candidate, selected) =
                        reference::rotate(&self.tab.doc, &ids, pivot, angle, false)?;
                    self.commit_reference(candidate, selected)?;
                }
                Action::Rotate(duplicate) => {
                    let angle = number(
                        &self.tab.numeric_transforms.reference.turn,
                        "rotation angle",
                    )?;
                    let edge = self
                        .chosen_reference()
                        .ok_or("Select a reference bond or edge")?
                        .edge;
                    let ids = reference::scope(
                        &self.tab.doc,
                        &self.tab.selected,
                        self.tab.numeric_transforms.reference.scope == Scope::Connected,
                    )?;
                    let pivot = self.reference_pivot(&ids, edge)?;
                    let (candidate, selected) =
                        reference::rotate(&self.tab.doc, &ids, pivot, angle, duplicate)?;
                    self.commit_reference(candidate, selected)?;
                }
                Action::Stretch | Action::StretchDrag => {
                    let plan = self.reference_stretch()?;
                    if matches!(action, Action::StretchDrag) {
                        self.select_tool(Tool::StretchBond {
                            fixed: plan.fixed,
                            moving: plan.moving,
                        });
                        self.tab.numeric_transforms.reference.drag_epoch =
                            Some(self.tab.file_epoch);
                        self.status =
                            "Drag the moving end or its branch along the bond · Escape cancels"
                                .into();
                        self.error = false;
                    } else {
                        let length = number(
                            &self.tab.numeric_transforms.reference.length,
                            "bond length in points",
                        )?;
                        let length = self.tab.doc.drawing_style.world(length);
                        let candidate = plan.apply(&self.tab.doc, length)?;
                        // Keep the reference's endpoints selected for another length edit.
                        self.commit_reference(candidate, self.tab.selected.clone())?;
                    }
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            self.status = error.clone();
            self.error = true;
            self.tab.numeric_transforms.error = Some(error);
        } else {
            self.tab.numeric_transforms.error = None;
        }
    }
    pub(super) fn reference_transform_panel(&self) -> Element<'_, Message> {
        let state = &self.tab.numeric_transforms.reference;
        let mut body = column![
            reshiki::accessibility::button(
                "reference-controls",
                "Reference bond and edge controls",
                text(if state.open {
                    "▾ Reference: align / stretch"
                } else {
                    "▸ Reference: align / stretch"
                })
                .size(12)
            )
            .padding([7, 9])
            .style(control(false))
            .expanded(state.open)
            .on_press(message(Action::Open(!state.open)))
        ]
        .spacing(8);
        if !state.open {
            return body.into();
        }
        let choices = self.reference_choices();
        let chosen = self.chosen_reference();
        let enabled = chosen.is_some();
        body = body
            .push(text("Select a bond's two atoms or a shape, then choose its reference.").size(11))
            .push(
                crate::appearance::pick_list(choices, chosen, |c| message(Action::Reference(c)))
                    .placeholder("Reference bond / edge")
                    .text_size(12)
                    .padding(6)
                    .width(Length::Fill),
            )
            .push(
                crate::appearance::pick_list(
                    [Scope::Connected, Scope::Selected],
                    Some(state.scope),
                    |s| message(Action::Scope(s)),
                )
                .text_size(12)
                .padding(6)
                .width(Length::Fill),
            )
            .push(
                crate::appearance::pick_list(
                    [Pivot::Center, Pivot::Start, Pivot::Middle, Pivot::Pinned],
                    Some(state.pivot),
                    |p| message(Action::Pivot(p)),
                )
                .text_size(12)
                .padding(6)
                .width(Length::Fill),
            );
        let button = |id: &'static str, label: &'static str, action: Action, enabled: bool| {
            reshiki::accessibility::button(id, label, text(label).size(12))
                .padding([6, 9])
                .style(control(false))
                .on_press_maybe(enabled.then_some(message(action)))
        };
        body = body.push(button(
            "reference-pin-center",
            "Pin selected center",
            Action::PinCenter,
            !self.tab.selected.is_empty(),
        ));
        if state.pivot == Pivot::Pinned {
            body = body.push(
                row![
                    text("X").size(12),
                    crate::appearance::text_input("pt", &state.x)
                        .on_input(|s| message(Action::X(s)))
                        .size(12)
                        .padding(5),
                    text("Y").size(12),
                    crate::appearance::text_input("pt", &state.y)
                        .on_input(|s| message(Action::Y(s)))
                        .size(12)
                        .padding(5),
                    text("pt").size(11)
                ]
                .spacing(4),
            );
        }
        body = body
            .push(
                row![
                    button(
                        "reference-horizontal",
                        "Horizontal",
                        Action::Align(0.),
                        enabled
                    ),
                    button(
                        "reference-vertical",
                        "Vertical",
                        Action::Align(90.),
                        enabled
                    )
                ]
                .spacing(5),
            )
            .push(
                row![
                    crate::appearance::text_input("Direction °", &state.target)
                        .on_input(|s| message(Action::Target(s)))
                        .on_submit(message(Action::AlignTarget))
                        .size(12)
                        .padding(5),
                    button(
                        "reference-to-angle",
                        "To angle",
                        Action::AlignTarget,
                        enabled
                    )
                ]
                .spacing(5),
            )
            .push(
                row![
                    text("Turn").size(12),
                    crate::appearance::text_input("°", &state.turn)
                        .on_input(|s| message(Action::Turn(s)))
                        .size(12)
                        .padding(5),
                    text("°").size(11)
                ]
                .spacing(5),
            )
            .push(
                row![
                    button("reference-rotate", "Rotate", Action::Rotate(false), enabled),
                    button(
                        "reference-copy-rotate",
                        "Copy + rotate",
                        Action::Rotate(true),
                        enabled
                    )
                ]
                .spacing(5),
            );
        let plan = self.reference_stretch();
        match plan {
            Ok(plan) => {
                let fixed = self
                    .tab
                    .doc
                    .atom(plan.fixed)
                    .map_or("atom", |a| a.element.as_str());
                let length = format!(
                    "{:.2}",
                    plan.length * self.tab.doc.drawing_style.points_per_world()
                );
                let end = if state.swap_fixed {
                    "reference end"
                } else {
                    "reference start"
                };
                body=body.push(text(format!("Stretch · fixed: {end} ({fixed}) · {length} pt")).size(11))
                    .push(button("reference-swap-fixed","Swap fixed end",Action::SwapFixed,true))
                    .push(row![crate::appearance::text_input(&length,&state.length).on_input(|s|message(Action::Length(s))).on_submit(message(Action::Stretch)).size(12).padding(5),text("pt").size(11),button("reference-stretch","Set length",Action::Stretch,!state.length.is_empty())].spacing(5))
                    .push(hover_hint(button("reference-stretch-drag","Drag to stretch",Action::StretchDrag,true),"Keeps this bond's original direction; the moving branch stays rigid. No atom merging.",tooltip::Position::Top));
            }
            Err(error) if enabled => body = body.push(text(error).size(11)),
            Err(_) => {}
        }
        body.into()
    }
}

#[cfg(test)]
mod tests;
