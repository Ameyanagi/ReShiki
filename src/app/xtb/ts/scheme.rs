//! Calculate a selected reaction path. The map for each arrow is reviewed on
//! its own copy so a shared intermediate can have different maps in each step.
// The setup and result views share types with the optional native worker.
#![cfg_attr(
    not(all(feature = "xtb", not(target_arch = "wasm32"))),
    allow(dead_code)
)]
use super::Action as TsAction;
use crate::app::xtb::{self, Page};
use crate::app::{App, InspectorTab, Message};
use iced::Task;
use iced::keyboard::{Key, Modifiers, key::Named};
use reshiki::qcdata::XtbMethod;
use std::cell::Cell;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};

mod drawing;
pub(super) mod plan;
pub(super) mod profile;
pub(super) mod run;
#[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
mod store;
mod view;

use plan::Plan;
use run::{StepOptions, TsInput};

static NEXT_REVIEW: AtomicU64 = AtomicU64::new(0);
const BARRIER_SKIPPED: &str = "Barrier skipped:";

#[derive(Clone, Copy, Debug)]
pub(crate) enum Quantity {
    Energy,
    Barrier,
    FreeEnergy,
    FreeBarrier,
}

#[derive(Clone, Debug)]
pub(crate) enum Action {
    Open,
    Close,
    Toggle(usize, Quantity),
    Method(XtbMethod),
    Run,
    Cancel,
    Retry(usize),
    Next(u64),
    Place,
    PlaceDiscovered,
    #[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
    Event(Box<run::Event>),
}

fn send(action: Action) -> Message {
    Message::Xtb(xtb::Action::Ts(TsAction::Scheme(action)))
}

pub(in crate::app) fn open_message() -> Message {
    send(Action::Open)
}

pub(in crate::app) fn modal_key(key: &Key, modifiers: Modifiers) -> Option<Message> {
    match key {
        Key::Named(Named::Escape) if modifiers.is_empty() => Some(send(Action::Close)),
        Key::Named(Named::Enter) if modifiers.command() => Some(send(Action::Run)),
        _ => None,
    }
}

struct MapReviews {
    id: u64,
    pending: VecDeque<usize>,
    active: Option<usize>,
    inputs: Vec<Result<Option<TsInput>, String>>,
    retry: Option<usize>,
}

pub(in crate::app::xtb) struct State {
    plan: Plan,
    name_requests: Vec<(String, Vec<u64>)>,
    refresh_names: bool,
    options: Vec<StepOptions>,
    method: XtbMethod,
    sheet: bool,
    original: Option<(Vec<StepOptions>, XtbMethod)>,
    reviews: Option<MapReviews>,
    work: run::State,
    epoch: u64,
    revision: u64,
    checked: Cell<Option<(u64, bool)>>,
    #[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
    pending_save: Option<bool>,
}

impl State {
    fn follows(
        &self,
        doc: &reshiki::document::Document,
        figure: &reshiki::graphics::EnergyProfile,
    ) -> bool {
        figure.linked
            && figure.source == self.plan.arrows.first().copied()
            && super::profile::source_reactions(doc, figure).is_some_and(|steps| {
                let mut arrows: Vec<_> = steps.iter().map(|step| step.arrow).collect();
                arrows.dedup();
                arrows == self.plan.arrows
            })
    }

    fn busy(&self) -> bool {
        self.reviews.is_some() || self.work.running()
    }

    #[cfg(all(test, not(target_arch = "wasm32"), feature = "xtb"))]
    pub(super) fn reviewed_step(&self, index: usize) -> Option<(StepOptions, &[String])> {
        Some((
            *self.options.get(index)?,
            &self.plan.steps.get(index)?.notes,
        ))
    }

    fn discard_choices(&mut self) {
        if let Some((options, method)) = self.original.take() {
            for (step, option) in self.plan.steps.iter_mut().zip(&options) {
                if option.transition_state() {
                    step.notes.retain(|note| !note.starts_with(BARRIER_SKIPPED));
                }
            }
            self.options = options;
            self.method = method;
        }
    }
}

