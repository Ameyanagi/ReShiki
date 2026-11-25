//! One xTB job on its own thread: the worker client driven by the optimizer.

use super::results::Evaluation;
use iced::futures::{Stream, channel::mpsc};
use reshiki_optimize::{
    ANGSTROM_PER_BOHR, Accuracy, EvalError, Level, Options, Potential, Status, optimize,
};
use reshiki_xtb_worker::{
    client::{Cancel, Error, Limits, Session},
    protocol::{
        Compute, Computed, Conventions, ErrorKind, Method, Open, Solvent, SolventModel, Units,
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
    /// ALPB solvent name, e.g. "water"; None is gas phase.
    pub solvent: Option<String>,
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
    Done(Result<Arc<Report>, String>),
}

#[derive(Debug)]
pub(crate) struct Report {
    /// From the worker's hello.
    pub engine_version: String,
    pub engine_rev: String,
    /// Hartree, at `positions`.
    pub energy: f64,
    pub gap_ev: Option<f64>,
    pub near_degenerate: bool,
    /// Whether optimization converged. Always true for energy jobs.
    pub converged: bool,
    /// Final geometry in Å (the input geometry for an energy job).
    pub positions: Vec<[f64; 3]>,
    /// Atom-major Eh/bohr at `positions`. Empty for an energy job.
    pub gradient: Vec<f64>,
    /// Accepted optimizer steps (0 for an energy job).
    pub steps: usize,
    /// One energy per accepted geometry, including the input.
    pub energies: Vec<f64>,
    /// Step-major Å coordinates, one block per energy. Empty for an energy job.
    pub path: Vec<f64>,
    /// Every energy the worker computed, in order, for the Log tab.
    pub evaluations: Vec<Evaluation>,
    /// How the engine computed (its numerics line).
    pub numerics: String,
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
            let done = Event::Done(result.map(Arc::new).map_err(|error| error.to_string()));
            let _ = worker_tx.unbounded_send(done);
            worker_tx.close_channel();
        });
    if let Err(error) = thread {
        let _ = tx.unbounded_send(Event::Done(Err(error.to_string())));
        tx.close_channel();
    }
    rx
}

fn run(
    job: Job,
    cancel: &Cancel,
    tx: &mpsc::UnboundedSender<Event>,
) -> Result<Report, Box<dyn std::error::Error>> {
    let limits = Limits::default();
    let (mut session, hello) = Session::start(&std::env::current_exe()?, limits, cancel)?;
    let engine = hello.engine.ok_or(Error::Unavailable)?;
    let open = Open {
        numbers: job.numbers,
        positions: job.positions,
        units: Units::Angstrom,
        charge: job.charge,
        uhf: Some(job.uhf),
        method: Method::Gfn2,
        conventions: Conventions::default(),
        solvent: job.solvent.map(|name| Solvent {
            model: SolventModel::Alpb,
            name,
            state: Default::default(),
            correction: None,
        }),
        scf: Default::default(),
        gfnff: None,
        threads: job.threads,
        heap_bytes: limits.heap_bytes,
    };
    session.open(&open, cancel)?;

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
            return Err("Calculation canceled".into());
        }
        match outcome.status {
            Status::Converged | Status::MaxSteps => {}
            Status::ScfFailure(message) => {
                return Err(format!("SCF did not converge: {message}").into());
            }
            Status::Failed(message) => return Err(message.into()),
            Status::Cancelled => return Err("Calculation canceled".into()),
        }
        if path.len() != open.numbers.len() * 3 * outcome.energies.len() {
            path.clear();
        }
        Some(outcome)
    } else {
        None
    };
    let final_xyz = outcome.as_ref().map(|outcome| outcome.positions.as_slice());
    let computed = session.compute(&request(final_xyz, false), cancel)?;
    let _ = session.close(cancel);
    evaluations.push(Evaluation::from(&computed));
    let energy = outcome
        .as_ref()
        .map_or(computed.energy, |outcome| outcome.energy);
    let positions = final_xyz.map_or_else(|| open.positions, angstroms);
    let converged = outcome
        .as_ref()
        .is_none_or(|out| out.status == Status::Converged);
    let steps = outcome.as_ref().map_or(0, |outcome| outcome.steps);
    let (gradient, energies) = outcome.map_or_else(
        || (Vec::new(), vec![energy]),
        |outcome| (outcome.gradient, outcome.energies),
    );
    Ok(Report {
        engine_version: engine.version,
        engine_rev: engine.rev,
        energy,
        gap_ev: computed.homo_lumo_gap_ev,
        near_degenerate: computed.near_degenerate,
        converged,
        positions,
        gradient,
        steps,
        energies,
        path,
        evaluations,
        numerics: computed.numerics,
    })
}

fn request(bohr: Option<&[f64]>, gradient: bool) -> Compute {
    let units = if bohr.is_some() {
        Units::Bohr
    } else {
        Units::Angstrom
    };
    let positions = bohr.map(|xyz| xyz.as_chunks::<3>().0.to_vec());
    Compute {
        positions,
        units,
        gradient,
        ..Compute::default()
    }
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
}

impl Potential for Xtb<'_> {
    fn evaluate(
        &mut self,
        positions: &[f64],
        gradient: &mut [f64],
        _accuracy: Accuracy,
    ) -> Result<f64, EvalError> {
        let request = request(Some(positions), true);
        let result = self.session.compute(&request, self.cancel);
        let computed = result.map_err(|error| match error {
            Error::Cancelled => EvalError::Cancelled,
            Error::Engine {
                kind: ErrorKind::Scf,
                message,
            } => EvalError::Scf(message),
            error => EvalError::Failed(error.to_string()),
        })?;
        self.evaluations.push(Evaluation::from(&computed));
        let rows = computed
            .gradient
            .ok_or_else(|| EvalError::Failed("the xTB worker returned no gradient".into()))?;
        gradient.copy_from_slice(rows.as_flattened());
        Ok(computed.energy)
    }
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
