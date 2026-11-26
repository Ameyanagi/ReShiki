//! Transition-state searches from the reaction arrow, with session-only path previews.
#[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
use super::Action as XtbAction;
use super::{is, state};
use crate::app::{App, Message};
use iced::Task;
use reshiki::document::Document;
use reshiki::qcdata::{
    CalcResult, Frame, Task as CalcTask, TransitionStateContext, TransitionStateRun,
};
use reshiki::reactions::ts_plan::{self, TsPlan};
use reshiki::reactions::{Reaction, Role};
use reshiki::science::ScienceError;
use sha2::{Digest, Sha256};
use std::cell::Cell;
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::{Arc, Weak};

pub(super) mod card;
pub(super) mod energy;
pub(in crate::app) mod export;
#[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
mod job;
pub(in crate::app) mod review;
pub(super) mod view;

#[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
pub(super) use job::Event;

#[derive(Clone, Debug)]
pub(crate) enum Action {
    Card(card::Action),
    Review(review::Mode, Option<u64>),
    ReviewMap(u64),
    ReviewSheet(review::Action),
    EnergyTask(state::Task),
    Image(usize),
    Rerun,
    Export,
}

#[derive(Clone)]
pub(super) struct Request {
    pub arrow: u64,
    bonds: card::BondChanges,
    pub context: TransitionStateContext,
    #[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
    plan: TsPlan,
    #[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
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
            step: None,
            forward: true,
        }
    }

    pub fn line(&self) -> String {
        [
            (Stage::Endpoints, "Building endpoints"),
            (Stage::Neb, "Finding a path"),
            (Stage::Saddle, "Refining the transition state"),
            (Stage::Irc, "Checking the reaction path"),
        ]
        .map(|(stage, label)| {
            if self.stage > stage {
                format!("✓ {label}")
            } else {
                label.to_owned()
            }
        })
        .join(" · ")
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
    saddle_converged: bool,
    num_imaginary: usize,
    diagnostics: Vec<(String, String)>,
    // Keep the search diagnostics in the session. The card uses the saved
    // endpoint bond graphs for its distances, including after reopening.
    #[allow(dead_code)]
    irc_energies: [Vec<f64>; 2],
    connects: bool,
    #[allow(dead_code)]
    forming: Vec<[usize; 2]>,
    #[allow(dead_code)]
    breaking: Vec<[usize; 2]>,
    #[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
    worker_frequencies: Vec<f64>,
    climbing: usize,
    engine_imaginary: Option<f64>,
}

pub(super) struct Session {
    extras: Extras,
    pub(super) arrow: u64,
    freshness: Cell<Option<((u64, u64), bool)>>,
}

