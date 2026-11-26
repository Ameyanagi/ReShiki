//! Transition-state searches from the reaction arrow, with session-only path previews.
#[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
use super::Action as XtbAction;
use super::{is, state};
use crate::app::{App, Message};
use iced::Task;
use reshiki::document::Document;
use reshiki::qcdata::{CalcResult, Frame, Task as CalcTask};
use reshiki::reactions::ts_plan::{self, TsPlan};
use reshiki::science::ScienceError;
use std::cell::Cell;
use std::rc::Rc;
use std::sync::{Arc, Weak};

pub(super) mod card;
pub(in crate::app) mod export;
#[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
mod job;
mod view;

#[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
pub(super) use job::Event;

#[derive(Clone, Debug)]
pub(crate) enum Action {
    Card(card::Action),
    Image(usize),
    Rerun,
    Export,
}

#[derive(Clone)]
pub(super) struct Request {
    pub arrow: u64,
    #[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
    plan: TsPlan,
    pub images: usize,
}

#[cfg_attr(
    not(all(not(target_arch = "wasm32"), feature = "xtb")),
    allow(dead_code)
)]
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(crate) enum Stage {
    #[default]
    Endpoints,
    Idpp,
    Neb,
    Saddle,
    Irc,
}

pub(super) struct Live {
    pub request: Request,
    stage: Stage,
    iteration: Option<usize>,
    max_force: f64,
    climbing: Option<usize>,
    step: Option<usize>,
    forward: bool,
}

impl Live {
    // These are the engine's default iteration limits.
    const NEB_LIMIT: usize = 300;
    const SADDLE_LIMIT: usize = 100;
    const IRC_LIMIT: usize = 60;

    pub(super) fn new(request: Request) -> Self {
        Self {
            request,
            stage: Stage::Endpoints,
            iteration: None,
            max_force: 0.0,
            climbing: None,
            step: None,
            forward: true,
        }
    }

    pub fn line(&self) -> String {
        [
            (Stage::Endpoints, "Endpoints".to_owned()),
            (Stage::Idpp, "IDPP".to_owned()),
            (
                Stage::Neb,
                self.iteration
                    .filter(|_| self.stage == Stage::Neb)
                    .map_or_else(
                        || "NEB".into(),
                        |iteration| format!("NEB {iteration}/{}", Self::NEB_LIMIT),
                    ),
            ),
            (Stage::Saddle, "TS refine".to_owned()),
            (Stage::Irc, "IRC".to_owned()),
        ]
        .map(|(stage, label)| {
            if self.stage > stage {
                format!("✓ {label}")
            } else {
                label
            }
        })
        .join(" · ")
    }

    pub fn detail(&self) -> String {
        match self.stage {
            Stage::Endpoints => "Building and optimizing both endpoints".into(),
            Stage::Idpp => "Interpolating the path with IDPP".into(),
            Stage::Neb => {
                let label = if self.climbing.is_some() {
                    "Climbing-image NEB"
                } else {
                    "NEB"
                };
                self.iteration.map_or_else(
                    || format!("{label} · {} images", self.request.images),
                    |iteration| {
                        format!(
                            "{label} · iteration {iteration} of at most {} · max force {:.3} Eh/bohr",
                            Self::NEB_LIMIT, self.max_force
                        )
                    },
                )
            }
            Stage::Saddle => self.step.map_or_else(
                || "Refining the saddle with P-RFO".into(),
                |step| format!("P-RFO · step {step}"),
            ),
            Stage::Irc => self.step.map_or_else(
                || "Checking both IRC branches".into(),
                |step| {
                    let branch = if self.forward { "forward" } else { "reverse" };
                    format!("IRC · {branch} branch · step {step}")
                },
            ),
        }
    }

    pub fn fraction(&self) -> Option<f32> {
        let part =
            |step: Option<usize>, limit: usize| step.unwrap_or(0).min(limit) as f32 / limit as f32;
        Some(match self.stage {
            Stage::Endpoints => return None,
            Stage::Idpp => 0.1,
            Stage::Neb => 0.15 + 0.5 * part(self.iteration, Self::NEB_LIMIT),
            Stage::Saddle => 0.65 + 0.15 * part(self.step, Self::SADDLE_LIMIT),
            Stage::Irc => {
                let branch = if self.forward { 0.8 } else { 0.9 };
                branch + 0.1 * part(self.step, Self::IRC_LIMIT)
            }
        })
    }
}

/// Data whose lifetime is the open document. The saved result carries only
/// the energies and the three structures needed by the Path tab and export.
#[derive(Clone, Debug)]
struct Extras {
    image_positions: Vec<Vec<[f64; 3]>>,
    // Keep the search diagnostics in the session. The card uses the saved
    // endpoint bond graphs for its distances, including after reopening.
    #[allow(dead_code)]
    irc_energies: [Vec<f64>; 2],
    connects: bool,
    #[allow(dead_code)]
    forming: Vec<[usize; 2]>,
    #[allow(dead_code)]
    breaking: Vec<[usize; 2]>,
    worker_frequencies: Vec<f64>,
    climbing: usize,
    engine_imaginary: Option<f64>,
}

