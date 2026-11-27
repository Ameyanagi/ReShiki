//! A single setup sheet and the multistep reaction's progress and Path result.
use super::run::{StepOptions, StepStatus, StepValue};
use super::{Action, Quantity, State, send};
use crate::app::energy_plot::profile::{Marker, Profile, Sample, Side, Style};
use crate::app::help::{Placement, modal};
use crate::app::models3d::thumbnail::Thumbnail;
use crate::app::workspace::{accessible_command, named_checkbox};
use crate::app::xtb::tone::{Tone, line};
use crate::app::xtb::{self, EnergyUnit};
use crate::app::{App, Message};
use iced::widget::{Space, canvas, column, container, progress_bar, row, scrollable, text};
use iced::{Alignment, Element, Length, Rectangle, Renderer, Theme, mouse};
use reshiki::graphics::{EnergyProfile, EnergyProfileStateKind};
use reshiki::qcdata::XtbMethod;

fn quantities(options: &StepOptions) -> [(Quantity, &'static str, &'static str, bool); 4] {
    [
        (
            Quantity::Energy,
            "energy",
            "Reaction energy ΔE",
            options.reaction_energy,
        ),
        (Quantity::Barrier, "barrier", "Barrier ΔE‡", options.barrier),
        (
            Quantity::FreeEnergy,
            "free-energy",
            "Free energy ΔG",
            options.free_energy,
        ),
        (
            Quantity::FreeBarrier,
            "free-barrier",
            "Free barrier ΔG‡",
            options.free_barrier,
        ),
    ]
}

fn any_quantity(options: &[StepOptions]) -> bool {
    options.iter().any(|options| options.any())
}

fn tick(
    index: usize,
    id: &str,
    label: &'static str,
    checked: bool,
    message: Option<Message>,
) -> Element<'static, Message> {
    named_checkbox(
        format!("xtb-scheme-{index}-{id}"),
        format!("Step {} · {label}", index + 1),
        label,
        checked,
        message,
    )
    .into()
}

fn completed(state: &State) -> usize {
    state
        .work
        .steps
        .iter()
        .filter(|status| matches!(status, StepStatus::Done))
        .count()
}

fn result_method(state: &State) -> XtbMethod {
    state
        .work
        .report
        .steps
        .iter()
        .flatten()
        .next()
        .map_or(state.method, |value| value.method)
}

fn global_error(state: &State) -> Option<&str> {
    state.work.error.as_deref().filter(|error| {
        !state
            .work
            .report
            .steps
            .iter()
            .flatten()
            .any(|value| value.error.as_deref() == Some(*error))
    })
}

fn status_line(status: &StepStatus) -> (&str, Tone) {
    match status {
        StepStatus::Pending => ("Waiting", Tone::Muted),
        StepStatus::Running => ("Calculating…", Tone::Warning),
        StepStatus::Done => ("Done", Tone::Good),
        StepStatus::Failed(reason) => (reason, Tone::Error),
        StepStatus::Cancelled => ("Canceled · finished results kept", Tone::Muted),
        StepStatus::Skipped => ("Not selected", Tone::Muted),
    }
}

fn energy_rows(value: &StepValue, unit: EnergyUnit) -> Vec<(String, String)> {
    [
        ("ΔE", value.energy),
        ("ΔE‡", value.barrier),
        ("ΔG (298 K)", value.gibbs),
        ("ΔG‡ (298 K)", value.free_barrier),
    ]
    .into_iter()
    .filter_map(|(name, value)| {
        value
            .filter(|value| value.is_finite())
            .map(|value| (name.into(), signed_energy(unit, value)))
    })
    .collect()
}

fn signed_energy(unit: EnergyUnit, value: f64) -> String {
    let formatted = unit.format(value);
    if value > 0. && unit.number(value) != unit.number(0.) {
        format!("+{formatted}")
    } else {
        formatted
    }
}

