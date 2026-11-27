//! Refine starting geometries whose force-field shapes need an electronic method.
use super::{Failure, Job, Key, element_name, force_field_caution, radical_subject};
use crate::app::{App, Message, xtb};
use iced::{Task, time::Instant};
use reshiki::{
    geometry::{AtomProblem, Error, ForceField, Optimized, Prepared},
    models3d::Model3d,
    qcdata::{Frame, XtbMethod},
    science,
};
use reshiki_xtb_worker::client::Cancel;

pub(super) struct Rough {
    atom: u64,
    symbol: String,
    method: XtbMethod,
}

impl Rough {
    pub(super) fn status(&self) -> String {
        let symbol = &self.symbol;
        let name = element_name(symbol);
        let method = self.method.label();
        format!("{symbol} ({name}) needs xTB: building a rough start, then refining with {method}…")
    }
}

pub(super) fn prepare(prepared: Prepared, rough: &mut Option<Rough>) -> Result<Prepared, Error> {
    match prepared.check_generation() {
        Ok(()) => Ok(prepared),
        Err(Error::Atom {
            atom,
            symbol,
            problem: AtomProblem::WrongShape { .. },
            ..
        }) if xtb::available() => {
            let numbers = atomic_numbers(&prepared).collect::<Vec<_>>();
            *rough = Some(Rough {
                atom,
                symbol,
                method: refine_method(&numbers),
            });
            Ok(prepared.rough())
        }
        Err(error) => Err(error),
    }
}

pub(super) fn needed(job: &Job, field: ForceField) -> bool {
    xtb::available()
        && (job.rough.is_some()
            || job.force_field == ForceField::Uff
            || field != job.force_field
            || job.strict_error.is_some()
            || !job.prepared.radicals().is_empty())
}

/// GFN2-xTB ends at Rn and keeps lanthanide f electrons in the core, so it
/// can reject the drawn spin. g-xTB covers H–Lr. From spread starts, we measured
/// tetrahedral ThCl₄ and octahedral UF₆ (U–F 1.97 Å) with g-xTB, and
/// square-planar XeF₄ and linear XeF₂ with GFN2-xTB. Generation already refuses
/// elements past Lr.
pub(super) fn refine_method(numbers: &[u8]) -> XtbMethod {
    if numbers.iter().any(|&z| (57..=71).contains(&z) || z > 86) {
        XtbMethod::GXtb
    } else {
        XtbMethod::Gfn2Xtb
    }
}

pub(in crate::app) fn unpaired_electrons(numbers: &[u8], charge: i32, unpaired: u32) -> u32 {
    let electrons = numbers.iter().map(|&z| i64::from(z)).sum::<i64>() - i64::from(charge);
    unpaired + u32::from((electrons - i64::from(unpaired)).rem_euclid(2) != 0)
}

/// A fixed per-coordinate offset in Å so an exactly symmetric start can leave a saddle.
pub(super) fn nudge(index: usize, axis: usize) -> f64 {
    let hash = (3 * index as u64 + axis as u64 + 1).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    let unit = f64::from((hash ^ (hash >> 32)) as u32) / f64::from(u32::MAX);
    (2. * unit - 1.) * 0.05
}

fn atomic_numbers(prepared: &Prepared) -> impl Iterator<Item = u8> + '_ {
    prepared
        .molecule()
        .state
        .graph
        .atoms
        .iter()
        .map(|atom| atom.atomic_number)
}

