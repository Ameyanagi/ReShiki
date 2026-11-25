//! Run the optional external worker without holding the Python GIL.

use crate::qc::{GeometryData, computed_data};
use pyo3::prelude::*;
use reshiki_optimize::{
    ANGSTROM_PER_BOHR, Accuracy, EvalError, Level, Options, Potential, Status, optimize,
};
use reshiki_xtb_worker::{
    client::{self, Cancel, Error, Limits, Session},
    protocol::{
        Compute, Computed, Conventions, Engine, ErrorKind, HessianRequest, MAX_HESSIAN_ATOMS,
        Method, Open, OrbitalOutput, ScfSettings, Solvent, SolventCorrection, SolventModel,
        ThermoRequest, Units,
    },
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    ops::ControlFlow,
    path::PathBuf,
    sync::mpsc::{self, RecvTimeoutError},
    thread,
    time::Duration,
};

#[derive(Deserialize)]
struct Calculation {
    geometry: GeometryData,
    method: String,
    conventions: Conventions,
    solvent: Option<String>,
    solvent_model: SolventModel,
    electronic_temperature: Option<f64>,
    accuracy: Option<f64>,
    max_iterations: Option<u32>,
    threads: u32,
    request: Request,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum OptimizationLevel {
    Normal,
    Tight,
}

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "lowercase")]
enum Request {
    Energy {
        gradient: bool,
        bond_orders: bool,
        orbitals: bool,
    },
    Optimize {
        level: OptimizationLevel,
        max_steps: Option<usize>,
        fixed_atoms: Vec<usize>,
    },
    Frequencies {
        temperature: f64,
        pressure: f64,
    },
}

#[pyfunction]
pub fn xtb_engine(py: Python<'_>, executable: PathBuf) -> PyResult<String> {
    run_with_worker(py, executable, |_, _, engine| Ok(json!(engine)))
}

#[pyfunction]
pub fn xtb_calculate(py: Python<'_>, executable: PathBuf, request_json: &str) -> PyResult<String> {
    let calculation: Calculation = serde_json::from_str(request_json)
        .map_err(|error| crate::invalid(py)(format!("Invalid xTB settings: {error}")))?;
    let method = parse_method(&calculation.method).map_err(crate::invalid(py))?;
    run_with_worker(py, executable, move |session, cancel, engine| {
        let program = format!("{} {}", engine.name, engine.version);
        calculation.run(session, cancel, method, &program)
    })
}

fn run_with_worker(
    py: Python<'_>,
    executable: PathBuf,
    run: impl FnOnce(&mut Session, &Cancel, Engine) -> client::Result<Value> + Send + 'static,
) -> PyResult<String> {
    let cancel = Cancel::default();
    let worker_cancel = cancel.clone();
    let (sender, mut receiver) = mpsc::channel();
    let worker = thread::Builder::new()
        .name("reshiki-xtb".into())
        .spawn(move || {
            let result: client::Result<Value> = (|| {
                let (mut session, hello) =
                    Session::start(&executable, Limits::default(), &worker_cancel)?;
                let engine = hello.engine.ok_or(Error::Unavailable)?;
                let result = run(&mut session, &worker_cancel, engine)?;
                // The result is complete. A worker that fails to close is still reaped.
                let _ = session.close(&worker_cancel);
                Ok(result)
            })();
            let json = result
                .map_err(|error| error.to_string())
                .and_then(|result| {
                    serde_json::to_string(&result).map_err(|error| error.to_string())
                });
            let _ = sender.send(json);
        })
        .map_err(|error| crate::failed(py)(format!("Could not start the xTB thread: {error}")))?;
    let received = loop {
        let poll = &mut receiver;
        let received = py.detach(move || poll.recv_timeout(Duration::from_millis(100)));
        if let Err(error) = py.check_signals() {
            cancel.cancel();
            let _ = py.detach(move || worker.join());
            return Err(error);
        }
        if !matches!(received, Err(RecvTimeoutError::Timeout)) {
            break received;
        }
    };
    py.detach(move || worker.join())
        .map_err(|_| crate::failed(py)("The xTB thread stopped unexpectedly".into()))?;
    received
        .map_err(|_| crate::failed(py)("The xTB thread returned no result".into()))?
        .map_err(crate::failed(py))
}

fn parse_method(method: &str) -> Result<Method, String> {
    match method.to_ascii_lowercase().as_str() {
        "gfn2" | "gfn2-xtb" => Ok(Method::Gfn2),
        "gfnff" | "gfn-ff" => Ok(Method::Gfnff),
        "gxtb" | "g-xtb" => Ok(Method::Gxtb),
        _ => Err(format!(
            "Unknown xTB method {method:?}. Use gfn2, gfnff or gxtb."
        )),
    }
}

