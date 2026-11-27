//! Predict controls and results for a single selected reaction arrow.
use super::{Action as TsAction, Live, StepState, review, view};
use crate::app::workspace::{
    accessible_checkbox, accessible_command, horizontal_line, hover_hint, muted_text,
};
use crate::app::xtb::tone::{Tone, line, toned_text};
use crate::app::xtb::{self, Action as XtbAction, EnergyUnit, Outcome};
use crate::app::{App, Message};
use crate::canvas::{BondChangeKind, BondChangeMark};
use iced::widget::{
    canvas, column, container, mouse_area, progress_bar, rich_text, row, span, text, tooltip,
};
use iced::{Element, Length, Point, Rectangle, Renderer, Task, Theme, mouse};
use reshiki::document::Document;
use reshiki::palette::{Color, Palette};
use reshiki::qcdata::{
    CalcResult, MappedAtom, TransitionStateMapping, TransitionStateRun, XtbMethod,
};
use reshiki::reactions::Reaction;
use reshiki::reactions::ts_plan::{Side, TsPlan, TsPlanError};
use std::cell::Cell;
use std::sync::Arc;

type Scope = (u64, u64);
type StereoCacheEntry = ((Scope, u64), Option<&'static str>);
type BondHover = (u64, BondChangeKind, [u32; 2]);

pub(in crate::app::xtb) struct State {
    pub(super) reaction_energy: bool,
    pub(super) barrier: bool,
    pub(super) free_energy: bool,
    pub(in crate::app::xtb) status: Option<(u64, Status, String)>,
    automatic_maps: Vec<(Scope, TsPlan)>,
    target_stereo: Cell<Option<StereoCacheEntry>>,
    hovered_bond: Option<BondHover>,
}

pub(in crate::app::xtb) enum Status {
    TransitionState,
    ReactionEnergy,
}

impl Default for State {
    fn default() -> Self {
        Self {
            reaction_energy: true,
            barrier: true,
            free_energy: false,
            status: None,
            automatic_maps: Vec::new(),
            target_stereo: Cell::new(None),
            hovered_bond: None,
        }
    }
}

impl State {
    pub(super) fn accept_automatic_map(&mut self, scope: Scope, plan: &TsPlan) {
        self.automatic_maps
            .retain(|(at, _)| at.0 == scope.0 && *at != scope);
        self.automatic_maps.push((scope, plan.clone()));
    }

    pub(super) fn automatic_map(&self, scope: Scope, plan: &TsPlan) -> bool {
        self.automatic_maps
            .iter()
            .any(|(at, mapped)| *at == scope && mapped == plan)
    }
}

#[derive(Clone, Debug)]
pub(crate) enum Action {
    ReactionEnergy,
    Barrier,
    FreeEnergy,
    Method(XtbMethod),
    Result(Arc<CalcResult>),
    Refine,
    ShowPath,
    Export,
    Animate,
}

/// The significant imaginary mode used to inspect the saved saddle.
pub(in crate::app::xtb) fn imaginary_mode(result: &CalcResult) -> Option<(usize, f64)> {
    result
        .vibrations
        .as_ref()?
        .frequencies_cm1
        .iter()
        .copied()
        .enumerate()
        .filter(|(_, frequency)| *frequency < -50.)
        .min_by(|a, b| a.1.total_cmp(&b.1))
}

fn send(action: Action) -> Message {
    Message::Xtb(XtbAction::Ts(TsAction::Card(action)))
}

pub(in crate::app::xtb) struct ReactionLabel {
    reactants: Vec<SpeciesLabel>,
    products: Vec<SpeciesLabel>,
    target_stereo_note: Option<&'static str>,
}

struct SpeciesLabel {
    coefficient: u16,
    name: String,
    formula: String,
}

impl ReactionLabel {
    pub(super) fn has_multiple_species(&self) -> bool {
        [&self.reactants, &self.products]
            .into_iter()
            .any(|side| side.len() > 1 || side.iter().any(|species| species.coefficient > 1))
    }

    pub(in crate::app::xtb) fn title(&self) -> String {
        self.line(false)
    }

    pub(super) fn endpoint_names(&self) -> [String; 2] {
        [
            species_line(&self.reactants, false, "Reactants"),
            species_line(&self.products, false, "Products"),
        ]
    }

    fn formula(&self) -> String {
        self.line(true)
    }

    fn line(&self, formula: bool) -> String {
        format!(
            "{} → {}",
            species_line(&self.reactants, formula, "Reactants"),
            species_line(&self.products, formula, "Products")
        )
    }

    pub(super) fn first_reactant(&self) -> Option<&str> {
        self.reactants.first().map(|species| species.name.as_str())
    }

    pub(super) fn first_reactant_formula(&self) -> Option<&str> {
        self.reactants
            .first()
            .map(|species| species.formula.as_str())
    }
}

fn species_line(species: &[SpeciesLabel], formula: bool, empty: &str) -> String {
    if species.is_empty() {
        return empty.to_owned();
    }
    species
        .iter()
        .map(|species| {
            let name = if formula {
                &species.formula
            } else {
                &species.name
            };
            if species.coefficient == 1 {
                name.clone()
            } else {
                format!("{} {name}", species.coefficient)
            }
        })
        .collect::<Vec<_>>()
        .join(" + ")
}

fn drawn_name<'a>(doc: &'a Document, atoms: &[u64]) -> Option<&'a str> {
    reshiki::compound_numbers::label(doc, atoms)
        .and_then(|label| {
            doc.annotations
                .iter()
                .find(|annotation| annotation.id == label.annotation)
        })
        .and_then(|label| nonempty(&label.text))
        .or_else(|| {
            reshiki::linked_properties::caption(doc, atoms)
                .and_then(|link| nonempty(&link.custom_name))
        })
        .or_else(|| {
            reshiki::molecule_names::label(doc, atoms).and_then(|name| nonempty(&name.text))
        })
}

#[derive(Clone)]
pub(super) struct BondChanges(pub(super) TransitionStateMapping);