fn frame(doc: &reshiki::document::Document, prepared: &Prepared, model: &Model3d) -> Option<Frame> {
    let numbers: Vec<_> = atomic_numbers(prepared)
        .chain(std::iter::repeat_n(1, model.hydrogens.len()))
        .collect();
    let positions_angstrom = model
        .atoms
        .iter()
        .map(|atom| atom.xyz)
        .chain(model.hydrogens.iter().map(|hydrogen| hydrogen.xyz))
        .enumerate()
        .map(|(index, xyz)| {
            let mut xyz = xyz.map(f64::from);
            for (axis, coordinate) in xyz.iter_mut().enumerate() {
                *coordinate += nudge(index, axis);
            }
            xyz
        })
        .collect();
    let (charge, multiplicity) = science::drawn_state(doc, *prepared.ids().first()?);
    let unpaired = unpaired_electrons(&numbers, charge, multiplicity - 1);
    Some(Frame {
        numbers,
        positions_angstrom,
        isotopes: vec![],
        masses_amu: vec![],
        charge,
        multiplicity: unpaired + 1,
        drawing_atoms: prepared
            .ids()
            .iter()
            .copied()
            .map(Some)
            .chain(std::iter::repeat_n(None, model.hydrogens.len()))
            .collect(),
    })
}

pub(super) struct Refining {
    method: XtbMethod,
    model: Model3d,
    field: ForceField,
    frame: Frame,
    spread: Option<String>,
    /// Step index and its energy in Hartree, when the optimizer reports one.
    last_step: Option<(usize, f64)>,
    started: Instant,
    status: String,
    _worker: Worker,
}

struct Worker {
    cancel: Cancel,
    _handle: iced::task::Handle,
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

impl Refining {
    fn seconds(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }

    fn line(&self, prepared: &Prepared) -> String {
        let method = self.method.label();
        let step = self.last_step.map_or(0, |(step, _)| step);
        let seconds = self.seconds();
        let line = format!("Refining the 3D start with {method} · step {step} · {seconds:.1} s");
        radical_subject(prepared.radicals())
            .map(|who| format!("{who} · {line}"))
            .unwrap_or(line)
    }
}

pub(super) fn subtitle(job: &Job) -> Option<String> {
    if let Some(refining) = &job.refining {
        Some(refining.line(&job.prepared))
    } else {
        job.rough
            .as_ref()
            .map(|rough| format!("Building a rough start · then {}", rough.method.label()))
    }
}

impl App {
    pub(super) fn start_model_refinement(
        &mut self,
        mut model_job: Job,
        model: Model3d,
        optimized: &Optimized,
    ) -> Task<Message> {
        let Some(frame) = frame(&self.tab.doc, &model_job.prepared, &model) else {
            self.record_model_failure(vec![], Failure::other("No atoms to refine"));
            return self.xtb_model_finished(model_job.key, None);
        };
        let method = refine_method(&frame.numbers);
        let config = xtb::state::Config {
            task: xtb::state::Task::Optimize,
            method,
            charge: frame.charge,
            unpaired: frame.multiplicity - 1,
            solvent: None,
            xtb68: false,
            accuracy: None,
            temperature_k: None,
            max_iterations: None,
            threads: self.xtb.settings.threads(),
            tight: false,
            fixed: vec![],
        };
        let job = xtb::job(&config, &frame, vec![]);
        let cancel = Cancel::default();
        let worker_cancel = cancel.clone();
        let key = model_job.key;
        use iced::futures::{StreamExt, stream};
        let events = stream::once(async move { xtb::run::start(job, worker_cancel) }).flatten();
        let (task, handle) = Task::run(events, move |event| {
            Message::Models3d(super::Action::Refined(key, event))
        })
        .abortable();
        let mut refining = Refining {
            method,
            model,
            field: optimized.force_field,
            frame,
            spread: optimized.approximations.iter().find_map(|note| {
                note.strip_prefix("Rough start for an xTB refinement: ")
                    .map(str::to_owned)
            }),
            last_step: None,
            started: Instant::now(),
            status: String::new(),
            _worker: Worker {
                cancel,
                _handle: handle.abort_on_drop(),
            },
        };
        let line = refining.line(&model_job.prepared);
        refining.status = format!("{line} · Esc cancels");
        self.status.clone_from(&refining.status);
        self.error = false;
        self.xtb_model_stage(key, line);
        model_job.refining = Some(refining);
        self.tab.model_job = Some(model_job);
        task
    }

