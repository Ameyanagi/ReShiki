//! Request dispatch and error responses for the xTB worker.

use crate::protocol::{
    self, Closed, ErrorKind, Outcome, Request, RequestOp, Response, ResponseError,
};
use serde::Serialize;
use std::io::{self, Read, Write};

const MAX_ERROR_MESSAGE_BYTES: usize = 1024;

/// Run the worker on stdin and stdout, returning the worker's exit code.
pub fn serve() -> i32 {
    serve_io(&mut io::stdin().lock(), &mut io::stdout().lock())
}

pub(crate) fn serve_io(reader: &mut impl Read, writer: &mut impl Write) -> i32 {
    let mut worker = Worker::default();
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
        match worker.respond(request, writer) {
            Ok(true) => return 0,
            Ok(false) => {}
            Err(_) => return 1,
        }
    }
}

#[derive(Default)]
struct Worker {
    #[cfg(feature = "engine")]
    opened: bool,
    #[cfg(feature = "engine")]
    session: Option<crate::engine::Session>,
}

impl Worker {
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
                    if std::mem::replace(&mut self.opened, true) {
                        return Err(ResponseError::new(
                            ErrorKind::State,
                            "open is allowed once per worker",
                        ));
                    }
                    let heap_bytes = open.validate()?;
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
                    if let Some(positions) = &compute.positions {
                        protocol::validate_positions(positions, session.atoms())?;
                    }
                    session.compute(compute)
                })();
                respond(writer, id, result)?;
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
                    operations: vec![],
                    max_atoms: protocol::MAX_ATOMS,
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