fn mapped_label_color(doc: &Document, reaction: &Reaction, atom: &MappedAtom, side: Side) -> Color {
    let (preferred, fallback) = match side {
        Side::Reactant => (&reaction.reactants, &reaction.products),
        Side::Product => (&reaction.products, &reaction.reactants),
    };
    preferred
        .iter()
        .chain(fallback)
        .flat_map(|participant| &participant.atoms)
        .filter_map(|&id| doc.atom(id))
        .find(|drawn| drawn.map_num == atom.number && drawn.element == atom.element)
        .map_or(Color::Ink, |drawn| drawn.display.mapping.style.color)
}

impl BondChanges {
    pub(super) fn from_plan(doc: &Document, plan: &TsPlan) -> Self {
        let (formed, broken) = match plan.start {
            Side::Reactant => (&plan.form, &plan.break_),
            Side::Product => (&plan.break_, &plan.form),
        };
        let labels = |bonds: &[(u64, u64)], reverse| {
            bonds
                .iter()
                .filter_map(|&(a, b)| {
                    let atom = |id| {
                        let atom = doc.atom(id)?;
                        Some(MappedAtom {
                            element: atom.element.clone(),
                            number: atom.map_num,
                        })
                    };
                    if reverse {
                        Some([atom(b)?, atom(a)?])
                    } else {
                        Some([atom(a)?, atom(b)?])
                    }
                })
                .collect()
        };
        Self(TransitionStateMapping {
            formed: labels(formed, false),
            broken: labels(broken, false),
            hydrogen_moves: labels(&plan.hydrogen_moves, plan.start == Side::Product),
            automatic: false,
        })
    }

    fn marks(
        &self,
        doc: &Document,
        reaction: &Reaction,
        hovered: Option<BondHover>,
    ) -> Vec<BondChangeMark> {
        let resolve = |mapped: &MappedAtom| {
            let mut matches = reaction
                .reactants
                .iter()
                .flat_map(|p| &p.atoms)
                .filter_map(|&id| doc.atom(id))
                .filter(|atom| atom.map_num == mapped.number && atom.element == mapped.element);
            let id = matches.next()?.id;
            matches.next().is_none().then_some(id)
        };
        [
            (BondChangeKind::Form, &self.0.formed),
            (BondChangeKind::Break, &self.0.broken),
        ]
        .into_iter()
        .flat_map(|(kind, bonds)| {
            bonds.iter().filter_map(move |[a, b]| {
                let atoms = [resolve(a)?, resolve(b)?];
                let joined = doc
                    .bond_between(atoms[0], atoms[1])
                    .is_some_and(|(_, bond)| bond.order != 0);
                (joined == (kind == BondChangeKind::Break)).then_some(BondChangeMark {
                    kind,
                    atoms,
                    highlighted: hovered == Some((reaction.arrow, kind, [a.number, b.number])),
                })
            })
        })
        .collect()
    }

    fn view(&self, doc: &Document, arrow: u64, current_only: bool) -> Element<'static, Message> {
        let palette = Palette::of(doc);
        let reaction = reshiki::reactions::preview(doc, arrow).ok();
        let label = |atom: &MappedAtom, side| {
            let color = reaction.as_ref().map_or(Color::Ink, |reaction| {
                mapped_label_color(doc, reaction, atom, side)
            });
            span(format!("{}{}", atom.element, atom.number))
                .color(crate::appearance::from_rgb(palette.rgb(color)))
        };
        let mut content = row![].spacing(3);
        if current_only {
            content = content.push(text("Current map ·").size(12));
        }
        for (heading, separator, bonds, side, kind) in [
            (
                "Bonds formed:",
                "–",
                &self.0.formed,
                Side::Product,
                Some(BondChangeKind::Form),
            ),
            (
                "· broken:",
                "–",
                &self.0.broken,
                Side::Reactant,
                Some(BondChangeKind::Break),
            ),
            (
                "· Hydrogen moves:",
                " → ",
                &self.0.hydrogen_moves,
                Side::Reactant,
                None,
            ),
        ] {
            if bonds.is_empty() && kind.is_none() {
                continue;
            }
            content = content.push(text(heading).size(12));
            if bonds.is_empty() {
                content = content.push(text("none").size(12));
            }
            for (index, [a, b]) in bonds.iter().enumerate() {
                let suffix = if index + 1 == bonds.len() { "" } else { "," };
                let pair = rich_text::<(), _, _, _>([
                    label(a, side),
                    span(separator),
                    label(b, side),
                    span(suffix),
                ])
                .size(12);
                let pair: Element<'_, Message> = if let Some(kind) = kind {
                    let hover = |entered| {
                        Message::ReactionBondHover(arrow, kind, [a.number, b.number], entered)
                    };
                    mouse_area(pair)
                        .on_enter(hover(true))
                        .on_exit(hover(false))
                        .into()
                } else {
                    pair.into()
                };
                content = content.push(pair);
            }
        }
        content.wrap().vertical_spacing(3).into()
    }
}

pub(in crate::app::xtb) enum ArrowResult<'a> {
    Found(&'a Arc<CalcResult>),
    Ambiguous,
    Missing,
}

impl App {
    pub(in crate::app) fn xtb_selected_reaction(&self) -> Option<u64> {
        let &[arrow] = self.tab.selected.as_slice() else {
            return None;
        };
        review::reaction_arrow(&self.tab.doc, arrow).then_some(arrow)
    }

    pub(in crate::app) fn xtb_reaction_bond_marks(&self) -> Vec<BondChangeMark> {
        let Some(arrow) = self.xtb_reaction_card_arrow() else {
            return vec![];
        };
        let doc = &self.tab.doc;
        let Ok(reaction) = reshiki::reactions::preview(doc, arrow) else {
            return vec![];
        };
        let search = match self.xtb_ts_card_result(arrow) {
            ArrowResult::Found(result) => super::saved_search(result),
            _ => None,
        };
        let drawn = review::scheme_plan(doc, arrow)
            .ok()
            .map(|plan| BondChanges::from_plan(doc, &plan));
        self.xtb_card_bonds(arrow, search, drawn)
            .map_or_else(Vec::new, |(bonds, _)| {
                bonds.marks(doc, &reaction, self.tab.xtb.ts_card.hovered_bond)
            })
    }

