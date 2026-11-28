//! A scheme owns a queue, while every calculation keeps the ordinary isolated
//! molecule/TS worker boundary. Partial results survive cancellation and retry.
use super::plan::Plan;
use crate::app::xtb::ts::Stage;
use reshiki::{
    document::Document,
    qcdata::{CalcResult, Frame, XtbMethod},
    reactions::ts_plan::TsPlan,
};
use std::collections::BTreeSet;
use std::sync::Arc;
#[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
static NEXT_SERIAL: AtomicU64 = AtomicU64::new(1);

#[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
mod worker;
#[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
use reshiki_xtb_worker::client::Cancel;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) struct StepOptions {
    pub reaction_energy: bool,
    pub barrier: bool,
    pub free_energy: bool,
    pub free_barrier: bool,
}

impl Default for StepOptions {
    fn default() -> Self {
        Self {
            reaction_energy: true,
            barrier: true,
            free_energy: false,
            free_barrier: false,
        }
    }
}

impl StepOptions {
    pub fn any(self) -> bool {
        self.reaction_energy || self.barrier || self.free_energy || self.free_barrier
    }
    pub fn transition_state(self) -> bool {
        self.barrier || self.free_barrier
    }
    pub fn frequencies(self) -> bool {
        self.free_energy || self.free_barrier
    }
}

fn frequency_species(
    plan: &Plan,
    options: &[StepOptions],
    steps: impl IntoIterator<Item = usize>,
) -> BTreeSet<usize> {
    let mut species = BTreeSet::new();
    for index in steps {
        let (Some(step), Some(options)) = (plan.steps.get(index), options.get(index)) else {
            continue;
        };
        if options.frequencies() {
            species.extend(step.reactants.iter().map(|term| term.species));
        }
        if options.free_energy {
            species.extend(step.products.iter().map(|term| term.species));
        }
    }
    species
}

/// Reviewed, balanced, detached reaction. Synthetic atoms never enter the
/// user's document, including during model generation and endpoint building.
#[derive(Clone, Debug)]
pub(in crate::app) struct TsInput {
    pub document: Document,
    pub plan: TsPlan,
    pub images: usize,
    pub charge: i32,
    pub multiplicity: u32,
    pub automatic: bool,
}

#[derive(Clone, Debug)]
pub(in crate::app) struct SpeciesValue {
    pub species: usize,
    pub method: XtbMethod,
    pub frame: Arc<Frame>,
    pub result: Arc<CalcResult>,
    pub launch: Option<reshiki::science::Launch>,
}

/// All frames and energies use this arrow's atom map, charge, spin and method.
#[derive(Clone, Debug)]
pub(in crate::app) struct DiscoveredMinimum {
    pub frame: Arc<Frame>,
    pub energy_hartree: f64,
    pub gibbs_correction_hartree: Option<f64>,
}

#[derive(Clone, Debug)]
pub(in crate::app) struct DiscoveredSegment {
    pub reactant: usize,
    pub product: usize,
    pub ts: Arc<CalcResult>,
    pub frames: Vec<Arc<Frame>>,
}

#[derive(Clone, Debug)]
pub(in crate::app) struct DiscoveredGap {
    pub reactant: usize,
    pub product: usize,
    pub reason: String,
}

#[derive(Clone, Debug)]
pub(in crate::app) struct DiscoveredPath {
    pub minima: Vec<DiscoveredMinimum>,
    pub order: Vec<usize>,
    pub segments: Vec<DiscoveredSegment>,
    pub gaps: Vec<DiscoveredGap>,
    pub diagnostics: Vec<String>,
}

impl DiscoveredPath {
    pub(super) fn has_gibbs(&self) -> bool {
        self.order
            .iter()
            .skip(1)
            .take(self.order.len().saturating_sub(2))
            .all(|id| {
                self.minima
                    .get(*id)
                    .and_then(|m| m.gibbs_correction_hartree)
                    .is_some()
            })
            && self.segments.iter().all(|s| s.ts.thermo.is_some())
    }
}