    pub(super) fn model_refined(&mut self, key: Key, event: xtb::RunEvent) -> Task<Message> {
        let current = self.model_key();
        // An edit made the refinement obsolete: stop its worker now.
        if self
            .tab
            .model_job
            .take_if(|job| job.key == key && job.current != current)
            .is_some()
        {
            return self.xtb_model_finished(key, Some("Drawing changed"));
        }
        let Some(job) = self.tab.model_job.as_mut().filter(|job| job.key == key) else {
            return Task::none();
        };
        let Some(refining) = &mut job.refining else {
            return Task::none();
        };
        match event {
            xtb::RunEvent::Done(result) => {
                let Some(mut job) = self.tab.model_job.take() else {
                    return Task::none();
                };
                if let Err(reason) = self.model_job_applies(&job) {
                    return self.xtb_model_finished(key, Some(reason));
                }
                let Some(refining) = job.refining.take() else {
                    return Task::none();
                };
                let applied = self.finish_model_refinement(&job, refining, result);
                return Task::batch([applied, self.xtb_model_finished(key, None)]);
            }
            xtb::RunEvent::Steps {
                first, energies, ..
            } => {
                if let Some(&energy) = energies.last() {
                    refining.last_step = Some((first + energies.len() - 1, energy));
                }
            }
            xtb::RunEvent::Hessian => return Task::none(),
            xtb::RunEvent::Tick => {}
        }
        let line = refining.line(&job.prepared);
        if self.status == refining.status {
            refining.status = format!("{line} · Esc cancels");
            self.status.clone_from(&refining.status);
            self.error = false;
        }
        self.xtb_model_stage(key, line);
        Task::none()
    }