    pub(in crate::app) fn xtb_keep_bond_hover(&mut self, visible: Option<u64>) {
        if let Some((arrow, ..)) = self.tab.xtb.ts_card.hovered_bond
            && visible != Some(arrow)
        {
            self.tab.xtb.ts_card.hovered_bond = None;
        }
    }

    pub(in crate::app) fn xtb_hover_bond(&mut self, bond: BondHover, entered: bool) {
        if entered && self.xtb_reaction_card_arrow() == Some(bond.0) {
            self.tab.xtb.ts_card.hovered_bond = Some(bond);
        } else if self.tab.xtb.ts_card.hovered_bond == Some(bond) {
            self.tab.xtb.ts_card.hovered_bond = None;
        }
    }

    fn xtb_card_bonds(
        &self,
        arrow: u64,
        search: Option<&TransitionStateRun>,
        drawn: Option<BondChanges>,
    ) -> Option<(BondChanges, bool)> {
        let drawn = drawn?;
        let request = self
            .tab
            .xtb
            .run
            .as_ref()
            .and_then(|run| run.ts.as_ref())
            .map(|live| &live.request)
            .filter(|request| request.arrow == arrow)
            .or_else(|| match &self.tab.xtb.outcome {
                Some(Outcome::Failed(failed)) => {
                    failed.ts().filter(|request| request.arrow == arrow)
                }
                _ => None,
            });
        let previous = request
            .map(|request| &request.bonds.0)
            .or_else(|| search.map(|search| &search.mapping));
        if let Some(mapping) = previous
            && drawn.0.formed == mapping.formed
            && drawn.0.broken == mapping.broken
            && drawn.0.hydrogen_moves == mapping.hydrogen_moves
        {
            return Some((BondChanges(mapping.clone()), false));
        }
        Some((drawn, true))
    }

    /// The reaction card actually shown in the front drawing's Predict tab.
    pub(in crate::app) fn xtb_reaction_card_arrow(&self) -> Option<u64> {
        (self.tabs.temporary_depth == 0
            && self.inspector_open
            && self.inspector_tab == crate::app::InspectorTab::Predict
            && self.xtb.page == xtb::Page::Xtb
            && !self.tube_bend_active()
            && self.tab.equipment_authoring.is_none())
        .then(|| self.xtb_selected_reaction())
        .flatten()
    }

