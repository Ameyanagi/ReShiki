//! Predict → xTB: the Calculate card, one running job per document and its finish.
//!
//! A run takes the molecule's 3D model (made first with MMFF when missing or
//! out of date), streams optimizer steps from a worker thread and moves a
//! preview of the model, never the document. At the end the result is
//! stored with the molecule and an optimized geometry is applied as one
//! Undo step, unless the stereo check finds a change: then the card asks.
// The browser editor shows only a note: runs, previews and results are desktop-only.
#![cfg_attr(target_arch = "wasm32", allow(dead_code, unused_variables))]
use super::{App, Message};
use iced::Task;
use reshiki::document::Document;
use reshiki::qcdata::{self, CalcResult, Frame};
#[cfg(any(test, not(target_arch = "wasm32")))]
use reshiki::science::isotope_masses;
use reshiki::science::{self, Freshness, Launch};
use results::RunInfo;
use state::{FailureDetails, Progress, Settings};
use std::rc::Rc;
use std::sync::{Arc, Weak};

mod conformers;
mod history;
pub(super) mod orbitals;
pub(super) use conformers::superposed;
pub(super) mod palette;
mod results;
#[cfg(not(target_arch = "wasm32"))]
mod run;
mod selection;
mod source;
pub(super) use source::Source;
mod state;
mod tone;
pub(super) mod ts;
pub(crate) mod vibrations;
mod view;

#[cfg(test)]
pub(super) use results::{Evaluation, View, headline, method_line, title as results_title};
#[cfg(test)]
pub(super) use state::Failure;
pub(crate) use state::Task as CalcTask;
pub(super) use state::formula;
pub(crate) use vibrations::Action as VibrationsAction;

/// The Predict tab's two pages.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Page {
    #[default]
    Nmr,
    Xtb,
}

#[derive(Clone, Debug)]
pub(crate) enum Action {
    Page(Page),
    Task(CalcTask),
    Method(qcdata::XtbMethod),
    Charge(String),
    Unpaired(String),
    Solvent(Option<&'static str>),
    HoldFixed(bool),
    Tight(bool),
    Threads(u32),
    Advanced,
    Accuracy(String),
    Temperature(String),
    Iterations(String),
    Xtb68(bool),
    Run,
    /// Start from a menu, the context row or the keyboard.
    Entry(Entry),
    /// A failed run again at the retry electronic temperature.
    Retry,
    /// A failed run again, unchanged.
    RunAgain,
    /// A failed SCF run again with the default iteration limit.
    ResetRetry,
    /// Copy a failed run's details.
    CopyDetails,
    Cancel,
    ApplyAnyway,
    Discard,
    Palette(palette::Action),
    Orbitals(orbitals::Action),
    /// The card's Conformers fields and the Conformers tab.
    Conformers(conformers::Action),
    Ts(ts::Action),
    Vibrations(vibrations::Action),
    #[cfg(not(target_arch = "wasm32"))]
    Event(u64, run::Event),
    #[cfg(not(target_arch = "wasm32"))]
    Search(u64, conformers::Event),
    #[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
    TsEvent(u64, ts::Event),
}

/// Ways to start a calculation without the card's Run button. Each shows
/// the card, sets what it names and runs with the card's other settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Entry {
    /// A task from the right-click Calculate (xTB) menu.
    Task(CalcTask),
    /// Optimize the geometry with GFN2-xTB.
    OptimizeGfn2,
    /// Calculate energy with a method that gives molecular orbitals.
    Orbitals,
    /// ⌘R: the last calculation again, on the selected molecule.
    Rerun,
}

/// ⌘R, as menus and tooltips show it.
pub(super) fn rerun_keys() -> String {
    super::shortcuts::label(&Message::Xtb(Action::Entry(Entry::Rerun))).unwrap_or_default()
}

/// Choose xTB before showing Predict so opening its settings does not start NMR.
pub(super) fn settings_messages() -> [Message; 2] {
    [
        Message::Xtb(Action::Page(Page::Xtb)),
        Message::Inspector(super::InspectorTab::Predict),
    ]
}

/// Whether this build can calculate; the entry points hide otherwise.
pub(super) fn available() -> bool {
    unavailable().is_none()
}

/// The window's card settings and page; the last used task stays chosen.
#[derive(Default)]
pub(crate) struct Window {
    pub(super) page: Page,
    pub(super) settings: Settings,
    /// The card's Conformers fields.
    pub(super) conformers: conformers::Settings,
    /// Document, file epoch and molecule for a pending inversion.
    conformer_inversion_target: Option<(u64, u64, Vec<u64>)>,
    /// Where the results palette sits, for every document of the window.
    pub(super) dock: palette::Dock,
}

/// One document's run and the last outcome the card shows.
#[derive(Default)]
pub(crate) struct State {
    pub(super) run: Option<Run>,
    pub(super) outcome: Option<Outcome>,
    selection_hint: Option<String>,
    serial: u64,
    pub(super) palette: palette::Palette,
    conformer_stereo: std::cell::RefCell<conformers::CardStereo>,
    ts_card: ts::card::State,
    ts_sessions: ts::Sessions,
    pub(in crate::app::xtb) ts_review: Option<ts::review::Dialog>,
    energy_state: ts::energy::State,
    pub(super) orbitals: orbitals::Orbitals,
    pub(super) vibrations: vibrations::State,
    /// What this session knows about stored results' runs, by result.
    infos: Vec<(Weak<CalcResult>, Arc<RunInfo>)>,
}

impl State {
    /// The run information of a result the document holds.
    fn info(&self, result: &Arc<CalcResult>) -> Option<&RunInfo> {
        self.infos
            .iter()
            .find(|(weak, _)| is(weak, result))
            .map(|(_, info)| info.as_ref())
    }

    fn remember(&mut self, result: &Arc<CalcResult>, info: Arc<RunInfo>) {
        self.infos.retain(|(weak, _)| weak.strong_count() > 0);
        self.infos.push((Arc::downgrade(result), info));
    }
}

/// Whether `weak` is the live allocation `result`: a live weak at the same
/// address is the same allocation.
fn is(weak: &Weak<CalcResult>, result: &Arc<CalcResult>) -> bool {
    weak.strong_count() > 0 && weak.as_ptr() == Arc::as_ptr(result)
}

