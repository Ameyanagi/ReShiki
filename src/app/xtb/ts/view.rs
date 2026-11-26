//! Transition-state path profile and the saved saddle's details.
use super::{Action, Session};
use crate::app::workspace::{accessible_command, control, panel};
use crate::app::xtb::palette::{Action as PaletteAction, Shown};
use crate::app::xtb::tone::{Tone, line};
use crate::app::xtb::{Action as XtbAction, state};
use crate::app::{App, Message};
use iced::widget::{canvas, column, container, row, text};
use iced::{Color, Element, Length, Point, Rectangle, Renderer, Theme, mouse};
use reshiki::qcdata::{CalcResult, Frame, Status};

const KCAL_PER_HARTREE: f64 = 627.509_474_063;
const IMAGINARY_CM1: f64 = 50.;
const RELOAD_NOTE: &str = "Run the search again to inspect structures along the saved path.";

/// One interpretation of the saved result, shared by the card, Path and status.
/// A candidate's energy is never exposed as a barrier before these checks pass.
#[derive(Debug, PartialEq)]
pub(in crate::app::xtb) struct Presentation {
    pub refined_kcal: Option<f64>,
    pub reason: Option<String>,
    pub warning: Option<String>,
    pub highest_path_kcal: Option<f64>,
    pub verification: String,
}

impl Presentation {
    pub fn reliable(&self) -> bool {
        self.refined_kcal.is_some()
    }

    #[cfg(any(test, all(not(target_arch = "wasm32"), feature = "xtb")))]
    pub fn status_line(&self) -> String {
        match self.barrier_line() {
            Some(barrier) => format!("Transition state · {barrier} · {}", self.verification),
            None => format!(
                "No reliable transition state found. {}",
                self.reason
                    .as_deref()
                    .unwrap_or("The search needs another attempt.")
            ),
        }
    }

    pub fn barrier_line(&self) -> Option<String> {
        self.refined_kcal
            .map(|value| format!("ΔE‡ {} kcal/mol", state::signed(value, 1)))
    }
}

fn reported_reason(result: &CalcResult) -> Option<&str> {
    match &result.status {
        Status::NotConverged { reason } | Status::Failed { reason } => Some(reason),
        Status::Converged => None,
    }
}

fn wrong_endpoints(reason: &str) -> bool {
    reason.starts_with("IRC reaches ") || reason.starts_with("IRC did not connect ")
}

fn readable_reason(reason: &str) -> String {
    if reason == "saddle refinement did not converge" {
        "The transition-state refinement did not converge.".into()
    } else if reason.contains("on both sides") {
        reason.replacen("IRC reaches", "The reaction path reaches", 1) + "."
    } else if wrong_endpoints(reason) {
        reason.replacen("IRC", "The reaction path", 1) + "."
    } else if reason.starts_with("engine/worker imaginary frequency counts disagree") {
        "The two frequency checks disagree about the number of imaginary frequencies.".into()
    } else if let Some(reason) = reason.strip_prefix("worker check failed: ") {
        format!("The independent frequency check failed: {reason}.")
    } else if let Some(reason) = reason.strip_prefix("IRC failed: ") {
        format!("The reaction path could not be checked: {reason}.")
    } else {
        reason.to_owned()
    }
}