    fn xtb_reaction_species_name<'a>(&'a self, atoms: &[u64]) -> Option<&'a str> {
        drawn_name(&self.tab.doc, atoms)
            .or_else(|| {
                reshiki::linked_properties::caption(&self.tab.doc, atoms)
                    .filter(|link| !link.needs_name())
                    .and_then(|link| nonempty(&link.name))
            })
            .or_else(|| self.xtb_existing_molecule_name(atoms))
    }

    /// Share Properties' naming queue; only the visible card asks for names.
    pub(in crate::app) fn xtb_reaction_name_request(&self) -> Option<(String, Vec<u64>)> {
        if self.ts_review_visible() {
            return None;
        }
        let arrow = self.xtb_reaction_card_arrow()?;
        let reaction = reshiki::reactions::preview(&self.tab.doc, arrow).ok()?;
        reaction
            .reactants
            .iter()
            .chain(&reaction.products)
            .flat_map(|participant| {
                reshiki::reactions::molecules(&self.tab.doc, &participant.atoms)
            })
            .find_map(|atoms| {
                if self.xtb_reaction_species_name(&atoms).is_some() {
                    return None;
                }
                let smiles = reshiki::naming::selected_identity(&self.tab.doc, &atoms)
                    .ok()?
                    .smiles;
                let state = &self.tab.naming;
                (!state.caption_failed.contains_key(&smiles)
                    && !state.caption_names.contains_key(&smiles))
                .then_some((smiles, atoms))
            })
    }

    pub(in crate::app::xtb) fn xtb_reaction_label(&self, arrow: u64) -> ReactionLabel {
        let doc = &self.tab.doc;
        let names = |participants: &[reshiki::reactions::Participant]| {
            participants
                .iter()
                .flat_map(|participant| {
                    reshiki::reactions::molecules(doc, &participant.atoms)
                        .into_iter()
                        .map(|atoms| {
                            let formula = review::species_formula(doc, &atoms)
                                .unwrap_or_else(|| "Unnamed molecule".into());
                            let name = self
                                .xtb_reaction_species_name(&atoms)
                                .map_or_else(|| formula.clone(), str::to_owned);
                            SpeciesLabel {
                                coefficient: participant.coefficient,
                                name,
                                formula,
                            }
                        })
                })
                .collect()
        };
        reshiki::reactions::preview(doc, arrow).map_or(
            ReactionLabel {
                reactants: vec![],
                products: vec![],
                target_stereo_note: None,
            },
            |reaction| {
                let reactants: Vec<_> = names(&reaction.reactants);
                let products: Vec<_> = names(&reaction.products);
                let target_stereo_note =
                    self.xtb_target_stereo_note(&reaction, (reactants.len(), products.len()));
                ReactionLabel {
                    reactants,
                    products,
                    target_stereo_note,
                }
            },
        )
    }

    pub(super) fn xtb_target_stereo_note(
        &self,
        reaction: &Reaction,
        shape: (usize, usize),
    ) -> Option<&'static str> {
        let (target, note) = match shape {
            (1, 1 | 2) => (
                &reaction.products,
                "Stereochemistry of the drawn product is not checked; compare it with the result.",
            ),
            (2, 1) => (
                &reaction.reactants,
                "Stereochemistry of the drawn reactant is not checked; compare it with the result.",
            ),
            _ => return None,
        };
        let key = ((self.tab.file_epoch, reaction.arrow), self.tab.revision);
        let cache = &self.tab.xtb.ts_card.target_stereo;
        if let Some((at, note)) = cache.get()
            && at == key
        {
            return note;
        }
        let doc = &self.tab.doc;
        let defined = target
            .iter()
            .flat_map(|participant| reshiki::reactions::molecules(doc, &participant.atoms))
            .any(|atoms| {
                reshiki::naming::selected_identity(doc, &atoms)
                    .is_ok_and(|identity| identity.smiles.contains(['@', '/', '\\']))
            });
        let note = defined.then_some(note);
        cache.set(Some((key, note)));
        note
    }

    /// Follow this arrow's TS result even when another calculation is shown.
    pub(in crate::app::xtb) fn xtb_ts_card_result(&self, arrow: u64) -> ArrowResult<'_> {
        let doc = &self.tab.doc;
        let mut ambiguous = false;
        for stored in doc.science.results.iter().rev() {
            let result = &stored.result;
            if !super::is_ts(result) {
                continue;
            }
            let Some(link) = doc.science.attachment(stored.attachment) else {
                continue;
            };
            if let Some(search) = super::saved_search(result) {
                if search.arrow == Some(arrow)
                    && super::bound_arrow(doc, arrow, &link.atoms).is_some()
                {
                    return ArrowResult::Found(result);
                }
                continue;
            }
            if let Some(session) = self.tab.xtb.ts_sessions.get(result) {
                if session.arrow == arrow && super::bound_arrow(doc, arrow, &link.atoms).is_some() {
                    return ArrowResult::Found(result);
                }
                continue;
            }
            let mut candidates = super::matching_plans(doc, &link.atoms).map(|(arrow, _)| arrow);
            let first = candidates.next();
            let second = candidates.next();
            if first == Some(arrow) && second.is_none() {
                return ArrowResult::Found(result);
            }
            ambiguous |=
                first == Some(arrow) || second == Some(arrow) || candidates.any(|id| id == arrow);
        }
        if ambiguous {
            ArrowResult::Ambiguous
        } else {
            ArrowResult::Missing
        }
    }

    /// The retained source remains the card's answer after an unsuccessful refine.
    pub(in crate::app::xtb) fn xtb_ts_card_display_result(&self, arrow: u64) -> ArrowResult<'_> {
        match self.xtb_ts_card_result(arrow) {
            ArrowResult::Found(result) => ArrowResult::Found(
                self.xtb_ts_linked_pair(result)
                    .filter(|pair| !view::presentation(pair[1], None).reliable())
                    .map_or(result, |pair| pair[0]),
            ),
            other => other,
        }
    }

    /// Open the saved saddle's imaginary mode without applying its geometry.
    pub(in crate::app::xtb) fn xtb_ts_animate(&mut self, result: Arc<CalcResult>) -> Task<Message> {
        if self.xtb_running() || self.tab.busy || !super::is_ts(&result) {
            return Task::none();
        }
        let Some((mode, _)) = imaginary_mode(&result) else {
            return Task::none();
        };
        let was_visible = self.xtb_vibrations_visible();
        self.xtb_show_result(&result);
        self.xtb_end_preview();
        self.tab.xtb.palette.view = crate::app::xtb::results::View::Vibrations;
        self.tab.xtb.palette.collapsed = false;
        let shown = if self.xtb.dock.inspector {
            self.xtb.page = crate::app::xtb::Page::Xtb;
            self.show_inspector_tab(crate::app::InspectorTab::Predict)
                .unwrap_or_else(Task::none)
        } else {
            Task::none()
        };
        self.xtb_vibrations_play_mode(mode);
        Task::batch([shown, self.xtb_vibrations_opened(was_visible)])
    }

    pub(in crate::app::xtb) fn xtb_ts_card_action(&mut self, action: Action) -> Task<Message> {
        let arrow = match &action {
            Action::Result(result) => super::saved_search(result).and_then(|run| run.arrow),
            _ => self.xtb_selected_reaction(),
        };
        let Some(arrow) = arrow else {
            return Task::none();
        };
        match action {
            Action::ReactionEnergy => {
                self.tab.xtb.ts_card.reaction_energy = !self.tab.xtb.ts_card.reaction_energy;
            }
            Action::Barrier => self.tab.xtb.ts_card.barrier = !self.tab.xtb.ts_card.barrier,
            Action::FreeEnergy => {
                self.tab.xtb.ts_card.free_energy = !self.tab.xtb.ts_card.free_energy;
            }
            Action::Method(method) => {
                if !self.xtb_running() && matches!(method, XtbMethod::Gfn2Xtb | XtbMethod::GXtb) {
                    self.xtb.settings.barrier_method = method;
                }
            }
            Action::Refine => {
                let ArrowResult::Found(result) = self.xtb_ts_card_display_result(arrow) else {
                    return Task::none();
                };
                return self.xtb_ts_action(TsAction::RefineGxtb(arrow, Arc::clone(result)));
            }
            Action::Result(result) => {
                let valid = !self.xtb_running()
                    && self.xtb_shown().is_some_and(|shown| {
                        self.xtb_ts_linked_pair(&shown.stored.result)
                            .is_some_and(|pair| {
                                pair.iter().any(|saved| Arc::ptr_eq(saved, &result))
                            })
                    });
                if valid {
                    self.xtb_open_saved_path(&result);
                    let status =
                        view::presentation(&result, None).status_line(self.appearance.energy_unit);
                    self.tab.xtb.ts_card.status =
                        Some((arrow, Status::TransitionState, status.clone()));
                    self.status = status;
                    self.error = false;
                }
            }
            Action::Export => return self.xtb_ts_export_arrow(arrow),
            Action::ShowPath => {
                if !self.xtb_show_reaction_path(arrow) {
                    return self.xtb_palette(super::super::palette::Action::Show);
                }
            }
            Action::Animate => {
                let ArrowResult::Found(result) = self.xtb_ts_card_display_result(arrow) else {
                    return Task::none();
                };
                return self.xtb_ts_animate(Arc::clone(result));
            }
        }
        Task::none()
    }

    pub(super) fn xtb_show_reaction_path(&mut self, arrow: u64) -> bool {
        if self.xtb_show_live_path(arrow) {
            return true;
        }
        let ArrowResult::Found(result) = self.xtb_ts_card_display_result(arrow) else {
            return false;
        };
        let result = Arc::clone(result);
        self.xtb_open_saved_path(&result);
        true
    }

    pub(super) fn xtb_open_saved_path(&mut self, result: &Arc<CalcResult>) {
        self.xtb_show_result(result);
        self.tab.xtb.palette.view = xtb::results::View::Path;
        self.tab.xtb.palette.collapsed = false;
        self.xtb_vibrations_sync();
    }

    pub(in crate::app::xtb) fn xtb_reaction_card(&self, arrow: u64) -> Element<'_, Message> {
        let doc = &self.tab.doc;
        let unit = self.appearance.energy_unit;
        let state = &self.tab.xtb.ts_card;
        let label = self.xtb_reaction_label(arrow);
        let plan = review::scheme_plan(doc, arrow);
        let latest_result = match self.xtb_ts_card_result(arrow) {
            ArrowResult::Found(result) => Some(result),
            _ => None,
        };
        let linked = latest_result.and_then(|result| self.xtb_ts_linked_pair(result));
        // A failed g-xTB refine retains the source's verified answer.
        let result = linked
            .filter(|pair| !view::presentation(pair[1], None).reliable())
            .map(|pair| pair[0])
            .or(latest_result);
        let stale = result.is_some_and(|result| self.xtb_ts_result_stale(result));
        let endpoint_energy = result.and_then(|result| view::reaction_energy_kcal(result));
        let (batch, batch_refusal) =
            self.xtb_energy_result(arrow, result.filter(|_| endpoint_energy.is_some()));
        let barrier_refusal = state
            .barrier
            .then(|| {
                batch_refusal.map(str::to_owned).or_else(|| {
                    review::barrier_refusal(&plan)
                        .filter(|_| !matches!(plan, Err(TsPlanError::Mismatch(_))))
                })
            })
            .flatten();
        let search = result.and_then(|result| super::saved_search(result));
        let presentation =
            result.map(|result| view::presentation(result, self.xtb_ts_refinement_session(result)));
        let failed = match &self.tab.xtb.outcome {
            Some(Outcome::Failed(failed))
                if failed.ts().is_some_and(|request| request.arrow == arrow) =>
            {
                Some(failed)
            }
            _ => None,
        };
        let refine_failed = failed.filter(|failed| {
            failed
                .ts()
                .is_some_and(|request| request.refined_from().is_some())
        });
        let failed = failed.filter(|_| refine_failed.is_none());
        let idle = self.tab.xtb.run.is_none()
            && !self.tab.xtb.energy_state.in_progress()
            && !self.tab.busy
            && !self.ts_review_open();
        let live = self
            .xtb_live_ts()
            .filter(|live| live.request.arrow == arrow);
        let matched = plan.is_ok()
            || result.zip(search).is_some_and(|(result, search)| {
                search.mapping.automatic && !self.xtb_ts_search_stale(result)
            });
        let subtitle = if matched {
            "Gas phase · atoms matched ✓"
        } else {
            "Gas phase"
        };
        let mut subtitle = row![line(subtitle, Tone::Muted)]
            .spacing(4)
            .align_y(iced::Alignment::Center);
        if xtb::available() {
            subtitle = subtitle.push(
                accessible_command("xtb-ts-review", "Review the reaction's atom map", "review")
                    .padding([1, 4])
                    .style(iced::widget::button::text)
                    .on_press_maybe(
                        idle.then_some(Message::Xtb(XtbAction::Ts(TsAction::ReviewMap(arrow)))),
                    ),
            );
        }
        let title = hover_hint(
            text(label.title()).size(14),
            label.formula(),
            tooltip::Position::Top,
        );
        let mut content = column![column![title, subtitle].spacing(3)].spacing(8);
        let barrier = presentation
            .as_ref()
            .and_then(|view| view.refined_kcal)
            .filter(|_| failed.is_none() && barrier_refusal.is_none());
        let reaction = batch
            .as_ref()
            .map(|totals| totals.energy)
            .or(endpoint_energy);
        let reaction_gibbs = batch.as_ref().map_or_else(
            || result.and_then(|result| view::reaction_gibbs_kcal(result)),
            |totals| totals.gibbs,
        );
        let barrier_gibbs =
            barrier.and_then(|_| result.and_then(|result| view::barrier_gibbs_kcal(result)));
        let unreliable =
            failed.is_some() || presentation.as_ref().is_some_and(|view| !view.reliable());
        let reference = if batch.is_some() {
            label.has_multiple_species().then_some("separated species")
        } else {
            result.and_then(|result| self.xtb_ts_reference(result, Some(arrow)))
        };
        let method = result.map(|result| view::method_name(result));
        let reaction_method = batch
            .as_ref()
            .map(|totals| totals.method.label())
            .or(method);
        let reaction_number = energy_number(
            unit,
            reaction_method,
            "Reaction ΔE",
            reaction,
            reference,
            "ΔG",
            reaction_gibbs,
        );
        content = if let Some(pair) = linked.filter(|_| barrier_refusal.is_none()) {
            content
                .push(linked_barriers(pair, unit))
                .push(reaction_number)
        } else if unreliable && barrier_refusal.is_none() {
            content.push(reaction_number)
        } else {
            content.push(
                row![
                    container(if let Some(reason) = &barrier_refusal {
                        line(format!("Barrier not available: {reason}"), Tone::Muted)
                    } else {
                        energy_number(
                            unit,
                            method,
                            if barrier.is_some() && batch.is_some() {
                                "Previous barrier ΔE‡"
                            } else {
                                "Barrier ΔE‡"
                            },
                            barrier,
                            None,
                            "ΔG‡",
                            barrier_gibbs,
                        )
                    })
                    .width(Length::FillPortion(1)),
                    container(reaction_number).width(Length::FillPortion(1)),
                ]
                .spacing(12),
            )
        };
        if reaction.is_some() {
            content = content.push(
                accessible_command(
                    "xtb-ts-place-energy-profile",
                    "Place energy profile",
                    "Place energy profile",
                )
                .style(crate::appearance::secondary)
                .on_press_maybe(
                    (idle && (batch.is_some() || !stale)).then_some(Message::Xtb(XtbAction::Ts(
                        TsAction::PlaceEnergyProfile(Some(arrow)),
                    ))),
                ),
            );
        }
        if let Some(failed) = refine_failed {
            let (reason, detail) = failed.explain(self.xtb_can_retry(failed));
            content = content
                .push(line(
                    "g-xTB refinement failed; the GFN2-xTB result is kept.",
                    Tone::Warning,
                ))
                .push(line(
                    if detail.is_empty() { reason } else { detail },
                    Tone::Muted,
                ));
        }
        if let Some(failed) = failed {
            let (reason, detail) = failed.explain(self.xtb_can_retry(failed));
            content = content
                .push(line("No reliable transition state found.", Tone::Warning))
                .push(line(
                    if detail.is_empty() { reason } else { detail },
                    Tone::Muted,
                ))
                .push(
                    xtb::available()
                        .then(|| failure_actions(view::Recovery::FinerPath, arrow, idle, false)),
                );
            if let Some(result) = result {
                content = content.push(line(xtb::results::previous_result(result), Tone::Muted));
            }
        } else if let Some(view) = &presentation
            && let Some(reason) = &view.reason
        {
            content = content
                .push(line("No reliable transition state found.", Tone::Warning))
                .push(line(reason.clone(), Tone::Muted))
                .push(xtb::available().then(|| {
                    failure_actions(
                        view.recovery,
                        arrow,
                        idle && (view.recovery != view::Recovery::ShowWhere || !stale),
                        result.is_some_and(|result| view::has_reached_endpoint(result)),
                    )
                }));
            if let Some(highest) = view.highest_path_kcal {
                content = content.push(line(
                    format!(
                        "{} highest point on the path: about {} above the reactants.",
                        view.method,
                        unit.format(highest)
                    ),
                    Tone::Muted,
                ));
            }
            // An early-stopped IRC descent still connects; the Path tab notes it.
        }
        if barrier.is_some() || reaction.is_some() {
            // The two methods can optimize different endpoint geometries.
            content = content.push(
                canvas(EnergyDiagram {
                    unit,
                    barrier: barrier.filter(|_| batch.is_none()),
                    product: reaction,
                })
                .width(Length::Fill)
                .height(82),
            );
        }
        if let Some(verification) = presentation
            .as_ref()
            .filter(|_| barrier.is_some() && linked.is_none())
        {
            let copy = format!("{} {}", verification.method, verification.verification);
            content = content.push(
                iced::widget::responsive(move |size| {
                    let preferred = 11.;
                    let width = crate::app::workspace::text_width(&copy, preferred).max(1.);
                    toned_text(copy.clone(), Tone::Good)
                        .size(preferred * (size.width.max(1.) / width).min(1.))
                        .wrapping(text::Wrapping::None)
                        .into()
                })
                .height(Length::Shrink),
            );
        }
        if let Some(result) = result {
            let mut actions = row![].spacing(5);
            if !unreliable {
                actions = actions
                    .push(
                        accessible_command(
                            "xtb-ts-card-path",
                            "Show the reaction path",
                            "Show path",
                        )
                        .style(crate::appearance::secondary)
                        .on_press(send(Action::ShowPath)),
                    )
                    .push(
                        accessible_command(
                            "xtb-ts-card-animate",
                            "Animate the transition state's imaginary mode",
                            "Animate TS",
                        )
                        .style(crate::appearance::secondary)
                        .on_press_maybe(
                            (idle && imaginary_mode(result).is_some())
                                .then(|| send(Action::Animate)),
                        ),
                    );
            }
            content = content.push(
                actions
                    .push(
                        accessible_command(
                            "xtb-ts-export-input",
                            "Export TS input",
                            "Export TS input…",
                        )
                        .style(crate::appearance::outline)
                        .on_press(send(Action::Export)),
                    )
                    .wrap()
                    .vertical_spacing(5),
            );
            if xtb::available()
                && view::is_gfn2(result)
                && search.is_some()
                && presentation.as_ref().is_some_and(|view| view.reliable())
            {
                content = content.push(
                    accessible_command(
                        "xtb-ts-refine-gxtb",
                        "Refine with g-xTB",
                        "Refine with g-xTB",
                    )
                    .style(crate::appearance::secondary)
                    .on_press_maybe((idle && !stale).then(|| send(Action::Refine))),
                );
            }
        }

        if barrier.is_some()
            && search.is_some_and(|search| search.free_energy)
            && let Some(reason) = result.and_then(|result| view::barrier_gibbs_unavailable(result))
        {
            content = content.push(line(reason, Tone::Warning));
        }
        if batch.is_none()
            && let Some(reason) = result.and_then(|result| view::reaction_gibbs_unavailable(result))
        {
            content = content.push(line(reason, Tone::Warning));
        }
        if let Some(result) = result.filter(|_| stale) {
            let title = self.xtb_saved_reaction_title(result, Some(arrow));
            let reason = if super::not_verified(result) {
                "Reaction freshness not verified"
            } else {
                "The reaction changed after this search"
            };
            content = content.push(line(format!("{reason} · {title}"), Tone::Warning));
        }
        let drawn = plan
            .as_ref()
            .ok()
            .map(|plan| BondChanges::from_plan(doc, plan));
        if let Some((bonds, current)) = self.xtb_card_bonds(arrow, search, drawn) {
            content = content.push(bonds.view(doc, arrow, current && result.is_some()));
        }
        if let Some(reason) = xtb::unavailable() {
            return crate::app::inspector::card(
                container(content.push(line(reason, Tone::Muted)))
                    .padding(10)
                    .width(Length::Fill),
            );
        }

        if let Some(live) = live {
            let small = live.small_reaction() && live.request.method == XtbMethod::Gfn2Xtb;
            let mut steps = column![stepper(live)].spacing(4);
            if self.xtb_palette_live_ts().is_none() || !self.xtb_palette_body_visible() {
                steps = steps.push(
                    accessible_command(
                        "xtb-ts-show-live-path",
                        "Show the running reaction's live path",
                        "Show live path",
                    )
                    .padding([1, 4])
                    .style(iced::widget::button::text)
                    .on_press(send(Action::ShowPath)),
                );
            }
            content = content
                .push(steps)
                .push(progress_bar(0.0..=1.0, live.fraction().unwrap_or(0.)).girth(3))
                .push(
                    row![
                        column![]
                            .push(small.then(|| line("usually 4–10 s", Tone::Muted)))
                            .width(Length::Fill),
                        accessible_command(
                            "xtb-ts-cancel",
                            "Cancel transition-state search",
                            "Cancel"
                        )
                        .on_press(Message::Xtb(XtbAction::Cancel)),
                    ]
                    .align_y(iced::Alignment::Center),
                );
        } else if self.xtb_automatic_mapping_pending(arrow) {
            content = content.push(line("Matching atoms…", Tone::Muted)).push(
                accessible_command("xtb-ts-mapping-cancel", "Cancel atom matching", "Cancel")
                    .on_press(Message::Xtb(XtbAction::Ts(TsAction::ReviewSheet(
                        review::Action::Cancel,
                    )))),
            );
        } else if let Some(outcome @ Outcome::Review(pending)) = &self.tab.xtb.outcome
            && pending
                .ts
                .as_ref()
                .is_some_and(|session| session.arrow == arrow)
        {
            content = content.push(xtb::view::outcome_view(self, outcome));
        }
        let supports_barrier = self.xtb_reaction_supports_barrier(arrow);
        if supports_barrier {
            content = content.push(label.target_stereo_note.map(|note| line(note, Tone::Muted)));
        }
        content = content
            .push(self.xtb_scheme_ignored(arrow))
            .push(self.xtb_energy_feedback(arrow, batch.is_some()))
            .push(horizontal_line());
        let tick = |id, label, checked, action| {
            accessible_checkbox(id, label, checked, idle.then(|| send(action)))
        };
        content = content.push(tick(
            "xtb-ts-reaction-energy",
            "Reaction energy ΔE",
            state.reaction_energy,
            Action::ReactionEnergy,
        ));
        if supports_barrier {
            content = content.push(
                column![
                    tick(
                        "xtb-ts-barrier",
                        "Barrier ΔE‡ (transition state)",
                        state.barrier,
                        Action::Barrier
                    ),
                    (live.is_none() && self.xtb.settings.barrier_method == XtbMethod::Gfn2Xtb)
                        .then(
                            || container(line("about 5–10 s for small molecules", Tone::Muted),)
                                .padding([0, 19])
                        ),
                ]
                .spacing(1),
            );
        }
        let methods = row([(XtbMethod::Gfn2Xtb, "gfn2"), (XtbMethod::GXtb, "gxtb")]
            .into_iter()
            .map(|(method, id)| {
                xtb::view::segment(
                    format!("xtb-ts-method-{id}"),
                    method.label(),
                    self.xtb.settings.barrier_method == method,
                    idle.then(|| send(Action::Method(method))),
                    None,
                )
            }))
        .spacing(4);
        content = content.push(
            column![
                line("Method", Tone::Muted),
                methods,
                (self.xtb.settings.barrier_method == XtbMethod::GXtb).then(|| {
                    line(
                        "closer to DFT energies; slower for large molecules",
                        Tone::Muted,
                    )
                }),
            ]
            .spacing(3),
        );
        let free_label = if supports_barrier && state.barrier && barrier_refusal.is_none() {
            "Free energies ΔG, ΔG‡ at 298 K"
        } else {
            "Free energy ΔG at 298 K"
        };
        let calculate = if result.is_some() || self.xtb_energy_has_summary(arrow) {
            "Recalculate"
        } else {
            "Calculate"
        };
        content = content.push(
            column![
                tick(
                    "xtb-ts-free-energy",
                    free_label,
                    state.free_energy,
                    Action::FreeEnergy
                ),
                container(line("adds frequencies at both ends", Tone::Muted)).padding([0, 19]),
            ]
            .spacing(1),
        );
        content = content.push(
            row![
                accessible_command("xtb-ts-calculate", calculate, calculate)
                    .style(crate::appearance::primary)
                    .on_press_maybe(
                        self.xtb_can_calculate_reaction(arrow)
                            .then_some(Message::Xtb(XtbAction::Ts(TsAction::Calculate(arrow))))
                    ),
                line(xtb::rerun_keys(), Tone::Muted),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center),
        );
        crate::app::inspector::card(container(content).padding(10).width(Length::Fill))
    }
}

