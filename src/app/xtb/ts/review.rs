//! Review reaction participants, atom correspondence and electronic state before xTB.
use super::super::{
    Action as XtbAction, CalcTask, Page, available,
    tone::{Tone, line},
};
use super::{Action as TsAction, Retry, energy::Species, scheme};
use crate::app::reaction_mapping::{Preview, calculate, with_roles};
use crate::app::{App, InspectorTab, Message};
use crate::canvas::{BondChangeKind, BondChangeMark};
use iced::keyboard::{Key, Modifiers, key::Named};
use iced::{Element, Task};
use reshiki::document::Document;
use reshiki::reaction_mapping::Proposal;
use reshiki::reactions::{
    self,
    ts_plan::{self, MapReview, Side, TsPlan, TsPlanError},
};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
};

pub(super) mod species;
mod view;

pub(super) const SHAPE_NOTE: &str = "Transition-state search handles 1→1, 1→2 and 2→1 reactions. Reaction energy works for any stoichiometry.";
static NEXT_SHEET: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    TransitionState,
    Energy,
}

#[derive(Clone, Debug)]
pub(crate) enum Action {
    Ready(u64, Result<Arc<Proposal>, String>),
    Charge(usize, String),
    Unpaired(usize, String),
    EditSpecies,
    Advanced,
    PathPoints(String),
    Accept,
    EditMap,
    SkipBarrier,
    Cancel,
}

enum Mapping {
    Drawing,
    Finding,
    Proposed(Arc<Proposal>),
    Failed(String),
}

impl Mapping {
    fn proposal(&self) -> Option<&Arc<Proposal>> {
        match self {
            Self::Proposed(proposal) => Some(proposal),
            _ => None,
        }
    }
}

struct Changes {
    bonds: String,
    action: String,
    marks: Vec<BondChangeMark>,
    hydrogens: Vec<String>,
}

impl Changes {
    fn read(doc: &Document, arrow: u64, plan: Option<&TsPlan>) -> Result<Self, TsPlanError> {
        if let Some(plan) = plan {
            return Ok(Self::from_plan(doc, plan));
        }
        ts_plan::map_review(doc, arrow).map(|review| Self::new(doc, review))
    }

