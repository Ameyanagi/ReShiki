//! Transition-state path profile and the saved saddle's details.
use super::irc::Profile;
use super::{Action, Session, irc};
use crate::app::workspace::{accessible_command, control, hover_hint, panel};
use crate::app::xtb::palette::{Action as PaletteAction, Shown};
use crate::app::xtb::tone::{Tone, line};
use crate::app::xtb::{Action as XtbAction, EnergyUnit, state};
use crate::app::{App, Message};
use iced::widget::{canvas, column, container, row, text, tooltip};
use iced::{Element, Length, Rectangle, Renderer, Theme, mouse};
use reshiki::qcdata::{CalcResult, Frame, KCAL_PER_HARTREE, Status, XtbMethod};

const IMAGINARY_CM1: f64 = 50.;
pub(super) const THERMO_IMAGINARY_CM1: f64 = 20.;
pub(super) const OPTIMIZED_COMPLEX: &str = "optimized complex";
const RELOAD_NOTE: &str = "Run the search again to inspect structures along the saved path.";
pub(super) const IRC_RETURNS_TO_REACTANT: &str = "IRC reaches the reactant on both sides";

/// Use the saved calculation's electronic method throughout its presentation.
pub(in crate::app::xtb) fn method_name(result: &CalcResult) -> &str {
    if let reshiki::qcdata::Settings::Xtb(settings) = &result.settings {
        return settings.method.label();
    }
    if let Some(search) = super::saved_search(result) {
        return search.method.label();
    }
    result
        .provenance
        .method
        .as_deref()
        .map(super::super::results::method_without_optimizer)
        .filter(|method| !method.trim().is_empty())
        .unwrap_or("xTB")
}

