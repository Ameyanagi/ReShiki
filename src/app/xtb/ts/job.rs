//! A desktop transition-state search using thread-affine, isolated xTB workers.

use super::view::THERMO_IMAGINARY_CM1;
use super::{Extras, Retry, Stage};
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
    protocol::{Computed, Conventions, Engine, Open, Units},
};
use std::{
    collections::BTreeSet,
    ops::ControlFlow,
    sync::Arc,
    time::{Duration, Instant},
};

const THERMO_TEMPERATURE_K: f64 = 298.15;

#[derive(Clone, Debug)]
pub(crate) enum Event {
    Stage(Stage),
    IrcSkipped,
    Neb {
        iteration: usize,
        energies: Vec<f64>,
    },
    Saddle {
        step: usize,
    },
    Irc {
        forward: bool,
        step: usize,
    },
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

pub(super) enum SearchPath {
    New(usize),
    Finer(usize),
    Saved {
        points: Vec<ts::Minimum>,
        converged: bool,
        candidate: ts::SaddleCandidate,
    },
    /// Only source-method coordinates cross into a target-method refinement.
    /// All three geometries use bohr and the original launch's atom order.
    Refine {
        parent: qcdata::TransitionStateId,
        reactant: Vec<f64>,
        product: Vec<f64>,
        saddle: Vec<f64>,
    },
}

pub(super) fn start(
    launch: Launch,
    request: super::Request,
    drawn_bonds: Vec<(u64, u64)>,
    path: SearchPath,
    cancel: Cancel,
) -> impl Stream<Item = Event> + Send + 'static {
    let (tx, rx) = mpsc::unbounded();
    let events = tx.clone();
    let thread = std::thread::Builder::new()
        .name("xtb-ts-run".into())
        .spawn(move || {
            let result = run(&launch, &request, &drawn_bonds, path, &cancel, &events);
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
    request: &super::Request,
    drawn_bonds: &[(u64, u64)],
    path: SearchPath,
    cancel: &Cancel,
    tx: &mpsc::UnboundedSender<Event>,
) -> Result<Report, Failure> {
    let started = Instant::now();
    let plan = &request.plan;
    let method = request.method;
    if !matches!(method, qcdata::XtbMethod::Gfn2Xtb | qcdata::XtbMethod::GXtb) {
        return Err(Failure::Other(
            "Transition-state calculations require GFN2-xTB or g-xTB".into(),
        ));
    }
    let images = match &path {
        SearchPath::New(images) | SearchPath::Finer(images) => *images,
        SearchPath::Saved { points, .. } => points.len(),
        SearchPath::Refine { .. } => ts::TsOptions::default().images,
    };
    let retry = match &path {
        SearchPath::New(_) => None,
        SearchPath::Finer(_) => Some(Retry::FinerPath),
        SearchPath::Saved { .. } => Some(Retry::RefineHighest),
        SearchPath::Refine { parent, .. } => Some(Retry::Gxtb(*parent)),
    };
    let mut trace = SearchTrace {
        retry,
        ..Default::default()
    };
    let refining = matches!(retry, Some(Retry::RefineHighest | Retry::Gxtb(_)));
    trace.enter(if refining {
        "Preparing refinement"
    } else {
        "Building endpoints"
    });
    if !(5..=24).contains(&images) {
        return Err(Failure::Other(
            "A transition-state search needs 5 to 24 images".into(),
        ));
    }
    check_cancel(cancel)?;
    let changes = changes(launch, plan)?;
    check_drawn_bonds(launch, drawn_bonds)?;
    let masses = engine_masses(&launch.frame)?;
    let limits = Limits::default();
    let executable = std::env::current_exe().map_err(|error| Failure::Other(error.to_string()))?;
    let open = worker_open(&launch.frame, method, limits);
    let count = if refining {
        1
    } else {
        worker_count(
            images,
            std::thread::available_parallelism().map_or(1, usize::from),
        )
    };
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
    let (reactant, product) = match &path {
        SearchPath::Saved { points, .. } => {
            let [reactant, .., product] = points.as_slice() else {
                return Err(Failure::Other("The saved path needs both endpoints".into()));
            };
            (reactant.positions.clone(), product.positions.clone())
        }
        SearchPath::Refine {
            reactant, product, ..
        } => (reactant.clone(), product.clone()),
        _ => endpoints(launch, plan.start, &changes, first, cancel, tx, &mut trace)?,
    };
    let options = ts::TsOptions {
        images,
        preserve_endpoint_identity: method == qcdata::XtbMethod::GXtb,
        ..ts::TsOptions::default()
    };
    let search = ts::Search {
        numbers: &open.numbers,
        masses_amu: &masses,
        reactant: &reactant,
        product: &product,
        options: &options,
    };
    let mut last_band = None;
    let trace = std::cell::RefCell::new(trace);
    let observe = |event: ts::Event<'_>| {
        trace.borrow_mut().observe(event);
        if let Some(event) = progress(event, &mut last_band, Instant::now)
            && send(tx, cancel, event).is_err()
        {
            return ControlFlow::Break(());
        }
        control(cancel, tx)
    };
    let verify = |worker: &mut Worker, positions: &[f64], imaginary: usize| {
        send(tx, cancel, Event::Stage(Stage::Saddle)).map_err(|_| ts::TsError::Cancelled)?;
        trace.borrow_mut().enter("Worker check");
        let check = worker
            .checked_hessian(positions, &masses)
            .map_err(ts::TsError::from)
            .and_then(|computed| checked_method(computed, method));
        if control(cancel, tx).is_break() || matches!(check, Err(ts::TsError::Cancelled)) {
            return Err(ts::TsError::Cancelled);
        }
        let agrees = check
            .as_ref()
            .ok()
            .and_then(|computed| computed.vibrations.as_ref())
            .is_some_and(|v| imaginary_count(&v.frequencies_cm1) == imaginary);
        Ok((agrees, check))
    };
    let (report, checked) = match path {
        SearchPath::Saved {
            points,
            converged,
            candidate,
        } => ts::refine_from_candidate(
            first, &search, &points, converged, &candidate, observe, verify,
        ),
        SearchPath::Refine { saddle, .. } => {
            ts::refine_with_verification(first, &search, &saddle, observe, verify)
        }
        _ => ts::search_with_verification(
            &mut workers,
            &search,
            observe,
            || control(cancel, tx),
            verify,
        ),
    }
    .map_err(failed)?;
    check_cancel(cancel)?;
    if report.irc.is_none() && report.irc_error.is_none() {
        send(tx, cancel, Event::IrcSkipped)?;
    }
    let mut trace = trace.into_inner();
    let endpoint_thermo = if request.free_energy {
        trace.enter("Endpoint thermochemistry");
        send(tx, cancel, Event::Stage(Stage::Thermochemistry))?;
        let worker = workers
            .first_mut()
            .ok_or_else(|| Failure::Other("No xTB worker".into()))?;
        let mut analysis =
            endpoint_thermochemistry(&launch.frame, &report, &masses, |positions, masses| {
                if control(cancel, tx).is_break() {
                    return Err(ts::TsError::Cancelled);
                }
                let computed = worker.checked_hessian(positions, masses);
                if control(cancel, tx).is_break() {
                    return Err(ts::TsError::Cancelled);
                }
                computed
                    .map_err(ts::TsError::from)
                    .and_then(|computed| checked_method(computed, method))
            })
            .map_err(failed)?;
        if plan.shape != (1, 1) {
            analysis.diagnostics.push((
                "Endpoint convention".into(),
                "Free energies use the optimized endpoint complexes.".into(),
            ));
        }
        Some(analysis)
    } else {
        None
    };
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
            endpoint_thermo,
            method,
            engine,
            changes: changes.in_reaction_direction(plan.start),
            evaluations,
            masses,
            seconds: started.elapsed().as_secs_f64(),
            trace,
        },
    )
}