    fn new(doc: &Document, changes: MapReview) -> Self {
        let atom_label = |id| {
            doc.atom(id)
                .map_or_else(|| "?".into(), |a| format!("{}{}", a.element, a.map_num))
        };
        let bonds = |pairs: &[(u64, u64)]| {
            pairs
                .iter()
                .map(|&(a, b)| format!("{}–{}", atom_label(a), atom_label(b)))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let mapped_hydrogens: BTreeSet<_> = changes
            .form
            .iter()
            .chain(&changes.break_)
            .flat_map(|&(a, b)| [a, b])
            .filter(|id| doc.atom(*id).is_some_and(|atom| atom.element == "H"))
            .map(atom_label)
            .collect();
        let hydrogens = changes
            .hydrogen_moves
            .iter()
            .map(|&(from, to)| {
                format!(
                    "H from {} to {} (least motion)",
                    atom_label(from),
                    atom_label(to)
                )
            })
            .chain(
                mapped_hydrogens
                    .into_iter()
                    .map(|atom| format!("{atom} (mapped)")),
            )
            .collect();
        let mut caption = Vec::new();
        let mut action = Vec::new();
        let mut marks = Vec::new();
        for (pairs, kind, verb, present) in [
            (&changes.form, BondChangeKind::Form, "form", "forms"),
            (&changes.break_, BondChangeKind::Break, "break", "breaks"),
        ] {
            if !pairs.is_empty() {
                let labels = bonds(pairs);
                caption.push(format!("{present} {labels}"));
                action.push(
                    if let Some(first) = pairs.first().filter(|_| pairs.len() > 2) {
                        format!(
                            "{verb} {} and {} more",
                            bonds(std::slice::from_ref(first)),
                            pairs.len() - 1
                        )
                    } else {
                        format!("{verb} {labels}")
                    },
                );
                marks.extend(pairs.iter().map(|&(a, b)| BondChangeMark {
                    kind,
                    atoms: [a, b],
                    highlighted: false,
                }));
            }
        }
        if action.is_empty() && !changes.hydrogen_moves.is_empty() {
            action.push("transfer hydrogen".into());
        }
        Self {
            bonds: caption.join(" · "),
            action: action.join(", "),
            marks,
            hydrogens,
        }
    }

    fn from_plan(doc: &Document, plan: &TsPlan) -> Self {
        let (form, break_, hydrogen_moves) = match plan.start {
            Side::Reactant => (
                plan.form.clone(),
                plan.break_.clone(),
                plan.hydrogen_moves.clone(),
            ),
            Side::Product => (
                plan.break_.clone(),
                plan.form.clone(),
                plan.hydrogen_moves.iter().map(|&(a, b)| (b, a)).collect(),
            ),
        };
        Self::new(
            doc,
            MapReview {
                form,
                break_,
                hydrogen_moves,
            },
        )
    }

    fn marks_for_side(&self, doc: &Document, side: Side, atoms: &[u64]) -> Vec<BondChangeMark> {
        let by_map: BTreeMap<_, _> = atoms
            .iter()
            .filter_map(|id| doc.atom(*id))
            .filter(|atom| atom.map_num != 0)
            .map(|atom| (atom.map_num, atom.id))
            .collect();
        let matched = |id| by_map.get(&doc.atom(id)?.map_num).copied();
        self.marks
            .iter()
            .filter_map(|mark| {
                Some(BondChangeMark {
                    kind: match (side, mark.kind) {
                        (Side::Reactant, kind) => kind,
                        (Side::Product, BondChangeKind::Form) => BondChangeKind::Break,
                        (Side::Product, BondChangeKind::Break) => BondChangeKind::Form,
                    },
                    atoms: [matched(mark.atoms[0])?, matched(mark.atoms[1])?],
                    ..*mark
                })
            })
            .collect()
    }
}

struct Picture {
    label: &'static str,
    preview: Preview,
    marks: Vec<BondChangeMark>,
}

pub(in crate::app::xtb) struct Dialog {
    id: u64,
    scheme: Option<(usize, Arc<Document>)>,
    detached_map: bool,
    at: (u64, u64),
    arrow: u64,
    mode: Mode,
    retry: Option<Retry>,
    explicit: bool,
    energy_fallback: bool,
    inferred: bool,
    unmapped_isotope_hydrogen: bool,
    ignored: Option<String>,
    target_stereo_note: Option<&'static str>,
    inputs: Vec<species::Input>,
    path_points: String,
    edit_species: bool,
    advanced: bool,
    task: CalcTask,
    free_energy: bool,
    mapping: Mapping,
    pictures: Option<[Picture; 2]>,
    changes: Result<Changes, TsPlanError>,
    plan: Result<TsPlan, TsPlanError>,
}

struct ReviewOptions {
    mode: Mode,
    explicit: bool,
    retry: Option<Retry>,
    energy_fallback: bool,
    free_energy: bool,
}

impl Dialog {
    fn heading(&self) -> &'static str {
        match self.mode {
            Mode::Energy => "Calculate reaction energy",
            Mode::TransitionState => "Which bonds change?",
        }
    }