impl Calculation {
    fn run(
        self,
        session: &mut Session,
        cancel: &Cancel,
        method: Method,
        program: &str,
    ) -> client::Result<Value> {
        let mut geometry = self.geometry;
        let uhf = geometry
            .multiplicity
            .checked_sub(1)
            .ok_or_else(|| Error::Input("Multiplicity must be positive".into()))?;
        session.open(
            &Open {
                numbers: geometry.numbers.clone(),
                positions: geometry.positions.clone(),
                units: Units::Angstrom,
                charge: geometry.charge,
                uhf: Some(uhf),
                method,
                conventions: self.conventions,
                solvent: self.solvent.map(|name| Solvent {
                    model: self.solvent_model,
                    name: name.to_lowercase(),
                    state: Default::default(),
                    correction: (method == Method::Gxtb).then_some(SolventCorrection::Gfn2),
                }),
                scf: ScfSettings {
                    accuracy: self.accuracy,
                    electronic_temperature_k: self.electronic_temperature,
                    max_iterations: self.max_iterations,
                    retries: vec![],
                },
                gfnff: None,
                threads: self.threads,
                heap_bytes: Limits::default().heap_bytes,
            },
            cancel,
        )?;

        let mut optimization = None;
        let request = match self.request {
            Request::Energy {
                gradient,
                bond_orders,
                orbitals,
            } => Compute {
                gradient,
                bond_orders,
                orbitals: orbital_levels(orbitals),
                ..Compute::default()
            },
            Request::Frequencies {
                temperature,
                pressure,
            } => Compute {
                hessian: Some(HessianRequest {
                    thermo: ThermoRequest {
                        temperature_k: Some(temperature),
                        pressure_pa: Some(pressure),
                        ..ThermoRequest::default()
                    },
                    ..HessianRequest::default()
                }),
                ..Compute::default()
            },
            Request::Optimize {
                level,
                max_steps,
                fixed_atoms,
            } => {
                let mut options = Options {
                    level: match level {
                        OptimizationLevel::Normal => Level::Normal,
                        OptimizationLevel::Tight => Level::Tight,
                    },
                    fixed_atoms,
                    ..Options::default()
                };
                if let Some(max_steps) = max_steps {
                    options.max_steps = max_steps;
                }
                let mut trajectory = Vec::new();
                let positions = geometry
                    .positions
                    .as_flattened()
                    .iter()
                    .map(|coordinate| coordinate / ANGSTROM_PER_BOHR)
                    .collect::<Vec<_>>();
                let mut xtb = Xtb {
                    session,
                    cancel,
                    failure: None,
                };
                let outcome = optimize(&mut xtb, &geometry.numbers, &positions, &options, |step| {
                    trajectory.truncate(step.index);
                    trajectory.push(angstroms(step.positions));
                    ControlFlow::Continue(())
                });
                match outcome.status {
                    Status::Converged | Status::MaxSteps => {}
                    Status::ScfFailure(message) => {
                        return Err(Error::Engine {
                            kind: ErrorKind::Scf,
                            message,
                        });
                    }
                    Status::Failed(message) => {
                        return Err(xtb.failure.unwrap_or(Error::Input(message)));
                    }
                    Status::Cancelled => return Err(Error::Cancelled),
                }
                geometry.positions = angstroms(&outcome.positions);
                let capabilities = method.xtb_method().capabilities();
                let request = Compute {
                    positions: Some(outcome.positions.as_chunks::<3>().0.to_vec()),
                    units: Units::Bohr,
                    gradient: true,
                    bond_orders: capabilities.bond_orders,
                    orbitals: orbital_levels(capabilities.orbitals),
                    ..Compute::default()
                };
                optimization = Some((outcome, trajectory));
                request
            }
        };
        let computed = session.compute(&request, cancel)?;
        let mut result = computed_data(&computed, &geometry, method.xtb_method().label(), program);
        if let Some((mut outcome, trajectory)) = optimization {
            let converged = outcome.status == Status::Converged;
            if let Some(last) = outcome.energies.last_mut() {
                *last = computed.energy;
            }
            if !converged {
                result["warnings"] = json!(["Optimization reached the step limit."]);
            }
            result["converged"] = json!(converged);
            result["steps"] = json!(outcome.steps);
            result["energies"] = json!(outcome.energies);
            result["trajectory"] = json!(trajectory);
        }
        Ok(result)
    }
}

fn orbital_levels(requested: bool) -> OrbitalOutput {
    if requested {
        OrbitalOutput::Levels
    } else {
        OrbitalOutput::None
    }
}

fn angstroms(positions: &[f64]) -> Vec<[f64; 3]> {
    positions
        .as_chunks::<3>()
        .0
        .iter()
        .map(|xyz| xyz.map(|coordinate| coordinate * ANGSTROM_PER_BOHR))
        .collect()
}

struct Xtb<'a> {
    session: &'a mut Session,
    cancel: &'a Cancel,
    failure: Option<Error>,
}

impl Xtb<'_> {
    fn compute(&mut self, request: &Compute) -> Result<Computed, EvalError> {
        self.failure = None;
        self.session.compute(request, self.cancel).map_err(|error| {
            let status = match &error {
                Error::Cancelled => EvalError::Cancelled,
                Error::Engine {
                    kind: ErrorKind::Scf,
                    message,
                } => EvalError::Scf(message.clone()),
                error => EvalError::Failed(error.to_string()),
            };
            self.failure = Some(error);
            status
        })
    }
}

impl Potential for Xtb<'_> {
    fn evaluate(
        &mut self,
        positions: &[f64],
        gradient: &mut [f64],
        _accuracy: Accuracy,
    ) -> Result<f64, EvalError> {
        let computed = self.compute(&Compute {
            positions: Some(positions.as_chunks::<3>().0.to_vec()),
            gradient: true,
            ..Compute::default()
        })?;
        let rows = computed
            .gradient
            .ok_or_else(|| EvalError::Failed("The xTB worker returned no gradient".into()))?;
        gradient.copy_from_slice(rows.as_flattened());
        Ok(computed.energy)
    }

    fn hessian(&mut self, positions: &[f64], out: &mut [f64]) -> Option<Result<(), EvalError>> {
        if positions.len() / 3 > MAX_HESSIAN_ATOMS {
            return None;
        }
        let request = Compute {
            positions: Some(positions.as_chunks::<3>().0.to_vec()),
            hessian: Some(HessianRequest::default()),
            ..Compute::default()
        };
        Some(
            self.compute(&request)
                .map(|computed| out.copy_from_slice(&computed.hessian)),
        )
    }
}
