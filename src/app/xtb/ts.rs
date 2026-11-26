//! Transition-state searches and reaction-path previews from the reaction arrow.
#[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
use super::Action as XtbAction;
use super::{is, state};
use crate::app::{App, Message};
use iced::Task;
use iced::time::Instant;
use reshiki::document::Document;
use reshiki::qcdata::{
    CalcResult, Frame, Task as CalcTask, TransitionStateContext, TransitionStateId,
    TransitionStateRun, XtbMethod,
};
use reshiki::reactions::ts_plan::{self, TsPlan};
use reshiki::reactions::{Reaction, Role};
use reshiki::science::ScienceError;
use sha2::{Digest, Sha256};
use std::cell::Cell;
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::{Arc, Weak};
use std::time::Duration;

pub(super) mod card;
pub(super) mod energy;
pub(in crate::app) mod export;
pub(super) mod irc;
#[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
mod job;
mod refine;
pub(in crate::app) mod review;
pub(in crate::app) mod thumbnails;
pub(super) mod view;

#[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
pub(super) use job::Event;

#[derive(Clone, Debug)]
pub(crate) enum Action {
    Card(card::Action),
    Calculate(u64),
    RefineGxtb(u64, Arc<CalcResult>),
    Review(review::Mode, Option<u64>),
    ReviewMap(u64),
    ReviewSheet(review::Action),
    Retry(u64, Retry),
    ShowWhere(u64),
    ReachedEndpoint,
    Image(usize),
    Profile(irc::Profile),
    PlayIrc,
    IrcTick,
    ExportIrc,
    Export,
    Tick,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Retry {
    FinerPath,
    RefineHighest,
    Gxtb(TransitionStateId),
}

#[derive(Clone)]
pub(super) struct Request {
    pub method: XtbMethod,
    pub arrow: u64,
    pub free_energy: bool,
    bonds: card::BondChanges,
    pub context: TransitionStateContext,
    #[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
    plan: TsPlan,
    #[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
    pub images: usize,
    retry: Option<Retry>,
}

impl Request {
    pub(super) fn refined_from(&self) -> Option<TransitionStateId> {
        match self.retry {
            Some(Retry::Gxtb(parent)) => Some(parent),
            _ => None,
        }
    }
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
    Thermochemistry,
}

impl Stage {
    fn step_index(self) -> usize {
        match self {
            Self::Endpoints => 0,
            Self::Idpp | Self::Neb => 1,
            Self::Saddle => 2,
            Self::Irc => 3,
            Self::Thermochemistry => 4,
        }
    }
}

const STEP_LABELS: [&str; 5] = [
    "Building both ends",
    "Finding a path",
    "Refining the transition state",
    "Checking where it leads",
    "Frequencies at both ends",
];

#[derive(Debug, PartialEq, Eq)]
enum StepState {
    Done(Option<Duration>),
    Current(Duration),
    Skipped,
    Todo,
}

pub(super) struct Live {
    pub request: Request,
    pub energies: Vec<f64>,
    reused_path: bool,
    irc_skipped: bool,
    stage: Stage,
    stage_started: Instant,
    finished_steps: [Option<Duration>; STEP_LABELS.len()],
    iteration: Option<usize>,
    step: Option<usize>,
    forward: bool,
}

impl Live {
    // These are the engine's default iteration limits.
    const NEB_LIMIT: usize = 300;
    const SADDLE_LIMIT: usize = 100;

    pub(super) fn new(request: Request) -> Self {
        let reused_path = request.retry == Some(Retry::RefineHighest);
        Self {
            request,
            energies: Vec::new(),
            reused_path,
            irc_skipped: false,
            stage: if reused_path {
                Stage::Saddle
            } else {
                Stage::Endpoints
            },
            stage_started: Instant::now(),
            finished_steps: [None; STEP_LABELS.len()],
            iteration: None,
            step: None,
            forward: true,
        }
    }

    #[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
    fn advance(&mut self, stage: Stage, now: Instant) {
        if self.stage.step_index() != stage.step_index() {
            if let Some(elapsed) = self.finished_steps.get_mut(self.stage.step_index()) {
                *elapsed.get_or_insert(Duration::ZERO) +=
                    now.saturating_duration_since(self.stage_started);
            }
            self.stage_started = now;
            self.step = None;
        }
        self.stage = stage;
    }

