//! A detached equipment draft, measured and reviewed on the ordinary canvas.
use super::workspace::{accessible_command, command, muted_text, section};
use super::{App, InspectorTab, Message, focus_drawing};
use crate::canvas::{Edit, Tool};
use iced::widget::{column, row, text};
use iced::{Element, Task, keyboard};
use reshiki::document::{Document, Point};
use reshiki::equipment::authoring::Draft;
use std::sync::Arc;

pub(super) const DEFINE: &str = "Define as equipment piece…";

#[derive(Debug, Clone)]
pub enum Action {
    Begin,
    Point(Point),
    Length(String),
    Name(String),
    Calibrate,
    MeasureAgain,
    Accept,
    Discard,
}

#[derive(Clone, PartialEq)]
struct Snapshot {
    draft: Arc<Draft>,
    points: Vec<Point>,
    length: String,
    name: String,
    measuring: bool,
}

pub(super) struct State {
    current: Snapshot,
    past: Vec<Snapshot>,
    future: Vec<Snapshot>,
    identity: String,
    pub(super) preview: Option<Document>,
    notice: Option<String>,
}

impl State {
    fn checkpoint(&mut self) {
        if self.past.len() == 100 {
            self.past.remove(0);
        }
        self.past.push(self.current.clone());
        self.future.clear();
    }

    fn named_draft(&self) -> Draft {
        let mut draft = (*self.current.draft).clone();
        draft
            .definition
            .names
            .insert("en".into(), self.current.name.trim().into());
        draft
    }

    fn refresh(&mut self, doc: &Document) {
        self.preview = None;
        if !self.current.draft.calibrated() {
            return;
        }
        match self.named_draft().accept(doc, &self.identity) {
            Ok(accepted) => self.preview = Some(accepted.document),
            Err(error) => self.notice = Some(error),
        }
    }

    fn ready(&self) -> bool {
        !self.current.measuring && !self.current.name.trim().is_empty() && self.preview.is_some()
    }

    pub(super) fn measurement(&self) -> (&[Point], bool) {
        (&self.current.points, self.current.measuring)
    }
}

