//! The worker-backed calculation hook for the CLI and MCP hosts.
use crate::{
    XTB_NEAR_DEGENERATE,
    qcdata::{ANGSTROM_PER_BOHR, Convergence, DEBYE_PER_E_BOHR, Frame, XtbMethod},
    updates::CURRENT_VERSION,
};
use reshiki_agent::ops::{
    calculate::{
        CalcFuture, CalcRequest, CalcTask, CalcValue, Calculated, Calculations, DEADLINE_EXCEEDED,
        Engine, EngineIdentity, Frequencies, Optimization, method_name,
    },
    progress::{Monotonic, Progress},
};
use reshiki_optimize::{
    Accuracy, EvalError, Level, Options, Outcome, Potential, Status, Step,
    conformers::{Topology, rmsd},
    optimize,
};
use reshiki_xtb_worker::{
    AXTB_LICENSE, AXTB_REV, AXTB_SOURCE, AXTB_VERSION,
    client::{Cancel, Error, Limits, Session},
    protocol::{
        self, Compute, Computed, ErrorKind, HessianRequest, Method, Open, ScfSettings, Units,
    },
};
use std::{fmt::Write as _, ops::ControlFlow, sync::Arc, thread, time::Instant};
use tokio::sync::oneshot;

const WORKER_STOPPED: &str = "the xTB worker stopped unexpectedly";

pub struct AgentCalculations;

impl Calculations for AgentCalculations {
    fn engine(&self) -> Engine {
        Engine {
            name: "axtb".into(),
            version: AXTB_VERSION.into(),
            license: AXTB_LICENSE.into(),
            source: AXTB_SOURCE.into(),
            rev: AXTB_REV.into(),
        }
    }

    fn calculate(&self, request: CalcRequest, progress: Option<Arc<Monotonic>>) -> CalcFuture {
        let started = Instant::now();
        let cancel = Cancel::default();
        let guard = CancelOnDrop(cancel.clone());
        let (tx, rx) = oneshot::channel();
        let spawned = thread::Builder::new()
            .name("xtb-agent".into())
            .spawn(move || {
                let _ = tx.send(run(request, progress.as_deref(), &cancel, started));
            });
        match spawned {
            Ok(_) => Box::pin(guard.receive(rx)),
            Err(_) => Box::pin(std::future::ready(Err(
                "could not start the xTB calculation".into(),
            ))),
        }
    }
}

struct CancelOnDrop(Cancel);

impl CancelOnDrop {
    async fn receive(
        self,
        rx: oneshot::Receiver<Result<Calculated, String>>,
    ) -> Result<Calculated, String> {
        let _guard = self;
        rx.await.map_err(|_| WORKER_STOPPED.to_owned())?
    }
}

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