    fn steps(&self, now: Instant) -> impl Iterator<Item = (&'static str, StepState)> + '_ {
        let len = 4 + usize::from(self.request.free_energy);
        STEP_LABELS
            .into_iter()
            .zip(self.finished_steps)
            .take(len)
            .enumerate()
            .map(move |(index, (label, elapsed))| {
                let state = if index == self.stage.step_index() {
                    StepState::Current(
                        elapsed.unwrap_or_default()
                            + now.saturating_duration_since(self.stage_started),
                    )
                } else if (index == Stage::Neb.step_index()
                    && self.request.refined_from().is_some())
                    || (index == Stage::Irc.step_index() && self.irc_skipped && elapsed.is_none())
                {
                    StepState::Skipped
                } else if let Some(elapsed) = elapsed {
                    StepState::Done(Some(elapsed))
                } else if index < Stage::Saddle.step_index() && self.reused_path {
                    StepState::Done(None)
                } else {
                    StepState::Todo
                };
                (label, state)
            })
    }

    pub fn line(&self) -> String {
        let label = STEP_LABELS
            .get(self.stage.step_index())
            .copied()
            .unwrap_or_default();
        if self.stage == Stage::Neb
            && let Some(iteration) = self.iteration
        {
            format!("{label} · iteration {iteration}")
        } else {
            label.into()
        }
    }

    fn small_reaction(&self) -> bool {
        #[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
        return self.request.plan.atoms <= 30;
        #[cfg(not(all(not(target_arch = "wasm32"), feature = "xtb")))]
        false
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
                branch
                    + 0.1
                        * part(
                            self.step,
                            reshiki_optimize::ts::TsOptions::default().irc_max_steps,
                        )
            }
            Stage::Thermochemistry => 0.99,
        })
    }
}

/// Worker data for storing the result and previewing the NEB band in this session.
#[derive(Clone, Debug)]
struct Extras {
    #[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
    endpoint_energies_hartree: [f64; 2],
    #[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
    endpoint_thermo: Option<[Option<reshiki::qcdata::Thermo>; 2]>,
    image_positions: Vec<Vec<[f64; 3]>>,
    #[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
    neb_converged: bool,
    #[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
    candidate: Option<reshiki_optimize::ts::SaddleCandidate>,
    #[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
    reached_positions_angstrom: Vec<[f64; 3]>,
    saddle_converged: bool,
    num_imaginary: usize,
    diagnostics: Vec<(String, String)>,
    #[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
    irc: Option<reshiki::qcdata::IrcPath>,
    connects: bool,
    #[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
    forming: Vec<[usize; 2]>,
    #[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
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

    fn can_refine(&self, result: &CalcResult) -> bool {
        #[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
        return self.refinement_candidate(result).is_some();
        #[cfg(not(all(not(target_arch = "wasm32"), feature = "xtb")))]
        {
            let _ = result;
            false
        }
    }

