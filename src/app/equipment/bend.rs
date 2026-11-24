//! An abortable rigid-tube bend. Typed values and canvas dragging prepare the
//! same immutable candidate; only Apply records a document edit.
use super::{App, Message};
use crate::app::{
    InspectorTab, document_tab::DocumentTab, document_tab::TabId, results, workspace,
};
use crate::canvas::{Edit, Tool, TubeBend};
use iced::widget::{column, row, text};
use iced::{Alignment, Element, Length, Task, keyboard};
use reshiki::document::{Document, Point};
use reshiki::equipment::tube_bend::{Draft, supports};
use std::collections::BTreeMap;

pub(crate) type Token = (u64, u64);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Target {
    tab: TabId,
    epoch: u64,
    revision: u64,
    piece: u64,
    at: Point,
}

impl Target {
    fn current(self, tab: &DocumentTab) -> bool {
        self.tab == tab.id && self.epoch == tab.file_epoch && self.revision == tab.revision
    }
}

#[derive(Debug, Clone)]
pub enum Action {
    Begin(Target),
    Angle(Token, String),
    Radius(Token, String),
    Drag(Token, Option<Point>),
    Apply(Token),
    ApplyCurrent,
    Cancel(Token),
}

struct Candidate {
    doc: Document,
    reconciled: reshiki::transaction::Reconciled,
    cut_length: Option<f64>,
}

pub(in crate::app) struct State {
    target: Target,
    token: Token,
    base: Document,
    model: Draft,
    angle: String,
    radius: String,
    candidate: Option<Candidate>,
    notice: Option<String>,
}

impl State {
    fn current(&self, tab: &DocumentTab) -> bool {
        self.target.current(tab)
    }

    fn refresh(&mut self) {
        match self.prepare() {
            Ok(candidate) => {
                self.candidate = candidate;
                self.notice = None;
            }
            Err(error) => self.notice = Some(error),
        }
    }

    fn can_apply(&self) -> bool {
        self.candidate.is_some() && self.notice.is_none()
    }

    fn prepare(&self) -> Result<Option<Candidate>, String> {
        if let Some(reason) = self.model.blocked_edit() {
            return Err(reason.into());
        }
        let angle = self
            .angle
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite())
            .ok_or("Enter a finite bend angle in degrees.")?;
        let radius = self
            .radius
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite() && *n > 0.)
            .ok_or("Enter a positive bend radius in millimetres.")?;
        if angle == 0. && !self.model.is_existing_bend() {
            return Ok(None);
        }
        let replacement = self.model.preview(angle, radius)?;
        let original = self
            .base
            .graphics
            .iter()
            .find(|g| g.id == self.target.piece);
        if original == Some(&replacement) {
            return Ok(None);
        }
        let cut_length = reshiki::equipment::parts_list::graphic_cut_length_mm(&replacement)?;
        let mut doc = self.base.clone();
        let graphic = doc
            .graphics
            .iter_mut()
            .find(|g| g.id == self.target.piece)
            .ok_or("The tube is no longer in this drawing.")?;
        *graphic = replacement;
        let reconciled = reshiki::transaction::reconcile_with_layout_anchor(
            &mut doc,
            self.base.clone(),
            self.target.piece,
        )
        .map_err(|error| error.message().to_owned())?;
        // Layouts must not repair the bend by moving an attached apparatus or
        // silently releasing a seat. An impossible candidate stays a draft.
        if !self.base.seats.iter().all(|seat| doc.seats.contains(seat)) {
            return Err(
                "This bend cannot keep the existing connections. Change the angle or radius."
                    .into(),
            );
        }
        let by_id: BTreeMap<_, _> = doc.graphics.iter().map(|g| (g.id, g)).collect();
        if self
            .base
            .graphics
            .iter()
            .filter(|g| g.id != self.target.piece)
            .any(|before| {
                by_id.get(&before.id).is_none_or(|after| {
                    before.origin != after.origin
                        || before.axis_x != after.axis_x
                        || before.axis_y != after.axis_y
                })
            })
        {
            return Err(
                "This bend would move other apparatus in the layout. Change the angle or radius."
                    .into(),
            );
        }
        Ok(Some(Candidate {
            doc,
            reconciled,
            cut_length,
        }))
    }
}

