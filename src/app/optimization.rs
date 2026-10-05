//! Detached, single-flight 3D optimization sessions.
//!
//! Worker requests own immutable snapshots. Pointer motion replaces one pending
//! target, and Apply is the only operation that writes the committed document.
use super::{App, InspectorTab, Message, document_tab::TabId};
use crate::canvas::{Edit, Tool, optimization::Context};
use iced::widget::{Space, column, row, text, tooltip};
use iced::{Alignment, Element, Length, Task};
use reshiki::document::Document;
use reshiki::geometry::{
    Client, Conformer, ForceField, Optimized, Pin, Point3, Prepared, ViewFrame,
};
use std::collections::BTreeMap;
use std::sync::Arc;

const BATCH_ITERATIONS: u32 = 20;
const MAX_RELAXATION_BATCHES: u32 = 50;
const MAX_STAGNANT_BATCHES: u32 = 8;
const ENERGY_PROGRESS_ABSOLUTE: f64 = 1e-6;
const ENERGY_PROGRESS_RELATIVE: f64 = 1e-8;

fn preview_control(
    id: &'static str,
    name: &'static str,
    label: &'static str,
    action: Action,
    enabled: bool,
    checked: Option<bool>,
    hint: &'static str,
) -> Element<'static, Message> {
    let primary = matches!(action, Action::Apply) || checked == Some(true);
    let fill = matches!(
        action,
        Action::Apply | Action::Cancel | Action::Start | Action::Stop
    );
    let value = match &action {
        Action::Field(field) => Some(field.to_string()),
        _ => None,
    };
    let mut control = reshiki::accessibility::button(id, name, text(label).size(12))
        .on_press_maybe(enabled.then_some(Message::Optimization(action)))
        .padding([7, 8])
        .style(if primary {
            crate::appearance::primary
        } else {
            crate::appearance::secondary
        });
    if let Some(checked) = checked {
        control = control.checked(checked);
    }
    if let Some(value) = value {
        control = control.value(value);
    }
    if fill {
        control = control.width(Length::Fill);
    }
    super::workspace::hover_hint(control, hint, tooltip::Position::Left).into()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Key {
    tab: TabId,
    epoch: u64,
    revision: u64,
    session: u64,
    constraints: u64,
    sequence: u64,
}

#[derive(Clone, Debug)]
pub(crate) enum Action {
    Begin,
    Field(ForceField),
    Start,
    Stop,
    Apply,
    Cancel,
    PinSelected,
    UnpinSelected,
    ClearPins,
    ShowOriginal(bool),
    Rotate(f64, f64),
    Roll(f64),
    DepthEnhancement(bool),
    ClearDepthAppearance,
    WorkerDone(Key, Result<Arc<Optimized>, String>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Preparing,
    Paused,
    Running,
    Applying,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Work {
    Generate,
    Relax,
    Apply,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RelaxationPause {
    Stalled,
    Limit,
}

impl RelaxationPause {
    fn phase(self) -> &'static str {
        match self {
            Self::Stalled => "Paused · No progress",
            Self::Limit => "Paused · Not converged",
        }
    }

    fn status(self) -> &'static str {
        match self {
            Self::Stalled => {
                "Relaxation paused: no further energy improvement · Preview retained · Edit targets or pins, or press Start to retry"
            }
            Self::Limit => {
                "Relaxation paused before convergence · Preview retained · Edit targets or pins, or press Start to retry"
            }
        }
    }
}

/// Bounded work for physical constraints, independent of per-request keys and
/// view changes. Only the latest fixed-point snapshot is retained.
#[derive(Default)]
struct Relaxation {
    field: Option<ForceField>,
    pins: Vec<Pin>,
    batches: u32,
    best_energy: Option<f64>,
    stagnant_batches: u32,
    paused: Option<RelaxationPause>,
}

impl Relaxation {
    fn matches(&self, field: ForceField, pins: &[Pin]) -> bool {
        self.field == Some(field)
            && self.pins.len() == pins.len()
            && self.pins.iter().zip(pins).all(|(before, after)| {
                before.atom == after.atom && before.position == after.position
            })
    }

    fn update(&mut self, field: ForceField, pins: Vec<Pin>) -> bool {
        if self.matches(field, &pins) {
            return false;
        }
        *self = Self {
            field: Some(field),
            pins,
            ..Self::default()
        };
        true
    }

    fn completed(&mut self, result: &Optimized) -> Option<RelaxationPause> {
        if result.converged {
            return None;
        }
        let best = self.best_energy.unwrap_or(result.initial_energy);
        let tolerance = ENERGY_PROGRESS_ABSOLUTE + ENERGY_PROGRESS_RELATIVE * best.abs();
        if result.energy < best - tolerance {
            self.best_energy = Some(result.energy);
            self.stagnant_batches = 0;
        } else {
            // Compare against the best energy, so small improvements can add
            // up across batches instead of repeatedly resetting the baseline.
            self.best_energy = Some(best);
            self.stagnant_batches = self.stagnant_batches.saturating_add(1);
        }
        if self.batches >= MAX_RELAXATION_BATCHES {
            Some(RelaxationPause::Limit)
        } else if self.stagnant_batches >= MAX_STAGNANT_BATCHES {
            Some(RelaxationPause::Stalled)
        } else {
            None
        }
    }
}

struct Flight {
    key: Key,
    work: Work,
    handle: iced::task::Handle,
    drag: Option<(u64, Point3)>,
    pins: Vec<Pin>,
    field: ForceField,
}

#[derive(Clone, Copy)]
struct Drag {
    atom: u64,
    depth: f32,
    target: Point3,
    released: bool,
}

pub(super) struct Session {
    tab: TabId,
    epoch: u64,
    revision: u64,
    id: u64,
    prepared: Arc<Prepared>,
    initial_pins: Vec<u64>,
    pins: BTreeMap<u64, Point3>,
    pin_ids: Vec<u64>,
    conformer: Option<Conformer>,
    frame: Option<ViewFrame>,
    preview: Document,
    original: bool,
    phase: Phase,
    running: bool,
    pending: bool,
    constraints: u64,
    sequence: u64,
    flight: Option<Flight>,
    drag: Option<Drag>,
    drag_original: Option<Conformer>,
    drag_original_energy: Option<f64>,
    field: ForceField,
    energy: Option<f64>,
    iterations: u64,
    converged: bool,
    initialization: Option<&'static str>,
    relaxation: Relaxation,
    notice: Option<String>,
    depth_enhancement: bool,
    depth_scopes: Vec<reshiki::depth_appearance::Scope>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct AccessibilityContext {
    session: u64,
    field: ForceField,
    running: bool,
    original: bool,
    depth_enhancement: bool,
    pins: usize,
    phase: u8,
    ready: bool,
    can_start: bool,
}

impl Session {
    pub(super) fn accessibility_context(&self) -> AccessibilityContext {
        let ready = self.conformer.is_some() && self.phase != Phase::Applying;
        AccessibilityContext {
            session: self.id,
            field: self.field,
            running: self.running,
            original: self.original,
            depth_enhancement: self.depth_enhancement,
            pins: self.pins.len(),
            phase: match self.phase {
                Phase::Preparing => 0,
                Phase::Paused => 1,
                Phase::Running => 2,
                Phase::Applying => 3,
                Phase::Failed => 4,
            },
            ready,
            can_start: self.phase != Phase::Applying && (ready || self.flight.is_none()),
        }
    }

    pub(super) fn current(&self, tab: TabId, epoch: u64, revision: u64) -> bool {
        self.tab == tab && self.epoch == epoch && self.revision == revision
    }

    pub(super) fn preview_document(&self) -> &Document {
        if self.original {
            self.prepared.source()
        } else {
            &self.preview
        }
    }

    fn key(&mut self) -> Key {
        self.constraints = self.constraints.wrapping_add(1);
        self.sequence = self.sequence.wrapping_add(1);
        Key {
            tab: self.tab,
            epoch: self.epoch,
            revision: self.revision,
            session: self.id,
            constraints: self.constraints,
            sequence: self.sequence,
        }
    }

    fn accepts(&self, key: Key, tab: TabId, epoch: u64, revision: u64) -> bool {
        self.current(tab, epoch, revision)
            && key.tab == self.tab
            && key.epoch == self.epoch
            && key.revision == self.revision
            && key.session == self.id
            && key.constraints == self.constraints
            && key.sequence == self.sequence
            && self.flight.as_ref().is_some_and(|flight| flight.key == key)
    }

    fn effective_conformer(&self) -> Option<Conformer> {
        let mut conformer = self.conformer.clone()?;
        for (id, target) in &self.pins {
            if let Some(index) = self.prepared.index(*id)
                && let Some(position) = conformer.positions.get_mut(index)
            {
                *position = *target;
            }
        }
        if let Some(drag) = self.drag
            && let Some(index) = self.prepared.index(drag.atom)
            && let Some(position) = conformer.positions.get_mut(index)
        {
            *position = drag.target;
        }
        Some(conformer)
    }

    fn constraints(&self) -> Vec<Pin> {
        let mut pins: Vec<_> = self
            .pins
            .iter()
            .filter_map(|(id, position)| {
                Some(Pin {
                    atom: self.prepared.index(*id)?,
                    position: *position,
                })
            })
            .collect();
        if let Some(drag) = self.drag
            && !self.pins.contains_key(&drag.atom)
            && let Some(atom) = self.prepared.index(drag.atom)
        {
            pins.push(Pin {
                atom,
                position: drag.target,
            });
        }
        pins
    }

    fn render(&mut self) -> Result<(), String> {
        let (Some(conformer), Some(frame)) = (self.effective_conformer(), &self.frame) else {
            return Ok(());
        };
        let mut preview = self
            .prepared
            .document(&conformer, frame)
            .map_err(|e| e.to_string())?;
        preview.depth_appearance = self.depth_scopes.clone();
        if self.depth_enhancement {
            reshiki::depth_appearance::enable(
                &mut preview,
                self.prepared.ids(),
                reshiki::depth_appearance::DEFAULT_STRENGTH,
            )?;
        }
        self.depth_scopes = preview.depth_appearance.clone();
        self.preview = preview;
        Ok(())
    }

    fn abort(&mut self) {
        if let Some(flight) = self.flight.take() {
            flight.handle.abort();
        }
        self.sequence = self.sequence.wrapping_add(1);
    }

