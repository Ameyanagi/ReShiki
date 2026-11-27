//! A sendable potential backed by a thread-affine xTB session.

use reshiki_optimize::{Accuracy, EvalError, Potential};
use reshiki_xtb_worker::{
    client::{Cancel, Error, Limits, Session},
    protocol::{Compute, Computed, Engine, ErrorKind, HessianRequest, Open, ThermoRequest, Units},
};
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
    thread::{self, JoinHandle},
};

pub struct Worker {
    requests: Option<Sender<Compute>>,
    results: Receiver<Result<Computed, Error>>,
    thread: Option<JoinHandle<()>>,
    pub gradients: usize,
    pub evaluations: Vec<Evaluation>,
}

pub struct HessianAnalysis {
    pub frequencies_cm1: Vec<f64>,
    pub hessian: Vec<f64>,
}

/// One completed worker request, retained without its large property arrays.
pub struct Evaluation {
    pub energy: f64,
    pub scf_iterations: usize,
    pub seconds: f64,
}

impl Worker {
    #[cfg(test)]
    pub(crate) fn with_transport(
        requests: Sender<Compute>,
        results: Receiver<Result<Computed, Error>>,
    ) -> Self {
        Self {
            requests: Some(requests),
            results,
            thread: None,
            gradients: 0,
            evaluations: Vec::new(),
        }
    }

    pub fn start(
        executable: PathBuf,
        open: Open,
        limits: Limits,
        cancel: Cancel,
    ) -> Result<(Self, Engine, u32), EvalError> {
        let (requests, incoming) = mpsc::channel();
        let (outgoing, results) = mpsc::channel();
        let (ready, started) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("ts-xtb".into())
            .spawn(move || {
                let startup = (|| {
                    let (mut session, hello) = Session::start(&executable, limits, &cancel)?;
                    let engine = hello.engine.ok_or(Error::Unavailable)?;
                    let opened = session.open(&open, &cancel)?;
                    Ok((session, engine, opened.uhf))
                })();
                match startup {
                    Ok((mut session, engine, uhf)) => {
                        if ready.send(Ok((engine, uhf))).is_ok() {
                            for request in incoming {
                                if outgoing.send(session.compute(&request, &cancel)).is_err() {
                                    break;
                                }
                            }
                        }
                        let _ = session.close(&cancel);
                    }
                    Err(error) => {
                        let _ = ready.send(Err(error));
                    }
                }
            })
            .map_err(|error| {
                EvalError::Failed(format!("cannot start xTB session thread: {error}"))
            })?;
        let (engine, uhf) = started.recv().map_err(disconnected)?.map_err(eval_error)?;
        Ok((
            Self {
                requests: Some(requests),
                results,
                thread: Some(thread),
                gradients: 0,
                evaluations: Vec::new(),
            },
            engine,
            uhf,
        ))
    }

    fn compute(&mut self, request: Compute) -> Result<Computed, EvalError> {
        self.requests
            .as_ref()
            .ok_or_else(|| disconnected("closed"))?
            .send(request)
            .map_err(disconnected)?;
        let computed = self
            .results
            .recv()
            .map_err(disconnected)?
            .map_err(eval_error)?;
        self.evaluations.push(Evaluation {
            energy: computed.energy,
            scf_iterations: computed.scf.as_ref().map_or(0, |scf| scf.iterations),
            seconds: computed.seconds,
        });
        Ok(computed)
    }

    fn analyze_hessian(
        &mut self,
        positions: &[f64],
        request: HessianRequest,
    ) -> Result<Computed, EvalError> {
        self.compute(Compute {
            positions: Some(positions.as_chunks::<3>().0.to_vec()),
            units: Units::Bohr,
            hessian: Some(request),
            ..Compute::default()
        })
    }

    pub fn checked_hessian(
        &mut self,
        positions: &[f64],
        masses_amu: &[f64],
    ) -> Result<Computed, EvalError> {
        let computed = self.analyze_hessian(
            positions,
            HessianRequest {
                masses_amu: Some(masses_amu.to_vec()),
                thermo: ThermoRequest {
                    symmetry_number: Some(1),
                    ..ThermoRequest::default()
                },
            },
        )?;
        if computed.vibrations.is_none() {
            return Err(EvalError::Failed(
                "the xTB worker returned no vibrational analysis".into(),
            ));
        }
        Ok(computed)
    }

    pub fn check_hessian(
        &mut self,
        positions: &[f64],
        masses_amu: &[f64],
    ) -> Result<HessianAnalysis, EvalError> {
        let computed = self.checked_hessian(positions, masses_amu)?;
        let vibrations = computed.vibrations.ok_or_else(|| {
            EvalError::Failed("the xTB worker returned no vibrational analysis".into())
        })?;
        Ok(HessianAnalysis {
            frequencies_cm1: vibrations.frequencies_cm1,
            hessian: computed.hessian,
        })
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        // Close the request channel before joining, so an idle session can
        // close and reap its process. Active requests share the caller's Cancel.
        self.requests.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Potential for Worker {
    fn evaluate(
        &mut self,
        positions: &[f64],
        gradient: &mut [f64],
        _accuracy: Accuracy,
    ) -> Result<f64, EvalError> {
        self.gradients += 1;
        let computed = self.compute(Compute {
            positions: Some(positions.as_chunks::<3>().0.to_vec()),
            units: Units::Bohr,
            gradient: true,
            ..Compute::default()
        })?;
        let rows = computed
            .gradient
            .filter(|rows| rows.as_flattened().len() == gradient.len())
            .ok_or_else(|| {
                EvalError::Failed("the xTB worker returned an invalid gradient".into())
            })?;
        gradient.copy_from_slice(rows.as_flattened());
        Ok(computed.energy)
    }

    fn hessian(&mut self, positions: &[f64], out: &mut [f64]) -> Option<Result<(), EvalError>> {
        Some(
            self.analyze_hessian(positions, HessianRequest::default())
                .and_then(|computed| {
                    if computed.hessian.len() != out.len() {
                        return Err(EvalError::Failed(
                            "the xTB worker returned an invalid Hessian".into(),
                        ));
                    }
                    out.copy_from_slice(&computed.hessian);
                    Ok(())
                }),
        )
    }
}

fn disconnected(_: impl std::fmt::Display) -> EvalError {
    EvalError::Failed("the xTB session thread disconnected".into())
}

fn eval_error(error: Error) -> EvalError {
    match error {
        Error::Cancelled => EvalError::Cancelled,
        Error::Engine {
            kind: ErrorKind::Scf,
            message,
        } => EvalError::Scf(message),
        Error::Unsupported(message) => EvalError::Failed(message),
        error => EvalError::Failed(error.to_string()),
    }
}