pub(super) struct Session {
    extras: Extras,
    arrow: u64,
    plan: TsPlan,
    /// The starting attachment covers just one side of the reaction.
    /// Check its reaction plan too when the drawing changes.
    freshness: Cell<Option<((u64, u64), bool)>>,
}

impl Session {
    pub(super) fn stale(&self, doc: &Document, revision: (u64, u64)) -> bool {
        if let Some((at, stale)) = self.freshness.get()
            && at == revision
        {
            return stale;
        }
        let stale = ts_plan::plan(doc, self.arrow).as_ref() != Ok(&self.plan);
        self.freshness.set(Some((revision, stale)));
        stale
    }

    pub(super) fn image(&self, index: usize) -> Option<Vec<[f64; 3]>> {
        self.extras.image_positions.get(index).cloned()
    }
}

#[derive(Default)]
pub(super) struct Sessions(Vec<(Weak<CalcResult>, Rc<Session>)>);

impl Sessions {
    pub(super) fn get(&self, result: &Arc<CalcResult>) -> Option<&Rc<Session>> {
        self.0
            .iter()
            .find(|(weak, _)| is(weak, result))
            .map(|(_, ts)| ts)
    }

    pub(super) fn remember(&mut self, result: &Arc<CalcResult>, session: Rc<Session>) {
        self.0.retain(|(weak, _)| weak.strong_count() > 0);
        self.0.push((Arc::downgrade(result), session));
    }
}

pub(super) fn is_ts(result: &CalcResult) -> bool {
    result.task == CalcTask::TransitionState
}

pub(super) fn path_energies(result: &CalcResult) -> &[f64] {
    result
        .trajectory
        .as_ref()
        .filter(|_| is_ts(result))
        .map_or(&[], |path| &path.energies_hartree)
}

/// A saved TS links to its starting molecule. Find current plans that can
/// still start from that molecule, including the roles on the opposite side.
pub(super) fn matching_plans<'a>(
    doc: &'a Document,
    atoms: &'a [Option<u64>],
) -> impl Iterator<Item = (u64, TsPlan)> + 'a {
    let anchor = atoms.iter().flatten().next();
    let count = atoms.iter().flatten().count();
    doc.reactions
        .iter()
        .filter(move |reaction| {
            anchor.is_some_and(|id| {
                reaction
                    .reactants
                    .iter()
                    .chain(&reaction.products)
                    .any(|participant| participant.atoms.contains(id))
            })
        })
        .filter_map(move |reaction| {
            let plan = ts_plan::plan(doc, reaction.arrow).ok()?;
            (plan.start_atoms.len() == count
                && plan.start_atoms.iter().all(|id| atoms.contains(&Some(*id))))
            .then_some((reaction.arrow, plan))
        })
}

/// Copy a path image's coordinates into the canvas preview. The model keeps
/// its existing bond topology, including the original parents of added H.
pub(super) fn preview_image(
    doc: &mut Document,
    atoms: &[Option<u64>],
    input: &Frame,
    image: &Frame,
) -> Result<(), ScienceError> {
    let anchor = atoms.iter().flatten().next().ok_or(ScienceError::NoModel)?;
    let model = doc
        .models
        .iter_mut()
        .find(|model| model.linked_atoms.contains(anchor))
        .ok_or(ScienceError::NoModel)?;
    let count = model.atoms.len() + model.hydrogens.len();
    let positions: Vec<_> = image
        .positions_angstrom
        .iter()
        .map(|xyz| xyz.map(|x| x as f32))
        .collect();
    if image.numbers != input.numbers
        || count != atoms.len()
        || count != image.numbers.len()
        || count != positions.len()
        || !model
            .geometry_sources()
            .into_iter()
            .chain(std::iter::repeat_n(None, model.hydrogens.len()))
            .eq(atoms.iter().copied())
        || !positions.iter().flatten().all(|x| x.is_finite())
    {
        return Err(ScienceError::ModelMismatch);
    }
    for (position, xyz) in model
        .atoms
        .iter_mut()
        .map(|a| &mut a.xyz)
        .chain(model.hydrogens.iter_mut().map(|h| &mut h.xyz))
        .zip(positions)
    {
        *position = xyz;
    }
    Ok(())
}