pub(in crate::app::xtb) fn presentation(
    result: &CalcResult,
    session: Option<&Session>,
) -> Presentation {
    let saved = super::saved_search(result);
    let frequencies = result.vibrations.as_ref().map(|v| &v.frequencies_cm1);
    let saved_count =
        frequencies.map(|frequencies| frequencies.iter().filter(|&&f| f < -IMAGINARY_CM1).count());
    let count = saved
        .map(|search| search.imaginary)
        .or_else(|| session.map(|s| s.extras.num_imaginary))
        .or(saved_count);
    let reported = reported_reason(result);
    // A saved warning proves convergence only when the search reached IRC:
    // the engine starts IRC solely after convergence and one imaginary mode.
    let saddle_converged = saved
        .map(|search| search.saddle_converged)
        .or_else(|| session.map(|s| s.extras.saddle_converged))
        .unwrap_or_else(|| {
            matches!(result.status, Status::Converged)
                || reported.is_some_and(|reason| reason.starts_with("IRC failed: "))
        });
    let accepted_status = matches!(result.status, Status::Converged)
        || matches!(&result.status, Status::NotConverged { reason } if reason.starts_with("IRC failed: "));
    let reliable = super::is_ts(result)
        && saddle_converged
        && count == Some(1)
        && accepted_status
        && !reported.is_some_and(wrong_endpoints);
    let energies = super::path_energies(result);
    let (deltas, candidate) = relative_energies(energies, result.energy_hartree);
    let refined_kcal = candidate.filter(|value| reliable && value.is_finite());
    let reason = if refined_kcal.is_some() {
        None
    } else if let Some(count) = count.filter(|&count| count != 1) {
        Some(match count {
            0 => "The search ended at a point with no significant imaginary frequency.".into(),
            2 => "The search ended at a point with two imaginary frequencies.".into(),
            count => format!("The search ended at a point with {count} imaginary frequencies."),
        })
    } else if let Some(reason) = reported {
        Some(readable_reason(reason))
    } else if !saddle_converged {
        Some("The transition-state refinement did not converge.".into())
    } else {
        Some(
            "The saved result does not contain enough information to verify a transition state."
                .into(),
        )
    };
    let frequency = saved
        .and_then(|search| search.imaginary_cm1)
        .or_else(|| session.and_then(|s| s.extras.engine_imaginary))
        .or_else(|| frequencies.and_then(|f| f.iter().copied().find(|&f| f < -IMAGINARY_CM1)));
    let frequency_check = match (count, frequency) {
        (Some(1), Some(frequency)) => {
            format!("One imaginary frequency {:.0}i cm⁻¹", frequency.abs())
        }
        (Some(1), None) => "One imaginary frequency".into(),
        _ => "Transition state not verified".into(),
    };
    let connects = session.map_or(matches!(result.status, Status::Converged), |s| {
        s.extras.connects
    });
    let verification = if connects {
        format!("{frequency_check} · connects reactant and product ✓")
    } else if reliable {
        format!("{frequency_check} · reaction path not verified")
    } else {
        frequency_check
    };
    Presentation {
        refined_kcal,
        reason,
        warning: reliable.then(|| reported.map(readable_reason)).flatten(),
        highest_path_kcal: deltas
            .into_iter()
            .filter(|value| value.is_finite())
            .reduce(f64::max),
        verification,
    }
}

/// Technical search detail belongs in Log, including after the file is reopened.
pub(in crate::app::xtb) fn search_log(result: &CalcResult, session: Option<&Session>) -> String {
    if !super::is_ts(result) {
        return String::new();
    }
    let mut rows =
        super::saved_search(result).map_or_else(
            || {
                session.map_or_else(
                    || {
                        vec![
                ("Method".into(), "IDPP → CI-NEB → P-RFO → IRC".into()),
                (
                    "Images".into(),
                    super::path_energies(result).len().to_string(),
                ),
                (
                    "NEB iterations / convergence".into(),
                    "not saved with this result".into(),
                ),
                (
                    "Saddle steps / reason".into(),
                    reported_reason(result)
                        .unwrap_or("converged; detailed steps not saved")
                        .into(),
                ),
                (
                    "Worker check".into(),
                    result.vibrations.as_ref().map_or_else(
                        || "not saved".into(),
                        |v| {
                            format!(
                                "significant imaginary modes in the saved frequencies: {}",
                                v.frequencies_cm1
                                    .iter()
                                    .filter(|&&f| f < -IMAGINARY_CM1)
                                    .count()
                            )
                        },
                    ),
                ),
                (
                    "IRC branches".into(),
                    if matches!(result.status, Status::Converged) {
                        "reactant and product; branch details not saved"
                    } else {
                        "branch details not saved"
                    }
                    .into(),
                ),
                (
                    "Evaluations".into(),
                    result.run.as_ref().map_or_else(
                        || "not saved".into(),
                        |run| format!("{} worker evaluations recorded", run.scf_iterations.len()),
                    ),
                ),
                ("Hessians".into(), "count not saved with this result".into()),
            ]
                    },
                    |session| session.extras.diagnostics.clone(),
                )
            },
            |search| search.diagnostics.clone(),
        );
    if let Some(run) = &result.run {
        rows.push(("Total time".into(), format!("{:.2} s", run.seconds)));
    }
    let mut lines = vec!["Transition-state search".into()];
    lines.extend(
        rows.into_iter()
            .map(|(label, value)| format!("{label}: {value}")),
    );
    lines.join("\n")
}

