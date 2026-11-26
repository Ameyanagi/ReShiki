//! Reaction energies are ordinary, sequential molecule calculations. Their
//! results remain attached to each molecule; only the queue and summary live
//! with the open document.
use crate::app::workspace::accessible_command;
use crate::app::xtb::tone::{Tone, line};
use crate::app::xtb::{self, CalcTask, Outcome, Request, state};
use crate::app::{App, Message};
use iced::widget::{column, row, text};
use iced::{Element, Length, Task};
use reshiki::document::Document;
use reshiki::qcdata::{self, CalcResult, Frame};
use reshiki::science::{self, Freshness};
use std::sync::{Arc, Weak};

const KCAL_PER_HARTREE: f64 = 627.509_474_063;
const TEMPERATURE_K: f64 = 298.15;

/// One connected molecule, with its reviewed electronic state. Coefficients
/// weight the final sum and never make a multi-molecule calculation input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::app::xtb) struct Species {
    pub atoms: Vec<u64>,
    pub coefficient: u32,
    pub product: bool,
    pub charge: i32,
    pub multiplicity: u32,
}

#[derive(Default)]
pub(in crate::app::xtb) struct State {
    task: Option<CalcTask>,
    batch: Option<Batch>,
    summaries: Vec<Summary>,
    stopped: Option<(u64, String)>,
}

struct Batch {
    arrow: u64,
    species: Vec<Species>,
    task: CalcTask,
    snapshot: String,
    next: usize,
    reused: usize,
    inputs: Vec<Input>,
    active: Option<u64>,
    review: bool,
}

struct Summary {
    arrow: u64,
    species: Vec<Species>,
    task: CalcTask,
    snapshot: String,
    reused: usize,
    inputs: Vec<Input>,
}

/// The exact stored calculation used for one term, retained across later
/// calculations of the same species. Attachment IDs alone are not unique to
/// results, and a frame can have several calculations with different values.
struct Input {
    attachment: u64,
    result: Arc<CalcResult>,
    previous: Vec<Weak<CalcResult>>,
}

impl Input {
    fn from_view(view: &science::ResultView<'_>) -> Self {
        Self {
            attachment: view.stored.attachment,
            result: Arc::clone(&view.stored.result),
            previous: Vec::new(),
        }
    }

    fn matches(&self, result: &Arc<CalcResult>) -> bool {
        Arc::ptr_eq(&self.result, result)
            || self
                .previous
                .iter()
                .any(|previous| xtb::is(previous, result))
    }
}

#[derive(Debug, PartialEq)]
struct Totals {
    energy: f64,
    gibbs: Option<f64>,
}

impl Totals {
    fn line(&self) -> String {
        let energy = format!("ΔE {:+.2} kcal/mol", self.energy);
        self.gibbs.map_or(energy.clone(), |gibbs| {
            format!("{energy} · ΔG (298.15 K) {gibbs:+.2} kcal/mol")
        })
    }
}

/// A conservative snapshot of roles and chemistry, independent of generated
/// 3D models and stored results. Edits never silently carry reviewed settings
/// over to a different species while the queue runs.
fn snapshot(doc: &Document, arrow: u64, species: &[Species]) -> Option<String> {
    let reaction = doc.reactions.iter().find(|r| r.arrow == arrow)?;
    let mut ids: Vec<_> = species
        .iter()
        .flat_map(|s| s.atoms.iter().copied())
        .collect();
    ids.sort_unstable();
    ids.dedup();
    let atoms: Option<Vec<_>> = ids
        .iter()
        .map(|&id| {
            let a = doc.atom(id)?;
            Some(serde_json::json!([
                a.id,
                a.element,
                a.charge,
                a.radical_electrons,
                a.singlet,
                a.isotope,
                a.explicit_h,
                a.no_implicit,
                a.aromatic,
                a.stereo,
                a.stereo_group,
                a.map_num,
                a.purpose,
                a.centroid,
                a.attachment,
            ]))
        })
        .collect();
    let bonds: Vec<_> = doc
        .bonds
        .iter()
        .filter(|b| ids.binary_search(&b.a).is_ok() || ids.binary_search(&b.b).is_ok())
        .map(|b| {
            serde_json::json!([
                b.a,
                b.b,
                b.order,
                b.stereo,
                b.stereo_atoms,
                b.stereo_authoritative,
                b.display,
                b.distance_derived,
            ])
        })
        .collect();
    let chemistry: Vec<_> = species
        .iter()
        .map(|species| science::baseline(doc, &species.atoms))
        .collect();
    Some(
        serde_json::json!([
            reaction.reactants,
            reaction.products,
            atoms?,
            bonds,
            chemistry
        ])
        .to_string(),
    )
}

