use super::{Error, MAX_REQUEST_BYTES, Output, Status};
use crate::chemistry::inchi::{
    kernel::{self, Molecule},
    output, wire,
};

pub(super) fn encode(input: &Molecule, heap_bytes: usize) -> Result<Vec<u8>, Error> {
    super::super::input::prepare(&input.state, input.positions.as_deref())
        .map_err(|_| Error::Input("Invalid molecular graph, annotations or coordinates"))?;
    let request = wire::Request {
        heap_bytes,
        operation: wire::Operation::Generate(Box::new(input.clone())),
    };
    wire::encode(&request, MAX_REQUEST_BYTES).map_err(|_| Error::Limit("request"))
}

pub(super) fn encode_import(
    inchi: &str,
    heap_bytes: usize,
    options: output::Options,
) -> Result<Vec<u8>, Error> {
    if inchi.len() > super::MAX_INCHI_BYTES {
        return Err(Error::Limit("InChI text"));
    }
    wire::encode(
        &wire::Request {
            heap_bytes,
            operation: wire::Operation::Read {
                inchi: inchi.into(),
                options,
            },
        },
        MAX_REQUEST_BYTES,
    )
    .map_err(|_| Error::Limit("request"))
}

fn reply(bytes: &[u8]) -> Result<wire::Reply, Error> {
    let response: wire::Response = wire::decode(bytes, super::MAX_RESPONSE_BYTES)
        .map_err(|_| Error::Protocol("Invalid response frame"))?;
    if response.version != kernel::VERSION {
        return Err(Error::Version(response.version));
    }
    response.result.map_err(Error::Rejected)
}
fn strings(values: &[&str]) -> Result<(), Error> {
    if values.iter().any(|v| v.len() > super::MAX_INCHI_BYTES) {
        return Err(Error::Limit("response string"));
    }
    Ok(())
}
pub(super) fn decode(bytes: &[u8]) -> Result<Output, Error> {
    let wire::Reply::Generated(result) = reply(bytes)? else {
        return Err(Error::Protocol("Expected generation response"));
    };
    strings(&[
        &result.inchi,
        &result.message,
        &result.log,
        &result.auxiliary,
    ])?;
    if !result.inchi.is_empty() && !result.inchi.starts_with("InChI=1S/") {
        return Err(Error::Protocol("Expected a standard InChI"));
    }
    Ok(Output {
        status: Status::from_code(
            i16::try_from(result.status).map_err(|_| Error::Protocol("Invalid status"))?,
        )?,
        inchi: result.inchi,
        message: result.message,
        log: result.log,
        auxiliary: result.auxiliary,
        diagnostics: result.diagnostics,
    })
}
pub(super) fn decode_import(bytes: &[u8]) -> Result<kernel::Imported, Error> {
    let wire::Reply::Imported(result) = reply(bytes)? else {
        return Err(Error::Protocol("Expected import response"));
    };
    Status::from_code(
        i16::try_from(result.status).map_err(|_| Error::Protocol("Invalid status"))?,
    )?;
    strings(&[&result.message, &result.log])?;
    if let Some(state) = &result.state {
        super::super::input::prepare(state, None)
            .map_err(|_| Error::Protocol("Invalid imported molecule"))?;
        if result.unspecified_bonds.len() != state.graph.bonds.len() {
            return Err(Error::Protocol("Invalid imported bond identities"));
        }
    } else if !result.unspecified_bonds.is_empty() {
        return Err(Error::Protocol("Bond identities without a molecule"));
    }
    Ok(*result)
}