fn message(action: Action) -> Message {
    Message::Equipment(super::Action::Bend(action))
}

impl App {
    fn current_tube_bend(&self) -> Option<&State> {
        self.tab
            .tube_bend
            .as_ref()
            .filter(|state| state.current(&self.tab))
    }

    pub(in crate::app) fn tube_bend_active(&self) -> bool {
        self.current_tube_bend().is_some()
    }

    pub(in crate::app) fn tube_bend_preview(&self) -> Option<&Document> {
        self.current_tube_bend()?
            .candidate
            .as_ref()
            .map(|candidate| &candidate.doc)
    }

    pub(in crate::app) fn tube_bend_canvas(&self) -> Option<TubeBend> {
        let state = self.current_tube_bend()?;
        let leg = state
            .candidate
            .as_ref()
            .and_then(|candidate| {
                candidate
                    .doc
                    .graphics
                    .iter()
                    .find(|g| g.id == state.target.piece)
                    .and_then(|graphic| state.model.outgoing_leg(graphic))
            })
            .or_else(|| state.model.starting_leg());
        Some(TubeBend {
            token: state.token,
            pivot: state.model.pivot(),
            leg,
            can_rotate: state.model.can_rotate(),
        })
    }

    pub(in crate::app) fn tube_bend_context_command(&self) -> Option<(&'static str, Action)> {
        let menu = self.context_menu.as_ref()?;
        let [piece] = menu.hit.as_deref()? else {
            return None;
        };
        let at = menu.at?;
        let graphic = self.tab.doc.graphics.iter().find(|g| g.id == *piece)?;
        if !supports(graphic) {
            return None;
        }
        let editing =
            Draft::new(&self.tab.doc, *piece, at).is_ok_and(|draft| draft.is_existing_bend());
        let target = Target {
            tab: self.tab.id,
            epoch: self.tab.file_epoch,
            revision: self.tab.revision,
            piece: *piece,
            at,
        };
        Some((
            if editing {
                "Edit Bend…"
            } else {
                "Add Bend Point…"
            },
            Action::Begin(target),
        ))
    }