    fn subtitle(&self) -> &'static str {
        if self
            .mapping
            .proposal()
            .is_some_and(|proposal| proposal.multiple_best)
        {
            "Several atom maps fit this drawing. Check the proposal, or draw the atom map."
        } else if self.mode == Mode::Energy {
            "Review the atom correspondence and species charges."
        } else {
            "Check the bonds that change."
        }
    }

    fn run_label(&self) -> String {
        if self.scheme.is_some() {
            return "Accept atom map".into();
        }
        if self.mode == Mode::Energy {
            return self.heading().into();
        }
        match &self.changes {
            Ok(changes) if !changes.action.is_empty() => format!("Run: {}", changes.action),
            _ => "Run transition-state search".into(),
        }
    }

    fn state_summary(&self) -> String {
        let mut charged = 0;
        let mut open_shell = 0;
        for input in &self.inputs {
            let Ok(charge) = input.charge.trim().replace('−', "-").parse::<i32>() else {
                return "Check species charges and unpaired electrons".into();
            };
            let Ok(unpaired) = input.unpaired.trim().parse::<u32>() else {
                return "Check species charges and unpaired electrons".into();
            };
            charged += usize::from(charge != 0);
            open_shell += usize::from(unpaired != 0);
        }
        let charge = match charged {
            0 => "All species neutral".into(),
            1 => "1 charged species".into(),
            count => format!("{count} charged species"),
        };
        let spin = match open_shell {
            0 => "no unpaired electrons".into(),
            1 => "1 species with unpaired electrons".into(),
            count => format!("{count} species with unpaired electrons"),
        };
        format!("{charge}, {spin}")
    }

    fn pictures(&self, source: &Document) -> Option<[Picture; 2]> {
        let reaction = reactions::preview(source, self.arrow).ok()?;
        let start = self
            .plan
            .as_ref()
            .ok()
            .filter(|_| self.mode == Mode::TransitionState)
            .map(|plan| plan.start);
        let sides = match start {
            Some(Side::Product) => [Side::Product, Side::Reactant],
            _ => [Side::Reactant, Side::Product],
        };
        Some(sides.map(|side| {
            let participants = match side {
                Side::Reactant => &reaction.reactants,
                Side::Product => &reaction.products,
            };
            let atoms: Vec<_> = participants
                .iter()
                .flat_map(|participant| participant.atoms.iter().copied())
                .collect();
            Picture {
                label: match (side, start == Some(side)) {
                    (Side::Reactant, true) => "Reactants (search start)",
                    (Side::Product, true) => "Products (search start)",
                    (Side::Reactant, false) => "Reactants",
                    (Side::Product, false) => "Products",
                },
                preview: Preview::for_atoms(source, &atoms),
                marks: self.changes.as_ref().map_or_else(
                    |_| Vec::new(),
                    |changes| changes.marks_for_side(source, side, &atoms),
                ),
            }
        }))
    }

    fn images(&self) -> Result<usize, String> {
        if self.retry.is_some() {
            return Ok(20);
        }
        self.path_points
            .trim()
            .parse::<usize>()
            .ok()
            .filter(|points| (10..=20).contains(points))
            .ok_or_else(|| "Enter a whole number from 10 to 20 for the path points.".into())
    }

    fn refresh(&mut self, source: &Document, candidate: &Document) {
        self.plan = ts_plan::plan_for_review(candidate, self.arrow);
        let source = if self.energy_fallback && self.plan.is_err() {
            self.mode = Mode::Energy;
            source
        } else {
            candidate
        };
        let plan = self
            .plan
            .as_ref()
            .ok()
            .filter(|_| self.mode == Mode::TransitionState);
        self.changes = Changes::read(source, self.arrow, plan);
        self.pictures = self.pictures(source);
    }

    fn barrier_refusal(&self) -> Option<String> {
        if !self.energy_fallback || self.mode != Mode::Energy {
            return None;
        }
        match &self.mapping {
            Mapping::Failed(error) => Some(error.clone()),
            _ => self.plan.as_ref().err().map(ToString::to_string),
        }
    }

    fn can_run(&self, species: &[Species]) -> Result<(), String> {
        if matches!(self.mapping, Mapping::Finding) {
            return Err("Finding atom correspondence…".into());
        }
        if self.mode == Mode::TransitionState {
            if let Mapping::Failed(error) = &self.mapping {
                return Err(error.clone());
            }
            let plan = self.plan.as_ref().map_err(|error| match error {
                TsPlanError::Shape { .. } => SHAPE_NOTE.into(),
                _ => error.to_string(),
            })?;
            self.images()?;
            species::ts_state(species, plan.start)?;
        }
        Ok(())
    }

    fn validate(&self) -> Result<Vec<Species>, String> {
        let species = species::validate(&self.inputs)?;
        self.can_run(&species)?;
        Ok(species)
    }

    fn needs_review(&self) -> bool {
        if self.energy_fallback && self.mode == Mode::Energy {
            return self.validate().is_err();
        }
        self.explicit
            || self.unmapped_isotope_hydrogen
            || match &self.mapping {
                Mapping::Drawing => false,
                Mapping::Proposed(proposal) => {
                    !proposal.complete
                        || proposal.multiple_best
                        || proposal.unmatched_reactants != 0
                        || proposal.unmatched_products != 0
                }
                Mapping::Finding | Mapping::Failed(_) => true,
            }
            || !self
                .changes
                .as_ref()
                .is_ok_and(|changes| changes.hydrogens.is_empty())
            || self.validate().is_err()
    }

    fn visible(&self) -> bool {
        self.explicit
            || self.unmapped_isotope_hydrogen
            || !matches!(self.mapping, Mapping::Finding)
            || species::validate(&self.inputs).is_err()
    }
}