    fn pause(&mut self) {
        self.abort();
        // Preserve the frozen displayed geometry; Apply still validates it.
        if let Some(conformer) = self.effective_conformer() {
            self.conformer = Some(conformer);
        }
        self.drag = None;
        self.drag_original = None;
        self.drag_original_energy = None;
        self.running = false;
        self.pending = false;
        // Explicit Stop remains stopped after later edits. Automatic pauses
        // retain their reason so changed constraints can resume live work.
        self.relaxation.paused = None;
        if self.phase != Phase::Failed {
            self.phase = Phase::Paused;
        }
    }

    fn dispatch(&mut self, work: Work) -> Task<Message> {
        if self.flight.is_some() {
            return Task::none();
        }
        let conformer = self.effective_conformer();
        if work != Work::Generate && conformer.is_none() {
            return Task::none();
        }
        let key = self.key();
        let prepared = self.prepared.clone();
        let field = self.field;
        let pins = self.constraints();
        let fixed = pins.clone();
        let drag = self.drag.map(|drag| (drag.atom, drag.target));
        self.pending = false;
        let task = Task::perform(
            async move {
                let client = Client::default();
                let result = match (work, conformer.as_ref()) {
                    (Work::Generate, _) => client.generate(&prepared, field).await,
                    (Work::Relax, Some(conformer)) => {
                        client
                            .relax(&prepared, conformer, field, &pins, BATCH_ITERATIONS)
                            .await
                    }
                    (Work::Apply, Some(conformer)) => {
                        client.evaluate(&prepared, conformer, field, &pins).await
                    }
                    (_, None) => Err(reshiki::geometry::Error::Coordinates(
                        "Missing optimization coordinates",
                    )),
                };
                result.map(Arc::new).map_err(|e| e.to_string())
            },
            move |result| Message::Optimization(Action::WorkerDone(key, result)),
        );
        let (task, handle) = task.abortable();
        self.flight = Some(Flight {
            key,
            work,
            handle: handle.abort_on_drop(),
            drag,
            pins: fixed,
            field,
        });
        if work == Work::Relax {
            self.relaxation.batches = self.relaxation.batches.saturating_add(1);
        }
        task
    }

    fn sync_relaxation(&mut self) {
        let resume = self.relaxation.paused.is_some();
        let pins = self.constraints();
        if self.relaxation.update(self.field, pins) && resume {
            self.running = true;
            self.pending = true;
            self.phase = Phase::Running;
        }
    }

    fn pause_relaxation(&mut self, reason: RelaxationPause) {
        self.running = false;
        self.pending = false;
        self.phase = Phase::Paused;
        self.relaxation.paused = Some(reason);
    }

    fn next(&mut self) -> Task<Message> {
        self.sync_relaxation();
        if self.conformer.is_some()
            && self.phase != Phase::Applying
            && self.running
            && (!self.converged || self.pending)
        {
            if self.flight.is_none() && self.relaxation.batches >= MAX_RELAXATION_BATCHES {
                self.pause_relaxation(RelaxationPause::Limit);
                return Task::none();
            }
            self.dispatch(Work::Relax)
        } else {
            Task::none()
        }
    }

    fn next_with_status(&mut self, status: &mut String) -> Task<Message> {
        let was_paused = self.relaxation.paused.is_some();
        let task = self.next();
        if was_paused && self.running {
            *status = "Relaxation resumed for the edited targets · Preview retained".into();
        } else if let Some(reason) = self.relaxation.paused {
            *status = reason.status().into();
        }
        task
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.abort();
    }
}

impl App {
    fn show_optimization_properties(&mut self) -> Task<Message> {
        let old_width = self.inspector_width();
        self.inspector_open = true;
        self.inspector_tab = InspectorTab::Properties;
        // Keep the molecule at the same screen position when the right panel
        // appears; its resize notification must not trigger another Fit.
        let width_change = old_width - self.inspector_width();
        self.tab.camera.center.x += width_change / (2. * self.tab.camera.zoom);
        self.viewport.width += width_change;
        iced::widget::operation::snap_to(
            "inspector-content",
            iced::widget::operation::RelativeOffset::START,
        )
    }

    pub(super) fn cancel_optimization(&mut self) {
        self.tab.optimization_serial = self.tab.optimization_serial.wrapping_add(1);
        self.tab.optimization = None;
    }

    pub(super) fn pause_optimization(&mut self) {
        if let Some(session) = &mut self.tab.optimization {
            session.pause();
        }
    }

    pub(super) fn optimization_canvas(&self) -> Option<Context<'_>> {
        let session = self.tab.optimization.as_ref()?;
        Some(Context {
            session: session.id,
            atoms: session.prepared.ids(),
            pins: &session.pin_ids,
            interactive: session.conformer.is_some()
                && !session.original
                && !matches!(session.phase, Phase::Preparing | Phase::Applying),
        })
    }

    pub(super) fn optimization_gate(&mut self, message: &Message) -> Option<Task<Message>> {
        self.tab.optimization.as_ref()?;
        if matches!(message, Message::Inspector(InspectorTab::Properties)) {
            return Some(self.show_optimization_properties());
        }
        if matches!(message, Message::Escape) {
            self.cancel_optimization();
            self.status = "3D preview cancelled · Drawing unchanged".into();
            self.error = false;
            return Some(Task::none());
        }
        let allowed = matches!(
            message,
            Message::Optimization(_)
                | Message::Canvas(
                    Edit::Select(_)
                        | Edit::RelaxDragStart { .. }
                        | Edit::RelaxDragTarget { .. }
                        | Edit::RelaxDragEnd { .. }
                        | Edit::RelaxDragCancel { .. }
                        | Edit::RelaxRotate { .. }
                        | Edit::Pan(..)
                        | Edit::Zoom(..)
                        | Edit::Hover(_)
                )
                | Message::Tool(Tool::Select | Tool::Tilt)
                | Message::InspectorScroll(_)
                | Message::Viewport(_)
                | Message::Fit
                | Message::Zoom(_)
                | Message::ToggleInspector
                | Message::Inspector(_)
                | Message::Appearance(_)
                | Message::ToggleView
                | Message::ObjectToolbar(super::object_toolbar::Action::Visible(_))
                | Message::Grid
                | Message::SmartGuides(_)
                | Message::Rulers(_)
                | Message::Crosshair(_)
                | Message::RulerUnit(_)
                | Message::Tick
                | Message::EngineDone { .. }
                | Message::Close(_)
                | Message::Discard
                | Message::Cancel
                | Message::New
                | Message::Open
                | Message::Tabs(_)
                | Message::Saved(..)
                | Message::Exported(_)
                | Message::FigureExported(_)
                | Message::Autosaved(..)
                | Message::FilePrepared(_)
                | Message::Opened(_)
                | Message::ClipboardRead { .. }
                | Message::ClipboardWritten { .. }
                | Message::CopyAsPrepared(..)
                | Message::CopyAsWritten(..)
                | Message::Updates(_)
        ) || super::tabs::document_result(message)
            || super::updates::background(message)
            || self.answers_save_dialog(message);
        if allowed {
            None
        } else {
            self.status = "Apply or cancel the 3D preview to continue editing".into();
            Some(Task::none())
        }
    }

