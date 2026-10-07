//! The operation tool catalog and the argument decoder for each tool.
use super::{
    analyze::{self, Analyze},
    budget::Budgets,
    documents::{self, Close},
    error::{ErrorKind, OpError},
    export::{self, Export},
    import::{self, Import},
    inspect::{self, Inspect},
    render::{self, Render},
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
    import::IMPORT,
    inspect::INSPECT,
    analyze::ANALYZE,
    render::RENDER,
    export::EXPORT,
];

/// A tool call with decoded arguments.
pub(crate) enum Op {
    Info,
    DocumentNew,
    DocumentList,
    DocumentClose(Close),
    Import(Import),
    Inspect(Inspect),
    Analyze(Analyze),
    Render(Render),
    Export(Export),
}

/// Decodes `args` for `tool`, with every input budget check, before any work
/// runs. `None` when no tool has that name.
pub(crate) fn decode(tool: &str, args: Value, budgets: &Budgets) -> Option<Result<Op, OpError>> {
    Some(match tool {
        "info" => documents::decode_none(args).map(|()| Op::Info),
        "document_new" => documents::decode_none(args).map(|()| Op::DocumentNew),
        "document_list" => documents::decode_none(args).map(|()| Op::DocumentList),
        "document_close" => documents::decode_close(args).map(Op::DocumentClose),
        "import" => import::decode(args, budgets).map(Op::Import),
        "inspect" => inspect::decode(args, budgets).map(Op::Inspect),
        "analyze" => analyze::decode(args, budgets).map(Op::Analyze),
        "render" => render::decode(args, budgets).map(Op::Render),
        "export" => export::decode(args).map(Op::Export),
        _ => return None,
    })
}

/// Whether `args` decode for `tool` under the default [`Budgets`], for the
/// schema and decoder contract tests. `Err` carries the decoder's message.
#[doc(hidden)]
pub fn decode_check(tool: &str, args: Value) -> Result<(), String> {
    match decode(tool, args, &Budgets::default()) {
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
