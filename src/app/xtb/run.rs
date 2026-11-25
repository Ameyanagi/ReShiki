//! One xTB job on its own thread: the worker client driven by the optimizer.

use super::results::Evaluation;
use super::state::Failure;
use iced::futures::{Stream, channel::mpsc};
use reshiki_optimize::{
    ANGSTROM_PER_BOHR, Accuracy, EvalError, Level, Options, Potential, Status, optimize,
};
use reshiki_xtb_worker::{
    client::{Cancel, Error, Limits, Session},
    protocol::{
        self, Compute, Computed, Conventions, Engine, ErrorKind, HessianRequest, Method, Open,
        OrbitalOutput, ScfSettings, Solvent, Units,
    },
};
use std::{
    ops::ControlFlow,
    sync::Arc,
    time::{Duration, Instant},
};

/// What to calculate. Positions are in ångströms, in frame order.
#[derive(Debug, Clone)]
pub(crate) struct Job {
    pub optimize: bool,
    pub numbers: Vec<u8>,
    pub positions: Vec<[f64; 3]>,
    pub charge: i32,
    /// Unpaired electrons.
    pub uhf: u32,
    pub method: Method,
    pub conventions: Conventions,
    pub scf: ScfSettings,
    pub solvent: Option<Solvent>,
    pub threads: u32,
    /// Zero-based frame atoms the optimizer holds fixed.
    pub fixed: Vec<usize>,
    /// Use Tight convergence instead of Normal.
    pub tight: bool,
}

#[derive(Debug, Clone)]
pub(crate) enum Event {
    /// Accepted steps since the previous event, starting at `first`.
    /// Step 0 is the input. Positions are in Å for the newest step.
    Steps {
        first: usize,
        energies: Vec<f64>,
        positions: Vec<[f64; 3]>,
    },
    /// Elapsed time goes on while the worker computes: every 250 ms.
    Tick,
    /// The last event of a job.
    Done(Result<Arc<Report>, Failure>),
}

#[derive(Debug)]
pub(crate) struct Report {
    /// From the worker's hello.
    pub engine: Engine,
    /// The worker's final calculation at `positions`.
    pub computed: Computed,
    /// Hartree, at `positions`.
    pub energy: f64,
    /// Whether optimization converged. Always true for energy jobs.
    pub converged: bool,
    /// Final geometry in Å (the input geometry for an energy job).
    pub positions: Vec<[f64; 3]>,
    /// Accepted optimizer steps (0 for an energy job).
    pub steps: usize,
    /// One energy per accepted geometry, including the input.
    pub energies: Vec<f64>,
    /// Step-major Å coordinates, one block per energy. Empty for an energy job.
    pub path: Vec<f64>,
    /// Every energy the worker computed, in order, for the Log tab.
    pub evaluations: Vec<Evaluation>,
}

impl From<&Computed> for Evaluation {
    fn from(computed: &Computed) -> Self {
        Self {
            energy: computed.energy,
            scf_iterations: computed.scf.as_ref().map_or(0, |scf| scf.iterations),
            seconds: computed.seconds,
        }
    }
}

/// Start `job` on a new thread. The stream ends after `Event::Done`.
pub(crate) fn start(job: Job, cancel: Cancel) -> impl Stream<Item = Event> + Send + 'static {
    let (tx, rx) = mpsc::unbounded();
    let ticks = tx.clone();
    // Ends when the receiver is dropped or the channel is closed after Done.
    let _ = std::thread::Builder::new()
        .name("xtb-tick".into())
        .spawn(move || {
            while !ticks.is_closed() {
                std::thread::sleep(Duration::from_millis(250));
                if ticks.unbounded_send(Event::Tick).is_err() {
                    break;
                }
            }
        });
    let worker_tx = tx.clone();
    let thread = std::thread::Builder::new()
        .name("xtb-run".into())
        .spawn(move || {
            let result = run(job, &cancel, &worker_tx);
            let done = Event::Done(result.map(Arc::new));
            let _ = worker_tx.unbounded_send(done);
            worker_tx.close_channel();
        });
    if let Err(error) = thread {
        let _ = tx.unbounded_send(Event::Done(Err(Failure::Other(error.to_string()))));
        tx.close_channel();
    }
    rx
}