fn send(action: Action) -> Message {
    Message::Xtb(XtbAction::Ts(action))
}

fn climbing_image(energies: &[f64], remembered: Option<usize>) -> usize {
    remembered
        .filter(|&index| index > 0 && index < energies.len().saturating_sub(1))
        .or_else(|| {
            energies
                .iter()
                .enumerate()
                .skip(1)
                .take(energies.len().saturating_sub(2))
                .reduce(|best, next| if next.1 > best.1 { next } else { best })
                .map(|(index, _)| index)
        })
        .unwrap_or(0)
}

fn profile_note(positions: bool, stale: bool) -> Option<&'static str> {
    if !positions {
        Some(RELOAD_NOTE)
    } else if stale {
        Some("The drawing changed; select a path point to inspect its saved energy.")
    } else {
        None
    }
}

fn selection_label(delta: f64) -> String {
    format!("ΔE {} kcal/mol", state::signed(delta, 1))
}

fn relative_energies(energies: &[f64], saddle: Option<f64>) -> (Vec<f64>, Option<f64>) {
    let Some(&reactant) = energies.first() else {
        return (vec![], None);
    };
    let delta = |energy| (energy - reactant) * KCAL_PER_HARTREE;
    (
        energies.iter().copied().map(delta).collect(),
        saddle.map(delta),
    )
}