    #[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
    fn refinement_candidate(
        &self,
        result: &CalcResult,
    ) -> Option<&reshiki_optimize::ts::SaddleCandidate> {
        if matches!(
            &result.status,
            reshiki::qcdata::Status::NotConverged { reason }
                | reshiki::qcdata::Status::Failed { reason }
                if reason.starts_with("engine/worker imaginary frequency counts disagree")
        ) {
            return None;
        }
        let candidate = self.extras.candidate.as_ref()?;
        let energies = path_energies(result);
        let atoms = result.trajectory.as_ref()?.numbers.len();
        let dimensions = atoms.checked_mul(3)?;
        (self.extras.num_imaginary >= 2
            && atoms > 0
            && saved_search(result).is_none_or(|search| search.arrow == Some(self.arrow))
            && (5..=24).contains(&energies.len())
            && self.extras.image_positions.len() == energies.len()
            && self
                .extras
                .image_positions
                .iter()
                .all(|image| image.len() == atoms)
            && candidate.positions.len() == dimensions
            && candidate.hessian.len() == dimensions.checked_mul(dimensions)?)
        .then_some(candidate)
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

    pub(super) fn xtb_ts_reference(
        &self,
        result: &Arc<CalcResult>,
        arrow: Option<u64>,
    ) -> Option<&'static str> {
        let complex = saved_search(result)
            .and_then(|search| search.context.as_ref()?.endpoint_complex)
            .or_else(|| {
                let arrow = arrow.filter(|_| !self.xtb_ts_search_stale(result))?;
                Some(self.xtb_reaction_label(arrow).has_multiple_species())
            });
        (complex == Some(true)).then_some(view::OPTIMIZED_COMPLEX)
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

fn optimized_reactant_energy(result: &CalcResult) -> Option<f64> {
    saved_search(result)
        .and_then(|search| search.endpoint_energies_hartree.map(|[r, _]| r))
        .or_else(|| path_energies(result).first().copied())
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

/// Copy a NEB path image's coordinates, retaining the starting topology.
/// Reached endpoints use their own perceived graph instead.
pub(super) fn preview_image(
    doc: &mut Document,
    atoms: &[Option<u64>],
    input: &Frame,
    image: &Frame,
) -> Result<(), ScienceError> {
    preview_model(doc, atoms, input, image).map(|_| ())
}

fn preview_model<'a>(
    doc: &'a mut Document,
    atoms: &[Option<u64>],
    input: &Frame,
    image: &Frame,
) -> Result<&'a mut reshiki::models3d::Model3d, ScienceError> {
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
    Ok(model)
}

/// An endpoint uses the search's covalent graph, including transferred or
/// disconnected hydrogens. Geometry-only atoms let the preview render every
/// perceived pair without forcing each added H to keep a parent bond.
pub(super) fn preview_endpoint_image(
    doc: &mut Document,
    atoms: &[Option<u64>],
    input: &Frame,
    image: &Frame,
) -> Result<(), ScienceError> {
    let positions: Vec<_> = image
        .positions_angstrom
        .iter()
        .flatten()
        .map(|value| value / reshiki_optimize::ANGSTROM_PER_BOHR)
        .collect();
    let bonds = reshiki_optimize::ts::bonds(&image.numbers, &positions)
        .map_err(|_| ScienceError::ModelMismatch)?;
    let model = preview_model(doc, atoms, input, image)?;
    let mut sources = model.geometry_sources();
    sources.extend(std::iter::repeat_n(None, model.hydrogens.len()));
    for hydrogen in model.hydrogens.drain(..) {
        model.atoms.push(reshiki::models3d::Placed {
            id: model.atoms.len() as u64,
            xyz: hydrogen.xyz,
        });
    }
    for (index, atom) in model.atoms.iter_mut().enumerate() {
        atom.id = index as u64;
    }
    model.generated = Some(reshiki::models3d::Generated {
        sources,
        elements: image
            .numbers
            .iter()
            .map(|&number| state::symbol(number).to_owned())
            .collect(),
        bonds: bonds.into_iter().map(|(a, b)| [a, b]).collect(),
    });
    // This transient geometry has its own perceived graph and geometry-only H.
    // It is not a generated conformer awaiting an update from the drawing.
    model.fingerprint = None;
    Ok(())
}

impl App {
    pub(super) fn xtb_live_ts(&self) -> Option<&Live> {
        self.tab.xtb.run.as_ref()?.ts.as_ref()
    }

    pub(super) fn xtb_ts_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Card(action) => self.xtb_ts_card_action(action),
            Action::Calculate(arrow) => self.xtb_calculate_reaction(arrow),
            Action::RefineGxtb(arrow, source) => self.xtb_refine_gxtb(arrow, &source),
            Action::Review(mode, arrow) => self.xtb_review_open(mode, arrow),
            Action::ReviewMap(arrow) => {
                self.xtb_review_show(review::Mode::TransitionState, Some(arrow))
            }
            Action::ReviewSheet(action) => self.xtb_review_action(action),
            Action::Retry(arrow, retry) => {
                let retry =
                    if retry == Retry::RefineHighest && !self.xtb_ts_refinement_available(arrow) {
                        Retry::FinerPath
                    } else {
                        retry
                    };
                self.xtb_review_retry(arrow, retry)
            }
            Action::ShowWhere(arrow) => {
                let card::ArrowResult::Found(result) = self.xtb_ts_card_result(arrow) else {
                    return Task::none();
                };
                if self.xtb_running()
                    || !view::has_reached_endpoint(result)
                    || self.xtb_ts_result_stale(result)
                {
                    return Task::none();
                }
                if self.xtb_show_reaction_path(arrow) {
                    self.xtb_end_preview();
                    self.xtb_preview_reached_endpoint();
                }
                Task::none()
            }
            Action::ReachedEndpoint => {
                self.xtb_preview_reached_endpoint();
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
            Action::Profile(profile) => {
                self.xtb_set_profile(profile);
                Task::none()
            }
            Action::PlayIrc => {
                self.xtb_play_irc();
                Task::none()
            }
            Action::IrcTick => {
                self.xtb_irc_tick();
                Task::none()
            }
            Action::ExportIrc => self.xtb_export_irc(),
            Action::Export => self.xtb_ts_export(),
            Action::Tick => Task::none(),
        }
    }