impl App {
    pub(in crate::app) fn xtb_scheme_current(&self) -> bool {
        let Some(state) = &self.tab.xtb.scheme else {
            return false;
        };
        if state.epoch != self.tab.file_epoch {
            return false;
        }
        if state.revision == self.tab.revision {
            return true;
        }
        if let Some((revision, current)) = state.checked.get()
            && revision == self.tab.revision
        {
            return current;
        }
        let current = state.plan.current(&self.tab.doc);
        state.checked.set(Some((self.tab.revision, current)));
        current
    }

    pub(in crate::app) fn xtb_scheme_selection(&self) -> bool {
        self.tab
            .doc
            .arrows
            .iter()
            .filter(|arrow| {
                self.tab.selected.contains(&arrow.id)
                    && arrow.purpose.is_chemistry()
                    && reshiki::arrows::Preset::from_kind(&arrow.kind)
                        .reaction_role_refusal()
                        .is_none()
            })
            .take(2)
            .count()
            == 2
    }

    pub(in crate::app) fn xtb_scheme_selected(&self) -> bool {
        let Some(state) = &self.tab.xtb.scheme else {
            return false;
        };
        if state.epoch != self.tab.file_epoch {
            return false;
        }
        state.busy()
            || state.sheet
            || (state
                .plan
                .arrows
                .iter()
                .all(|arrow| self.tab.selected.contains(arrow))
                && self
                    .tab
                    .doc
                    .arrows
                    .iter()
                    .filter(|arrow| self.tab.selected.contains(&arrow.id))
                    .all(|arrow| state.plan.arrows.contains(&arrow.id)))
            || self
                .selected_energy_profile()
                .and_then(|graphic| graphic.energy_profile.as_deref())
                .is_some_and(|profile| state.follows(&self.tab.doc, profile))
    }

    pub(in crate::app) fn xtb_can_open_scheme(&self) -> bool {
        xtb::available()
            && !self.xtb_running()
            && !self.ts_review_open()
            && !self.tab.xtb.energy_state.in_progress()
            && !self.tab.busy
            && self.tab.optimization.is_none()
            && self.tab.model_job.is_none()
            && !self.tab.drawing_edit_active()
    }

    pub(in crate::app) fn xtb_scheme_open_action(&self) -> Option<Message> {
        self.xtb_can_open_scheme().then(open_message)
    }

    pub(in crate::app) fn scheme_sheet_visible(&self) -> bool {
        self.tab
            .xtb
            .scheme
            .as_ref()
            .is_some_and(|state| state.sheet)
    }

    pub(in crate::app::xtb) fn xtb_scheme_running(&self) -> bool {
        self.tab.xtb.scheme.as_ref().is_some_and(State::busy)
    }

    /// Scheme labels share the card's local naming queue without adding
    /// captions or a drawing revision. Built-in balancing species already
    /// have names and never request one.
    pub(in crate::app) fn xtb_scheme_name_request(&self) -> Option<(String, Vec<u64>)> {
        let state = self.tab.xtb.scheme.as_ref()?;
        if self.ts_review_visible()
            || !self.xtb_scheme_current()
            || !(state.sheet
                || self.xtb_scheme_palette_visible()
                || (self.tabs.temporary_depth == 0
                    && self.inspector_open
                    && self.inspector_tab == InspectorTab::Predict
                    && self.xtb.page == Page::Xtb
                    && !self.tube_bend_active()
                    && self.tab.equipment_authoring.is_none()
                    && self.xtb_scheme_selected()))
        {
            return None;
        }
        state
            .name_requests
            .iter()
            .find(|(smiles, _)| {
                !self.tab.naming.caption_names.contains_key(smiles)
                    && !self.tab.naming.caption_failed.contains_key(smiles)
            })
            .cloned()
    }

