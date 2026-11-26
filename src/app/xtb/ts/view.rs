//! Transition-state path profile and the saved saddle's details.
use super::{Action, Session};
use crate::app::workspace::{accessible_command, control, hover_hint, panel};
use crate::app::xtb::palette::{Action as PaletteAction, Shown};
use crate::app::xtb::tone::{Tone, line};
use crate::app::xtb::{Action as XtbAction, EnergyUnit, state};
use crate::app::{App, Message};
use iced::widget::{canvas, column, container, row, text, tooltip};
use iced::{Color, Element, Length, Point, Rectangle, Renderer, Theme, mouse};
use reshiki::qcdata::{CalcResult, Frame, KCAL_PER_HARTREE, Status};

const IMAGINARY_CM1: f64 = 50.;
pub(super) const THERMO_IMAGINARY_CM1: f64 = 20.;
pub(super) const OPTIMIZED_COMPLEX: &str = "optimized complex";
const RELOAD_NOTE: &str = "Run the search again to inspect structures along the saved path.";
pub(super) const IRC_RETURNS_TO_REACTANT: &str = "IRC reaches the reactant on both sides";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Recovery {
    CheckMap,
    RefineHighest,
    ShowWhere,
    FinerPath,
}

impl Recovery {
    pub(super) fn button(
        self,
        id: &'static str,
        arrow: Option<u64>,
        enabled: bool,
        has_endpoint: bool,
    ) -> Element<'static, Message> {
        let label = match self {
            Self::CheckMap => "Check the atom map",
            Self::RefineHighest => "Refine from the highest point",
            Self::ShowWhere => "Show where it leads",
            Self::FinerPath => "Try again with a finer path",
        };
        let missing_endpoint = self == Self::ShowWhere && !has_endpoint;
        let command: Element<'static, Message> = accessible_command(id, label, label)
            .style(crate::appearance::primary)
            .on_press_maybe(arrow.filter(|_| enabled && !missing_endpoint).map(|arrow| {
                send(match self {
                    Self::CheckMap => Action::ReviewMap(arrow),
                    Self::RefineHighest => Action::Retry(arrow, super::Retry::RefineHighest),
                    Self::ShowWhere => Action::ShowWhere(arrow),
                    Self::FinerPath => Action::Retry(arrow, super::Retry::FinerPath),
                })
            }))
            .into();
        if missing_endpoint {
            hover_hint(
                command,
                "This result has no saved endpoint",
                tooltip::Position::Top,
            )
            .into()
        } else {
            command
        }
    }
}

/// One interpretation of the saved result, shared by the card, Path and status.
/// A candidate's energy is never exposed as a barrier before these checks pass.
#[derive(Debug, PartialEq)]
pub(in crate::app::xtb) struct Presentation {
    pub refined_kcal: Option<f64>,
    pub reason: Option<String>,
    pub(super) recovery: Recovery,
    pub warning: Option<String>,
    pub highest_path_kcal: Option<f64>,
    pub verification: String,
}

impl Presentation {
    pub fn reliable(&self) -> bool {
        self.refined_kcal.is_some()
    }

    pub fn status_line(&self, unit: EnergyUnit) -> String {
        match self.barrier_line(unit) {
            Some(barrier) => format!("Transition state · {barrier}"),
            None => "xTB · No reliable transition state found.".into(),
        }
    }