/// Use the same complete starting and final inventories as the plotted levels.
fn overall(state: &State, free: bool) -> Option<f64> {
    super::profile::overall(&state.plan, &state.work.report, free)
}

impl App {
    pub(in crate::app) fn with_scheme_sheet<'a>(
        &'a self,
        base: Element<'a, Message>,
    ) -> Element<'a, Message> {
        let Some(state) = self.tab.xtb.scheme.as_ref().filter(|state| state.sheet) else {
            return base;
        };
        let current = self.xtb_scheme_current();
        let idle = !self.xtb_running() && !self.tab.busy;
        let mut content = column![
            text(format!("Calculate scheme · {} steps", state.plan.steps.len())).size(21),
            line(format!("{} unique species · shared species are calculated once", state.plan.species.len()), Tone::Muted),
            text("Method · gas phase").size(12),
            row([(XtbMethod::Gfn2Xtb, "gfn2"), (XtbMethod::GXtb, "gxtb")].map(|(method, id)| {
                xtb::view::segment(
                    format!("xtb-scheme-method-{id}"), method.label(), state.method == method,
                    idle.then(|| send(Action::Method(method))), None,
                )
            })).spacing(4),
            line("Free energies use frequencies at 298.15 K. For a barrier, review the atom map when prompted.", Tone::Muted),
        ].spacing(10);
        for (index, (options, reaction)) in state.options.iter().zip(&state.plan.steps).enumerate()
        {
            let [energy, barrier, free_energy, free_barrier] =
                quantities(options).map(|(quantity, id, label, checked)| {
                    tick(
                        index,
                        id,
                        label,
                        checked,
                        idle.then(|| send(Action::Toggle(index, quantity))),
                    )
                });
            let mut step = column![
                text(format!(
                    "Step {} · {}",
                    index + 1,
                    state.plan.step_label(index)
                ))
                .size(13),
                row![energy, barrier].spacing(18).wrap().vertical_spacing(5),
                row![free_energy, free_barrier]
                    .spacing(18)
                    .wrap()
                    .vertical_spacing(5),
            ]
            .spacing(7);
            for note in &reaction.notes {
                step = step.push(line(note, Tone::Warning));
            }
            content = content.push(
                container(step)
                    .padding(10)
                    .width(Length::Fill)
                    .style(crate::app::workspace::panel),
            );
        }
        if !current {
            content = content.push(line(
                "The scheme changed. Close this sheet and select it again.",
                Tone::Error,
            ));
        }
        if !any_quantity(&state.options) {
            content = content.push(line(
                "Choose at least one quantity to calculate.",
                Tone::Muted,
            ));
        }
        if let Some(reason) = xtb::unavailable() {
            content = content.push(line(reason, Tone::Muted));
        }
        content = content.push(
            row![
                accessible_command("xtb-scheme-cancel-setup", "Cancel scheme setup", "Cancel")
                    .on_press(send(Action::Close)),
                Space::new().width(Length::Fill),
                accessible_command(
                    "xtb-scheme-calculate",
                    "Calculate the reaction scheme",
                    "Run"
                )
                .style(crate::appearance::primary)
                .on_press_maybe(
                    (idle && current && xtb::available() && any_quantity(&state.options))
                        .then(|| send(Action::Run))
                ),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        );
        modal(
            base,
            scrollable(content).into(),
            720.,
            700.,
            Placement::Fit,
            send(Action::Close),
        )
    }

    /// The selected scheme gets one card; its setup is always a separate sheet.
    pub(in crate::app::xtb) fn xtb_scheme_card(&self) -> Element<'_, Message> {
        let state = self
            .tab
            .xtb
            .scheme
            .as_ref()
            .filter(|_| self.xtb_scheme_selected());
        let mut content = column![
            text(state.map_or_else(|| "Reaction scheme".to_owned(), |s| s.plan.title())).size(14)
        ]
        .spacing(8);
        if let Some(state) = state {
            content = content.push(line(
                format!(
                    "{} steps{} · {} unique species · {}",
                    state.plan.steps.len(),
                    if state.plan.cyclic { " in a cycle" } else { "" },
                    state.plan.species.len(),
                    result_method(state).label()
                ),
                Tone::Muted,
            ));
            if state.busy() {
                content = content.push(self.xtb_scheme_progress(state, true));
            } else {
                for (name, value) in [
                    ("Overall ΔE", overall(state, false)),
                    ("Overall ΔG", overall(state, true)),
                ] {
                    if let Some(value) = value {
                        content = content.push(
                            text(format!(
                                "{name} {}",
                                signed_energy(self.appearance.energy_unit, value)
                            ))
                            .size(13),
                        );
                    }
                }
                let failed = state
                    .work
                    .steps
                    .iter()
                    .filter(|status| matches!(status, StepStatus::Failed(_)))
                    .count();
                if failed != 0 {
                    content = content.push(line(
                        format!(
                            "{failed} {} attention · finished results kept",
                            if failed == 1 {
                                "step needs"
                            } else {
                                "steps need"
                            }
                        ),
                        Tone::Warning,
                    ));
                }
                if let Some(error) = global_error(state) {
                    content = content.push(line(error, Tone::Error));
                }
            }
        } else {
            content = content.push(line(
                "Calculate the selected steps together. Shared species are calculated once.",
                Tone::Muted,
            ));
        }
        content = content.push(
            accessible_command(
                "xtb-scheme-setup",
                "Choose quantities for the reaction scheme",
                "Calculate scheme…",
            )
            .on_press_maybe(self.xtb_scheme_open_action()),
        );
        if let Some(state) = state {
            if !self.tab.xtb.palette.open && !state.work.steps.is_empty() {
                content = content.push(
                    accessible_command(
                        "xtb-scheme-show-results",
                        "Show the reaction scheme results",
                        "Show results",
                    )
                    .on_press(Message::Xtb(xtb::Action::Palette(
                        xtb::palette::Action::Show,
                    ))),
                );
            }
            if state.busy() {
                content = content.push(
                    accessible_command(
                        "xtb-scheme-cancel",
                        "Cancel the scheme and keep finished results",
                        "Cancel (keep finished)",
                    )
                    .on_press(send(Action::Cancel)),
                );
            } else if self.xtb_scheme_current() && self.xtb_scheme_profile_available() {
                content = content.push(self.xtb_scheme_place_button("xtb-scheme-place-card"));
            }
        }
        crate::app::inspector::card(container(content).padding(10).width(Length::Fill))
    }

    pub(in crate::app::xtb) fn xtb_scheme_palette_visible(&self) -> bool {
        self.tab.xtb.palette.open && self.xtb_scheme_selected() && self.tab.xtb.scheme.is_some()
    }

    fn xtb_scheme_progress<'a>(&'a self, state: &'a State, details: bool) -> Element<'a, Message> {
        let count = completed(state);
        let total = state.plan.steps.len();
        let mut content = column![
            line(
                format!("Calculating scheme · {count} of {total} steps done"),
                Tone::Muted
            ),
            progress_bar(0.0..=total.max(1) as f32, count as f32).girth(5),
        ]
        .spacing(6);
        if let Some(reviews) = &state.reviews {
            content = content.push(line(
                reviews.active.map_or_else(
                    || "Preparing atom-map reviews…".into(),
                    |index| format!("Reviewing the atom map · step {} of {total}", index + 1),
                ),
                Tone::Warning,
            ));
        }
        if let Some(progress) = &state.work.progress {
            content = content.push(line(
                format!(
                    "{} of {} species done · {}",
                    progress.completed_species, progress.total_species, progress.label
                ),
                Tone::Muted,
            ));
        }
        if let Some(error) = global_error(state) {
            content = content.push(line(error, Tone::Error));
        }
        if details {
            if let Some(progress) = &state.work.progress {
                for (index, species) in state.plan.species.iter().enumerate() {
                    let used =
                        state
                            .plan
                            .steps
                            .iter()
                            .zip(&state.options)
                            .any(|(step, options)| {
                                options.any()
                                    && step
                                        .reactants
                                        .iter()
                                        .chain(&step.products)
                                        .any(|term| term.species == index)
                            });
                    if !used {
                        continue;
                    }
                    let status = if progress.species == Some(index) {
                        "Calculating…"
                    } else if state.work.report.species(index).is_some() {
                        "Done"
                    } else {
                        "—"
                    };
                    content = content.push(
                        row![
                            text(&species.label).size(12).width(Length::Fill),
                            line(status, Tone::Muted)
                        ]
                        .spacing(8),
                    );
                }
            }
            for index in 0..total {
                let (status, tone) =
                    state
                        .work
                        .steps
                        .get(index)
                        .map(status_line)
                        .unwrap_or_else(|| {
                            if state
                                .reviews
                                .as_ref()
                                .is_some_and(|reviews| reviews.active == Some(index))
                            {
                                ("Reviewing atom map…", Tone::Warning)
                            } else {
                                ("Waiting", Tone::Muted)
                            }
                        });
                content = content.push(
                    column![
                        text(format!(
                            "Step {} · {}",
                            index + 1,
                            state.plan.step_label(index)
                        ))
                        .size(12),
                        line(status, tone),
                    ]
                    .spacing(2),
                );
            }
        }
        content.into()
    }

    fn xtb_scheme_place_button(&self, id: &'static str) -> Element<'_, Message> {
        accessible_command(
            id,
            "Place the scheme energy profile in the drawing",
            "Place energy profile",
        )
        .on_press(send(Action::Place))
        .into()
    }

    pub(in crate::app::xtb) fn xtb_scheme_path(&self, inspector: bool) -> Element<'_, Message> {
        let Some(state) = &self.tab.xtb.scheme else {
            return Space::new().into();
        };
        let unit = self.appearance.energy_unit;
        let current = self.xtb_scheme_current();
        let profile = self.xtb_scheme_profile();
        let has_profile = profile.is_some();
        let mut content = column![line(
            format!(
                "{} · gas phase · {} unique species",
                result_method(state).label(),
                state.plan.species.len()
            ),
            Tone::Muted
        )]
        .spacing(8);
        if !current {
            content = content.push(line(
                "The scheme changed. These energies refer to the earlier drawing.",
                Tone::Warning,
            ));
        }
        if state.busy() {
            content = content.push(self.xtb_scheme_progress(state, false));
        }
        if let Some(profile) = profile {
            let chart = SchemeChart::new(profile, &self.tab.doc, !current);
            let accessible = chart.accessible_name();
            content = content.push(reshiki::accessibility::image(
                "xtb-scheme-profile",
                accessible,
                canvas(chart)
                    .width(Length::Fill)
                    .height(if inspector { 290. } else { 260. }),
            ));
        }
        for index in 0..state.plan.steps.len() {
            let mut step = column![
                text(format!(
                    "Step {} · {}",
                    index + 1,
                    state.plan.step_label(index)
                ))
                .size(12)
            ]
            .spacing(3);
            if let Some(value) = state.work.step(index) {
                for (name, value) in energy_rows(value, unit) {
                    step = step.push(
                        row![
                            text(name).size(12).width(Length::Fill),
                            text(value).size(12)
                        ]
                        .spacing(8),
                    );
                }
                if let Some(error) = &value.error {
                    step = step.push(line(error, Tone::Error));
                }
            }
            if let Some(status) = state.work.steps.get(index) {
                let (label, tone) = status_line(status);
                if !matches!(status, StepStatus::Done)
                    && !state
                        .work
                        .step(index)
                        .is_some_and(|value| value.error.as_deref() == Some(label))
                {
                    step = step.push(line(label, tone));
                }
                if matches!(status, StepStatus::Failed(_) | StepStatus::Cancelled) {
                    step = step.push(
                        accessible_command(
                            format!("xtb-scheme-retry-{index}"),
                            format!("Retry step {}", index + 1),
                            "Retry",
                        )
                        .on_press_maybe(
                            (!self.xtb_running() && current && xtb::available())
                                .then(|| send(Action::Retry(index))),
                        ),
                    );
                }
            }
            content = content.push(
                container(step)
                    .padding(8)
                    .width(Length::Fill)
                    .style(crate::app::workspace::panel),
            );
        }
        for (name, value) in [
            ("Overall ΔE", overall(state, false)),
            ("Overall ΔG (298 K)", overall(state, true)),
        ] {
            if let Some(value) = value {
                content = content.push(
                    row![
                        text(name).size(13).width(Length::Fill),
                        text(signed_energy(unit, value)).size(13)
                    ]
                    .spacing(8),
                );
            }
        }
        for note in &state.plan.notes {
            content = content.push(line(note, Tone::Muted));
        }
        if !state.busy()
            && let Some(error) = global_error(state)
        {
            content = content.push(line(error, Tone::Error));
        }
        if state.busy() {
            content = content.push(
                accessible_command(
                    "xtb-scheme-path-cancel",
                    "Cancel the scheme and keep finished results",
                    "Cancel (keep finished)",
                )
                .on_press(send(Action::Cancel)),
            );
        } else if has_profile && current {
            content = content.push(self.xtb_scheme_place_button("xtb-scheme-place"));
        }
        content.into()
    }
}