    pub(in crate::app) fn xtb_schedule_scheme_name_refresh(&mut self) {
        if let Some(state) = &mut self.tab.xtb.scheme {
            state.refresh_names = true;
        }
    }

    pub(in crate::app) fn xtb_refresh_scheme_names(&mut self) {
        let Some(state) = self
            .tab
            .xtb
            .scheme
            .as_ref()
            .filter(|state| state.refresh_names)
        else {
            return;
        };
        if !self.xtb_scheme_current() {
            return;
        }
        let labels = |plan: &Plan| {
            plan.levels().map(|levels| {
                levels
                    .into_iter()
                    .map(|level| {
                        super::profile::state(
                            reshiki::graphics::EnergyProfileStateKind::Reactant,
                            level.label(),
                            0.,
                        )
                        .label
                    })
                    .collect::<Vec<_>>()
            })
        };
        let previous = labels(&state.plan);
        let names: std::collections::BTreeMap<_, _> = state
            .plan
            .species
            .iter()
            .flat_map(|species| &species.instances)
            .filter_map(|atoms| {
                Some((
                    atoms.clone(),
                    self.xtb_existing_molecule_name(atoms)?.to_owned(),
                ))
            })
            .collect();
        if let Some(state) = self.tab.xtb.scheme.as_mut() {
            state
                .plan
                .resolve_names(&self.tab.doc, |atoms| names.get(atoms).cloned());
            state.refresh_names = false;
        }
        let Some(state) = self.tab.xtb.scheme.as_ref() else {
            return;
        };
        let (Some(previous), Some(labels)) = (previous, labels(&state.plan)) else {
            return;
        };
        if previous == labels {
            return;
        }
        let linked: Vec<_> = self
            .tab
            .doc
            .graphics
            .iter()
            .enumerate()
            .filter_map(|(index, graphic)| {
                let figure = graphic.energy_profile.as_deref()?;
                if !state.follows(&self.tab.doc, figure) {
                    return None;
                }
                let reactions = super::profile::source_reactions(&self.tab.doc, figure)?;
                let minima = figure
                    .steps
                    .first()
                    .map(|step| step.from)
                    .into_iter()
                    .chain(figure.steps.iter().zip(&reactions).enumerate().filter_map(
                        |(leg, (step, reaction))| {
                            reactions
                                .get(leg + 1)
                                .is_none_or(|next| next.arrow != reaction.arrow)
                                .then_some(step.to)
                        },
                    ))
                    .collect::<Vec<_>>();
                Some((index, minima))
            })
            .collect();
        // Like automatic property captions, a late presentation name belongs
        // to the existing figure's edit. Keep its geometry, energies and style.
        let mut changed = false;
        for (index, minima) in linked {
            let Some(figure) = self
                .tab
                .doc
                .graphics
                .get_mut(index)
                .and_then(|graphic| graphic.energy_profile.as_deref_mut())
            else {
                continue;
            };
            for ((index, old), new) in minima.into_iter().zip(&previous).zip(&labels) {
                if let Some(state) = figure.states.get_mut(index)
                    && state.label == *old
                    && old != new
                {
                    state.label = new.clone();
                    changed = true;
                }
            }
        }
        if changed {
            self.tab.label_generation = self.tab.label_generation.wrapping_add(1);
        }
    }

