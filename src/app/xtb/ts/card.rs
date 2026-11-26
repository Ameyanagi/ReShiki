//! Predict controls and results for a single selected reaction arrow.
use super::{Action as TsAction, review, view};
use crate::app::workspace::{accessible_command, hover_hint};
use crate::app::xtb::tone::{Tone, line};
use crate::app::xtb::{self, Action as XtbAction, Outcome};
use crate::app::{App, Message};
use iced::widget::{
    column, container, progress_bar, responsive, rich_text, row, span, text, tooltip,
};
use iced::{Element, Length, Task};
use reshiki::document::Document;
use reshiki::palette::{Color, Palette};
use reshiki::qcdata::{CalcResult, MappedAtom, TransitionStateMapping};
use reshiki::reactions::Reaction;
use reshiki::reactions::ts_plan::{self, Side, TsPlan, TsPlanError};
use std::sync::Arc;

type Scope = (u64, u64);

#[derive(Default)]
pub(in crate::app::xtb) struct State {
    automatic_maps: Vec<(Scope, TsPlan)>,
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
    Find,
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

fn mapped_label_color(doc: &Document, reaction: &Reaction, atom: &MappedAtom) -> Color {
    reaction
        .reactants
        .iter()
        .chain(&reaction.products)
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
        let label = |atom: &MappedAtom| {
            let color = reaction.as_ref().map_or(Color::Ink, |reaction| {
                mapped_label_color(doc, reaction, atom)
            });
            span(format!("{}{}", atom.element, atom.number))
                .color(crate::appearance::from_rgb(palette.rgb(color)))
        };
        let mut spans = vec![];
        if current_only {
            spans.push(span("Current map · "));
        }
        for (heading, separator, bonds) in [
            ("Bonds formed: ", "–", &self.0.formed),
            (" · broken: ", "–", &self.0.broken),
            (" · Hydrogen moves: ", " → ", &self.0.hydrogen_moves),
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
                spans.extend([label(a), span(separator), label(b)]);
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
            },
            |reaction| ReactionLabel {
                reactants: names(&reaction.reactants),
                products: names(&reaction.products),
            },
        )
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
        self.xtb_show_result(&result);
        self.xtb_end_preview();
        self.tab.xtb.palette.view = crate::app::xtb::results::View::Vibrations;
        self.tab.xtb.vibrations.mode = mode;
        self.tab.xtb.vibrations.playing = true;
        self.xtb.dock.collapsed = false;
        let shown = if self.xtb.dock.inspector {
            self.xtb.page = crate::app::xtb::Page::Xtb;
            self.show_inspector_tab(crate::app::InspectorTab::Predict)
                .unwrap_or_else(Task::none)
        } else {
            Task::none()
        };
        Task::batch([shown, self.xtb_vibrations_opened()])
    }

    pub(in crate::app::xtb) fn xtb_ts_card_action(&mut self, action: Action) -> Task<Message> {
        let Some(arrow) = self.xtb_selected_reaction() else {
            return Task::none();
        };
        match action {
            Action::Find => self.xtb_review_open(review::Mode::TransitionState, Some(arrow)),
            Action::FinerPath => self.xtb_review_rerun(arrow),
            Action::Export => self.xtb_ts_export_arrow(arrow),
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
                self.xtb.dock.collapsed = false;
                self.xtb_vibrations_sync();
                Task::none()
            }
        }
    }

    pub(in crate::app::xtb) fn xtb_reaction_card(&self, arrow: u64) -> Element<'_, Message> {
        let doc = &self.tab.doc;
        let label = self.xtb_reaction_label(arrow);
        let plan = review::scheme_plan(doc, arrow).ok();
        let auto = plan.as_ref().is_some_and(|plan| {
            self.tab
                .xtb
                .ts_card
                .automatic_map((self.tab.file_epoch, arrow), plan)
        });
        let (exportable, auto) = match self.xtb_ts_card_result(arrow) {
            ArrowResult::Found(result) => (
                true,
                auto || super::saved_search(result).is_some_and(|search| {
                    search.mapping.automatic && !self.xtb_ts_search_stale(result)
                }),
            ),
            _ => (false, auto),
        };
        let idle = self.tab.xtb.run.is_none()
            && !self.tab.xtb.energy_state.in_progress()
            && !self.tab.busy
            && !self.ts_review_open();
        let subtitle = if auto {
            "GFN2-xTB · gas phase · atoms matched automatically ✓"
        } else if plan.is_some() {
            "GFN2-xTB · gas phase · atoms matched ✓"
        } else {
            "GFN2-xTB · gas phase"
        };
        let heading = column![
            text(format!("Reaction (xTB) · {}", label.title())).size(14),
            row![
                line(subtitle, Tone::Muted),
                accessible_command("xtb-ts-review", "Review the reaction's atom map", "review")
                    .padding([1, 4])
                    .style(iced::widget::button::text)
                    .on_press_maybe(
                        (idle && xtb::available())
                            .then_some(Message::Xtb(XtbAction::Ts(TsAction::ReviewMap(arrow))))
                    ),
            ]
            .spacing(4)
            .align_y(iced::Alignment::Center)
            .wrap(),
        ]
        .spacing(3);
        let sections = responsive(move |size| {
            let energy = self.xtb_reaction_energy_section(arrow);
            let barrier = self.xtb_ts_barrier_section(arrow, idle);
            if size.width >= 250. {
                row![
                    container(energy).width(Length::FillPortion(1)),
                    container(barrier).width(Length::FillPortion(1)),
                ]
                .spacing(10)
                .into()
            } else {
                column![energy, barrier].spacing(12).into()
            }
        })
        .height(Length::Shrink);
        let actions = row![
            hover_hint(
                accessible_command(
                    "xtb-ts-place-profile",
                    "Place energy profile",
                    "Place energy profile"
                ),
                "Coming next",
                tooltip::Position::Top,
            ),
            accessible_command("xtb-ts-export-input", "Export TS input", "Export TS input…")
                .on_press_maybe(exportable.then(|| send(Action::Export))),
        ]
        .spacing(6)
        .wrap()
        .vertical_spacing(6);
        let mut content = column![heading].spacing(10);
        if let Some(reason) = xtb::unavailable() {
            content = content.push(line(reason, Tone::Muted));
        }
        if let Some(ignored) = self.xtb_scheme_ignored(arrow) {
            content = content.push(ignored);
        }
        content = content.push(sections).push(actions);
        crate::app::inspector::card(container(content).padding(10).width(Length::Fill))
    }

    fn xtb_ts_barrier_section(&self, arrow: u64, idle: bool) -> Element<'_, Message> {
        let mut body = column![text("Barrier (transition state)").size(13)]
            .spacing(7)
            .width(Length::Fill);
        let doc = &self.tab.doc;
        if let Some(live) = self
            .tab
            .xtb
            .run
            .as_ref()
            .and_then(|run| run.ts.as_ref())
            .filter(|live| live.request.arrow == arrow)
        {
            return body
                .push(live.request.bonds.view(doc, arrow, false))
                .push(line(live.line(), Tone::Muted))
                .push(progress_bar(0.0..=1.0, live.fraction().unwrap_or(0.)).girth(3))
                .push(
                    accessible_command("xtb-ts-cancel", "Cancel transition-state search", "Cancel")
                        .on_press(Message::Xtb(XtbAction::Cancel)),
                )
                .into();
        }
        if self.xtb_automatic_mapping_pending(arrow) {
            return body
                .push(line("Matching atoms…", Tone::Muted))
                .push(
                    accessible_command("xtb-ts-mapping-cancel", "Cancel atom matching", "Cancel")
                        .on_press(Message::Xtb(XtbAction::Ts(TsAction::ReviewSheet(
                            review::Action::Cancel,
                        )))),
                )
                .into();
        }
        if let Some(outcome @ Outcome::Review(pending)) = &self.tab.xtb.outcome
            && pending
                .ts
                .as_ref()
                .is_some_and(|session| session.arrow == arrow)
        {
            return body.push(xtb::view::outcome_view(outcome, true)).into();
        }

        let can_start = idle && xtb::available();
        let failed = if let Some(Outcome::Failed(failed)) = &self.tab.xtb.outcome
            && let Some(request) = failed.ts().filter(|request| request.arrow == arrow)
        {
            let (reason, detail) = failed.explain();
            body = body
                .push(request.bonds.view(doc, arrow, false))
                .push(line("No reliable transition state found.", Tone::Warning))
                .push(line(
                    if detail.is_empty() { reason } else { detail },
                    Tone::Muted,
                ))
                .push(finer_path(can_start));
            true
        } else {
            false
        };

        match self.xtb_ts_card_result(arrow) {
            ArrowResult::Found(result) => {
                if failed {
                    body = body.push(line(xtb::results::previous_result(result), Tone::Muted));
                }
                let session = self
                    .tab
                    .xtb
                    .ts_sessions
                    .get(result)
                    .map(|session| session.as_ref());
                let search = super::saved_search(result);
                if let Some(search) = search {
                    if !failed {
                        body =
                            body.push(BondChanges(search.mapping.clone()).view(doc, arrow, false));
                    }
                } else if let Ok(plan) = ts_plan::plan_for_review(doc, arrow) {
                    body = body.push(BondChanges::from_plan(doc, &plan).view(doc, arrow, true));
                }
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
                    body = body.push(line(
                        if super::not_verified(result) {
                            format!("Reaction freshness not verified · {title}")
                        } else {
                            format!("The reaction changed after this search · {title}")
                        },
                        Tone::Warning,
                    ));
                }
                let result_view = view::presentation(result, session);
                if let Some(energy) = result_view.barrier_line() {
                    body = body
                        .push(line(
                            energy,
                            if stale || failed {
                                Tone::Muted
                            } else {
                                Tone::Good
                            },
                        ))
                        .push(line(result_view.verification, Tone::Muted));
                    if let Some(warning) = result_view.warning {
                        body = body.push(line(warning, Tone::Warning));
                    }
                    body = body.push(
                        row![
                            accessible_command(
                                "xtb-ts-card-path",
                                "Show the reaction path",
                                "Show path"
                            )
                            .on_press(send(Action::ShowPath)),
                            accessible_command(
                                "xtb-ts-card-animate",
                                "Animate the transition state's imaginary mode",
                                "Animate"
                            )
                            .on_press_maybe(idle.then(|| send(Action::Animate))),
                        ]
                        .spacing(6)
                        .wrap(),
                    );
                } else {
                    let mut actions = row![].spacing(6);
                    if !failed {
                        actions = actions.push(finer_path(can_start));
                    }
                    body = body
                        .push(line("No reliable transition state found.", Tone::Warning))
                        .push(line(result_view.reason.unwrap_or_default(), Tone::Muted))
                        .push(
                            actions
                                .push(
                                    accessible_command(
                                        "xtb-ts-card-path",
                                        "Show the path found by the search",
                                        "Show what was found",
                                    )
                                    .on_press(send(Action::ShowPath)),
                                )
                                .wrap()
                                .vertical_spacing(6),
                        );
                    if let Some(highest) = result_view.highest_path_kcal {
                        body = body.push(line(
                            format!("Highest point on the path: about {highest:.0} kcal/mol above the reactants (not a transition state)."),
                            Tone::Muted,
                        ));
                    }
                    return body.into();
                }
            }
            _ if failed => return body.into(),
            ArrowResult::Missing | ArrowResult::Ambiguous => {
                match review::scheme_plan(doc, arrow) {
                    Ok(plan) => {
                        body = body.push(BondChanges::from_plan(doc, &plan).view(doc, arrow, false))
                    }
                    Err(TsPlanError::Shape { .. }) => {
                        body = body.push(line(review::SHAPE_NOTE, Tone::Muted))
                    }
                    Err(
                        error @ (TsPlanError::TooLarge { .. }
                        | TsPlanError::Unsupported(_)
                        | TsPlanError::NoChange),
                    ) => {
                        body = body.push(line(error.to_string(), Tone::Muted));
                    }
                    Err(_) => {}
                }
            }
        }
        let label = "Find transition state";
        body.push(
            accessible_command("xtb-ts-find", label, label)
                .style(crate::appearance::primary)
                .on_press_maybe(can_start.then(|| send(Action::Find))),
        )
        .into()
    }
}

fn nonempty(value: &str) -> Option<&str> {
    (!value.trim().is_empty()).then_some(value)
}

fn finer_path(enabled: bool) -> Element<'static, Message> {
    let label = "Try again with a finer path";
    accessible_command("xtb-ts-rerun", label, label)
        .style(crate::appearance::primary)
        .on_press_maybe(enabled.then(|| send(Action::FinerPath)))
        .into()
}

#[cfg(test)]
pub(in crate::app::xtb::ts) mod tests;