/// Like user templates, a new definition has an identity independent of its
/// object number in this drawing, so different drawings can share a library.
fn fresh_identity() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let time = iced::time::SystemTime::now()
        .duration_since(iced::time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    #[cfg(not(target_arch = "wasm32"))]
    let process = std::process::id();
    #[cfg(target_arch = "wasm32")]
    let process = 0;
    format!(
        "user/piece-{time:x}-{process:x}-{:x}",
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}

impl App {
    pub(super) fn can_define_equipment(&self) -> bool {
        !self.tab.busy
            && self.updates.idle()
            && !self.tab.drawing_edit_active()
            && self.tab.cycle_inputs.proposal.is_none()
            && Draft::from_selection(&self.tab.doc, &self.tab.selected).is_ok()
    }

    pub(super) fn equipment_authoring_action(&mut self, action: Action) -> Task<Message> {
        if matches!(action, Action::Begin) {
            if self.tab.busy
                || !self.updates.idle()
                || self.tab.drawing_edit_active()
                || self.tab.cycle_inputs.proposal.is_some()
            {
                self.status = "Finish the current edit before defining equipment".into();
                self.error = true;
                return Task::none();
            }
            let draft = match Draft::from_selection(&self.tab.doc, &self.tab.selected) {
                Ok(draft) => draft,
                Err(error) => {
                    self.status = error;
                    self.error = true;
                    return Task::none();
                }
            };
            let identity = loop {
                let candidate = fresh_identity();
                if !self
                    .tab
                    .doc
                    .piece_definitions
                    .iter()
                    .any(|d| d.id == candidate)
                {
                    break candidate;
                }
            };
            self.tab.equipment_authoring = Some(State {
                current: Snapshot {
                    draft: Arc::new(draft),
                    points: vec![],
                    length: String::new(),
                    name: "New equipment piece".into(),
                    measuring: true,
                },
                past: vec![],
                future: vec![],
                identity,
                preview: None,
                notice: None,
            });
            self.context_menu = None;
            self.commands.open = false;
            self.tab.connect = None;
            self.tab.arrows.cancel_source();
            self.tool = Tool::Select;
            self.inspector_open = true;
            self.inspector_tab = InspectorTab::Properties;
            self.error = false;
            self.status = "Click two points whose distance you know".into();
            return focus_drawing();
        }
        if matches!(action, Action::Discard) {
            if self.tab.equipment_authoring.take().is_some() {
                self.error = false;
                self.status = "Equipment draft discarded".into();
            }
            return focus_drawing();
        }
        if matches!(action, Action::Accept) {
            let Some(state) = self.tab.equipment_authoring.as_ref().filter(|s| s.ready()) else {
                return Task::none();
            };
            let accepted = state.named_draft().accept(&self.tab.doc, &state.identity);
            let result = accepted.and_then(|accepted| {
                self.try_replace_document(accepted.document, vec![accepted.piece_id])
            });
            match result {
                Ok(()) => {
                    self.tab.equipment_authoring = None;
                    self.set_selection(self.tab.selected.clone());
                    self.error = false;
                    self.status = "Equipment piece defined in this drawing".into();
                }
                Err(error) => {
                    self.error = true;
                    self.status = error.clone();
                    if let Some(state) = &mut self.tab.equipment_authoring {
                        state.notice = Some(error);
                        state.preview = None;
                    }
                }
            }
            return focus_drawing();
        }
        let Some(state) = &mut self.tab.equipment_authoring else {
            return Task::none();
        };
        state.notice = None;
        let focus = matches!(action, Action::Calibrate | Action::MeasureAgain);
        match action {
            Action::Point(point) if state.current.measuring && state.current.points.len() < 2 => {
                state.checkpoint();
                state.current.points.push(point);
            }
            Action::Length(value) if value != state.current.length => {
                state.checkpoint();
                state.current.length = value;
            }
            Action::Name(value) if value != state.current.name => {
                state.checkpoint();
                state.current.name = value.clone();
            }
            Action::MeasureAgain => {
                state.checkpoint();
                state.current.points.clear();
                state.current.measuring = true;
            }
            Action::Calibrate => {
                let result = match (
                    state.current.points.as_slice(),
                    state.current.length.trim().parse::<f64>(),
                ) {
                    ([a, b], Ok(mm)) => {
                        let mut draft = (*state.current.draft).clone();
                        draft.calibrate([*a, *b], mm).map(|()| draft)
                    }
                    _ => Err("Mark two points and enter their distance in millimetres.".into()),
                };
                match result {
                    Ok(draft) => {
                        state.checkpoint();
                        state.current.draft = Arc::new(draft);
                        state.current.measuring = false;
                    }
                    Err(error) => state.notice = Some(error),
                }
            }
            _ => {}
        }
        state.refresh(&self.tab.doc);
        self.error = false;
        self.status = if state.ready() {
            "Review the piece, then Accept or Discard"
        } else {
            "Click two points whose distance you know"
        }
        .into();
        if focus { focus_drawing() } else { Task::none() }
    }

    fn equipment_draft_history(&mut self, redo: bool) -> Task<Message> {
        let Some(state) = &mut self.tab.equipment_authoring else {
            return Task::none();
        };
        let restored = if redo {
            state.future.pop()
        } else {
            state.past.pop()
        };
        if let Some(restored) = restored {
            let old = std::mem::replace(&mut state.current, restored);
            if redo {
                state.past.push(old);
            } else {
                state.future.push(old);
            }
            state.notice = None;
            state.refresh(&self.tab.doc);
        }
        focus_drawing()
    }

    pub(super) fn equipment_authoring_gate(&mut self, message: &Message) -> Option<Task<Message>> {
        let measuring = self.tab.equipment_authoring.as_ref()?.current.measuring;
        match message {
            Message::Escape => return Some(self.equipment_authoring_action(Action::Discard)),
            Message::Undo => return Some(self.equipment_draft_history(false)),
            Message::Redo => return Some(self.equipment_draft_history(true)),
            Message::ContextKey(key) if key == "Enter" => {
                return Some(self.equipment_authoring_action(if measuring {
                    Action::Calibrate
                } else {
                    Action::Accept
                }));
            }
            Message::Canvas(Edit::EquipmentAuthoringPoint(point)) => {
                return Some(self.equipment_authoring_action(Action::Point(*point)));
            }
            _ => {}
        }
        // Keep this mode visible. Navigation and classified completions still work;
        // opening/closing drawings or changing panels waits for Accept or Discard.
        let allowed = matches!(
            message,
            Message::EquipmentAuthoring(_)
                | Message::Canvas(
                    Edit::Pan(..)
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
        ) || super::results::admitted(super::results::Modal::Preview, message)
            || matches!(message, Message::Updates(_))
                && super::results::admitted(super::results::Modal::AtomText, message);
        if allowed {
            None
        } else {
            self.status = "Accept or Discard the equipment draft to continue".into();
            Some(Task::none())
        }
    }

    pub(super) fn equipment_authoring_panel(&self) -> Element<'_, Message> {
        let Some(state) = self.tab.equipment_authoring.as_ref() else {
            return column![].into();
        };
        let current = &state.current;
        let mut body = column![
            section("Define equipment"),
            text("Name").size(12),
            crate::appearance::text_input("Piece name", &current.name)
                .on_input(|value| Message::EquipmentAuthoring(Action::Name(value)))
                .on_submit(Message::ContextKey("Enter".into()))
                .padding(7)
                .size(12),
        ]
        .spacing(10);
        if current.measuring {
            body = body.push(
                text(match current.points.len() {
                    0 => "Click the first point of a known length on the drawing.",
                    1 => "Click the second point.",
                    _ => "Enter the distance between the marked points.",
                })
                .size(12),
            );
            body = body.push(
                row![
                    crate::appearance::text_input("Distance", &current.length)
                        .on_input(|value| Message::EquipmentAuthoring(Action::Length(value)))
                        .on_submit(Message::EquipmentAuthoring(Action::Calibrate))
                        .padding(7)
                        .size(12),
                    text("mm").size(12),
                ]
                .spacing(7)
                .align_y(iced::Alignment::Center),
            );
            body = body.push(
                accessible_command("equipment-calibrate", "Preview equipment", "Preview")
                    .on_press_maybe(
                        (current.points.len() == 2)
                            .then_some(Message::EquipmentAuthoring(Action::Calibrate)),
                    ),
            );
            if !current.points.is_empty() {
                body = body.push(command(
                    "Mark again",
                    Message::EquipmentAuthoring(Action::MeasureAgain),
                ));
            }
        } else {
            body = body.push(text(format!("Measured length: {} mm", current.length.trim())).size(12))
                .push(command("Measure again", Message::EquipmentAuthoring(Action::MeasureAgain)))
                .push(text("The canvas shows the equipment piece. Accept replaces the selected shapes in this drawing.").size(12));
        }
        body = body.push(
            row![
                accessible_command("equipment-draft-undo", "Undo draft edit", "Undo")
                    .on_press_maybe((!state.past.is_empty()).then_some(Message::Undo)),
                accessible_command("equipment-draft-redo", "Redo draft edit", "Redo")
                    .on_press_maybe((!state.future.is_empty()).then_some(Message::Redo)),
            ]
            .spacing(6),
        );
        if let Some(notice) = &state.notice {
            body = body.push(
                text(notice)
                    .size(12)
                    .color(iced::Color::from_rgb8(182, 66, 61)),
            );
        }
        body.push(text("This drawing only").size(11).style(muted_text))
            .into()
    }

    pub(super) fn equipment_authoring_footer(&self) -> Element<'_, Message> {
        let Some(state) = self.tab.equipment_authoring.as_ref() else {
            return row![].into();
        };
        row![
            accessible_command(
                "equipment-draft-discard",
                "Discard equipment draft",
                "Discard"
            )
            .on_press(Message::EquipmentAuthoring(Action::Discard)),
            accessible_command("equipment-draft-accept", "Accept equipment piece", "Accept")
                .style(crate::appearance::primary)
                .on_press_maybe(
                    state
                        .ready()
                        .then_some(Message::EquipmentAuthoring(Action::Accept))
                ),
        ]
        .spacing(8)
        .into()
    }
}

/// Draft history and cancellation precede focused text fields. Typing and
/// Enter still belong to those fields before the canvas receives them.
pub(super) fn modal_key(key: &keyboard::Key, modifiers: keyboard::Modifiers) -> Option<Message> {
    use keyboard::{Key, key::Named};
    if matches!(key, Key::Named(Named::Escape)) {
        return Some(Message::Escape);
    }
    match super::shortcuts::key_message(key, key, modifiers) {
        Some(message @ (Message::Undo | Message::Redo)) => Some(message),
        _ => None,
    }
}

/// Focused controls take Enter first; the canvas confirms only an unused key.
pub(super) fn leftover_key(key: &keyboard::Key, _: keyboard::Modifiers) -> Option<Message> {
    matches!(key, keyboard::Key::Named(keyboard::key::Named::Enter))
        .then(|| Message::ContextKey("Enter".into()))
}

#[cfg(test)]
mod tests;