pub(in crate::app) fn reaction_arrow(doc: &Document, id: u64) -> bool {
    doc.arrows.iter().any(|arrow| {
        arrow.id == id
            && arrow.purpose.is_chemistry()
            && reshiki::arrows::Preset::from_kind(&arrow.kind)
                .reaction_role_refusal()
                .is_none()
    })
}

fn send(action: Action) -> Message {
    Message::Xtb(XtbAction::Ts(TsAction::ReviewSheet(action)))
}

pub(super) fn scheme_plan(doc: &Document, arrow: u64) -> Result<TsPlan, TsPlanError> {
    let source = with_roles(doc, arrow).map_err(|_| TsPlanError::NotReaction)?;
    ts_plan::plan(&source, arrow)
}

pub(super) fn barrier_refusal(plan: &Result<TsPlan, TsPlanError>) -> Option<String> {
    let error = plan.as_ref().err()?;
    match error {
        TsPlanError::NotReaction
        | TsPlanError::Unmapped { .. }
        | TsPlanError::UnmappedHydrogen
        | TsPlanError::UnmappedIsotopeHydrogen => None,
        _ => Some(error.to_string()),
    }
}

pub(super) fn species_formula(doc: &Document, atoms: &[u64]) -> Option<String> {
    species::species_formula(doc, atoms)
}

pub(in crate::app) fn modal_key(key: &Key, modifiers: Modifiers) -> Option<Message> {
    match key {
        Key::Named(Named::Escape) if modifiers.is_empty() => Some(send(Action::Cancel)),
        Key::Named(Named::Enter) if modifiers.command() => Some(send(Action::Accept)),
        _ => None,
    }
}

impl App {
    pub(in crate::app) fn xtb_reaction_supports_barrier(&self, arrow: u64) -> bool {
        if !reaction_arrow(&self.tab.doc, arrow) {
            return false;
        }
        let Ok(source) = with_roles(&self.tab.doc, arrow) else {
            return false;
        };
        let Ok(reaction) = reshiki::reaction_mapping::reaction(&source, arrow) else {
            return false;
        };
        reaction.ready()
            && reaction
                .reactants
                .iter()
                .chain(&reaction.products)
                .all(|participant| participant.coefficient == 1)
            && species::shape(&source, reaction)
                .is_ok_and(|shape| matches!(shape, (1, 1 | 2) | (2, 1)))
    }

    pub(in crate::app) fn xtb_can_calculate_reaction(&self, arrow: u64) -> bool {
        let choices = &self.tab.xtb.ts_card;
        available()
            && reaction_arrow(&self.tab.doc, arrow)
            && !self.tab.busy
            && !self.xtb_running()
            && !self.tab.xtb.energy_state.in_progress()
            && !self.ts_review_open()
            && self.tab.optimization.is_none()
            && self.tab.model_job.is_none()
            && !self.tab.drawing_edit_active()
            && (choices.reaction_energy
                || choices.free_energy
                || (choices.barrier && self.xtb_reaction_supports_barrier(arrow)))
    }

