//! Request dispatch and error responses for the xTB worker.

use crate::protocol::{
    self, Closed, ErrorKind, Outcome, Request, RequestOp, Response, ResponseError,
};
use serde::Serialize;
use std::io::{self, Read, Write};

const MAX_ERROR_MESSAGE_BYTES: usize = 1024;

/// Run the worker on stdin and stdout, returning the worker's exit code.
pub fn serve() -> i32 {
    let mut worker = Worker::default();
    #[cfg(all(unix, feature = "engine"))]
    {
        worker.lapack_error = clear_lapack_overrides();
    }
    worker.serve_io(&mut io::stdin().lock(), &mut io::stdout().lock())
}

#[cfg(all(unix, feature = "engine"))]
fn clear_lapack_overrides() -> Option<String> {
    use std::{env, os::unix::process::CommandExt, process::Command};

    if env::var_os("AXTB_LAPACK").is_none() && env::var_os("AXTB_LAPACK_BLAS").is_none() {
        return None;
    }
    // exec only returns on failure. Inherited pipes and resource limits carry
    // over to the new image of the worker.
    let error = match env::current_exe() {
        Ok(executable) => Command::new(executable)
            .args(env::args_os().skip(1))
            .env_remove("AXTB_LAPACK")
            .env_remove("AXTB_LAPACK_BLAS")
            .exec(),
        Err(error) => error,
    };
    Some(format!(
        "could not restart the xTB worker without AXTB_LAPACK and AXTB_LAPACK_BLAS: {error}"
    ))
}

#[derive(Default)]
pub(crate) struct Worker {
    #[cfg(all(unix, feature = "engine"))]
    lapack_error: Option<String>,
    #[cfg(feature = "engine")]
    opened: bool,
    #[cfg(feature = "engine")]
    session: Option<crate::engine::Session>,
}

impl Worker {
    pub(crate) fn serve_io(&mut self, reader: &mut impl Read, writer: &mut impl Write) -> i32 {
        let mut last_id = 0;
        loop {
            let request: Request = match protocol::read_frame(reader) {
                Ok(Some(request)) => request,
                Ok(None) => return 0,
                Err(error) => {
                    let _ = protocol_error(writer, 0, error.to_string());
                    return 2;
                }
            };
            if request.id <= last_id {
                let _ = protocol_error(writer, 0, "request ids must be positive and increasing");
                return 2;
            }
            last_id = request.id;
            match self.respond(request, writer) {
                Ok(true) => return 0,
                Ok(false) => {}
                Err(_) => return 1,
            }
        }
    }

    #[cfg(feature = "engine")]
    fn respond(
        &mut self,
        request: Request,
        writer: &mut impl Write,
    ) -> Result<bool, protocol::FrameError> {
        let id = request.id;
        match request.op {
            RequestOp::Hello {} => respond(writer, id, Ok(crate::engine::hello()))?,
            RequestOp::Open(open) => {
                let result = (|| {
                    #[cfg(unix)]
                    if let Some(message) = &self.lapack_error {
                        return Err(ResponseError::new(ErrorKind::Unavailable, message));
                    }
                    if std::mem::replace(&mut self.opened, true) {
                        return Err(ResponseError::new(
                            ErrorKind::State,
                            "open is allowed once per worker",
                        ));
                    }
                    let heap_bytes = open.heap_budget()?;
                    let (session, opened) = crate::engine::Session::open(open, heap_bytes)?;
                    self.session = Some(session);
                    Ok(opened)
                })();
                respond(writer, id, result)?;
            }
            RequestOp::Compute(compute) => {
                let result = (|| {
                    let session = self.session.as_mut().ok_or_else(|| {
                        ResponseError::new(ErrorKind::State, "compute requires an open session")
                    })?;
                    session.compute(compute)
                })();
                match result {
                    Ok(computed) => {
                        respond(writer, id, Ok(&computed))?;
                        for array in &computed.arrays {
                            protocol::write_array(writer, computed.array_values(array.name))?;
                        }
                        writer.flush()?;
                    }
                    Err(error) => respond::<Closed>(writer, id, Err(error))?,
                }
            }
            RequestOp::Close {} => {
                respond(writer, id, Ok(Closed {}))?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    #[cfg(not(feature = "engine"))]
    fn respond(
        &mut self,
        request: Request,
        writer: &mut impl Write,
    ) -> Result<bool, protocol::FrameError> {
        if let RequestOp::Hello {} = request.op {
            respond(
                writer,
                request.id,
                Ok(protocol::Hello {
                    protocol: protocol::VERSION,
                    engine: None,
                    methods: vec![],
                    max_atoms: protocol::MAX_ATOMS,
                    max_hessian_atoms: protocol::MAX_HESSIAN_ATOMS,
                }),
            )?;
        } else {
            respond::<Closed>(
                writer,
                request.id,
                Err(ResponseError::new(
                    ErrorKind::Unavailable,
                    "xTB is not included in this build",
                )),
            )?;
        }
        Ok(false)
    }
}

fn protocol_error(
    writer: &mut impl Write,
    id: u64,
    message: impl Into<String>,
) -> Result<(), protocol::FrameError> {
    respond::<Closed>(
        writer,
        id,
        Err(ResponseError::new(ErrorKind::Protocol, message)),
    )
}

fn respond<T: Serialize>(
    writer: &mut impl Write,
    id: u64,
    result: Result<T, ResponseError>,
) -> Result<(), protocol::FrameError> {
    let frame = match result {
        Ok(ok) => protocol::encode_frame(&Response {
            id,
            result: Outcome::Ok { ok },
        }),
        Err(mut error) => {
            let kind = error.kind;
            error
                .message
                .truncate(error.message.floor_char_boundary(MAX_ERROR_MESSAGE_BYTES));
            let encode = |error| {
                protocol::encode_frame(&Response::<Closed> {
                    id,
                    result: Outcome::Error { error },
                })
            };
            match encode(error) {
                Err(protocol::FrameError::Length) => encode(ResponseError::new(
                    kind,
                    "worker error message exceeds frame limit",
                )),
                result => result,
            }
        }
    }?;
    writer.write_all(&frame)?;
    writer.flush()?;
    Ok(())
}
