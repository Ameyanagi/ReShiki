//! Direct manipulation for rigid tube bends. A drag previews an immutable
//! candidate and records one document edit when the pointer is released.
use super::{App, Message};
use crate::app::{InspectorTab, document_tab::DocumentTab, document_tab::TabId, workspace};
use crate::canvas::{Edit, Tool, TubeBend};
use iced::widget::{column, row, text};
use iced::{Alignment, Element, Length, Task};
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Field {
    Angle,
    Radius,
}

#[derive(Debug, Clone)]
pub enum Action {
    Begin(Target),
    Input(Token, Field, String),
    Drag(Token, Point),
    Finish(Token, Option<Point>),
    Abort(Token),
    FinishInput {
        token: Token,
        field: Field,
        unfocused: bool,
    },
}

struct Candidate {
    doc: Document,
    reconciled: reshiki::transaction::Reconciled,
    cut_length: Option<f64>,
}

pub(in crate::app) struct State {
    target: Target,
    token: Token,
    input_token: Token,
    pending_input: Option<Field>,
    base: Document,
    model: Draft,
    angle: String,
    radius: String,
    resting_radius: f64,
    cut_length: Option<f64>,
    dragging: bool,
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

    fn new(target: Target, token: Token, base: Document, model: Draft) -> Self {
        let resting_radius = model.default_radius_mm();
        let angle = angle_text(model.default_angle_deg());
        let cut_length = base
            .graphics
            .iter()
            .find(|graphic| graphic.id == target.piece)
            .and_then(|graphic| {
                reshiki::equipment::parts_list::graphic_cut_length_mm(graphic)
                    .ok()
                    .flatten()
            });
        Self {
            target,
            token,
            input_token: token,
            pending_input: None,
            base,
            model,
            angle,
            radius: resting_radius.to_string(),
            resting_radius,
            cut_length,
            dragging: false,
            candidate: None,
            notice: None,
        }
    }

    fn restore(&mut self) {
        self.angle = angle_text(self.model.default_angle_deg());
        self.radius = self.resting_radius.to_string();
        self.dragging = false;
        self.pending_input = None;
        self.candidate = None;
        self.notice = None;
    }

