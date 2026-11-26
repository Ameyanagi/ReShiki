//! Reaction energies are ordinary, sequential molecule calculations. Their
//! results remain attached to each molecule; only the queue and summary live
//! with the open document.
use crate::app::workspace::accessible_command;
use crate::app::xtb::tone::{Tone, line};
use crate::app::xtb::{self, CalcTask, Outcome, Request, state};
use crate::app::{App, Message};
use iced::widget::{Space, column, container, row, text};
use iced::{Element, Length, Task};
use reshiki::document::Document;
use reshiki::qcdata::{self, CalcResult, Frame};
use reshiki::science::{self, Freshness};
use std::sync::Arc;

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
    active: Option<u64>,
    review: bool,
}

struct Summary {
    arrow: u64,
    species: Vec<Species>,
    task: CalcTask,
    snapshot: String,
    reused: usize,
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
    Some(serde_json::json!([reaction.reactants, reaction.products, atoms?, bonds]).to_string())
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
        && settings.convergence.is_none_or(|convergence| {
            convergence
                == if task == CalcTask::OptFreq {
                    qcdata::Convergence::Tight
                } else {
                    qcdata::Convergence::Normal
                }
        })
        && settings.fixed_atoms.is_empty();
    let thermo_match = task != CalcTask::OptFreq
        || result.vibrations.is_some()
            && result.thermo.as_ref().is_some_and(|thermo| {
                (thermo.temperature_k - TEMPERATURE_K).abs() < 1e-6
                    && (thermo.pressure_pa - 101_325.).abs() < 1e-3
                    && thermo.gibbs_correction_hartree.is_finite()
            });
    optimized
        && settings_match
        && thermo_match
        && frame.charge == species.charge
        && frame.multiplicity == species.multiplicity
}

fn reusable(doc: &Document, species: &Species, task: CalcTask) -> Option<Arc<CalcResult>> {
    let &atom = species.atoms.first()?;
    science::results_for(doc, atom)
        .into_iter()
        .rev()
        .find(|view| {
            view.freshness == Freshness::Current
                && view.attachment.atoms.iter().flatten().count() == species.atoms.len()
                && species
                    .atoms
                    .iter()
                    .all(|id| view.attachment.atoms.contains(&Some(*id)))
                && matching(&view.stored.result, view.frame, species, task)
        })
        .map(|view| Arc::clone(&view.stored.result))
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

fn current_totals(doc: &Document, species: &[Species], task: CalcTask) -> Option<Totals> {
    let results: Option<Vec<_>> = species.iter().map(|s| reusable(doc, s, task)).collect();
    totals(
        species
            .iter()
            .zip(results.as_ref()?.iter().map(Arc::as_ref)),
        task,
    )
}

impl State {
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
                self.status =
                    "Reaction energy stopped: the reaction changed; review it again".into();
                self.tab.xtb.energy_state.stopped = Some((batch.arrow, self.status.clone()));
                self.error = false;
                return Task::none();
            }
            let Some(species) = batch.species.get(batch.next).cloned() else {
                let Some(totals) = current_totals(&self.tab.doc, &batch.species, batch.task) else {
                    self.status =
                        "Reaction energy stopped: a species result is no longer current".into();
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
                });
                return Task::none();
            };
            if reusable(&self.tab.doc, &species, batch.task).is_some() {
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
            let reason = "Reaction energy stopped: the reaction changed; review it again";
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
        let stored = matches!(self.tab.xtb.outcome, Some(Outcome::Done { .. }))
            && batch
                .species
                .get(batch.next)
                .is_some_and(|species| reusable(&self.tab.doc, species, batch.task).is_some());
        if !stored {
            let reason = "Reaction energy stopped: the species optimization did not converge";
            self.tab.xtb.energy_state.stop(reason);
            self.status = reason.into();
            self.error = false;
            return Task::none();
        }
        batch.next += 1;
        batch.active = None;
        batch.review = false;
        self.xtb_reaction_energy_next()
    }

    pub(in crate::app) fn xtb_reaction_energy_card(&self, arrow: u64) -> Element<'_, Message> {
        if !xtb::available() {
            return Space::new().height(0).into();
        }
        let state = &self.tab.xtb.energy_state;
        let mut body = column![text("Reaction energy").size(13)].spacing(7);
        let batch = state.batch.as_ref().filter(|b| b.arrow == arrow);
        if let Some(batch) = batch {
            body = body.push(line(state.progress().unwrap_or_default(), Tone::Muted));
            if batch.review {
                body = body.push(line(
                    "Review the optimized geometry in Calculate to continue.",
                    Tone::Warning,
                ));
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
            let summary = state.summaries.iter().find(|s| s.arrow == arrow);
            if let Some(summary) = summary {
                let current = snapshot(&self.tab.doc, arrow, &summary.species).as_ref()
                    == Some(&summary.snapshot);
                if let Some(totals) = current
                    .then(|| current_totals(&self.tab.doc, &summary.species, summary.task))
                    .flatten()
                {
                    body = body.push(line(totals.line(), Tone::Good));
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
                        "The reaction changed; calculate again for current energies.",
                        Tone::Warning,
                    ));
                }
            } else if let Some((_, reason)) = state.stopped.as_ref().filter(|(at, _)| *at == arrow)
            {
                body = body.push(line(reason.clone(), Tone::Warning));
            } else {
                body = body.push(line(
                    "Optimize each reactant and product with GFN2-xTB.",
                    Tone::Muted,
                ));
            }
            let idle = self.tab.xtb.run.is_none() && state.batch.is_none() && !self.tab.busy;
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
                    "Calculate…",
                )
                .width(Length::Fill)
                .on_press_maybe(idle.then_some(Message::Xtb(xtb::Action::Ts(
                    super::Action::Review(super::review::Mode::Energy, Some(arrow)),
                )))),
            );
        }
        body = body.push(line(
            "Σ products − Σ reactants, weighted by coefficients",
            Tone::Muted,
        ));
        crate::app::inspector::card(container(body).padding(10))
    }
}

#[cfg(test)]
mod tests;
