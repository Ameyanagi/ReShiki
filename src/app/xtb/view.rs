//! The Predict tab's NMR · xTB switch and the Calculate card.
use super::state::{self, LATER_TASKS, METHODS, SOLVENTS, Settings};
use super::tone::{Tone, line};
use super::{Action, CalcTask, Outcome, Page};
use crate::app::inspector::Section;
use crate::app::workspace::{accessible_checkbox, accessible_command, control, hover_hint};
use crate::app::{App, InspectorTab, Message};
use iced::widget::{Space, canvas, column, container, progress_bar, row, text, tooltip};
use iced::{Alignment, Color, Element, Length, Point, Rectangle, Renderer, Theme, mouse};
use reshiki::models3d::Model3d;
use reshiki::qcdata::XtbMethod;
use std::borrow::Cow;

const WIDE: Length = Length::Fill;

fn label<'a>(content: &'a str) -> Element<'a, Message> {
    text(content).size(12).into()
}

/// One segment of a segmented control; `None` leaves it disabled with `hint`.
pub(super) fn segment<'a>(
    id: String,
    name: impl Into<Cow<'a, str>>,
    active: bool,
    message: Option<Message>,
    hint: Option<&'a str>,
) -> Element<'a, Message> {
    let name = name.into();
    let label = text(name.clone()).size(12).center().width(WIDE);
    let button = reshiki::accessibility::button(id, name.into_owned(), label)
        .padding([7, 6])
        .checked(active)
        .style(control(active))
        .width(WIDE)
        .on_press_maybe(message);
    match hint {
        Some(hint) => hover_hint(button, hint, tooltip::Position::Top).into(),
        None => button.into(),
    }
}