fn stepper(live: &Live) -> Element<'static, Message> {
    column(
        live.steps(iced::time::Instant::now())
            .map(|(label, state)| {
                let (marker, label, elapsed) = match state {
                    StepState::Done(elapsed) => (toned_text("✓", Tone::Good), text(label), elapsed),
                    StepState::Current(elapsed) => (
                        text("●"),
                        text(format!("{} · {}", live.request.method.label(), live.line())).font(
                            iced::Font {
                                weight: iced::font::Weight::Bold,
                                ..iced::Font::DEFAULT
                            },
                        ),
                        Some(elapsed),
                    ),
                    StepState::Todo => (
                        toned_text("○", Tone::Muted),
                        toned_text(label, Tone::Muted),
                        None,
                    ),
                    StepState::Skipped => (
                        toned_text("–", Tone::Muted),
                        toned_text(format!("{label} · skipped"), Tone::Muted),
                        None,
                    ),
                };
                row![
                    marker.size(12).width(16),
                    label.size(12).width(Length::Fill),
                    elapsed.map(|duration| line(
                        format!("{:.1} s", duration.as_secs_f64()),
                        Tone::Muted
                    )),
                ]
                .spacing(4)
                .align_y(iced::Alignment::Center)
                .into()
            }),
    )
    .spacing(4)
    .into()
}

