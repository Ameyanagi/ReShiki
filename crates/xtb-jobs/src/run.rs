//! One xTB worker session driven by the optimizer.

use crate::Evaluation;
pub use crate::Failure;
use reshiki_optimize::{
    ANGSTROM_PER_BOHR, Accuracy, EvalError, Level, Options, Potential, Status, optimize,
};
use reshiki_qcdata::{XtbMethod, vibrational::rotational_symmetry_number};
use reshiki_xtb_worker::{
    client::{Cancel, Error, Limits, Session},
    protocol::{
        self, Compute, Computed, Conventions, Engine, ErrorKind, HessianRequest, Method, Open,
        OrbitalOutput, ScfSettings, Solvent, Units,
    },
};
use std::{
    ops::ControlFlow,
    path::Path,
    time::{Duration, Instant},
};

/// What to calculate. Positions are in ångströms, in frame order.
#[derive(Debug, Clone)]
pub struct Job {
    pub optimize: bool,
    pub frequencies: bool,
    /// Analysis masses (u) for isotope-labelled molecules; standard masses otherwise.
    pub masses: Option<Vec<f64>>,
    /// Atom labels encoded as Z * 1000 + isotope (0 for natural), when masses are explicit.
    pub isotope_labels: Vec<u32>,
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
    /// GFN-FF only: the geometry (Å) its topology comes from, when it is not
    /// `positions` (a conformer search keeps one topology for every start).
    pub topology: Option<Vec<[f64; 3]>>,
    pub max_steps: Option<usize>,
    /// Override the final electronic properties, which otherwise follow the method.
    pub properties: Option<Properties>,
    pub thermo: protocol::ThermoRequest,
}

pub fn worker_method(method: XtbMethod) -> Method {
    match method {
        XtbMethod::Gfn2Xtb => Method::Gfn2,
        XtbMethod::GfnFf => Method::Gfnff,
        XtbMethod::GXtb => Method::Gxtb,
    }
}

#[derive(Debug, Clone)]
pub struct Properties {
    pub gradient: bool,
    pub bond_orders: bool,
    pub orbitals: OrbitalOutput,
}

#[derive(Debug, Clone)]
pub enum Event {
    /// Accepted steps since the previous event, starting at `first`.
    /// Step 0 is the input. Positions are in Å for the newest step.
    Steps {
        first: usize,
        energies: Vec<f64>,
        positions: Vec<[f64; 3]>,
    },
    /// The final Hessian evaluation is starting.
    Hessian,
}