    pub(in crate::app) fn tube_bend_action(&mut self, action: Action) -> Task<Message> {
        if let Action::Begin(target) = action {
            if self.tube_bend_active() || !target.current(&self.tab) {
                return Task::none();
            }
            let model = match Draft::new(&self.tab.doc, target.piece, target.at) {
                Ok(model) => model,
                Err(error) => {
                    self.status = error;
                    self.error = true;
                    return Task::none();
                }
            };
            self.tab.tube_bend_serial = self.tab.tube_bend_serial.wrapping_add(1);
            let mut state = State {
                target,
                token: (self.tab.file_epoch, self.tab.tube_bend_serial),
                base: self.tab.doc.clone(),
                angle: model.default_angle_deg().to_string(),
                radius: model.default_radius_mm().to_string(),
                model,
                candidate: None,
                notice: None,
            };
            state.refresh();
            let old_width = self.inspector_width();
            let old_viewport = self.viewport;
            let was_open = self.inspector_open;
            self.tab.tube_bend = Some(state);
            self.tool = Tool::Select;
            self.tab.connect = None;
            self.context_menu = None;
            self.inspector_tab = InspectorTab::Properties;
            self.inspector_open = true;
            if !was_open {
                self.preserve_inspector_page_position(old_width, old_viewport);
            }
            self.tube_bend_status();
            return crate::app::focus_drawing();
        }
        let Some(state) = self.current_tube_bend() else {
            return Task::none();
        };
        let token = match &action {
            Action::Angle(token, _)
            | Action::Radius(token, _)
            | Action::Drag(token, _)
            | Action::Apply(token)
            | Action::Cancel(token) => *token,
            Action::ApplyCurrent => state.token,
            Action::Begin(_) => return Task::none(),
        };
        if state.token != token {
            return Task::none();
        }
        match action {
            Action::Apply(_) | Action::ApplyCurrent => {
                if !state.can_apply() {
                    return Task::none();
                }
                let Some(mut state) = self.tab.tube_bend.take() else {
                    return Task::none();
                };
                if state.base != self.tab.doc {
                    self.status = "The drawing changed. Start the tube bend again.".into();
                    self.error = true;
                    return crate::app::focus_drawing();
                }
                let Some(candidate) = state.candidate.take() else {
                    self.tab.tube_bend = Some(state);
                    self.tube_bend_status();
                    return Task::none();
                };
                let editing = state.model.is_existing_bend();
                self.prepare_document_change(candidate.doc != self.tab.doc);
                self.tab.erase_stroke = false;
                self.tab.doc = candidate.doc;
                self.commit_changed(candidate.reconciled, false);
                self.tab.selected = vec![state.target.piece];
                if !self.error {
                    self.status = if editing {
                        "Tube bend updated"
                    } else {
                        "Tube bend added"
                    }
                    .into();
                    if let Some(length) = candidate.cut_length {
                        self.status
                            .push_str(&format!(" · Cut length {length:.3} mm"));
                    }
                }
                return crate::app::focus_drawing();
            }
            Action::Cancel(_) => {
                self.tab.tube_bend = None;
                self.status = "Tube bend canceled".into();
                self.error = false;
                return crate::app::focus_drawing();
            }
            Action::Angle(_, value) => {
                if let Some(state) = &mut self.tab.tube_bend {
                    state.angle = value;
                    state.refresh();
                }
            }
            Action::Radius(_, value) => {
                if let Some(state) = &mut self.tab.tube_bend {
                    state.radius = value;
                    state.refresh();
                }
            }
            Action::Drag(_, at) => {
                if let Some(state) = &mut self.tab.tube_bend
                    && state.model.can_rotate()
                    && let Some(angle) = at.and_then(|at| state.model.angle_to(at))
                {
                    let angle = ((angle * 10.).round() / 10.).to_string();
                    if state.angle != angle {
                        state.angle = angle;
                        state.refresh();
                    }
                }
            }
            Action::Begin(_) => {}
        }
        self.tube_bend_status();
        Task::none()
    }

    fn tube_bend_status(&mut self) {
        let Some(state) = &self.tab.tube_bend else {
            return;
        };
        self.error = state.notice.is_some();
        self.status = state.notice.clone().unwrap_or_else(|| {
            if state.can_apply() {
                "Adjust the bend, then Apply"
            } else if state.model.can_rotate() {
                "Drag the round handle to rotate"
            } else {
                "Change the bend radius"
            }
            .into()
        });
    }

    pub(in crate::app) fn tube_bend_gate(&mut self, message: &Message) -> Option<Task<Message>> {
        self.tab.tube_bend.as_ref()?;
        if !self.tube_bend_active() {
            self.tab.tube_bend = None;
            return None;
        }
        if matches!(message, Message::Escape) {
            let token = self.current_tube_bend()?.token;
            return Some(self.tube_bend_action(Action::Cancel(token)));
        }
        let allowed = matches!(
            message,
            Message::Equipment(super::Action::Bend(_))
                | Message::Canvas(
                    Edit::TubeBendDrag { .. }
                        | Edit::Pan(..)
                        | Edit::Zoom(..)
                        | Edit::Navigate(..)
                        | Edit::Hover(_)
                        | Edit::PointerHover(_)
                )
                | Message::Viewport(_)
                | Message::InspectorScroll(_)
                | Message::Fit
                | Message::Zoom(_)
                | Message::Tick
                | Message::ModifiersChanged(_)
        ) || results::admitted(results::Modal::Preview, message);
        if allowed {
            None
        } else {
            self.status = "Apply or Cancel the tube bend to continue".into();
            Some(Task::none())
        }
    }

