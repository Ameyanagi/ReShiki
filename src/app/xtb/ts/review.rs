//! Review reaction participants, atom correspondence and electronic state before xTB.
use super::super::{
    Action as XtbAction, CalcTask, Page, available,
    tone::{Tone, line},
};
use super::{Action as TsAction, energy::Species};
use crate::app::help::{Placement, modal};
use crate::app::reaction_mapping::{Preview, calculate, with_roles};
use crate::app::workspace::accessible_command;
use crate::app::{App, InspectorTab, Message};
use iced::keyboard::{Key, Modifiers, key::Named};
use iced::widget::{Space, column, row, scrollable, text};
use iced::{Alignment, Element, Length, Task};
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
use std::{borrow::Cow, collections::BTreeSet};

mod species;

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
    PathPoints(String),
    Accept,
    EditMap,
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
            if pairs.is_empty() {
                "none".into()
            } else {
                pairs
                    .iter()
                    .map(|&(a, b)| format!("{}–{}", atom_label(a), atom_label(b)))
                    .collect::<Vec<_>>()
                    .join(", ")
            }
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
        Self {
            bonds: format!(
                "Bonds formed: {} · broken: {}",
                bonds(&changes.form),
                bonds(&changes.break_)
            ),
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
}

pub(in crate::app::xtb) struct Dialog {
    id: u64,
    at: (u64, u64),
    arrow: u64,
    mode: Mode,
    rerun: bool,
    explicit: bool,
    inferred: bool,
    unmapped_isotope_hydrogen: bool,
    ignored: Option<String>,
    target_stereo_note: Option<&'static str>,
    inputs: Vec<species::Input>,
    path_points: String,
    task: CalcTask,
    free_energy: bool,
    mapping: Mapping,
    preview: Option<Preview>,
    changes: Result<Changes, TsPlanError>,
    plan: Result<TsPlan, TsPlanError>,
}

impl Dialog {
    fn images(&self) -> Result<usize, String> {
        if self.rerun {
            return Ok(20);
        }
        self.path_points
            .trim()
            .parse::<usize>()
            .ok()
            .filter(|points| (10..=20).contains(points))
            .ok_or_else(|| "Enter a whole number from 10 to 20 for the path points.".into())
    }

    fn refresh(&mut self, source: Document) {
        self.plan = ts_plan::plan_for_review(&source, self.arrow);
        let plan = self
            .plan
            .as_ref()
            .ok()
            .filter(|_| self.mode == Mode::TransitionState);
        self.changes = Changes::read(&source, self.arrow, plan);
        self.preview = Some(Preview::for_reaction(source, self.arrow));
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
        let mode = if self.tab.xtb.ts_card.barrier && self.xtb_reaction_supports_barrier(arrow) {
            Mode::TransitionState
        } else {
            Mode::Energy
        };
        self.xtb_review_open(mode, Some(arrow))
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
        self.xtb_review_open_inner(mode, arrow, false, false)
    }

    pub(super) fn xtb_review_show(&mut self, mode: Mode, arrow: Option<u64>) -> Task<Message> {
        self.xtb_review_open_inner(mode, arrow, true, false)
    }

    pub(super) fn xtb_review_rerun(&mut self, arrow: u64) -> Task<Message> {
        self.xtb_review_open_inner(Mode::TransitionState, Some(arrow), true, true)
    }

