//! The operation tool catalog and the argument decoder for each tool.
use super::{
    documents::{self, Close},
    error::{ErrorKind, OpError},
};
use crate::tool_spec::ToolSpec;
use serde::de::DeserializeOwned;
use serde_json::Value;

/// The operation tools, in the fixed order hosts list them.
pub const SPECS: &[ToolSpec] = &[
    documents::INFO,
    documents::NEW,
    documents::LIST,
    documents::CLOSE,
];

/// A tool call with decoded arguments.
pub(crate) enum Op {
    Info,
    DocumentNew,
    DocumentList,
    DocumentClose(Close),
}

/// Decodes `args` for `tool`, with every input budget check, before any work
/// runs. `None` when no tool has that name.
pub(crate) fn decode(tool: &str, args: Value) -> Option<Result<Op, OpError>> {
    Some(match tool {
        "info" => documents::decode_none(args).map(|()| Op::Info),
        "document_new" => documents::decode_none(args).map(|()| Op::DocumentNew),
        "document_list" => documents::decode_none(args).map(|()| Op::DocumentList),
        "document_close" => documents::decode_close(args).map(Op::DocumentClose),
        _ => return None,
    })
}

/// Whether `args` decode for `tool`, for the schema and decoder contract
/// tests. `Err` carries the decoder's message.
#[doc(hidden)]
pub fn decode_check(tool: &str, args: Value) -> Result<(), String> {
    match decode(tool, args) {
        None => Err(format!("Unknown tool {tool}")),
        Some(decoded) => decoded.map(drop).map_err(|error| error.message),
    }
}

/// Decodes a tool's arguments, which must be a JSON object. serde also reads
/// structs from JSON arrays, which the schemas forbid.
pub(crate) fn arguments<T: DeserializeOwned>(args: Value) -> Result<T, OpError> {
    if !args.is_object() {
        return Err(OpError::new(
            ErrorKind::InvalidArguments,
            "Tool arguments must be a JSON object",
        ));
    }
    serde_json::from_value(args)
        .map_err(|error| OpError::new(ErrorKind::InvalidArguments, error.to_string()))
}