    fn xtb_open_scheme(&mut self) -> Task<Message> {
        if self.scheme_sheet_visible() || !self.xtb_can_open_scheme() {
            return Task::none();
        }
        if self.naming_can_edit() {
            self.xtb_refresh_scheme_names();
        }
        let selected = self
            .tab
            .xtb
            .scheme
            .as_ref()
            .filter(|_| self.xtb_scheme_selected());
        let current = selected.is_some() && self.xtb_scheme_current();
        let plan = match selected {
            // Retained rows refer to this plan's species indices, including after
            // reordering arrows in the drawing. Keep those indices on reopening.
            Some(state) if current => Ok(state.plan.clone()),
            Some(state) if self.selected_energy_profile().is_some() => {
                Plan::read(&self.tab.doc, &state.plan.arrows)
            }
            _ => Plan::read(&self.tab.doc, &self.tab.selected),
        };
        let mut plan = match plan {
            Ok(plan) => plan,
            Err(reason) => {
                self.status = reason;
                self.error = true;
                return Task::none();
            }
        };
        plan.resolve_names(&self.tab.doc, |atoms| {
            self.xtb_existing_molecule_name(atoms).map(str::to_owned)
        });
        let name_requests = plan
            .species
            .iter()
            .flat_map(|species| &species.instances)
            .filter_map(|atoms| self.xtb_species_name_request(atoms))
            .collect();
        let state = &mut self.tab.xtb.scheme;
        if let Some(existing) = state.as_mut().filter(|_| current) {
            existing.original = Some((existing.options.clone(), existing.method));
            existing.plan = plan;
            existing.name_requests = name_requests;
            existing.refresh_names = false;
            existing.sheet = true;
            existing.revision = self.tab.revision;
            existing.checked.set(None);
        } else {
            let options = vec![StepOptions::default(); plan.steps.len()];
            *state = Some(State {
                original: Some((options.clone(), self.xtb.settings.barrier_method)),
                options,
                plan,
                name_requests,
                refresh_names: false,
                method: self.xtb.settings.barrier_method,
                sheet: true,
                reviews: None,
                work: run::State::default(),
                epoch: self.tab.file_epoch,
                revision: self.tab.revision,
                checked: Cell::default(),
                #[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
                pending_save: None,
            });
        }
        self.xtb.page = Page::Xtb;
        self.error = false;
        self.show_inspector_tab(InspectorTab::Predict)
            .unwrap_or_else(Task::none)
    }

    pub(in crate::app::xtb) fn xtb_scheme_action(&mut self, action: Action) -> Task<Message> {
        if matches!(action, Action::Open) {
            return self.xtb_open_scheme();
        }
        if matches!(action, Action::Run | Action::Retry(_)) {
            return self.xtb_prepare_scheme(match action {
                Action::Retry(i) => Some(i),
                _ => None,
            });
        }
        let Some(state) = self.tab.xtb.scheme.as_mut() else {
            return Task::none();
        };
        match action {
            Action::Close if state.sheet => {
                state.sheet = false;
                state.discard_choices();
                return crate::app::focus_drawing();
            }
            Action::Toggle(index, quantity) if state.sheet => {
                if let Some(options) = state.options.get_mut(index) {
                    let flag = match quantity {
                        Quantity::Energy => &mut options.reaction_energy,
                        Quantity::Barrier => &mut options.barrier,
                        Quantity::FreeEnergy => &mut options.free_energy,
                        Quantity::FreeBarrier => &mut options.free_barrier,
                    };
                    *flag = !*flag;
                }
            }
            Action::Method(method)
                if state.sheet && matches!(method, XtbMethod::Gfn2Xtb | XtbMethod::GXtb) =>
            {
                state.method = method;
            }
            Action::Cancel if state.busy() => {
                #[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
                let calculating = state.work.running();
                if state.reviews.take().is_some() {
                    self.tab.xtb.ts_review = None;
                    state.discard_choices();
                }
                state.work.stop();
                self.status = "Scheme calculation canceled · finished results kept".into();
                self.error = false;
                #[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
                if calculating && self.xtb_scheme_current() {
                    self.xtb_store_scheme(false);
                }
                return crate::app::focus_drawing();
            }
            Action::Next(id)
                if state
                    .reviews
                    .as_ref()
                    .is_some_and(|r| r.id == id && r.active.is_none()) =>
            {
                return self.xtb_scheme_next();
            }
            Action::Place => self.xtb_place_scheme_profile(),
            Action::PlaceDiscovered => self.xtb_place_discovered_scheme(),
            #[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
            Action::Event(event) => {
                if !state.work.accepts(&event) {
                    return Task::none();
                }
                let draining = !state.work.running();
                if state.epoch != self.tab.file_epoch
                    || (state.revision != self.tab.revision && !state.plan.current(&self.tab.doc))
                {
                    state.work.cancel();
                    if !draining {
                        self.status =
                            "The scheme changed. Select its arrows and calculate again.".into();
                    }
                    return Task::none();
                }
                state.revision = self.tab.revision;
                let finished = event.finished();
                if state.work.accept(*event) && finished {
                    if !draining {
                        self.status = if let Some(reason) = &state.work.error {
                            reason.clone()
                        } else {
                            let done = state
                                .work
                                .steps
                                .iter()
                                .filter(|s| matches!(s, run::StepStatus::Done))
                                .count();
                            let failed = state
                                .work
                                .steps
                                .iter()
                                .filter(|s| matches!(s, run::StepStatus::Failed(_)))
                                .count();
                            if failed == 0 {
                                format!("Scheme complete · {done} steps calculated")
                            } else {
                                format!("Scheme finished · {done} calculated · {failed} failed")
                            }
                        };
                    }
                    self.xtb_store_scheme(true);
                }
            }
            _ => {}
        }
        Task::none()
    }