fn run(
    request: CalcRequest,
    progress: Option<&Monotonic>,
    cancel: &Cancel,
    started: Instant,
) -> Result<Calculated, String> {
    let CalcRequest {
        task,
        frame,
        settings,
        deadline,
    } = request;
    let limits = Limits::default();
    let exe = std::env::current_exe().map_err(|error| worker_error(Error::Spawn(error)))?;
    let (mut session, hello) = Session::start(&exe, limits, cancel).map_err(worker_error)?;
    let engine = hello
        .engine
        .ok_or_else(|| worker_error(Error::Unavailable))?;
    let open = Open {
        numbers: frame.numbers.clone(),
        positions: frame.positions_angstrom.clone(),
        units: Units::Angstrom,
        charge: frame.charge,
        uhf: Some(frame.multiplicity - 1),
        method: match settings.method {
            XtbMethod::Gfn2Xtb => Method::Gfn2,
            XtbMethod::GXtb => Method::Gxtb,
            XtbMethod::GfnFf => Method::Gfnff,
        },
        conventions: settings.conventions,
        solvent: settings.solvent,
        scf: ScfSettings {
            accuracy: settings.accuracy,
            electronic_temperature_k: settings.electronic_temperature_k,
            max_iterations: settings.max_iterations,
            retries: settings.scf_retries,
        },
        gfnff: None,
        threads: settings.threads.unwrap_or_else(|| {
            thread::available_parallelism().map_or(1, |n| n.get().min(8) as u32)
        }),
        heap_bytes: limits.heap_bytes,
    };
    session.open(&open, cancel).map_err(worker_error)?;

    let outcome = if matches!(task, CalcTask::Optimize | CalcTask::OptFreq) {
        let positions: Vec<_> = open
            .positions
            .as_flattened()
            .iter()
            .map(|x| x / ANGSTROM_PER_BOHR)
            .collect();
        let options = Options {
            level: match settings.convergence {
                Some(Convergence::Tight) => Level::Tight,
                Some(Convergence::Normal) | None => Level::Normal,
            },
            fixed_atoms: settings
                .fixed_atoms
                .into_iter()
                .map(|atom| atom as usize)
                .collect(),
            ..Options::default()
        };
        let mut xtb = Xtb {
            session: &mut session,
            cancel,
        };
        let outcome = optimize(&mut xtb, &open.numbers, &positions, &options, |step| {
            optimization_step(step, deadline, progress)
        });
        match &outcome.status {
            Status::Converged | Status::MaxSteps => {}
            Status::ScfFailure(message) | Status::Failed(message) => return Err(message.clone()),
            Status::Cancelled if deadline_passed(deadline) => return Err(DEADLINE_EXCEEDED.into()),
            Status::Cancelled => return Err(worker_error(Error::Cancelled)),
        }
        Some(outcome)
    } else {
        None
    };

    let hessian = matches!(task, CalcTask::Frequencies | CalcTask::OptFreq);
    if hessian && let Some(progress) = progress {
        progress.report(Progress {
            completed: outcome.as_ref().map_or(1, |outcome| outcome.steps + 2) as f64,
            total: None,
            message: Some("Computing the Hessian".into()),
        });
    }
    let computed = session
        .compute(
            &Compute {
                positions: outcome
                    .as_ref()
                    .map(|outcome| outcome.positions.as_chunks::<3>().0.to_vec()),
                hessian: hessian.then(HessianRequest::default),
                ..Compute::default()
            },
            cancel,
        )
        .map_err(worker_error)?;
    session.close(cancel).map_err(worker_error)?;

    let optimization = outcome
        .as_ref()
        .map(|outcome| optimization(&frame, settings.method, outcome))
        .transpose()?;
    Ok(build_value(
        task,
        &frame,
        &computed,
        engine,
        optimization,
        started.elapsed().as_secs_f64(),
    ))
}

fn deadline_passed(deadline: Option<Instant>) -> bool {
    deadline.is_some_and(|deadline| Instant::now() >= deadline)
}

fn optimization_step(
    step: &Step<'_>,
    deadline: Option<Instant>,
    progress: Option<&Monotonic>,
) -> ControlFlow<()> {
    if deadline_passed(deadline) {
        return ControlFlow::Break(());
    }
    if let Some(progress) = progress {
        let energy = format!("{:.6}", step.energy).replace('-', "−");
        progress.report(Progress {
            completed: (step.index + 1) as f64,
            total: None,
            message: Some(format!(
                "Optimizing · step {} · E = {energy} Eh",
                step.index
            )),
        });
    }
    ControlFlow::Continue(())
}

struct Xtb<'a> {
    session: &'a mut Session,
    cancel: &'a Cancel,
}

impl Potential for Xtb<'_> {
    fn evaluate(
        &mut self,
        positions: &[f64],
        gradient: &mut [f64],
        _accuracy: Accuracy,
    ) -> Result<f64, EvalError> {
        let computed = self
            .session
            .compute(
                &Compute {
                    positions: Some(positions.as_chunks::<3>().0.to_vec()),
                    gradient: true,
                    ..Compute::default()
                },
                self.cancel,
            )
            .map_err(eval_error)?;
        let rows = computed
            .gradient
            .ok_or_else(|| EvalError::Failed("the xTB worker returned no gradient".into()))?;
        gradient.copy_from_slice(rows.as_flattened());
        Ok(computed.energy)
    }

    fn hessian(&mut self, positions: &[f64], out: &mut [f64]) -> Option<Result<(), EvalError>> {
        if positions.len() / 3 > protocol::MAX_HESSIAN_ATOMS {
            return None;
        }
        Some(
            self.session
                .compute(
                    &Compute {
                        positions: Some(positions.as_chunks::<3>().0.to_vec()),
                        hessian: Some(HessianRequest::default()),
                        ..Compute::default()
                    },
                    self.cancel,
                )
                .map(|computed| out.copy_from_slice(&computed.hessian))
                .map_err(eval_error),
        )
    }
}

fn worker_error(error: Error) -> String {
    match error {
        Error::Engine { message, .. }
        | Error::Input(message)
        | Error::Limit(message)
        | Error::Unsupported(message) => message,
        Error::Exited { .. } | Error::Timeout | Error::Protocol(_) => WORKER_STOPPED.into(),
        Error::Cancelled => "Calculation canceled".into(),
        Error::Spawn(_) => "could not start the xTB worker".into(),
        Error::Unavailable => error.to_string(),
    }
}