    fn finish_model_refinement(
        &mut self,
        job: &Job,
        mut refining: Refining,
        result: Result<std::sync::Arc<xtb::run::Report>, reshiki_xtb_jobs::Failure>,
    ) -> Task<Message> {
        let result = result
            .map_err(|error| {
                let action = if job.rough.is_some() {
                    "stopped"
                } else {
                    "failed"
                };
                format!("{action}: {}", error.to_string().trim_end_matches('.'))
            })
            .and_then(|report| {
                let frame = &mut refining.frame;
                if report.positions.len() != frame.numbers.len() {
                    return Err("returned a different number of atoms".into());
                }
                frame.positions_angstrom.clone_from(&report.positions);
                let doc = &self.tab.doc;
                match science::stereo_changes(doc, &frame.drawing_atoms, frame) {
                    Ok(changes) if !changes.is_empty() => {
                        return Err("changed the stereochemistry".into());
                    }
                    Err(_) => {
                        let ids = job.prepared.ids();
                        if doc
                            .atoms
                            .iter()
                            .any(|atom| atom.stereo.is_some() && ids.contains(&atom.id))
                            || doc.bonds.iter().any(|bond| {
                                (bond.stereo.is_some()
                                    || bond.order == 1
                                        && !bond.projection
                                        && reshiki::bonds::is_stereo_marker(&bond.display))
                                    && ids.contains(&bond.a)
                                    && ids.contains(&bond.b)
                            })
                        {
                            return Err("could not check the stereochemistry".into());
                        }
                    }
                    Ok(_) => {}
                }
                // Each hydrogen must stay nearest the atom it belongs to.
                if !hydrogens_kept(&job.prepared, &refining.model, &report.positions) {
                    return Err("moved a hydrogen to another atom".into());
                }
                Ok(report)
            });
        let note = match result {
            Ok(report) => {
                let note = refined_note(&refining, &report);
                let model = &mut refining.model;
                let positions = model
                    .atoms
                    .iter_mut()
                    .map(|atom| &mut atom.xyz)
                    .chain(model.hydrogens.iter_mut().map(|hydrogen| &mut hydrogen.xyz));
                for (xyz, position) in positions.zip(&report.positions) {
                    *xyz = position.map(|coordinate| coordinate as f32);
                }
                note
            }
            Err(reason) => {
                if let Some(rough) = &job.rough {
                    let symbol = &rough.symbol;
                    let name = element_name(symbol);
                    let method = refining.method.label();
                    self.record_model_failure(job.prepared.ids().to_vec(), Failure {
                        message: format!("Can't build a starting 3D structure for {symbol} ({name}): {method} {reason}."),
                        atom: Some(rough.atom),
                    });
                    return Task::none();
                }
                format!(
                    "xTB refinement {reason}; showing the {} model",
                    refining.field
                )
            }
        };
        let notes = force_field_caution(job, refining.field)
            .into_iter()
            .chain([note]);
        let named = self.model_succeeded(job, refining.model, notes);
        if !self.error {
            self.xtb_model_spin(job.key, &refining.frame);
        }
        named
    }
}

/// Whether the drawn and generated hydrogens are still nearest their bonded atoms.
pub(super) fn hydrogens_kept(prepared: &Prepared, model: &Model3d, positions: &[[f64; 3]]) -> bool {
    let graph = &prepared.molecule().state.graph;
    let Some((drawn, generated)) = positions.split_at_checked(graph.atoms.len()) else {
        return false;
    };
    let distance =
        |a: &[f64; 3], b: &[f64; 3]| a.iter().zip(b).map(|(a, b)| (a - b).powi(2)).sum::<f64>();
    let nearest = |at: &[f64; 3], non_hydrogen: bool| {
        drawn
            .iter()
            .zip(&graph.atoms)
            .enumerate()
            .filter(|(_, (_, atom))| !non_hydrogen || atom.atomic_number != 1)
            .min_by(|(_, (a, _)), (_, (b, _))| distance(a, at).total_cmp(&distance(b, at)))
            .map(|(index, _)| index)
    };
    let generated_kept = model.hydrogens.iter().zip(generated).all(|(hydrogen, at)| {
        nearest(at, false).map(|index| index as u64) == Some(hydrogen.parent)
    });
    generated_kept
        && graph
            .atoms
            .iter()
            .zip(drawn)
            .enumerate()
            .all(|(index, (atom, at))| {
                if atom.atomic_number != 1 {
                    return true;
                }
                let mut neighbors = graph.bonds.iter().filter_map(|bond| {
                    if bond.a == index {
                        Some(bond.b)
                    } else if bond.b == index {
                        Some(bond.a)
                    } else {
                        None
                    }
                });
                match (neighbors.next(), neighbors.next()) {
                    // H₂ and bridging hydrogens have no single heavy parent.
                    (Some(parent), None)
                        if graph.atoms.get(parent).is_some_and(|a| a.atomic_number != 1) =>
                    {
                        nearest(at, true) == Some(parent)
                    }
                    _ => true,
                }
            })
        // No bond, drawn or to a generated hydrogen, may come apart: the
        // geometry worker refuses bonds over 4 Å too.
        && graph
            .bonds
            .iter()
            .map(|bond| (bond.a, bond.b))
            .chain(
                model
                    .hydrogens
                    .iter()
                    .enumerate()
                    .map(|(index, hydrogen)| (graph.atoms.len() + index, hydrogen.parent as usize)),
            )
            .all(|(a, b)| match (positions.get(a), positions.get(b)) {
                (Some(a), Some(b)) => distance(a, b) <= 16.,
                _ => false,
            })
}

fn refined_note(refining: &Refining, report: &xtb::run::Report) -> String {
    let method = refining.method.label();
    let steps = report.steps;
    let seconds = refining.seconds();
    let mut note = if report.converged {
        let step = if steps == 1 { "step" } else { "steps" };
        format!("Refined with {method} · {steps} {step}, {seconds:.1} s")
    } else {
        format!("Refined with {method} · stopped after {steps} steps, not converged")
    };
    if let Some(spread) = &refining.spread {
        note.push_str(&format!(" (start: {spread})"));
    }
    note
}