    fn xtb_prepare_scheme(&mut self, retry: Option<usize>) -> Task<Message> {
        if !self.xtb_can_open_scheme() {
            return Task::none();
        }
        let Some(state) = self.tab.xtb.scheme.as_mut() else {
            return Task::none();
        };
        if !state.options.iter().any(|step| step.any()) || retry.is_none() && !state.sheet {
            return Task::none();
        }
        if let Some(index) = retry
            && !matches!(
                state.work.steps.get(index),
                Some(run::StepStatus::Failed(_) | run::StepStatus::Cancelled)
            )
        {
            return Task::none();
        }
        if state.epoch != self.tab.file_epoch || !state.plan.current(&self.tab.doc) {
            self.status = "The scheme changed. Select its arrows and calculate again.".into();
            self.error = true;
            state.sheet = false;
            return Task::none();
        }
        if retry.is_some() {
            // A replacement conformer leaves the scheme's chemistry snapshot current.
            state.plan.refresh_calculations(&self.tab.doc);
        }
        state.sheet = false;
        state.revision = self.tab.revision;
        state.reviews = Some(MapReviews {
            id: NEXT_REVIEW.fetch_add(1, Ordering::Relaxed),
            pending: state
                .options
                .iter()
                .enumerate()
                .filter(|(index, option)| {
                    retry.is_none_or(|i| i == *index) && option.transition_state()
                })
                .map(|(index, _)| index)
                .collect(),
            active: None,
            inputs: vec![Ok(None); state.options.len()],
            retry,
        });
        self.xtb_scheme_next()
    }

