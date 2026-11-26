//! A desktop transition-state search using thread-affine, isolated xTB workers.

use super::{Extras, Stage};
use crate::app::xtb::{
    results::{Evaluation, RunInfo},
    state::{Failure, symbol},
};
use iced::futures::{Stream, channel::mpsc};
use reshiki::{
    qcdata::{self, CalcResult, Frame},
    reactions::ts_plan::{Side, TsPlan},
    science::Launch,
    xtb_pool::Worker,
};
use reshiki_optimize::{
    ANGSTROM_PER_BOHR,
    ts::{self, TsReport},
};
use reshiki_xtb_worker::{
    client::{Cancel, Limits},
    protocol::{Computed, Conventions, Engine, Method, Open, Units},
};
use std::{collections::BTreeSet, ops::ControlFlow, sync::Arc, time::Instant};

#[derive(Clone, Debug)]
pub(crate) enum Event {
    Stage(Stage),
    Neb { iteration: usize },
    Saddle { step: usize },
    Irc { forward: bool, step: usize },
    Done(Result<Arc<Report>, Failure>),
}

#[derive(Clone, Debug)]
pub(crate) struct Report {
    /// TS first, followed by the optimized reactant and product.
    pub(super) frames: Vec<Frame>,
    pub(super) result: CalcResult,
    pub(super) extras: Extras,
    pub(super) info: RunInfo,
}

pub(super) fn start(
    launch: Launch,
    plan: TsPlan,
    images: usize,
    cancel: Cancel,
) -> impl Stream<Item = Event> + Send + 'static {
    let (tx, rx) = mpsc::unbounded();
    let events = tx.clone();
    let thread = std::thread::Builder::new()
        .name("xtb-ts-run".into())
        .spawn(move || {
            let result = run(&launch, &plan, images, &cancel, &events);
            let _ = events.unbounded_send(Event::Done(result.map(Arc::new)));
            events.close_channel();
        });
    if let Err(error) = thread {
        let _ = tx.unbounded_send(Event::Done(Err(Failure::Other(error.to_string()))));
        tx.close_channel();
    }
    rx
}

fn run(
    launch: &Launch,
    plan: &TsPlan,
    images: usize,
    cancel: &Cancel,
    tx: &mpsc::UnboundedSender<Event>,
) -> Result<Report, Failure> {
    let started = Instant::now();
    let mut trace = SearchTrace::default();
    trace.enter("Building endpoints");
    if !(5..=24).contains(&images) {
        return Err(Failure::Other(
            "A transition-state search needs 5 to 24 images".into(),
        ));
    }
    check_cancel(cancel)?;
    let changes = changes(launch, plan)?;
    let masses = engine_masses(&launch.frame)?;
    let limits = Limits::default();
    let executable = std::env::current_exe().map_err(|error| Failure::Other(error.to_string()))?;
    let open = Open {
        numbers: launch.frame.numbers.clone(),
        positions: launch.frame.positions_angstrom.clone(),
        units: Units::Angstrom,
        charge: launch.frame.charge,
        uhf: launch.frame.multiplicity.checked_sub(1),
        method: Method::Gfn2,
        conventions: Conventions::Axtb,
        solvent: None,
        scf: Default::default(),
        gfnff: None,
        threads: 1,
        heap_bytes: limits.heap_bytes,
    };
    let count = worker_count(
        images,
        std::thread::available_parallelism().map_or(1, usize::from),
    );
    let mut workers = Vec::with_capacity(count);
    let mut engine = None;
    for _ in 0..count {
        check_cancel(cancel)?;
        let (worker, hello, _) =
            Worker::start(executable.clone(), open.clone(), limits, cancel.clone())
                .map_err(failed)?;
        engine.get_or_insert(hello);
        workers.push(worker);
    }
    let first = workers
        .first_mut()
        .ok_or_else(|| Failure::Other("No xTB worker".into()))?;
    send(tx, cancel, Event::Stage(Stage::Endpoints))?;
    let (start, folded) = ts::prepare_reactive_start(
        first,
        &open.numbers,
        &bohr(&launch.frame.positions_angstrom),
        &changes.form,
        &changes.break_,
        |_, _| control(cancel, tx),
    )
    .map_err(failed)?;
    let (other, matched) = ts::build_endpoint(
        first,
        &open.numbers,
        &start,
        &changes.form,
        &changes.break_,
        |_, _| control(cancel, tx),
    )
    .map_err(failed)?;
    if !matched {
        let name = if plan.start == Side::Reactant {
            "product"
        } else {
            "reactant"
        };
        return Err(Failure::Other(format!(
            "The constructed {name} does not match the requested bond changes"
        )));
    }
    check_cancel(cancel)?;
    let (reactant, product) = match plan.start {
        Side::Reactant => (start, other.positions),
        Side::Product => (other.positions, start),
    };
    let options = ts::TsOptions {
        images,
        ..ts::TsOptions::default()
    };
    trace.reactive_start = folded.map(|fold| {
        fold.description(|i| {
            format!(
                "{}{}",
                open.numbers.get(i).copied().map_or("?", symbol),
                i + 1
            )
        })
    });
    let search = ts::Search {
        numbers: &open.numbers,
        masses_amu: &masses,
        reactant: &reactant,
        product: &product,
        options: &options,
    };
    let report = ts::search_with_cancel(
        &mut workers,
        &search,
        |event| {
            trace.observe(event);
            if let Some(event) = progress(event)
                && send(tx, cancel, event).is_err()
            {
                return ControlFlow::Break(());
            }
            control(cancel, tx)
        },
        || control(cancel, tx),
    )
    .map_err(failed)?;
    check_cancel(cancel)?;
    trace.enter("Worker check");
    let checked = workers
        .first_mut()
        .ok_or_else(|| Failure::Other("No xTB worker".into()))?
        .checked_hessian(&report.ts.positions, &masses)
        .map_err(ts::TsError::from);
    check_cancel(cancel)?;
    trace.finish();
    let evaluations = workers
        .iter()
        .flat_map(|worker| worker.evaluations.iter())
        .map(|e| Evaluation {
            energy: e.energy,
            scf_iterations: e.scf_iterations,
            seconds: e.seconds,
        })
        .collect();
    let engine = engine.ok_or_else(|| Failure::Other("No xTB engine".into()))?;
    complete(
        launch,
        Completion {
            report,
            checked,
            engine,
            changes: changes.in_reaction_direction(plan.start),
            evaluations,
            masses,
            seconds: started.elapsed().as_secs_f64(),
            trace,
        },
    )
}

