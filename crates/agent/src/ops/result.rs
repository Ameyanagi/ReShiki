//! The protocol-neutral result of a tool call.
use super::{
    error::OpError,
    wire::{envelope_json, versions_json, warnings},
};
use crate::{
    envelope::{Envelope, ValidationStatus, Versions, Warning},
    transaction::Rejection,
};
use serde::Serialize;
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
    /// A tool execution error: `value` includes `{error: {code, message}, versions}`.
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
    /// A successful JSON object without an envelope or attached content.
    pub fn plain(value: Map<String, Value>) -> Self {
        Self {
            value,
            images: Vec::new(),
            files: Vec::new(),
            is_error: false,
        }
    }

    /// A successful `{value, warnings, validation, versions}` envelope.
    pub fn valid<T: Serialize>(value: T, warnings: Vec<Warning>, versions: Versions) -> Self {
        Self::plain(envelope_json(&Envelope {
            value,
            warnings,
            validation: ValidationStatus::Valid,
            versions,
        }))
    }

    /// Sets the files produced by this result, keeping its JSON and error flag.
    pub fn with_files(mut self, files: Vec<Blob>) -> Self {
        self.files = files;
        self
    }

    /// Sets the images produced by this result, keeping its JSON and error flag.
    pub fn with_images(mut self, images: Vec<Image>) -> Self {
        self.images = images;
        self
    }

    /// The tool execution error result `{error: {code, message}, versions}`.
    pub fn error(error: &OpError, versions: &Versions) -> Self {
        Self {
            is_error: true,
            ..Self::plain(Map::from_iter([
                ("error".to_owned(), error_json(error)),
                ("versions".to_owned(), versions_json(versions)),
            ]))
        }
    }

    /// A rejected edit: an error envelope with validation `rejected` and a
    /// `null` value, plus the `{error: {code, message}}` of every tool error.
    pub fn rejected(rejection: Rejection, messages: Vec<String>, versions: &Versions) -> Self {
        let error = OpError::from(rejection.clone());
        let mut value = envelope_json(&Envelope {
            value: Value::Null,
            warnings: warnings(messages),
            validation: ValidationStatus::Rejected(rejection),
            versions: versions.clone(),
        });
        value.insert("error".to_owned(), error_json(&error));
        Self {
            is_error: true,
            ..Self::plain(value)
        }
    }
}

/// The common error member of plain tool errors and rejected envelopes.
fn error_json(error: &OpError) -> Value {
    json!({"code": error.kind.code(), "message": error.message})
}

#[cfg(test)]
mod tests;