impl App {
    /// Predict: the NMR · xTB switch above its page.
    pub(in crate::app) fn predict_inspector(&self) -> Element<'_, Message> {
        let page = self.xtb.page;
        let switch = row([(Page::Nmr, "NMR", "nmr"), (Page::Xtb, "xTB", "xtb")].map(
            |(choice, name, id)| {
                segment(
                    format!("predict-page-{id}"),
                    name,
                    page == choice,
                    Some(Message::Xtb(Action::Page(choice))),
                    None,
                )
            },
        ))
        .spacing(4);
        let mut content = column![switch].spacing(12);
        match page {
            Page::Nmr => content = content.push(self.nmr_inspector()),
            Page::Xtb => {
                content = content.push(match self.xtb_selected_reaction() {
                    Some(arrow) => self.xtb_reaction_card(arrow),
                    None => self.xtb_card(),
                });
                if let Some(palette) = self.xtb_inspector_palette() {
                    content = content.push(palette);
                }
            }
        }
        content.into()
    }

    /// "Show results" when the palette could show something it does not.
    fn xtb_show_results(&self) -> Option<Element<'_, Message>> {
        self.xtb_can_show().then(|| {
            accessible_command("xtb-show-results", "Show the xTB results", "Show results")
                .on_press(Message::Xtb(Action::Palette(super::palette::Action::Show)))
                .into()
        })
    }

    fn xtb_card(&self) -> Element<'_, Message> {
        if let Some(reason) = super::unavailable() {
            // Stored results stay readable in builds that cannot calculate.
            let mut body = column![text(reason).size(12)].spacing(8);
            if let Some(show) = self.xtb_show_results() {
                body = body.push(show);
            }
            return container(body).padding(10).width(WIDE).into();
        }
        // A search whose minimizer is an xTB method runs with it: the card
        // shows and checks that method.
        let settings = &super::conformers::effective(&self.xtb.settings, &self.xtb.conformers);
        let fixed_method = settings.method != self.xtb.settings.method
            || (settings.task == CalcTask::Conformers
                && self.xtb.conformers.minimizer.xtb().is_some());
        let run = self.tab.xtb.run.as_ref();
        let idle = run.is_none();
        let target = self.xtb_target().filter(|atoms| !atoms.is_empty());
        let first = target.as_ref().and_then(|atoms| atoms.first().copied());
        let model = self.xtb_card_model(first);
        let numbers = model.and_then(|model| model_numbers(&self.tab.doc, model));
        let subtitle = match (&target, &numbers) {
            (None, _) => self.xtb_selection_hint().0,
            (Some(_), Some(numbers)) => format!(
                "{} · {} ({} atoms)",
                settings.method.label(),
                state::formula(numbers),
                numbers.len()
            ),
            (Some(atoms), None) => format!(
                "{} · {} atom{} drawn",
                settings.method.label(),
                atoms.len(),
                if atoms.len() == 1 { "" } else { "s" }
            ),
        };
        let set = |action: Action| idle.then_some(Message::Xtb(action));

        // Task and method: what the worker runs now, then what comes later.
        // Three per row, with available tasks first.
        let mut segments: Vec<Element<'_, Message>> = CalcTask::ALL
            .map(|(task, name)| {
                segment(
                    format!("xtb-task-{}", name.to_lowercase()),
                    name,
                    settings.task == task,
                    set(Action::Task(task)),
                    None,
                )
            })
            .into();
        segments.extend(LATER_TASKS.map(|(name, reason)| {
            segment(format!("xtb-task-{name}"), name, false, None, Some(reason))
        }));
        let mut segments = segments.into_iter();
        let tasks = column(
            std::iter::from_fn(|| {
                let three: Vec<_> = segments.by_ref().take(3).collect();
                (!three.is_empty()).then(|| row(three).spacing(4).into())
            })
            .collect::<Vec<Element<'_, Message>>>(),
        )
        .spacing(4);
        let methods = column(METHODS.map(|method| {
            let name = method.label();
            let purpose = match method {
                XtbMethod::Gfn2Xtb => "default · most molecules",
                XtbMethod::GXtb => "closer to DFT energies",
                XtbMethod::GfnFf => "very large molecules, metals",
            };
            let active = settings.method == method;
            let button = reshiki::accessibility::button(
                format!("xtb-method-{}", name.to_lowercase()),
                format!("{name} — {purpose}"),
                column![label(name), line(purpose, Tone::Muted)].spacing(2),
            )
            .padding([7, 8])
            .checked(active)
            .style(control(active))
            .width(WIDE)
            .on_press_maybe(set(Action::Method(method)).filter(|_| !fixed_method));
            if fixed_method {
                hover_hint(
                    button,
                    "The search's minimizer sets the method",
                    tooltip::Position::Top,
                )
                .into()
            } else {
                button.into()
            }
        }))
        .spacing(4);
        let electronic = state::electronic(settings.method);
        let solvent_ok = reshiki_xtb_jobs::method::supports_solvent(settings.method);

        // Charge and spin, prefilled from the drawing, with the live check.
        let (drawn, typed) = self.xtb_card_charge(first);
        let (charge_text, unpaired_text) = if typed {
            (settings.charge.clone(), settings.unpaired.clone())
        } else {
            (String::new(), String::new())
        };
        let field = |id: &'static str,
                     name: &'static str,
                     value: String,
                     drawn: String,
                     on: fn(String) -> Action| {
            let hint = if value.trim().is_empty() {
                "from drawing"
            } else {
                "typed"
            };
            column![
                label(name),
                reshiki::accessibility::text_input(id, name, &drawn, &value)
                    .on_input_maybe(idle.then_some(move |value| Message::Xtb(on(value))))
                    .on_submit_maybe(set(Action::Run))
                    .size(12)
                    .padding(6),
                line(hint, Tone::Muted),
            ]
            .spacing(3)
            .width(WIDE)
        };
        let fields = row![
            field(
                "xtb-charge",
                "Charge",
                charge_text,
                drawn.0.to_string(),
                Action::Charge
            ),
            field(
                "xtb-unpaired",
                "Unpaired electrons",
                unpaired_text,
                drawn.1.to_string(),
                Action::Unpaired
            ),
        ]
        .spacing(8);
        let check_line = match check_electrons(settings, drawn, typed, numbers.as_deref()) {
            Ok(text) if numbers.is_some() => line(text, Tone::Good),
            Ok(text) => line(text, Tone::Muted),
            Err(text) => line(text, Tone::Warning),
        };

        // Solvent: the ALPB list, gas phase first.
        let solvents: Vec<Solvent> = std::iter::once(Solvent(None))
            .chain(SOLVENTS.map(|name| Solvent(Some(name))))
            .collect();
        let solvent = crate::appearance::pick_list(
            solvents,
            Some(Solvent(settings.solvent)),
            |s: Solvent| Message::Xtb(Action::Solvent(s.0)),
        )
        .text_size(12)
        .width(WIDE);
        let solvent: Element<'_, Message> = if idle && solvent_ok {
            solvent.into()
        } else {
            let shown = Solvent(settings.solvent.filter(|_| solvent_ok));
            container(text(shown.to_string()).size(12))
                .padding(6)
                .into()
        };

        // Starting geometry: the current model, or MMFF first.
        let stale = first.is_some_and(|atom| {
            self.tab
                .doc
                .models
                .iter()
                .any(|m| m.linked_atoms.contains(&atom))
        }) && model.is_none();
        let geometry = if model.is_some() {
            line("Current 3D model", Tone::Muted)
        } else if stale {
            line(
                "The 3D model is out of date. Run updates it with MMFF first; the 2D drawing never moves.",
                Tone::Warning,
            )
        } else {
            line(
                "No 3D model yet. Run makes one with MMFF first; the 2D drawing never moves.",
                Tone::Warning,
            )
        };

        // Hold selected atoms fixed (optimization only).
        let selected = target.as_ref().map_or(0, |atoms| {
            atoms
                .iter()
                .filter(|id| self.tab.selected.contains(id))
                .count()
        });
        let optimize = settings.task == CalcTask::Optimize;
        let fix_hint = if !optimize {
            "Optimize only".to_owned()
        } else if selected == 0 || self.tab.selected.is_empty() {
            "Select atoms to hold them fixed".to_owned()
        } else {
            format!(
                "{selected} atom{} selected",
                if selected == 1 { "" } else { "s" }
            )
        };
        let hold = column![
            accessible_checkbox(
                "xtb-hold-fixed",
                "Hold selected atoms fixed",
                settings.hold_fixed && optimize,
                (idle && optimize).then_some(Message::Xtb(Action::HoldFixed(!settings.hold_fixed))),
            ),
            line(fix_hint, Tone::Muted),
        ]
        .spacing(3);

        // Progress and outcomes come first, where they are seen without scrolling.
        let mut body = column![].spacing(6);
        if let Some(run) = run {
            body = body.push(progress(run));
        } else if let Some(outcome) = self.xtb_outcome() {
            body = body.push(outcome_view(self, outcome));
        }
        if let Some(show) = self.xtb_show_results() {
            body = body.push(show);
        }
        body = body.push(
            column![
                label("Task"),
                tasks,
                line(
                    "Every run shows its orbitals in the results' Orbitals tab.",
                    Tone::Muted
                ),
                label("Method"),
                methods,
            ]
            .spacing(6),
        );
        if let Some(note) =
            state::method_note(settings.method, settings.solvent.is_some() && solvent_ok)
        {
            body = body.push(line(note, Tone::Muted));
        }
        body = body.push(
            column![
                fields,
                check_line,
                label("Solvent (ALPB)"),
                solvent,
                label("Starting geometry"),
                geometry,
            ]
            .spacing(6),
        );
        if optimize {
            body = body.push(hold);
        }
        let search = settings.task == CalcTask::Conformers;
        if search {
            body =
                body.push(self.xtb_conformer_fields(idle, numbers.as_deref(), target.as_deref()));
        }
        body = body.push(self.xtb_advanced(idle, electronic));
        self.inspector_section(Section::Xtb, "xTB calculation", subtitle, true, body)
    }

    /// Keep Run in reach while the calculation settings scroll.
    pub(in crate::app) fn xtb_run_footer(&self) -> Option<Element<'_, Message>> {
        if !super::available()
            || self.inspector_tab != InspectorTab::Predict
            || self.xtb.page != Page::Xtb
            || self.xtb_selected_reaction().is_some()
            || self.xtb_running()
            || self.tube_bend_active()
        {
            return None;
        }
        let settings = &super::conformers::effective(&self.xtb.settings, &self.xtb.conformers);
        let target = self.xtb_target().unwrap_or_default();
        let first = target.first().copied();
        let numbers = self
            .xtb_card_model(first)
            .and_then(|model| model_numbers(&self.tab.doc, model));
        let (drawn, typed) = self.xtb_card_charge(first);
        let count = numbers.as_ref().map_or(target.len(), Vec::len);
        let limit = settings.task.limit();
        let search_ok = settings.task != CalcTask::Conformers
            || self
                .xtb
                .conformers
                .config(numbers.as_ref().map(Vec::len))
                .is_ok();
        let ready = first.is_some()
            && count <= limit
            && search_ok
            && check_electrons(settings, drawn, typed, numbers.as_deref()).is_ok();
        let keys = super::rerun_keys();
        let mut footer = column![
            row![
                hover_hint(
                    reshiki::accessibility::button(
                        "xtb-run",
                        "Run xTB calculation",
                        text("Run").size(12)
                    )
                    .padding([7, 18])
                    .style(crate::appearance::primary)
                    .on_press_maybe(ready.then_some(Message::Xtb(Action::Run))),
                    format!("Run ({keys})"),
                    tooltip::Position::Top,
                ),
                line(keys, Tone::Muted),
                first
                    .is_none()
                    .then(|| line(self.xtb_selection_hint().1, Tone::Muted)),
            ]
            .spacing(10)
            .align_y(Alignment::Center)
        ]
        .spacing(6);
        if count > limit {
            footer = footer.push(line(
                if settings.task.frequencies() {
                    format!("Frequencies are limited to {limit} atoms; this molecule has {count}.")
                } else {
                    format!("Too many atoms: {count}. The limit is {limit} atoms.")
                },
                Tone::Error,
            ));
        }
        Some(footer.into())
    }

    fn xtb_card_model(&self, first: Option<u64>) -> Option<&Model3d> {
        first
            .and_then(|atom| self.xtb_model(atom))
            .and_then(|index| self.tab.doc.models.get(index))
    }

    fn xtb_card_charge(&self, first: Option<u64>) -> ((i32, u32), bool) {
        let drawn = first.map_or((0, 0), |atom| {
            let (charge, multiplicity) = reshiki::science::drawn_state(&self.tab.doc, atom);
            (charge, multiplicity.saturating_sub(1))
        });
        let typed = first.is_some() && self.xtb.settings.molecule == self.xtb_key();
        (drawn, typed)
    }

    fn xtb_advanced(&self, idle: bool, electronic: bool) -> Element<'_, Message> {
        let settings = &self.xtb.settings;
        let toggle = accessible_command(
            "xtb-advanced",
            "Advanced settings",
            if settings.advanced {
                "Advanced ▾"
            } else {
                "Advanced ▸"
            },
        )
        .on_press(Message::Xtb(Action::Advanced));
        if !settings.advanced {
            return toggle.into();
        }
        let set = |action: Action| idle.then_some(Message::Xtb(action));
        let convergence = row([(false, "Normal"), (true, "Tight")].map(|(tight, name)| {
            segment(
                format!("xtb-convergence-{}", name.to_lowercase()),
                name,
                settings.tight == tight,
                set(Action::Tight(tight)),
                None,
            )
        }))
        .spacing(4);
        let auto = Settings::default().threads();
        let threads = row([
            (0, format!("Auto ({auto})")),
            (1, "1".into()),
            (2, "2".into()),
            (4, "4".into()),
        ]
        .map(|(count, name)| {
            let active = settings.threads == count;
            accessible_command(
                format!("xtb-threads-{count}"),
                format!("{name} threads"),
                name,
            )
            .checked(active)
            .style(control(active))
            .width(WIDE)
            .on_press_maybe(set(Action::Threads(count)))
            .into()
        }))
        .spacing(4);
        // SCF fields: empty is the method's default, shown greyed.
        let scf_idle = idle && electronic;
        let default_kelvin = if settings.method == reshiki::qcdata::XtbMethod::GXtb {
            "0"
        } else {
            "300"
        };
        let field = |id: &'static str,
                     name: &'static str,
                     default: &'static str,
                     value: &str,
                     on: fn(String) -> Action| {
            column![
                label(name),
                reshiki::accessibility::text_input(id, name, default, value)
                    .on_input_maybe(scf_idle.then_some(move |value| Message::Xtb(on(value))))
                    .on_submit_maybe(scf_idle.then_some(Message::Xtb(Action::Run)))
                    .size(12)
                    .padding(6),
            ]
            .spacing(3)
            .width(WIDE)
        };
        let scf = row![
            field(
                "xtb-accuracy",
                "Accuracy",
                "1.0",
                &settings.accuracy,
                Action::Accuracy
            ),
            field(
                "xtb-temperature",
                "Electronic temp. (K)",
                default_kelvin,
                &settings.temperature,
                Action::Temperature
            ),
            field(
                "xtb-iterations",
                "Max SCF iterations",
                "250",
                &settings.iterations,
                Action::Iterations
            ),
        ]
        .spacing(8);
        let mut advanced = column![toggle, scf].spacing(6);
        if !electronic {
            advanced = advanced.push(line("No SCF settings with GFN-FF.", Tone::Muted));
        } else if let Err(reason) = settings.scf() {
            advanced = advanced.push(line(reason, Tone::Warning));
        }
        advanced
            .push(accessible_checkbox(
                "xtb-conventions",
                "xtb 6.8 compatible",
                settings.xtb68,
                idle.then_some(Message::Xtb(Action::Xtb68(!settings.xtb68))),
            ))
            .push(line(
                "Numerical conventions of xtb 6.8 instead of axtb's; recorded with the result.",
                Tone::Muted,
            ))
            .push(label("Threads"))
            .push(threads)
            .push(label("Optimizer convergence"))
            .push(convergence)
            .into()
    }
}