fn valid_species(doc: &Document, species: &[Species]) -> bool {
    species.iter().any(|s| !s.product)
        && species.iter().any(|s| s.product)
        && species.iter().all(|s| {
            s.coefficient > 0
                && s.multiplicity > 0
                && s.atoms.first().is_some_and(|&atom| {
                    let molecule = reshiki::molecule_names::component(doc, atom);
                    molecule.len() == s.atoms.len()
                        && molecule.iter().all(|id| s.atoms.contains(id))
                })
        })
}

/// axtb's automatic rotational symmetry number for its detected point group.
fn point_group_symmetry(group: &str) -> Option<u32> {
    match group.to_ascii_lowercase().as_str() {
        "c1" | "cs" | "ci" | "cinfv" | "kh" => Some(1),
        "dinfh" => Some(2),
        "t" | "td" | "th" => Some(12),
        "o" | "oh" => Some(24),
        "i" | "ih" => Some(60),
        group => {
            let (family, suffix) = group.split_at_checked(1)?;
            let order: u32 = suffix
                .trim_end_matches(|c: char| c.is_ascii_alphabetic())
                .parse()
                .ok()?;
            match family {
                "c" => Some(order),
                "d" => order.checked_mul(2),
                "s" => Some(order / 2),
                _ => None,
            }
        }
    }
}

/// Match both Calculate symmetry policies. For natural abundances the engine
/// reports the point group it detected. Isotope-labelled jobs request a
/// geometry-based override because the engine's detection ignores isotopes.
fn matching_thermo(result: &CalcResult, frame: &Frame) -> bool {
    let (Some(thermo), Some(vibrations)) = (&result.thermo, &result.vibrations) else {
        return false;
    };
    let masses = &vibrations.masses_amu;
    if masses.len() != frame.numbers.len()
        || masses.iter().any(|mass| !mass.is_finite() || *mass <= 0.)
        || !frame.masses_amu.is_empty() && *masses != frame.masses_amu
    {
        return false;
    }
    let symmetry = if let Some(expected_masses) = reshiki::science::isotope_masses(frame) {
        if masses != &expected_masses {
            return false;
        }
        let labels: Vec<_> = frame
            .numbers
            .iter()
            .enumerate()
            .map(|(index, &number)| {
                u32::from(number) * 1000
                    + u32::from(frame.isotopes.get(index).copied().unwrap_or(0))
            })
            .collect();
        Some(qcdata::vibrational::rotational_symmetry_number(
            &labels,
            &frame.positions_angstrom,
            masses,
        ))
    } else if frame.masses_amu.is_empty() {
        thermo.point_group.as_deref().and_then(point_group_symmetry)
    } else {
        None
    };
    (thermo.temperature_k - TEMPERATURE_K).abs() < 1e-6
        && (thermo.pressure_pa - 101_325.).abs() < 1e-3
        && (thermo.frequency_scale - 1.).abs() < 1e-12
        && (thermo.rotor_cutoff_cm1 - 50.).abs() < 1e-12
        && (thermo.imaginary_cutoff_cm1 - 20.).abs() < 1e-12
        && symmetry == Some(thermo.symmetry_number)
        && thermo.gibbs_correction_hartree.is_finite()
}