    fn xtb_ts_can_rerun(&self, shown: &super::palette::Shown<'_>) -> bool {
        super::available() && !self.xtb_running() && shown.arrow.is_some()
    }

    fn xtb_ts_result_stale(&self, result: &Arc<CalcResult>) -> bool {
        let doc = &self.tab.doc;
        doc.science
            .results
            .iter()
            .find(|stored| Arc::ptr_eq(result, &stored.result))
            .and_then(|stored| doc.science.attachment(stored.attachment))
            .is_none_or(|link| link.freshness(doc) == reshiki::science::Freshness::Stale)
            || self.xtb_ts_search_stale(result)
    }

    fn xtb_ts_refinement_available(&self, arrow: u64) -> bool {
        let card::ArrowResult::Found(result) = self.xtb_ts_card_result(arrow) else {
            return false;
        };
        self.xtb_ts_refinement_session(result)
            .is_some_and(|session| {
                view::presentation(result, Some(session)).recovery == view::Recovery::RefineHighest
            })
    }

    fn xtb_ts_refinement_session(&self, result: &Arc<CalcResult>) -> Option<&Session> {
        let session = self
            .tab
            .xtb
            .ts_sessions
            .get(result)
            .filter(|session| session.can_refine(result))?;
        if self.xtb_ts_search_stale(result) {
            return None;
        }
        let doc = &self.tab.doc;
        let stored = doc
            .science
            .results
            .iter()
            .find(|stored| Arc::ptr_eq(&stored.result, result))?;
        let link = doc.science.attachment(stored.attachment)?;
        let input = doc.science.frame(&link.frame)?;
        (link.freshness(doc) == reshiki::science::Freshness::Current
            && super::model_shows(doc, &link.atoms, &input.positions_angstrom))
        .then_some(session.as_ref())
    }

    fn xtb_ts_begin(
        &mut self,
        arrow: u64,
        plan: TsPlan,
        images: usize,
        (charge, unpaired): (i32, u32),
        free_energy: bool,
        retry: Option<Retry>,
    ) -> Task<Message> {
        #[cfg(not(all(not(target_arch = "wasm32"), feature = "xtb")))]
        let _ = images;
        if !super::available() || !self.xtb_can_start() {
            return Task::none();
        }
        let method = match retry {
            Some(Retry::Gxtb(_)) => XtbMethod::GXtb,
            Some(_) => match self.xtb_ts_card_result(arrow) {
                card::ArrowResult::Found(result) => saved_search(result)
                    .map_or_else(|| self.xtb.settings.barrier_method, |search| search.method),
                _ => self.xtb.settings.barrier_method,
            },
            None => self.xtb.settings.barrier_method,
        };
        let config = state::Config {
            // The run's electronic state and initial geometry use the
            // ordinary xTB launch. The TS pipeline drives the actual task.
            task: state::Task::Energy,
            method,
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
        bonds.0.automatic = match retry {
            Some(Retry::Gxtb(parent)) => self
                .xtb_ts_source(arrow, parent)
                .and_then(|stored| saved_search(&stored.result))
                .is_some_and(|source| source.mapping.automatic),
            _ => self
                .tab
                .xtb
                .ts_card
                .automatic_map((self.tab.file_epoch, arrow), &plan),
        };
        let Ok(reaction) = reshiki::reaction_mapping::reaction(&self.tab.doc, arrow) else {
            return Task::none();
        };
        let label = self.xtb_reaction_label(arrow);
        let context = TransitionStateContext {
            reactant_chemistry: endpoint_chemistry(&self.tab.doc, reaction, Role::Reactant),
            product_chemistry: endpoint_chemistry(&self.tab.doc, reaction, Role::Product),
            title: label.title(),
            reactant: label.first_reactant().unwrap_or("Reaction").to_owned(),
            endpoint_complex: Some(plan.shape != (1, 1)),
        };
        self.xtb_begin(super::Request {
            atoms: plan.start_atoms.clone(),
            config,
            keep_full: false,
            start: None,
            search: None,
            ts: Some(Request {
                method,
                arrow,
                free_energy,
                bonds,
                context,
                #[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
                plan,
                #[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
                images,
                retry,
            }),
        })
    }
}

#[cfg(all(test, not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
mod tests;

#[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
impl App {
    fn xtb_ts_retry_path(
        &self,
        request: &Request,
        launch: &reshiki::science::Launch,
    ) -> Option<job::SearchPath> {
        match request.retry {
            None => return Some(job::SearchPath::New(request.images)),
            Some(Retry::FinerPath) => return Some(job::SearchPath::Finer(request.images)),
            Some(Retry::Gxtb(parent)) => return self.xtb_gxtb_path(request.arrow, parent),
            Some(Retry::RefineHighest) => {}
        }
        let card::ArrowResult::Found(result) = self.xtb_ts_card_result(request.arrow) else {
            return Some(job::SearchPath::Finer(request.images));
        };
        let energies = path_energies(result);
        let fallback = || job::SearchPath::Finer((energies.len() + 5).clamp(request.images, 24));
        let Some(session) = self.tab.xtb.ts_sessions.get(result) else {
            return Some(fallback());
        };
        let Some(candidate) = session.refinement_candidate(result) else {
            return Some(fallback());
        };
        let images = &session.extras.image_positions;
        if saved_search(result).is_none_or(|search| search.method != request.method)
            || result.input_frame != Some(launch.frame.hash())
            || self.xtb_ts_search_stale(result)
            || !(5..=24).contains(&energies.len())
            || images.len() != energies.len()
            || images
                .iter()
                .any(|image| image.len() != launch.frame.numbers.len())
        {
            return Some(fallback());
        }
        Some(job::SearchPath::Saved {
            points: images
                .iter()
                .zip(energies)
                .map(|(image, &energy)| reshiki_optimize::ts::Minimum {
                    positions: job::bohr(image),
                    energy,
                })
                .collect(),
            converged: session.extras.neb_converged,
            candidate: candidate.clone(),
        })
    }