fn run(job: Job, cancel: &Cancel, tx: &mpsc::UnboundedSender<Event>) -> Result<Report, Failure> {
    let limits = Limits::default();
    let exe = std::env::current_exe().map_err(|error| worker_failure(&Error::Spawn(error)))?;
    let (mut session, hello) =
        Session::start(&exe, limits, cancel).map_err(|error| worker_failure(&error))?;
    let engine = hello
        .engine
        .ok_or_else(|| worker_failure(&Error::Unavailable))?;
    let open = Open {
        numbers: job.numbers,
        positions: job.positions,
        units: Units::Angstrom,
        charge: job.charge,
        uhf: Some(job.uhf),
        method: job.method,
        conventions: job.conventions,
        solvent: job.solvent,
        scf: job.scf.clone(),
        gfnff: None,
        threads: job.threads,
        heap_bytes: limits.heap_bytes,
    };
    match session.open(&open, cancel) {
        Ok(_) => {}
        Err(Error::Engine {
            kind: ErrorKind::Input,
            message,
        }) if message.contains("same parity as the valence electron count") => {
            drop(session);
            let probe = || {
                let (mut session, _) = Session::start(&exe, limits, cancel)?;
                let opened = session.open(&Open { uhf: None, ..open }, cancel)?;
                session.close(cancel)?;
                Ok::<_, Error>(opened.uhf)
            };
            return Err(match probe() {
                Ok(valence) if valence % 2 != job.uhf % 2 => Failure::Spin { valence },
                _ => Failure::Other(message),
            });
        }
        Err(error) => return Err(worker_failure(&error)),
    }

    let mut path = Vec::new();
    let mut evaluations = Vec::new();
    let outcome = if job.optimize {
        let xyz = open.positions.as_flattened();
        let positions: Vec<_> = xyz.iter().map(|x| x / ANGSTROM_PER_BOHR).collect();
        let level = if job.tight {
            Level::Tight
        } else {
            Level::Normal
        };
        let options = Options {
            level,
            fixed_atoms: job.fixed,
            tight_final: false,
            ..Options::default()
        };
        let mut xtb = Xtb {
            session: &mut session,
            cancel,
            evaluations: &mut evaluations,
            failure: None,
        };
        let mut batch = Batch::default();
        let outcome = optimize(&mut xtb, &open.numbers, &positions, &options, |step| {
            let positions = angstroms(step.positions);
            path.extend_from_slice(positions.as_flattened());
            let event = batch.push(step.index, step.energy, positions, Instant::now());
            if event.is_some_and(|event| tx.unbounded_send(event).is_err()) {
                return ControlFlow::Break(());
            }
            ControlFlow::Continue(())
        });
        let remaining = batch.flush();
        if remaining.is_some_and(|event| tx.unbounded_send(event).is_err()) {
            return Err(canceled());
        }
        match outcome.status {
            Status::Converged | Status::MaxSteps => {}
            Status::ScfFailure(message) => return Err(Failure::Scf(message)),
            Status::Failed(message) => return Err(xtb.failure.unwrap_or(Failure::Other(message))),
            Status::Cancelled => return Err(canceled()),
        }
        if path.len() != open.numbers.len() * 3 * outcome.energies.len() {
            path.clear();
        }
        Some(outcome)
    } else {
        None
    };
    let final_xyz = outcome.as_ref().map(|outcome| outcome.positions.as_slice());
    let electronic = job.method.xtb_method().capabilities().orbitals;
    // One evaluation gives the stored energy, gradient and properties.
    let final_request = Compute {
        positions: final_xyz.map(|xyz| xyz.as_chunks::<3>().0.to_vec()),
        units: Units::Bohr,
        gradient: outcome.is_some(),
        bond_orders: electronic,
        orbitals: if electronic {
            OrbitalOutput::Levels
        } else {
            OrbitalOutput::None
        },
        ..Compute::default()
    };
    let computed = session
        .compute(&final_request, cancel)
        .map_err(|error| worker_failure(&error))?;
    // The result is complete; a worker that fails to exit cleanly is reaped.
    let _ = session.close(cancel);
    evaluations.push(Evaluation::from(&computed));
    let energy = computed.energy;
    let positions = final_xyz.map_or_else(|| open.positions, angstroms);
    let converged = outcome
        .as_ref()
        .is_none_or(|out| out.status == Status::Converged);
    let steps = outcome.as_ref().map_or(0, |outcome| outcome.steps);
    let mut energies = outcome.map_or_else(Vec::new, |outcome| outcome.energies);
    match energies.last_mut() {
        Some(last) => *last = energy,
        None => energies.push(energy),
    }
    Ok(Report {
        engine,
        computed,
        energy,
        converged,
        positions,
        steps,
        energies,
        path,
        evaluations,
    })
}

fn angstroms(positions: &[f64]) -> Vec<[f64; 3]> {
    let (atoms, _) = positions.as_chunks::<3>();
    let scale = |coordinate| coordinate * ANGSTROM_PER_BOHR;
    atoms.iter().map(|xyz| xyz.map(scale)).collect()
}

struct Xtb<'a> {
    session: &'a mut Session,
    cancel: &'a Cancel,
    evaluations: &'a mut Vec<Evaluation>,
    // The optimizer's Failed status carries only a string.
    failure: Option<Failure>,
}

impl Potential for Xtb<'_> {
    fn evaluate(
        &mut self,
        positions: &[f64],
        gradient: &mut [f64],
        _accuracy: Accuracy,
    ) -> Result<f64, EvalError> {
        let request = Compute {
            positions: Some(positions.as_chunks::<3>().0.to_vec()),
            units: Units::Bohr,
            gradient: true,
            ..Compute::default()
        };
        let computed = self
            .session
            .compute(&request, self.cancel)
            .map_err(|error| {
                self.failure = Some(worker_failure(&error));
                eval_error(error)
            })?;
        self.failure = None;
        self.evaluations.push(Evaluation::from(&computed));
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
        let request = Compute {
            positions: Some(positions.as_chunks::<3>().0.to_vec()),
            units: Units::Bohr,
            hessian: Some(HessianRequest::default()),
            ..Compute::default()
        };
        Some(
            self.session
                .compute(&request, self.cancel)
                .map(|computed| out.copy_from_slice(&computed.hessian))
                .map_err(eval_error),
        )
    }
}