    pub(in crate::app) fn tube_bend_panel(&self) -> Element<'_, Message> {
        let Some(state) = self.current_tube_bend() else {
            return column![].into();
        };
        let token = state.token;
        let apply = message(Action::Apply(token));
        let can_rotate = state.model.can_rotate();
        let can_edit = state.model.blocked_edit().is_none();
        let angle = reshiki::accessibility::text_input(
            "tube-bend-angle",
            "Bend angle (degrees)",
            "0",
            &state.angle,
        )
        .size(12)
        .padding([5, 7])
        .width(92)
        .on_input_maybe(can_rotate.then_some(move |value| message(Action::Angle(token, value))))
        .on_submit_maybe(state.can_apply().then_some(apply.clone()));
        let radius = reshiki::accessibility::text_input(
            "tube-bend-radius",
            "Bend centerline radius (mm)",
            "Radius",
            &state.radius,
        )
        .size(12)
        .padding([5, 7])
        .width(92)
        .on_input_maybe(can_edit.then_some(move |value| message(Action::Radius(token, value))))
        .on_submit_maybe(state.can_apply().then_some(apply));
        let mut body = column![
            workspace::section(if state.model.is_existing_bend() {
                "Edit tube bend"
            } else {
                "Bend point"
            }),
            row![
                text("Angle").size(12).width(Length::Fill),
                angle,
                text("°").size(12)
            ]
            .spacing(6)
            .align_y(Alignment::Center),
            row![
                text("Radius").size(12).width(Length::Fill),
                radius,
                text("mm").size(12)
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        ]
        .spacing(10);
        if can_rotate {
            body = body.push(
                text("Positive angles turn clockwise.")
                    .size(11)
                    .style(workspace::muted_text),
            );
        } else if can_edit {
            body = body.push(
                text("Both ends are seated. You can still change the radius.")
                    .size(11)
                    .style(workspace::muted_text),
            );
        }
        if state.notice.is_none()
            && let Some(length) = state
                .candidate
                .as_ref()
                .and_then(|candidate| candidate.cut_length)
        {
            body = body.push(text(format!("Cut length {length:.3} mm")).size(12));
        }
        if let Some(notice) = &state.notice {
            body = body.push(
                text(notice)
                    .size(12)
                    .color(iced::Color::from_rgb8(182, 66, 61)),
            );
        }
        body.into()
    }

    pub(in crate::app) fn tube_bend_footer(&self) -> Element<'_, Message> {
        let Some(state) = self.current_tube_bend() else {
            return row![].into();
        };
        row![
            workspace::accessible_command("tube-bend-cancel", "Cancel tube bend", "Cancel")
                .on_press(message(Action::Cancel(state.token))),
            workspace::accessible_command("tube-bend-apply", "Apply tube bend", "Apply")
                .style(crate::appearance::primary)
                .on_press_maybe(
                    state
                        .can_apply()
                        .then_some(message(Action::Apply(state.token)))
                ),
        ]
        .spacing(8)
        .into()
    }
}

pub(in crate::app) fn modal_key(key: &keyboard::Key, _: keyboard::Modifiers) -> Option<Message> {
    matches!(key, keyboard::Key::Named(keyboard::key::Named::Escape)).then_some(Message::Escape)
}

pub(in crate::app) fn leftover_key(key: &keyboard::Key, _: keyboard::Modifiers) -> Option<Message> {
    matches!(key, keyboard::Key::Named(keyboard::key::Named::Enter))
        .then(|| message(Action::ApplyCurrent))
}

#[cfg(test)]
mod tests;