fn check_electrons(
    settings: &Settings,
    drawn: (i32, u32),
    typed: bool,
    numbers: Option<&[u8]>,
) -> Result<String, String> {
    let config = settings.config(drawn, typed, vec![])?;
    let Some(numbers) = numbers else {
        return Ok("Electrons are checked once the 3D model is made".into());
    };
    let protons = numbers.iter().map(|&z| u32::from(z)).sum();
    state::electrons(protons, config.charge, config.unpaired)
}

/// A solvent choice for the pick list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Solvent(Option<&'static str>);

impl std::fmt::Display for Solvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0.unwrap_or("None (gas phase)"))
    }
}

/// The atomic numbers of a model's atoms then its added hydrogens, as a
/// calculation sends them.
fn model_numbers(
    doc: &reshiki::document::Document,
    model: &reshiki::models3d::Model3d,
) -> Option<Vec<u8>> {
    let heavy = (0..model.atoms.len()).map(|index| {
        let symbol = match &model.generated {
            Some(generated) => generated.elements.get(index).map(String::as_str),
            None => doc
                .atom(model.atoms.get(index)?.id)
                .map(|a| a.element.as_str()),
        }?;
        let z = reshiki::editing::ELEMENTS
            .iter()
            .position(|e| *e == symbol)?;
        u8::try_from(z + 1).ok()
    });
    heavy
        .chain(model.hydrogens.iter().map(|_| Some(1)))
        .collect()
}