/// Keep the entire reaction on the same GFN2 gas-phase Hamiltonian, with
/// unconstrained optimized minima. Threads do not change result eligibility.
fn matching(result: &CalcResult, frame: &Frame, species: &Species, task: CalcTask) -> bool {
    let qcdata::Settings::Xtb(settings) = &result.settings else {
        return false;
    };
    let optimized = result.task == qcdata::Task::Optimize
        && result.status == qcdata::Status::Converged
        && result.energy_hartree.is_some_and(f64::is_finite);
    let settings_match = settings.method == qcdata::XtbMethod::Gfn2Xtb
        && settings.conventions == qcdata::XtbConventions::Axtb
        && settings.solvent.is_none()
        && settings.electronic_temperature_k.is_none()
        && settings.accuracy.is_none()
        && settings.max_iterations.is_none()
        && settings.scf_retries.is_empty()
        && settings.convergence
            == Some(if task == CalcTask::OptFreq {
                qcdata::Convergence::Tight
            } else {
                qcdata::Convergence::Normal
            })
        && settings.fixed_atoms.is_empty();
    let thermo_match = task != CalcTask::OptFreq || matching_thermo(result, frame);
    optimized
        && settings_match
        && thermo_match
        && frame.charge == species.charge
        && frame.multiplicity == species.multiplicity
}

fn eligible(view: &science::ResultView<'_>, species: &Species, task: CalcTask) -> bool {
    view.freshness == Freshness::Current
        && view.attachment.atoms.iter().flatten().count() == species.atoms.len()
        && species
            .atoms
            .iter()
            .all(|id| view.attachment.atoms.contains(&Some(*id)))
        && matching(&view.stored.result, view.frame, species, task)
}

fn reusable(doc: &Document, species: &Species, task: CalcTask) -> Option<Input> {
    let &atom = species.atoms.first()?;
    science::results_for(doc, atom)
        .into_iter()
        .rev()
        .find(|view| eligible(view, species, task))
        .map(|view| Input::from_view(&view))
}

/// Called immediately after ordinary storage. Only that newly stored result
/// can finish the active term, even if another eligible result exists.
fn completed(doc: &Document, species: &Species, task: CalcTask) -> Option<Input> {
    let index = doc.science.results.len().checked_sub(1)?;
    science::results_for(doc, *species.atoms.first()?)
        .into_iter()
        .find(|view| view.index == index && eligible(view, species, task))
        .map(|view| Input::from_view(&view))
}

fn totals<'a>(
    values: impl IntoIterator<Item = (&'a Species, &'a CalcResult)>,
    task: CalcTask,
) -> Option<Totals> {
    let mut energy = 0.;
    let mut gibbs = (task == CalcTask::OptFreq).then_some(0.);
    for (species, result) in values {
        let coefficient = f64::from(species.coefficient) * if species.product { 1. } else { -1. };
        let electronic = result.energy_hartree.filter(|e| e.is_finite())?;
        energy += coefficient * electronic;
        if let Some(total) = &mut gibbs {
            let correction = result.thermo.as_ref()?.gibbs_correction_hartree;
            *total += coefficient * (electronic + correction);
        }
    }
    let energy = energy * KCAL_PER_HARTREE;
    let gibbs = gibbs.map(|g| g * KCAL_PER_HARTREE);
    (energy.is_finite() && gibbs.is_none_or(f64::is_finite)).then_some(Totals { energy, gibbs })
}