impl App {
    pub(in crate::app::xtb) fn xtb_ts_path<'a>(
        &'a self,
        shown: &Shown<'a>,
        inspector: bool,
    ) -> Element<'a, Message> {
        let result = &shown.stored.result;
        let energies = super::path_energies(result);
        let climbing = climbing_image(
            energies,
            super::saved_search(result)
                .map(|search| search.climbing_image)
                .or_else(|| shown.ts.map(|s| s.extras.climbing)),
        );
        let selected = self.xtb_step().filter(|&index| index < energies.len());
        let presentation = presentation(result, shown.ts);
        let (deltas, _) = relative_energies(energies, None);
        let refined_delta = presentation.refined_kcal;
        let positions = shown.ts.is_some_and(|session| {
            session.extras.image_positions.len() == energies.len()
                && session
                    .extras
                    .image_positions
                    .iter()
                    .all(|image| image.len() == shown.frame.numbers.len())
        });
        let interactive = !self.xtb_running();
        let chart = PathChart {
            deltas: deltas.clone(),
            refined_delta,
            selected,
            climbing,
            interactive,
            stale: shown.stale,
        };
        let chart = reshiki::accessibility::image(
            "xtb-ts-path-profile",
            chart.accessible_name(),
            canvas(chart).width(Length::Fill).height(190),
        );
        let mut profile = column![chart].spacing(6);
        if let Some(delta) = selected.and_then(|index| deltas.get(index).copied()) {
            profile = profile.push(text(selection_label(delta)).size(12));
        }
        if !deltas.is_empty() {
            // The buttons also make every canvas point keyboard accessible.
            let images = row(deltas.iter().enumerate().map(|(index, &delta)| {
                reshiki::accessibility::button(
                    format!("xtb-ts-image-{index}"),
                    selection_label(delta),
                    text(if index == 0 {
                        "R"
                    } else if index + 1 == deltas.len() {
                        "P"
                    } else {
                        "•"
                    })
                    .size(11),
                )
                .padding([3, 6])
                .checked(selected == Some(index))
                .style(control(selected == Some(index)))
                .on_press_maybe(interactive.then(|| send(Action::Image(index))))
                .into()
            }))
            .spacing(2)
            .wrap();
            profile = profile.push(images);
        } else {
            profile = profile.push(line("No path energy profile was saved.", Tone::Muted));
        }
        if selected.is_some() {
            profile = profile.push(
                accessible_command(
                    "xtb-ts-back",
                    "Return to the transition state result",
                    "Back to result",
                )
                .on_press(Message::Xtb(XtbAction::Palette(PaletteAction::StepBack))),
            );
        }
        if !energies.is_empty()
            && let Some(note) = profile_note(positions, shown.stale)
        {
            profile = profile.push(line(note, Tone::Muted));
        }

        let mut details = column![
            text(if presentation.reliable() {
                "Transition state"
            } else {
                "No reliable transition state found."
            })
            .size(14)
        ]
        .spacing(7);
        if let Some(reason) = &presentation.reason {
            details = details.push(line(reason.clone(), Tone::Warning));
        }
        if presentation.reliable() {
            details = details.push(line(presentation.verification.clone(), Tone::Good));
        }
        if let Some(warning) = &presentation.warning {
            details = details.push(line(warning.clone(), Tone::Warning));
        }
        if super::card::imaginary_mode(result).is_some() {
            details = details.push(
                accessible_command(
                    "xtb-ts-animate",
                    "Animate the saved imaginary mode",
                    "Animate imaginary mode",
                )
                .on_press_maybe(interactive.then(|| {
                    Message::Xtb(XtbAction::Vibrations(
                        crate::app::xtb::vibrations::Action::AnimateImaginary,
                    ))
                })),
            );
        }
        let maps: Vec<_> = shown
            .atoms
            .iter()
            .map(|id| {
                (!shown.stale)
                    .then(|| id.and_then(|id| self.tab.doc.atom(id)))
                    .flatten()
                    .map(|atom| atom.map_num)
                    .filter(|&number| number > 0)
            })
            .collect();
        let changes = self.xtb_ts_bond_changes(shown);
        for (kind, pair) in changes {
            if let Some(value) = bond_length(shown.frame, pair) {
                details = details.push(
                    row![
                        text(format!("{kind} {}", bond_label(shown.frame, pair, &maps)))
                            .size(12)
                            .width(Length::Fill),
                        text(format!("{value:.2} Å")).size(12),
                    ]
                    .spacing(10)
                    .wrap(),
                );
            }
        }
        if !presentation.reliable() {
            details = details.push(
                reshiki::accessibility::button(
                    "xtb-ts-rerun-20",
                    "Try again with a finer path",
                    text("Try again with a finer path").size(12),
                )
                .padding([5, 9])
                .style(control(false))
                .on_press_maybe(self.xtb_ts_can_rerun(shown).then(|| send(Action::Rerun))),
            );
        }
        details = details.push(line(
            "Semi-empirical barriers are rough, often several kcal/mol off. Refine with DFT before quoting them.",
            Tone::Muted,
        ));
        details = details.push(
            reshiki::accessibility::button(
                "xtb-ts-path-export-input",
                "Export TS input",
                text("Export TS input…").size(12),
            )
            .padding([6, 10])
            .style(control(false))
            .on_press(send(Action::Export)),
        );
        let card = container(details)
            .padding(12)
            .width(Length::Fill)
            .style(panel);
        if inspector {
            column![profile, card].spacing(12).into()
        } else {
            row![
                container(profile).width(Length::FillPortion(3)),
                container(card).width(Length::FillPortion(2)),
            ]
            .spacing(18)
            .into()
        }
    }

    /// Compare the saved optimized endpoints, so reopened results use the same
    /// covalent-radius criterion as the search instead of the current drawing.
    fn xtb_ts_bond_changes(&self, shown: &Shown<'_>) -> Vec<(&'static str, [usize; 2])> {
        let Some(endpoints) = shown.stored.result.endpoints else {
            return vec![];
        };
        let science = &self.tab.doc.science;
        let (Some(reactant), Some(product)) = (
            science.frame(&endpoints.reactant),
            science.frame(&endpoints.product),
        ) else {
            return vec![];
        };
        changed_bonds(reactant, product)
    }
}

fn bond_label(frame: &Frame, [a, b]: [usize; 2], maps: &[Option<u32>]) -> String {
    let atom = |index: usize| {
        format!(
            "{}{}",
            state::symbol(frame.numbers.get(index).copied().unwrap_or(0)),
            maps.get(index)
                .copied()
                .flatten()
                .map_or(index + 1, |number| number as usize)
        )
    };
    format!("{}···{}", atom(a), atom(b))
}