/// The progress row: text, |ΔE| curve, Cancel and a thin bar.
fn progress(run: &super::Run) -> Element<'_, Message> {
    let cancel =
        reshiki::accessibility::button("xtb-cancel", "Cancel calculation", text("Cancel").size(12))
            .padding([6, 12])
            .style(crate::appearance::secondary)
            .on_press(Message::Xtb(Action::Cancel));
    let text_line = line(
        run.launch.as_ref().map_or_else(
            || "Making the 3D model first (MMFF94s)".into(),
            |_| match &run.search {
                Some(live) => live.line(run.seconds()),
                None => run.progress.line(run.config.task, run.seconds()),
            },
        ),
        Tone::Muted,
    );
    let mut rows = column![text_line].spacing(6);
    let optimizing = run.config.task.optimizes() && run.launch.is_some() && !run.progress.hessian;
    let mut controls = row![].spacing(8).align_y(Alignment::Center);
    if optimizing {
        controls = controls.push(
            canvas(Spark {
                deltas: run.progress.deltas().map(f64::abs).collect(),
                threshold: if run.config.tight { 1e-6 } else { 5e-6 },
            })
            .width(170)
            .height(46),
        );
    }
    controls = controls.push(Space::new().width(Length::Fill)).push(cancel);
    rows = rows.push(controls);
    if let Some(live) = run.search.as_ref().filter(|_| run.launch.is_some()) {
        // Embedding has no count yet: the bar waits at the start.
        rows = rows.push(progress_bar(0.0..=1.0, live.fraction().unwrap_or(0.)).girth(3));
    }
    if optimizing {
        rows = rows.push(
            progress_bar(
                0.0..=reshiki_optimize_steps() as f32,
                run.progress.step() as f32,
            )
            .girth(3),
        );
    }
    rows.into()
}

