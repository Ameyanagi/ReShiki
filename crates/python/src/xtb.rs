//! Run the optional external worker without holding the Python GIL.

mod conformers;
pub use conformers::xtb_conformers;

use crate::qc::{GeometryData, computed_data};
use pyo3::prelude::*;
use reshiki_model::science::isotope_masses;
use reshiki_xtb_jobs::run::{self, Failure, Job, Properties, Report};
use reshiki_xtb_worker::{
    client::{Cancel, Error, Limits, Session},
    protocol::{
        Conventions, Method, OrbitalOutput, ScfSettings, Solvent, SolventCorrection, SolventModel,
        ThermoRequest,
    },
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::mpsc::{self, RecvTimeoutError},
    thread,
    time::Duration,
};

#[derive(Deserialize)]
struct WorkerSettings {
    method: String,
    unpaired: Option<u32>,
    conventions: Conventions,
    solvent: Option<String>,
    solvent_model: SolventModel,
    electronic_temperature: Option<f64>,
    accuracy: Option<f64>,
    max_iterations: Option<u32>,
    threads: u32,
}

impl WorkerSettings {
    fn method(&self) -> Result<Method, String> {
        let method = &self.method;
        match method.to_ascii_lowercase().as_str() {
            "gfn2" | "gfn2-xtb" => Ok(Method::Gfn2),
            "gfnff" | "gfn-ff" => Ok(Method::Gfnff),
            "gxtb" | "g-xtb" => Ok(Method::Gxtb),
            _ => Err(format!(
                "Unknown xTB method {method:?}. Use gfn2, gfnff or gxtb."
            )),
        }
    }

    fn job(self, geometry: &GeometryData, uhf: u32, method: Method) -> Job {
        Job {
            optimize: false,
            frequencies: false,
            masses: None,
            numbers: geometry.numbers.clone(),
            positions: geometry.positions.clone(),
            charge: geometry.charge,
            uhf,
            method,
            conventions: self.conventions,
            scf: ScfSettings {
                accuracy: self.accuracy,
                electronic_temperature_k: self.electronic_temperature,
                max_iterations: self.max_iterations,
                ..Default::default()
            },
            solvent: self.solvent.map(|name| Solvent {
                model: self.solvent_model,
                name: name.to_lowercase(),
                state: Default::default(),
                correction: (method == Method::Gxtb).then_some(SolventCorrection::Gfn2),
            }),
            threads: self.threads,
            fixed: vec![],
            tight: false,
            topology: None,
            max_steps: None,
            properties: None,
            thermo: Default::default(),
        }
    }
}

#[derive(Deserialize)]
struct Calculation {
    geometry: GeometryData,
    #[serde(flatten)]
    settings: WorkerSettings,
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
    run_interruptibly(py, move |cancel| {
        let (session, hello) = Session::start(&executable, Limits::default(), cancel)?;
        let engine = hello.engine.ok_or(Error::Unavailable)?;
        let _ = session.close(cancel);
        Ok(json!(engine).to_string())
    })?
    .map_err(|error: Error| crate::failed(py)(error.to_string()))
}

#[pyfunction]
pub fn xtb_calculate(py: Python<'_>, executable: PathBuf, request_json: &str) -> PyResult<String> {
    let calculation: Calculation = serde_json::from_str(request_json)
        .map_err(|error| crate::invalid(py)(format!("Invalid xTB settings: {error}")))?;
    let method = calculation.settings.method().map_err(crate::invalid(py))?;
    let mut geometry = calculation.geometry;
    let uhf = calculation
        .settings
        .unpaired
        .or_else(|| geometry.multiplicity.checked_sub(1))
        .ok_or_else(|| crate::failed(py)("xTB input: Multiplicity must be positive".into()))?;
    let mut job = calculation.settings.job(&geometry, uhf, method);
    match calculation.request {
        Request::Energy {
            gradient,
            bond_orders,
            orbitals,
        } => {
            job.properties = Some(Properties {
                gradient,
                bond_orders,
                orbitals: if orbitals {
                    OrbitalOutput::Levels
                } else {
                    OrbitalOutput::None
                },
            });
        }
        Request::Frequencies {
            temperature,
            pressure,
        } => {
            job.frequencies = true;
            job.masses = isotope_masses(&geometry.to_frame());
            job.thermo = ThermoRequest {
                temperature_k: Some(temperature),
                pressure_pa: Some(pressure),
                ..Default::default()
            };
            job.properties = Some(Properties {
                gradient: false,
                bond_orders: false,
                orbitals: OrbitalOutput::None,
            });
        }
        Request::Optimize {
            level,
            max_steps,
            fixed_atoms,
        } => {
            job.optimize = true;
            job.tight = matches!(level, OptimizationLevel::Tight);
            job.max_steps = max_steps;
            job.fixed = fixed_atoms;
        }
    }
    let optimize = job.optimize;
    run_interruptibly(py, move |cancel| {
        let report = run::run(&executable, job, cancel, |_| {})?;
        geometry.positions.clone_from(&report.positions);
        let result = if optimize {
            optimization_data(&report, &geometry)
        } else {
            report_data(&report, &geometry)
        };
        Ok(result.to_string())
    })?
    .map_err(|failure| crate::failed(py)(failure_message(&failure)))
}

fn report_data(report: &Report, geometry: &GeometryData) -> Value {
    let computed = &report.computed;
    let method = computed.method.xtb_method();
    let program = format!("{} {}", report.engine.name, report.engine.version);
    computed_data(computed, geometry, method.label(), &program)
}

fn optimization_data(report: &Report, geometry: &GeometryData) -> Value {
    let mut result = report_data(report, geometry);
    if !report.converged {
        result["warnings"] = json!(["Optimization reached the step limit."]);
    }
    let trajectory: Vec<_> = report
        .path
        .as_chunks::<3>()
        .0
        .chunks(geometry.numbers.len())
        .collect();
    result["converged"] = json!(report.converged);
    result["steps"] = json!(report.steps);
    result["energies"] = json!(report.energies);
    result["trajectory"] = json!(trajectory);
    result
}

fn failure_message(failure: &Failure) -> String {
    let message = failure.to_string();
    if matches!(failure, Failure::Spin { .. })
        || message.contains("same parity as the valence electron count")
    {
        return format!("{message}. Pass unpaired= with the number of unpaired valence electrons.");
    }
    message
}

fn run_interruptibly<T: Send + 'static>(
    py: Python<'_>,
    run: impl FnOnce(&Cancel) -> T + Send + 'static,
) -> PyResult<T> {
    let cancel = Cancel::default();
    let worker_cancel = cancel.clone();
    let (sender, mut receiver) = mpsc::channel();
    let worker = thread::Builder::new()
        .name("reshiki-xtb".into())
        .spawn(move || {
            let _ = sender.send(run(&worker_cancel));
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
    received.map_err(|_| crate::failed(py)("The xTB thread returned no result".into()))
}