fn input_totals(
    doc: &Document,
    species: &[Species],
    inputs: &[Input],
    task: CalcTask,
) -> Option<Totals> {
    let science = &doc.science;
    if inputs.len() != species.len()
        || !inputs.iter().all(|input| {
            science.results.iter().any(|stored| {
                stored.attachment == input.attachment && input.matches(&stored.result)
            }) && science.frame(&input.result.frame).is_some()
                && science
                    .attachment(input.attachment)
                    .is_some_and(|attachment| attachment.freshness(doc) == Freshness::Current)
        })
    {
        return None;
    }
    totals(
        species
            .iter()
            .zip(inputs.iter().map(|input| input.result.as_ref())),
        task,
    )
}

impl State {
    /// Follow light or pinned copies while keeping Undo's original identity.
    pub(in crate::app::xtb) fn follow_result(
        &mut self,
        old: &science::Stored,
        new: &science::Stored,
    ) {
        let inputs = self
            .batch
            .iter_mut()
            .flat_map(|batch| &mut batch.inputs)
            .chain(
                self.summaries
                    .iter_mut()
                    .flat_map(|summary| &mut summary.inputs),
            );
        for input in inputs {
            if input.attachment == old.attachment && input.matches(&old.result) {
                input
                    .previous
                    .retain(|previous| previous.strong_count() > 0);
                input.previous.push(Arc::downgrade(&input.result));
                input.result = Arc::clone(&new.result);
            }
        }
    }

    pub(in crate::app::xtb) fn task(&self, fallback: CalcTask) -> CalcTask {
        self.task.unwrap_or(if fallback == CalcTask::OptFreq {
            CalcTask::OptFreq
        } else {
            CalcTask::Optimize
        })
    }

    pub(in crate::app::xtb) fn in_progress(&self) -> bool {
        self.batch.is_some()
    }

    pub(in crate::app::xtb) fn running(&self, serial: u64) -> bool {
        self.batch
            .as_ref()
            .is_some_and(|b| b.active == Some(serial))
    }

    pub(in crate::app::xtb) fn stop(&mut self, reason: &str) -> bool {
        let Some(batch) = self.batch.take() else {
            return false;
        };
        self.stopped = Some((batch.arrow, reason.into()));
        true
    }

    pub(in crate::app::xtb) fn progress(&self) -> Option<String> {
        let batch = self.batch.as_ref()?;
        Some(if batch.review {
            format!(
                "Reaction energy · review species {} of {}",
                batch.next + 1,
                batch.species.len()
            )
        } else {
            format!(
                "Reaction energy · species {} of {}",
                batch.next + 1,
                batch.species.len()
            )
        })
    }
}

impl App {
    pub(in crate::app::xtb) fn xtb_reaction_energy_task(&mut self, task: CalcTask) {
        if matches!(task, CalcTask::Optimize | CalcTask::OptFreq)
            && self.tab.xtb.run.is_none()
            && !self.tab.xtb.energy_state.in_progress()
        {
            self.tab.xtb.energy_state.task = Some(task);
        }
    }

    /// Entered only after the shared review has validated every species.
    pub(in crate::app::xtb) fn xtb_reaction_energy_begin(
        &mut self,
        arrow: u64,
        species: Vec<Species>,
        task: CalcTask,
    ) -> Task<Message> {
        if !xtb::available() || !self.xtb_can_start() {
            return Task::none();
        }
        if !matches!(task, CalcTask::Optimize | CalcTask::OptFreq)
            || !valid_species(&self.tab.doc, &species)
        {
            self.status = "Review the reaction species before calculating its energy".into();
            self.error = false;
            return Task::none();
        }
        let Some(snapshot) = snapshot(&self.tab.doc, arrow, &species) else {
            return Task::none();
        };
        self.tab.xtb.energy_state.task = Some(task);
        self.tab.xtb.energy_state.stopped = None;
        self.tab.xtb.energy_state.batch = Some(Batch {
            arrow,
            species,
            task,
            snapshot,
            next: 0,
            reused: 0,
            inputs: Vec::new(),
            active: None,
            review: false,
        });
        self.xtb_reaction_energy_next()
    }