fn eval_error(error: Error) -> EvalError {
    match error {
        Error::Cancelled => EvalError::Cancelled,
        Error::Engine {
            kind: ErrorKind::Scf,
            message,
        } => EvalError::Scf(message),
        error => EvalError::Failed(worker_error(error)),
    }
}

fn optimization(
    frame: &Frame,
    method: XtbMethod,
    outcome: &Outcome,
) -> Result<Optimization, String> {
    let positions: Vec<_> = outcome
        .positions
        .as_chunks::<3>()
        .0
        .iter()
        .map(|xyz| xyz.map(|coordinate| coordinate * ANGSTROM_PER_BOHR))
        .collect();
    // Count all atoms and keep only the identity mapping.
    let topology =
        Topology::new(vec![6; frame.numbers.len()], vec![]).map_err(|error| error.to_string())?;
    let rmsd_angstrom = rmsd(
        &topology,
        &topology.automorphisms(1),
        &frame.positions_angstrom,
        &positions,
    )
    .map_err(|error| error.to_string())?;
    Ok(Optimization {
        converged: outcome.status == Status::Converged,
        steps: outcome.steps,
        rmsd_angstrom,
        xyz: xyz(&frame.numbers, &positions, method)?,
    })
}

fn xyz(numbers: &[u8], positions: &[[f64; 3]], method: XtbMethod) -> Result<String, String> {
    let mut text = format!(
        "{}\nOptimized with {} (ReShiki {CURRENT_VERSION})\n",
        numbers.len(),
        method.label()
    );
    for (&number, [x, y, z]) in numbers.iter().zip(positions) {
        let symbol = crate::chemistry::ELEMENTS
            .get(usize::from(number))
            .ok_or("invalid atomic number")?
            .symbol;
        writeln!(&mut text, "{symbol} {x:.8} {y:.8} {z:.8}").map_err(|error| error.to_string())?;
    }
    Ok(text)
}

fn build_value(
    task: CalcTask,
    frame: &Frame,
    computed: &Computed,
    engine: protocol::Engine,
    optimization: Option<Optimization>,
    seconds: f64,
) -> Calculated {
    let mut warnings = Vec::new();
    if let Some(optimization) = &optimization
        && !optimization.converged
    {
        warnings.push(format!(
            "The optimization did not converge in {} steps.",
            optimization.steps
        ));
    }
    if computed.near_degenerate {
        warnings.push(XTB_NEAR_DEGENERATE.into());
    }
    let energy = computed.energy;
    let frequencies = computed.vibrations.as_ref().map(|vibrations| {
        let imaginary = vibrations
            .frequencies_cm1
            .iter()
            .filter(|&&f| f < 0.0)
            .count();
        if imaginary > 0 {
            warnings.push(format!(
                "{imaginary} imaginary frequencies: the geometry is not a minimum."
            ));
        }
        let thermo = computed.thermo.as_ref();
        Frequencies {
            frequencies_cm1: vibrations
                .frequencies_cm1
                .iter()
                .map(|&f| round(f, 100.0))
                .collect(),
            ir_intensities_km_mol: vibrations
                .ir_intensities_km_mol
                .iter()
                .flatten()
                .map(|&i| round(i, 100.0))
                .collect(),
            imaginary,
            temperature_k: thermo.map(|t| t.temperature_k),
            zero_point_energy_hartree: thermo.map(|t| t.zero_point_energy_hartree),
            enthalpy_hartree: thermo.map(|t| energy + t.enthalpy_correction_hartree),
            gibbs_hartree: thermo.map(|t| energy + t.gibbs_correction_hartree),
        }
    });
    Calculated {
        value: CalcValue {
            task,
            method: method_name(computed.method.xtb_method()).into(),
            engine: EngineIdentity {
                name: engine.name,
                version: engine.version,
                rev: engine.rev,
            },
            atoms: frame.numbers.len(),
            charge: frame.charge,
            multiplicity: frame.multiplicity,
            energy_hartree: energy,
            homo_lumo_gap_ev: computed.homo_lumo_gap_ev,
            dipole_debye: computed
                .dipole_e_bohr
                .map(|[x, y, z]| x.hypot(y).hypot(z) * DEBYE_PER_E_BOHR),
            charges: computed
                .charges
                .as_ref()
                .map(|charges| charges.values.iter().map(|&q| round(q, 10_000.0)).collect()),
            optimization,
            frequencies,
            seconds,
        },
        warnings,
    }
}

fn round(value: f64, scale: f64) -> f64 {
    (value * scale).round() / scale + 0.0
}

#[cfg(test)]
mod tests;