fn bond_length(frame: &Frame, [a, b]: [usize; 2]) -> Option<f64> {
    let a = frame.positions_angstrom.get(a)?;
    let b = frame.positions_angstrom.get(b)?;
    Some(
        a.iter()
            .zip(b)
            .map(|(a, b)| (a - b).powi(2))
            .sum::<f64>()
            .sqrt(),
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn changed_bonds(reactant: &Frame, product: &Frame) -> Vec<(&'static str, [usize; 2])> {
    use std::collections::BTreeSet;
    if reactant.numbers != product.numbers {
        return vec![];
    }
    let graph = |frame: &Frame| {
        let positions: Vec<_> = frame
            .positions_angstrom
            .iter()
            .flatten()
            .map(|value| value / reshiki_optimize::ANGSTROM_PER_BOHR)
            .collect();
        reshiki_optimize::ts::bonds(&frame.numbers, &positions)
            .ok()
            .map(|bonds| bonds.into_iter().collect::<BTreeSet<_>>())
    };
    let (Some(reactant), Some(product)) = (graph(reactant), graph(product)) else {
        return vec![];
    };
    product
        .difference(&reactant)
        .map(|&(a, b)| ("Forming", [a, b]))
        .chain(
            reactant
                .difference(&product)
                .map(|&(a, b)| ("Breaking", [a, b])),
        )
        .collect()
}

#[cfg(target_arch = "wasm32")]
fn changed_bonds(_: &Frame, _: &Frame) -> Vec<(&'static str, [usize; 2])> {
    vec![]
}

/// Reactant-relative energy along the reaction coordinate.
struct PathChart {
    deltas: Vec<f64>,
    /// Only a verified refined saddle receives a TS marker.
    refined_delta: Option<f64>,
    selected: Option<usize>,
    climbing: usize,
    interactive: bool,
    stale: bool,
}

impl PathChart {
    const LEFT: f32 = 48.;
    const RIGHT: f32 = 24.;
    const TOP: f32 = 34.;
    const BOTTOM: f32 = 34.;

    fn x(&self, index: usize, bounds: Rectangle) -> f32 {
        let width = (bounds.width - Self::LEFT - Self::RIGHT).max(1.);
        Self::LEFT + width * index as f32 / self.deltas.len().saturating_sub(1).max(1) as f32
    }

    fn nearest(&self, x: f32, bounds: Rectangle) -> Option<usize> {
        (0..self.deltas.len()).min_by(|&a, &b| {
            (self.x(a, bounds) - x)
                .abs()
                .total_cmp(&(self.x(b, bounds) - x).abs())
        })
    }

    fn energy_bounds(&self) -> (f64, f64) {
        self.deltas
            .iter()
            .chain(self.refined_delta.iter())
            .fold((0_f64, 0_f64), |(lo, hi), &energy| {
                (lo.min(energy), hi.max(energy))
            })
    }

    fn refined_ts_marker(&self) -> Option<(f64, String)> {
        self.refined_delta
            .map(|delta| (delta, format!("TS‡ {}", state::signed(delta, 1))))
    }

    fn accessible_name(&self) -> String {
        let mut name = format!(
            "Reaction path energy profile · {} images",
            self.deltas.len()
        );
        if let Some((_, marker)) = self.refined_ts_marker() {
            name.push_str(&format!(" · {marker} kcal/mol"));
        }
        if let Some(delta) = self.selected.and_then(|index| self.deltas.get(index)) {
            name.push_str(&format!(" · Selected {}", selection_label(*delta)));
        }
        name
    }
}

impl canvas::Program<Message> for PathChart {
    type State = ();

    fn update(
        &self,
        _: &mut (),
        event: &iced::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        let iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) = event else {
            return None;
        };
        let point = cursor.position_in(bounds).filter(|_| self.interactive)?;
        let image = self.nearest(point.x, bounds)?;
        Some(canvas::Action::publish(send(Action::Image(image))).and_capture())
    }

    fn mouse_interaction(
        &self,
        _: &(),
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if self.interactive && cursor.is_over(bounds) && !self.deltas.is_empty() {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }

    fn draw(
        &self,
        _: &(),
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let muted = crate::appearance::muted(theme);
        let ink = if self.stale {
            muted
        } else {
            theme.palette().text
        };
        let amber = crate::appearance::readable(
            theme.palette().warning,
            &[theme.palette().background],
            reshiki::color_contrast::TEXT_TARGET,
        );
        frame.fill_text(canvas::Text {
            content: "ΔE from reactants (kcal/mol)".into(),
            position: Point::new(Self::LEFT, 3.),
            color: muted,
            size: 10.into(),
            ..Default::default()
        });
        if self.deltas.is_empty() {
            return vec![frame.into_geometry()];
        }
        let (lo, hi) = self.energy_bounds();
        let range = (hi - lo).max(1.);
        let plot_height = (bounds.height - Self::TOP - Self::BOTTOM).max(1.);
        let y = |energy: f64| Self::TOP + ((hi - energy) / range) as f32 * plot_height;
        let grid = Color { a: 0.25, ..muted };
        for energy in [hi, 0., lo] {
            let start = Point::new(Self::LEFT, y(energy));
            frame.stroke(
                &canvas::Path::line(start, Point::new(bounds.width - Self::RIGHT, start.y)),
                canvas::Stroke::default().with_color(grid).with_width(1.),
            );
        }
        let points: Vec<_> = self
            .deltas
            .iter()
            .enumerate()
            .map(|(index, &energy)| Point::new(self.x(index, bounds), y(energy)))
            .collect();
        if let Some(point) = self.selected.and_then(|index| points.get(index)) {
            frame.stroke(
                &canvas::Path::line(
                    Point::new(point.x, Self::TOP),
                    Point::new(point.x, bounds.height - Self::BOTTOM),
                ),
                canvas::Stroke::default()
                    .with_color(Color {
                        a: 0.25,
                        ..theme.palette().primary
                    })
                    .with_width(1.),
            );
        }
        frame.stroke(
            &canvas::Path::new(|path| {
                for (index, &point) in points.iter().enumerate() {
                    if index == 0 {
                        path.move_to(point);
                    } else {
                        path.line_to(point);
                    }
                }
            }),
            canvas::Stroke::default().with_color(ink).with_width(1.5),
        );
        for (index, &point) in points.iter().enumerate() {
            let selected = self.selected == Some(index);
            if selected {
                frame.stroke(
                    &canvas::Path::circle(point, 6.),
                    canvas::Stroke::default().with_color(ink).with_width(1.),
                );
            }
            frame.fill(
                &canvas::Path::circle(point, if selected { 3.8 } else { 2.6 }),
                ink,
            );
            let label = if index == 0 {
                Some("R".into())
            } else if index + 1 == self.deltas.len() {
                Some("P".into())
            } else {
                None
            };
            if let Some(content) = label {
                frame.fill_text(canvas::Text {
                    content,
                    position: Point::new(point.x, point.y + 7.),
                    color: ink,
                    size: 11.into(),
                    align_x: iced::alignment::Horizontal::Center.into(),
                    ..Default::default()
                });
            }
        }
        if let Some((delta, content)) = self.refined_ts_marker()
            && let Some(climbing) = points.get(self.climbing)
        {
            let point = Point::new(climbing.x, y(delta));
            frame.stroke(
                &canvas::Path::new(|path| {
                    path.move_to(Point::new(point.x, point.y - 6.5));
                    path.line_to(Point::new(point.x + 6.5, point.y));
                    path.line_to(Point::new(point.x, point.y + 6.5));
                    path.line_to(Point::new(point.x - 6.5, point.y));
                    path.close();
                }),
                canvas::Stroke::default().with_color(amber).with_width(1.4),
            );
            let label_y = point.y + if point.y >= climbing.y { 8. } else { -19. };
            frame.fill_text(canvas::Text {
                content,
                position: Point::new(point.x, label_y),
                color: amber,
                size: 11.into(),
                align_x: iced::alignment::Horizontal::Center.into(),
                ..Default::default()
            });
        }
        frame.fill_text(canvas::Text {
            content: "Reaction coordinate".into(),
            position: Point::new(
                (Self::LEFT + bounds.width - Self::RIGHT) / 2.,
                bounds.height - 13.,
            ),
            color: muted,
            size: 10.into(),
            align_x: iced::alignment::Horizontal::Center.into(),
            ..Default::default()
        });
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests;