impl Session {
    fn stale(&self, doc: &Document, search: &TransitionStateRun, revision: (u64, u64)) -> bool {
        if let Some((at, stale)) = self.freshness.get()
            && at == revision
        {
            return stale;
        }
        let stale = saved_search_stale(doc, search);
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

pub(super) fn saved_search(result: &CalcResult) -> Option<&TransitionStateRun> {
    result.run.as_ref()?.transition_state.as_deref()
}

fn endpoint_chemistry(doc: &Document, reaction: &Reaction, role: Role) -> String {
    let members: HashSet<_> = reaction
        .participants(role)
        .iter()
        .flat_map(|participant| participant.atoms.iter().copied())
        .collect();
    let mut atoms: Vec<_> = doc
        .atoms
        .iter()
        .filter(|atom| members.contains(&atom.id))
        .collect();
    // Insertion reassigns drawing IDs, so break map-number ties in drawing order.
    atoms.sort_by_key(|atom| atom.map_num);
    let ids: Vec<_> = atoms.iter().map(|atom| atom.id).collect();
    format!(
        "{:x}",
        Sha256::digest(reshiki::science::baseline(doc, &ids))
    )
}

pub(super) fn bound_arrow(doc: &Document, arrow: u64, atoms: &[Option<u64>]) -> Option<u64> {
    if !review::reaction_arrow(doc, arrow) {
        return None;
    }
    let reaction = reshiki::reaction_mapping::reaction(doc, arrow).ok()?;
    let mut linked = atoms.iter().flatten().peekable();
    linked.peek()?;
    reaction
        .reactants
        .iter()
        .chain(&reaction.products)
        .any(|participant| linked.clone().all(|id| participant.atoms.contains(id)))
        .then_some(arrow)
}

pub(super) fn not_verified(result: &CalcResult) -> bool {
    is_ts(result) && saved_search(result).is_none_or(|search| search.context.is_none())
}

fn saved_search_stale(doc: &Document, search: &TransitionStateRun) -> bool {
    let (Some(arrow), Some(context)) = (search.arrow, &search.context) else {
        return true;
    };
    let Ok(reaction) = reshiki::reaction_mapping::reaction(doc, arrow) else {
        return true;
    };
    let Ok(plan) = ts_plan::plan_for_review(doc, arrow) else {
        return true;
    };
    let mut drawn = card::BondChanges::from_plan(doc, &plan).0;
    drawn.automatic = search.mapping.automatic;
    drawn != search.mapping
        || context.reactant_chemistry != endpoint_chemistry(doc, reaction, Role::Reactant)
        || context.product_chemistry != endpoint_chemistry(doc, reaction, Role::Product)
}

impl App {
    pub(super) fn xtb_ts_search_stale(&self, result: &Arc<CalcResult>) -> bool {
        let Some(search) = saved_search(result) else {
            return true;
        };
        let doc = &self.tab.doc;
        match self.tab.xtb.ts_sessions.get(result) {
            Some(session) => session.stale(doc, search, (self.tab.file_epoch, self.tab.revision)),
            None => saved_search_stale(doc, search),
        }
    }

    pub(super) fn xtb_saved_reaction_title(
        &self,
        result: &CalcResult,
        arrow: Option<u64>,
    ) -> String {
        saved_search(result)
            .and_then(|search| search.context.as_ref())
            .map(|context| context.title.clone())
            .or_else(|| arrow.map(|arrow| self.xtb_reaction_label(arrow).title()))
            .unwrap_or_else(|| "Reaction".into())
    }
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
            let plan = ts_plan::plan_for_review(doc, reaction.arrow).ok()?;
            (plan.start_atoms.len() == count
                && plan.start_atoms.iter().all(|id| atoms.contains(&Some(*id))))
            .then_some((reaction.arrow, plan))
        })
}