/// The optimizer's step limit, which the bar counts against.
fn reshiki_optimize_steps() -> usize {
    #[cfg(not(target_arch = "wasm32"))]
    return reshiki_optimize::Options::default().max_steps;
    #[cfg(target_arch = "wasm32")]
    500
}

/// The card's last outcome. A finished result defers to the results
/// palette while it shows; its notes stay here.
pub(super) fn outcome_view<'a>(app: &App, outcome: &'a Outcome) -> Element<'a, Message> {
    match outcome {
        Outcome::Done {
            line: summary,
            notes,
            ..
        } => {
            let mut lines = column![].spacing(4);
            if !app.xtb_palette_visible() {
                lines = lines.push(line(summary.clone(), Tone::Good));
            }
            for note in notes {
                lines = lines.push(line(note.clone(), Tone::Warning));
            }
            lines.into()
        }
        Outcome::Failed(failed) => line(failed.explain(app.xtb_can_retry(failed)).0, Tone::Error),
        Outcome::Review(review) => {
            let mut lines = column![].spacing(4);
            for reason in &review.reasons {
                lines = lines.push(text(reason.clone()).size(12));
            }
            lines = lines.push(line(
                "Nothing was applied. The 3D model still shows the structure from before the optimization.",
                Tone::Muted,
            ));
            lines = lines.push(line(review.pending.line.clone(), Tone::Muted));
            lines = lines.push(
                row![
                    reshiki::accessibility::button(
                        "xtb-apply-anyway",
                        "Apply anyway",
                        text("Apply anyway").size(12)
                    )
                    .padding([6, 12])
                    .style(crate::appearance::primary)
                    .on_press(Message::Xtb(Action::ApplyAnyway)),
                    accessible_command("xtb-discard", "Discard optimized geometry", "Discard")
                        .on_press(Message::Xtb(Action::Discard)),
                ]
                .spacing(8),
            );
            container(lines)
                .padding(10)
                .width(WIDE)
                .style(|theme: &Theme| {
                    let warning = theme.palette().warning;
                    container::Style {
                        background: Some(Color { a: 0.12, ..warning }.into()),
                        border: iced::Border {
                            color: warning,
                            width: 1.,
                            radius: 6.0.into(),
                        },
                        ..Default::default()
                    }
                })
                .into()
        }
    }
}