/// The same labelled axes, level layout and molecular renderer as the TS Path.
struct SchemeChart {
    profile: Profile,
    energy_kind: &'static str,
    thumbnails: Vec<Thumbnail>,
    untraced_steps: Vec<Vec<Sample>>,
    traced: bool,
}

impl SchemeChart {
    fn new(energy: EnergyProfile, doc: &reshiki::document::Document, stale: bool) -> Self {
        let coordinates = profile_coordinates(&energy);
        let mut thumbnails = vec![];
        let mut stationary: Vec<_> = energy
            .states
            .iter()
            .zip(&coordinates)
            .map(|(state, &x)| Sample {
                x,
                energy: state.energy_kcal,
            })
            .collect();
        stationary.sort_by(|a, b| a.x.total_cmp(&b.x));
        let (traces, untraced_steps) = step_traces(&energy, &coordinates);
        let traced = !traces.is_empty();
        let samples = if traced {
            let mut samples = vec![];
            for trace in traces {
                if !samples.is_empty() {
                    samples.push(Sample {
                        x: f64::NAN,
                        energy: f64::NAN,
                    });
                }
                samples.extend(trace);
            }
            samples
        } else {
            stationary
        };
        let markers = energy
            .states
            .iter()
            .zip(coordinates)
            .map(|(state, x)| {
                let transition = state.kind == EnergyProfileStateKind::TransitionState;
                let thumbnail = state.geometry.as_ref().and_then(|geometry| {
                    let image = Thumbnail::new(
                        &geometry.frame,
                        &geometry.bonds,
                        &geometry.forming,
                        &geometry.breaking,
                        energy.rotation,
                    )?
                    .in_document(doc);
                    let index = thumbnails.len();
                    thumbnails.push(image);
                    Some(index)
                });
                Marker {
                    sample: Sample {
                        x,
                        energy: state.energy_kcal,
                    },
                    label: state.label.clone(),
                    side: if transition { Side::Above } else { Side::Below },
                    transition,
                    note: None,
                    connector_from: None,
                    tooltip: None,
                    thumbnail,
                }
            })
            .collect();
        Self {
            profile: Profile {
                samples,
                overlay: vec![],
                markers,
                selected: None,
                style: if traced { Style::Line } else { Style::Levels },
                unit: energy.unit,
                x_title: "Reaction coordinate".into(),
                x_ticks: false,
                highlight_curve: false,
                stale,
            },
            energy_kind: energy.energy_kind.label(),
            thumbnails,
            untraced_steps,
            traced,
        }
    }