#[derive(Clone, Debug)]
pub(in crate::app) struct StepValue {
    pub arrow: u64,
    pub method: XtbMethod,
    /// Captured request provenance: a missing TS may be a failed search,
    /// whereas ordinary energy-only steps never requested a connection check.
    pub transition_state_requested: bool,
    pub ts: Option<Arc<CalcResult>>,
    pub discovered: Option<DiscoveredPath>,
    pub frames: Vec<Arc<Frame>>,
    pub launch: Option<reshiki::science::Launch>,
    /// All displayed quantities use kcal/mol. Missing values are never zero.
    pub energy: Option<f64>,
    pub gibbs: Option<f64>,
    pub barrier: Option<f64>,
    pub free_barrier: Option<f64>,
    pub error: Option<String>,
}

impl StepValue {
    fn empty(arrow: u64, method: XtbMethod) -> Self {
        Self {
            arrow,
            method,
            transition_state_requested: false,
            ts: None,
            discovered: None,
            frames: Vec::new(),
            launch: None,
            energy: None,
            gibbs: None,
            barrier: None,
            free_barrier: None,
            error: None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(in crate::app) struct Report {
    pub species: Vec<SpeciesValue>,
    pub steps: Vec<Option<StepValue>>,
}

impl Report {
    pub fn species(&self, index: usize) -> Option<&SpeciesValue> {
        self.species.iter().find(|value| value.species == index)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::app) enum StepStatus {
    Pending,
    Running,
    Done,
    Failed(String),
    Cancelled,
    Skipped,
}

#[derive(Clone, Debug)]
pub(in crate::app) struct Progress {
    pub step: usize,
    pub species: Option<usize>,
    pub label: String,
    pub completed_species: usize,
    pub total_species: usize,
    pub stage: Option<Stage>,
}

#[derive(Clone, Debug)]
pub(crate) struct Event {
    serial: u64,
    kind: EventKind,
}

#[derive(Clone, Debug)]
enum EventKind {
    Progress(Progress),
    Species { key: String, value: SpeciesValue },
    Endpoints { index: usize, value: StepValue },
    Step { index: usize, value: StepValue },
    Finished(Result<(), String>),
}

impl Event {
    /// Exactly one final event is emitted, including failed launches and cancel.
    pub fn finished(&self) -> bool {
        matches!(self.kind, EventKind::Finished(_))
    }
}

#[derive(Clone, Debug)]
struct Cached {
    key: String,
    method: XtbMethod,
    frame: Arc<Frame>,
    result: Arc<CalcResult>,
    launch: Option<reshiki::science::Launch>,
}

#[derive(Default)]
pub(in crate::app) struct State {
    pub report: Report,
    pub steps: Vec<StepStatus>,
    pub progress: Option<Progress>,
    pub error: Option<String>,
    cache: Vec<Cached>,
    serial: u64,
    running: bool,
    draining: bool,
    #[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
    cancel: Option<Cancel>,
}

impl Drop for State {
    fn drop(&mut self) {
        #[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
        if let Some(cancel) = &self.cancel {
            cancel.cancel();
        }
    }
}

impl State {
    pub fn running(&self) -> bool {
        self.running
    }
    pub fn step(&self, index: usize) -> Option<&StepValue> {
        self.report.steps.get(index)?.as_ref()
    }

    /// Original document results must satisfy the single-arrow eligibility
    /// rules and reproduce the drawing's stereo. Detached copies carry no
    /// attachments, so check each result with its original drawing atom map.
    pub fn seed(
        &mut self,
        doc: &Document,
        plan: &Plan,
        method: XtbMethod,
        options: &[StepOptions],
    ) {
        let frequencies = frequency_species(plan, options, 0..plan.steps.len());
        for (index, species) in plan.species.iter().enumerate() {
            let lookup = |frequencies| {
                species.instances.iter().find_map(|atoms| {
                    crate::app::xtb::ts::energy::scheme_reusable(
                        doc,
                        atoms,
                        species.charge,
                        species.multiplicity,
                        method,
                        frequencies,
                    )
                    .filter(|(frame, _, launch)| {
                        check_species_stereo(doc, &launch.atoms, frame).is_ok()
                    })
                })
            };
            let cached = frequencies
                .contains(&index)
                .then(|| lookup(true))
                .flatten()
                .or_else(|| lookup(false));
            if let Some((frame, result, launch)) = cached {
                self.cache_value(species.key.clone(), method, frame, result, Some(launch));
            }
        }
    }

    fn cache_value(
        &mut self,
        key: String,
        method: XtbMethod,
        frame: Arc<Frame>,
        result: Arc<CalcResult>,
        launch: Option<reshiki::science::Launch>,
    ) {
        if !transition_thermo(&result, &frame)
            && self.cache.iter().any(|old| {
                old.key == key && old.method == method && transition_thermo(&old.result, &old.frame)
            })
        {
            return;
        }
        self.cache
            .retain(|old| old.key != key || old.method != method);
        self.cache.push(Cached {
            key,
            method,
            frame,
            result,
            launch,
        });
    }

    /// Stop execution while retaining this run's final completed data. Progress
    /// cannot restart the UI, and Finished closes the remaining event window.
    pub fn stop(&mut self) {
        #[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
        if let Some(cancel) = self.cancel.take() {
            cancel.cancel();
        }
        self.draining |= self.running;
        self.running = false;
        self.progress = None;
        for status in &mut self.steps {
            if matches!(status, StepStatus::Pending | StepStatus::Running) {
                *status = StepStatus::Cancelled;
            }
        }
    }

    /// Invalidate a replaced run or stale document, including any pending drain.
    pub fn cancel(&mut self) {
        self.stop();
        self.draining = false;
        self.serial = self.serial.wrapping_add(1);
    }

    /// Check before document freshness so events from replaced work cannot act.
    pub fn accepts(&self, event: &Event) -> bool {
        event.serial == self.serial
            && (self.running || (self.draining && !matches!(event.kind, EventKind::Progress(_))))
    }

    #[cfg(test)]
    pub(super) fn completion_event(&self) -> Event {
        Event {
            serial: self.serial,
            kind: EventKind::Finished(Ok(())),
        }
    }

    /// Every accepted species and step is available to placement and retry.
    pub fn accept(&mut self, event: Event) -> bool {
        if !self.accepts(&event) {
            return false;
        }
        match event.kind {
            EventKind::Progress(progress) => {
                if let Some(status) = self.steps.get_mut(progress.step) {
                    *status = StepStatus::Running;
                }
                self.progress = Some(progress);
            }
            EventKind::Species { key, value } => {
                self.cache_value(
                    key,
                    value.method,
                    Arc::clone(&value.frame),
                    Arc::clone(&value.result),
                    value.launch.clone(),
                );
                self.report
                    .species
                    .retain(|old| old.species != value.species);
                self.report.species.push(value);
            }
            EventKind::Endpoints { index, value } => {
                if let Some(slot) = self.report.steps.get_mut(index) {
                    *slot = Some(value);
                }
            }
            EventKind::Step { index, value } => {
                if !self.draining
                    && let Some(status) = self.steps.get_mut(index)
                {
                    *status = value.error.as_ref().map_or(StepStatus::Done, |reason| {
                        StepStatus::Failed(reason.clone())
                    });
                }
                if let Some(slot) = self.report.steps.get_mut(index) {
                    *slot = Some(value);
                }
            }
            EventKind::Finished(result) => {
                self.running = false;
                let draining = std::mem::take(&mut self.draining);
                self.progress = None;
                self.error = if draining { None } else { result.err() };
                if let Some(reason) = &self.error {
                    for status in &mut self.steps {
                        if matches!(status, StepStatus::Pending | StepStatus::Running) {
                            *status = StepStatus::Failed(reason.clone());
                        }
                    }
                }
                #[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
                {
                    self.cancel = None;
                }
            }
        }
        true
    }

    #[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
    pub fn start(
        &mut self,
        plan: &Plan,
        options: &[StepOptions],
        method: XtbMethod,
        inputs: Vec<Result<Option<TsInput>, String>>,
        threads: u32,
        retry: Option<usize>,
    ) -> iced::futures::channel::mpsc::UnboundedReceiver<Event> {
        self.cancel();
        self.serial = NEXT_SERIAL.fetch_add(1, Ordering::Relaxed);
        self.running = true;
        self.error = None;
        let cancel = Cancel::default();
        self.cancel = Some(cancel.clone());
        if retry.is_none() || self.report.steps.len() != plan.steps.len() {
            self.report = Report {
                species: Vec::new(),
                steps: vec![None; plan.steps.len()],
            };
            self.steps = options
                .iter()
                .map(|options| {
                    if options.any() {
                        StepStatus::Pending
                    } else {
                        StepStatus::Skipped
                    }
                })
                .collect();
        } else if let Some(index) = retry {
            if let Some(status) = self.steps.get_mut(index) {
                *status = StepStatus::Pending;
            }
            if let Some(value) = self.report.steps.get_mut(index) {
                *value = None;
            }
        }
        worker::start(
            worker::Request {
                plan: plan.clone(),
                options: options.to_vec(),
                method,
                inputs,
                threads,
                retry,
                cache: self.request_cache(plan, method, retry.is_some()),
                serial: self.serial,
            },
            cancel,
        )
    }

    fn request_cache(&self, plan: &Plan, method: XtbMethod, retry: bool) -> Vec<Cached> {
        let mut cache = self.cache.clone();
        if retry {
            // A retained step is defined by the exact minima it originally
            // used. A newer drawing result must not replace a shared minimum
            // in just the retried step and break energy cancellation.
            for value in &self.report.species {
                let Some(species) = plan.species.get(value.species) else {
                    continue;
                };
                if value.method != method {
                    continue;
                }
                cache.retain(|old| old.key != species.key || old.method != method);
                cache.push(Cached {
                    key: species.key.clone(),
                    method,
                    frame: Arc::clone(&value.frame),
                    result: Arc::clone(&value.result),
                    launch: value.launch.clone(),
                });
            }
        }
        cache
    }
}

fn check_species_stereo(
    drawing: &Document,
    atoms: &[Option<u64>],
    frame: &Frame,
) -> Result<(), String> {
    let changes = reshiki::science::stereo_changes(drawing, atoms, frame)
        .map_err(|error| format!("The stereo check could not run: {error}."))?;
    if changes.is_empty() {
        return Ok(());
    }
    Err(changes
        .iter()
        .map(|change| {
            crate::app::xtb::state::describe(change, |id| {
                crate::app::xtb::state::stereo_atom_name(drawing, atoms, id)
            })
        })
        .collect::<Vec<_>>()
        .join(" "))
}

pub(super) fn species_energy(value: &SpeciesValue, free: bool) -> Option<f64> {
    let energy = value
        .result
        .energy_hartree
        .filter(|energy| energy.is_finite())?;
    let correction = if free {
        if !transition_thermo(&value.result, &value.frame) {
            return None;
        }
        value.result.thermo.as_ref()?.gibbs_correction_hartree
    } else {
        0.
    };
    let energy = energy + correction;
    energy.is_finite().then_some(energy)
}

fn transition_thermo(result: &CalcResult, frame: &Frame) -> bool {
    crate::app::xtb::ts::energy::matching_thermo(result, frame)
        && result.vibrations.as_ref().is_some_and(|v| {
            v.frequencies_cm1
                .iter()
                .all(|f| f.is_finite() && *f >= -20.)
        })
}
