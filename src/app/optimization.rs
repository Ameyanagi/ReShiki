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
mod tests;