    fn xtb_review_open_inner(
        &mut self,
        mode: Mode,
        arrow: Option<u64>,
        explicit: bool,
        rerun: bool,
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
        let reaction = match reactions::preview(&self.tab.doc, arrow) {
            Ok(reaction) if reaction.ready() => reaction,
            Ok(_) => return self.review_error("Assign reactants and products first".into()),
            Err(error) => return self.review_error(error.into()),
        };
        let source = match with_roles(&self.tab.doc, arrow) {
            Ok(source) => source,
            Err(error) => return self.review_error(error.into()),
        };
        let (inputs, shape) = match species::inputs(&source, &reaction) {
            Ok(inputs) => inputs,
            Err(error) => return self.review_error(error),
        };
        let plan = ts_plan::plan_for_review(&source, arrow);
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
            return self.review_error(match error {
                TsPlanError::Shape { .. } => SHAPE_NOTE.into(),
                _ => error.to_string(),
            });
        }
        let changes = Changes::read(
            &source,
            arrow,
            plan.as_ref().ok().filter(|_| mode == Mode::TransitionState),
        );
        let unmapped_isotope_hydrogen =
            matches!(changes, Err(TsPlanError::UnmappedIsotopeHydrogen));
        let mapping = if mode == Mode::TransitionState && !matches!(shape, (1, 1 | 2) | (2, 1)) {
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
        let free_energy = self.tab.xtb.ts_card.free_energy;
        let task = if free_energy {
            CalcTask::OptFreq
        } else {
            CalcTask::Optimize
        };
        let mut dialog = Dialog {
            id,
            at: self.review_stamp(),
            arrow,
            mode,
            rerun,
            explicit,
            task,
            free_energy,
            inputs,
            path_points: "15".into(),
            mapping,
            plan,
            changes,
            inferred: matches!(source, Cow::Owned(_)),
            unmapped_isotope_hydrogen,
            ignored: species::ignored(&source, &reaction),
            target_stereo_note: (mode == Mode::TransitionState)
                .then(|| self.xtb_target_stereo_note(&reaction, shape))
                .flatten(),
            preview: None,
        };
        let finding = matches!(dialog.mapping, Mapping::Finding);
        // Every entry route reveals the same reaction card while a detached
        // map is calculated. The sheet is reserved for decisions to review.
        if !dialog.needs_review() {
            return self.xtb_review_run(dialog);
        }
        dialog.preview = Some(Preview::for_reaction(source.as_ref().clone(), arrow));
        let map_task = finding.then(|| {
            Task::perform(calculate(source.into_owned(), arrow), move |result| {
                send(Action::Ready(id, result))
            })
        });
        if finding && !dialog.visible() {
            self.status = "Matching reaction atoms…".into();
            self.error = false;
        }
        self.tab.xtb.ts_review = Some(dialog);
        map_task.unwrap_or_else(Task::none)
    }

