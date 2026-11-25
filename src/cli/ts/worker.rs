//! A sendable potential backed by a thread-affine xTB session.

use reshiki_optimize::{Accuracy, EvalError, Potential};
use reshiki_xtb_worker::{
    client::{Cancel, Error, Limits, Session},
    protocol::{Compute, Computed, Engine, ErrorKind, HessianRequest, Open, ThermoRequest, Units},
};
use std::{
    path::Path,
    sync::mpsc::{self, Receiver, Sender},
    thread::{self, Scope},
};

pub(super) struct Worker {
    requests: Sender<Compute>,
    results: Receiver<Result<Computed, Error>>,
    pub(super) gradients: usize,
}

pub(super) struct HessianAnalysis {
    pub frequencies_cm1: Vec<f64>,
    pub hessian: Vec<f64>,
}

impl Worker {
    #[cfg(test)]
    pub(super) fn with_transport(
        requests: Sender<Compute>,
        results: Receiver<Result<Computed, Error>>,
    ) -> Self {
        Self {
            requests,
            results,
            gradients: 0,
        }
    }

    pub(super) fn start<'scope, 'env>(
        scope: &'scope Scope<'scope, 'env>,
        executable: &'env Path,
        open: &'env Open,
        limits: Limits,
    ) -> Result<(Self, Engine, u32), EvalError> {
        let (requests, incoming) = mpsc::channel();
        let (outgoing, results) = mpsc::channel();
        let (ready, started) = mpsc::channel();
        thread::Builder::new()
            .name("ts-xtb".into())
            .spawn_scoped(scope, move || {
                let cancel = Cancel::default();
                let startup = (|| {
                    let (mut session, hello) = Session::start(executable, limits, &cancel)?;
                    let engine = hello.engine.ok_or(Error::Unavailable)?;
                    let opened = session.open(open, &cancel)?;
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
                requests,
                results,
                gradients: 0,
            },
            engine,
            uhf,
        ))
    }

    fn compute(&mut self, request: Compute) -> Result<Computed, EvalError> {
        self.requests.send(request).map_err(disconnected)?;
        self.results
            .recv()
            .map_err(disconnected)?
            .map_err(eval_error)
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

    pub(super) fn check_hessian(
        &mut self,
        positions: &[f64],
        masses_amu: &[f64],
    ) -> Result<HessianAnalysis, EvalError> {
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
        let vibrations = computed.vibrations.ok_or_else(|| {
            EvalError::Failed("the xTB worker returned no vibrational analysis".into())
        })?;
        Ok(HessianAnalysis {
            frequencies_cm1: vibrations.frequencies_cm1,
            hessian: computed.hessian,
        })
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