    pub(super) fn xtb_calculate_reaction(&mut self, arrow: u64) -> Task<Message> {
        if !self.xtb_can_calculate_reaction(arrow) {
            return Task::none();
        }
        let choices = &self.tab.xtb.ts_card;
        let mode = if choices.barrier {
            Mode::TransitionState
        } else {
            Mode::Energy
        };
        let energy_fallback = choices.barrier && (choices.reaction_energy || choices.free_energy);
        self.xtb_review_open_inner(mode, Some(arrow), false, None, energy_fallback)
    }

    pub(in crate::app) fn xtb_review_target(&self) -> Option<u64> {
        let doc = &self.tab.doc;
        if let Some(arrow) = self.selected_reaction_arrow() {
            return reaction_arrow(doc, arrow.id).then_some(arrow.id);
        }
        let mut candidates = doc
            .arrows
            .iter()
            .filter(|arrow| reactions::preview(doc, arrow.id).is_ok());
        let first = candidates.next()?;
        candidates.next().is_none().then_some(first.id)
    }

    pub(in crate::app) fn ts_review_open(&self) -> bool {
        self.tab
            .xtb
            .ts_review
            .as_ref()
            .is_some_and(|dialog| dialog.visible() || dialog.at == self.review_stamp())
    }

    pub(in crate::app) fn ts_review_visible(&self) -> bool {
        self.tab.xtb.ts_review.as_ref().is_some_and(Dialog::visible)
    }

    pub(in crate::app) fn xtb_automatic_mapping_pending(&self, arrow: u64) -> bool {
        self.tab.xtb.ts_review.as_ref().is_some_and(|dialog| {
            dialog.arrow == arrow
                && dialog.at == self.review_stamp()
                && matches!(dialog.mapping, Mapping::Finding)
                && !dialog.visible()
        })
    }