/// Each method keeps its own barrier and checks; a candidate never borrows the
/// source's successful frequency or connectivity check.
pub(super) fn linked_barriers(
    pair: [&Arc<CalcResult>; 2],
    unit: EnergyUnit,
) -> Element<'static, Message> {
    row(pair.into_iter().map(|result| {
        let presentation = view::presentation(result, None);
        let mut details = column![
            energy_number(
                unit,
                Some(&presentation.method),
                "Barrier ΔE‡",
                presentation.refined_kcal,
                None,
                "ΔG‡",
                view::barrier_gibbs_kcal(result)
            ),
            line(
                format!("{} {}", presentation.method, presentation.verification),
                if presentation.reliable() {
                    Tone::Good
                } else {
                    Tone::Muted
                },
            ),
        ]
        .spacing(3);
        if let Some(reason) = presentation.reason {
            details = details.push(line(
                format!("g-xTB refinement failed: {reason}"),
                Tone::Warning,
            ));
        }
        container(details).width(Length::FillPortion(1)).into()
    }))
    .spacing(12)
    .into()
}

fn energy_number(
    unit: EnergyUnit,
    method: Option<&str>,
    label: &str,
    energy: Option<f64>,
    reference: Option<&'static str>,
    gibbs_label: &'static str,
    gibbs: Option<f64>,
) -> Element<'static, Message> {
    let with_method =
        |label: &str| method.map_or_else(|| label.to_owned(), |method| format!("{method} {label}"));
    let mut number = row![
        text(energy.map_or_else(|| "—".into(), |value| unit.number(value)))
            .size(22)
            .font(iced::Font {
                weight: iced::font::Weight::Semibold,
                ..iced::Font::DEFAULT
            }),
    ]
    .spacing(4)
    .align_y(iced::Alignment::Center);
    if energy.is_some() {
        number = number.push(text(unit.label()).size(10).style(muted_text));
    }
    column![
        line(with_method(label), Tone::Muted),
        number,
        reference
            .filter(|_| energy.is_some())
            .map(|reference| line(reference, Tone::Muted)),
        gibbs.map(|value| line(
            format!("{} {}", with_method(gibbs_label), unit.format(value)),
            Tone::Muted
        )),
    ]
    .spacing(1)
    .into()
}