    fn accessible_name(&self) -> String {
        format!(
            "Scheme energy profile · {} ({}) · {}",
            self.energy_kind,
            self.profile.unit.label(),
            self.profile
                .markers
                .iter()
                .map(|marker| format!(
                    "{} {}",
                    marker.label,
                    self.profile.unit.number(marker.sample.energy)
                ))
                .collect::<Vec<_>>()
                .join(" · ")
        )
    }
}

/// Only saved IRC samples make a curve; an untraced step has a dashed guide
/// between its calculated stationary states, with no invented barrier.
fn step_traces(
    profile: &EnergyProfile,
    coordinates: &[f64],
) -> (Vec<Vec<Sample>>, Vec<Vec<Sample>>) {
    let mut traces = vec![];
    let mut untraced = vec![];
    for (index, step) in profile.steps.iter().enumerate() {
        let trace: Vec<_> = step
            .irc
            .iter()
            .filter(|sample| sample.x.is_finite() && sample.energy_kcal.is_finite())
            .map(|sample| Sample {
                x: index as f64 + sample.x,
                energy: sample.energy_kcal,
            })
            .collect();
        if trace.len() >= 2 {
            traces.push(trace);
        } else {
            untraced.push(
                std::iter::once(step.from)
                    .chain(step.transition)
                    .chain(std::iter::once(step.to))
                    .filter_map(|index| {
                        Some(Sample {
                            x: *coordinates.get(index)?,
                            energy: profile.states.get(index)?.energy_kcal,
                        })
                    })
                    .collect(),
            );
        }
    }
    (traces, untraced)
}