    pub(super) fn optimization_action(&mut self, action: Action) -> Task<Message> {
        if matches!(action, Action::Begin) {
            if self.tab.busy
                || self.tab.cleanup.is_some()
                || self.tab.optimization.is_some()
                || self.tab.inline_text.is_some()
                || self.tab.atom_text.is_some()
                || self.tab.joining.is_some()
            {
                return Task::none();
            }
            let prepared = match Prepared::new(&self.tab.doc, &self.tab.selected) {
                Ok(prepared) => Arc::new(prepared),
                Err(error) => {
                    self.status = error.to_string();
                    self.error = true;
                    return Task::none();
                }
            };
            let selected = self
                .tab
                .doc
                .expand_abbreviation_selection(&self.tab.selected);
            let mut initial_pins: Vec<_> = prepared
                .ids()
                .iter()
                .copied()
                .filter(|id| selected.contains(id))
                .collect();
            if initial_pins.len() == prepared.ids().len() {
                initial_pins.clear();
            }
            self.tab.optimization_serial = self.tab.optimization_serial.wrapping_add(1);
            let mut session = Session {
                tab: self.tab.id,
                epoch: self.tab.file_epoch,
                revision: self.tab.revision,
                id: self.tab.optimization_serial,
                preview: self.tab.doc.clone(),
                prepared,
                initial_pins,
                pins: BTreeMap::new(),
                pin_ids: vec![],
                conformer: None,
                frame: None,
                original: false,
                phase: Phase::Preparing,
                running: false,
                pending: false,
                constraints: 0,
                sequence: 0,
                flight: None,
                drag: None,
                drag_original: None,
                drag_original_energy: None,
                field: ForceField::Mmff94s,
                energy: None,
                iterations: 0,
                converged: false,
                initialization: None,
                relaxation: Relaxation::default(),
                notice: None,
                depth_enhancement: true,
                depth_scopes: self.tab.doc.depth_appearance.clone(),
            };
            let task = session.dispatch(Work::Generate);
            self.tab.optimization = Some(session);
            self.tool = Tool::Select;
            self.status = "Generating stereo-preserving 3D · MMFF94s".into();
            self.error = false;
            return Task::batch([task, self.show_optimization_properties()]);
        }
        if matches!(action, Action::Cancel) {
            self.cancel_optimization();
            self.status = "3D preview cancelled · Drawing unchanged".into();
            self.error = false;
            return Task::none();
        }
        let Some(session) = &mut self.tab.optimization else {
            return Task::none();
        };
        if !session.current(self.tab.id, self.tab.file_epoch, self.tab.revision) {
            self.cancel_optimization();
            return Task::none();
        }
        if let Action::WorkerDone(key, result) = action {
            if !session.accepts(key, self.tab.id, self.tab.file_epoch, self.tab.revision) {
                return Task::none();
            }
            let Some(flight) = session.flight.take() else {
                return Task::none();
            };
            let result = match result {
                Ok(result) => result,
                Err(error) => {
                    session.running = false;
                    session.pending = false;
                    session.phase = Phase::Failed;
                    session.notice = Some(error.clone());
                    self.status = error;
                    self.error = true;
                    return Task::none();
                }
            };
            // An exact fixed point is a contract, not a soft positional restraint.
            if flight.pins.iter().any(|pin| {
                result
                    .conformer
                    .positions
                    .get(pin.atom)
                    .is_none_or(|position| {
                        (position.x - pin.position.x).abs() > 1e-7
                            || (position.y - pin.position.y).abs() > 1e-7
                            || (position.z - pin.position.z).abs() > 1e-7
                    })
            }) {
                session.running = false;
                session.pending = false;
                session.phase = Phase::Failed;
                session.notice =
                    Some("The optimizer moved a pinned atom; the result was discarded".into());
                self.status = "The optimizer moved a pinned atom; the result was discarded".into();
                self.error = true;
                return Task::none();
            }
            if session.frame.is_none() {
                match session.prepared.view_frame(&result.conformer) {
                    Ok(frame) => session.frame = Some(frame),
                    Err(error) => {
                        session.phase = Phase::Failed;
                        session.notice = Some(error.to_string());
                        self.status = error.to_string();
                        self.error = true;
                        return Task::none();
                    }
                }
                for id in &session.initial_pins {
                    if let Some(index) = session.prepared.index(*id)
                        && let Some(position) = result.conformer.positions.get(index)
                    {
                        session.pins.insert(*id, *position);
                    }
                }
                session.pin_ids = session.pins.keys().copied().collect();
            }
            session.conformer = Some(result.conformer.clone());
            session.field = result.force_field;
            session.energy = (!session.pending).then_some(result.energy);
            session.iterations = session
                .iterations
                .saturating_add(u64::from(result.iterations));
            session.converged = result.converged;
            if flight.work == Work::Generate {
                session.initialization = result.diagnostics.iter().rev().find_map(|diagnostic| {
                    if diagnostic.starts_with("initialization=existing-3d") {
                        Some("Existing 3D geometry")
                    } else if diagnostic.starts_with("initialization=cage-ETDG") {
                        Some("Cage starting geometry")
                    } else if diagnostic.starts_with("initialization=ETDG-fallback") {
                        Some("Alternative starting geometry")
                    } else if diagnostic.starts_with("initialization=single-conformer") {
                        Some("Single conformer")
                    } else {
                        None
                    }
                });
            }
            // Successful initialization is context, not an error notice.
            session.notice = None;
            if let Some(drag) = session.drag
                && drag.released
                && flight.drag == Some((drag.atom, drag.target))
            {
                session.drag = None;
                session.drag_original = None;
                session.drag_original_energy = None;
                session.pending = flight.work != Work::Apply;
            }
            if let Err(error) = session.render() {
                session.phase = Phase::Failed;
                session.running = false;
                session.notice = Some(error.clone());
                self.status = error;
                self.error = true;
                return Task::none();
            }
            if flight.work == Work::Apply {
                let document = session.preview.clone();
                self.cancel_optimization();
                let before = self.tab.doc.clone();
                self.tab.doc = document;
                self.changed(before);
                if !self.error {
                    self.status =
                        "3D projection applied · Undo restores the original drawing".into();
                }
                return Task::none();
            }
            session.sync_relaxation();
            if flight.work == Work::Relax
                && session.relaxation.matches(flight.field, &flight.pins)
                && let Some(reason) = session.relaxation.completed(&result)
            {
                session.pause_relaxation(reason);
                self.status = reason.status().into();
                self.error = false;
                return Task::none();
            }
            session.phase = if session.running {
                Phase::Running
            } else {
                Phase::Paused
            };
            self.status = if result.converged {
                "3D preview converged · Rotate, drag, then Apply or Cancel"
            } else {
                "3D preview · Select atoms to pin or drag unpinned atoms"
            }
            .into();
            self.error = false;
            return session.next_with_status(&mut self.status);
        }
        if session.phase == Phase::Applying {
            return Task::none();
        }
        match action {
            Action::Start => {
                if session.conformer.is_none() {
                    session.phase = Phase::Preparing;
                    return session.dispatch(Work::Generate);
                }
                session.original = false;
                session.running = true;
                session.pending = true;
                session.phase = Phase::Running;
                session.notice = None;
                session.relaxation = Relaxation::default();
                self.status = "Relaxing 3D preview…".into();
                return session.next_with_status(&mut self.status);
            }
            Action::Stop => {
                session.pause();
                self.status = "Relaxation stopped · Preview retained".into();
            }
            Action::Apply => {
                if session.conformer.is_none() || session.phase == Phase::Failed {
                    return Task::none();
                }
                session.pause();
                session.original = false;
                session.phase = Phase::Applying;
                self.status = "Checking the final 3D projection…".into();
                return session.dispatch(Work::Apply);
            }
            Action::Field(field) => {
                session.abort();
                session.field = field;
                session.energy = None;
                session.notice = None;
                if session.conformer.is_none() {
                    session.phase = Phase::Preparing;
                    return session.dispatch(Work::Generate);
                }
                session.phase = if session.running {
                    Phase::Running
                } else {
                    Phase::Paused
                };
                session.pending = true;
                return session.next_with_status(&mut self.status);
            }
            Action::PinSelected => {
                if let Some(conformer) = session.effective_conformer() {
                    for id in &self.tab.selected {
                        if let Some(index) = session.prepared.index(*id)
                            && let Some(position) = conformer.positions.get(index)
                        {
                            session.pins.insert(*id, *position);
                        }
                    }
                }
                session.pin_ids = session.pins.keys().copied().collect();
                session.pending = true;
            }
            Action::UnpinSelected => {
                for id in &self.tab.selected {
                    session.pins.remove(id);
                }
                session.pin_ids = session.pins.keys().copied().collect();
                session.pending = true;
            }
            Action::ClearPins => {
                session.pins.clear();
                session.pin_ids.clear();
                session.pending = true;
            }
            Action::ShowOriginal(show) => session.original = show,
            Action::Rotate(x, y) => {
                if let Some(frame) = &mut session.frame {
                    frame.rotate(x, y);
                }
            }
            Action::Roll(degrees) => {
                if let Some(frame) = &mut session.frame {
                    frame.roll(degrees);
                }
            }
            Action::DepthEnhancement(enabled) => {
                if !enabled {
                    reshiki::depth_appearance::freeze(&mut session.preview, session.prepared.ids());
                    session.depth_scopes = session.preview.depth_appearance.clone();
                }
                session.depth_enhancement = enabled;
            }
            Action::ClearDepthAppearance => {
                reshiki::depth_appearance::clear(&mut session.preview, session.prepared.ids());
                session.depth_scopes = session.preview.depth_appearance.clone();
                session.depth_enhancement = false;
            }
            Action::Begin | Action::Cancel | Action::WorkerDone(..) => return Task::none(),
        }
        if let Err(error) = session.render() {
            session.notice = Some(error);
        }
        session.next_with_status(&mut self.status)
    }

    pub(super) fn optimization_edit(&mut self, edit: Edit) -> Option<Task<Message>> {
        let optimizer_edit = matches!(
            &edit,
            Edit::RelaxDragStart { .. }
                | Edit::RelaxDragTarget { .. }
                | Edit::RelaxDragEnd { .. }
                | Edit::RelaxDragCancel { .. }
                | Edit::RelaxRotate { .. }
        );
        let Some(session) = &mut self.tab.optimization else {
            return optimizer_edit.then(Task::none);
        };
        if !session.current(self.tab.id, self.tab.file_epoch, self.tab.revision) {
            return Some(Task::none());
        }
        match edit {
            Edit::Select(ids) => {
                self.tab.selected = ids
                    .into_iter()
                    .filter(|id| session.prepared.index(*id).is_some())
                    .collect();
            }
            Edit::RelaxDragStart { session: id, atom } if id == session.id => {
                if session.original
                    || session.phase == Phase::Applying
                    || session.pins.contains_key(&atom)
                {
                    return Some(Task::none());
                }
                let index = session.prepared.index(atom)?;
                let position = *session.conformer.as_ref()?.positions.get(index)?;
                let depth = session.frame.as_ref()?.project(position).1;
                self.tab.selected = vec![atom];
                session.drag_original = session.effective_conformer();
                session.drag_original_energy = session.energy;
                session.drag = Some(Drag {
                    atom,
                    depth,
                    target: position,
                    released: false,
                });
            }
            Edit::RelaxDragTarget {
                session: id,
                atom,
                target,
            } if id == session.id => {
                if let (Some(drag), Some(frame)) = (&mut session.drag, &session.frame)
                    && drag.atom == atom
                    && target.x.is_finite()
                    && target.y.is_finite()
                {
                    drag.target = frame.unproject(target, drag.depth);
                    session.pending = true;
                    session.energy = None;
                }
            }
            Edit::RelaxDragEnd {
                session: id,
                atom,
                target,
            } if id == session.id => {
                if let (Some(drag), Some(frame)) = (&mut session.drag, &session.frame)
                    && drag.atom == atom
                {
                    if let Some(target) = target.filter(|p| p.x.is_finite() && p.y.is_finite()) {
                        drag.target = frame.unproject(target, drag.depth);
                    }
                    drag.released = true;
                    session.pending = true;
                    session.energy = None;
                }
            }
            Edit::RelaxDragCancel { session: id } if id == session.id => {
                if session.drag.take().is_some() {
                    session.abort();
                    if let Some(original) = session.drag_original.take() {
                        session.conformer = Some(original);
                    }
                    session.energy = session.drag_original_energy.take();
                    session.pending = false;
                }
            }
            Edit::RelaxRotate { session: id, x, y } if id == session.id => {
                if let Some(frame) = &mut session.frame {
                    frame.rotate(x, y);
                }
            }
            Edit::RelaxDragStart { .. }
            | Edit::RelaxDragTarget { .. }
            | Edit::RelaxDragEnd { .. }
            | Edit::RelaxDragCancel { .. }
            | Edit::RelaxRotate { .. } => return Some(Task::none()),
            _ => return None,
        }
        if let Err(error) = session.render() {
            session.notice = Some(error);
        }
        Some(session.next_with_status(&mut self.status))
    }