    pub(in crate::app) fn xtb_scheme_ignored(&self, arrow: u64) -> Option<Element<'_, Message>> {
        if !available() {
            return None;
        }
        let reaction = reactions::preview(&self.tab.doc, arrow).ok()?;
        species::ignored(&self.tab.doc, &reaction).map(|text| line(text, Tone::Muted))
    }

    fn review_stamp(&self) -> (u64, u64) {
        (self.tab.file_epoch, self.tab.revision)
    }

    fn review_error(&mut self, error: String) -> Task<Message> {
        self.status = error;
        self.error = true;
        Task::none()
    }

    pub(super) fn xtb_review_open(&mut self, mode: Mode, arrow: Option<u64>) -> Task<Message> {
        self.xtb_review_open_inner(mode, arrow, false, None, false)
    }

    pub(super) fn xtb_review_show(&mut self, mode: Mode, arrow: Option<u64>) -> Task<Message> {
        self.xtb_review_open_inner(mode, arrow, true, None, false)
    }

    pub(super) fn xtb_review_retry(&mut self, arrow: u64, retry: Retry) -> Task<Message> {
        self.xtb_review_open_inner(Mode::TransitionState, Some(arrow), true, Some(retry), false)
    }

    fn xtb_review_open_inner(
        &mut self,
        mode: Mode,
        arrow: Option<u64>,
        explicit: bool,
        retry: Option<Retry>,
        energy_fallback: bool,
    ) -> Task<Message> {
        if self
            .tab
            .xtb
            .ts_review
            .as_ref()
            .is_some_and(|dialog| dialog.at != self.review_stamp() && !dialog.visible())
        {
            self.tab.xtb.ts_review = None;
        }
        if !available() || !self.xtb_can_start() || self.ts_review_open() {
            return Task::none();
        }
        let Some(arrow) = arrow.or_else(|| self.xtb_review_target()) else {
            return self.review_error("Select a reaction arrow first".into());
        };
        self.tab.selected = vec![arrow];
        self.xtb.page = Page::Xtb;
        let _ = self.show_inspector_tab(InspectorTab::Predict);
        let options = ReviewOptions {
            mode,
            explicit,
            retry,
            energy_fallback,
            free_energy: self.tab.xtb.ts_card.free_energy,
        };
        match self.prepare_review(&self.tab.doc, arrow, options) {
            Ok((dialog, task)) => self.show_prepared_review(dialog, task),
            Err(reason) => self.review_error(reason),
        }
    }

    pub(super) fn xtb_review_scheme_step(
        &mut self,
        index: usize,
        document: Document,
        arrow: u64,
        free_energy: bool,
        detached_map: bool,
    ) -> Task<Message> {
        let options = ReviewOptions {
            mode: Mode::TransitionState,
            explicit: false,
            retry: None,
            energy_fallback: false,
            free_energy,
        };
        match self.prepare_review(&document, arrow, options) {
            Ok((mut dialog, task)) => {
                dialog.scheme = Some((index, Arc::new(document)));
                dialog.detached_map = detached_map;
                self.show_prepared_review(dialog, task)
            }
            Err(reason) => self.xtb_scheme_reviewed(index, Err(reason)),
        }
    }

    fn prepare_review(
        &self,
        doc: &Document,
        arrow: u64,
        options: ReviewOptions,
    ) -> Result<(Dialog, Task<Message>), String> {
        let ReviewOptions {
            mut mode,
            explicit,
            retry,
            energy_fallback,
            free_energy,
        } = options;
        let reaction = reactions::preview(doc, arrow)?;
        if !reaction.ready() {
            return Err("Assign reactants and products first".into());
        }
        let source = with_roles(doc, arrow)?;
        let (inputs, shape) = species::inputs(&source, &reaction)?;
        let plan = ts_plan::plan_for_review(&source, arrow);
        if energy_fallback && barrier_refusal(&plan).is_some() {
            mode = Mode::Energy;
        }
        if !explicit
            && mode == Mode::TransitionState
            && let Err(
                error @ (TsPlanError::Shape { .. }
                | TsPlanError::TooLarge { .. }
                | TsPlanError::Unsupported(_)
                | TsPlanError::NoChange),
            ) = &plan
            && species::validate(&inputs).is_ok()
        {
            return Err(match error {
                TsPlanError::Shape { .. } => SHAPE_NOTE.into(),
                _ => error.to_string(),
            });
        }
        let changes = Changes::read(
            &source,
            arrow,
            plan.as_ref().ok().filter(|_| mode == Mode::TransitionState),
        );
        let fallback = energy_fallback && mode == Mode::Energy;
        let unmapped_isotope_hydrogen =
            !fallback && matches!(changes, Err(TsPlanError::UnmappedIsotopeHydrogen));
        let mapping = if fallback {
            Mapping::Drawing
        } else if mode == Mode::TransitionState && !matches!(shape, (1, 1 | 2) | (2, 1)) {
            Mapping::Failed(SHAPE_NOTE.into())
        } else if reaction
            .reactants
            .iter()
            .chain(&reaction.products)
            .any(|p| p.coefficient != 1)
        {
            Mapping::Failed("Expand coefficients to review a one-to-one atom map".into())
        } else if matches!(
            changes,
            Err(TsPlanError::Unmapped { .. }
                | TsPlanError::UnmappedHydrogen
                | TsPlanError::UnmappedIsotopeHydrogen)
        ) {
            Mapping::Finding
        } else {
            Mapping::Drawing
        };
        let id = NEXT_SHEET.fetch_add(1, Ordering::Relaxed);
        let task = if free_energy {
            CalcTask::OptFreq
        } else {
            CalcTask::Optimize
        };
        let edit_species = species::validate(&inputs).is_err();
        let mut dialog = Dialog {
            id,
            scheme: None,
            detached_map: false,
            at: self.review_stamp(),
            arrow,
            mode,
            retry,
            explicit,
            energy_fallback,
            task,
            free_energy,
            inputs,
            path_points: "15".into(),
            edit_species,
            advanced: false,
            mapping,
            plan,
            changes,
            inferred: matches!(source, Cow::Owned(_)),
            unmapped_isotope_hydrogen,
            ignored: species::ignored(&source, &reaction),
            target_stereo_note: (mode == Mode::TransitionState)
                .then(|| self.xtb_target_stereo_note(&reaction, shape))
                .flatten(),
            pictures: None,
        };
        let finding = matches!(dialog.mapping, Mapping::Finding);
        if dialog.needs_review() {
            dialog.pictures = dialog.pictures(&source);
        }
        let map_task = finding.then(|| {
            Task::perform(calculate(source.into_owned(), arrow), move |result| {
                send(Action::Ready(id, result))
            })
        });
        Ok((dialog, map_task.unwrap_or_else(Task::none)))
    }

    fn show_prepared_review(&mut self, dialog: Dialog, task: Task<Message>) -> Task<Message> {
        let finding = matches!(dialog.mapping, Mapping::Finding);
        if let Some((index, _)) = dialog.scheme
            && !finding
            && let Err(reason) = dialog.validate()
        {
            return self.xtb_scheme_reviewed(index, Err(reason));
        }
        if !dialog.needs_review() {
            return self.xtb_review_run(dialog);
        }
        if finding && !dialog.visible() {
            self.status = "Matching reaction atoms…".into();
            self.error = false;
        }
        self.tab.xtb.ts_review = Some(dialog);
        task
    }

    pub(super) fn xtb_review_action(&mut self, action: Action) -> Task<Message> {
        let at = self.review_stamp();
        let Some(dialog) = self.tab.xtb.ts_review.as_mut() else {
            return Task::none();
        };
        match action {
            Action::Cancel => {
                if dialog.scheme.is_some() {
                    return self.xtb_scheme_action(scheme::Action::Cancel);
                }
                if !dialog.visible() {
                    self.status = "Atom matching canceled".into();
                    self.error = false;
                }
                self.tab.xtb.ts_review = None;
                return crate::app::focus_drawing();
            }
            _ if dialog.at != at => {
                if dialog.scheme.is_some() {
                    return self.xtb_scheme_action(scheme::Action::Cancel);
                }
                if !dialog.visible() {
                    self.tab.xtb.ts_review = None;
                    self.status = "Reaction changed; atom matching canceled".into();
                    self.error = false;
                }
                return Task::none();
            }
            Action::Ready(id, result)
                if id == dialog.id && matches!(dialog.mapping, Mapping::Finding) =>
            {
                let scheme_doc = dialog.scheme.as_ref().map(|(_, doc)| Arc::clone(doc));
                let result = result.and_then(|proposal| {
                    let source =
                        with_roles(scheme_doc.as_deref().unwrap_or(&self.tab.doc), dialog.arrow)?;
                    let candidate = proposal.apply(&source, true)?;
                    Ok((source, candidate, proposal))
                });
                dialog.mapping = match result {
                    Ok((source, candidate, proposal)) => {
                        dialog.refresh(&source, &candidate);
                        if dialog.energy_fallback && dialog.mode == Mode::Energy {
                            Mapping::Drawing
                        } else {
                            Mapping::Proposed(proposal)
                        }
                    }
                    Err(error) => {
                        if dialog.energy_fallback {
                            dialog.mode = Mode::Energy;
                        }
                        Mapping::Failed(error)
                    }
                };
                if let Some((index, _)) = dialog.scheme
                    && let Err(reason) = dialog.validate()
                {
                    self.tab.xtb.ts_review = None;
                    return self.xtb_scheme_reviewed(index, Err(reason));
                }
                if !dialog.needs_review()
                    && let Some(dialog) = self.tab.xtb.ts_review.take()
                {
                    return if dialog.scheme.is_some() || self.xtb_can_start() {
                        self.xtb_review_run(dialog)
                    } else {
                        Task::none()
                    };
                }
            }
            Action::Ready(..) => {}
            Action::Charge(index, value) => {
                if let Some(input) = dialog.inputs.get_mut(index) {
                    input.charge = value;
                }
            }
            Action::Unpaired(index, value) => {
                if let Some(input) = dialog.inputs.get_mut(index) {
                    input.unpaired = value;
                }
            }
            Action::PathPoints(value) => dialog.path_points = value,
            Action::EditSpecies => dialog.edit_species = !dialog.edit_species,
            Action::Advanced => dialog.advanced = !dialog.advanced,
            Action::EditMap => {
                if dialog.detached_map || matches!(dialog.mapping, Mapping::Finding) {
                    return Task::none();
                }
                let arrow = dialog.arrow;
                let proposal = dialog.mapping.proposal().cloned();
                if dialog.scheme.is_some() {
                    let _ = self.xtb_scheme_action(scheme::Action::Cancel);
                }
                self.tab.xtb.ts_review = None;
                return self.open_reaction_map(arrow, proposal);
            }
            Action::SkipBarrier => {
                if !dialog.detached_map {
                    return Task::none();
                }
                let Some((index, _)) = dialog.scheme else {
                    return Task::none();
                };
                self.tab.xtb.ts_review = None;
                return self.xtb_scheme_skip_barrier(index);
            }
            Action::Accept => {
                if dialog.validate().is_err() || (dialog.scheme.is_none() && !self.xtb_can_start())
                {
                    return Task::none();
                }
                if let Some(dialog) = self.tab.xtb.ts_review.take() {
                    return crate::app::focus_drawing().chain(self.xtb_review_run(dialog));
                }
            }
        }
        Task::none()
    }

    fn xtb_review_run(&mut self, dialog: Dialog) -> Task<Message> {
        let species = match dialog.validate() {
            Ok(species) => species,
            Err(error) => return self.review_error(error),
        };
        let arrow = dialog.arrow;
        if let Some((index, source)) = &dialog.scheme {
            let input = (|| {
                let images = dialog.images()?;
                let plan = dialog.plan.as_ref().map_err(ToString::to_string)?;
                let start = species::ts_state(&species, plan.start)?;
                let proposal = dialog.mapping.proposal();
                let document = match proposal {
                    Some(proposal) => proposal.apply(source, true)?,
                    None => source.as_ref().clone(),
                };
                Ok(scheme::run::TsInput {
                    document,
                    plan: plan.clone(),
                    images,
                    charge: start.charge,
                    multiplicity: start.multiplicity,
                    automatic: proposal.is_some(),
                })
            })();
            return self.xtb_scheme_reviewed(*index, input);
        }
        let result = with_roles(&self.tab.doc, arrow)
            .map_err(str::to_owned)
            .and_then(|source| {
                if let Some(proposal) = dialog.mapping.proposal() {
                    proposal.apply(&source, true).map(Some)
                } else {
                    Ok(match source {
                        Cow::Owned(candidate) => Some(candidate),
                        Cow::Borrowed(_) => None,
                    })
                }
            });
        let candidate = match result {
            Ok(candidate) => candidate,
            Err(error) => return self.review_error(error),
        };
        if let Some(candidate) = candidate {
            let selection = if dialog.needs_review() {
                vec![arrow]
            } else {
                self.tab.selected.clone()
            };
            if let Err(error) = self.try_replace_document(candidate, selection) {
                return self.review_error(error);
            }
        } else {
            self.tab.selected = vec![arrow];
        }
        if dialog.mapping.proposal().is_some()
            && let Ok(plan) = &dialog.plan
        {
            self.tab
                .xtb
                .ts_card
                .accept_automatic_map((self.tab.file_epoch, arrow), plan);
        }
        match dialog.mode {
            Mode::TransitionState => {
                let Ok(images) = dialog.images() else {
                    return Task::none();
                };
                let Ok(plan) = dialog.plan else {
                    return Task::none();
                };
                let start = match species::ts_state(&species, plan.start) {
                    Ok(start) => start,
                    Err(error) => return self.review_error(error),
                };
                let unpaired = start.multiplicity - 1;
                self.xtb_ts_begin(
                    arrow,
                    plan,
                    images,
                    (start.charge, unpaired),
                    dialog.free_energy,
                    dialog.retry,
                )
            }
            Mode::Energy => {
                let refusal = dialog.barrier_refusal();
                self.xtb_reaction_energy_begin(arrow, species, dialog.task, refusal)
            }
        }
    }
}

#[cfg(test)]
mod tests;