fn profile_coordinates(profile: &EnergyProfile) -> Vec<f64> {
    let mut coordinates = vec![0.; profile.states.len()];
    for (index, step) in profile.steps.iter().enumerate() {
        if let Some(x) = coordinates.get_mut(step.from) {
            *x = index as f64;
        }
        if let Some(x) = coordinates.get_mut(step.to) {
            *x = index as f64 + 1.;
        }
        if let Some(x) = step
            .transition
            .and_then(|transition| coordinates.get_mut(transition))
        {
            *x = index as f64 + 0.5;
        }
    }
    coordinates
}

impl canvas::Program<Message> for SchemeChart {
    type State = ();
    fn draw(
        &self,
        _: &(),
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let layout = self.profile.layout(bounds);
        self.profile
            .draw_energy(&mut frame, theme, &layout, self.energy_kind);
        let ink = if self.profile.stale {
            crate::appearance::muted(theme)
        } else {
            theme.palette().text
        };
        for step in &self.untraced_steps {
            let guide = canvas::Path::new(|path| {
                for (index, &sample) in step.iter().enumerate() {
                    let point = layout.point(sample);
                    if index == 0 {
                        path.move_to(point);
                    } else {
                        path.line_to(point);
                    }
                }
            });
            frame.stroke(
                &guide,
                canvas::Stroke {
                    line_dash: canvas::LineDash {
                        segments: &[3., 3.],
                        offset: 0,
                    },
                    ..canvas::Stroke::default().with_color(ink).with_width(0.8)
                },
            );
        }
        if self.traced {
            let half =
                (layout.plot.width / self.profile.markers.len().max(1) as f32 * 0.3).min(22.);
            for marker in &self.profile.markers {
                let at = layout.point(marker.sample);
                frame.stroke(
                    &canvas::Path::line(
                        iced::Point::new(at.x - half, at.y),
                        iced::Point::new(at.x + half, at.y),
                    ),
                    canvas::Stroke::default().with_color(ink).with_width(2.),
                );
            }
        }
        for model in &layout.models {
            if let Some(thumbnail) = self.thumbnails.get(model.thumbnail) {
                thumbnail.draw(
                    &mut frame,
                    model.bounds.center(),
                    model.bounds.size(),
                    theme,
                );
            }
        }
        self.profile
            .draw_hover(&mut frame, theme, &layout, cursor.position_in(bounds));
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests;