    pub fn barrier_line(&self, unit: EnergyUnit) -> Option<String> {
        self.refined_kcal
            .map(|value| format!("ΔE‡ {}", unit.format(value)))
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

fn disagreement_counts(reason: &str) -> Option<(usize, usize)> {
    let counts =
        reason.strip_prefix("engine/worker imaginary frequency counts disagree: engine ")?;
    let (engine, worker) = counts.split_once(", worker ")?;
    Some((engine.parse().ok()?, worker.parse().ok()?))
}

pub(super) fn has_reached_endpoint(result: &CalcResult) -> bool {
    super::saved_search(result).is_some_and(|search| !search.reached_positions_angstrom.is_empty())
}

fn readable_reason(reason: &str) -> String {
    if reason == IRC_RETURNS_TO_REACTANT {
        "The path returned to the reactant on both sides. The bonds that should change may not match this drawing.".into()
    } else if reason == "saddle refinement did not converge" {
        "The transition-state refinement did not converge.".into()
    } else if reason.contains("on both sides") {
        reason.replacen("IRC reaches", "The reaction path reaches", 1) + "."
    } else if wrong_endpoints(reason) {
        reason.replacen("IRC", "The reaction path", 1) + "."
    } else if let Some(reason) = reason.strip_prefix("worker check failed: ") {
        format!("The independent frequency check failed: {reason}.")
    } else if let Some(reason) = reason.strip_prefix("IRC failed: ") {
        let short = if reason.starts_with("SCF failure") {
            "the electronic calculation did not converge"
        } else if reason.contains("step limit") {
            "the endpoint optimization reached its step limit"
        } else {
            reason
                .lines()
                .next()
                .unwrap_or(reason)
                .trim_end_matches('.')
        };
        format!("The reaction-path check could not finish: {short}.")
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
    let disagreement = reported.and_then(disagreement_counts).or_else(|| {
        count
            .zip(saved_count)
            .filter(|(engine, worker)| engine != worker)
    });
    // Converged results passed both the endpoint and independent frequency checks.
    let accepted = matches!(result.status, Status::Converged);
    let saddle_converged = saved
        .map(|search| search.saddle_converged)
        .or_else(|| session.map(|s| s.extras.saddle_converged))
        .unwrap_or(accepted);
    let connects = session.map_or(accepted, |s| s.extras.connects);
    let reliable = super::is_ts(result)
        && saddle_converged
        && count == Some(1)
        && disagreement.is_none()
        && connects
        && accepted;
    let energies = super::path_energies(result);
    let (deltas, candidate) = relative_energies(energies, result.energy_hartree);
    let refined_kcal = candidate.filter(|value| reliable && value.is_finite());
    let recovery = match reported {
        _ if disagreement.is_some() => Recovery::FinerPath,
        Some(IRC_RETURNS_TO_REACTANT) => Recovery::CheckMap,
        Some(reason) if wrong_endpoints(reason) => Recovery::ShowWhere,
        _ if count.is_some_and(|count| count >= 2)
            && session.is_some_and(|session| session.can_refine(result)) =>
        {
            Recovery::RefineHighest
        }
        _ => Recovery::FinerPath,
    };
    let reason = if refined_kcal.is_some() {
        None
    } else if let Some((engine, worker)) = disagreement {
        Some(format!(
            "The two frequency checks disagree (engine {engine}, worker {worker})."
        ))
    } else if matches!(recovery, Recovery::CheckMap | Recovery::ShowWhere) {
        reported.map(readable_reason)
    } else if let Some(count) = count.filter(|&count| count >= 2) {
        Some(if count == 2 {
            "The search ended at a point with two imaginary frequencies.".into()
        } else {
            format!("The search ended at a point with {count} imaginary frequencies.")
        })
    } else if count == Some(0) {
        Some("The search ended at a point with no significant imaginary frequency.".into())
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
            format!("✓ one imaginary frequency {:.0}i cm⁻¹", frequency.abs())
        }
        (Some(1), None) => "✓ one imaginary frequency".into(),
        _ => "Transition state not verified".into(),
    };
    let verification = if connects {
        format!("{frequency_check} · ✓ path connects reactant and product")
    } else {
        frequency_check
    };
    let diagnostics = saved
        .map(|search| search.diagnostics.as_slice())
        .or_else(|| session.map(|s| s.extras.diagnostics.as_slice()));
    let warning = (reliable
        && diagnostics.is_some_and(|rows| {
            rows.iter().any(|(label, detail)| {
                label.starts_with("IRC ") && detail.contains(" · downhill trace not converged · ")
            })
        }))
    .then(|| "Reaction-path descent ended before gradient convergence.".into());
    Presentation {
        refined_kcal,
        reason,
        recovery,
        warning,
        highest_path_kcal: deltas
            .into_iter()
            .filter(|value| value.is_finite())
            .reduce(f64::max),
        verification,
    }
}

/// Technical search detail belongs in Log, including after the file is reopened.
pub(in crate::app::xtb) fn search_log(
    result: &CalcResult,
    session: Option<&Session>,
    unit: EnergyUnit,
) -> String {
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
    lines.extend(rows.into_iter().map(|(label, value)| {
        if label == "Folded the starting conformer"
            && let Some((distances, energy)) = value.rsplit_once(" (")
            && let Some(energy) = energy.strip_suffix(" kcal/mol)")
            && let Ok(kcal) = energy.parse::<f64>()
        {
            let plus = if kcal.is_sign_positive() { "+" } else { "" };
            format!("{label}: {distances} ({plus}{})", unit.format(kcal))
        } else {
            format!("{label}: {value}")
        }
    }));
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

fn selection_label(delta: f64, unit: EnergyUnit) -> String {
    format!("ΔE {}", unit.format(delta))
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

/// The optimized endpoints remain valid reaction-energy evidence even if the
/// saddle was not verified. Older saves kept these same endpoints in the band.
pub(in crate::app::xtb) fn reaction_energy_kcal(result: &CalcResult) -> Option<f64> {
    let hartree = super::saved_search(result)
        .and_then(|search| search.reaction_energy_hartree())
        .or_else(|| {
            let [reactant, .., product] = super::path_energies(result) else {
                return None;
            };
            Some(product - reactant)
        })?;
    let kcal = hartree * KCAL_PER_HARTREE;
    kcal.is_finite().then_some(kcal)
}

pub(in crate::app::xtb) fn reaction_gibbs_kcal(result: &CalcResult) -> Option<f64> {
    let delta = super::saved_search(result)?.reaction_gibbs_hartree()? * KCAL_PER_HARTREE;
    delta.is_finite().then_some(delta)
}

pub(super) fn reaction_gibbs_unavailable(result: &CalcResult) -> Option<String> {
    let search = super::saved_search(result)?;
    if !search.free_energy || reaction_gibbs_kcal(result).is_some() {
        return None;
    }
    let reason = search.diagnostics.iter().find_map(|(label, detail)| {
        let side = match label.as_str() {
            "Reactant thermochemistry" => "reactant",
            "Product thermochemistry" => "product",
            _ => return None,
        };
        let reason = detail.strip_prefix("Free energy unavailable: ")?;
        Some(format!("{side} frequency analysis failed ({reason})"))
    });
    Some(format!(
        "Reaction ΔG not reported: {}",
        reason.as_deref().unwrap_or("see the calculation log")
    ))
}

/// The sorted frequency list starts with the reaction mode for a verified TS.
pub(super) fn extra_thermo_imaginary(frequencies: &[f64]) -> Option<f64> {
    frequencies
        .iter()
        .copied()
        .filter(|&f| f <= -THERMO_IMAGINARY_CM1)
        .nth(1)
}

pub(super) fn barrier_gibbs_unavailable(result: &CalcResult) -> Option<String> {
    let frequency = extra_thermo_imaginary(&result.vibrations.as_ref()?.frequencies_cm1)?;
    Some(format!(
        "ΔG‡ not reported: a second small imaginary mode ({:.0}i cm⁻¹)",
        frequency.abs()
    ))
}

/// A Gibbs barrier needs the same verified saddle as the electronic barrier,
/// plus TS and reactant corrections evaluated under matching conditions.
pub(in crate::app::xtb) fn barrier_gibbs_kcal(result: &CalcResult) -> Option<f64> {
    if !presentation(result, None).reliable() {
        return None;
    }
    let search = super::saved_search(result)?;
    let [reactant, _] = search.endpoint_energies_hartree?;
    let [reactant_thermo, _] = search.endpoint_thermo.as_ref()?;
    let reactant_thermo = reactant_thermo.as_ref()?;
    let ts_thermo = result.thermo.as_ref()?;
    if result
        .vibrations
        .as_ref()
        .is_some_and(|v| extra_thermo_imaginary(&v.frequencies_cm1).is_some())
    {
        return None;
    }
    if (ts_thermo.temperature_k - reactant_thermo.temperature_k).abs() > 1e-6
        || (ts_thermo.pressure_pa - reactant_thermo.pressure_pa).abs() > 1e-3
        || (ts_thermo.frequency_scale - reactant_thermo.frequency_scale).abs() > 1e-12
        || (ts_thermo.rotor_cutoff_cm1 - reactant_thermo.rotor_cutoff_cm1).abs() > 1e-12
        || (ts_thermo.imaginary_cutoff_cm1 - reactant_thermo.imaginary_cutoff_cm1).abs() > 1e-12
    {
        return None;
    }
    let delta = (result.energy_hartree? + ts_thermo.gibbs_correction_hartree
        - reactant
        - reactant_thermo.gibbs_correction_hartree)
        * KCAL_PER_HARTREE;
    delta.is_finite().then_some(delta)
}

/// Copy and export the same reactant-relative quantities shown on Path.
pub(in crate::app::xtb) fn path_rows(
    result: &CalcResult,
    session: Option<&Session>,
    unit: EnergyUnit,
    reference: Option<&str>,
) -> Vec<(String, String)> {
    let (deltas, _) = relative_energies(super::path_energies(result), None);
    let mut rows: Vec<_> = deltas
        .iter()
        .enumerate()
        .map(|(index, &delta)| {
            let label = if index == 0 {
                "R ΔE".into()
            } else if index + 1 == deltas.len() {
                reference.map_or_else(|| "P ΔE".into(), |reference| format!("P ΔE ({reference})"))
            } else {
                format!("Image {} ΔE", index + 1)
            };
            (label, unit.format(delta))
        })
        .collect();
    if let Some(delta) = presentation(result, session).refined_kcal {
        rows.push(("TS‡ ΔE".into(), unit.format(delta)));
    }
    if let Some(delta) = barrier_gibbs_kcal(result) {
        rows.push(("TS‡ ΔG (298.15 K)".into(), unit.format(delta)));
    }
    if let Some(delta) = reaction_gibbs_kcal(result) {
        rows.push(("P ΔG (298.15 K)".into(), unit.format(delta)));
    }
    rows
}

impl App {
    pub(in crate::app::xtb) fn xtb_ts_live_path(&self, live: &super::Live) -> Element<'_, Message> {
        if live.energies.is_empty() {
            return container(line("Finding a path…", Tone::Muted))
                .center(Length::Fill)
                .height(190)
                .into();
        }
        PathChart::live(&live.energies, self.appearance.energy_unit).widget()
    }

    fn xtb_ts_path_chart(&self, shown: &Shown<'_>, refined_delta: Option<f64>) -> PathChart {
        let result = &shown.stored.result;
        let energies = super::path_energies(result);
        let (deltas, _) = relative_energies(energies, None);
        PathChart {
            deltas,
            refined_delta,
            selected: self.xtb_step().filter(|&index| index < energies.len()),
            climbing: climbing_image(
                energies,
                super::saved_search(result)
                    .map(|search| search.climbing_image)
                    .or_else(|| shown.ts.map(|s| s.extras.climbing)),
            ),
            interactive: !self.xtb_running(),
            stale: shown.stale,
            live: false,
            unit: self.appearance.energy_unit,
        }
    }

    pub(in crate::app::xtb) fn xtb_ts_path<'a>(
        &'a self,
        shown: &Shown<'a>,
        inspector: bool,
    ) -> Element<'a, Message> {
        let result = &shown.stored.result;
        let energies = super::path_energies(result);
        let presentation =
            presentation(result, self.xtb_ts_refinement_session(&shown.stored.result));
        let chart = self.xtb_ts_path_chart(shown, presentation.refined_kcal);
        let selected = chart.selected;
        let deltas = chart.deltas.clone();
        let interactive = chart.interactive;
        let unit = chart.unit;
        let positions = shown.ts.is_some_and(|session| {
            session.extras.image_positions.len() == energies.len()
                && session
                    .extras
                    .image_positions
                    .iter()
                    .all(|image| image.len() == shown.frame.numbers.len())
        });
        let mut profile = column![chart.widget()].spacing(6);
        if let Some(delta) = selected.and_then(|index| deltas.get(index).copied()) {
            profile = profile.push(text(selection_label(delta, unit)).size(12));
        }
        if !deltas.is_empty() {
            // The buttons also make every canvas point keyboard accessible.
            let images = row(deltas.iter().enumerate().map(|(index, &delta)| {
                reshiki::accessibility::button(
                    format!("xtb-ts-image-{index}"),
                    selection_label(delta, unit),
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
        let reached = self.xtb_reached_endpoint_selected();
        if has_reached_endpoint(result) {
            profile = profile.push(
                accessible_command(
                    "xtb-ts-reached-endpoint",
                    "Reached endpoint",
                    "Reached endpoint",
                )
                .checked(reached)
                .style(control(reached))
                .on_press_maybe(
                    (interactive && !shown.stale).then(|| send(Action::ReachedEndpoint)),
                ),
            );
        } else if presentation.recovery == Recovery::ShowWhere {
            profile = profile.push(line(
                "The reached geometry was not saved with this result.",
                Tone::Muted,
            ));
        }
        if selected.is_some() || reached {
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
            if super::saved_search(result).is_some_and(|search| search.free_energy)
                && let Some(reason) = barrier_gibbs_unavailable(result)
            {
                details = details.push(line(reason, Tone::Warning));
            }
        }
        details = details
            .push(reaction_gibbs_unavailable(result).map(|reason| line(reason, Tone::Warning)));
        if let Some(warning) = &presentation.warning {
            details = details.push(line(warning.clone(), Tone::Muted));
        }
        if super::card::imaginary_mode(result).is_some() {
            details = details.push(
                accessible_command(
                    "xtb-ts-animate",
                    "Animate the saved imaginary mode",
                    "Animate imaginary mode",
                )
                .style(crate::appearance::secondary)
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
        let geometry = self.xtb_ts_bond_geometry(shown, &maps);
        if !geometry.is_empty() {
            let distance = |value: String| {
                text(value)
                    .size(12)
                    .width(42)
                    .align_x(iced::alignment::Horizontal::Right)
            };
            let mut table = column![
                text("Bond distances (Å)").size(12),
                row![
                    text("Bond").size(11).width(Length::Fill),
                    distance("R".into()),
                    distance("TS".into()),
                    distance("P".into()),
                ]
                .spacing(6),
            ]
            .spacing(5);
            for [bond, reactant, ts, product] in geometry {
                table = table.push(
                    row![
                        text(bond).size(12).width(Length::Fill),
                        distance(reactant),
                        distance(ts),
                        distance(product),
                    ]
                    .spacing(6),
                );
            }
            details = details.push(table);
        }
        if !presentation.reliable() && crate::app::xtb::available() {
            details = details.push(presentation.recovery.button(
                "xtb-ts-rerun-20",
                shown.arrow,
                self.xtb_ts_can_rerun(shown)
                    && (presentation.recovery != Recovery::ShowWhere || !shown.stale),
                has_reached_endpoint(result),
            ));
        }
        details = details.push(line(
            "Semi-empirical barriers are rough estimates. Refine with DFT before quoting them.",
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
    fn xtb_ts_bond_geometry(&self, shown: &Shown<'_>, maps: &[Option<u32>]) -> Vec<[String; 4]> {
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
        bond_geometry_rows(reactant, shown.frame, product, maps)
    }
}

fn bond_geometry_rows(
    reactant: &Frame,
    ts: &Frame,
    product: &Frame,
    maps: &[Option<u32>],
) -> Vec<[String; 4]> {
    let distance = |frame: &Frame, pair| {
        bond_length(frame, pair).map_or_else(|| "—".into(), |value| format!("{value:.2}"))
    };
    changed_bonds(reactant, product)
        .into_iter()
        .map(|(kind, pair)| {
            [
                format!("{kind} {}", bond_label(ts, pair, maps)),
                distance(reactant, pair),
                distance(ts, pair),
                distance(product, pair),
            ]
        })
        .collect()
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
    live: bool,
    unit: EnergyUnit,
}

impl PathChart {
    const LEFT: f32 = 48.;
    const RIGHT: f32 = 24.;
    const TOP: f32 = 34.;
    const BOTTOM: f32 = 34.;

    fn live(energies: &[f64], unit: EnergyUnit) -> Self {
        Self {
            deltas: relative_energies(energies, None).0,
            refined_delta: None,
            selected: None,
            climbing: 0,
            interactive: false,
            stale: false,
            live: true,
            unit,
        }
    }

    fn widget(self) -> Element<'static, Message> {
        reshiki::accessibility::image(
            "xtb-ts-path-profile",
            self.accessible_name(),
            canvas(self).width(Length::Fill).height(190),
        )
    }

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
            .map(|delta| (delta, format!("TS‡ {}", self.unit.number(delta))))
    }

    fn axis_title(&self) -> String {
        format!("ΔE from reactants ({})", self.unit.label())
    }

    fn endpoint_marker(&self, index: usize) -> Option<String> {
        if self.live {
            return None;
        }
        let point = if index == 0 {
            "R"
        } else if index + 1 == self.deltas.len() {
            "P"
        } else {
            return None;
        };
        self.deltas
            .get(index)
            .map(|&delta| format!("{point} {}", self.unit.number(delta)))
    }

    fn accessible_name(&self) -> String {
        let profile = if self.live {
            "Live reaction path"
        } else {
            "Reaction path"
        };
        let mut name = format!("{profile} energy profile · {} images", self.deltas.len());
        if let Some((_, marker)) = self.refined_ts_marker() {
            name.push_str(&format!(" · {marker} {}", self.unit.label()));
        }
        if let Some(delta) = self.selected.and_then(|index| self.deltas.get(index)) {
            name.push_str(&format!(
                " · Selected {}",
                selection_label(*delta, self.unit)
            ));
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
            content: self.axis_title(),
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
        let mut ticks = vec![hi, 0., lo];
        ticks.dedup();
        for energy in ticks {
            let start = Point::new(Self::LEFT, y(energy));
            frame.stroke(
                &canvas::Path::line(start, Point::new(bounds.width - Self::RIGHT, start.y)),
                canvas::Stroke::default().with_color(grid).with_width(1.),
            );
            frame.fill_text(canvas::Text {
                content: self.unit.number(energy),
                position: Point::new(Self::LEFT - 7., start.y - 6.),
                color: muted,
                size: 10.into(),
                align_x: iced::alignment::Horizontal::Right.into(),
                ..Default::default()
            });
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
            if let Some(content) = self.endpoint_marker(index) {
                frame.fill_text(canvas::Text {
                    content,
                    position: Point::new(point.x, point.y + 7.),
                    color: ink,
                    size: 11.into(),
                    align_x: if index == 0 {
                        iced::alignment::Horizontal::Left.into()
                    } else {
                        iced::alignment::Horizontal::Right.into()
                    },
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
