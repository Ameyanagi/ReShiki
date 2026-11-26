//! The selected reaction arrow's transition-state card and its reviewed inputs.
use super::Action as TsAction;
use crate::app::workspace::{accessible_checkbox, accessible_command, muted_text};
use crate::app::xtb::tone::{Tone, line};
use crate::app::xtb::{Action as XtbAction, state::Settings as CalcSettings};
use crate::app::{App, Message};
use iced::widget::{Space, column, container, progress_bar, row, text};
use iced::{Alignment, Element, Length, Task};
use reshiki::document::Document;
use reshiki::qcdata::CalcResult;
use reshiki::reactions::ts_plan::{self, Side, TsPlan, TsPlanError};
use std::sync::Arc;

const TITLE: &str = "Transition state (xTB)";
const FIND: &str = "Find transition state (xTB)…";
type Scope = (u64, u64);

/// These fields belong to the document, while typed electronic settings and
/// hydrogen review belong to one arrow in one loaded file.
pub(in crate::app::xtb) struct State {
    open: bool,
    search_settings: bool,
    images: String,
    charge: String,
    unpaired: String,
    scope: Option<Scope>,
    review: Option<(TsPlan, bool)>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            open: true,
            search_settings: false,
            images: "15".into(),
            charge: String::new(),
            unpaired: String::new(),
            scope: None,
            review: None,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum Action {
    Open(bool),
    SearchSettings,
    Images(String),
    Charge(String),
    Unpaired(String),
    Reviewed(bool),
    Find,
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

#[derive(Debug, PartialEq, Eq)]
struct Config {
    images: usize,
    charge: i32,
    unpaired: u32,
}

impl State {
    fn plan(&self, doc: &Document, scope: Scope) -> Result<TsPlan, TsPlanError> {
        let drawn = ts_plan::plan(doc, scope.1);
        if drawn.is_err()
            && self.scope == Some(scope)
            && let Some((review, _)) = &self.review
            && let Ok(plan) = ts_plan::plan_for_review(doc, scope.1)
            && &plan == review
        {
            return Ok(plan);
        }
        drawn
    }

    pub(super) fn accept_review(
        &mut self,
        scope: Scope,
        plan: &TsPlan,
        charge: i32,
        unpaired: u32,
    ) -> usize {
        self.retarget(scope);
        self.open = true;
        self.charge = charge.to_string();
        self.unpaired = unpaired.to_string();
        self.review = Some((plan.clone(), true));
        let images = self
            .images
            .trim()
            .parse::<usize>()
            .ok()
            .filter(|images| (10..=20).contains(images))
            .unwrap_or(15);
        self.images = images.to_string();
        images
    }

    fn retarget(&mut self, scope: Scope) {
        if self.scope != Some(scope) {
            self.scope = Some(scope);
            self.charge.clear();
            self.unpaired.clear();
            self.review = None;
        }
    }

    fn typed(&self, scope: Scope) -> (&str, &str) {
        if self.scope == Some(scope) {
            (&self.charge, &self.unpaired)
        } else {
            ("", "")
        }
    }

    fn reviewed(&self, scope: Scope, plan: &TsPlan) -> bool {
        self.review
            .as_ref()
            .filter(|(reviewed, _)| self.scope == Some(scope) && reviewed == plan)
            .map_or(plan.hydrogen_moves.is_empty(), |(_, checked)| *checked)
    }

    fn config(&self, doc: &Document, scope: Scope, plan: &TsPlan) -> Result<Config, String> {
        let images = self
            .images
            .trim()
            .parse::<usize>()
            .ok()
            .filter(|images| (10..=20).contains(images))
            .ok_or("Images must be a whole number from 10 to 20")?;
        let (charge, unpaired) = self.typed(scope);
        let (charge, unpaired) = CalcSettings {
            charge: charge.into(),
            unpaired: unpaired.into(),
            ..Default::default()
        }
        .state(drawn_state(doc, plan))?;
        // Count the implicit hydrogens as well as the atoms in the drawing,
        // so the same electron/spin check as Calculate applies before Run.
        let drawn_protons = plan.start_atoms.iter().try_fold(0u32, |total, id| {
            let atom = doc.atom(*id).ok_or("The reaction drawing changed")?;
            let z = reshiki::editing::ELEMENTS
                .iter()
                .position(|element| *element == atom.element)
                .ok_or("The reaction contains an unknown element")?;
            Ok::<_, &str>(total + z as u32 + 1)
        })?;
        let implicit_h = plan.atoms.saturating_sub(plan.start_atoms.len()) as u32;
        crate::app::xtb::state::electrons(drawn_protons + implicit_h, charge, unpaired)?;
        Ok(Config {
            images,
            charge,
            unpaired,
        })
    }

    fn ready(&self, doc: &Document, scope: Scope, plan: &TsPlan, idle: bool) -> bool {
        idle && self.reviewed(scope, plan) && self.config(doc, scope, plan).is_ok()
    }
}

fn drawn_state(doc: &Document, plan: &TsPlan) -> (i32, u32) {
    plan.start_atoms.first().map_or((0, 0), |&atom| {
        let (charge, multiplicity) = reshiki::science::drawn_state(doc, atom);
        (charge, multiplicity.saturating_sub(1))
    })
}

fn send(action: Action) -> Message {
    Message::Xtb(XtbAction::Ts(TsAction::Card(action)))
}

/// Use map numbers rather than drawing IDs in the chemistry-facing summary.
fn atom_label(doc: &Document, id: u64) -> String {
    doc.atom(id).map_or_else(
        || "?".into(),
        |atom| format!("{}{}", atom.element, atom.map_num),
    )
}

#[derive(Debug, PartialEq, Eq)]
struct Summary {
    atom_map: String,
    bonds: String,
    hydrogens: Vec<String>,
}

fn summary(doc: &Document, plan: &TsPlan) -> Summary {
    // The plan runs away from the connected side. Keep the review in the
    // reaction arrow's direction even when a 2→1 search starts on product.
    let (forming, breaking) = match plan.start {
        Side::Reactant => (&plan.form, &plan.break_),
        Side::Product => (&plan.break_, &plan.form),
    };
    let heavy = plan
        .start_atoms
        .iter()
        .filter_map(|id| doc.atom(*id))
        .filter(|atom| atom.element != "H")
        .count();
    let pairs = |bonds: &[(u64, u64)]| {
        if bonds.is_empty() {
            return "none".into();
        }
        bonds
            .iter()
            .map(|&(a, b)| format!("{}–{}", atom_label(doc, a), atom_label(doc, b)))
            .collect::<Vec<_>>()
            .join(", ")
    };
    Summary {
        atom_map: format!("Atom map: {heavy} of {heavy} heavy atoms"),
        bonds: format!(
            "Bonds formed: {} · broken: {}",
            pairs(forming),
            pairs(breaking)
        ),
        hydrogens: if plan.hydrogen_moves.is_empty() {
            vec!["Hydrogens that move: none".into()]
        } else {
            std::iter::once("Hydrogens that move:".into())
                .chain(plan.hydrogen_moves.iter().map(|&(from, to)| {
                    let (from, to) = match plan.start {
                        Side::Reactant => (from, to),
                        Side::Product => (to, from),
                    };
                    format!(
                        "H from {} to {} (least motion)",
                        atom_label(doc, from),
                        atom_label(doc, to)
                    )
                }))
                .collect()
        },
    }
}

fn subtitle(doc: &Document, arrow: u64, plan: &Result<TsPlan, TsPlanError>) -> String {
    let shape = match plan {
        Ok(plan) => Some(plan.shape),
        Err(TsPlanError::Shape {
            reactants,
            products,
        }) => Some((*reactants, *products)),
        _ => reshiki::reaction_mapping::reaction(doc, arrow)
            .ok()
            .filter(|reaction| reaction.ready())
            .map(|reaction| {
                let count = |participants: &[reshiki::reactions::Participant]| {
                    let atoms: Vec<_> = participants
                        .iter()
                        .flat_map(|participant| participant.atoms.iter().copied())
                        .collect();
                    reshiki::reactions::molecules(doc, &atoms).len()
                };
                (count(&reaction.reactants), count(&reaction.products))
            }),
    };
    shape.map_or_else(
        || "GFN2-xTB".into(),
        |(reactants, products)| format!("GFN2-xTB · {reactants} → {products}"),
    )
}

impl App {
    /// Follow this arrow's TS result even when another calculation is shown.
    fn xtb_ts_card_result(&self, arrow: u64) -> Option<&Arc<CalcResult>> {
        self.tab
            .doc
            .science
            .results
            .iter()
            .rev()
            .find_map(|stored| {
                let result = &stored.result;
                if !super::is_ts(result) {
                    return None;
                }
                let matches = self.tab.xtb.ts_sessions.get(result).map_or_else(
                    || {
                        self.tab
                            .doc
                            .science
                            .attachment(stored.attachment)
                            .is_some_and(|link| {
                                super::matching_plans(&self.tab.doc, &link.atoms)
                                    .any(|(candidate, _)| candidate == arrow)
                            })
                    },
                    |session| session.arrow == arrow,
                );
                matches.then_some(result)
            })
    }

    /// Open the saved saddle's imaginary mode without applying its geometry.
    pub(in crate::app::xtb) fn xtb_ts_animate(&mut self, result: Arc<CalcResult>) -> Task<Message> {
        if self.xtb_running() || self.tab.busy || !super::is_ts(&result) {
            return Task::none();
        }
        let Some((mode, _)) = imaginary_mode(&result) else {
            return Task::none();
        };
        self.tab.xtb.palette.show(&result);
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
        if let Action::Open(open) = action {
            self.tab.xtb.ts_card.open = open;
            return if open {
                Task::none()
            } else {
                crate::app::focus_drawing()
            };
        }
        if matches!(action, Action::SearchSettings) {
            self.tab.xtb.ts_card.search_settings = !self.tab.xtb.ts_card.search_settings;
            return Task::none();
        }
        if !crate::app::xtb::available() || self.tab.xtb.run.is_some() || self.tab.busy {
            return Task::none();
        }
        let Some(arrow) = self.selected_reaction_arrow().filter(|arrow| {
            arrow.purpose.is_chemistry()
                && reshiki::arrows::Preset::from_kind(&arrow.kind)
                    .reaction_role_refusal()
                    .is_none()
        }) else {
            return Task::none();
        };
        let arrow = arrow.id;
        let scope = (self.tab.file_epoch, arrow);
        let card = &mut self.tab.xtb.ts_card;
        card.retarget(scope);
        match action {
            Action::Images(value) => card.images = value,
            Action::Charge(value) => card.charge = value,
            Action::Unpaired(value) => card.unpaired = value,
            Action::Reviewed(checked) => {
                if let Ok(plan) = card.plan(&self.tab.doc, scope) {
                    card.review = Some((plan, checked));
                }
            }
            Action::Find => {
                let Ok(plan) = card.plan(&self.tab.doc, scope) else {
                    return Task::none();
                };
                if !card.reviewed(scope, &plan) {
                    return Task::none();
                }
                if let Ok(config) = card.config(&self.tab.doc, scope, &plan) {
                    return self.xtb_ts_begin(
                        arrow,
                        plan,
                        config.images,
                        config.charge,
                        config.unpaired,
                    );
                }
            }
            Action::Animate => {
                if let Some(result) = self.xtb_ts_card_result(arrow).cloned() {
                    return self.xtb_ts_animate(result);
                }
            }
            Action::Open(_) | Action::SearchSettings => {}
        }
        Task::none()
    }

    /// Shown next to Reaction roles only for one selected chemistry arrow.
    pub(in crate::app) fn xtb_ts_card(&self, arrow: u64) -> Element<'_, Message> {
        let card = &self.tab.xtb.ts_card;
        // Match Calculate's platform/build note exactly, with no search
        // controls or chemistry diagnostics in an unavailable build.
        if let Some(reason) = crate::app::xtb::unavailable() {
            return container(text(reason).size(12))
                .padding(10)
                .width(Length::Fill)
                .into();
        }
        let scope = (self.tab.file_epoch, arrow);
        let assigned = card.plan(&self.tab.doc, scope);
        let plan = if matches!(assigned, Err(TsPlanError::NotReaction))
            && let Err(shape @ TsPlanError::Shape { .. }) =
                super::review::scheme_plan(&self.tab.doc, arrow)
        {
            Err(shape)
        } else {
            assigned
        };
        let heading = column![
            row![
                text(TITLE).size(13).width(Length::Fill),
                text(if card.open { "−" } else { "+" })
                    .size(16)
                    .style(muted_text),
            ]
            .align_y(Alignment::Center),
            text(subtitle(&self.tab.doc, arrow, &plan))
                .size(11)
                .style(muted_text),
        ]
        .spacing(3);
        let mut content = column![
            reshiki::accessibility::button(
                "xtb-ts-card",
                format!(
                    "{TITLE}: {}",
                    if card.open { "expanded" } else { "collapsed" }
                ),
                heading,
            )
            .expanded(card.open)
            .padding(10)
            .width(Length::Fill)
            .style(iced::widget::button::text)
            .on_press(send(Action::Open(!card.open))),
        ];
        if !card.open {
            return crate::app::inspector::card(content);
        }
        let idle = self.tab.xtb.run.is_none() && !self.tab.busy;
        let set = |action| idle.then(|| send(action));
        let mut body = column![].spacing(6);
        if let Some(result) = self.xtb_ts_card_result(arrow) {
            let imaginary = imaginary_mode(result);
            body = body.push(match imaginary {
                Some((_, frequency)) => row![
                    line(
                        format!(
                            "Imaginary frequency {}i cm⁻¹",
                            crate::app::xtb::state::signed(frequency, 0)
                        ),
                        Tone::Good,
                    ),
                    accessible_command(
                        "xtb-ts-card-animate",
                        "Animate the transition state's imaginary mode",
                        "Animate",
                    )
                    .on_press_maybe(set(Action::Animate)),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .into(),
                None => line("No significant imaginary frequency.", Tone::Warning),
            });
        }
        if let Some((run, live)) = self
            .tab
            .xtb
            .run
            .as_ref()
            .and_then(|run| run.ts.as_ref().map(|live| (run, live)))
            .filter(|(_, live)| live.request.arrow == arrow)
        {
            let stage = if run.launch.is_some() {
                live.line()
            } else {
                run.status()
            };
            let mut progress = column![line(stage, Tone::Muted)].spacing(6);
            if run.launch.is_some() {
                progress = progress.push(line(live.detail(), Tone::Muted));
            }
            body = body.push(
                progress
                    .push(
                        row![
                            Space::new().width(Length::Fill),
                            accessible_command("xtb-ts-cancel", "Cancel TS search", "Cancel")
                                .style(crate::appearance::secondary)
                                .on_press(Message::Xtb(XtbAction::Cancel)),
                        ]
                        .align_y(Alignment::Center),
                    )
                    .push(progress_bar(0.0..=1.0, live.fraction().unwrap_or(0.)).girth(3)),
            );
        }
        match &plan {
            Ok(plan) => {
                let review = summary(&self.tab.doc, plan);
                body = body
                    .push(line(review.atom_map, Tone::Good))
                    .push(line(review.bonds, Tone::Muted));
                for hydrogen in review.hydrogens {
                    body = body.push(line(hydrogen, Tone::Muted));
                }
                let reviewed = card.reviewed(scope, plan);
                body = body.push(accessible_checkbox(
                    "xtb-ts-hydrogens-reviewed",
                    "Hydrogens reviewed",
                    reviewed,
                    set(Action::Reviewed(!reviewed)),
                ));
            }
            Err(error) => {
                body = body.push(line(
                    if matches!(error, TsPlanError::Shape { .. }) {
                        super::review::SHAPE_NOTE.into()
                    } else {
                        error.to_string()
                    },
                    Tone::Warning,
                ));
                if matches!(error, TsPlanError::Unmapped { .. }) {
                    body = body.push(
                        accessible_command("xtb-ts-auto-map", "Auto-map first", "Auto-map first")
                            .on_press_maybe(idle.then_some(Message::Mapping(
                                crate::app::reaction_mapping::Action::Auto,
                            ))),
                    );
                }
            }
        }
        if let Ok(plan) = &plan {
            body = body.push(self.xtb_ts_search_settings(arrow, Some(plan), idle));
            if let Err(reason) = card.config(&self.tab.doc, scope, plan) {
                body = body.push(line(reason, Tone::Warning));
            }
        }
        if self.tab.xtb.run.is_none() && matches!(&plan, Ok(_) | Err(TsPlanError::Unmapped { .. }))
        {
            let ready = plan
                .as_ref()
                .is_ok_and(|plan| card.ready(&self.tab.doc, scope, plan, idle));
            body = body.push(
                accessible_command("xtb-ts-find", FIND, FIND)
                    .style(crate::appearance::primary)
                    .width(Length::Fill)
                    .on_press_maybe(ready.then(|| send(Action::Find))),
            );
        }
        content = content.push(container(body).padding(iced::Padding {
            top: 2.,
            right: 10.,
            bottom: 12.,
            left: 10.,
        }));
        crate::app::inspector::card(content)
    }

    fn xtb_ts_search_settings(
        &self,
        arrow: u64,
        plan: Option<&TsPlan>,
        idle: bool,
    ) -> Element<'_, Message> {
        let card = &self.tab.xtb.ts_card;
        let toggle = accessible_command(
            "xtb-ts-search-settings",
            "Search settings",
            if card.search_settings {
                "Search settings ▾"
            } else {
                "Search settings ▸"
            },
        )
        .expanded(card.search_settings)
        .on_press(send(Action::SearchSettings));
        if !card.search_settings {
            return toggle.into();
        }
        let (charge, unpaired) = card.typed((self.tab.file_epoch, arrow));
        let drawn = plan.map_or((0, 0), |plan| drawn_state(&self.tab.doc, plan));
        let field = |id: &'static str,
                     name: &'static str,
                     value: &str,
                     default: String,
                     on: fn(String) -> Action| {
            column![
                text(name).size(12),
                reshiki::accessibility::text_input(id, name, &default, value)
                    .on_input_maybe(idle.then_some(move |value| send(on(value))))
                    .size(12)
                    .padding(6),
            ]
            .spacing(3)
            .width(Length::Fill)
        };
        column![
            toggle,
            text("Method").size(12),
            crate::app::xtb::view::segment(
                "xtb-ts-method-gfn2".into(),
                "GFN2-xTB",
                true,
                None,
                None,
            ),
            line("Other methods after validation", Tone::Muted),
            field(
                "xtb-ts-images",
                "Images (10–20)",
                &card.images,
                "15".into(),
                Action::Images,
            ),
            row![
                field(
                    "xtb-ts-charge",
                    "Charge",
                    charge,
                    drawn.0.to_string(),
                    Action::Charge,
                ),
                field(
                    "xtb-ts-unpaired",
                    "Unpaired electrons",
                    unpaired,
                    drawn.1.to_string(),
                    Action::Unpaired,
                ),
            ]
            .spacing(8),
        ]
        .spacing(6)
        .into()
    }
}

#[cfg(test)]
pub(in crate::app::xtb::ts) mod tests;