    fn xtb_scheme_next(&mut self) -> Task<Message> {
        let Some(state) = self.tab.xtb.scheme.as_mut() else {
            return Task::none();
        };
        let Some(reviews) = &mut state.reviews else {
            return Task::none();
        };
        if state.epoch != self.tab.file_epoch || state.revision != self.tab.revision {
            state.reviews = None;
            self.status = "The drawing changed. Review the scheme again.".into();
            return Task::none();
        }
        if let Some(index) = reviews.pending.pop_front() {
            reviews.active = Some(index);
            let Some((step, options)) = state.plan.steps.get(index).zip(state.options.get(index))
            else {
                return self.xtb_scheme_action(Action::Cancel);
            };
            let arrow = step.arrow;
            let free = options.free_barrier;
            let detached_map = step.has_added_copies();
            return match state.plan.review_document(index, &self.tab.doc) {
                Ok(doc) => self.xtb_review_scheme_step(index, doc, arrow, free, detached_map),
                Err(reason) => self.xtb_scheme_reviewed(index, Err(reason)),
            };
        }
        #[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
        if let Some(reviews) = state.reviews.take() {
            state.original = None;
            for (index, (step, options)) in
                state.plan.steps.iter_mut().zip(&state.options).enumerate()
            {
                if reviews.retry.is_none_or(|retry| retry == index) && options.transition_state() {
                    step.notes.retain(|note| !note.starts_with(BARRIER_SKIPPED));
                }
            }
            state
                .work
                .seed(&self.tab.doc, &state.plan, state.method, &state.options);
            let events = state.work.start(
                &state.plan,
                &state.options,
                state.method,
                reviews.inputs,
                self.xtb.settings.threads(),
                reviews.retry,
            );
            let palette = &mut self.tab.xtb.palette;
            palette.open = true;
            palette.collapsed = false;
            palette.view = xtb::results::View::Path;
            self.status = "Calculating reaction scheme…".into();
            self.error = false;
            return Task::run(events, |event| send(Action::Event(Box::new(event))));
        }
        state.reviews = None;
        Task::none()
    }

    pub(super) fn xtb_scheme_reviewed(
        &mut self,
        index: usize,
        input: Result<TsInput, String>,
    ) -> Task<Message> {
        match input {
            Ok(input) => self.xtb_scheme_finish_review(index, Ok(Some(input))),
            Err(reason) => {
                let added_hydrogen = self.tab.xtb.scheme.as_ref().is_some_and(|state| {
                    state.plan.steps.get(index).is_some_and(|step| {
                        step.reactants.iter().chain(&step.products).any(|term| {
                            state.plan.species.get(term.species).is_some_and(|species| {
                                species.atoms.is_empty() && species.formula == "H2"
                            })
                        })
                    })
                });
                let note = if added_hydrogen {
                    "Barrier skipped: the map could not include the added H₂".into()
                } else {
                    format!("{BARRIER_SKIPPED} {reason}")
                };
                self.xtb_scheme_skip_barrier_with_note(index, Some(note))
            }
        }
    }

    pub(super) fn xtb_scheme_skip_barrier(&mut self, index: usize) -> Task<Message> {
        self.xtb_scheme_skip_barrier_with_note(index, None)
    }

    fn xtb_scheme_skip_barrier_with_note(
        &mut self,
        index: usize,
        note: Option<String>,
    ) -> Task<Message> {
        let Some(state) = self.tab.xtb.scheme.as_mut() else {
            return Task::none();
        };
        if !state
            .reviews
            .as_ref()
            .is_some_and(|reviews| reviews.active == Some(index))
        {
            return Task::none();
        }
        let Some(options) = state.options.get_mut(index) else {
            return Task::none();
        };
        options.barrier = false;
        options.free_barrier = false;
        options.reaction_energy = true;
        if let Some(note) = note
            && let Some(step) = state.plan.steps.get_mut(index)
        {
            step.notes.retain(|note| !note.starts_with(BARRIER_SKIPPED));
            step.notes.push(note);
        }
        self.xtb_scheme_finish_review(index, Ok(None))
    }

    fn xtb_scheme_finish_review(
        &mut self,
        index: usize,
        input: Result<Option<TsInput>, String>,
    ) -> Task<Message> {
        let Some(reviews) = self
            .tab
            .xtb
            .scheme
            .as_mut()
            .and_then(|s| s.reviews.as_mut())
        else {
            return Task::none();
        };
        if reviews.active != Some(index) {
            return Task::none();
        }
        reviews.active = None;
        let Some(entry) = reviews.inputs.get_mut(index) else {
            return self.xtb_scheme_action(Action::Cancel);
        };
        *entry = input;
        Task::done(send(Action::Next(reviews.id)))
    }
}

#[cfg(all(test, not(target_arch = "wasm32"), feature = "xtb"))]
mod naming_tests;
#[cfg(all(test, not(target_arch = "wasm32"), feature = "xtb"))]
mod tests;