/// An independent frequency check must describe the same Hamiltonian as the
/// saddle and IRC. Treat a mismatched response as a failed check, never relabel it.
fn checked_method(computed: Computed, method: qcdata::XtbMethod) -> Result<Computed, ts::TsError> {
    if computed.method.xtb_method() != method {
        return Err(ts::TsError::Failed(format!(
            "the xTB worker returned {} for a {} calculation",
            computed.method.xtb_method().label(),
            method.label()
        )));
    }
    Ok(computed)
}

fn endpoints(
    launch: &Launch,
    side: Side,
    changes: &Changes,
    worker: &mut Worker,
    cancel: &Cancel,
    tx: &mpsc::UnboundedSender<Event>,
    trace: &mut SearchTrace,
) -> Result<(Vec<f64>, Vec<f64>), Failure> {
    let numbers = &launch.frame.numbers;
    let (start, folded) = ts::prepare_reactive_start(
        worker,
        numbers,
        &bohr(&launch.frame.positions_angstrom),
        &changes.form,
        &changes.break_,
        |_, _| control(cancel, tx),
    )
    .map_err(failed)?;
    let (other, matched) = ts::build_endpoint(
        worker,
        numbers,
        &start,
        &changes.form,
        &changes.break_,
        |_, _| control(cancel, tx),
    )
    .map_err(failed)?;
    if !matched {
        let name = if side == Side::Reactant {
            "product"
        } else {
            "reactant"
        };
        return Err(Failure::Other(format!(
            "The constructed {name} does not match the requested bond changes"
        )));
    }
    check_cancel(cancel)?;
    let (reactant, product) = match side {
        Side::Reactant => (start, other.positions),
        Side::Product => (other.positions, start),
    };
    trace.reactive_start = folded.map(|fold| {
        let distances = fold.distance_description(|i| {
            format!("{}{}", numbers.get(i).copied().map_or("?", symbol), i + 1)
        });
        format!("{distances} ({:+} kcal/mol)", fold.energy_change_kcal)
    });
    Ok((reactant, product))
}

