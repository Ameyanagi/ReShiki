//! Refine starting geometries whose force-field shapes need an electronic method.
use super::{Failure, Job, Key, element_name, force_field_caution, radical_subject};
use crate::app::{App, Message, xtb};
use iced::{Task, time::Instant};
use reshiki::{
    geometry::{AtomProblem, Error, ForceField, Prepared},
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

pub(super) fn unpaired_electrons(numbers: &[u8], charge: i32, unpaired: u32) -> u32 {
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
        drawing_atoms: vec![],
    })
}

pub(super) struct Refining {
    method: XtbMethod,
    model: Model3d,
    field: ForceField,
    /// Step index and its energy in Hartree, when the optimizer reports one.
    last_step: Option<(usize, f64)>,
    started: Instant,
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
        field: ForceField,
    ) -> Task<Message> {
        let Some(frame) = frame(&self.tab.doc, &model_job.prepared, &model) else {
            self.record_model_failure(vec![], Failure::other("No atoms to refine"));
            return Task::none();
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
        let refining = Refining {
            method,
            model,
            field,
            last_step: None,
            started: Instant::now(),
            _worker: Worker {
                cancel,
                _handle: handle.abort_on_drop(),
            },
        };
        self.status = format!("{} · Esc cancels", refining.line(&model_job.prepared));
        self.error = false;
        model_job.refining = Some(refining);
        self.tab.model_job = Some(model_job);
        task
    }

    pub(super) fn model_refined(&mut self, key: Key, event: xtb::RunEvent) -> Task<Message> {
        let current = self.model_key();
        let Some(job) = self.tab.model_job.as_mut().filter(|job| job.key == key) else {
            return Task::none();
        };
        let Some(refining) = &mut job.refining else {
            return Task::none();
        };
        match event {
            xtb::RunEvent::Done(result) => {
                let applied = if let Some(mut job) = self.tab.model_job.take()
                    && let Some(refining) = job.refining.take()
                    && self.model_job_applies(&job)
                {
                    self.finish_model_refinement(&job, refining, result)
                } else {
                    Task::none()
                };
                return Task::batch([applied, self.xtb_model_finished(key, false)]);
            }
            xtb::RunEvent::Steps {
                first, energies, ..
            } => {
                if let Some(&energy) = energies.last() {
                    refining.last_step = Some((first + energies.len() - 1, energy));
                }
            }
            xtb::RunEvent::Hessian | xtb::RunEvent::Tick => {}
        }
        if job.current == current {
            self.status = format!("{} · Esc cancels", refining.line(&job.prepared));
            self.error = false;
        }
        Task::none()
    }

    fn finish_model_refinement(
        &mut self,
        job: &Job,
        mut refining: Refining,
        result: Result<std::sync::Arc<xtb::run::Report>, reshiki_xtb_jobs::Failure>,
    ) -> Task<Message> {
        match result {
            Ok(report) => {
                let note = refined_note(job, &refining, &report);
                let model = &mut refining.model;
                let positions = model
                    .atoms
                    .iter_mut()
                    .map(|atom| &mut atom.xyz)
                    .chain(model.hydrogens.iter_mut().map(|hydrogen| &mut hydrogen.xyz));
                for (xyz, position) in positions.zip(&report.positions) {
                    *xyz = position.map(|coordinate| coordinate as f32);
                }
                self.model_succeeded(job, refining.model, Some(note))
            }
            Err(error) => {
                let error = error.to_string();
                let reason = error.trim_end_matches('.');
                if let Some(rough) = &job.rough {
                    let symbol = &rough.symbol;
                    let name = element_name(symbol);
                    let method = refining.method.label();
                    self.record_model_failure(job.prepared.ids().to_vec(), Failure {
                        message: format!("Can't build a starting 3D structure for {symbol} ({name}): {method} stopped: {reason}."),
                        atom: Some(rough.atom),
                    });
                    Task::none()
                } else {
                    let field = refining.field;
                    let failed =
                        format!("xTB refinement failed: {reason}; showing the {field} model");
                    let note = force_field_caution(job, field)
                        .map(|caution| format!("{caution} · {failed}"))
                        .unwrap_or(failed);
                    self.model_succeeded(job, refining.model, Some(note))
                }
            }
        }
    }
}

fn refined_note(job: &Job, refining: &Refining, report: &xtb::run::Report) -> String {
    let method = refining.method.label();
    let steps = report.steps;
    let seconds = refining.seconds();
    let mut note = if report.converged {
        let step = if steps == 1 { "step" } else { "steps" };
        format!("Refined with {method} · {steps} {step}, {seconds:.1} s")
    } else {
        format!("Refined with {method} · stopped after {steps} steps, not converged")
    };
    if let Some(rough) = &job.rough {
        note.push_str(&format!(" (start: {} ligands spread out)", rough.symbol));
    } else if job.strict_error.is_some() {
        note.push_str(" (start: approximate UFF model)");
    } else if refining.field != job.force_field {
        note.push_str(" (MMFF has no parameters)");
    }
    note
}