/// What a run calculates and how: from the card, or a result's Recalculate.
#[derive(Clone)]
pub(crate) struct Request {
    atoms: Vec<u64>,
    config: state::Config,
    /// "Keep full results in this file" for the new result.
    keep_full: bool,
    /// Start here (Å, frame order) instead of the model: "Push along mode".
    /// Only a converged optimization from here replaces the model geometry.
    start: Option<Start>,
    /// The search, for a Conformers task.
    search: Option<conformers::Config>,
    ts: Option<ts::Request>,
}

/// A displaced input and the accepted geometry from which it was pushed.
#[derive(Clone)]
struct Start {
    positions: Vec<[f64; 3]>,
    from: Vec<[f64; 3]>,
}

pub(crate) struct Run {
    serial: u64,
    atoms: Vec<u64>,
    config: state::Config,
    keep_full: bool,
    start: Option<Start>,
    /// A conformer search's settings and progress.
    search: Option<conformers::Live>,
    ts: Option<ts::Live>,
    /// Absent while the 3D model is made first.
    launch: Option<Launch>,
    /// The 3D model generation this run asked for, once it asked.
    model: Option<super::models3d::Key>,
    started: iced::time::Instant,
    progress: Progress,
    /// The document state the run started from; edits stop the preview.
    revision: (u64, u64),
    preview: Option<Box<Document>>,
    label: String,
    #[cfg(not(target_arch = "wasm32"))]
    cancel: reshiki_xtb_worker::client::Cancel,
    handle: Option<iced::task::Handle>,
}

impl Drop for Run {
    /// Dropping a run (Cancel, a closed tab) stops its worker at once.
    fn drop(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        self.cancel.cancel();
    }
}

/// What the card shows after a run.
pub(crate) enum Outcome {
    Done {
        atoms: Vec<u64>,
        line: String,
        notes: Vec<String>,
    },
    Failed(Box<Failed>),
    /// Nothing applied yet: the stereo check found changes, or an edit was open.
    Review(Box<Review>),
}

/// A failed run and what Retry and Run again need.
pub(crate) struct Failed {
    pub failure: state::Failure,
    /// The molecule that failed, including errors before a run started.
    pub atoms: Vec<u64>,
    /// The run's request; `None` when no run had started.
    pub request: Option<Box<Request>>,
    /// The frame's atomic numbers, once the 3D model existed.
    pub numbers: Vec<u8>,
    pub seconds: f64,
    /// The document state when the run failed; Retry and Run again need
    /// it unchanged, so they calculate what failed.
    pub revision: (u64, u64),
}

impl Failed {
    fn ts(&self) -> Option<&ts::Request> {
        self.request.as_ref()?.ts.as_ref()
    }

    fn method(&self) -> qcdata::XtbMethod {
        self.request
            .as_ref()
            .map_or(qcdata::XtbMethod::Gfn2Xtb, |r| r.config.method)
    }

    fn scf_iteration_limit(&self) -> Option<u32> {
        let request = self
            .request
            .as_ref()
            .filter(|request| request.ts.is_none())?;
        request.config.max_iterations.filter(|&n| {
            matches!(self.failure.reason(), state::Failure::Scf(_))
                && n < state::DEFAULT_SCF_ITERATIONS
        })
    }

    /// The error card's sentence and its second line.
    fn explain(&self, retry: bool) -> (String, String) {
        let (title, detail) = self.failure.explain(self.method(), &self.numbers);
        if retry && let Some(n) = self.scf_iteration_limit() {
            return (
                format!(
                    "The electronic structure (SCF) did not converge within {n} iteration{}, the Max SCF iterations you set.",
                    if n == 1 { "" } else { "s" }
                ),
                detail,
            );
        }
        (title, detail)
    }

    /// Copy details: versions, what ran and why it stopped; no structure.
    pub(crate) fn details(&self) -> String {
        let mut parts = vec![format!("ReShiki {}", env!("CARGO_PKG_VERSION"))];
        #[cfg(not(target_arch = "wasm32"))]
        parts.push(format!("axtb {}", reshiki_xtb_worker::AXTB_REV));
        if let Some(request) = &self.request {
            let config = &request.config;
            let default = |value: Option<String>| value.unwrap_or_else(|| "default".into());
            parts.push(format!(
                "{} · {:?} · charge {} · unpaired electrons {} · solvent {} · conventions {} · \
                 accuracy {} · electronic temperature {} · max SCF iterations {} · {} threads · {}",
                config.method.label(),
                config.task,
                config.charge,
                config.unpaired,
                config.solvent.unwrap_or("none"),
                if config.xtb68 { "xtb 6.8" } else { "axtb" },
                default(config.accuracy.map(|x| x.to_string())),
                default(config.temperature_k.map(|k| format!("{k} K"))),
                default(config.max_iterations.map(|n| n.to_string())),
                config.threads,
                if config.tight { "tight" } else { "normal" },
            ));
            if !config.fixed.is_empty() {
                parts.push(format!("{} atoms held fixed", config.fixed.len()));
            }
        }
        if !self.numbers.is_empty() {
            parts.push(format!(
                "{}, {} atoms",
                state::formula(&self.numbers),
                self.numbers.len()
            ));
        }
        let reason = match self.failure.reason() {
            state::Failure::Scf(message)
            | state::Failure::Crashed {
                summary: message, ..
            }
            | state::Failure::Other(message) => message.clone(),
            _ => self.explain(false).0,
        };
        parts.push(format!("{reason} (after {:.1} s)", self.seconds));
        parts.join(" · ")
    }
}

pub(crate) struct Review {
    pending: Pending,
    reasons: Vec<String>,
    ts: Option<Rc<ts::Session>>,
}

/// A finished calculation, not yet stored.
struct Pending {
    /// Started away from the model ("Push along mode").
    pushed: bool,
    /// The accepted model geometry before a push, in frame order.
    accepted: Option<Vec<[f64; 3]>>,
    launch: Launch,
    frames: Vec<Frame>,
    result: CalcResult,
    line: String,
    notes: Vec<String>,
    info: RunInfo,
    attempt: Option<Attempt>,
}

struct Attempt {
    request: Box<Request>,
    seconds: f64,
    revision: (u64, u64),
}

/// A run's progress, which never closes menus or moves focus.
pub(super) fn is_progress(message: &Message) -> bool {
    #[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
    if matches!(message, Message::Xtb(Action::TsEvent(..))) {
        return true;
    }
    #[cfg(not(target_arch = "wasm32"))]
    return matches!(
        message,
        Message::Xtb(
            Action::Event(..) | Action::Search(..) | Action::Vibrations(vibrations::Action::Tick)
        )
    );
    #[cfg(target_arch = "wasm32")]
    false
}