    fn xtb_reaction_energy_next(&mut self) -> Task<Message> {
        loop {
            let Some(mut batch) = self.tab.xtb.energy_state.batch.take() else {
                return Task::none();
            };
            if snapshot(&self.tab.doc, batch.arrow, &batch.species).as_ref()
                != Some(&batch.snapshot)
            {
                self.status = "Reaction energy stopped: the reaction changed".into();
                self.tab.xtb.energy_state.stopped = Some((batch.arrow, self.status.clone()));
                self.error = false;
                return Task::none();
            }
            let Some(species) = batch.species.get(batch.next).cloned() else {
                let Some(totals) =
                    input_totals(&self.tab.doc, &batch.species, &batch.inputs, batch.task)
                else {
                    self.status = "Reaction energy stopped: a species result is stale".into();
                    self.tab.xtb.energy_state.stopped = Some((batch.arrow, self.status.clone()));
                    self.error = false;
                    return Task::none();
                };
                self.status = format!("Reaction energy · {}", totals.line());
                self.error = false;
                self.tab
                    .xtb
                    .energy_state
                    .summaries
                    .retain(|s| s.arrow != batch.arrow);
                self.tab.xtb.energy_state.summaries.push(Summary {
                    arrow: batch.arrow,
                    species: batch.species,
                    task: batch.task,
                    snapshot: batch.snapshot,
                    reused: batch.reused,
                    inputs: batch.inputs,
                });
                return Task::none();
            };
            if let Some(input) = reusable(&self.tab.doc, &species, batch.task) {
                batch.inputs.push(input);
                batch.next += 1;
                batch.reused += 1;
                self.tab.xtb.energy_state.batch = Some(batch);
                continue;
            }
            let config = state::Config {
                task: batch.task,
                method: qcdata::XtbMethod::Gfn2Xtb,
                charge: species.charge,
                unpaired: species.multiplicity - 1,
                solvent: None,
                xtb68: false,
                accuracy: None,
                temperature_k: None,
                max_iterations: None,
                threads: self.xtb.settings.threads(),
                tight: false,
                fixed: vec![],
            };
            self.tab.xtb.energy_state.batch = Some(batch);
            let task = self.xtb_begin(Request {
                atoms: species.atoms,
                config,
                keep_full: false,
                start: None,
                search: None,
                ts: None,
            });
            let serial = self.tab.xtb.run.as_ref().map(|r| r.serial);
            if let Some(batch) = &mut self.tab.xtb.energy_state.batch {
                batch.active = serial;
            }
            if let Some(progress) = self.tab.xtb.energy_state.progress() {
                self.status = format!("{progress} · {}", self.status);
            }
            return task;
        }
    }

    /// Called after ordinary storage (including Apply anyway). A stereo
    /// question keeps the queue paused until that same result is accepted.
    pub(in crate::app::xtb) fn xtb_reaction_energy_continue(&mut self) -> Task<Message> {
        if self.tab.xtb.run.is_some() {
            return Task::none();
        }
        let Some(batch) = &mut self.tab.xtb.energy_state.batch else {
            return Task::none();
        };
        if snapshot(&self.tab.doc, batch.arrow, &batch.species).as_ref() != Some(&batch.snapshot) {
            let reason = "Reaction energy stopped: the reaction changed";
            self.tab.xtb.energy_state.stop(reason);
            self.status = reason.into();
            self.error = false;
            return Task::none();
        }
        if matches!(self.tab.xtb.outcome, Some(Outcome::Review(_))) {
            batch.review = true;
            self.status = format!("Reaction energy paused · {}", self.status);
            return Task::none();
        }
        let input = matches!(self.tab.xtb.outcome, Some(Outcome::Done { .. }))
            .then(|| {
                batch
                    .species
                    .get(batch.next)
                    .and_then(|species| completed(&self.tab.doc, species, batch.task))
            })
            .flatten();
        let Some(input) = input else {
            let reason = "Reaction energy stopped: the species optimization did not converge";
            self.tab.xtb.energy_state.stop(reason);
            self.status = reason.into();
            self.error = false;
            return Task::none();
        };
        batch.inputs.push(input);
        batch.next += 1;
        batch.active = None;
        batch.review = false;
        self.xtb_reaction_energy_next()
    }