    pub(super) fn optimization_panel(&self) -> Element<'_, Message> {
        let Some(session) = &self.tab.optimization else {
            return Space::new().into();
        };
        let ready = session.conformer.is_some() && session.phase != Phase::Applying;
        let editable = session.phase != Phase::Applying;
        let phase = match session.phase {
            Phase::Preparing => "Generating 3D…",
            Phase::Paused if session.relaxation.paused.is_some() => session
                .relaxation
                .paused
                .map_or("Paused", RelaxationPause::phase),
            Phase::Paused if session.pending => "Paused · Ready to relax",
            Phase::Paused if session.converged => "Paused · Converged",
            Phase::Paused => "Paused",
            Phase::Running if session.converged && session.flight.is_none() => {
                "Live relaxation ready"
            }
            Phase::Running => "Relaxing…",
            Phase::Applying => "Checking projection…",
            Phase::Failed => "3D needs attention",
        };
        let energy = session
            .energy
            .map(|energy| format!("{energy:.3} kcal/mol"))
            .unwrap_or_else(|| {
                if session.flight.is_some() {
                    "Energy updating…"
                } else {
                    "Start to calculate energy"
                }
                .into()
            });
        let phase = session.initialization.map_or_else(
            || phase.to_owned(),
            |initialization| format!("{phase} · {initialization}"),
        );
        let field = |id, name, label, field| {
            preview_control(
                id,
                name,
                label,
                Action::Field(field),
                editable,
                Some(session.field == field),
                "Choose the force field used to relax this molecule",
            )
        };
        fn section<'a>(
            title: &'static str,
            controls: Element<'a, Message>,
        ) -> Element<'a, Message> {
            column![
                text(title).size(11).style(super::workspace::muted_text),
                controls
            ]
            .spacing(5)
            .into()
        }
        let mut panel = column![
            text("3D preview").size(14),
            text(phase).size(12),
            text(format!("{} pinned · {energy}", session.pins.len()))
                .size(11).style(super::workspace::muted_text),
            section("Force field & relaxation", column![
                row![
                    field("optimization.field.mmff94", "Use MMFF94 force field", "MMFF94", ForceField::Mmff94),
                    field("optimization.field.mmff94s", "Use MMFF94s force field", "MMFF94s", ForceField::Mmff94s),
                    field("optimization.field.uff", "Use UFF force field", "UFF", ForceField::Uff),
                ].spacing(4).align_y(Alignment::Center).width(Length::Fill).wrap(),
                preview_control("optimization.run", if session.running { "Stop relaxation" } else if session.conformer.is_none() { "Generate 3D" } else { "Start relaxation" },
                    if session.running { "Stop relaxation" } else if session.conformer.is_none() { "Generate 3D" } else { "Start relaxation" },
                    if session.running { Action::Stop } else { Action::Start },
                    editable && (ready || session.flight.is_none()), None,
                    "Start or pause relaxation; the preview stays editable until you Apply or Cancel"),
            ].spacing(6).into()),
            section("Pins", row![
                preview_control("optimization.pin-selected", "Pin selected atoms", "Pin selected", Action::PinSelected, ready && !self.tab.selected.is_empty(), None,
                    "Hold selected atoms at their current 3D positions during relaxation"),
                preview_control("optimization.unpin-selected", "Unpin selected atoms", "Unpin", Action::UnpinSelected, ready && !self.tab.selected.is_empty(), None,
                    "Release selected atoms so they can move and be dragged"),
                preview_control("optimization.clear-pins", "Clear all atom pins", "Clear pins", Action::ClearPins, ready && !session.pins.is_empty(), None,
                    "Release every pinned atom in this preview"),
            ].spacing(6).align_y(Alignment::Center).width(Length::Fill).wrap().into()),
            section("View", column![
                row![
                    preview_control("optimization.rotate-left", "Rotate projection left", "↶", Action::Rotate(0., -15.), ready, None, "Rotate the view left by 15°; the molecule's energy and pins stay fixed"),
                    preview_control("optimization.rotate-right", "Rotate projection right", "↷", Action::Rotate(0., 15.), ready, None, "Rotate the view right by 15°; the molecule's energy and pins stay fixed"),
                    preview_control("optimization.tilt-up", "Tilt projection up", "Tilt up", Action::Rotate(15., 0.), ready, None, "Tilt the view up by 15°"),
                    preview_control("optimization.tilt-down", "Tilt projection down", "Tilt down", Action::Rotate(-15., 0.), ready, None, "Tilt the view down by 15°"),
                    preview_control("optimization.roll", "Roll projection clockwise", "Roll", Action::Roll(15.), ready, None, "Roll the view clockwise by 15°"),
                ].spacing(4).align_y(Alignment::Center).width(Length::Fill).wrap().vertical_spacing(5),
                preview_control("optimization.show-original", "Show original drawing", "Show original", Action::ShowOriginal(!session.original), editable, Some(session.original),
                    "Compare with the original drawing without discarding this preview"),
            ].spacing(5).into()),
            section("Depth appearance", row![
                preview_control("optimization.automatic-depth", "Automatic depth appearance", "Automatic depth", Action::DepthEnhancement(!session.depth_enhancement), editable, Some(session.depth_enhancement),
                    "Update depth shading as you rotate; turn off to keep the current shading"),
                preview_control("optimization.clear-depth", "Clear depth appearance", "Clear depth", Action::ClearDepthAppearance, ready, None,
                    "Restore original ink while retaining the 3D projection"),
            ].spacing(6).align_y(Alignment::Center).width(Length::Fill).wrap().into()),
            text("Shift-click selects atoms to pin. Unpin to drag; use the Tilt tool to rotate freely.")
                .size(11).style(super::workspace::muted_text),
        ].spacing(10).width(Length::Fill);
        if session.phase == Phase::Preparing {
            panel = panel.push(
                text("Building the preview. Apply becomes available when it is ready.")
                    .size(11)
                    .style(super::workspace::muted_text),
            );
        } else if let Some(reason) = session.relaxation.paused {
            panel = panel.push(
                text(reason.status())
                    .size(11)
                    .style(super::workspace::muted_text),
            );
        }
        if let Some(notice) = &session.notice {
            panel = panel.push(text(notice).size(11).style(crate::appearance::text_color(
                iced::Color::from_rgb8(182, 66, 61),
            )));
        }
        panel.into()
    }

    pub(super) fn optimization_footer(&self) -> Element<'_, Message> {
        let Some(session) = &self.tab.optimization else {
            return Space::new().into();
        };
        let ready = session.conformer.is_some()
            && !matches!(session.phase, Phase::Applying | Phase::Failed);
        row![
            preview_control(
                "optimization.cancel",
                "Cancel 3D preview",
                "Cancel",
                Action::Cancel,
                true,
                None,
                "Discard the preview and keep the original drawing"
            ),
            preview_control(
                "optimization.apply",
                "Apply 3D projection",
                "Apply",
                Action::Apply,
                ready,
                None,
                "Keep this projection as one editable, undoable change"
            ),
        ]
        .spacing(8)
        .width(Length::Fill)
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reshiki::document::Point;

    fn drawing() -> (App, u64, u64, Document) {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        let a = app.tab.doc.add_atom("C", Point::default());
        let b = app.tab.doc.add_atom("C", Point::new(42., 0.));
        app.tab.doc.add_bond(a, b, 1, "plain");
        app.tab.doc.add_atom("O", Point::new(180., 80.));
        app.tab.selected = vec![a];
        app.tab.saved = app.tab.doc.clone();
        let original = app.tab.doc.clone();
        (app, a, b, original)
    }

    fn result(session: &Session) -> Optimized {
        let conformer = session.effective_conformer().unwrap_or(Conformer {
            positions: vec![
                Point3::default(),
                Point3 {
                    x: 1.5,
                    y: 0.,
                    z: 0.4,
                },
            ],
            original_atom_count: 2,
            hydrogen_parents: vec![],
        });
        Optimized {
            conformer,
            initial_energy: 2.,
            energy: 1.,
            gradient: None,
            converged: true,
            iterations: 20,
            force_field: session.field,
            diagnostics: vec![],
        }
    }

    fn finish(app: &mut App) {
        let session = app.tab.optimization.as_ref().unwrap();
        let key = session.flight.as_ref().unwrap().key;
        let result = result(session);
        let _ = app.optimization_action(Action::WorkerDone(key, Ok(Arc::new(result))));
    }

    fn prepared_preview() -> (App, u64, u64, Document) {
        let (mut app, a, b, original) = drawing();
        let _ = app.optimization_action(Action::Begin);
        finish(&mut app);
        (app, a, b, original)
    }

    #[test]
    fn initialization_context_survives_relaxation_without_an_error_or_source_edit() {
        for (diagnostic, label) in [
            ("initialization=existing-3d", "Existing 3D geometry"),
            ("initialization=cage-ETDG", "Cage starting geometry"),
            (
                "initialization=ETDG-fallback",
                "Alternative starting geometry",
            ),
            ("initialization=single-conformer", "Single conformer"),
        ] {
            let (mut app, _, _, original) = drawing();
            let _ = app.optimization_action(Action::Begin);
            let session = app.tab.optimization.as_ref().unwrap();
            let key = session.flight.as_ref().unwrap().key;
            let mut computed = result(session);
            // Only the final initialization is shown after a recovered retry.
            computed.diagnostics = vec![
                "initialization=single-conformer; earlier retry".into(),
                diagnostic.into(),
            ];
            let _ = app.optimization_action(Action::WorkerDone(key, Ok(Arc::new(computed))));
            let session = app.tab.optimization.as_ref().unwrap();
            assert_eq!(session.initialization, Some(label));
            assert!(session.notice.is_none());
            assert!(!app.error);
            assert_eq!(app.tab.doc, original);

            let _ = app.optimization_action(Action::Start);
            finish(&mut app);
            assert_eq!(
                app.tab.optimization.as_ref().unwrap().initialization,
                Some(label)
            );
            let _ = app.optimization_action(Action::Cancel);
            assert_eq!(app.tab.doc, original);
        }
    }

    fn complete_live(app: &mut App, energy: f64, converged: bool) -> Task<Message> {
        let session = app.tab.optimization.as_ref().unwrap();
        let flight = session.flight.as_ref().unwrap();
        assert_eq!(flight.work, Work::Relax);
        let key = flight.key;
        let mut computed = result(session);
        computed.initial_energy = energy + 1.;
        computed.energy = energy;
        computed.converged = converged;
        app.optimization_action(Action::WorkerDone(key, Ok(Arc::new(computed))))
    }

    fn stall_live(app: &mut App) {
        for _ in 0..=MAX_STAGNANT_BATCHES {
            let _ = complete_live(app, 1., false);
        }
        let session = app.tab.optimization.as_ref().unwrap();
        assert_eq!(session.relaxation.paused, Some(RelaxationPause::Stalled));
        assert!(session.flight.is_none());
    }

    fn preview_controls(running: bool) -> [(&'static str, Action); 17] {
        [
            (
                "optimization.field.mmff94",
                Action::Field(ForceField::Mmff94),
            ),
            (
                "optimization.field.mmff94s",
                Action::Field(ForceField::Mmff94s),
            ),
            ("optimization.field.uff", Action::Field(ForceField::Uff)),
            (
                "optimization.run",
                if running { Action::Stop } else { Action::Start },
            ),
            ("optimization.pin-selected", Action::PinSelected),
            ("optimization.unpin-selected", Action::UnpinSelected),
            ("optimization.clear-pins", Action::ClearPins),
            ("optimization.rotate-left", Action::Rotate(0., -15.)),
            ("optimization.rotate-right", Action::Rotate(0., 15.)),
            ("optimization.tilt-up", Action::Rotate(15., 0.)),
            ("optimization.tilt-down", Action::Rotate(-15., 0.)),
            ("optimization.roll", Action::Roll(15.)),
            ("optimization.show-original", Action::ShowOriginal(true)),
            (
                "optimization.automatic-depth",
                Action::DepthEnhancement(false),
            ),
            ("optimization.clear-depth", Action::ClearDepthAppearance),
            ("optimization.cancel", Action::Cancel),
            ("optimization.apply", Action::Apply),
        ]
    }

    #[test]
    fn preview_and_cancellation_leave_document_revision_history_and_redo_untouched() {
        let (mut app, _, _, original) = drawing();
        let mut later = original.clone();
        later.add_atom("F", Point::new(240., 100.));
        app.tab.history.commit(original.clone(), &later);
        app.tab.doc = later;
        assert!(app.tab.history.undo(&mut app.tab.doc));
        let revision = app.tab.revision;
        let _ = app.optimization_action(Action::Begin);
        finish(&mut app);
        assert_eq!(app.tab.doc, original);
        assert_eq!(app.tab.revision, revision);
        assert!(app.tab.history.can_redo());
        let _ = app.optimization_action(Action::Rotate(25., 40.));
        let _ = app.optimization_action(Action::Cancel);
        assert_eq!(app.tab.doc, original);
        assert_eq!(app.tab.revision, revision);
        assert!(app.tab.history.can_redo());
        assert!(!app.tab.history.can_undo());
    }

    #[test]
    fn apply_validates_then_commits_once_with_exact_undo_and_redo() {
        let (mut app, _, _, original) = prepared_preview();
        let unrelated = original.atoms.last().unwrap().clone();
        let _ = app.optimization_action(Action::Rotate(20., 35.));
        let _ = app.optimization_action(Action::Apply);
        assert_eq!(app.tab.doc, original, "Application waits for validation");
        finish(&mut app);
        assert!(app.tab.optimization.is_none());
        let applied = app.tab.doc.clone();
        assert_ne!(applied, original);
        assert_eq!(applied.atom(unrelated.id), Some(&unrelated));
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, original);
        assert!(!app.tab.history.can_undo());
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, applied);
    }

    #[test]
    fn applying_after_live_drag_stop_and_depth_freeze_retains_pins_and_is_one_edit() {
        let (mut app, pinned, dragged, original) = prepared_preview();
        let _ = app.optimization_action(Action::Rotate(25., -35.));
        let _ = app.optimization_action(Action::PinSelected);
        let session = app.tab.optimization.as_ref().unwrap();
        let pin = session.pins[&pinned];
        let id = session.id;
        let target = session
            .preview
            .atom(dragged)
            .unwrap()
            .position
            .offset(2., 1.);
        let _ = app.optimization_action(Action::Start);
        let _ = app.optimization_edit(Edit::RelaxDragStart {
            session: id,
            atom: dragged,
        });
        let _ = app.optimization_edit(Edit::RelaxDragTarget {
            session: id,
            atom: dragged,
            target,
        });
        let _ = app.optimization_edit(Edit::RelaxDragEnd {
            session: id,
            atom: dragged,
            target: Some(target),
        });
        let _ = app.optimization_action(Action::Stop);
        let _ = app.optimization_action(Action::DepthEnhancement(false));
        let session = app.tab.optimization.as_ref().unwrap();
        assert_eq!(session.pins.get(&pinned), Some(&pin));
        assert!(session.flight.is_none());
        let coordinates = session.effective_conformer().unwrap().positions;
        let frozen = session.preview.depth_appearance.clone();
        assert!(!frozen.is_empty());
        assert!(frozen.iter().all(|scope| !scope.automatic));
        let projected_pin = session.frame.as_ref().unwrap().project(pin);
        let _ = app.optimization_action(Action::Apply);
        let session = app.tab.optimization.as_ref().unwrap();
        assert_eq!(session.phase, Phase::Applying);
        assert_eq!(
            session.effective_conformer().unwrap().positions,
            coordinates
        );
        assert_eq!(session.pins.get(&pinned), Some(&pin));
        let flight = session.flight.as_ref().unwrap();
        assert_eq!(flight.work, Work::Apply);
        assert_eq!(flight.pins.len(), 1);
        assert_eq!(flight.pins[0].atom, session.prepared.index(pinned).unwrap());
        assert_eq!(flight.pins[0].position, pin);
        assert_eq!(app.tab.doc, original, "Apply waits for its energy check");
        assert!(!app.tab.history.can_undo());
        finish(&mut app);
        assert!(app.tab.optimization.is_none());
        assert!(!app.error);
        let applied = app.tab.doc.clone();
        assert_eq!(applied.depth_appearance, frozen);
        assert_eq!(applied.atom(pinned).unwrap().position, projected_pin.0);
        assert_eq!(applied.atom(pinned).unwrap().depth, projected_pin.1);
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, original);
        assert!(!app.tab.history.can_undo());
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, applied);
    }

    #[test]
    fn stale_completions_never_clear_a_new_sessions_inflight_task_or_status() {
        let (mut app, _, _, _) = drawing();
        let _ = app.optimization_action(Action::Begin);
        let old = app
            .tab
            .optimization
            .as_ref()
            .unwrap()
            .flight
            .as_ref()
            .unwrap()
            .key;
        let _ = app.optimization_action(Action::Cancel);
        let _ = app.optimization_action(Action::Begin);
        let new = app
            .tab
            .optimization
            .as_ref()
            .unwrap()
            .flight
            .as_ref()
            .unwrap()
            .key;
        let status = app.status.clone();
        let _ = app.optimization_action(Action::WorkerDone(old, Err("Old failed request".into())));
        assert_eq!(app.status, status);
        assert!(!app.error);
        assert_eq!(
            app.tab
                .optimization
                .as_ref()
                .unwrap()
                .flight
                .as_ref()
                .unwrap()
                .key,
            new
        );
        let mut bad = new;
        bad.constraints = bad.constraints.wrapping_add(1);
        let _ = app.optimization_action(Action::WorkerDone(bad, Err("Old constraints".into())));
        assert_eq!(app.status, status);
        assert!(app.tab.optimization.as_ref().unwrap().flight.is_some());
        app.pause_optimization();
        let _ = app.optimization_action(Action::WorkerDone(new, Err("Cancelled request".into())));
        assert!(app.tab.optimization.as_ref().unwrap().flight.is_none());
        assert!(!app.error);
    }

    #[test]
    fn view_rotation_preserves_physical_pins_coordinates_and_energy_and_depth_can_freeze() {
        let (mut app, a, _, _) = prepared_preview();
        let session = app.tab.optimization.as_ref().unwrap();
        let pins = session.pins.clone();
        let coordinates = session.conformer.as_ref().unwrap().positions.clone();
        let energy = session.energy;
        assert!(pins.contains_key(&a));
        let _ = app.optimization_action(Action::Rotate(40., -30.));
        let session = app.tab.optimization.as_ref().unwrap();
        assert_eq!(session.pins, pins);
        assert_eq!(session.conformer.as_ref().unwrap().positions, coordinates);
        assert_eq!(session.energy, energy);
        let _ = app.optimization_action(Action::DepthEnhancement(false));
        let weights = app
            .tab
            .optimization
            .as_ref()
            .unwrap()
            .preview
            .depth_appearance
            .clone();
        assert!(weights.iter().all(|scope| !scope.automatic));
        let _ = app.optimization_action(Action::Rotate(35., 15.));
        assert_eq!(
            app.tab
                .optimization
                .as_ref()
                .unwrap()
                .preview
                .depth_appearance,
            weights
        );
        let _ = app.optimization_action(Action::ClearDepthAppearance);
        assert!(
            app.tab
                .optimization
                .as_ref()
                .unwrap()
                .preview
                .depth_appearance
                .is_empty()
        );
    }

    #[test]
    fn exact_pin_violation_is_rejected_without_replacing_the_preview() {
        for action in [Action::Start, Action::Apply] {
            let (mut app, a, _, original) = prepared_preview();
            let preview = app.tab.optimization.as_ref().unwrap().preview.clone();
            let _ = app.optimization_action(action);
            let session = app.tab.optimization.as_ref().unwrap();
            let key = session.flight.as_ref().unwrap().key;
            let mut invalid = result(session);
            invalid.conformer.positions[session.prepared.index(a).unwrap()].x += 0.01;
            let _ = app.optimization_action(Action::WorkerDone(key, Ok(Arc::new(invalid))));
            assert_eq!(app.tab.doc, original);
            assert_eq!(app.tab.optimization.as_ref().unwrap().preview, preview);
            assert_eq!(app.tab.optimization.as_ref().unwrap().phase, Phase::Failed);
            assert!(!app.tab.history.can_undo());
            assert!(app.error);
        }
    }

    #[test]
    fn pointer_targets_are_coalesced_behind_one_flight_and_use_inverse_view_projection() {
        let (mut app, a, _, _) = prepared_preview();
        let _ = app.optimization_action(Action::ClearPins);
        let _ = app.optimization_action(Action::Rotate(25., 40.));
        let id = app.tab.optimization.as_ref().unwrap().id;
        let _ = app.optimization_edit(Edit::RelaxDragStart {
            session: id,
            atom: a,
        });
        let _ = app.optimization_edit(Edit::RelaxDragTarget {
            session: id,
            atom: a,
            target: Point::new(10., 20.),
        });
        let _ = app.optimization_action(Action::Start);
        let first = app
            .tab
            .optimization
            .as_ref()
            .unwrap()
            .flight
            .as_ref()
            .unwrap()
            .key;
        for i in 0..100 {
            let _ = app.optimization_edit(Edit::RelaxDragTarget {
                session: id,
                atom: a,
                target: Point::new(i as f32, 35.),
            });
        }
        let session = app.tab.optimization.as_ref().unwrap();
        assert_eq!(session.flight.as_ref().unwrap().key, first);
        assert_eq!(session.sequence, first.sequence);
        assert_eq!(session.constraints, first.constraints);
        assert_eq!(session.constraints().len(), 1);
        let drag = session.drag.unwrap();
        let (point, depth) = session.frame.as_ref().unwrap().project(drag.target);
        assert!(point.distance(Point::new(99., 35.)) < 0.001);
        assert!((depth - drag.depth).abs() < 0.001);
    }

    #[test]
    fn cancelling_a_drag_restores_its_geometry_and_discards_a_late_fixed_target() {
        let (mut app, a, _, original_document) = prepared_preview();
        let _ = app.optimization_action(Action::ClearPins);
        let session = app.tab.optimization.as_ref().unwrap();
        let before = session.conformer.as_ref().unwrap().positions.clone();
        let energy = session.energy;
        let id = session.id;
        let _ = app.optimization_edit(Edit::RelaxDragStart {
            session: id,
            atom: a,
        });
        let _ = app.optimization_edit(Edit::RelaxDragTarget {
            session: id,
            atom: a,
            target: Point::new(20., 35.),
        });
        let _ = app.optimization_action(Action::Start);
        let key = app
            .tab
            .optimization
            .as_ref()
            .unwrap()
            .flight
            .as_ref()
            .unwrap()
            .key;
        let _ = app.optimization_edit(Edit::RelaxDragCancel { session: id });
        let session = app.tab.optimization.as_ref().unwrap();
        assert_eq!(session.conformer.as_ref().unwrap().positions, before);
        assert_eq!(session.energy, energy);
        assert!(session.drag.is_none());
        assert!(session.flight.is_none());
        let _ = app.optimization_action(Action::WorkerDone(key, Err("Cancelled drag".into())));
        assert_eq!(app.tab.doc, original_document);
        assert!(!app.error);
    }

    #[test]
    fn whole_molecule_generation_starts_unpinned_and_partial_selection_is_fixed() {
        let (mut whole, a, b, _) = drawing();
        whole.tab.selected = vec![a, b];
        let _ = whole.optimization_action(Action::Begin);
        finish(&mut whole);
        assert!(whole.tab.optimization.as_ref().unwrap().pins.is_empty());
        let (partial, a, _, _) = prepared_preview();
        let session = partial.tab.optimization.as_ref().unwrap();
        assert_eq!(session.pin_ids, vec![a]);
        let index = session.prepared.index(a).unwrap();
        assert_eq!(
            session.pins.get(&a),
            session.conformer.as_ref().unwrap().positions.get(index)
        );
    }

    #[test]
    fn topology_edits_are_blocked_before_they_can_modify_the_committed_drawing() {
        let (mut app, a, _, original) = prepared_preview();
        app.tab.selected = vec![a];
        let _ = app.update(Message::Charge(1));
        assert_eq!(app.tab.doc, original);
        assert!(app.tab.optimization.is_some());
        assert!(!app.tab.history.can_undo());
    }

    #[test]
    fn switching_and_closing_tabs_pause_or_discard_their_tagged_results() {
        let (mut app, _, _, original) = prepared_preview();
        let id = app.tab.id;
        let _ = app.optimization_action(Action::Start);
        let session = app.tab.optimization.as_ref().unwrap();
        let key = session.flight.as_ref().unwrap().key;
        let computed = Arc::new(result(session));
        app.add_tab();
        let front = app.tab.doc.clone();
        let _ = app.update(Message::Tab(
            id,
            Box::new(Message::Optimization(Action::WorkerDone(
                key,
                Ok(computed.clone()),
            ))),
        ));
        assert_eq!(app.tab.doc, front);
        let parked = app.tabs.background.iter().find(|tab| tab.id == id).unwrap();
        assert_eq!(parked.doc, original);
        assert!(parked.optimization.as_ref().unwrap().flight.is_none());
        let _ = app.update(Message::Tabs(super::super::tabs::Action::Select(id)));
        assert!(app.tab.optimization.is_some());
        let _ = app.close_active_tab();
        assert!(app.tab_index(id).is_none());
        let front = app.tab.doc.clone();
        let _ = app.update(Message::Tab(
            id,
            Box::new(Message::Optimization(Action::WorkerDone(key, Ok(computed)))),
        ));
        assert_eq!(app.tab.doc, front);
        assert!(app.strip().all(|tab| tab.optimization.is_none()));
    }

    #[test]
    fn improving_nonconverged_batches_stop_at_the_budget_and_apply_is_one_undoable_edit() {
        let (mut app, _, _, original) = prepared_preview();
        let _ = app.optimization_action(Action::ClearPins);
        let _ = app.optimization_action(Action::Start);
        let running_context = app
            .tab
            .optimization
            .as_ref()
            .unwrap()
            .accessibility_context();
        for batch in 1..=MAX_RELAXATION_BATCHES {
            let task = complete_live(&mut app, 1000. - f64::from(batch), false);
            let session = app.tab.optimization.as_ref().unwrap();
            assert_eq!(app.tab.doc, original);
            assert!(!app.tab.history.can_undo());
            if batch < MAX_RELAXATION_BATCHES {
                assert!(task.units() > 0);
                assert!(session.running);
                assert!(session.flight.is_some());
            } else {
                assert_eq!(task.units(), 0);
                assert!(!session.running);
                assert!(session.flight.is_none());
                assert_eq!(session.phase, Phase::Paused);
                assert_eq!(session.relaxation.paused, Some(RelaxationPause::Limit));
                assert_ne!(running_context, session.accessibility_context());
            }
        }
        assert!(app.status.contains("paused before convergence"));
        assert!(!app.error);
        let session = app.tab.optimization.as_mut().unwrap();
        assert_eq!(session.next().units(), 0);
        let preview = session.preview.clone();
        let _ = app.optimization_action(Action::Apply);
        assert_eq!(
            app.tab.optimization.as_ref().unwrap().phase,
            Phase::Applying
        );
        assert_eq!(app.tab.doc, original);
        finish(&mut app);
        assert!(app.tab.optimization.is_none());
        assert_eq!(app.tab.doc, preview);
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, original);
        assert!(!app.tab.history.can_undo());
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, preview);
    }

    #[test]
    fn stalled_nonconverged_batches_pause_after_multiple_batches_and_cancel_keeps_the_drawing() {
        let (mut app, _, _, original) = prepared_preview();
        let _ = app.optimization_action(Action::Start);
        assert!(complete_live(&mut app, 1., false).units() > 0);
        for _ in 1..MAX_STAGNANT_BATCHES {
            assert!(complete_live(&mut app, 1., false).units() > 0);
        }
        assert_eq!(complete_live(&mut app, 1., false).units(), 0);
        let session = app.tab.optimization.as_ref().unwrap();
        assert_eq!(session.phase, Phase::Paused);
        assert_eq!(session.relaxation.paused, Some(RelaxationPause::Stalled));
        assert_eq!(session.relaxation.batches, MAX_STAGNANT_BATCHES + 1);
        assert!(app.status.contains("no further energy improvement"));
        assert!(session.notice.is_none());
        assert!(!app.error);
        assert_eq!(app.tab.doc, original);
        let _ = app.optimization_action(Action::Cancel);
        assert!(app.tab.optimization.is_none());
        assert_eq!(app.tab.doc, original);
        assert!(!app.tab.history.can_undo());
    }

    #[test]
    fn small_cumulative_energy_improvements_keep_live_relaxation_active() {
        let (mut app, _, _, original) = prepared_preview();
        let _ = app.optimization_action(Action::Start);
        let _ = complete_live(&mut app, 1., false);
        for batch in 1..=2 * MAX_STAGNANT_BATCHES {
            // Each step is below the progress threshold, but two successive
            // steps make meaningful progress against the retained baseline.
            assert!(complete_live(&mut app, 1. - f64::from(batch) * 6e-7, false).units() > 0);
        }
        let session = app.tab.optimization.as_ref().unwrap();
        assert!(session.running);
        assert!(session.relaxation.paused.is_none());
        assert!(session.flight.is_some());
        let _ = app.optimization_action(Action::Stop);
        assert_eq!(app.tab.doc, original);
    }

    #[test]
    fn real_pin_and_field_changes_resume_a_paused_budget_while_noop_edits_and_rotation_do_not() {
        for action in [
            Action::PinSelected,
            Action::UnpinSelected,
            Action::ClearPins,
            Action::Field(ForceField::Uff),
        ] {
            let (mut app, _, b, original) = prepared_preview();
            let _ = app.optimization_action(Action::Start);
            stall_live(&mut app);
            assert_eq!(app.optimization_action(Action::Rotate(15., 25.)).units(), 0);
            assert_eq!(
                app.optimization_action(Action::Field(ForceField::Mmff94s))
                    .units(),
                0
            );
            assert_eq!(app.optimization_action(Action::PinSelected).units(), 0);
            let session = app.tab.optimization.as_ref().unwrap();
            assert!(!session.running);
            assert_eq!(session.relaxation.batches, MAX_STAGNANT_BATCHES + 1);
            if matches!(action, Action::PinSelected) {
                app.tab.selected = vec![b];
            }
            assert!(app.optimization_action(action).units() > 0);
            let session = app.tab.optimization.as_ref().unwrap();
            assert!(session.running);
            assert_eq!(session.phase, Phase::Running);
            assert_eq!(session.relaxation.batches, 1);
            assert_eq!(session.relaxation.stagnant_batches, 0);
            assert!(session.relaxation.best_energy.is_none());
            assert!(session.relaxation.paused.is_none());
            assert!(session.flight.is_some());
            assert_eq!(app.tab.doc, original);
        }
    }

    #[test]
    fn changed_drag_targets_resume_and_an_older_flight_does_not_consume_the_new_budget() {
        let (mut app, a, _, original) = prepared_preview();
        let _ = app.optimization_action(Action::ClearPins);
        let _ = app.optimization_action(Action::Start);
        stall_live(&mut app);
        let id = app.tab.optimization.as_ref().unwrap().id;
        let _ = app.optimization_edit(Edit::RelaxDragStart {
            session: id,
            atom: a,
        });
        let session = app.tab.optimization.as_ref().unwrap();
        let key = session.flight.as_ref().unwrap().key;
        let mut older = result(session);
        older.converged = false;
        let _ = app.optimization_edit(Edit::RelaxDragTarget {
            session: id,
            atom: a,
            target: Point::new(10., 15.),
        });
        let session = app.tab.optimization.as_ref().unwrap();
        assert_eq!(session.relaxation.batches, 0);
        assert_eq!(session.flight.as_ref().unwrap().key, key);
        let task = app.optimization_action(Action::WorkerDone(key, Ok(Arc::new(older))));
        assert!(task.units() > 0);
        let session = app.tab.optimization.as_ref().unwrap();
        assert_eq!(session.relaxation.batches, 1);
        assert_eq!(session.relaxation.stagnant_batches, 0);
        assert!(session.relaxation.best_energy.is_none());
        stall_live(&mut app);
        let task = app
            .optimization_edit(Edit::RelaxDragTarget {
                session: id,
                atom: a,
                target: Point::new(20., 15.),
            })
            .unwrap();
        assert!(task.units() > 0);
        assert_eq!(app.tab.optimization.as_ref().unwrap().relaxation.batches, 1);
        let _ = app.optimization_action(Action::Stop);
        let _ = app.optimization_edit(Edit::RelaxDragStart {
            session: id,
            atom: a,
        });
        let task = app
            .optimization_edit(Edit::RelaxDragTarget {
                session: id,
                atom: a,
                target: Point::new(25., 15.),
            })
            .unwrap();
        assert_eq!(task.units(), 0);
        let session = app.tab.optimization.as_ref().unwrap();
        assert!(!session.running);
        assert!(session.flight.is_none());
        assert_eq!(app.tab.doc, original);
    }

    #[test]
    fn stop_retains_visual_drag_targets_without_dispatch_and_start_uses_the_latest_target() {
        let (mut app, a, _, original) = prepared_preview();
        let _ = app.optimization_action(Action::ClearPins);
        let _ = app.optimization_action(Action::Start);
        assert!(app.tab.optimization.as_ref().unwrap().flight.is_some());
        let _ = app.optimization_action(Action::Stop);
        let id = app.tab.optimization.as_ref().unwrap().id;
        let before = app.tab.optimization.as_ref().unwrap().preview.clone();
        let _ = app.optimization_edit(Edit::RelaxDragStart {
            session: id,
            atom: a,
        });
        for x in [5., 15., 25.] {
            let task = app
                .optimization_edit(Edit::RelaxDragTarget {
                    session: id,
                    atom: a,
                    target: Point::new(x, 15.),
                })
                .unwrap();
            assert_eq!(task.units(), 0);
        }
        let task = app
            .optimization_edit(Edit::RelaxDragEnd {
                session: id,
                atom: a,
                target: Some(Point::new(25., 15.)),
            })
            .unwrap();
        assert_eq!(task.units(), 0);
        let session = app.tab.optimization.as_ref().unwrap();
        assert!(!session.running);
        assert!(session.flight.is_none());
        assert!(session.pending);
        assert_ne!(session.preview, before);
        assert_eq!(app.tab.doc, original);
        let target = session.drag.unwrap().target;
        let task = app.optimization_action(Action::Start);
        assert!(task.units() > 0);
        let session = app.tab.optimization.as_ref().unwrap();
        let flight = session.flight.as_ref().unwrap();
        assert_eq!(flight.drag, Some((a, target)));
        assert_eq!(flight.pins.len(), 1);
        assert_eq!(flight.pins[0].position, target);
    }

    #[test]
    fn a_converged_running_session_idles_without_polling_and_relaxes_the_next_drag() {
        let (mut app, a, _, _) = prepared_preview();
        let _ = app.optimization_action(Action::ClearPins);
        let _ = app.optimization_action(Action::Start);
        finish(&mut app);
        let session = app.tab.optimization.as_mut().unwrap();
        assert!(session.running);
        assert!(session.flight.is_none());
        assert_eq!(session.next().units(), 0);
        let id = session.id;
        let _ = app.optimization_edit(Edit::RelaxDragStart {
            session: id,
            atom: a,
        });
        let task = app
            .optimization_edit(Edit::RelaxDragTarget {
                session: id,
                atom: a,
                target: Point::new(10., 15.),
            })
            .unwrap();
        assert!(task.units() > 0);
        let session = app.tab.optimization.as_ref().unwrap();
        assert!(session.flight.is_some());
        assert_eq!(session.relaxation.batches, 1);
        assert_eq!(session.relaxation.stagnant_batches, 0);
    }

    #[test]
    fn switching_between_tilt_and_select_preserves_the_preview_and_session() {
        let (mut app, _, _, original) = prepared_preview();
        let id = app.tab.optimization.as_ref().unwrap().id;
        let _ = app.update(Message::Tool(Tool::Tilt));
        assert_eq!(app.tool, Tool::Tilt);
        let _ = app.update(Message::Canvas(Edit::RelaxRotate {
            session: id,
            x: 25.,
            y: 35.,
        }));
        let rotated = app.tab.optimization.as_ref().unwrap().preview.clone();
        let _ = app.update(Message::Tool(Tool::Select));
        assert_eq!(app.tool, Tool::Select);
        let session = app.tab.optimization.as_ref().unwrap();
        assert_eq!(session.id, id);
        assert_eq!(session.preview, rotated);
        assert_eq!(app.tab.doc, original);
        assert!(!app.tab.history.can_undo());
        let _ = app.update(Message::Escape);
        assert!(app.tab.optimization.is_none());
        assert_eq!(app.tab.doc, original);
    }

    #[test]
    fn accessibility_context_changes_for_controls_but_not_projection_or_energy_frames() {
        let (mut app, _, _, _) = prepared_preview();
        let before = app
            .tab
            .optimization
            .as_ref()
            .unwrap()
            .accessibility_context();
        let _ = app.optimization_action(Action::Rotate(25., 35.));
        let session = app.tab.optimization.as_mut().unwrap();
        session.energy = Some(99.);
        session.iterations += 20;
        assert_eq!(before, session.accessibility_context());
        for action in [
            Action::ShowOriginal(true),
            Action::DepthEnhancement(false),
            Action::Field(ForceField::Uff),
            Action::ClearPins,
            Action::Start,
            Action::Stop,
        ] {
            let before = app
                .tab
                .optimization
                .as_ref()
                .unwrap()
                .accessibility_context();
            let _ = app.optimization_action(action);
            assert_ne!(
                before,
                app.tab
                    .optimization
                    .as_ref()
                    .unwrap()
                    .accessibility_context()
            );
        }
        let before = app
            .tab
            .optimization
            .as_ref()
            .unwrap()
            .accessibility_context();
        let _ = app.optimization_action(Action::Cancel);
        let _ = app.optimization_action(Action::Begin);
        assert_ne!(
            before,
            app.tab
                .optimization
                .as_ref()
                .unwrap()
                .accessibility_context()
        );
    }

    struct PreviewUi {
        renderer: iced::Renderer,
        cache: iced_runtime::user_interface::Cache,
        size: iced::Size,
    }

    impl PreviewUi {
        async fn new(size: iced::Size) -> Self {
            use iced::advanced::renderer::Headless;
            Self {
                renderer: <iced::Renderer as Headless>::new(
                    iced::Font::with_name(reshiki::style::ui_font_family()),
                    iced::Pixels(16.),
                    None,
                )
                .await
                .unwrap(),
                cache: iced_runtime::user_interface::Cache::new(),
                size,
            }
        }

        fn operate<T: 'static>(
            &mut self,
            app: &App,
            operation: &mut dyn iced::advanced::widget::Operation<T>,
        ) {
            let mut ui = iced_runtime::UserInterface::build(
                app.view(),
                self.size,
                std::mem::take(&mut self.cache),
                &mut self.renderer,
            );
            ui.operate(
                &self.renderer,
                &mut iced::advanced::widget::operation::black_box(operation),
            );
            self.cache = ui.into_cache();
        }

        fn snapshot(&mut self, app: &App) -> reshiki::accessibility::Snapshot {
            let mut collect =
                reshiki::accessibility::Collect::new(iced::Rectangle::with_size(self.size));
            self.operate(app, &mut collect);
            let snapshot = collect.snapshot().clone();
            assert!(snapshot.duplicate_ids.is_empty());
            snapshot
        }

        fn focus(&mut self, app: &App, id: &str) {
            use iced::advanced::widget::{Operation, operation};
            let mut operation: Box<dyn Operation> =
                Box::new(reshiki::accessibility::FocusControl::new(id));
            loop {
                self.operate(app, operation.as_mut());
                match operation.finish() {
                    operation::Outcome::Chain(next) => operation = next,
                    _ => break,
                }
            }
        }

        fn event(
            &mut self,
            app: &App,
            event: iced::Event,
            cursor: iced::mouse::Cursor,
        ) -> (iced::event::Status, Vec<Message>) {
            let mut ui = iced_runtime::UserInterface::build(
                app.view(),
                self.size,
                std::mem::take(&mut self.cache),
                &mut self.renderer,
            );
            let mut messages = Vec::new();
            let (_, statuses) = ui.update(
                &[event],
                cursor,
                &mut self.renderer,
                &mut iced::advanced::clipboard::Null,
                &mut messages,
            );
            self.cache = ui.into_cache();
            (statuses[0], messages)
        }

        fn settle(&mut self, app: &mut App) {
            for _ in 0..3 {
                let (_, messages) = self.event(
                    app,
                    iced::Event::Window(iced::window::Event::RedrawRequested(
                        std::time::Instant::now(),
                    )),
                    iced::mouse::Cursor::Unavailable,
                );
                if messages.is_empty() {
                    break;
                }
                for message in messages {
                    let _ = app.update(message);
                }
            }
        }

        fn canvas_bounds(&self, app: &App) -> iced::Rectangle {
            use iced::advanced::{Layout, layout, widget::Tree};
            fn canvas(layout: Layout<'_>, size: iced::Size) -> Option<iced::Rectangle> {
                if layout.bounds().size() == size && layout.children().next().is_none() {
                    return Some(layout.bounds());
                }
                layout.children().find_map(|child| canvas(child, size))
            }
            let mut view = app.view();
            let mut tree = Tree::new(view.as_widget());
            let node = view.as_widget_mut().layout(
                &mut tree,
                &self.renderer,
                &layout::Limits::new(self.size, self.size),
            );
            canvas(Layout::new(&node), app.viewport).expect("actual canvas layout")
        }
    }

    fn preview_key(named: iced::keyboard::key::Named, repeat: bool) -> iced::Event {
        use iced::keyboard::{
            self, Key, Modifiers,
            key::{Code, Named, Physical},
        };
        iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key: Key::Named(named),
            modified_key: Key::Named(named),
            physical_key: Physical::Code(if named == Named::Tab {
                Code::Tab
            } else {
                Code::Enter
            }),
            location: keyboard::Location::Standard,
            modifiers: Modifiers::empty(),
            text: None,
            repeat,
        })
    }

    #[test]
    fn beginning_or_reopening_preview_reveals_properties_without_moving_the_molecule() {
        for (open, tab) in [
            (false, InspectorTab::Properties),
            (true, InspectorTab::Assistant),
            (true, InspectorTab::Templates),
        ] {
            let (mut app, a, _, original) = drawing();
            app.inspector_open = open;
            app.inspector_tab = tab;
            app.tab.camera.zoom = 1.5;
            app.viewport = iced::Size::new(1000. - app.inspector_width(), 600.);
            let at = app.tab.doc.atom(a).unwrap().position;
            let screen = app
                .tab
                .camera
                .screen(at, iced::Rectangle::with_size(app.viewport));
            let _ = app.optimization_action(Action::Begin);
            assert!(app.inspector_open);
            assert_eq!(app.inspector_tab, InspectorTab::Properties);
            assert_eq!(
                app.tab
                    .camera
                    .screen(at, iced::Rectangle::with_size(app.viewport)),
                screen
            );
            assert_eq!(app.tab.doc, original);
            let _ = app.update(Message::Inspector(InspectorTab::Templates));
            assert_eq!(app.inspector_tab, InspectorTab::Templates);
            let _ = app.update(Message::Inspector(InspectorTab::Properties));
            assert_eq!(app.inspector_tab, InspectorTab::Properties);
            assert!(app.tab.optimization.is_some());
        }
    }

    #[tokio::test]
    #[ignore = "Opt-in real renderer semantics and keyboard checks"]
    async fn preview_controls_publish_native_actions_and_tab_enter_dispatches_the_same_actions() {
        use iced::advanced::widget::{Operation, operation};
        use iced::keyboard::key::Named;
        use iced::{Rectangle, Size, mouse};
        use reshiki::accessibility::{
            Activate, Role,
            tree::{NativeTree, Request},
        };
        for size in [Size::new(1040., 680.), Size::new(1280., 820.)] {
            let (mut app, _, _, _) = prepared_preview();
            let mut ui = PreviewUi::new(size).await;
            ui.settle(&mut app);
            let snapshot = ui.snapshot(&app);
            let controls: Vec<_> = snapshot
                .nodes
                .iter()
                .filter(|node| node.id.starts_with("optimization."))
                .collect();
            assert_eq!(controls.len(), 17);
            for (id, checked) in [
                ("optimization.field.mmff94", false),
                ("optimization.field.mmff94s", true),
                ("optimization.field.uff", false),
                ("optimization.show-original", false),
                ("optimization.automatic-depth", true),
            ] {
                let node = controls.iter().find(|node| node.id == id).unwrap();
                assert_eq!(node.role, Role::ToggleButton);
                assert_eq!(node.checked, Some(checked));
            }
            let mut native = NativeTree::default();
            let tree = native
                .update(&snapshot, "ReShiki", Rectangle::with_size(size), 2.)
                .unwrap();
            ui.focus(&app, "optimization.field.mmff94");
            for (index, (id, expected)) in preview_controls(false).into_iter().enumerate() {
                if index > 0 {
                    let (status, messages) = ui.event(
                        &app,
                        preview_key(Named::Tab, false),
                        mouse::Cursor::Unavailable,
                    );
                    assert_eq!(status, iced::event::Status::Captured);
                    assert!(
                        !messages
                            .iter()
                            .any(|message| matches!(message, Message::Optimization(_)))
                    );
                }
                let current = ui.snapshot(&app);
                let focused: Vec<_> = current.nodes.iter().filter(|node| node.focused).collect();
                assert_eq!(focused.len(), 1);
                assert_eq!(focused[0].id, id);
                assert!(
                    focused[0].visible_bounds.is_some(),
                    "Tab reveals the scrolled control {id}"
                );
                let native_id = tree
                    .nodes
                    .iter()
                    .find(|(_, node)| node.author_id() == Some(id))
                    .unwrap()
                    .0;
                assert_eq!(
                    native.resolve(&accesskit::ActionRequest {
                        action: accesskit::Action::Click,
                        target_tree: accesskit::TreeId::ROOT,
                        target_node: native_id,
                        data: None,
                    }),
                    Some(Request::Activate(id.into()))
                );
                let mut activate = Activate::<Message>::new(id);
                ui.operate(&app, &mut activate);
                assert!(
                    matches!(activate.finish(), operation::Outcome::Some(Message::Optimization(actual)) if format!("{actual:?}") == format!("{expected:?}"))
                );
                for repeat in [false, true] {
                    let (status, messages) = ui.event(
                        &app,
                        preview_key(Named::Enter, repeat),
                        mouse::Cursor::Unavailable,
                    );
                    assert_eq!(status, iced::event::Status::Captured);
                    let actions: Vec<_> = messages
                        .iter()
                        .filter_map(|message| {
                            if let Message::Optimization(action) = message {
                                Some(format!("{action:?}"))
                            } else {
                                None
                            }
                        })
                        .collect();
                    if repeat {
                        assert!(actions.is_empty());
                    } else {
                        assert_eq!(actions, vec![format!("{expected:?}")]);
                    }
                }
            }
        }
        let (mut app, _, _, _) = drawing();
        let _ = app.optimization_action(Action::Begin);
        let mut ui = PreviewUi::new(Size::new(1040., 680.)).await;
        ui.settle(&mut app);
        let snapshot = ui.snapshot(&app);
        for id in [
            "optimization.run",
            "optimization.apply",
            "optimization.pin-selected",
            "optimization.rotate-left",
        ] {
            assert!(
                !snapshot
                    .nodes
                    .iter()
                    .find(|node| node.id == id)
                    .unwrap()
                    .enabled,
                "Unavailable until generation finishes: {id}"
            );
        }
        finish(&mut app);
        app.tab.optimization.as_mut().unwrap().phase = Phase::Applying;
        let snapshot = ui.snapshot(&app);
        assert_eq!(
            snapshot
                .nodes
                .iter()
                .filter(|node| node.id.starts_with("optimization.") && node.enabled)
                .map(|node| node.id.as_str())
                .collect::<Vec<_>>(),
            vec!["optimization.cancel"]
        );
        for (id, _) in preview_controls(false) {
            if id == "optimization.cancel" {
                continue;
            }
            let mut activate = Activate::<Message>::new(id);
            ui.operate(&app, &mut activate);
            assert!(matches!(activate.finish(), operation::Outcome::None));
        }
    }

    #[tokio::test]
    #[ignore = "Opt-in renderer right-panel layout, scrolling and pointer checks"]
    async fn properties_preview_controls_fit_scroll_and_keep_canvas_toolbar_height() {
        use iced::{Event, Size, mouse};
        for size in [
            Size::new(1040., 480.),
            Size::new(1040., 680.),
            Size::new(1280., 820.),
        ] {
            let (mut app, _, _, original) = drawing();
            app.inspector_open = true;
            app.inspector_tab = InspectorTab::Properties;
            let mut ui = PreviewUi::new(size).await;
            ui.settle(&mut app);
            let before = ui.canvas_bounds(&app);
            let _ = app.optimization_action(Action::Begin);
            finish(&mut app);
            ui.settle(&mut app);
            let after = ui.canvas_bounds(&app);
            assert_eq!(
                before, after,
                "Preview keeps the ordinary 46 px canvas toolbar and viewport"
            );
            assert_eq!(app.inspector_width(), 300.);
            let snapshot = ui.snapshot(&app);
            let controls: Vec<_> = snapshot
                .nodes
                .iter()
                .filter(|node| node.id.starts_with("optimization."))
                .collect();
            assert_eq!(controls.len(), 17);
            for control in controls {
                assert!(
                    control.bounds.x >= size.width - 300. - 0.5
                        && control.bounds.x + control.bounds.width <= size.width + 0.5,
                    "{control:?} must fit the 300 px Properties panel"
                );
            }
            for running in [false, true] {
                if running {
                    let _ = app.optimization_action(Action::Start);
                }
                for (id, expected) in preview_controls(running) {
                    ui.focus(&app, id);
                    let snapshot = ui.snapshot(&app);
                    for footer in ["optimization.cancel", "optimization.apply"] {
                        let node = snapshot
                            .nodes
                            .iter()
                            .find(|node| node.id == footer)
                            .unwrap();
                        let visible = node.visible_bounds.expect("fixed footer is always visible");
                        assert!(
                            (visible.height - node.bounds.height).abs() < 0.5
                                && (visible.width - node.bounds.width).abs() < 0.5
                        );
                    }
                    let node = snapshot.nodes.iter().find(|node| node.id == id).unwrap();
                    let visible = node
                        .visible_bounds
                        .expect("focus reveals the control through the real inspector scroller");
                    assert!(
                        (visible.height - node.bounds.height).abs() < 0.5
                            && (visible.width - node.bounds.width).abs() < 0.5,
                        "Fully reveal {id}: {node:?}"
                    );
                    let cursor = mouse::Cursor::Available(visible.center());
                    let mut actions = Vec::new();
                    for event in [mouse::Event::ButtonPressed, mouse::Event::ButtonReleased] {
                        let (_, messages) =
                            ui.event(&app, Event::Mouse(event(mouse::Button::Left)), cursor);
                        actions.extend(messages.into_iter().filter_map(|message| {
                            if let Message::Optimization(action) = message {
                                Some(format!("{action:?}"))
                            } else {
                                None
                            }
                        }));
                    }
                    assert_eq!(
                        actions,
                        vec![format!("{expected:?}")],
                        "Click the real right-panel control {id}"
                    );
                }
            }
            assert_eq!(app.tab.doc, original);
            assert!(!app.tab.history.can_undo());
            // The toolbar link restores the card after the inspector is hidden.
            app.inspector_open = false;
            ui.settle(&mut app);
            let snapshot = ui.snapshot(&app);
            let link = snapshot
                .nodes
                .iter()
                .find(|node| node.id == "context-3d-properties")
                .unwrap();
            let cursor = mouse::Cursor::Available(link.visible_bounds.unwrap().center());
            for event in [mouse::Event::ButtonPressed, mouse::Event::ButtonReleased] {
                let (_, messages) =
                    ui.event(&app, Event::Mouse(event(mouse::Button::Left)), cursor);
                for message in messages {
                    if matches!(message, Message::Inspector(InspectorTab::Properties)) {
                        let _ = app.update(message);
                    }
                }
            }
            assert!(app.inspector_open);
            assert_eq!(app.inspector_tab, InspectorTab::Properties);
            assert!(app.tab.optimization.is_some());
        }
    }
}
