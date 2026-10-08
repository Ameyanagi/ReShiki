//! The protocol-neutral result of a tool call.
use super::{error::OpError, wire::versions_json};
use crate::envelope::Versions;
use serde_json::{Map, Value, json};

/// What a tool call produced. Transports map it to their own content types.
///
/// `value` is always a JSON object, so it is valid as MCP `structuredContent`
/// in every supported revision (2025-11-25 requires an object).
#[derive(Debug, Clone, PartialEq)]
pub struct ToolResult {
    pub value: Map<String, Value>,
    pub images: Vec<Image>,
    pub files: Vec<Blob>,
    /// A tool execution error: `value` is `{error: {code, message}, versions}`.
    pub is_error: bool,
}

/// An image for the client to look at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub mime: &'static str,
    pub bytes: Vec<u8>,
}

/// A file produced by the call, such as an export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blob {
    pub mime: &'static str,
    pub name: String,
    pub bytes: Vec<u8>,
}

impl ToolResult {
    /// The tool execution error result `{error: {code, message}, versions}`.
    pub fn error(error: &OpError, versions: &Versions) -> Self {
        Self {
            value: Map::from_iter([
                (
                    "error".to_owned(),
                    json!({"code": error.kind.code(), "message": error.message}),
                ),
                ("versions".to_owned(), versions_json(versions)),
            ]),
            images: Vec::new(),
            files: Vec::new(),
            is_error: true,
        }
    }
}