    pub(super) fn xtb_reaction_energy_section(&self, arrow: u64) -> Element<'_, Message> {
        let state = &self.tab.xtb.energy_state;
        let mut body = column![text("Reaction energy").size(13)].spacing(7);
        let batch = state.batch.as_ref().filter(|b| b.arrow == arrow);
        if let Some(batch) = batch {
            body = body.push(line(state.progress().unwrap_or_default(), Tone::Muted));
            if batch.review
                && let Some(outcome @ Outcome::Review(_)) = &self.tab.xtb.outcome
            {
                body = body.push(xtb::view::outcome_view(self, outcome));
            }
            body = body.push(
                accessible_command(
                    "xtb-reaction-energy-cancel",
                    "Cancel reaction energy",
                    "Cancel",
                )
                .on_press(Message::Xtb(xtb::Action::Cancel)),
            );
        } else {
            // A failed rerun comes first; an older summary is then the previous result.
            let stopped = state.stopped.as_ref().filter(|(at, _)| *at == arrow);
            if let Some((_, reason)) = stopped {
                body = body.push(line(reason.clone(), Tone::Warning));
            }
            let summary = state.summaries.iter().find(|s| s.arrow == arrow);
            if let Some(summary) = summary {
                let current = snapshot(&self.tab.doc, arrow, &summary.species).as_ref()
                    == Some(&summary.snapshot);
                if let Some(totals) = current
                    .then(|| {
                        input_totals(
                            &self.tab.doc,
                            &summary.species,
                            &summary.inputs,
                            summary.task,
                        )
                    })
                    .flatten()
                {
                    body = body.push(match stopped {
                        Some(_) => line(format!("Previous result: {}", totals.line()), Tone::Muted),
                        None => line(totals.line(), Tone::Good),
                    });
                    if summary.reused > 0 {
                        body = body.push(line(
                            format!(
                                "Reused {} current species result{}",
                                summary.reused,
                                if summary.reused == 1 { "" } else { "s" }
                            ),
                            Tone::Muted,
                        ));
                    }
                } else {
                    body = body.push(line(
                        "The reaction or a species result changed; calculate again for current energies.",
                        Tone::Warning,
                    ));
                }
            }
            let idle = xtb::available()
                && self.tab.xtb.run.is_none()
                && state.batch.is_none()
                && !self.tab.busy
                && !self.ts_review_open();
            let task = state.task(self.xtb.settings.task);
            body = body.push(
                row([
                    (CalcTask::Optimize, "Optimize", "optimize"),
                    (CalcTask::OptFreq, "Opt + freq", "opt-freq"),
                ]
                .into_iter()
                .map(|(choice, name, id)| {
                    crate::app::xtb::view::segment(
                        format!("xtb-reaction-energy-task-{id}"),
                        name,
                        task == choice,
                        idle.then_some(Message::Xtb(xtb::Action::Ts(super::Action::EnergyTask(
                            choice,
                        )))),
                        None,
                    )
                }))
                .spacing(4),
            );
            body = body.push(
                accessible_command(
                    "xtb-reaction-energy-review",
                    "Calculate reaction energy",
                    "Calculate",
                )
                .style(crate::appearance::primary)
                .width(Length::Fill)
                .on_press_maybe(idle.then_some(Message::Xtb(xtb::Action::Ts(
                    super::Action::Review(super::review::Mode::Energy, Some(arrow)),
                )))),
            );
        }
        body.width(Length::Fill).into()
    }
}

#[cfg(test)]
mod tests;