    fn drag(&mut self, at: Point) {
        if self.model.can_rotate()
            && let Some(angle) = self.model.angle_to(at)
        {
            self.dragging = true;
            let original = self.model.default_angle_deg();
            let angle = if (angle - original).abs() < 0.05 {
                angle_text(original)
            } else {
                ((angle * 10.).round() / 10.).to_string()
            };
            if self.angle != angle {
                self.angle = angle;
                self.refresh();
            }
        }
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

fn angle_text(angle: f64) -> String {
    // Keep enough precision for radius-only edits between seated fittings.
    ((angle * 1e8).round() / 1e8).to_string()
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
            if !target.current(&self.tab) {
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
            let mut state = State::new(target, self.next_bend_token(), self.tab.doc.clone(), model);
            state.refresh();
            let old_width = self.inspector_width();
            let old_viewport = self.viewport;
            let was_open = self.inspector_open;
            self.tab.tube_bend = Some(state);
            self.tab.selected = vec![target.piece];
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
        let current = match &action {
            Action::Input(token, ..) | Action::FinishInput { token, .. } => {
                state.input_token == *token
            }
            Action::Drag(token, _) | Action::Finish(token, _) | Action::Abort(token) => {
                state.token == *token
            }
            Action::Begin(_) => false,
        };
        if !current {
            return Task::none();
        }
        if let Action::FinishInput {
            field, unfocused, ..
        } = action
        {
            self.finish_bend_input(field, unfocused);
            return Task::none();
        }

        if let Some(field) = state.pending_input
            && (matches!(&action, Action::Input(_, next, _) if *next != field)
                || matches!(&action, Action::Drag(..) | Action::Finish(..)))
        {
            // Keyboard and accessibility focus changes can deliver the new
            // field's input before the old field's unfocus message.
            self.finish_bend_input(field, true);
        }
        let Some(state) = self.tab.tube_bend.as_mut() else {
            return Task::none();
        };
        match action {
            Action::Input(_, field, value) if !state.dragging => {
                match field {
                    Field::Angle if state.model.can_rotate() => state.angle = value,
                    Field::Radius if state.model.blocked_edit().is_none() => state.radius = value,
                    _ => return Task::none(),
                }
                state.pending_input = Some(field);
                state.refresh();
            }
            Action::Drag(_, at) => state.drag(at),
            Action::Finish(_, at) => {
                if let Some(at) = at {
                    state.drag(at);
                }
                if state.dragging && state.notice.is_none() && state.candidate.is_some() {
                    self.commit_bend();
                    return Task::none();
                }
                let notice = state.notice.take();
                state.restore();
                state.notice = notice;
                self.retire_bend_capture();
            }
            Action::Abort(_) => {
                if state.dragging {
                    state.restore();
                }
                self.retire_bend_capture();
            }
            _ => return Task::none(),
        }
        self.tube_bend_status();
        Task::none()
    }

    fn finish_bend_input(&mut self, field: Field, unfocused: bool) {
        let Some(state) = self.tab.tube_bend.as_mut() else {
            return;
        };
        if state.dragging || state.pending_input != Some(field) {
            return;
        }
        if state.notice.is_none() {
            if state.candidate.is_some() {
                self.commit_bend();
                return;
            }
            // At a new point, changing only the radius prepares the next
            // rotation without creating a document edit.
            if let Ok(radius) = state.radius.trim().parse::<f64>() {
                state.resting_radius = radius;
            }
            state.pending_input = None;
        } else if unfocused {
            let notice = state.notice.take();
            state.restore();
            state.notice = notice;
        }
        self.tube_bend_status();
    }

    fn retire_bend_capture(&mut self) {
        let token = self.next_bend_token();
        if let Some(state) = &mut self.tab.tube_bend {
            state.token = token;
        }
    }

    fn next_bend_token(&mut self) -> Token {
        self.tab.tube_bend_serial = self.tab.tube_bend_serial.wrapping_add(1);
        (self.tab.file_epoch, self.tab.tube_bend_serial)
    }

    fn commit_bend(&mut self) {
        let Some(mut state) = self.tab.tube_bend.take() else {
            return;
        };
        let Some(candidate) = state
            .candidate
            .take()
            .filter(|_| state.base == self.tab.doc)
        else {
            return;
        };
        self.prepare_document_change(true);
        self.tab.erase_stroke = false;
        self.tab.doc = candidate.doc;
        self.commit_changed(candidate.reconciled, false);
        self.tab.selected = vec![state.target.piece];
        if !self.error {
            self.status = if state.model.is_existing_bend() {
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
        if let Ok(model) = state.model.after_commit(&self.tab.doc) {
            let target = Target {
                revision: self.tab.revision,
                ..state.target
            };
            let mut next = State::new(target, self.next_bend_token(), self.tab.doc.clone(), model);
            // A click on the visible grip can blur a numeric field in the
            // same event. It already grabbed the on-screen candidate, which
            // is now committed, so keep that pointer capture valid.
            if !state.dragging {
                next.token = state.token;
                // Any queued input for the newly focused field belongs to
                // this same editor. Completions are scoped to the dirty field.
                next.input_token = state.input_token;
            }
            self.tab.tube_bend = Some(next);
        }
    }

    fn tube_bend_status(&mut self) {
        let Some(state) = &self.tab.tube_bend else {
            return;
        };
        self.error = state.notice.is_some();
        self.status = state.notice.clone().unwrap_or_else(|| {
            if state.dragging {
                "Release to set the bend"
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
        } else if matches!(message, Message::Escape) {
            self.tab.tube_bend = None;
            self.status = "Select / move".into();
            self.error = false;
            return Some(crate::app::focus_drawing());
        } else if matches!(message, Message::Undo | Message::Redo) {
            self.tab.tube_bend = None;
        } else if matches!(
            message,
            Message::Save
                | Message::SaveAs
                | Message::New
                | Message::Open
                | Message::Tabs(
                    super::super::tabs::Action::Select(_)
                        | super::super::tabs::Action::Cycle(_)
                        | super::super::tabs::Action::Number(_)
                        | super::super::tabs::Action::Close(_)
                )
                | Message::Tool(_)
                | Message::Inspector(_)
                | Message::ToggleInspector
                | Message::SelectAll
                | Message::InvertSelection
        ) || matches!(message, Message::Canvas(edit) if !matches!(
            edit,
            Edit::TubeBendDrag { .. }
                | Edit::TubeBendFinish { .. }
                | Edit::TubeBendAbort { .. }
                | Edit::Pan(..)
                | Edit::Zoom(..)
                | Edit::Navigate(..)
                | Edit::Hover(_)
                | Edit::PointerHover(_)
        )) {
            if let Some(field) = self
                .current_tube_bend()
                .and_then(|state| state.pending_input)
            {
                self.finish_bend_input(field, true);
            }
            self.tab.tube_bend = None;
        }
        None
    }

    pub(in crate::app) fn tube_bend_panel(&self) -> Element<'_, Message> {
        let Some(state) = self.current_tube_bend() else {
            return column![].into();
        };
        let token = state.input_token;
        let finish = |field, unfocused| {
            message(Action::FinishInput {
                token,
                field,
                unfocused,
            })
        };
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
        .on_input_maybe(
            can_rotate.then_some(move |value| message(Action::Input(token, Field::Angle, value))),
        )
        .on_submit_maybe(can_rotate.then(|| finish(Field::Angle, false)))
        .on_unfocus(finish(Field::Angle, true));
        let radius = reshiki::accessibility::text_input(
            "tube-bend-radius",
            "Bend centerline radius (mm)",
            "Radius",
            &state.radius,
        )
        .size(12)
        .padding([5, 7])
        .width(92)
        .on_input_maybe(
            can_edit.then_some(move |value| message(Action::Input(token, Field::Radius, value))),
        )
        .on_submit_maybe(can_edit.then(|| finish(Field::Radius, false)))
        .on_unfocus(finish(Field::Radius, true));
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
                .or(state.cut_length)
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
}

#[cfg(test)]
mod tests;