    pub(super) fn xtb_review_action(&mut self, action: Action) -> Task<Message> {
        let at = self.review_stamp();
        let Some(dialog) = self.tab.xtb.ts_review.as_mut() else {
            return Task::none();
        };
        match action {
            Action::Cancel => {
                if !dialog.visible() {
                    self.status = "Atom matching canceled".into();
                    self.error = false;
                }
                self.tab.xtb.ts_review = None;
                return crate::app::focus_drawing();
            }
            _ if dialog.at != at => {
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
                let result = result.and_then(|proposal| {
                    let source = with_roles(&self.tab.doc, dialog.arrow)?;
                    Ok((proposal.apply(&source, true)?, proposal))
                });
                dialog.mapping = match result {
                    Ok((candidate, proposal)) => {
                        dialog.refresh(candidate);
                        Mapping::Proposed(proposal)
                    }
                    Err(error) => Mapping::Failed(error),
                };
                if !dialog.needs_review()
                    && let Some(dialog) = self.tab.xtb.ts_review.take()
                {
                    return if self.xtb_can_start() {
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
            Action::EditMap => {
                if matches!(dialog.mapping, Mapping::Finding) {
                    return Task::none();
                }
                let arrow = dialog.arrow;
                let proposal = dialog.mapping.proposal().cloned();
                self.tab.xtb.ts_review = None;
                return self.open_reaction_map(arrow, proposal);
            }
            Action::Accept => {
                if dialog.validate().is_err() || !self.xtb_can_start() {
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
                    start.charge,
                    unpaired,
                    dialog.free_energy,
                )
            }
            Mode::Energy => self.xtb_reaction_energy_begin(arrow, species, dialog.task),
        }
    }

    pub(in crate::app) fn with_ts_review<'a>(
        &'a self,
        base: Element<'a, Message>,
    ) -> Element<'a, Message> {
        let Some(dialog) = &self.tab.xtb.ts_review else {
            return base;
        };
        if !dialog.visible() {
            return base;
        }
        let current = dialog.at == self.review_stamp();
        let title = match dialog.mode {
            Mode::TransitionState => "Review transition-state search (GFN2-xTB)",
            Mode::Energy => "Review reaction energy (GFN2-xTB)",
        };
        let mut content = column![text(title).size(21)].spacing(12);
        if let Some(thumbnail) = dialog
            .preview
            .as_ref()
            .and_then(|preview| preview.thumbnail(160))
        {
            content = content.push(thumbnail);
        }
        if dialog.inferred {
            content = content.push(line(
                "Reactants and products detected from the drawing",
                Tone::Muted,
            ));
        }
        if let Some(ignored) = &dialog.ignored {
            content = content.push(line(ignored, Tone::Muted));
        }
        if let Some(note) = dialog.target_stereo_note {
            content = content.push(line(note, Tone::Muted));
        }
        if dialog.unmapped_isotope_hydrogen {
            content = content.push(line(
                "Isotope-labelled hydrogens need an explicit atom map.",
                Tone::Muted,
            ));
        }
        if let Mapping::Proposed(proposal) = &dialog.mapping {
            let detail = if !proposal.complete {
                " · Search budget reached"
            } else if proposal.multiple_best {
                " · Several mappings tie"
            } else {
                ""
            };
            content = content.push(line(
                format!("Proposed atom map · {} pairs · {} reactant / {} product atoms unmatched{detail}",
                    proposal.pairs.len(), proposal.unmatched_reactants, proposal.unmatched_products),
                Tone::Muted,
            ));
        }
        if let Ok(changes) = &dialog.changes {
            content = content.push(line(&changes.bonds, Tone::Muted));
            if changes.hydrogens.is_empty() {
                content = content.push(line("Hydrogens that move: none", Tone::Muted));
            } else {
                content = content.push(line("Hydrogens that move", Tone::Muted));
                for entry in &changes.hydrogens {
                    content = content.push(line(entry, Tone::Muted));
                }
            }
        }
        for (index, input) in dialog.inputs.iter().enumerate() {
            let field =
                |label: &'static str, value: &'a str, action: fn(usize, String) -> Action| {
                    column![
                        text(label).size(11),
                        reshiki::accessibility::text_input(
                            format!("ts.review.{index}.{label}"),
                            format!("{} · {label}", input.label),
                            "",
                            value,
                        )
                        .style(crate::appearance::input_style)
                        .size(13)
                        .padding(5)
                        .width(94)
                        .on_input(move |value| send(action(index, value)))
                    ]
                    .spacing(3)
                };
            content = content.push(
                row![
                    text(&input.label).size(13).width(Length::Fill),
                    field("Charge", &input.charge, Action::Charge),
                    field("Unpaired electrons", &input.unpaired, Action::Unpaired),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            );
        }
        if dialog.mode == Mode::TransitionState && !dialog.rerun {
            content = content.push(
                row![
                    text("Path points (10–20)").size(12),
                    reshiki::accessibility::text_input(
                        "ts.review.path-points",
                        "Path points",
                        "15",
                        &dialog.path_points,
                    )
                    .style(crate::appearance::input_style)
                    .size(13)
                    .padding(5)
                    .width(94)
                    .on_input(|value| send(Action::PathPoints(value))),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            );
        }
        if dialog.free_energy {
            content = content.push(line(
                if dialog.mode == Mode::TransitionState {
                    "Report ΔG and ΔG‡ at 298.15 K"
                } else {
                    "Report ΔG at 298.15 K"
                },
                Tone::Muted,
            ));
        }
        if dialog.mode == Mode::Energy {
            if let Mapping::Failed(reason) = &dialog.mapping {
                content = content.push(line(reason, Tone::Muted));
            } else if let Err(reason) = &dialog.changes
                && !matches!(dialog.mapping, Mapping::Finding)
            {
                content = content.push(line(reason.to_string(), Tone::Muted));
            }
        }
        let valid = if current {
            let checked = species::validate(&dialog.inputs);
            if checked.is_ok() {
                content = content.push(line("Atoms and charge balance", Tone::Good));
            }
            checked.and_then(|species| dialog.can_run(&species))
        } else {
            Err("The reaction drawing changed. Close this review and try again.".into())
        };
        if let Err(error) = &valid {
            content = content.push(line(
                error,
                if matches!(dialog.mapping, Mapping::Finding) {
                    Tone::Muted
                } else {
                    Tone::Error
                },
            ));
        }
        content = content.push(
            row![
                accessible_command("ts.review.edit", "Edit map", "Edit map").on_press_maybe(
                    (current && !matches!(dialog.mapping, Mapping::Finding))
                        .then(|| send(Action::EditMap))
                ),
                Space::new().width(Length::Fill),
                accessible_command("ts.review.cancel", "Cancel", "Cancel")
                    .on_press(send(Action::Cancel)),
                accessible_command("ts.review.accept", "Accept", "Accept")
                    .style(crate::appearance::primary)
                    .on_press_maybe(valid.is_ok().then(|| send(Action::Accept))),
            ]
            .spacing(8),
        );
        modal(
            base,
            scrollable(content).into(),
            640.,
            700.,
            Placement::Fit,
            send(Action::Cancel),
        )
    }
}

#[cfg(test)]
mod tests;