/// Every potential in the pool retains this method for the lifetime of the search.
pub(super) fn worker_open(frame: &Frame, method: qcdata::XtbMethod, limits: Limits) -> Open {
    Open {
        numbers: frame.numbers.clone(),
        positions: frame.positions_angstrom.clone(),
        units: Units::Angstrom,
        charge: frame.charge,
        uhf: frame.multiplicity.checked_sub(1),
        method: reshiki_xtb_jobs::run::worker_method(method),
        conventions: Conventions::Axtb,
        solvent: None,
        scf: Default::default(),
        gfnff: None,
        threads: 1,
        heap_bytes: limits.heap_bytes,
    }
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

/// The start geometry must still have the drawn bonds: an earlier
/// optimization can open a strained ring while the drawing stays the same,
/// and the search would then follow a different reaction.
fn check_drawn_bonds(launch: &Launch, drawn: &[(u64, u64)]) -> Result<(), Failure> {
    let index = |id: u64| launch.atoms.iter().position(|&atom| atom == Some(id));
    let want: BTreeSet<_> = drawn
        .iter()
        .filter_map(|&(a, b)| Some(pair(index(a)?, index(b)?)))
        .collect();
    let frame = &launch.frame;
    let have: BTreeSet<_> = ts::bonds(&frame.numbers, &bohr(&frame.positions_angstrom))
        .map_err(failed)?
        .into_iter()
        .filter(|&(a, b)| {
            [a, b]
                .iter()
                .all(|&i| matches!(launch.atoms.get(i), Some(Some(_))))
        })
        .map(|(a, b)| pair(a, b))
        .collect();
    if want == have {
        Ok(())
    } else {
        Err(Failure::Other(
            "The 3D model's bonds no longer match the drawing. Rebuild the 3D model, then run again."
                .into(),
        ))
    }
}

fn pair(a: usize, b: usize) -> (usize, usize) {
    (a.min(b), a.max(b))
}

struct Completion {
    report: TsReport,
    checked: Result<Computed, ts::TsError>,
    endpoint_thermo: Option<EndpointThermo>,
    method: qcdata::XtbMethod,
    engine: Engine,
    changes: Changes,
    evaluations: Vec<Evaluation>,
    masses: Vec<f64>,
    seconds: f64,
    trace: SearchTrace,
}

struct EndpointThermo {
    corrections: [Option<qcdata::Thermo>; 2],
    diagnostics: Vec<(String, String)>,
}

/// Analyze the exact optimized search endpoints. Electronic energies returned
/// with these Hessians never replace the energies used by the search.
fn endpoint_thermochemistry(
    frame: &Frame,
    report: &TsReport,
    masses: &[f64],
    mut analyze: impl FnMut(&[f64], &[f64]) -> Result<Computed, ts::TsError>,
) -> Result<EndpointThermo, ts::TsError> {
    let mut corrections = [None, None];
    let mut diagnostics = Vec::with_capacity(2);
    for ((name, endpoint), slot) in [("Reactant", &report.reactant), ("Product", &report.product)]
        .into_iter()
        .zip(&mut corrections)
    {
        let correction = analyze(&endpoint.positions, masses).and_then(|computed| {
            let vibrations = computed.vibrations.ok_or_else(|| {
                ts::TsError::Failed("The xTB worker returned no vibrational analysis".into())
            })?;
            if vibrations
                .frequencies_cm1
                .iter()
                .any(|&frequency| frequency < -THERMO_IMAGINARY_CM1)
            {
                return Err(ts::TsError::Failed(format!(
                    "the optimized endpoint has an imaginary frequency below -{THERMO_IMAGINARY_CM1} cm-1"
                )));
            }
            Ok(thermochemistry(
                frame,
                &angstroms(&endpoint.positions),
                masses,
                &vibrations.frequencies_cm1,
            ))
        });
        match correction {
            Ok(correction) => {
                *slot = Some(correction);
                diagnostics.push((
                    format!("{name} thermochemistry"),
                    "298.15 K · 101325 Pa · quasi-RRHO".into(),
                ));
            }
            Err(ts::TsError::Cancelled) => return Err(ts::TsError::Cancelled),
            Err(error) => diagnostics.push((
                format!("{name} thermochemistry"),
                format!("Free energy unavailable: {error}"),
            )),
        }
    }
    Ok(EndpointThermo {
        corrections,
        diagnostics,
    })
}

fn thermochemistry(
    frame: &Frame,
    positions_angstrom: &[[f64; 3]],
    masses_amu: &[f64],
    frequencies_cm1: &[f64],
) -> qcdata::Thermo {
    use qcdata::vibrational::{ThermoInput, rotational_symmetry_number, thermochemistry};

    let labels: Vec<_> = frame
        .numbers
        .iter()
        .enumerate()
        .map(|(index, &number)| {
            u32::from(number) * 1000 + u32::from(frame.isotopes.get(index).copied().unwrap_or(0))
        })
        .collect();
    thermochemistry(&ThermoInput {
        frequencies_cm1,
        masses_amu,
        positions_angstrom,
        symmetry_number: rotational_symmetry_number(&labels, positions_angstrom, masses_amu),
        linear: None,
        temperature_k: THERMO_TEMPERATURE_K,
        pressure_pa: 101_325.,
        frequency_scale: 1.,
        rotor_cutoff_cm1: 50.,
        imaginary_cutoff_cm1: THERMO_IMAGINARY_CM1,
    })
}

/// Search counters and wall times are independent of the visible progress text.
#[derive(Default)]
struct SearchTrace {
    retry: Option<Retry>,
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
            let seconds = started.elapsed().as_secs_f64();
            if let Some((_, total)) = self.times.iter_mut().find(|(name, _)| name == stage) {
                *total += seconds;
            } else {
                self.times.push((stage.into(), seconds));
            }
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
        endpoint_thermo,
        method,
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
    let method_refinement = matches!(trace.retry, Some(Retry::Gxtb(_)));
    let checked = checked
        .and_then(|computed| checked_method(computed, method))
        .and_then(|computed| {
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
            engine_candidate(&transition, &report, &engine, method),
            Vec::new(),
            Some(format!("worker check failed: {error}")),
        ),
    };
    result.task = qcdata::Task::TransitionState;
    // The search's refined saddle is the energy used by every barrier display.
    result.energy_hartree = Some(report.ts.energy);
    // Reuse the existing TS frequency analysis; endpoint corrections and this
    // correction share the same temperature, isotope and symmetry policy.
    result.thermo = endpoint_thermo.as_ref().and_then(|_| {
        let vibrations = result.vibrations.as_ref()?;
        super::view::extra_thermo_imaginary(&vibrations.frequencies_cm1)
            .is_none()
            .then(|| {
                thermochemistry(
                    &transition,
                    &transition.positions_angstrom,
                    &masses,
                    &vibrations.frequencies_cm1,
                )
            })
    });
    result.input_frame = Some(launch.frame.hash());
    result.endpoints = Some(qcdata::Endpoints {
        reactant: reactant.hash(),
        product: product.hash(),
    });
    result.status = match reason {
        Some(reason) => qcdata::Status::NotConverged { reason },
        None => qcdata::Status::Converged,
    };
    result.trajectory = (!method_refinement).then(|| qcdata::Trajectory {
        rmsd_to_best_angstrom: Vec::new(),
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
        search: None,
        transition_state: None,
    });
    result
        .validate(&transition)
        .map_err(|error| Failure::Other(error.to_string()))?;
    let mut diagnostic_rows = diagnostics(&report, &worker_frequencies, &result, &trace);
    let endpoint_corrections = endpoint_thermo.and_then(|analysis| {
        diagnostic_rows.extend(analysis.diagnostics);
        analysis
            .corrections
            .iter()
            .any(Option::is_some)
            .then_some(analysis.corrections)
    });
    let irc = ordered_irc(&report, &transition, !method_refinement)?;
    let image_positions: Vec<_> = report
        .path
        .iter()
        .filter(|_| !method_refinement)
        .map(|point| angstroms(&point.positions))
        .collect();
    if irc.is_some()
        && let Some(band) = &mut result.trajectory
    {
        band.positions_angstrom = image_positions
            .iter()
            .flatten()
            .flatten()
            .copied()
            .collect();
    }
    let extras = Extras {
        endpoint_energies_hartree: [report.reactant.energy, report.product.energy],
        endpoint_thermo: endpoint_corrections,
        saddle_converged: report.saddle_converged,
        num_imaginary: report.imaginary,
        diagnostics: diagnostic_rows,
        image_positions,
        neb_converged: !method_refinement && report.neb_converged,
        reached_positions_angstrom: reached_endpoint(&report),
        irc,
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
        candidate: (report.imaginary >= 2).then_some(ts::SaddleCandidate {
            positions: report.ts.positions,
            hessian: report.hessian,
        }),
    };
    Ok(Report {
        frames: vec![transition, reactant, product],
        result,
        extras,
        info,
    })
}

/// Order the negative branch toward the reactant independently of the arbitrary
/// eigenvector sign used to launch the engine's two branches.
fn ordered_irc(
    report: &TsReport,
    frame: &Frame,
    has_neb_band: bool,
) -> Result<Option<qcdata::IrcPath>, Failure> {
    let Some(branches) = &report.irc else {
        return Ok(None);
    };
    use ts::Reached::{Product, Reactant};
    let [plus, minus] = branches;
    let ordered = match (plus.reached, minus.reached) {
        (Reactant, _) => [plus, minus],
        (_, Reactant) | (Product, _) => [minus, plus],
        _ => [plus, minus],
    };
    let points: Vec<_> = ordered[0]
        .path
        .iter()
        .rev()
        .chain(std::iter::once(&report.ts))
        .chain(ordered[1].path.iter())
        .collect();
    let trajectory = |points: &[&ts::Minimum]| qcdata::Trajectory {
        numbers: frame.numbers.clone(),
        isotopes: frame.isotopes.clone(),
        masses_amu: frame.masses_amu.clone(),
        charge: frame.charge,
        multiplicity: frame.multiplicity,
        positions_angstrom: points
            .iter()
            .flat_map(|point| point.positions.iter().map(|x| x * ANGSTROM_PER_BOHR))
            .collect(),
        energies_hartree: points.iter().map(|point| point.energy).collect(),
        labels: Vec::new(),
        minimizer_energies_hartree: Vec::new(),
        cluster_sizes: Vec::new(),
        rmsd_to_best_angstrom: Vec::new(),
    };
    let reached = ordered.map(|branch| match branch.reached {
        ts::Reached::Reactant => qcdata::IrcEndpoint::Reactant,
        ts::Reached::Product => qcdata::IrcEndpoint::Product,
        ts::Reached::Other => qcdata::IrcEndpoint::Other,
    });
    let mut path = qcdata::IrcPath::new(
        trajectory(&points),
        ordered[0].path.len(),
        reached,
        ordered.map(|branch| branch.completed),
        ordered.map(|branch| qcdata::IrcRelaxedEnd {
            energy_hartree: branch.end.energy,
            positions_angstrom: branch
                .end
                .positions
                .iter()
                .map(|x| x * ANGSTROM_PER_BOHR)
                .collect(),
        }),
    )
    .map_err(|error| Failure::Other(error.to_string()))?;
    if has_neb_band {
        let neb = trajectory(&report.path.iter().collect::<Vec<_>>());
        path.project_neb(&neb, report.climbing_image)
            .map_err(|error| Failure::Other(error.to_string()))?;
    }
    Ok(Some(path))
}

fn reached_endpoint(report: &TsReport) -> Vec<[f64; 3]> {
    use ts::Reached::{Other, Product};

    let Some(branches) = report.irc.as_ref().filter(|_| !report.connects) else {
        return Vec::new();
    };
    branches
        .iter()
        .find(|branch| branch.reached == Other)
        .or_else(|| {
            branches
                .iter()
                .all(|branch| branch.reached == Product)
                .then(|| &branches[0])
        })
        .map_or_else(Vec::new, |branch| angstroms(&branch.end.positions))
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
    let refining = trace.retry == Some(Retry::RefineHighest);
    let method_refinement = matches!(trace.retry, Some(Retry::Gxtb(_)));
    let mut rows = vec![
        (
            "Electronic method".into(),
            match &result.settings {
                qcdata::Settings::Xtb(settings) => settings.method.label().into(),
                _ => "xTB".into(),
            },
        ),
        (
            "Method".into(),
            if method_refinement {
                "Target-method endpoints → fresh Hessian → P-RFO → IRC"
            } else if refining {
                "Displaced saddle candidate → P-RFO → IRC"
            } else {
                "IDPP → CI-NEB → P-RFO → IRC"
            }
            .into(),
        ),
        (
            "Images".into(),
            if method_refinement {
                "No band search · target-method R / TS / P geometries".into()
            } else {
                format!(
                    "{} · climbing image {}",
                    report.path.len(),
                    report.climbing_image + 1
                )
            },
        ),
        (
            "NEB".into(),
            if method_refinement {
                "not run · refinement from a saved transition state".into()
            } else {
                format!(
                    "{} · {}",
                    if refining {
                        "reused the saved path".into()
                    } else {
                        trace.neb_iterations.map_or_else(
                            || "iterations unavailable".into(),
                            |n| format!("{n} iterations"),
                        )
                    },
                    convergence(report.neb_converged)
                )
            },
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
    if let Some(retry) = trace.retry {
        rows.push((
            "Retry".into(),
            match retry {
                Retry::RefineHighest => {
                    "Displaced the saved candidate along an extra imaginary mode".into()
                }
                Retry::FinerPath => format!("New search with {} path points", report.path.len()),
                Retry::Gxtb(_) => "Started from the verified GFN2-xTB saddle with a g-xTB Hessian and g-xTB-relaxed endpoints".into(),
            },
        ));
    }
    if let Some(detail) = &trace.reactive_start {
        rows.push(("Folded the starting conformer".into(), detail.clone()));
    }
    let worker = imaginary_count(worker_frequencies);
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
                "imaginary modes: engine {} · worker {worker} · {}",
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

fn engine_candidate(
    frame: &Frame,
    report: &TsReport,
    engine: &Engine,
    method: qcdata::XtbMethod,
) -> CalcResult {
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

fn imaginary_count(frequencies: &[f64]) -> usize {
    frequencies
        .iter()
        .filter(|&&f| f < -ts::IMAGINARY_CM1)
        .count()
}

/// Keep the same acceptance rule and diagnostic order as the CLI.
fn failure_reason(report: &TsReport, worker_frequencies: &[f64]) -> Option<String> {
    use ts::Reached::{Other, Product, Reactant};

    let worker = imaginary_count(worker_frequencies);
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
                Some([Reactant, Reactant]) => super::view::IRC_RETURNS_TO_REACTANT,
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

fn progress(
    event: ts::Event<'_>,
    last_band: &mut Option<Instant>,
    clock: impl FnOnce() -> Instant,
) -> Option<Event> {
    Some(match event {
        ts::Event::Stage(stage) => Event::Stage(match stage {
            ts::Stage::Endpoints => Stage::Endpoints,
            ts::Stage::Interpolation => Stage::Idpp,
            ts::Stage::Neb => Stage::Neb,
            ts::Stage::Saddle | ts::Stage::Frequencies => Stage::Saddle,
            ts::Stage::Irc => Stage::Irc,
        }),
        ts::Event::Neb {
            iteration,
            energies,
            finished,
            ..
        } => {
            let now = clock();
            if !finished
                && last_band
                    .is_some_and(|last| now.duration_since(last) < Duration::from_millis(250))
            {
                return None;
            }
            *last_band = Some(now);
            Event::Neb {
                iteration,
                energies: energies.to_vec(),
            }
        }
        ts::Event::Saddle { step, .. } => Event::Saddle { step },
        ts::Event::Irc { forward, step, .. } => Event::Irc { forward, step },
        ts::Event::Endpoint { .. } | ts::Event::SaddleTrial { .. } | ts::Event::SaddleStatus(_) => {
            return None;
        }
    })
}

pub(super) fn bohr(positions: &[[f64; 3]]) -> Vec<f64> {
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