/// Why this build cannot calculate, if it cannot.
pub(super) fn unavailable() -> Option<&'static str> {
    if cfg!(windows) {
        return Some("xTB calculations are coming to Windows once code signing is in place.");
    }
    #[cfg(target_arch = "wasm32")]
    return Some("xTB calculations run in the desktop app.");
    #[cfg(not(target_arch = "wasm32"))]
    (!reshiki_xtb_worker::INCLUDED).then_some("This build does not include the xTB engine.")
}

impl App {
    pub(super) fn xtb_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Run => return self.xtb_run(),
            Action::Entry(entry) => return self.xtb_entry(entry),
            Action::Retry | Action::RunAgain | Action::ResetRetry => {
                let Some(Outcome::Failed(failed)) = &self.tab.xtb.outcome else {
                    return Task::none();
                };
                let Some(request) = failed.request.as_deref() else {
                    return Task::none();
                };
                if !self.xtb_can_retry(failed) {
                    self.status = "The drawing changed: run it from the Calculate card".into();
                    self.error = false;
                    return Task::none();
                }
                let mut request = request.clone();
                if matches!(action, Action::Retry) {
                    if request.ts.is_some() {
                        return Task::none();
                    }
                    let Some(hotter) = request.config.hotter() else {
                        return Task::none();
                    };
                    request.config = hotter;
                }
                let reset = matches!(action, Action::ResetRetry);
                if reset && failed.scf_iteration_limit().is_none() {
                    return Task::none();
                }
                if !self.xtb_can_start() {
                    return Task::none();
                }
                if reset {
                    self.xtb.settings.iterations.clear();
                    request.config.max_iterations = None;
                }
                return self.xtb_begin(request);
            }
            Action::CopyDetails => {
                if let Some(Outcome::Failed(failed)) = &self.tab.xtb.outcome {
                    let details = failed.details();
                    self.status = "Copied the calculation details".into();
                    self.error = false;
                    return iced::clipboard::write(details);
                }
            }
            Action::Cancel => return self.xtb_cancel(),
            Action::ApplyAnyway => {
                self.xtb_apply_anyway();
                return self.xtb_reaction_energy_continue();
            }
            Action::Discard => {
                if let Some(Outcome::Review(_)) = self.tab.xtb.outcome.take() {
                    self.status = "Optimized geometry discarded".into();
                    self.error = false;
                }
                self.tab
                    .xtb
                    .energy_state
                    .stop("Reaction energy canceled: optimized geometry discarded");
            }
            #[cfg(not(target_arch = "wasm32"))]
            Action::Event(serial, event) => return self.xtb_event(serial, event),
            Action::Page(page) => self.xtb.page = page,
            Action::Palette(action) => {
                let opened = matches!(action, palette::Action::View(results::View::Vibrations));
                let was_visible = self.xtb_vibrations_visible();
                let task = self.xtb_palette(action);
                if opened {
                    return Task::batch([task, self.xtb_vibrations_opened(was_visible)]);
                }
                if was_visible == self.xtb_vibrations_visible() {
                    self.xtb_vibrations_sync();
                }
                return task;
            }
            Action::Vibrations(action) => return self.xtb_vibrations_action(action),
            Action::Conformers(action) => return self.xtb_conformers(action),
            Action::Ts(action) => return self.xtb_ts_action(action),
            #[cfg(not(target_arch = "wasm32"))]
            Action::Search(serial, event) => self.xtb_search_event(serial, event),
            #[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
            Action::TsEvent(serial, event) => self.xtb_ts_event(serial, event),
            Action::Orbitals(action) => return self.xtb_orbitals(action),
            setting => {
                let target = self.xtb_key();
                let settings = &mut self.xtb.settings;
                match setting {
                    Action::Task(task) => settings.task = task,
                    Action::Charge(value) => {
                        settings.retarget(target);
                        settings.charge = value;
                    }
                    Action::Unpaired(value) => {
                        settings.retarget(target);
                        settings.unpaired = value;
                    }
                    Action::Solvent(solvent) => settings.solvent = solvent,
                    Action::HoldFixed(hold) => settings.hold_fixed = hold,
                    Action::Tight(tight) => settings.tight = tight,
                    Action::Threads(threads) => settings.threads = threads,
                    Action::Advanced => settings.advanced = !settings.advanced,
                    Action::Method(method) => settings.method = method,
                    Action::Accuracy(value) => settings.accuracy = value,
                    Action::Temperature(value) => settings.temperature = value,
                    Action::Iterations(value) => settings.iterations = value,
                    Action::Xtb68(on) => settings.xtb68 = on,
                    _ => {}
                }
            }
        }
        Task::none()
    }

    /// Who typed charge and unpaired electrons belong to: this document's target.
    pub(super) fn xtb_key(&self) -> Option<[u64; 3]> {
        let atom = *self.xtb_target()?.first()?;
        Some([self.tab.id.0, self.tab.file_epoch, atom])
    }

    /// The molecule the card calculates: the selected one, or the only one.
    pub(super) fn xtb_target(&self) -> Option<Vec<u64>> {
        let doc = &self.tab.doc;
        if let Some(atoms) = reshiki::models3d::selected_molecule(doc, &self.tab.selected) {
            return Some(atoms);
        }
        let first = doc.atoms.first()?.id;
        let atoms = reshiki::molecule_names::component(doc, first);
        (self.tab.selected.is_empty() && atoms.len() == doc.atoms.len()).then_some(atoms)
    }

    fn xtb_inverts_for(&self, atoms: &[u64]) -> bool {
        self.xtb
            .conformer_inversion_target
            .as_ref()
            .is_some_and(|(tab, epoch, target)| {
                (*tab, *epoch) == (self.tab.id.0, self.tab.file_epoch) && target.as_slice() == atoms
            })
    }

    pub(super) fn xtb_sync_conformer_inversion(&mut self) {
        if self.xtb.conformer_inversion_target.is_some()
            && !self
                .xtb_target()
                .is_some_and(|atoms| self.xtb_inverts_for(&atoms))
        {
            self.xtb.conformer_inversion_target = None;
            self.xtb.conformers.invert_undefined_stereo = false;
        }
    }

    /// The last outcome, if it describes the card's molecule.
    fn xtb_outcome(&self) -> Option<&Outcome> {
        let outcome = self.tab.xtb.outcome.as_ref()?;
        if self.xtb_selected_reaction().is_some()
            || matches!(outcome, Outcome::Failed(failed) if failed.ts().is_some())
        {
            return None;
        }
        let Some(target) = self.xtb_target() else {
            return (!matches!(outcome, Outcome::Failed(_)) || self.xtb_failed().is_some())
                .then_some(outcome);
        };
        let intersects = |atoms: &[u64]| atoms.iter().any(|id| target.contains(id));
        let belongs = match outcome {
            Outcome::Done { atoms, .. } => intersects(atoms),
            Outcome::Failed(failed) => intersects(&failed.atoms),
            Outcome::Review(review) => review
                .pending
                .launch
                .atoms
                .iter()
                .flatten()
                .any(|atom| target.contains(atom)),
        };
        belongs.then_some(outcome)
    }

    /// The molecule failure that belongs with the result on display.
    fn xtb_failed(&self) -> Option<&Failed> {
        let Outcome::Failed(failed) = self.tab.xtb.outcome.as_ref()? else {
            return None;
        };
        if failed.ts().is_some() || self.xtb_selected_reaction().is_some() {
            return None;
        }
        // The targeted molecule decides; with none, any shown result must
        // be this molecule's own.
        let belongs = match (self.xtb_target(), self.xtb_shown_entry()) {
            (Some(target), _) => target.iter().any(|atom| failed.atoms.contains(atom)),
            (None, Some((_, stored))) => {
                !ts::is_ts(&stored.result)
                    && self
                        .tab
                        .doc
                        .science
                        .attachment(stored.attachment)?
                        .atoms
                        .iter()
                        .flatten()
                        .any(|atom| failed.atoms.contains(atom))
            }
            (None, None) => true,
        };
        belongs.then_some(failed)
    }

    /// The current 3D model of the molecule of `atom`, by index.
    fn xtb_model(&self, atom: u64) -> Option<usize> {
        let index = self
            .tab
            .doc
            .models
            .iter()
            .position(|m| m.linked_atoms.contains(&atom))?;
        (!self.model_stale(index)).then_some(index)
    }

    /// The model preview while a run is current, else a previewed step,
    /// else an animated vibration.
    pub(super) fn xtb_preview(&self) -> Option<&Document> {
        let Some(run) = self.tab.xtb.run.as_ref() else {
            return self
                .xtb_step_preview()
                .or_else(|| self.xtb_vibration_preview())
                .or_else(|| self.xtb_orbital_preview());
        };
        (run.revision == (self.tab.file_epoch, self.tab.revision))
            .then_some(run.preview.as_deref())
            .flatten()
    }

    /// The canvas outline and badge around the molecule being calculated.
    pub(super) fn xtb_outline(&self) -> Option<crate::canvas::ModelGeneration<'_>> {
        let run = self
            .tab
            .xtb
            .run
            .as_ref()
            .filter(|run| run.launch.is_some())?;
        Some(crate::canvas::ModelGeneration {
            atoms: &run.atoms,
            started: run.started,
            label: "Calculating xTB",
        })
    }

    pub(super) fn xtb_running(&self) -> bool {
        self.tab.xtb.run.is_some()
    }

    /// Whether a run can start now; if not, the status says why.
    fn xtb_can_start(&mut self) -> bool {
        // A build without the engine shows no Run button; its worker would
        // answer "not included" anyway.
        if self.tab.xtb.run.is_some() {
            return false;
        }
        if self.tab.xtb.energy_state.in_progress() {
            self.status = "Finish or cancel the reaction energy calculation first".into();
            self.error = false;
            return false;
        }
        if self.tab.busy
            || self.tab.optimization.is_some()
            || self.tab.model_job.is_some()
            || self.tab.drawing_edit_active()
        {
            self.status = "Finish the current operation, then run xTB".into();
            self.error = false;
            return false;
        }
        true
    }

    /// Whether Retry and Run again can repeat `failed` as it ran.
    pub(super) fn xtb_can_retry(&self, failed: &Failed) -> bool {
        failed.request.is_some()
            && self.tab.xtb.run.is_none()
            && failed.revision == (self.tab.file_epoch, self.tab.revision)
    }

    fn xtb_entry(&mut self, entry: Entry) -> Task<Message> {
        if !available() || !self.xtb_can_start() {
            return Task::none();
        }
        if self.xtb_target().is_none() {
            self.xtb_show_selection_hint();
            return Task::none();
        }
        match entry {
            Entry::Task(task) => self.xtb.settings.task = task,
            Entry::OptimizeGfn2 => {
                self.xtb.settings.task = CalcTask::Optimize;
                self.xtb.settings.method = qcdata::XtbMethod::Gfn2Xtb;
            }
            Entry::Orbitals => {
                self.xtb.settings.task = CalcTask::Energy;
                if !self.xtb.settings.method.capabilities().orbitals {
                    self.xtb.settings.method = qcdata::XtbMethod::Gfn2Xtb;
                }
            }
            Entry::Rerun => {}
        }
        self.xtb.page = Page::Xtb;
        let _ = self.show_inspector_tab(super::InspectorTab::Predict);
        self.xtb_run()
    }

    fn xtb_run(&mut self) -> Task<Message> {
        if !self.xtb_can_start() {
            return Task::none();
        }
        self.xtb_sync_conformer_inversion();
        let Some((atoms, first)) = self
            .xtb_target()
            .and_then(|atoms| Some((atoms.clone(), *atoms.first()?)))
        else {
            self.xtb_show_selection_hint();
            return Task::none();
        };
        let settings = &conformers::effective(&self.xtb.settings, &self.xtb.conformers);
        let (charge, multiplicity) = science::drawn_state(&self.tab.doc, first);
        let drawn = (charge, multiplicity.saturating_sub(1));
        let typed = settings.molecule.is_some() && settings.molecule == self.xtb_key();
        let selected: std::collections::HashSet<u64> = self.tab.selected.iter().copied().collect();
        let fixed = if settings.hold_fixed && settings.task == CalcTask::Optimize {
            atoms
                .iter()
                .copied()
                .filter(|id| selected.contains(id))
                .collect()
        } else {
            vec![]
        };
        let config = match settings.config(drawn, typed, fixed) {
            Ok(config) => config,
            Err(reason) => {
                self.xtb_fail(state::Failure::Other(reason.into()));
                return Task::none();
            }
        };
        let search = match config.task {
            CalcTask::Conformers => match self.xtb.conformers.config(None) {
                Ok(search) => Some(search),
                Err(reason) => {
                    self.xtb_fail(state::Failure::Other(reason));
                    return Task::none();
                }
            },
            _ => None,
        };
        let request = Request {
            atoms,
            config,
            keep_full: false,
            start: None,
            search,
            ts: None,
        };
        self.xtb_begin(request)
    }

    /// Start `request`; its run replaces the card's last outcome.
    fn xtb_begin(&mut self, request: Request) -> Task<Message> {
        let limit = request.config.task.limit();
        if request.atoms.len() > limit {
            self.xtb_fail(state::Failure::TooMany {
                count: request.atoms.len(),
                limit,
            });
            return Task::none();
        }
        if request.search.is_some() {
            self.xtb.conformer_inversion_target = None;
            self.xtb.conformers.invert_undefined_stereo = false;
        }
        self.xtb_end_preview();
        self.tab.xtb.serial = self.tab.xtb.serial.wrapping_add(1);
        self.tab.xtb.outcome = None;
        self.tab.xtb.run = Some(Run {
            serial: self.tab.xtb.serial,
            atoms: request.atoms,
            search: request.search.map(|config| conformers::Live {
                method: request.config.method,
                config,
                phase: Default::default(),
                stopping: false,
            }),
            ts: request.ts.map(ts::Live::new),
            config: request.config,
            keep_full: request.keep_full,
            start: request.start,
            launch: None,
            model: None,
            started: iced::time::Instant::now(),
            progress: Progress::default(),
            revision: (self.tab.file_epoch, self.tab.revision),
            preview: None,
            label: String::new(),
            #[cfg(not(target_arch = "wasm32"))]
            cancel: Default::default(),
            handle: None,
        });
        self.xtb_vibrations_sync();
        self.xtb_launch()
    }

    /// Start the worker, or make the 3D model first.
    fn xtb_launch(&mut self) -> Task<Message> {
        let Some(run) = &self.tab.xtb.run else {
            return Task::none();
        };
        let Some(&atom) = run.atoms.first() else {
            return Task::none();
        };
        match science::launch(
            &self.tab.doc,
            atom,
            run.config.charge,
            run.config.unpaired.saturating_add(1),
        ) {
            Ok(launch) => self.xtb_start(launch),
            Err(science::ScienceError::NoModel | science::ScienceError::StaleModel)
                if run.model.is_none() =>
            {
                let task = self.model_for_calculation(run.atoms.clone());
                let Some(key) = self.tab.model_job.as_ref().map(|job| job.key) else {
                    self.xtb_fail(state::Failure::Other(format!(
                        "xTB not started · {}",
                        self.status
                    )));
                    return Task::none();
                };
                if let Some(run) = &mut self.tab.xtb.run {
                    run.model = Some(key);
                    run.started = iced::time::Instant::now();
                    self.status = run.status();
                }
                self.error = false;
                task
            }
            Err(error @ (science::ScienceError::NoModel | science::ScienceError::StaleModel)) => {
                self.xtb_fail(state::Failure::Other(format!("xTB not started: {error}")));
                Task::none()
            }
            Err(error) => {
                self.xtb_fail(state::Failure::Other(error.to_string()));
                Task::none()
            }
        }
    }

    /// The model generation `key` ended, in any way (done, failed, dropped,
    /// canceled or replaced): a run waiting for it starts or stops.
    pub(super) fn xtb_model_finished(&mut self, key: super::models3d::Key) -> Task<Message> {
        let waiting = self
            .tab
            .xtb
            .run
            .as_ref()
            .is_some_and(|run| run.launch.is_none() && run.model == Some(key));
        if waiting {
            return self.xtb_launch();
        }
        Task::none()
    }

    #[cfg(target_arch = "wasm32")]
    fn xtb_start(&mut self, _: Launch) -> Task<Message> {
        self.xtb_fail(state::Failure::Other(
            "xTB calculations run in the desktop app.".into(),
        ));
        Task::none()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn xtb_start(&mut self, mut launch: Launch) -> Task<Message> {
        let (epoch, revision) = (self.tab.file_epoch, self.tab.revision);
        let Some(run) = &mut self.tab.xtb.run else {
            return Task::none();
        };
        if run.search.is_some() {
            return self.xtb_search_start(launch);
        }
        let config = &run.config;
        let count = launch.frame.numbers.len();
        let limit = config.task.limit();
        if count > limit {
            self.xtb_fail(state::Failure::TooMany { count, limit });
            return Task::none();
        }
        #[cfg(feature = "xtb")]
        if run.ts.is_some() {
            return self.xtb_ts_start(launch);
        }
        if let Some(start) = &run.start {
            if start.positions.len() != count
                || start.from.len() != count
                || !model_shows(&self.tab.doc, &launch.atoms, &start.from)
            {
                self.xtb_fail(state::Failure::Other(
                    "The 3D model moved after this calculation".into(),
                ));
                return Task::none();
            }
            launch.frame = Arc::new(Frame {
                positions_angstrom: start.positions.clone(),
                ..(*launch.frame).clone()
            });
        }
        // The vibrational analysis needs the masses of isotope-labelled atoms;
        // the stored frame keeps them, so a recompute uses the same ones.
        if config.task.frequencies()
            && launch.frame.masses_amu.is_empty()
            && let Some(masses) = isotope_masses(&launch.frame)
        {
            launch.frame = Arc::new(Frame {
                masses_amu: masses,
                ..(*launch.frame).clone()
            });
        }
        let fixed = fixed_indices(&launch.atoms, &config.fixed).collect();
        let job = job(config, &launch.frame, fixed);
        run.label = format!("xTB · {}", state::formula(&launch.frame.numbers));
        run.launch = Some(launch);
        run.revision = (epoch, revision);
        run.started = iced::time::Instant::now();
        let serial = run.serial;
        // The thread starts when the task first runs, never in a test that
        // drops the task.
        let cancel = run.cancel.clone();
        let events = iced::futures::StreamExt::flatten(iced::futures::stream::once(async move {
            run::start(job, cancel)
        }));
        let (task, handle) = Task::run(events, move |event| {
            Message::Xtb(Action::Event(serial, event))
        })
        .abortable();
        run.handle = Some(handle.abort_on_drop());
        self.status = run.status();
        self.error = false;
        task
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn xtb_event(&mut self, serial: u64, event: run::Event) -> Task<Message> {
        let current = (self.tab.file_epoch, self.tab.revision);
        let reaction_energy = self.tab.xtb.energy_state.running(serial);
        let Some(run) = self.tab.xtb.run.as_mut().filter(|run| run.serial == serial) else {
            return Task::none();
        };
        match event {
            run::Event::Tick | run::Event::Hessian => {
                run.progress.hessian |= matches!(event, run::Event::Hessian);
                self.status = run.status();
                self.error = false;
            }
            run::Event::Steps {
                first,
                energies,
                positions,
            } => {
                run.progress.push(first, &energies);
                run.preview = (run.revision == current)
                    .then_some(run.launch.as_ref())
                    .flatten()
                    .and_then(|launch| launch.atoms.iter().flatten().next().copied())
                    .and_then(|atom| preview(&self.tab.doc, atom, &positions))
                    .map(Box::new);
                self.status = run.status();
                self.error = false;
            }
            run::Event::Done(result) => match result.and_then(|report| pending(run, &report)) {
                Ok(pending) => {
                    self.tab.xtb.run = None;
                    self.xtb_store(pending, false);
                    if reaction_energy {
                        return self.xtb_reaction_energy_continue();
                    }
                }
                Err(failure) => self.xtb_fail(failure),
            },
        }
        if reaction_energy && let Some(progress) = self.tab.xtb.energy_state.progress() {
            self.status = format!("{progress} · {}", self.status);
        }
        Task::none()
    }

    /// End the run in flight, if any, with `failure`. The card and the
    /// results palette show it, keeping the request for Retry and Run again.
    fn xtb_fail(&mut self, failure: state::Failure) {
        self.tab
            .xtb
            .energy_state
            .stop("Reaction energy stopped: a species calculation failed");
        let run = self.tab.xtb.run.take();
        let atoms = run.as_ref().map_or_else(
            || self.xtb_target().unwrap_or_default(),
            |run| run.atoms.clone(),
        );
        let numbers = run
            .as_ref()
            .and_then(|run| run.launch.as_ref())
            .map(|launch| launch.frame.numbers.clone())
            .unwrap_or_default();
        let attempt = run.as_ref().map(Run::attempt);
        self.xtb_record_failure(failure, attempt, atoms, numbers);
    }

    fn xtb_record_failure(
        &mut self,
        failure: state::Failure,
        attempt: Option<Attempt>,
        atoms: Vec<u64>,
        numbers: Vec<u8>,
    ) {
        let mut failed = Failed {
            failure,
            atoms,
            request: None,
            numbers,
            seconds: 0.,
            revision: (self.tab.file_epoch, self.tab.revision),
        };
        if let Some(attempt) = attempt {
            failed.request = Some(attempt.request);
            failed.seconds = attempt.seconds;
            // An edit during the run retires Retry.
            failed.revision = attempt.revision;
        }
        self.status = format!(
            "xTB calculation failed: {}",
            failed.explain(self.xtb_can_retry(&failed)).0
        );
        self.error = true;
        if failed
            .ts()
            .is_none_or(|ts| self.xtb_selected_reaction() == Some(ts.arrow))
        {
            self.tab.xtb.palette.open = true;
            self.tab.xtb.palette.previous = false;
            // The card's actions must show, so a collapsed dock opens.
            self.tab.xtb.palette.collapsed = false;
        }
        self.tab.xtb.outcome = Some(Outcome::Failed(Box::new(failed)));
    }

    /// Cancel stops the worker (or the model generation) and applies nothing.
    pub(super) fn xtb_cancel(&mut self) -> Task<Message> {
        if self.xtb_search_stop() {
            return Task::none();
        }
        let reaction_energy = self.tab.xtb.energy_state.stop("Reaction energy canceled");
        let Some(run) = self.tab.xtb.run.take() else {
            if reaction_energy {
                self.status = "Reaction energy canceled".into();
                self.error = false;
            }
            return Task::none();
        };
        if run.launch.is_none() {
            self.tab.model_job.take_if(|job| Some(job.key) == run.model);
        }
        self.status = if reaction_energy {
            "Reaction energy canceled"
        } else {
            "Calculation canceled"
        }
        .into();
        self.error = false;
        if run.ts.is_some() {
            return super::focus_drawing();
        }
        Task::none()
    }

    fn xtb_apply_anyway(&mut self) {
        if self.tab.drawing_edit_active() {
            self.status = "Finish the current edit, then apply".into();
            self.error = false;
            return;
        }
        if let Some(Outcome::Review(review)) = self.tab.xtb.outcome.take() {
            self.xtb_store_with_session(review.pending, true, review.ts);
        }
    }

    /// Store the result with its molecule and apply an optimized geometry,
    /// as one Undo step. Unless `forced`, a stereo change or an open edit
    /// stops at the review card instead.
    fn xtb_store(&mut self, pending: Pending, forced: bool) {
        self.xtb_store_with_session(pending, forced, None);
    }

    fn xtb_store_with_session(
        &mut self,
        pending: Pending,
        forced: bool,
        ts: Option<Rc<ts::Session>>,
    ) {
        let mut doc = self.tab.doc.clone();
        let attached = match science::attach(
            &mut doc,
            &pending.launch,
            pending.frames.clone(),
            pending.result.clone(),
        ) {
            Ok(attached) => attached,
            Err(error) => {
                self.xtb_store_failed(pending, error.to_string());
                return;
            }
        };
        let optimized = matches!(
            pending.result.task,
            qcdata::Task::Optimize | qcdata::Task::Conformers
        );
        let mut notes = pending.notes.clone();
        let moved = pending.accepted.as_ref().map_or_else(
            || !shows(&self.tab.doc, &pending.launch),
            |accepted| !model_shows(&self.tab.doc, &pending.launch.atoms, accepted),
        );
        if attached.freshness == Freshness::Stale {
            notes.push(
                "The molecule changed during the calculation: the result is kept, the 3D model is unchanged."
                    .into(),
            );
        } else if moved {
            notes.push(
                "The 3D model changed during the calculation: the result is kept, the model is not moved."
                    .into(),
            );
        } else if !forced {
            let mut reasons = Vec::new();
            if optimized && let Some(frame) = pending.frames.first() {
                reasons = match science::stereo_changes(&self.tab.doc, &pending.launch.atoms, frame)
                {
                    Ok(changes) => changes
                        .iter()
                        .map(|change| {
                            state::describe(change, |id| {
                                self.xtb_atom_name(&pending.launch.atoms, id)
                            })
                        })
                        .collect(),
                    Err(error) => vec![format!("The stereo check could not run: {error}.")],
                };
            }
            if self.tab.drawing_edit_active() {
                reasons.push("An edit was open when the calculation finished.".into());
            }
            if let Some(first) = reasons.first() {
                self.status = format!("{} · nothing applied", first.trim_end_matches('.'));
                self.error = false;
                self.tab.xtb.outcome = Some(Outcome::Review(Box::new(Review {
                    pending,
                    reasons,
                    ts,
                })));
                return;
            }
        }
        // A push that did not converge never replaces the accepted structure.
        let unconverged_push =
            pending.pushed && !matches!(pending.result.status, qcdata::Status::Converged);
        if unconverged_push {
            notes.push(
                "The re-optimization did not converge: the accepted structure is unchanged.".into(),
            );
        }
        let applied = optimized
            && !unconverged_push
            && !moved
            && attached.freshness == Freshness::Current
            && match science::apply_geometry(&mut doc, attached.attachment, &pending.result.frame) {
                Ok(()) => true,
                Err(error) => {
                    notes.push(format!("The geometry was not applied: {error}."));
                    false
                }
            };
        let before = std::mem::replace(&mut self.tab.doc, doc);
        self.changed(before);
        if self.error {
            self.xtb_store_failed(pending, self.status.clone());
            return;
        }
        if let Some(result) = self
            .tab
            .doc
            .science
            .results
            .last()
            .map(|s| s.result.clone())
        {
            self.tab.xtb.remember(&result, Arc::new(pending.info));
            if let Some(ts) = ts {
                self.tab.xtb.ts_sessions.remember(&result, ts);
                if self.tabs.temporary_depth == 0 {
                    self.tab.xtb.palette.collapsed = false;
                    if self.xtb.dock.inspector {
                        self.xtb.page = Page::Xtb;
                        let _ = self.show_inspector_tab(super::InspectorTab::Predict);
                    }
                }
            }
            self.xtb_show_result(&result);
            // After a push the question was the imaginary mode: show its answer.
            if pending.pushed && applied {
                self.tab.xtb.palette.view = results::View::Vibrations;
            }
        }
        self.status = if applied {
            format!("Applied xTB geometry · {} · ⌘Z undoes", pending.line)
        } else {
            format!("xTB · {}", pending.line)
        };
        self.tab.xtb.outcome = Some(Outcome::Done {
            atoms: pending.launch.atoms.into_iter().flatten().collect(),
            line: pending.line,
            notes,
        });
    }

    fn xtb_store_failed(&mut self, mut pending: Pending, reason: String) {
        let failure = state::Failure::Other(reason);
        if let Some(attempt) = pending.attempt.take() {
            let atoms = pending.launch.atoms.into_iter().flatten().collect();
            self.xtb_record_failure(
                failure,
                Some(attempt),
                atoms,
                pending.launch.frame.numbers.clone(),
            );
        } else {
            self.xtb_fail(failure);
        }
    }

    /// "C3" for the third frame atom, and the drawing's CIP code there.
    fn xtb_atom_name(&self, atoms: &[Option<u64>], id: u64) -> (String, Option<String>) {
        let index = atoms.iter().position(|a| *a == Some(id));
        let atom = self.tab.doc.atom(id);
        let element = atom.map_or("?", |a| a.element.as_str());
        let label = index.map_or_else(|| element.to_owned(), |i| format!("{element}{}", i + 1));
        (label, atom.and_then(|a| a.cip_label.clone()))
    }
}

impl Run {
    fn attempt(&self) -> Attempt {
        Attempt {
            request: Box::new(Request {
                atoms: self.atoms.clone(),
                config: self.config.clone(),
                keep_full: self.keep_full,
                start: self.start.clone(),
                search: self.search.as_ref().map(|live| live.config.clone()),
                ts: self.ts.as_ref().map(|live| live.request.clone()),
            }),
            seconds: self.seconds(),
            revision: self.revision,
        }
    }

    fn seconds(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }

    fn status(&self) -> String {
        if let Some(live) = &self.ts {
            return format!("xTB · {}", live.line());
        }
        if self.launch.is_none() {
            return "xTB · making the 3D model first (MMFF94s)".into();
        }
        if let Some(live) = &self.search {
            return format!("{} · {}", self.label, live.line(self.seconds()));
        }
        format!(
            "{} · {}",
            self.label,
            self.progress.line(self.config.task, self.seconds())
        )
    }
}

/// The worker job for `config` on `frame`, holding frame atoms `fixed`.
#[cfg(not(target_arch = "wasm32"))]
fn job(config: &state::Config, frame: &Frame, fixed: Vec<usize>) -> run::Job {
    use reshiki_xtb_jobs::{method, run::worker_method};
    use reshiki_xtb_worker::protocol::{Conventions, ScfSettings, SolventModel};

    let isotope_labels = if frame.masses_amu.is_empty() {
        Vec::new()
    } else {
        frame
            .numbers
            .iter()
            .enumerate()
            .map(|(index, &number)| {
                u32::from(number) * 1000
                    + u32::from(frame.isotopes.get(index).copied().unwrap_or(0))
            })
            .collect()
    };
    run::Job {
        optimize: config.task.optimizes(),
        frequencies: config.task.frequencies(),
        masses: (!frame.masses_amu.is_empty()).then(|| frame.masses_amu.clone()),
        isotope_labels,
        numbers: frame.numbers.clone(),
        positions: frame.positions_angstrom.clone(),
        charge: config.charge,
        uhf: config.unpaired,
        method: worker_method(config.method),
        conventions: if config.xtb68 {
            Conventions::Xtb68
        } else {
            Conventions::Axtb
        },
        scf: ScfSettings {
            accuracy: config.accuracy,
            electronic_temperature_k: config.temperature_k,
            max_iterations: config.max_iterations,
            retries: vec![],
        },
        solvent: config
            .solvent
            .map(|name| method::solvent(SolventModel::Alpb, name)),
        threads: config.threads,
        fixed,
        tight: config.tight,
        topology: None,
        max_steps: None,
        properties: None,
        thermo: Default::default(),
    }
}

/// Whether the molecule's model still has the geometry `launch` started from.
fn shows(doc: &Document, launch: &Launch) -> bool {
    model_shows(doc, &launch.atoms, &launch.frame.positions_angstrom)
}

/// Whether the 3D model of the frame atoms `atoms` is at `positions` (Å).
fn model_shows(doc: &Document, atoms: &[Option<u64>], positions: &[[f64; 3]]) -> bool {
    let Some(atom) = atoms.iter().flatten().next() else {
        return false;
    };
    doc.models
        .iter()
        .find(|m| m.linked_atoms.contains(atom))
        .is_some_and(|model| model_matches(model, atoms, positions))
}

fn model_matches(
    model: &reshiki::models3d::Model3d,
    atoms: &[Option<u64>],
    positions: &[[f64; 3]],
) -> bool {
    let mut sources = model.geometry_sources();
    sources.resize(model.atoms.len() + model.hydrogens.len(), None);
    let now = model.atoms.iter().map(|a| a.xyz);
    let now = now.chain(model.hydrogens.iter().map(|h| h.xyz));
    atoms.len() == positions.len()
        && sources == atoms
        && now.eq(positions.iter().map(|p| p.map(|x| x as f32)))
}

/// The document with the model of `atom`'s molecule at `positions` (Å,
/// frame order), shown beside the drawing if it was 2D only.
pub(super) fn preview(doc: &Document, atom: u64, positions: &[[f64; 3]]) -> Option<Document> {
    let index = doc
        .models
        .iter()
        .position(|m| m.linked_atoms.contains(&atom))?;
    let mut doc = doc.clone();
    let model = doc.models.get_mut(index)?;
    if model.atoms.len() + model.hydrogens.len() != positions.len() {
        return None;
    }
    let mut xyz = positions.iter().map(|p| p.map(|x| x as f32));
    for (atom, p) in model.atoms.iter_mut().zip(xyz.by_ref()) {
        atom.xyz = p;
    }
    for (hydrogen, p) in model.hydrogens.iter_mut().zip(xyz) {
        hydrogen.xyz = p;
    }
    // A model shown as 2D only appears beside the drawing while it moves.
    if model.display == reshiki::models3d::Display::TwoD {
        reshiki::models3d::set_display(&mut doc, index, reshiki::models3d::Display::Both);
    }
    Some(doc)
}

#[cfg(not(target_arch = "wasm32"))]
fn fixed_indices(atoms: &[Option<u64>], fixed: &[u64]) -> impl Iterator<Item = usize> {
    atoms
        .iter()
        .enumerate()
        .filter_map(move |(index, id)| id.is_some_and(|id| fixed.contains(&id)).then_some(index))
}

/// The result to store from a finished run.
#[cfg(not(target_arch = "wasm32"))]
fn pending(run: &Run, report: &run::Report) -> Result<Pending, state::Failure> {
    let launch = run
        .launch
        .clone()
        .ok_or_else(|| state::Failure::Other("the run finished before it started".into()))?;
    let input = launch.frame.hash();
    let optimized = run.config.task.optimizes();
    let frames = if optimized {
        vec![Frame {
            positions_angstrom: report.positions.clone(),
            ..(*launch.frame).clone()
        }]
    } else {
        vec![]
    };
    let frame = frames.first().unwrap_or(&launch.frame);
    let task = match run.config.task {
        CalcTask::Energy => qcdata::Task::Energy,
        CalcTask::Optimize | CalcTask::OptFreq => qcdata::Task::Optimize,
        CalcTask::Frequencies => qcdata::Task::Hessian,
        CalcTask::Conformers => qcdata::Task::Conformers,
    };
    let gfnff_reference = (run.config.method == qcdata::XtbMethod::GfnFf).then_some(input);
    let computed = &report.computed;
    let mut result = computed
        .to_calc_result(frame, task, &report.engine, gfnff_reference)
        .map_err(|error| state::Failure::Other(error.to_string()))?;
    if optimized {
        result.provenance.method = Some(run::optimizer_provenance(
            result.provenance.method.as_deref().unwrap_or("xTB"),
            &report.optimizers,
        ));
    }
    if let qcdata::Settings::Xtb(settings) = &mut result.settings {
        settings.threads = Some(run.config.threads);
        settings.convergence =
            optimized.then_some(if run.config.tight || run.config.task.frequencies() {
                qcdata::Convergence::Tight
            } else {
                qcdata::Convergence::Normal
            });
        settings.fixed_atoms = fixed_indices(&launch.atoms, &run.config.fixed)
            .map(|index| index as u32)
            .collect();
    }
    result.run = Some(qcdata::RunSummary {
        seconds: run.seconds(),
        scf_iterations: report
            .evaluations
            .iter()
            .map(|evaluation| evaluation.scf_iterations as u32)
            .collect(),
        finished: Some(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)),
        search: None,
        transition_state: None,
    });
    let status = if report.converged {
        qcdata::Status::Converged
    } else {
        qcdata::Status::NotConverged {
            reason: format!("Stopped after {} steps", report.steps),
        }
    };
    let trajectory = optimized.then(|| qcdata::Trajectory {
        numbers: launch.frame.numbers.clone(),
        isotopes: launch.frame.isotopes.clone(),
        masses_amu: launch.frame.masses_amu.clone(),
        charge: launch.frame.charge,
        multiplicity: launch.frame.multiplicity,
        positions_angstrom: report.path.clone(),
        energies_hartree: report.energies.clone(),
        labels: vec![],
        minimizer_energies_hartree: vec![],
        cluster_sizes: vec![],
        rmsd_to_best_angstrom: vec![],
    });
    result.input_frame = optimized.then_some(input);
    result.status = status;
    result.trajectory = trajectory;
    result.keep_full = run.keep_full;
    let mut notes = Vec::new();
    if computed.near_degenerate {
        notes.push(state::NEAR_DEGENERATE.into());
    }
    let info = RunInfo {
        gap_ev: computed.homo_lumo_gap_ev,
        near_degenerate: computed.near_degenerate,
        evaluations: report.evaluations.clone(),
        numerics: computed.numerics.clone(),
    };
    let line = state::summary(
        report.energy,
        computed.homo_lumo_gap_ev,
        optimized.then_some((report.converged, report.steps)),
        result.vibrations.as_ref(),
    );
    Ok(Pending {
        pushed: run.start.is_some(),
        accepted: run.start.as_ref().map(|start| start.from.clone()),
        info,
        launch,
        frames,
        result,
        line,
        notes,
        attempt: None,
    })
}

#[cfg(test)]
pub(super) use run::{Event as RunEvent, Report as RunReport, example_computed, example_engine};

#[cfg(test)]
impl App {
    /// Answer the run in flight as its worker would.
    pub(super) fn xtb_answer(&mut self, event: run::Event) {
        let serial = self.tab.xtb.run.as_ref().expect("a run in flight").serial;
        let _ = self.update(Message::Xtb(Action::Event(serial, event)));
    }
}

#[cfg(test)]
mod tests;