/// The saved starting molecule identifies an arrow only when one current
/// reaction could own its result. A reopened file has no originating session.
pub(super) fn unique_matching_plan(doc: &Document, atoms: &[Option<u64>]) -> Option<(u64, TsPlan)> {
    let mut candidates = matching_plans(doc, atoms);
    let candidate = candidates.next()?;
    candidates.next().is_none().then_some(candidate)
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
            Action::Review(mode, arrow) => self.xtb_review_open(mode, arrow),
            Action::ReviewMap(arrow) => {
                self.xtb_review_show(review::Mode::TransitionState, Some(arrow))
            }
            Action::ReviewSheet(action) => self.xtb_review_action(action),
            Action::EnergyTask(task) => {
                self.xtb_reaction_energy_task(task);
                Task::none()
            }
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

    fn xtb_ts_can_rerun(&self, shown: &super::palette::Shown<'_>) -> bool {
        super::available() && !self.xtb_running() && shown.arrow.is_some()
    }

    fn xtb_ts_rerun(&mut self) -> Task<Message> {
        let Some(arrow) = self.xtb_shown().and_then(|shown| shown.arrow) else {
            return Task::none();
        };
        self.xtb_review_rerun(arrow)
    }

    fn xtb_ts_begin(
        &mut self,
        arrow: u64,
        plan: TsPlan,
        images: usize,
        charge: i32,
        unpaired: u32,
    ) -> Task<Message> {
        #[cfg(not(all(not(target_arch = "wasm32"), feature = "xtb")))]
        let _ = images;
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
        let mut bonds = card::BondChanges::from_plan(&self.tab.doc, &plan);
        bonds.0.automatic = self
            .tab
            .xtb
            .ts_card
            .automatic_map((self.tab.file_epoch, arrow), &plan);
        let Ok(reaction) = reshiki::reaction_mapping::reaction(&self.tab.doc, arrow) else {
            return Task::none();
        };
        let label = self.xtb_reaction_label(arrow);
        let context = TransitionStateContext {
            reactant_chemistry: endpoint_chemistry(&self.tab.doc, reaction, Role::Reactant),
            product_chemistry: endpoint_chemistry(&self.tab.doc, reaction, Role::Product),
            title: label.title(),
            reactant: label.first_reactant().unwrap_or("Reaction").to_owned(),
        };
        self.xtb_begin(super::Request {
            atoms: plan.start_atoms.clone(),
            config,
            keep_full: false,
            start: None,
            search: None,
            ts: Some(Request {
                arrow,
                bonds,
                context,
                #[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
                plan,
                #[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
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
        // Hydrogen interactions (order 0) are not covalent bonds.
        let drawn_bonds: Vec<_> = self
            .tab
            .doc
            .bonds
            .iter()
            .filter(|bond| {
                bond.order > 0
                    && plan.start_atoms.contains(&bond.a)
                    && plan.start_atoms.contains(&bond.b)
            })
            .map(|bond| (bond.a, bond.b))
            .collect();
        let cancel = run.cancel.clone();
        run.label = "xTB · transition state".into();
        run.launch = Some(launch.clone());
        run.revision = (self.tab.file_epoch, self.tab.revision);
        run.started = iced::time::Instant::now();
        let serial = run.serial;
        let events = iced::futures::StreamExt::flatten(iced::futures::stream::once(async move {
            job::start(launch, plan, drawn_bonds, images, cancel)
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
            Event::Neb { iteration } => live.iteration = Some(iteration),
            Event::Saddle { step } => live.step = Some(step),
            Event::Irc { forward, step } => {
                live.forward = forward;
                live.step = Some(step);
            }
            Event::Done(Ok(report)) => {
                let request = live.request.clone();
                let attempt = run.attempt();
                let Some(launch) = run.launch.take() else {
                    return;
                };
                let job::Report {
                    mut result,
                    frames,
                    extras,
                    info,
                } = Arc::unwrap_or_clone(report);
                result.keep_full = run.keep_full;
                if let Some(summary) = &mut result.run {
                    let mut diagnostics = extras.diagnostics.clone();
                    if !extras.worker_frequencies.is_empty() {
                        diagnostics.push((
                            "Worker frequencies (cm⁻¹)".into(),
                            extras
                                .worker_frequencies
                                .iter()
                                .map(|value| format!("{value:.1}"))
                                .collect::<Vec<_>>()
                                .join(", "),
                        ));
                    }
                    summary.transition_state = Some(Box::new(TransitionStateRun {
                        arrow: Some(request.arrow),
                        saddle_converged: extras.saddle_converged,
                        imaginary: extras.num_imaginary,
                        imaginary_cm1: extras.engine_imaginary,
                        climbing_image: extras.climbing,
                        mapping: request.bonds.0,
                        diagnostics,
                        context: Some(request.context),
                    }));
                }
                let session = Rc::new(Session {
                    extras,
                    arrow: request.arrow,
                    freshness: Cell::new(None),
                });
                let line = view::presentation(&result, Some(&session)).status_line();
                let pending = super::Pending {
                    pushed: false,
                    accepted: None,
                    launch,
                    frames,
                    result,
                    line,
                    notes: vec![],
                    info,
                    attempt: Some(attempt),
                };
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
