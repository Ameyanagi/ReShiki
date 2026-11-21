//! A detached equipment draft, measured and reviewed on the ordinary canvas.
mod trial;
use super::workspace::{accessible_command, command, muted_text, section};
use super::{App, InspectorTab, Message, focus_drawing};
use crate::canvas::{Edit, EquipmentAuthoring, Tool};
use iced::widget::{column, row, text};
use iced::{Element, Task, keyboard};
use reshiki::document::{Document, Point};
use reshiki::equipment::authoring::{Draft, JointKind};
use reshiki::equipment::format::{Gender, JointDraw, Role, TAPER_SIZES};
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
    BeginJoint,
    JointEnd(JointEnd),
    JointSize(String),
    FlipJoint,
    MarkJointAgain,
    AddJoint,
    CancelJoint,
    TestSnapping,
    TestPiece(trial::Choice),
    TestTurn(f32),
    TestStart(u64),
    TestAt {
        at: Point,
        epoch: u64,
        reach: f32,
        finish: bool,
    },
    FinishTesting,
    Accept,
    Discard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JointEnd {
    Socket,
    Cone,
}

impl JointEnd {
    fn gender(self) -> Gender {
        match self {
            Self::Socket => Gender::F,
            Self::Cone => Gender::M,
        }
    }
}

impl std::fmt::Display for JointEnd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Socket => "Socket",
            Self::Cone => "Cone",
        })
    }
}

#[derive(Clone, PartialEq)]
struct JointEdit {
    points: Vec<Point>,
    end: JointEnd,
    size: String,
    flip: bool,
}

#[derive(Clone, PartialEq)]
struct Snapshot {
    draft: Arc<Draft>,
    points: Vec<Point>,
    length: String,
    measured_length: Option<String>,
    name: String,
    measuring: bool,
    joint: Option<JointEdit>,
}

pub(super) struct State {
    current: Snapshot,
    past: Vec<Snapshot>,
    future: Vec<Snapshot>,
    identity: String,
    pub(super) preview: Option<Document>,
    base_preview: Option<Document>,
    preview_piece: Option<u64>,
    trial: Option<trial::Trial>,
    notice: Option<String>,
    joint_candidate: Option<Arc<Draft>>,
    direction: Option<[Point; 2]>,
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
        self.base_preview = None;
        self.preview_piece = None;
        if let Some(trial) = &mut self.trial {
            trial.invalidate();
        }
        self.joint_candidate = None;
        self.direction = None;
        let mut draft = self.named_draft();
        if let Some(edit) = &self.current.joint
            && let [a, b] = edit.points.as_slice()
        {
            let edges = [*a, *b];
            let Some(id) = (1..=draft.definition.joints.len() + 1)
                .map(|n| format!("joint-{n}"))
                .find(|id| !draft.definition.joints.iter().any(|j| j.id() == id))
            else {
                self.notice = Some("The draft cannot add another joint.".into());
                return;
            };
            let kind = JointKind::Taper {
                size: edit.size.clone(),
                gender: edit.end.gender(),
                draw: JointDraw::Standard,
            };
            let glass = draft
                .definition
                .shapes
                .iter()
                .any(|s| s.role == Role::Glass);
            let result = if glass {
                draft.add_joint(&id, edges, kind.clone(), edit.flip)
            } else {
                Err("A standard joint needs a filled glass outline. Fill the neck shape before defining it as equipment.".into())
            };
            match result {
                Ok(joint) => {
                    self.direction = Some(direction_arrow(&draft, joint.at(), joint.dir()));
                    self.joint_candidate = Some(Arc::new(draft.clone()));
                }
                Err(error) => {
                    self.notice = Some(error);
                    if draft.calibrated() {
                        if let Ok(rim) = draft.rim(edges) {
                            let mut dir = rim.dir;
                            if edit.flip {
                                dir = dir.map(|v| -v);
                            }
                            self.direction = Some(direction_arrow(&draft, rim.at, dir));
                        }
                    } else if glass {
                        // The first joint sets the physical scale used by the
                        // normal probes. An uncalibrated rim can choose the
                        // opposite normal around a notch. A valid opposite
                        // trial supplies that scale without changing the draft.
                        let mut opposite = draft.clone();
                        if let Ok(joint) = opposite.add_joint(&id, edges, kind, !edit.flip) {
                            self.direction = Some(direction_arrow(
                                &opposite,
                                joint.at(),
                                joint.dir().map(|v| -v),
                            ));
                        }
                    }
                }
            }
        }
        if !draft.calibrated() {
            return;
        }
        match draft.accept(doc, &self.identity) {
            Ok(accepted) => {
                self.preview_piece = Some(accepted.piece_id);
                self.base_preview = Some(accepted.document.clone());
                self.preview = Some(accepted.document);
                self.refresh_trial();
            }
            Err(error) => self.notice = Some(error),
        }
    }

    fn refresh_trial(&mut self) {
        if let (Some(trial), Some(base)) = (&mut self.trial, &self.base_preview) {
            self.preview = Some(trial.render(base));
        }
    }

    fn keep_trial_pose(&mut self) {
        if let (Some(trial), Some(shown)) = (&mut self.trial, &self.preview) {
            trial.keep_pose(shown);
        }
    }

    fn ready(&self) -> bool {
        !self.current.measuring
            && self.current.joint.is_none()
            && !self.current.name.trim().is_empty()
            && self.preview.is_some()
    }

    pub(super) fn measurement(&self) -> EquipmentAuthoring<'_> {
        EquipmentAuthoring {
            points: if self.trial.is_some() {
                &[]
            } else {
                self.current
                    .joint
                    .as_ref()
                    .map_or(&self.current.points, |j| &j.points)
            },
            measuring: self.trial.is_none()
                && (self.current.measuring || self.current.joint.is_some()),
            direction: self.direction.filter(|_| self.trial.is_none()),
            trial: self.trial.as_ref().and_then(|t| {
                let id = t.id?;
                let g = self
                    .preview
                    .as_ref()?
                    .graphics
                    .iter()
                    .find(|g| g.id == id)?;
                Some((id, reshiki::equipment::placed::middle(g), t.epoch))
            }),
            snap: self.trial.as_ref().and_then(|t| t.snap.as_ref()),
        }
    }
}