/// |ΔE| per step on a log scale, with the convergence threshold dashed.
struct Spark {
    deltas: Vec<f64>,
    threshold: f64,
}

impl Spark {
    const TOP: f64 = -1.;
    const BOTTOM: f64 = -8.;
}

impl canvas::Program<Message> for Spark {
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
        let (left, width, height) = (24f32, bounds.width - 26., bounds.height - 10.);
        let y = |log: f64| {
            let t = ((Self::TOP - log.clamp(Self::BOTTOM, Self::TOP)) / (Self::TOP - Self::BOTTOM))
                as f32;
            2. + t * height
        };
        let muted = crate::appearance::muted(theme);
        let accent = theme.palette().primary;
        frame.stroke(
            &canvas::Path::rectangle(Point::new(left, 2.), iced::Size::new(width, height)),
            canvas::Stroke::default()
                .with_color(Color { a: 0.4, ..muted })
                .with_width(1.),
        );
        let threshold = y(self.threshold.log10());
        frame.stroke(
            &canvas::Path::line(
                Point::new(left, threshold),
                Point::new(left + width, threshold),
            ),
            canvas::Stroke {
                line_dash: canvas::LineDash {
                    segments: &[3., 3.],
                    offset: 0,
                },
                ..canvas::Stroke::default().with_color(accent).with_width(1.)
            },
        );
        // Superscript minus: desktop only, like the whole card.
        for (log, label) in [(Self::TOP, "10\u{207B}¹"), (Self::BOTTOM, "10\u{207B}⁸")] {
            frame.fill_text(canvas::Text {
                content: label.into(),
                position: Point::new(left - 3., y(log) - 5.),
                color: muted,
                size: 8.5.into(),
                align_x: iced::alignment::Horizontal::Right.into(),
                ..Default::default()
            });
        }
        if self.deltas.len() > 1 {
            let step = width / (self.deltas.len() - 1).max(1) as f32;
            let path = canvas::Path::new(|path| {
                for (index, delta) in self.deltas.iter().enumerate() {
                    let point = Point::new(left + index as f32 * step, y(delta.max(1e-12).log10()));
                    if index == 0 {
                        path.move_to(point);
                    } else {
                        path.line_to(point);
                    }
                }
            });
            frame.stroke(
                &path,
                canvas::Stroke::default()
                    .with_color(theme.palette().text)
                    .with_width(1.3),
            );
        }
        frame.fill_text(canvas::Text {
            content: "|ΔE| per step, log".into(),
            position: Point::new(left + width, height + 1.),
            color: muted,
            size: 8.5.into(),
            align_x: iced::alignment::Horizontal::Right.into(),
            ..Default::default()
        });
        vec![frame.into_geometry()]
    }
}