    pub(super) fn xtb_ts_start(&mut self, launch: reshiki::science::Launch) -> Task<Message> {
        let Some(live) = self.xtb_live_ts() else {
            return Task::none();
        };
        let Some(path) = self.xtb_ts_retry_path(&live.request, &launch) else {
            self.xtb_fail(state::Failure::Other(
                "The saved transition state or its endpoints are unavailable.".into(),
            ));
            return Task::none();
        };
        let request = live.request.clone();
        let plan = &request.plan;
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
        let Some(run) = self.tab.xtb.run.as_mut() else {
            return Task::none();
        };
        if let Some(live) = &mut run.ts {
            if let job::SearchPath::Saved { points, .. } = &path {
                live.energies = points.iter().map(|point| point.energy).collect();
            } else if live.reused_path {
                // The starting frame changed between review and launch.
                live.reused_path = false;
                live.stage = Stage::Endpoints;
                live.stage_started = Instant::now();
            }
        }
        let cancel = run.cancel.clone();
        run.label = format!("{} · transition state", request.method.label());
        run.launch = Some(launch.clone());
        run.revision = (self.tab.file_epoch, self.tab.revision);
        run.started = iced::time::Instant::now();
        let serial = run.serial;
        let events = iced::futures::StreamExt::flatten(iced::futures::stream::once(async move {
            job::start(launch, request, drawn_bonds, path, cancel)
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
            Event::Stage(stage) => live.advance(stage, Instant::now()),
            Event::IrcSkipped => live.irc_skipped = true,
            Event::Neb {
                iteration,
                energies,
            } => {
                live.iteration = Some(iteration);
                live.energies = energies;
            }
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
                    mut extras,
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
                        method: request.method,
                        refined_from: request.refined_from(),
                        arrow: Some(request.arrow),
                        free_energy: request.free_energy,
                        endpoint_energies_hartree: Some(extras.endpoint_energies_hartree),
                        endpoint_thermo: extras.endpoint_thermo.clone(),
                        reached_positions_angstrom: extras.reached_positions_angstrom.clone(),
                        saddle_converged: extras.saddle_converged,
                        imaginary: extras.num_imaginary,
                        imaginary_cm1: extras.engine_imaginary,
                        climbing_image: extras.climbing,
                        irc: extras.irc.take(),
                        forming_bonds: std::mem::take(&mut extras.forming),
                        breaking_bonds: std::mem::take(&mut extras.breaking),
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
                let line = view::presentation(&result, Some(&session))
                    .status_line(self.appearance.energy_unit);
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