fn point([x, y]: [f64; 2]) -> Point {
    Point::new(x as f32, y as f32)
}

fn direction_arrow(draft: &Draft, at: [f64; 2], dir: [f64; 2]) -> [Point; 2] {
    let at = draft.frame().apply(at);
    let dir = draft.frame().apply_dir(dir);
    [point(at), point([at[0] + dir[0], at[1] + dir[1]])]
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
                    measured_length: None,
                    name: "New equipment piece".into(),
                    measuring: true,
                    joint: None,
                },
                past: vec![],
                future: vec![],
                identity,
                preview: None,
                base_preview: None,
                preview_piece: None,
                trial: None,
                notice: None,
                joint_candidate: None,
                direction: None,
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
                        state.base_preview = None;
                        state.preview_piece = None;
                        if let Some(trial) = &mut state.trial {
                            trial.invalidate();
                        }
                    }
                }
            }
            return focus_drawing();
        }
        let Some(state) = &mut self.tab.equipment_authoring else {
            return Task::none();
        };
        match &action {
            Action::TestSnapping => {
                if let Some(base) = &state.base_preview
                    && let Some(piece) = base
                        .graphics
                        .iter()
                        .find(|g| Some(g.id) == state.preview_piece)
                {
                    let choices = trial::choices(base, &state.identity);
                    let ports = reshiki::equipment::solve::ports(piece);
                    if ports.is_empty() {
                        return focus_drawing();
                    }
                    let choice = choices
                        .iter()
                        .filter(|c| {
                            let g = reshiki::equipment::placed::graphic(
                                0,
                                c.definition.clone(),
                                Point::default(),
                                1.,
                            );
                            reshiki::equipment::solve::ports(&g).iter().any(|p| {
                                ports
                                    .iter()
                                    .any(|q| p.gender != q.gender && p.size() == q.size())
                            })
                        })
                        .min_by_key(|c| c.definition.joints.len())
                        .or(choices.first())
                        .cloned();
                    if let Some(choice) = choice {
                        let reach = reshiki::equipment::solve::REACH_PX / self.tab.camera.zoom;
                        let scale = reshiki::equipment::placed::world_per_mm(
                            base,
                            reshiki::equipment::placed::DEFAULT_PT_PER_MM,
                        );
                        state.trial = Some(trial::Trial::new(
                            choice,
                            choices,
                            self.tab
                                .camera
                                .center
                                .offset(60. / self.tab.camera.zoom, 0.),
                            scale,
                            reach,
                        ));
                        state.refresh_trial();
                    }
                }
                return focus_drawing();
            }
            Action::FinishTesting => {
                state.trial = None;
                state.preview = state.base_preview.clone();
                return focus_drawing();
            }
            Action::TestStart(epoch) => {
                if state.trial.as_ref().is_some_and(|t| t.epoch == *epoch) {
                    state.keep_trial_pose();
                }
                return Task::none();
            }
            Action::TestAt {
                at,
                epoch,
                reach,
                finish,
            } => {
                if let Some(trial) = &mut state.trial
                    && trial.epoch == *epoch
                    && at.x.is_finite()
                    && at.y.is_finite()
                    && reach.is_finite()
                    && *reach > 0.
                {
                    trial.at = *at;
                    trial.reach = *reach;
                    state.refresh_trial();
                    if *finish {
                        state.keep_trial_pose();
                    }
                }
                return Task::none();
            }
            Action::TestPiece(choice) => {
                if let Some(trial) = &mut state.trial
                    && trial.choices.contains(choice)
                {
                    trial.choice = choice.clone();
                    trial.invalidate();
                    state.refresh_trial();
                }
                return Task::none();
            }
            Action::TestTurn(turn) => {
                if turn.is_finite()
                    && (-180. ..=180.).contains(turn)
                    && let Some(trial) = &mut state.trial
                {
                    trial.turn = *turn;
                    trial.invalidate();
                    state.refresh_trial();
                }
                return Task::none();
            }
            _ => {}
        }
        state.notice = None;
        let focus = matches!(
            action,
            Action::Calibrate
                | Action::MeasureAgain
                | Action::BeginJoint
                | Action::MarkJointAgain
                | Action::AddJoint
                | Action::CancelJoint
        );
        match action {
            Action::Point(point)
                if state
                    .current
                    .joint
                    .as_ref()
                    .is_some_and(|j| j.points.len() < 2) =>
            {
                state.checkpoint();
                if let Some(joint) = &mut state.current.joint {
                    joint.points.push(point);
                }
            }
            Action::Point(point)
                if state.current.joint.is_none()
                    && state.current.measuring
                    && state.current.points.len() < 2 =>
            {
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
            Action::MeasureAgain
                if state.current.joint.is_none()
                    && state.current.draft.definition.joints.is_empty() =>
            {
                state.checkpoint();
                state.current.points.clear();
                state.current.measuring = true;
            }
            Action::BeginJoint if state.current.joint.is_none() => {
                state.checkpoint();
                state.current.joint = Some(JointEdit {
                    points: vec![],
                    end: JointEnd::Socket,
                    size: "15/25".into(),
                    flip: false,
                });
            }
            Action::JointEnd(end) if state.current.joint.as_ref().is_some_and(|j| j.end != end) => {
                state.checkpoint();
                if let Some(joint) = &mut state.current.joint {
                    joint.end = end;
                }
            }
            Action::JointSize(size)
                if state.current.joint.as_ref().is_some_and(|j| j.size != size) =>
            {
                state.checkpoint();
                if let Some(joint) = &mut state.current.joint {
                    joint.size = size;
                }
            }
            Action::FlipJoint if state.current.joint.is_some() => {
                state.checkpoint();
                if let Some(joint) = &mut state.current.joint {
                    joint.flip = !joint.flip;
                }
            }
            Action::MarkJointAgain if state.current.joint.is_some() => {
                state.checkpoint();
                if let Some(joint) = &mut state.current.joint {
                    joint.points.clear();
                }
            }
            Action::CancelJoint if state.current.joint.is_some() => {
                state.checkpoint();
                state.current.joint = None;
            }
            Action::AddJoint if state.joint_candidate.is_some() && state.preview.is_some() => {
                if let Some(draft) = state.joint_candidate.take() {
                    state.checkpoint();
                    if state.current.measuring {
                        state.current.points.clear();
                        state.current.length.clear();
                    }
                    state.current.draft = draft;
                    state.current.joint = None;
                    state.current.measuring = false;
                }
            }
            Action::Calibrate if state.current.joint.is_none() => {
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
                        state.current.measured_length = Some(state.current.length.trim().into());
                        state.current.measuring = false;
                    }
                    Err(error) => state.notice = Some(error),
                }
            }
            _ => {}
        }
        state.refresh(&self.tab.doc);
        self.error = false;
        self.status = if state.current.joint.is_some() {
            "Mark both rim edges, review the joint, then Add joint"
        } else if state.ready() {
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
        let current = &self.tab.equipment_authoring.as_ref()?.current;
        let confirm = if current.joint.is_some() {
            Action::AddJoint
        } else if current.measuring {
            Action::Calibrate
        } else {
            Action::Accept
        };
        match message {
            Message::Escape => return Some(self.equipment_authoring_action(Action::Discard)),
            Message::Undo => return Some(self.equipment_draft_history(false)),
            Message::Redo => return Some(self.equipment_draft_history(true)),
            Message::ContextKey(key) if key == "Enter" => {
                return Some(self.equipment_authoring_action(confirm));
            }
            Message::Canvas(Edit::EquipmentAuthoringPoint(point)) => {
                return Some(self.equipment_authoring_action(Action::Point(*point)));
            }
            Message::Canvas(Edit::EquipmentAuthoringTestStart(epoch)) => {
                return Some(self.equipment_authoring_action(Action::TestStart(*epoch)));
            }
            Message::Canvas(Edit::EquipmentAuthoringTest {
                at,
                epoch,
                reach,
                finish,
            }) => {
                return Some(self.equipment_authoring_action(Action::TestAt {
                    at: *at,
                    epoch: *epoch,
                    reach: *reach,
                    finish: *finish,
                }));
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
            || matches!(message, Message::Assistant(_) | Message::Updates(_))
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
        if let Some(trial) = &state.trial {
            body = body
                .push(section("Test snapping"))
                .push(crate::appearance::pick_list(
                    trial.choices.as_slice(), Some(trial.choice.clone()),
                    |choice| Message::EquipmentAuthoring(Action::TestPiece(choice)),
                ).text_size(12))
                .push(text(format!("Turn test piece: {:.0}°", trial.turn)).size(12))
                .push(iced::widget::slider(-180. ..=180., trial.turn,
                    |turn| Message::EquipmentAuthoring(Action::TestTurn(turn))).step(15.))
                .push(text("Drag the gray test piece toward a joint. Test pieces are discarded; Accept keeps your piece at its original position.").size(12))
                .push(command("Finish testing", Message::EquipmentAuthoring(Action::FinishTesting)));
        }
        if let Some(joint) = &current.joint {
            body = body
                .push(text("Ground-glass joint").size(13))
                .push(
                    text(match joint.points.len() {
                        0 => "Click one edge of the opening, then the opposite rim edge.",
                        1 => "Click the opposite rim edge.",
                        _ => "Review the standard joint and its outward arrow on the canvas.",
                    })
                    .size(12),
                )
                .push(
                    row![
                        crate::appearance::pick_list(
                            [JointEnd::Socket, JointEnd::Cone],
                            Some(joint.end),
                            |end| Message::EquipmentAuthoring(Action::JointEnd(end))
                        )
                        .text_size(12),
                        crate::appearance::pick_list(
                            TAPER_SIZES.map(|s| s.label.to_string()).to_vec(),
                            Some(joint.size.clone()),
                            |size| Message::EquipmentAuthoring(Action::JointSize(size))
                        )
                        .text_size(12),
                    ]
                    .spacing(7),
                );
            if !current.draft.calibrated() {
                body = body.push(
                    text("This size sets the scale of the whole piece when you add the joint.")
                        .size(12),
                );
            } else if let [a, b] = joint.points.as_slice()
                && let Ok(sizes) = current.draft.taper_sizes([*a, *b], joint.end.gender())
                && !sizes.is_empty()
            {
                body = body.push(
                    text(format!(
                        "Fits this opening: {}",
                        sizes.iter().map(|s| s.label).collect::<Vec<_>>().join(", ")
                    ))
                    .size(12),
                );
            }
            if !joint.points.is_empty() {
                let mut controls = row![command(
                    "Mark again",
                    Message::EquipmentAuthoring(Action::MarkJointAgain)
                )]
                .spacing(6);
                if joint.points.len() == 2 {
                    controls = controls.push(command(
                        "Flip",
                        Message::EquipmentAuthoring(Action::FlipJoint),
                    ));
                }
                body = body.push(controls);
            }
            body = body
                .push(
                    text(
                        "ReShiki draws the standard joint. Its base must meet a filled glass neck.",
                    )
                    .size(12),
                )
                .push(
                    row![
                        command(
                            "Cancel joint",
                            Message::EquipmentAuthoring(Action::CancelJoint)
                        ),
                        accessible_command(
                            "equipment-add-joint",
                            "Add joint to equipment draft",
                            "Add joint"
                        )
                        .on_press_maybe(
                            (state.joint_candidate.is_some() && state.preview.is_some())
                                .then_some(Message::EquipmentAuthoring(Action::AddJoint))
                        ),
                    ]
                    .spacing(6),
                );
        } else if current.measuring {
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
            body = body.push(
                text(current.measured_length.as_ref().map_or_else(
                    || "Scale set from the first joint".into(),
                    |length| format!("Measured length: {length} mm"),
                ))
                .size(12),
            );
            if current.draft.definition.joints.is_empty() {
                body = body.push(command(
                    "Measure again",
                    Message::EquipmentAuthoring(Action::MeasureAgain),
                ));
            } else {
                body = body.push(
                    text(format!(
                        "Fixed joints: {}",
                        current.draft.definition.joints.len()
                    ))
                    .size(12),
                );
            }
            body = body.push(text("The canvas shows the equipment piece. Accept replaces the selected shapes in this drawing.").size(12));
        }
        if current.joint.is_none() {
            body = body.push(command(
                "Add joint…",
                Message::EquipmentAuthoring(Action::BeginJoint),
            ));
        }
        if state.trial.is_none()
            && state.base_preview.as_ref().is_some_and(|doc| {
                doc.graphics
                    .iter()
                    .find(|g| Some(g.id) == state.preview_piece)
                    .is_some_and(|g| !reshiki::equipment::solve::ports(g).is_empty())
            })
        {
            body = body.push(command(
                "Test snapping…",
                Message::EquipmentAuthoring(Action::TestSnapping),
            ));
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