impl App {
    pub(super) fn xtb_ts_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Card(action) => self.xtb_ts_card_action(action),
            Action::Image(index) => {
                if self
                    .xtb_shown()
                    .is_some_and(|shown| is_ts(&shown.stored.result))
                {
                    self.xtb_preview_step(index);
                }
                Task::none()
            }
            Action::Rerun => self.xtb_ts_rerun(),
            Action::Export => self.xtb_ts_export(),
        }
    }

    pub(super) fn xtb_ts_target(&self) -> Option<Vec<u64>> {
        let arrow = self.selected_reaction_arrow()?.id;
        Some(ts_plan::plan(&self.tab.doc, arrow).ok()?.start_atoms)
    }

    fn xtb_ts_rerun_plan(&self, shown: &super::palette::Shown<'_>) -> Option<(u64, TsPlan)> {
        if shown.stale || !is_ts(&shown.stored.result) {
            return None;
        }
        if let Some(session) = shown.ts {
            return Some((session.arrow, session.plan.clone()));
        }
        // The file links the result to the starting molecule. Only infer an
        // arrow after reopening when that molecule identifies a single plan.
        let mut candidates = matching_plans(&self.tab.doc, shown.atoms);
        let candidate = candidates.next()?;
        candidates.next().is_none().then_some(candidate)
    }

    fn xtb_ts_can_rerun(&self, shown: &super::palette::Shown<'_>) -> bool {
        super::available() && !self.xtb_running() && self.xtb_ts_rerun_plan(shown).is_some()
    }

    fn xtb_ts_rerun(&mut self) -> Task<Message> {
        let Some(shown) = self.xtb_shown() else {
            return Task::none();
        };
        let Some((arrow, plan)) = self.xtb_ts_rerun_plan(&shown) else {
            return Task::none();
        };
        self.xtb_ts_begin(
            arrow,
            plan,
            20,
            shown.frame.charge,
            shown.frame.multiplicity.saturating_sub(1),
        )
    }

    fn xtb_ts_begin(
        &mut self,
        arrow: u64,
        plan: TsPlan,
        images: usize,
        charge: i32,
        unpaired: u32,
    ) -> Task<Message> {
        if !super::available() || !self.xtb_can_start() {
            return Task::none();
        }
        let config = state::Config {
            // The run's electronic state and initial geometry use the
            // ordinary xTB launch. The TS pipeline drives the actual task.
            task: state::Task::Energy,
            method: reshiki::qcdata::XtbMethod::Gfn2Xtb,
            charge,
            unpaired,
            solvent: None,
            xtb68: false,
            accuracy: None,
            temperature_k: None,
            max_iterations: None,
            threads: 1,
            tight: false,
            fixed: vec![],
        };
        self.xtb_begin(super::Request {
            atoms: plan.start_atoms.clone(),
            config,
            keep_full: false,
            start: None,
            search: None,
            ts: Some(Request {
                arrow,
                #[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
                plan,
                images,
            }),
        })
    }
}

#[cfg(all(test, not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
mod tests;

#[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
impl App {
    pub(super) fn xtb_ts_start(&mut self, launch: reshiki::science::Launch) -> Task<Message> {
        let Some(run) = self.tab.xtb.run.as_mut() else {
            return Task::none();
        };
        let Some(live) = &run.ts else {
            return Task::none();
        };
        let Request { plan, images, .. } = live.request.clone();
        let cancel = run.cancel.clone();
        run.label = "xTB · transition state".into();
        run.launch = Some(launch.clone());
        run.revision = (self.tab.file_epoch, self.tab.revision);
        run.started = iced::time::Instant::now();
        let serial = run.serial;
        let events = iced::futures::StreamExt::flatten(iced::futures::stream::once(async move {
            job::start(launch, plan, images, cancel)
        }));
        let (task, handle) = Task::run(events, move |event| {
            Message::Xtb(XtbAction::TsEvent(serial, event))
        })
        .abortable();
        run.handle = Some(handle.abort_on_drop());
        self.status = run.status();
        self.error = false;
        task
    }

    pub(super) fn xtb_ts_event(&mut self, serial: u64, event: Event) {
        let Some(run) = self.tab.xtb.run.as_mut().filter(|run| run.serial == serial) else {
            return;
        };
        let Some(live) = run.ts.as_mut() else {
            return;
        };
        match event {
            Event::Stage(stage) => {
                if live.stage != stage {
                    live.stage = stage;
                    live.step = None;
                }
            }
            Event::Neb {
                iteration,
                max_force,
                climbing,
            } => {
                live.iteration = Some(iteration);
                live.max_force = max_force;
                live.climbing = climbing;
            }
            Event::Saddle { step } => live.step = Some(step),
            Event::Irc { forward, step } => {
                live.forward = forward;
                live.step = Some(step);
            }
            Event::Done(Ok(report)) => {
                let Some(launch) = run.launch.take() else {
                    return;
                };
                let request = live.request.clone();
                let job::Report {
                    mut result,
                    frames,
                    extras,
                    info,
                } = Arc::unwrap_or_clone(report);
                result.keep_full = run.keep_full;
                let line = match &result.status {
                    reshiki::qcdata::Status::Converged => {
                        "Transition state · IRC connects reactant and product".into()
                    }
                    reshiki::qcdata::Status::NotConverged { reason }
                    | reshiki::qcdata::Status::Failed { reason } => {
                        format!("Transition state candidate · {reason}")
                    }
                };
                let pending = super::Pending {
                    pushed: false,
                    accepted: None,
                    launch,
                    frames,
                    result,
                    line,
                    notes: vec![],
                    info,
                };
                let session = Rc::new(Session {
                    extras,
                    arrow: request.arrow,
                    plan: request.plan,
                    freshness: Cell::new(None),
                });
                self.tab.xtb.run = None;
                self.xtb_store_with_session(pending, false, Some(session));
                return;
            }
            Event::Done(Err(failure)) => {
                self.xtb_fail(failure);
                return;
            }
        }
        self.status = run.status();
        self.error = false;
    }
}