pub(super) fn is_gfn2(result: &CalcResult) -> bool {
    method_name(result) == XtbMethod::Gfn2Xtb.label()
}

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
    pub method: String,
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
            Some(barrier) => format!("Transition state · {} · {barrier}", self.method),
            None => format!("{} · No reliable transition state found.", self.method),
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
    let gxtb = method_name(result) == XtbMethod::GXtb.label();
    let connects = if gxtb {
        use reshiki::qcdata::IrcEndpoint::{Product, Reactant};
        saved
            .and_then(|search| search.irc.as_ref())
            .is_some_and(|path| matches!(path.reached, [Reactant, Product] | [Product, Reactant]))
            && session.is_none_or(|s| s.extras.connects)
    } else {
        session.map_or(accepted, |s| s.extras.connects)
    };
    let worker_verified = !gxtb || saved_count == Some(1);
    let reliable = super::is_ts(result)
        && saddle_converged
        && count == Some(1)
        && worker_verified
        && disagreement.is_none()
        && connects
        && accepted;
    let energies = super::path_energies(result);
    let (deltas, candidate) = relative_energies(
        energies,
        result.energy_hartree,
        super::optimized_reactant_energy(result),
    );
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
    } else if gxtb && !worker_verified {
        Some("The saved g-xTB result has no verified worker frequency spectrum.".into())
    } else if gxtb && !connects {
        Some("The saved g-xTB IRC does not verify both reactant and product endpoints.".into())
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
        _ if disagreement.is_some() => "Frequency checks disagree".into(),
        _ if !worker_verified => "Worker frequency check unavailable".into(),
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
        method: method_name(result).to_owned(),
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
    rows.retain(|(label, _)| label != "Electronic method");
    if let Some(run) = &result.run {
        rows.push(("Total time".into(), format!("{:.2} s", run.seconds)));
    }
    let mut lines = vec![
        "Transition-state search".into(),
        format!("Electronic method: {}", method_name(result)),
    ];
    if super::saved_search(result).is_some_and(|search| search.refined_from.is_some()) {
        lines.push(
            "Starting saddle: GFN2-xTB; saddle, Hessian and IRC recalculated with g-xTB".into(),
        );
    }
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

fn relative_energies(
    energies: &[f64],
    saddle: Option<f64>,
    reference: Option<f64>,
) -> (Vec<f64>, Option<f64>) {
    let Some(reactant) = reference.or_else(|| energies.first().copied()) else {
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
    let search = super::saved_search(result);
    let refinement = search.is_some_and(|search| search.refined_from.is_some());
    let deltas = if refinement {
        search
            .and_then(|search| search.endpoint_energies_hartree)
            .map(|[reactant, product]| vec![0., (product - reactant) * KCAL_PER_HARTREE])
            .unwrap_or_default()
    } else {
        relative_energies(
            super::path_energies(result),
            None,
            super::optimized_reactant_energy(result),
        )
        .0
    };
    let mut rows = vec![("Electronic method".into(), method_name(result).into())];
    rows.extend(
        deltas
            .iter()
            .enumerate()
            .map(|(index, &delta)| {
                let label = if index == 0 {
                    "R ΔE".into()
                } else if index + 1 == deltas.len() {
                    reference
                        .map_or_else(|| "P ΔE".into(), |reference| format!("P ΔE ({reference})"))
                } else if refinement {
                    "TS candidate ΔE".into()
                } else {
                    format!("Image {} ΔE", index + 1)
                };
                (label, unit.format(delta))
            })
            .collect::<Vec<_>>(),
    );
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
        use reshiki::qcdata::IrcEndpoint::{Other, Product, Reactant};

        let result = &shown.stored.result;
        let energies = super::path_energies(result);
        let reference = super::optimized_reactant_energy(result);
        let (neb, _) = relative_energies(energies, None, reference);
        let search = super::saved_search(result);
        let refinement = search.is_some_and(|search| search.refined_from.is_some());
        let mut chart = PathChart {
            selected: self.xtb_step().filter(|&index| index < energies.len()),
            climbing: climbing_image(
                energies,
                search
                    .map(|s| s.climbing_image)
                    .or_else(|| shown.ts.map(|s| s.extras.climbing)),
            ),
            interactive: !self.xtb_running(),
            stale: shown.stale,
            live: false,
            unit: self.appearance.energy_unit,
            deltas: neb,
            refined_delta,
            ts_note: self.xtb_ts_annotation(shown),
            coordinates: vec![],
            neb_overlay: vec![],
            thumbnails: vec![],
            endpoint_labels: ["R", "P"],
            relaxed_deltas: None,
            irc: false,
            refinement,
        };
        if let Some(path) = irc::saved(result).filter(|_| self.xtb_profile(result) == Profile::Irc)
            && let Some(reference) = reference
        {
            let delta = |energy: f64| (energy - reference) * KCAL_PER_HARTREE;
            chart.irc = true;
            chart.interactive = false;
            chart.selected = None;
            chart.climbing = path.ts_index;
            chart.coordinates = path.arc_lengths_sqrt_amu_bohr.clone();
            if !refinement {
                chart.neb_overlay = path
                    .neb_arc_lengths_sqrt_amu_bohr
                    .iter()
                    .copied()
                    .zip(energies.iter().copied().map(delta))
                    .collect();
            }
            chart.deltas = path
                .trajectory
                .energies_hartree
                .iter()
                .copied()
                .map(delta)
                .collect();
            chart.relaxed_deltas = path
                .relaxed_ends
                .as_ref()
                .map(|ends| ends.each_ref().map(|end| delta(end.energy_hartree)));
            chart.endpoint_labels = path.reached.map(|reached| match reached {
                Reactant => "R",
                Product => "P",
                Other => "End",
            });
            if path.has_frames()
                && path.reached == [Reactant, Product]
                && let Some(models) = super::thumbnails::from_irc(&self.tab.doc, result, path)
            {
                chart.thumbnails = Vec::from(models);
            }
        } else if let Some(models) = super::thumbnails::from_result(&self.tab.doc, result) {
            chart.thumbnails = Vec::from(models);
        }
        chart
    }

    fn xtb_ts_annotation(&self, shown: &Shown<'_>) -> Option<String> {
        let result = &shown.stored.result;
        let search = super::saved_search(result);
        let maps = self.xtb_ts_atom_maps(shown);
        let mut notes = Vec::new();
        if let Some(ends) = result.endpoints
            && let Some(r) = self.tab.doc.science.frame(&ends.reactant)
            && let Some(p) = self.tab.doc.science.frame(&ends.product)
        {
            let changes = super::thumbnails::bond_changes(result, r, p);
            let distances = changes
                .forming
                .into_iter()
                .filter_map(|pair| {
                    let length = bond_length(shown.frame, pair)?;
                    Some(format!(
                        "{} {length:.2} Å",
                        bond_label(shown.frame, pair, &maps)
                    ))
                })
                .collect::<Vec<_>>();
            if !distances.is_empty() {
                notes.push(format!("forming {}", distances.join(", ")));
            }
        }
        if let Some(frequency) = search.and_then(|s| s.imaginary_cm1).or_else(|| {
            result
                .vibrations
                .as_ref()?
                .frequencies_cm1
                .iter()
                .copied()
                .find(|&f| f < -IMAGINARY_CM1)
        }) {
            notes.push(format!("{:.0}i cm⁻¹", frequency.abs()));
        }
        (!notes.is_empty()).then(|| notes.join(" · "))
    }

    fn xtb_ts_atom_maps(&self, shown: &Shown<'_>) -> Vec<Option<u32>> {
        shown
            .atoms
            .iter()
            .map(|id| {
                id.filter(|_| !shown.stale)
                    .and_then(|id| self.tab.doc.atom(id))
                    .map(|a| a.map_num)
                    .filter(|&n| n > 0)
            })
            .collect()
    }

    pub(in crate::app::xtb) fn xtb_profile_controls(
        &self,
        shown: &Shown<'_>,
    ) -> Element<'static, Message> {
        let result = &shown.stored.result;
        let profile = self.xtb_profile(result);
        let path = irc::saved(result);
        let refinement =
            super::saved_search(result).is_some_and(|search| search.refined_from.is_some());
        let tab = |id, label, choice, enabled: bool| {
            accessible_command(id, label, label)
                .checked(profile == choice)
                .style(control(profile == choice))
                .on_press_maybe(enabled.then(|| send(Action::Profile(choice))))
        };
        let irc_tab = tab("xtb-path-irc", "IRC", Profile::Irc, path.is_some());
        let hint = match path {
            None => "No IRC energies were saved with this result",
            Some(path) if !path.has_frames() => "Open the full file to animate the IRC",
            Some(_) if refinement => {
                "ΔE is relative to the g-xTB optimized reactant. The saddle and both IRC branches use g-xTB."
            }
            _ => {
                "ΔE is relative to the optimized reactant. Gray dots show NEB images matched by mass-weighted arc length on each side of the saddle."
            }
        };
        let mut tabs = row![hover_hint(irc_tab, hint, tooltip::Position::Top)].spacing(1);
        if !refinement {
            tabs = tabs.push(tab(
                "xtb-path-neb",
                "NEB band",
                Profile::Neb,
                !super::path_energies(result).is_empty(),
            ));
        }
        let mut controls = row![].spacing(6);
        if let Some(pair) = self.xtb_ts_linked_pair(result) {
            controls =
                controls.push(
                    row(pair.into_iter().map(|saved| {
                        let active = std::sync::Arc::ptr_eq(saved, result);
                        accessible_command(
                            if is_gfn2(saved) {
                                "xtb-path-method-gfn2"
                            } else {
                                "xtb-path-method-gxtb"
                            },
                            method_name(saved).to_owned(),
                            method_name(saved).to_owned(),
                        )
                        .checked(active)
                        .style(control(active))
                        .on_press_maybe((!self.xtb_running()).then(|| {
                            send(Action::Card(super::card::Action::Result(saved.clone())))
                        }))
                        .into()
                    }))
                    .spacing(1),
                );
        }
        controls = controls.push(tabs);
        if profile == Profile::Irc && path.is_some_and(|p| p.has_frames()) {
            let playing = self.xtb_irc_preview().is_some();
            let label = if playing { "Stop" } else { "Animate IRC" };
            controls = controls
                .push(
                    accessible_command("xtb-animate-irc", label, label).on_press_maybe(
                        (!shown.stale && !self.xtb_running()).then(|| send(Action::PlayIrc)),
                    ),
                )
                .push(
                    accessible_command("xtb-export-irc", "Export IRC (XYZ)", "Export IRC (XYZ)…")
                        .on_press(send(Action::ExportIrc)),
                );
        }
        controls.wrap().vertical_spacing(4).into()
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
        let is_irc = chart.irc;
        let interactive = !self.xtb_running();
        let unit = chart.unit;
        let positions = shown.ts.is_some_and(|session| {
            session.extras.image_positions.len() == energies.len()
                && session
                    .extras
                    .image_positions
                    .iter()
                    .all(|image| image.len() == shown.frame.numbers.len())
        });
        let mut profile = column![].spacing(6);
        if inspector {
            profile = profile.push(self.xtb_profile_controls(shown));
        }
        let height = if inspector {
            232.
        } else {
            // Space remaining between the dock header and its pinned Path controls.
            (crate::app::xtb::palette::dock_height(self.window_size.height) - 148.)
                .clamp(146., 212.)
        };
        profile = profile.push(chart.widget_height(height));
        if let Some(delta) = selected.and_then(|index| deltas.get(index).copied()) {
            profile = profile.push(text(selection_label(delta, unit)).size(12));
        }
        if !is_irc && !deltas.is_empty() {
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
        } else if deltas.is_empty() {
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
        if !is_irc
            && !energies.is_empty()
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
            .size(14),
            line(method_name(result).to_owned(), Tone::Muted),
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
        if presentation.reliable()
            && is_gfn2(result)
            && super::saved_search(result).is_some()
            && crate::app::xtb::available()
        {
            details = details.push(
                accessible_command(
                    "xtb-path-refine-gxtb",
                    "Refine with g-xTB",
                    "Refine with g-xTB",
                )
                .style(crate::appearance::secondary)
                .on_press_maybe(
                    shown
                        .arrow
                        .filter(|_| !shown.stale && interactive)
                        .map(|arrow| send(Action::RefineGxtb(arrow, result.clone()))),
                ),
            );
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
        let maps = self.xtb_ts_atom_maps(shown);
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

fn changed_bonds(reactant: &Frame, product: &Frame) -> Vec<(&'static str, [usize; 2])> {
    let changes = super::thumbnails::endpoint_changes(reactant, product);
    changes
        .forming
        .into_iter()
        .map(|pair| ("Forming", pair))
        .chain(changes.breaking.into_iter().map(|pair| ("Breaking", pair)))
        .collect()
}

/// Reactant-relative energy along a NEB or mass-weighted IRC coordinate.
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
    /// Empty for the uniformly spaced NEB image coordinate.
    coordinates: Vec<f64>,
    /// NEB comparison samples in the same coordinate and energy reference.
    neb_overlay: Vec<(f64, f64)>,
    ts_note: Option<String>,
    thumbnails: Vec<crate::app::models3d::thumbnail::Thumbnail>,
    irc: bool,
    refinement: bool,
    /// IRC reaches may differ from the optimized, intended R and P structures.
    endpoint_labels: [&'static str; 2],
    relaxed_deltas: Option<[f64; 2]>,
}

impl PathChart {
    fn live(energies: &[f64], unit: EnergyUnit) -> Self {
        Self {
            deltas: relative_energies(energies, None, None).0,
            refined_delta: None,
            selected: None,
            climbing: 0,
            interactive: false,
            stale: false,
            live: true,
            unit,
            coordinates: vec![],
            neb_overlay: vec![],
            ts_note: None,
            thumbnails: vec![],
            irc: false,
            refinement: false,
            endpoint_labels: ["R", "P"],
            relaxed_deltas: None,
        }
    }

    fn widget(self) -> Element<'static, Message> {
        self.widget_height(260.)
    }

    fn widget_height(self, height: f32) -> Element<'static, Message> {
        reshiki::accessibility::image(
            "xtb-ts-path-profile",
            self.accessible_name(),
            canvas(self).width(Length::Fill).height(height),
        )
    }

    fn coordinate(&self, index: usize) -> f64 {
        self.coordinates.get(index).copied().unwrap_or(index as f64)
    }

    fn profile(&self) -> crate::app::energy_plot::profile::Profile {
        use crate::app::energy_plot::profile::{Marker, Profile, Sample, Side, Style};
        let mut markers = vec![];
        if !self.live && !self.deltas.is_empty() {
            let relaxed = self
                .relaxed_deltas
                .map_or([None; 2], |deltas| deltas.map(Some));
            for ((index, thumbnail), (label, relaxed)) in [(0, 0), (self.deltas.len() - 1, 2)]
                .into_iter()
                .zip(self.endpoint_labels.into_iter().zip(relaxed))
            {
                if !self.irc && index == 0 && thumbnail == 2 {
                    continue;
                }
                if let Some(&energy) = self.deltas.get(index).filter(|e| e.is_finite()) {
                    let x = self.coordinate(index);
                    markers.push(Marker {
                        sample: Sample {
                            x,
                            energy: relaxed.unwrap_or(energy),
                        },
                        label: label.into(),
                        side: Side::Below,
                        transition: false,
                        note: None,
                        thumbnail: (self.thumbnails.len() > thumbnail).then_some(thumbnail),
                        connector_from: relaxed.map(|_| Sample { x, energy }),
                        tooltip: relaxed.map(|_| "relaxed IRC end".into()),
                    });
                }
            }
            if let Some(delta) = self.refined_delta.filter(|delta| delta.is_finite()) {
                let x = if self.irc {
                    0.
                } else {
                    self.coordinate(self.climbing)
                };
                markers.push(Marker {
                    sample: Sample { x, energy: delta },
                    label: "TS‡".into(),
                    side: Side::Above,
                    transition: true,
                    note: self.ts_note.clone(),
                    thumbnail: (self.thumbnails.len() > 1).then_some(1),
                    connector_from: None,
                    tooltip: None,
                });
            }
        }
        // Thumbnails are packed in coordinate order (R, TS, P), independent of
        // whether the refined saddle marker is available.
        markers.sort_by(|a, b| a.sample.x.total_cmp(&b.sample.x));
        Profile {
            samples: self
                .deltas
                .iter()
                .enumerate()
                .map(|(index, &energy)| Sample {
                    x: self.coordinate(index),
                    energy,
                })
                .collect(),
            overlay: self
                .neb_overlay
                .iter()
                .map(|&(x, energy)| Sample { x, energy })
                .collect(),
            markers,
            selected: self.selected,
            style: Style::Line,
            unit: self.unit,
            x_title: if self.irc {
                "IRC path length s (amu½·bohr)"
            } else {
                "Reaction coordinate"
            }
            .into(),
            x_ticks: self.irc,
            highlight_curve: self.irc,
            stale: self.stale,
        }
    }

    #[cfg(test)]
    fn x(&self, index: usize, bounds: Rectangle) -> f32 {
        self.profile().x(self.coordinate(index), bounds)
    }

    fn nearest(&self, x: f32, bounds: Rectangle) -> Option<usize> {
        self.profile().nearest(x, bounds)
    }

    #[cfg(test)]
    fn energy_bounds(&self) -> (f64, f64) {
        self.profile().energy_bounds()
    }

    fn refined_ts_marker(&self) -> Option<(f64, String)> {
        self.refined_delta
            .filter(|delta| delta.is_finite())
            .map(|delta| (delta, format!("TS‡ {}", self.unit.number(delta))))
    }

    fn axis_title(&self) -> String {
        format!("ΔE ({})", self.unit.label())
    }

    fn endpoint_marker(&self, branch: usize) -> Option<String> {
        if self.live {
            return None;
        }
        let label = self.endpoint_labels.get(branch)?;
        let traced = if branch == 0 {
            self.deltas.first()
        } else {
            self.deltas.last()
        };
        let energy = self
            .relaxed_deltas
            .and_then(|deltas| deltas.get(branch).copied())
            .or(traced.copied())?;
        Some(format!("{label} {}", self.unit.number(energy)))
    }

    fn accessible_name(&self) -> String {
        let profile = if self.live {
            "Live reaction path"
        } else if self.irc {
            "IRC"
        } else {
            "Reaction path"
        };
        let mut name = format!(
            "{profile} energy profile · {} {} · {}",
            self.deltas.len(),
            if self.irc || self.refinement {
                "points"
            } else {
                "images"
            },
            self.axis_title()
        );
        if self.irc {
            name.push_str(" · IRC path length s (amu½·bohr)");
            if self.endpoint_labels[0] == "R" {
                name.push_str(", reactant side negative");
            }
            if !self.neb_overlay.is_empty() {
                name.push_str(" · Gray dots show NEB images matched by arc length");
            }
            if self.relaxed_deltas.is_some() {
                name.push_str(
                    " · Dotted connectors show where the IRC end was relaxed to the minimum",
                );
            }
        }
        for branch in 0..2 {
            if let Some(marker) = self.endpoint_marker(branch) {
                name.push_str(&format!(" · {marker} {}", self.unit.label()));
            }
        }
        if let Some((_, marker)) = self.refined_ts_marker() {
            name.push_str(&format!(" · {marker} {}", self.unit.label()));
            if let Some(note) = &self.ts_note {
                name.push_str(&format!(" · {note}"));
            }
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
        if self.irc
            && matches!(
                event,
                iced::Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft)
            )
        {
            return Some(canvas::Action::request_redraw());
        }
        let iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) = event else {
            return None;
        };
        let point = cursor
            .position_in(bounds)
            .filter(|_| self.interactive && !self.irc)?;
        let image = self.nearest(point.x, bounds)?;
        Some(canvas::Action::publish(send(Action::Image(image))).and_capture())
    }

    fn mouse_interaction(
        &self,
        _: &(),
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if self.interactive && !self.irc && cursor.is_over(bounds) && !self.deltas.is_empty() {
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
        cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let profile = self.profile();
        let layout = profile.layout(bounds);
        profile.draw(&mut frame, theme, &layout);
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
        profile.draw_hover(&mut frame, theme, &layout, cursor.position_in(bounds));
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests;