fn worker_count(images: usize, parallelism: usize) -> usize {
    4.min(parallelism).min(images.saturating_sub(2))
}

fn engine_masses(frame: &Frame) -> Result<Vec<f64>, Failure> {
    use reshiki::chemistry::{ELEMENTS, ISOTOPES};

    if !frame.masses_amu.is_empty() {
        return Ok(frame.masses_amu.clone());
    }
    frame
        .numbers
        .iter()
        .zip(frame.isotopes.iter().copied().chain(std::iter::repeat(0)))
        .map(|(&z, isotope)| {
            if isotope != 0 {
                // Use the nuclide table and the mass-number fallback used for
                // labelled atoms in the drawing's molecular properties.
                let mass = ISOTOPES
                    .binary_search_by_key(&(z, isotope), |&(z, isotope, _)| (z, isotope))
                    .ok()
                    .and_then(|index| ISOTOPES.get(index))
                    .map_or(f64::from(isotope), |&(_, _, mass)| mass);
                return Ok(mass);
            }
            ELEMENTS
                .get(usize::from(z))
                .map(|element| element.average_mass_amu())
                .ok_or_else(|| Failure::Other(format!("Unknown element Z={z}")))
        })
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
struct Changes {
    form: Vec<(usize, usize)>,
    break_: Vec<(usize, usize)>,
}

impl Changes {
    fn in_reaction_direction(self, start: Side) -> Self {
        match start {
            Side::Reactant => self,
            Side::Product => Self {
                form: self.break_,
                break_: self.form,
            },
        }
    }
}

/// Resolve drawing IDs once, before any worker runs. Equivalent donor-bound
/// hydrogens keep the one with the shortest displacement toward the acceptor.
fn changes(launch: &Launch, plan: &TsPlan) -> Result<Changes, Failure> {
    let frame = &launch.frame;
    frame
        .validate()
        .map_err(|error| Failure::Other(error.to_string()))?;
    if frame.numbers.len() > ts::MAX_ATOMS {
        return Err(Failure::TooMany {
            count: frame.numbers.len(),
            limit: ts::MAX_ATOMS,
        });
    }
    if launch.atoms.len() != frame.numbers.len() {
        return Err(Failure::Other(
            "The 3D model's atom mapping is incomplete".into(),
        ));
    }
    let index = |id: u64| {
        launch
            .atoms
            .iter()
            .position(|&atom| atom == Some(id))
            .ok_or_else(|| {
                Failure::Other(format!("Reaction atom {id} is missing from the 3D model"))
            })
    };
    let pairs = |pairs: &[(u64, u64)]| -> Result<Vec<_>, Failure> {
        pairs
            .iter()
            .map(|&(a, b)| Ok(pair(index(a)?, index(b)?)))
            .collect()
    };
    let mut form = pairs(&plan.form)?;
    let mut break_ = pairs(&plan.break_)?;
    let bonds = ts::bonds(&frame.numbers, &bohr(&frame.positions_angstrom)).map_err(failed)?;
    // Mapped hydrogens have a prescribed identity even when their bonds stay
    // the same. Reserve them along with every H used in an explicit change.
    let mut used: BTreeSet<_> = form
        .iter()
        .chain(&break_)
        .flat_map(|&(a, b)| [a, b])
        .filter(|&i| frame.numbers.get(i) == Some(&1))
        .collect();
    for &id in &plan.mapped_hydrogens {
        used.insert(index(id)?);
    }
    for &(donor, acceptor) in &plan.hydrogen_moves {
        let (donor, acceptor) = (index(donor)?, index(acceptor)?);
        let target = frame
            .positions_angstrom
            .get(acceptor)
            .ok_or_else(|| Failure::Other("Invalid hydrogen acceptor".into()))?;
        let hydrogen = frame
            .numbers
            .iter()
            .enumerate()
            .filter(|&(h, &z)| z == 1 && !used.contains(&h) && bonds.contains(&pair(donor, h)))
            .filter_map(|(h, _)| {
                frame.positions_angstrom.get(h).map(|xyz| {
                    let distance: f64 = xyz.iter().zip(target).map(|(a, b)| (a - b).powi(2)).sum();
                    (h, distance)
                })
            })
            .min_by(|(a, da), (b, db)| da.total_cmp(db).then_with(|| a.cmp(b)))
            .map(|(h, _)| h)
            .ok_or_else(|| {
                Failure::Other(format!(
                    "Reaction atom {} has no available bonded hydrogen in the 3D model",
                    donor + 1
                ))
            })?;
        used.insert(hydrogen);
        break_.push(pair(donor, hydrogen));
        form.push(pair(acceptor, hydrogen));
    }
    form.sort_unstable();
    form.dedup();
    break_.sort_unstable();
    break_.dedup();
    Ok(Changes { form, break_ })
}

fn pair(a: usize, b: usize) -> (usize, usize) {
    (a.min(b), a.max(b))
}

struct Completion {
    report: TsReport,
    checked: Result<Computed, ts::TsError>,
    engine: Engine,
    changes: Changes,
    evaluations: Vec<Evaluation>,
    masses: Vec<f64>,
    seconds: f64,
    trace: SearchTrace,
}

/// Search counters and wall times are independent of the visible progress text.
#[derive(Default)]
struct SearchTrace {
    reactive_start: Option<String>,
    neb_iterations: Option<usize>,
    saddle_steps: Option<usize>,
    current: Option<(&'static str, Instant)>,
    times: Vec<(String, f64)>,
}

impl SearchTrace {
    fn enter(&mut self, stage: &'static str) {
        if self.current.is_some_and(|(current, _)| current == stage) {
            return;
        }
        self.finish();
        self.current = Some((stage, Instant::now()));
    }

    fn finish(&mut self) {
        if let Some((stage, started)) = self.current.take() {
            self.times
                .push((stage.into(), started.elapsed().as_secs_f64()));
        }
    }

    fn observe(&mut self, event: ts::Event<'_>) {
        match event {
            ts::Event::Stage(stage) => self.enter(match stage {
                ts::Stage::Endpoints => "Building endpoints",
                ts::Stage::Interpolation => "IDPP interpolation",
                ts::Stage::Neb => "CI-NEB",
                ts::Stage::Saddle => "P-RFO refinement",
                ts::Stage::Frequencies => "Frequency analysis",
                ts::Stage::Irc => "IRC verification",
            }),
            ts::Event::Neb { iteration, .. } => self.neb_iterations = Some(iteration),
            ts::Event::Saddle { step, .. } => self.saddle_steps = Some(step),
            _ => {}
        }
    }
}

fn complete(launch: &Launch, done: Completion) -> Result<Report, Failure> {
    let Completion {
        report,
        checked,
        engine,
        changes,
        evaluations,
        masses,
        seconds,
        trace,
    } = done;
    let frame = |positions: &[f64]| Frame {
        positions_angstrom: angstroms(positions),
        masses_amu: masses.clone(),
        ..(*launch.frame).clone()
    };
    let transition = frame(&report.ts.positions);
    let reactant = frame(&report.reactant.positions);
    let product = frame(&report.product.positions);
    let checked = checked.and_then(|computed| {
        let frequencies = computed
            .vibrations
            .as_ref()
            .map(|v| v.frequencies_cm1.clone())
            .ok_or_else(|| {
                ts::TsError::Failed("The xTB worker returned no vibrational analysis".into())
            })?;
        let result = computed
            .to_calc_result(&transition, qcdata::Task::Hessian, &engine, None)
            .map_err(|error| ts::TsError::Failed(error.to_string()))?;
        Ok((result, frequencies, computed))
    });
    let mut info = RunInfo {
        evaluations,
        ..RunInfo::default()
    };
    let (mut result, worker_frequencies, reason) = match checked {
        Ok((result, frequencies, computed)) => {
            info.gap_ev = computed.homo_lumo_gap_ev;
            info.near_degenerate = computed.near_degenerate;
            info.numerics = computed.numerics;
            let reason = failure_reason(&report, &frequencies);
            (result, frequencies, reason)
        }
        Err(ts::TsError::Cancelled) => return Err(failed(ts::TsError::Cancelled)),
        Err(error) => (
            engine_candidate(&transition, &report, &engine),
            Vec::new(),
            Some(format!("worker check failed: {error}")),
        ),
    };
    result.task = qcdata::Task::TransitionState;
    // The search's refined saddle is the energy used by every barrier display.
    result.energy_hartree = Some(report.ts.energy);
    result.input_frame = Some(launch.frame.hash());
    result.endpoints = Some(qcdata::Endpoints {
        reactant: reactant.hash(),
        product: product.hash(),
    });
    result.status = match reason {
        Some(reason) => qcdata::Status::NotConverged { reason },
        None => qcdata::Status::Converged,
    };
    result.trajectory = Some(qcdata::Trajectory {
        numbers: transition.numbers.clone(),
        isotopes: transition.isotopes.clone(),
        masses_amu: transition.masses_amu.clone(),
        charge: transition.charge,
        multiplicity: transition.multiplicity,
        positions_angstrom: Vec::new(),
        energies_hartree: report.path.iter().map(|point| point.energy).collect(),
        labels: Vec::new(),
        minimizer_energies_hartree: Vec::new(),
        cluster_sizes: Vec::new(),
    });
    result.run = Some(qcdata::RunSummary {
        seconds,
        scf_iterations: info
            .evaluations
            .iter()
            .map(|e| e.scf_iterations as u32)
            .collect(),
        finished: Some(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)),
        transition_state: None,
    });
    result
        .validate(&transition)
        .map_err(|error| Failure::Other(error.to_string()))?;
    let extras = Extras {
        saddle_converged: report.saddle_converged,
        num_imaginary: report.imaginary,
        diagnostics: diagnostics(&report, &worker_frequencies, &result, &trace),
        image_positions: report
            .path
            .iter()
            .map(|point| angstroms(&point.positions))
            .collect(),
        irc_energies: report.irc.as_ref().map_or_else(
            || [Vec::new(), Vec::new()],
            |branches| branches.each_ref().map(|branch| branch.energies.clone()),
        ),
        connects: report.connects,
        forming: changes.form.iter().map(|&(a, b)| [a, b]).collect(),
        breaking: changes.break_.iter().map(|&(a, b)| [a, b]).collect(),
        worker_frequencies,
        climbing: report.climbing_image,
        engine_imaginary: report
            .frequencies_cm1
            .first()
            .copied()
            .filter(|&f| f < -ts::IMAGINARY_CM1),
    };
    Ok(Report {
        frames: vec![transition, reactant, product],
        result,
        extras,
        info,
    })
}

fn diagnostics(
    report: &TsReport,
    worker_frequencies: &[f64],
    result: &CalcResult,
    trace: &SearchTrace,
) -> Vec<(String, String)> {
    let convergence = |converged| {
        if converged {
            "converged"
        } else {
            "not converged"
        }
    };
    let mut rows = vec![
        ("Method".into(), "IDPP → CI-NEB → P-RFO → IRC".into()),
        (
            "Images".into(),
            format!(
                "{} · climbing image {}",
                report.path.len(),
                report.climbing_image + 1
            ),
        ),
        (
            "NEB".into(),
            format!(
                "{} · {}",
                trace.neb_iterations.map_or_else(
                    || "iterations unavailable".into(),
                    |n| format!("{n} iterations")
                ),
                convergence(report.neb_converged)
            ),
        ),
        (
            "Saddle".into(),
            format!(
                "{} · {} · {}",
                trace
                    .saddle_steps
                    .map_or_else(|| "steps unavailable".into(), |n| format!("{n} steps")),
                convergence(report.saddle_converged),
                report.saddle_reason
            ),
        ),
    ];
    if let Some(detail) = &trace.reactive_start {
        rows.push(("Folded the starting conformer".into(), detail.clone()));
    }
    let worker = worker_frequencies
        .iter()
        .filter(|&&f| f < -ts::IMAGINARY_CM1)
        .count();
    rows.push((
        "Worker check".into(),
        if worker_frequencies.is_empty() {
            match &result.status {
                qcdata::Status::NotConverged { reason } | qcdata::Status::Failed { reason }
                    if reason.starts_with("worker check failed: ") =>
                {
                    reason.clone()
                }
                _ => "frequency check unavailable".into(),
            }
        } else {
            format!(
                "engine {} imaginary mode(s); worker {worker} · {}",
                report.imaginary,
                if report.imaginary == worker {
                    "agree"
                } else {
                    "disagree"
                }
            )
        },
    ));
    if let Some(branches) = &report.irc {
        for (label, branch) in ["IRC +mode", "IRC −mode"].into_iter().zip(branches) {
            let endpoint = match branch.reached {
                ts::Reached::Reactant => "reactant",
                ts::Reached::Product => "product",
                ts::Reached::Other => "different minimum",
            };
            rows.push((
                label.into(),
                format!(
                    "{endpoint} · downhill trace {} · {} points",
                    convergence(branch.completed),
                    branch.path.len()
                ),
            ));
        }
    } else {
        rows.push(("IRC".into(), "not completed".into()));
    }
    if let Some(error) = &report.irc_error {
        rows.push(("IRC error".into(), error.clone()));
    }
    rows.push(("Evaluations".into(), report.evaluations.to_string()));
    rows.push(("Hessians".into(), report.hessians.to_string()));
    rows.extend(
        trace
            .times
            .iter()
            .map(|(stage, seconds)| (format!("Time · {stage}"), format!("{seconds:.2} s"))),
    );
    rows.extend(
        report
            .saddle_diagnostics
            .iter()
            .enumerate()
            .map(|(index, diagnostic)| {
                (
                    format!("Saddle diagnostic {}", index + 1),
                    diagnostic.clone(),
                )
            }),
    );
    rows
}

fn engine_candidate(frame: &Frame, report: &TsReport, engine: &Engine) -> CalcResult {
    let method = Method::Gfn2.xtb_method();
    CalcResult {
        frame: frame.hash(),
        input_frame: None,
        task: qcdata::Task::TransitionState,
        endpoints: None,
        settings: qcdata::Settings::Xtb(qcdata::XtbSettings {
            method,
            conventions: Conventions::Axtb,
            solvent: None,
            electronic_temperature_k: None,
            accuracy: None,
            max_iterations: None,
            scf_retries: Vec::new(),
            gfnff: None,
            threads: Some(1),
            convergence: None,
            fixed_atoms: Vec::new(),
        }),
        provenance: qcdata::Provenance {
            program: "axtb".into(),
            program_version: Some(engine.version.clone()),
            method: Some(method.label().into()),
            basis: None,
            source: qcdata::Source::Calculated {
                engine_revision: engine.rev.clone(),
            },
        },
        status: qcdata::Status::Converged,
        energy_hartree: Some(report.ts.energy),
        energy_terms: Default::default(),
        gradient_hartree_per_bohr: Vec::new(),
        hessian_hartree_per_bohr2: report.hessian.clone(),
        charges: Vec::new(),
        dipole_e_bohr: None,
        bond_orders: Vec::new(),
        orbitals: None,
        vibrations: Some(qcdata::Vibrations {
            frequencies_cm1: report.frequencies_cm1.clone(),
            ir_intensities_km_mol: Vec::new(),
            modes: Vec::new(),
            reduced_masses_amu: Vec::new(),
            masses_amu: frame.masses_amu.clone(),
        }),
        thermo: None,
        trajectory: None,
        keep_full: false,
        omitted_bytes: None,
        run: None,
    }
}

/// Keep the same acceptance rule and diagnostic order as the CLI.
fn failure_reason(report: &TsReport, worker_frequencies: &[f64]) -> Option<String> {
    use ts::Reached::{Other, Product, Reactant};

    let worker = worker_frequencies
        .iter()
        .filter(|&&f| f < -ts::IMAGINARY_CM1)
        .count();
    if worker != report.imaginary {
        Some(format!(
            "engine/worker imaginary frequency counts disagree: engine {}, worker {worker}",
            report.imaginary
        ))
    } else if let Some(reason) = &report.irc_error {
        Some(format!("IRC failed: {reason}"))
    } else if !report.saddle_converged {
        Some("saddle refinement did not converge".into())
    } else if report.imaginary != 1 {
        Some(format!(
            "expected one imaginary frequency below -{} cm-1, found {}",
            ts::IMAGINARY_CM1,
            report.imaginary
        ))
    } else if !report.connects {
        Some(
            match report
                .irc
                .as_ref()
                .map(|branches| branches.each_ref().map(|b| b.reached))
            {
                Some([Reactant, Reactant]) => "IRC reaches the reactant on both sides",
                Some([Product, Product]) => "IRC reaches the product on both sides",
                Some([Reactant, Other] | [Other, Reactant]) => {
                    "IRC reaches the reactant and a different minimum"
                }
                Some([Product, Other] | [Other, Product]) => {
                    "IRC reaches the product and a different minimum"
                }
                Some([Other, Other]) => "IRC reaches neither the reactant nor the product",
                _ => "IRC did not connect the reactant and product",
            }
            .into(),
        )
    } else {
        None
    }
}

fn progress(event: ts::Event<'_>) -> Option<Event> {
    Some(match event {
        ts::Event::Stage(stage) => Event::Stage(match stage {
            ts::Stage::Endpoints => Stage::Endpoints,
            ts::Stage::Interpolation => Stage::Idpp,
            ts::Stage::Neb => Stage::Neb,
            ts::Stage::Saddle | ts::Stage::Frequencies => Stage::Saddle,
            ts::Stage::Irc => Stage::Irc,
        }),
        ts::Event::Neb { iteration, .. } => Event::Neb { iteration },
        ts::Event::Saddle { step, .. } => Event::Saddle { step },
        ts::Event::Irc { forward, step, .. } => Event::Irc { forward, step },
        ts::Event::Endpoint { .. } | ts::Event::SaddleTrial { .. } | ts::Event::SaddleStatus(_) => {
            return None;
        }
    })
}

fn bohr(positions: &[[f64; 3]]) -> Vec<f64> {
    positions
        .as_flattened()
        .iter()
        .map(|x| x / ANGSTROM_PER_BOHR)
        .collect()
}

fn angstroms(positions: &[f64]) -> Vec<[f64; 3]> {
    positions
        .as_chunks::<3>()
        .0
        .iter()
        .map(|xyz| xyz.map(|x| x * ANGSTROM_PER_BOHR))
        .collect()
}

fn check_cancel(cancel: &Cancel) -> Result<(), Failure> {
    if cancel.is_cancelled() {
        Err(Failure::Other("Calculation canceled".into()))
    } else {
        Ok(())
    }
}

fn control(cancel: &Cancel, tx: &mpsc::UnboundedSender<Event>) -> ControlFlow<()> {
    if cancel.is_cancelled() || tx.is_closed() {
        cancel.cancel();
        ControlFlow::Break(())
    } else {
        ControlFlow::Continue(())
    }
}

fn send(tx: &mpsc::UnboundedSender<Event>, cancel: &Cancel, event: Event) -> Result<(), Failure> {
    check_cancel(cancel)?;
    tx.unbounded_send(event).map_err(|_| {
        cancel.cancel();
        Failure::Other("Calculation canceled".into())
    })
}

fn failed(error: impl Into<ts::TsError>) -> Failure {
    match error.into() {
        ts::TsError::Scf(message) => Failure::Scf(message),
        ts::TsError::Cancelled => Failure::Other("Calculation canceled".into()),
        error => Failure::Other(error.to_string()),
    }
}

#[cfg(test)]
mod tests;