struct EnergyDiagram {
    unit: EnergyUnit,
    barrier: Option<f64>,
    product: Option<f64>,
}

impl canvas::Program<Message> for EnergyDiagram {
    type State = ();

    fn draw(
        &self,
        _: &(),
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let levels = [
            ("R", Some(0.), 0.15),
            ("TS‡", self.barrier, 0.5),
            ("P", self.product, 0.85),
        ];
        let (low, high) = levels
            .iter()
            .filter_map(|(_, energy, _)| *energy)
            .fold((0_f64, 0_f64), |(low, high), value| {
                (low.min(value), high.max(value))
            });
        let point = |value: f64, fraction| {
            let fraction_y = if low == high {
                0.5
            } else {
                ((high - value) / (high - low)) as f32
            };
            Point::new(
                bounds.width * fraction,
                19. + fraction_y * (bounds.height - 28.),
            )
        };
        let mut previous: Option<Point> = None;
        let half_bar = 17.;
        for (label, energy, fraction) in levels {
            let Some(energy) = energy else { continue };
            let here = point(energy, fraction);
            if let Some(last) = previous {
                frame.stroke(
                    &canvas::Path::line(
                        Point::new(last.x + half_bar, last.y),
                        Point::new(here.x - half_bar, here.y),
                    ),
                    canvas::Stroke {
                        line_dash: canvas::LineDash {
                            segments: &[3., 3.],
                            offset: 0,
                        },
                        ..canvas::Stroke::default().with_color(crate::appearance::muted(theme))
                    },
                );
            }
            let color = if label == "TS‡" {
                theme.palette().primary
            } else {
                theme.palette().text
            };
            frame.stroke(
                &canvas::Path::line(
                    Point::new(here.x - half_bar, here.y),
                    Point::new(here.x + half_bar, here.y),
                ),
                canvas::Stroke::default().with_width(2.5).with_color(color),
            );
            frame.fill_text(canvas::Text {
                content: format!("{label} {}", self.unit.number(energy)),
                position: Point::new(here.x, here.y - 5.),
                color,
                size: 10.5.into(),
                align_x: iced::alignment::Horizontal::Center.into(),
                align_y: iced::alignment::Vertical::Bottom,
                ..Default::default()
            });
            previous = Some(here);
        }
        vec![frame.into_geometry()]
    }
}

fn nonempty(value: &str) -> Option<&str> {
    (!value.trim().is_empty()).then_some(value)
}

fn failure_actions(
    recovery: view::Recovery,
    arrow: u64,
    idle: bool,
    has_endpoint: bool,
) -> Element<'static, Message> {
    row![
        recovery.button(
            "xtb-ts-rerun",
            Some(arrow),
            idle,
            has_endpoint,
            TsAction::ShowWhere(arrow)
        ),
        accessible_command(
            "xtb-ts-card-path",
            "Show what was found",
            "Show what was found"
        )
        .style(crate::appearance::secondary)
        .on_press(send(Action::ShowPath)),
    ]
    .spacing(5)
    .wrap()
    .vertical_spacing(5)
    .into()
}

#[cfg(test)]
pub(in crate::app::xtb::ts) mod tests;