fn eval_error(error: Error) -> EvalError {
    match error {
        Error::Cancelled => EvalError::Cancelled,
        Error::Engine {
            kind: ErrorKind::Scf,
            message,
        } => EvalError::Scf(message),
        error => EvalError::Failed(error.to_string()),
    }
}

fn worker_failure(error: &Error) -> Failure {
    match error {
        Error::Engine {
            kind: ErrorKind::Scf,
            message,
        } => Failure::Scf(message.clone()),
        Error::Engine {
            kind: ErrorKind::UnsupportedElement,
            message,
        } => message
            .trim_end()
            .rsplit_once("Z=")
            .and_then(|(_, number)| number.parse::<u8>().ok())
            .map_or_else(
                || Failure::Other(message.clone()),
                |z| Failure::Element {
                    z,
                    // g-xTB in solvent also runs GFN2-xTB.
                    by: super::state::METHODS
                        .into_iter()
                        .find(|method| message.starts_with(method.label())),
                },
            ),
        Error::Engine {
            kind: ErrorKind::Limit,
            message,
        }
        | Error::Limit(message) => Failure::Other(message.clone()),
        Error::Exited { status, .. } => {
            Failure::Crashed(format!("xTB worker exited with {status}"))
        }
        // Fixed texts: Copy details never carries the worker's own words.
        Error::Protocol(_) => Failure::Crashed("xTB worker protocol error".into()),
        Error::Timeout => Failure::Crashed("xTB worker reached its request deadline".into()),
        Error::Spawn(_) => Failure::Crashed("could not start the xTB worker".into()),
        Error::Cancelled => canceled(),
        error => Failure::Other(error.to_string()),
    }
}

fn canceled() -> Failure {
    Failure::Other("Calculation canceled".into())
}

#[derive(Default)]
struct Batch {
    first: usize,
    energies: Vec<f64>,
    positions: Vec<[f64; 3]>,
    sent_at: Option<Instant>,
}

impl Batch {
    fn push(
        &mut self,
        index: usize,
        energy: f64,
        positions: Vec<[f64; 3]>,
        now: Instant,
    ) -> Option<Event> {
        if self.energies.is_empty() {
            self.first = index;
        }
        self.energies.push(energy);
        self.positions = positions;
        let elapsed = self.sent_at.map(|sent| now.duration_since(sent));
        if elapsed.is_some_and(|elapsed| elapsed < Duration::from_millis(100)) {
            return None;
        }
        self.sent_at = Some(now);
        self.flush()
    }

    fn flush(&mut self) -> Option<Event> {
        (!self.energies.is_empty()).then(|| Event::Steps {
            first: self.first,
            energies: std::mem::take(&mut self.energies),
            positions: std::mem::take(&mut self.positions),
        })
    }
}

/// The hello's engine, as tests answer it.
#[cfg(test)]
pub(crate) fn example_engine() -> Engine {
    Engine {
        name: "axtb".into(),
        version: "0.2.0".into(),
        license: "LGPL-3.0-or-later".into(),
        source: "https://github.com/Ameyanagi/axtb".into(),
        rev: "9d0388c".into(),
    }
}

/// A neutral singlet GFN2-xTB answer with only an energy and a gap.
#[cfg(test)]
pub(crate) fn example_computed(energy: f64, gap_ev: f64) -> Computed {
    serde_json::from_value(serde_json::json!({
        "method": "gfn2",
        "conventions": "axtb",
        "engine_conventions": "axtb",
        "numerics": "f64",
        "settings": { "charge": 0, "uhf": 0, "solvent": null, "scf": {}, "threads": 1 },
        "energy": energy,
        "energy_terms": {},
        "homo_lumo_gap_ev": gap_ev,
        "near_degenerate": gap_ev < protocol::NEAR_DEGENERATE_GAP_EV,
        "arrays": [],
        "seconds": 0.003
    }))
    .expect("worker response")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batches_accepted_steps_and_flushes_the_remainder() {
        let mut batch = Batch::default();
        let now = Instant::now();
        let mut push = |index, energy, ms| {
            let time = now + Duration::from_millis(ms);
            batch.push(index, energy, vec![[energy, 0., 0.]], time)
        };
        assert!(matches!(
            push(0, -1., 0),
            Some(Event::Steps { first: 0, .. })
        ));
        assert!(push(1, -2., 99).is_none());
        assert!(
            matches!(push(2, -3., 100), Some(Event::Steps { first: 1, energies, positions })
            if energies == [-2., -3.] && positions == [[-3., 0., 0.]])
        );
        assert!(push(3, -4., 100).is_none());
        assert!(
            matches!(batch.flush(), Some(Event::Steps { first: 3, energies, .. })
            if energies == [-4.])
        );
        assert!(batch.flush().is_none());
    }
}
