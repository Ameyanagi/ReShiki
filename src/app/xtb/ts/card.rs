//! Predict controls and results for a single selected reaction arrow.
use super::{Action as TsAction, review, view};
use crate::app::workspace::{accessible_checkbox, accessible_command, horizontal_line, muted_text};
use crate::app::xtb::tone::{Tone, line, toned_text};
use crate::app::xtb::{self, Action as XtbAction, EnergyUnit, Outcome};
use crate::app::{App, Message};
use iced::widget::{canvas, column, container, progress_bar, rich_text, row, span, text};
use iced::{Element, Length, Point, Rectangle, Renderer, Task, Theme, mouse};
use reshiki::document::Document;
use reshiki::palette::{Color, Palette};
use reshiki::qcdata::{CalcResult, MappedAtom, TransitionStateMapping};
use reshiki::reactions::Reaction;
use reshiki::reactions::ts_plan::{Side, TsPlan, TsPlanError};
use std::cell::Cell;
use std::sync::Arc;

type Scope = (u64, u64);
type StereoCacheEntry = ((Scope, u64), Option<&'static str>);

pub(in crate::app::xtb) struct State {
    pub(super) reaction_energy: bool,
    pub(super) barrier: bool,
    pub(super) free_energy: bool,
    pub(in crate::app::xtb) status: Option<(u64, Status, String)>,
    automatic_maps: Vec<(Scope, TsPlan)>,
    target_stereo: Cell<Option<StereoCacheEntry>>,
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
    ShowPath,
    FinerPath,
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
    reactants: Vec<(u16, String)>,
    products: Vec<(u16, String)>,
    target_stereo_note: Option<&'static str>,
}

impl ReactionLabel {
    pub(in crate::app::xtb) fn title(&self) -> String {
        let side = |species: &[(u16, String)], empty: &str| {
            if species.is_empty() {
                return empty.to_owned();
            }
            species
                .iter()
                .map(|(coefficient, name)| {
                    if *coefficient == 1 {
                        name.clone()
                    } else {
                        format!("{coefficient} {name}")
                    }
                })
                .collect::<Vec<_>>()
                .join(" + ")
        };
        format!(
            "{} → {}",
            side(&self.reactants, "Reactants"),
            side(&self.products, "Products")
        )
    }

    pub(super) fn first_reactant(&self) -> Option<&str> {
        self.reactants.first().map(|(_, name)| name.as_str())
    }
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
        let mut spans = vec![];
        if current_only {
            spans.push(span("Current map · "));
        }
        for (heading, separator, bonds, side) in [
            ("Bonds formed: ", "–", &self.0.formed, Side::Product),
            (" · broken: ", "–", &self.0.broken, Side::Reactant),
            (
                " · Hydrogen moves: ",
                " → ",
                &self.0.hydrogen_moves,
                Side::Reactant,
            ),
        ] {
            if bonds.is_empty() && separator == " → " {
                continue;
            }
            spans.push(span(heading));
            if bonds.is_empty() {
                spans.push(span("none"));
            }
            for (index, [a, b]) in bonds.iter().enumerate() {
                if index != 0 {
                    spans.push(span(", "));
                }
                spans.extend([label(a, side), span(separator), label(b, side)]);
            }
        }
        rich_text::<(), _, _, _>(spans).size(12).into()
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

    pub(in crate::app::xtb) fn xtb_reaction_label(&self, arrow: u64) -> ReactionLabel {
        let doc = &self.tab.doc;
        let names = |participants: &[reshiki::reactions::Participant]| {
            participants
                .iter()
                .flat_map(|participant| {
                    reshiki::reactions::molecules(doc, &participant.atoms)
                        .into_iter()
                        .map(|atoms| {
                            let link = reshiki::linked_properties::caption(doc, &atoms);
                            let name = link
                                .and_then(|link| nonempty(&link.custom_name))
                                .or_else(|| {
                                    reshiki::molecule_names::label(doc, &atoms)
                                        .and_then(|name| nonempty(&name.text))
                                })
                                .or_else(|| self.xtb_existing_molecule_name(&atoms))
                                .or_else(|| {
                                    link.filter(|link| !link.needs_name())
                                        .and_then(|link| nonempty(&link.name))
                                })
                                .map(str::to_owned)
                                .or_else(|| review::species_formula(doc, &atoms))
                                .unwrap_or_else(|| "Unnamed molecule".into());
                            (participant.coefficient, name)
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
        let Some(arrow) = self.xtb_selected_reaction() else {
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
            Action::FinerPath => return self.xtb_review_rerun(arrow),
            Action::Export => return self.xtb_ts_export_arrow(arrow),
            Action::Animate | Action::ShowPath => {
                let ArrowResult::Found(result) = self.xtb_ts_card_result(arrow) else {
                    return Task::none();
                };
                let result = Arc::clone(result);
                if matches!(action, Action::Animate) {
                    return self.xtb_ts_animate(result);
                }
                self.xtb_show_result(&result);
                self.tab.xtb.palette.view = xtb::results::View::Path;
                self.tab.xtb.palette.collapsed = false;
                self.xtb_vibrations_sync();
            }
        }
        Task::none()
    }

    pub(in crate::app::xtb) fn xtb_reaction_card(&self, arrow: u64) -> Element<'_, Message> {
        let doc = &self.tab.doc;
        let unit = self.appearance.energy_unit;
        let state = &self.tab.xtb.ts_card;
        let label = self.xtb_reaction_label(arrow);
        let plan = review::scheme_plan(doc, arrow);
        let result = match self.xtb_ts_card_result(arrow) {
            ArrowResult::Found(result) => Some(result),
            _ => None,
        };
        let search = result.and_then(|result| super::saved_search(result));
        let presentation = result.map(|result| {
            view::presentation(
                result,
                self.tab.xtb.ts_sessions.get(result).map(AsRef::as_ref),
            )
        });
        let failed = match &self.tab.xtb.outcome {
            Some(Outcome::Failed(failed)) => failed
                .ts()
                .filter(|request| request.arrow == arrow)
                .map(|request| (failed, request)),
            _ => None,
        };
        let idle = self.tab.xtb.run.is_none()
            && !self.tab.xtb.energy_state.in_progress()
            && !self.tab.busy
            && !self.ts_review_open();
        let matched = plan.is_ok()
            || result.zip(search).is_some_and(|(result, search)| {
                search.mapping.automatic && !self.xtb_ts_search_stale(result)
            });
        let subtitle = if matched {
            "GFN2-xTB · gas phase · atoms matched ✓"
        } else {
            "GFN2-xTB · gas phase"
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
        let mut content =
            column![column![text(label.title()).size(14), subtitle].spacing(3)].spacing(8);
        let barrier = presentation
            .as_ref()
            .and_then(|view| view.refined_kcal)
            .filter(|_| failed.is_none());
        let endpoint_energy = result.and_then(|result| view::reaction_energy_kcal(result));
        let batch = self.xtb_energy_totals(arrow, result.filter(|_| endpoint_energy.is_some()));
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
        let reaction_number = energy_number(unit, "Reaction ΔE", reaction, "ΔG", reaction_gibbs);
        content = if unreliable {
            content.push(reaction_number)
        } else {
            content.push(
                row![
                    container(energy_number(
                        unit,
                        if barrier.is_some() && batch.is_some() {
                            "Previous barrier ΔE‡"
                        } else {
                            "Barrier ΔE‡"
                        },
                        barrier,
                        "ΔG‡",
                        barrier_gibbs
                    ))
                    .width(Length::FillPortion(1)),
                    container(reaction_number).width(Length::FillPortion(1)),
                ]
                .spacing(12),
            )
        };
        if let Some((failed, request)) = failed {
            let (reason, detail) = failed.explain(self.xtb_can_retry(failed));
            content = content
                .push(request.bonds.view(doc, arrow, false))
                .push(line("No reliable transition state found.", Tone::Warning))
                .push(line(
                    if detail.is_empty() { reason } else { detail },
                    Tone::Muted,
                ))
                .push(finer_path(idle && xtb::available()));
            if let Some(result) = result {
                content = content.push(line(xtb::results::previous_result(result), Tone::Muted));
            }
        } else if let Some(view) = &presentation {
            if let Some(reason) = &view.reason {
                content = content
                    .push(line("No reliable transition state found.", Tone::Warning))
                    .push(line(reason.clone(), Tone::Muted))
                    .push(finer_path(idle && xtb::available()));
                if let Some(highest) = view.highest_path_kcal {
                    content = content.push(line(
                        format!(
                            "Highest point on the path: about {} above the reactants.",
                            unit.format(highest)
                        ),
                        Tone::Muted,
                    ));
                }
            }
            if let Some(warning) = &view.warning {
                content = content.push(line(warning.clone(), Tone::Warning));
            }
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
        if let Some(verification) = presentation.as_ref().filter(|_| barrier.is_some()) {
            let copy = verification.verification.clone();
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
            let mut actions = row![
                accessible_command(
                    "xtb-ts-card-path",
                    "Show the reaction path",
                    if unreliable {
                        "Show what was found"
                    } else {
                        "Show path"
                    }
                )
                .style(crate::appearance::secondary)
                .on_press(send(Action::ShowPath)),
            ]
            .spacing(5);
            if !unreliable {
                actions = actions.push(
                    accessible_command(
                        "xtb-ts-card-animate",
                        "Animate the transition state's imaginary mode",
                        "Animate TS",
                    )
                    .style(crate::appearance::secondary)
                    .on_press_maybe(
                        (idle && imaginary_mode(result).is_some()).then(|| send(Action::Animate)),
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
        }

        if batch.is_none()
            && reaction_gibbs.is_some()
            && (label.reactants.len() > 1 || label.products.len() > 1)
        {
            content = content.push(line(
                "Free energies use the optimized endpoint complexes.",
                Tone::Muted,
            ));
        }
        if batch.is_none()
            && reaction_gibbs.is_none()
            && search.is_some_and(|search| search.free_energy)
        {
            content = content.push(line(
                "Free energies unavailable. See the calculation log.",
                Tone::Warning,
            ));
        }
        if let Some(result) = result {
            let stale = doc
                .science
                .results
                .iter()
                .find(|stored| Arc::ptr_eq(result, &stored.result))
                .and_then(|stored| doc.science.attachment(stored.attachment))
                .is_some_and(|link| link.freshness(doc) == reshiki::science::Freshness::Stale)
                || self.xtb_ts_search_stale(result);
            if stale {
                let title = self.xtb_saved_reaction_title(result, Some(arrow));
                let reason = if super::not_verified(result) {
                    "Reaction freshness not verified"
                } else {
                    "The reaction changed after this search"
                };
                content = content.push(line(format!("{reason} · {title}"), Tone::Warning));
            }
        }
        if let Some(reason) = xtb::unavailable() {
            return crate::app::inspector::card(
                container(content.push(line(reason, Tone::Muted)))
                    .padding(10)
                    .width(Length::Fill),
            );
        }

        if let Some(live) = self
            .tab
            .xtb
            .run
            .as_ref()
            .and_then(|run| run.ts.as_ref())
            .filter(|live| live.request.arrow == arrow)
        {
            content = content
                .push(live.request.bonds.view(doc, arrow, false))
                .push(line(live.line(), Tone::Muted))
                .push(progress_bar(0.0..=1.0, live.fraction().unwrap_or(0.)).girth(3))
                .push(
                    accessible_command("xtb-ts-cancel", "Cancel transition-state search", "Cancel")
                        .on_press(Message::Xtb(XtbAction::Cancel)),
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
        } else if let Some(search) = search {
            content = content.push(BondChanges(search.mapping.clone()).view(doc, arrow, false));
        } else if let Ok(plan) = &plan {
            content =
                content.push(BondChanges::from_plan(doc, plan).view(doc, arrow, result.is_some()));
        }
        let supports_barrier = self.xtb_reaction_supports_barrier(arrow);
        if supports_barrier {
            content = content.push(label.target_stereo_note.map(|note| line(note, Tone::Muted)));
            if let Err(
                error @ (TsPlanError::TooLarge { .. }
                | TsPlanError::Unsupported(_)
                | TsPlanError::NoChange),
            ) = &plan
            {
                content = content.push(line(error.to_string(), Tone::Muted));
            }
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
                    container(line("about 5–10 s for small molecules", Tone::Muted))
                        .padding([0, 19]),
                ]
                .spacing(1),
            );
        }
        let free_label = if supports_barrier && state.barrier {
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

fn energy_number(
    unit: EnergyUnit,
    label: &'static str,
    energy: Option<f64>,
    gibbs_label: &'static str,
    gibbs: Option<f64>,
) -> Element<'static, Message> {
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
        line(label, Tone::Muted),
        number,
        gibbs.map(|value| line(format!("{gibbs_label} {}", unit.format(value)), Tone::Muted)),
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

fn finer_path(enabled: bool) -> Element<'static, Message> {
    let label = "Try again with a finer path";
    accessible_command("xtb-ts-rerun", label, label)
        .style(crate::appearance::secondary)
        .on_press_maybe(enabled.then(|| send(Action::FinerPath)))
        .into()
}

#[cfg(test)]
pub(in crate::app::xtb::ts) mod tests;