#[derive(Debug)]
pub struct Report {
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

pub fn run(
    executable: &Path,
    job: Job,
    cancel: &Cancel,
    mut on_event: impl FnMut(Event),
) -> Result<Report, Failure> {
    let limits = Limits::default();
    let (mut session, hello) =
        Session::start(executable, limits, cancel).map_err(worker_failure)?;
    let engine = hello
        .engine
        .ok_or_else(|| worker_failure(Error::Unavailable))?;
    let solvent = job.solvent.clone().map(|mut solvent| {
        solvent.correction = crate::method::solvent_correction(job.method.xtb_method());
        solvent
    });
    let open = Open {
        numbers: job.numbers.clone(),
        positions: job.positions.clone(),
        units: Units::Angstrom,
        charge: job.charge,
        uhf: Some(job.uhf),
        method: job.method,
        conventions: job.conventions,
        solvent,
        scf: job.scf.clone(),
        gfnff: job.topology.clone().map(|positions| protocol::GfnFfOpen {
            reference_positions: Some(positions),
            fragment_charges: None,
        }),
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
                let (mut session, _) = Session::start(executable, limits, cancel)?;
                let opened = session.open(&Open { uhf: None, ..open }, cancel)?;
                session.close(cancel)?;
                Ok::<_, Error>(opened.uhf)
            };
            let reason = match probe() {
                Ok(valence) if valence % 2 != job.uhf % 2 => Failure::Spin { valence },
                _ => Failure::Other(message.clone()),
            };
            return Err(with_worker_error(
                Error::Engine {
                    kind: ErrorKind::Input,
                    message,
                },
                reason,
            ));
        }
        Err(error) => return Err(worker_failure(error)),
    }

    let mut path = Vec::new();
    let mut evaluations = Vec::new();
    let outcome = if job.optimize {
        let xyz = open.positions.as_flattened();
        let positions: Vec<_> = xyz.iter().map(|x| x / ANGSTROM_PER_BOHR).collect();
        let options = options(&job);
        let mut xtb = Xtb {
            session: &mut session,
            cancel,
            evaluations: &mut evaluations,
            failure: None,
        };
        let mut batch = Batch::default();
        let outcome = optimize(&mut xtb, &open.numbers, &positions, &options, |step| {
            let positions = angstroms(step.positions);
            // The Tight recheck reports the same step with a new energy.
            path.truncate(step.index * step.positions.len());
            path.extend_from_slice(positions.as_flattened());
            if let Some(event) = batch.push(step.index, step.energy, positions, Instant::now()) {
                on_event(event);
            }
            if cancel.is_cancelled() {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        });
        if let Some(event) = batch.flush() {
            on_event(event);
        }
        match outcome.status {
            Status::Converged | Status::MaxSteps => {}
            Status::ScfFailure(message) => {
                return Err(xtb.failure.unwrap_or(Failure::Scf(message)));
            }
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
    // One evaluation gives the stored energy, gradient and properties.
    let final_request = final_request(&job, final_xyz);
    if job.frequencies {
        on_event(Event::Hessian);
    }
    let computed = session
        .compute(&final_request, cancel)
        .map_err(worker_failure)?;
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

fn options(job: &Job) -> Options {
    let mut options = Options {
        level: if job.tight || job.frequencies {
            Level::Tight
        } else {
            Level::Normal
        },
        fixed_atoms: job.fixed.clone(),
        tight_final: job.frequencies,
        ..Options::default()
    };
    if let Some(max_steps) = job.max_steps {
        options.max_steps = max_steps;
    }
    options
}

fn final_request(job: &Job, positions: Option<&[f64]>) -> Compute {
    let capabilities = job.method.xtb_method().capabilities();
    let properties = job.properties.clone().unwrap_or_else(|| Properties {
        gradient: positions.is_some(),
        bond_orders: capabilities.bond_orders,
        orbitals: if capabilities.orbitals {
            OrbitalOutput::Levels
        } else {
            OrbitalOutput::None
        },
    });
    Compute {
        positions: positions.map(|xyz| xyz.as_chunks::<3>().0.to_vec()),
        units: Units::Bohr,
        gradient: properties.gradient,
        hessian: job.frequencies.then(|| HessianRequest {
            masses_amu: job.masses.clone(),
            thermo: protocol::ThermoRequest {
                symmetry_number: job.thermo.symmetry_number.or_else(|| {
                    job.masses.as_ref().map(|masses| {
                        let final_angstrom = positions.map(angstroms);
                        let geometry = final_angstrom.as_deref().unwrap_or(&job.positions);
                        rotational_symmetry_number(&job.isotope_labels, geometry, masses)
                    })
                }),
                ..job.thermo.clone()
            },
        }),
        bond_orders: properties.bond_orders,
        orbitals: properties.orbitals,
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
                let evaluation = eval_error(&error);
                self.failure = Some(worker_failure(error));
                evaluation
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
                .map_err(|error| eval_error(&error)),
        )
    }
}

fn eval_error(error: &Error) -> EvalError {
    match error {
        Error::Cancelled => EvalError::Cancelled,
        Error::Engine {
            kind: ErrorKind::Scf,
            message,
        } => EvalError::Scf(message.clone()),
        error => EvalError::Failed(error.to_string()),
    }
}

fn with_worker_error(error: Error, reason: Failure) -> Failure {
    Failure::Worker {
        error: error.into(),
        reason: Box::new(reason),
    }
}

fn worker_failure(error: Error) -> Failure {
    let reason = worker_failure_reason(&error);
    with_worker_error(error, reason)
}

fn worker_failure_reason(error: &Error) -> Failure {
    let summary = match error {
        Error::Engine {
            kind: ErrorKind::Scf,
            message,
        } => return Failure::Scf(message.clone()),
        Error::Engine {
            kind: ErrorKind::UnsupportedElement,
            message,
        } => {
            return message
                .trim_end()
                .rsplit_once("Z=")
                .and_then(|(_, number)| number.parse::<u8>().ok())
                .map_or_else(
                    || Failure::Other(message.clone()),
                    |z| Failure::Element {
                        z,
                        // g-xTB in solvent also runs GFN2-xTB.
                        by: Method::ALL
                            .into_iter()
                            .map(Method::xtb_method)
                            .find(|method| message.starts_with(method.label())),
                    },
                );
        }
        Error::Engine {
            kind: ErrorKind::Limit,
            message,
        }
        | Error::Limit(message) => return Failure::Other(message.clone()),
        Error::Exited { status, .. } => format!("xTB worker exited with {status}"),
        Error::Protocol(_) => "xTB worker protocol error".into(),
        Error::Timeout => "xTB worker reached its request deadline".into(),
        Error::Spawn(_) => "could not start the xTB worker".into(),
        Error::Cancelled => return canceled(),
        error => return Failure::Other(error.to_string()),
    };
    Failure::crashed(summary)
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
        self.energies.truncate(index - self.first);
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
#[cfg(any(test, feature = "test-fixtures"))]
#[doc(hidden)]
pub fn example_engine() -> Engine {
    Engine {
        name: "axtb".into(),
        version: "0.2.0".into(),
        license: "LGPL-3.0-or-later".into(),
        source: "https://github.com/Ameyanagi/axtb".into(),
        rev: "9d0388c".into(),
    }
}

/// A neutral singlet GFN2-xTB answer with only an energy and a gap.
#[cfg(any(test, feature = "test-fixtures"))]
#[doc(hidden)]
pub fn example_computed(energy: f64, gap_ev: f64) -> Computed {
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

    fn job() -> Job {
        Job {
            optimize: true,
            frequencies: false,
            masses: None,
            isotope_labels: Vec::new(),
            numbers: vec![8, 1, 1],
            positions: vec![[0.; 3]; 3],
            charge: 0,
            uhf: 0,
            method: Method::Gfn2,
            conventions: Conventions::Axtb,
            scf: ScfSettings::default(),
            solvent: None,
            threads: 1,
            fixed: vec![0],
            tight: false,
            topology: None,
            max_steps: None,
            properties: None,
            thermo: Default::default(),
        }
    }

    fn d2o_job() -> Job {
        Job {
            frequencies: true,
            masses: Some(vec![15.999, 2.014101778, 2.014101778]),
            isotope_labels: vec![8000, 1002, 1002],
            positions: vec![[0., 0., 0.], [0.7586, 0., 0.5043], [-0.7586, 0., 0.5043]],
            ..job()
        }
    }

    #[test]
    fn d2o_hessian_uses_the_symmetry_of_its_geometry() {
        let mut job = d2o_job();
        let final_bohr: Vec<_> = job
            .positions
            .iter()
            .map(|xyz| xyz.map(|coordinate| coordinate / ANGSTROM_PER_BOHR))
            .collect();
        job.positions[1][0] = 0.8586;

        let start = final_request(&job, None);
        assert!(start.positions.is_none());
        let start_hessian = start.hessian.expect("frequency request");
        assert_eq!(start_hessian.masses_amu, job.masses);
        assert_eq!(start_hessian.thermo.symmetry_number, Some(1));

        let optimized = final_request(&job, Some(final_bohr.as_flattened()));
        assert_eq!(optimized.positions.as_deref(), Some(final_bohr.as_slice()));
        assert_eq!(optimized.units, Units::Bohr);
        let optimized_hessian = optimized.hessian.expect("frequency request");
        assert_eq!(optimized_hessian.masses_amu, job.masses);
        assert_eq!(optimized_hessian.thermo.symmetry_number, Some(2));
    }

    #[test]
    fn final_frequency_request_keeps_electronic_properties() {
        let positions = [0., 1., 2., 3., 4., 5.];
        let mut job = job();
        job.frequencies = true;
        for method in [Method::Gfn2, Method::Gxtb, Method::Gfnff] {
            job.method = method;
            let request = final_request(&job, Some(&positions));
            assert_eq!(request.positions, Some(vec![[0., 1., 2.], [3., 4., 5.]]));
            assert_eq!(request.units, Units::Bohr);
            assert!(request.gradient);
            assert!(request.hessian.is_some());
            let electronic = method != Method::Gfnff;
            assert_eq!(request.bond_orders, electronic);
            assert_eq!(
                request.orbitals,
                if electronic {
                    OrbitalOutput::Levels
                } else {
                    OrbitalOutput::None
                }
            );
        }
        job.optimize = false;
        let request = final_request(&job, None);
        assert!(request.positions.is_none());
        assert!(!request.gradient);
        let hessian = request.hessian.expect("frequency request");
        assert!(hessian.masses_amu.is_none());
        assert!(hessian.thermo.symmetry_number.is_none());

        job.frequencies = false;
        let request = final_request(&job, None);
        assert!(request.positions.is_none());
        assert!(!request.gradient);
        assert!(request.hessian.is_none());
    }

    #[test]
    fn frequencies_require_tight_optimization_and_final_recheck() {
        let mut job = job();
        let normal = options(&job);
        assert_eq!(normal.level, Level::Normal);
        assert!(!normal.tight_final);
        assert_eq!(normal.fixed_atoms, job.fixed);

        job.frequencies = true;
        let frequencies = options(&job);
        assert_eq!(frequencies.level, Level::Tight);
        assert!(frequencies.tight_final);
        assert_eq!(frequencies.fixed_atoms, job.fixed);

        job.frequencies = false;
        job.tight = true;
        let tight = options(&job);
        assert_eq!(tight.level, Level::Tight);
        assert!(!tight.tight_final);
    }

    #[test]
    fn job_can_override_the_step_limit_properties_and_thermochemistry() {
        let mut job = d2o_job();
        job.max_steps = Some(12);
        assert_eq!(options(&job).max_steps, 12);

        job.optimize = false;
        job.thermo.temperature_k = Some(310.0);
        job.thermo.pressure_pa = Some(101_000.0);
        job.properties = Some(Properties {
            gradient: true,
            bond_orders: false,
            orbitals: OrbitalOutput::None,
        });
        let request = final_request(&job, None);
        assert!(request.gradient);
        assert!(!request.bond_orders);
        assert_eq!(request.orbitals, OrbitalOutput::None);
        let hessian = request.hessian.unwrap();
        assert_eq!(hessian.masses_amu, job.masses);
        assert_eq!(hessian.thermo.temperature_k, Some(310.0));
        assert_eq!(hessian.thermo.pressure_pa, Some(101_000.0));
        assert_eq!(hessian.thermo.symmetry_number, Some(2));

        job.thermo.symmetry_number = Some(1);
        let hessian = final_request(&job, None).hessian.unwrap();
        assert_eq!(hessian.thermo.symmetry_number, Some(1));
    }

    #[test]
    fn worker_failures_keep_spawn_and_protocol_diagnostics() {
        let cases = [
            (
                Error::Spawn(std::io::Error::from(std::io::ErrorKind::NotFound)),
                "could not start the xTB worker",
            ),
            (
                Error::Protocol("reply ID did not match".into()),
                "xTB worker protocol error",
            ),
        ];
        for (error, summary) in cases {
            let diagnostic = error.to_string();
            let failure = worker_failure(error);
            assert_eq!(failure.to_string(), summary);
            assert!(matches!(
                failure,
                Failure::Worker { error, .. } if error.to_string() == diagnostic
            ));
        }
    }

    #[cfg(unix)]
    #[test]
    fn unexpected_worker_exit_keeps_stderr() {
        use std::{os::unix::process::ExitStatusExt, process::ExitStatus};

        let status = ExitStatus::from_raw(1 << 8);
        let failure = worker_failure(Error::Exited {
            status,
            stderr: "worker stderr".into(),
        });
        assert_eq!(
            failure.to_string(),
            format!("xTB worker exited with {status}")
        );
        assert!(matches!(
            failure,
            Failure::Worker { error, .. }
                if error.to_string() == format!("xTB worker exited with {status}: worker stderr")
        ));
    }

    #[test]
    fn hessian_limit_keeps_the_client_error_and_card_reason() {
        let message = format!(
            "expected at most {} Hessian atoms",
            protocol::MAX_HESSIAN_ATOMS
        );
        let failure = worker_failure(Error::Limit(message.clone()));
        assert_eq!(failure.to_string(), message);
        assert!(matches!(failure, Failure::Worker { error, .. }
            if matches!(error.as_ref(), Error::Limit(detail) if detail == &message)
                && error.to_string() == format!("xTB resource limit: {message}")));
    }

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
        assert!(push(1, -2.1, 99).is_none());
        assert!(
            matches!(push(2, -3., 100), Some(Event::Steps { first: 1, energies, positions })
            if energies == [-2.1, -3.] && positions == [[-3., 0., 0.]])
        );
        assert!(push(3, -4., 100).is_none());
        assert!(
            matches!(batch.flush(), Some(Event::Steps { first: 3, energies, .. })
            if energies == [-4.])
        );
        assert!(batch.flush().is_none());
    }
}
