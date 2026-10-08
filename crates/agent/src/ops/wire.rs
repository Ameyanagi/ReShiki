//! Wire types and JSON Schema helpers for the operation API.
//!
//! # Required-nullable rule
//!
//! Every ops argument struct is `#[serde(deny_unknown_fields)]`, and every
//! nullable field carries `#[serde(deserialize_with = "Option::deserialize")]`.
//! Plain serde treats an omitted `Option` field as `None`; with
//! `deserialize_with` an omitted field is a serde error instead. That matches
//! the strict schemas, which list every property in `required` and wrap
//! nullable ones in [`nullable`], so callers pass `null` for an absent value.
//!
//! # Schemas
//!
//! The helpers use only the keywords and patterns of the agent schema
//! contract harness (`crates/agent/tests/support/json_schema.rs`).
use super::budget::MAX_IDS;
use crate::{
    envelope::{Envelope, ExportReceipt, IdRemap, ValidationStatus, Versions},
    transaction::Rejection,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use serde_json::{Map, Value, json};
use std::{fmt, sync::Arc};

const OBJECT_ID_MESSAGE: &str = "Object IDs are decimal strings such as \"12\"";
const REVISION_MESSAGE: &str = "Revisions are decimal strings such as \"0\" or \"12\"";
const HANDLE_MESSAGE: &str =
    "Document handles are 1 to 64 characters of A-Z, a-z, 0-9, \"_\" and \"-\"";

/// An object ID on the wire: a decimal string such as `"12"`.
///
/// It deserializes ONLY from a JSON string matching `^[1-9][0-9]{0,19}$` that
/// parses as a `u64` other than `u64::MAX`, the IDs `Document::validate`
/// accepts. JSON numbers are rejected because IDs above 2^53 occur and lose
/// precision as numbers in many clients.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObjectId(pub u64);

/// A document revision on the wire: a decimal string; `"0"` is allowed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Revision(pub u64);

/// An opaque document handle: 1 to 64 characters of `[A-Za-z0-9_-]`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DocHandle(String);

impl DocHandle {
    /// `None` unless `text` is 1 to 64 characters of `[A-Za-z0-9_-]`.
    pub fn new(text: impl Into<String>) -> Option<Self> {
        let text = text.into();
        valid_handle(&text).then_some(Self(text))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// `^[1-9][0-9]{0,19}$`, parsed as a `u64`.
fn decimal(text: &str) -> Option<u64> {
    let bytes = text.as_bytes();
    let matches = matches!(bytes.first(), Some(b'1'..=b'9'))
        && bytes.len() <= 20
        && bytes.iter().all(u8::is_ascii_digit);
    if matches { text.parse().ok() } else { None }
}

fn object_id(text: &str) -> Option<ObjectId> {
    decimal(text).filter(|id| *id != u64::MAX).map(ObjectId)
}

fn revision(text: &str) -> Option<Revision> {
    if text == "0" {
        Some(Revision(0))
    } else {
        decimal(text).map(Revision)
    }
}

fn handle(text: &str) -> Option<DocHandle> {
    DocHandle::new(text)
}

fn valid_handle(text: &str) -> bool {
    (1..=64).contains(&text.len())
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

/// Accepts only a string that `parse` accepts; every failure names `message`.
struct Text<T> {
    parse: fn(&str) -> Option<T>,
    message: &'static str,
}

impl<T> de::Visitor<'_> for Text<T> {
    type Value = T;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message)
    }

    fn visit_str<E: de::Error>(self, text: &str) -> Result<T, E> {
        (self.parse)(text).ok_or_else(|| E::custom(self.message))
    }
}

impl fmt::Display for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Display for Revision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Display for DocHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for ObjectId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl Serialize for Revision {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl Serialize for DocHandle {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ObjectId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_str(Text {
            parse: object_id,
            message: OBJECT_ID_MESSAGE,
        })
    }
}

impl<'de> Deserialize<'de> for Revision {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_str(Text {
            parse: revision,
            message: REVISION_MESSAGE,
        })
    }
}

impl<'de> Deserialize<'de> for DocHandle {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_str(Text {
            parse: handle,
            message: HANDLE_MESSAGE,
        })
    }
}

/// `{"type":"string","pattern":"^[1-9][0-9]{0,19}$"}`
pub fn object_id_schema() -> Value {
    json!({"type":"string","pattern":"^[1-9][0-9]{0,19}$"})
}

/// `{"type":"string","pattern":"^(0|[1-9][0-9]{0,19})$"}`
pub fn revision_schema() -> Value {
    json!({"type":"string","pattern":"^(0|[1-9][0-9]{0,19})$"})
}

/// `{"type":"string","pattern":"^[A-Za-z0-9_-]{1,64}$"}`
pub fn handle_schema() -> Value {
    json!({"type":"string","pattern":"^[A-Za-z0-9_-]{1,64}$"})
}

/// An array of at most [`MAX_IDS`] object IDs.
pub fn ids_schema() -> Value {
    json!({"type":"array","maxItems":MAX_IDS,"items":object_id_schema()})
}

/// `schema` or `null`.
pub fn nullable(schema: Value) -> Value {
    json!({"anyOf":[{"type":"null"}, schema]})
}

/// A JSON-RPC request ID, set by the transport. MCP request IDs are a string
/// or an integer (<https://modelcontextprotocol.io/specification/2026-07-28/basic>),
/// and `Int(1)` is a different ID from `Str("1")`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RequestId {
    Int(i64),
    Str(String),
}

/// The caller, assigned by the server and never taken from the request.
/// Session documents belong to the principal that created them.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Principal(Arc<str>);

impl Principal {
    pub fn new(name: &str) -> Self {
        Self(Arc::from(name))
    }

    /// The single principal of a stdio connection.
    pub fn local() -> Self {
        Self::new("local")
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// [`Versions`] as `{app, operation_api, engine_protocol, document}`.
pub fn versions_json(versions: &Versions) -> Value {
    json!({
        "app": versions.app,
        "operation_api": versions.operation_api,
        "engine_protocol": versions.engine_protocol,
        "document": versions.document,
    })
}

/// The wire form of an [`Envelope`]:
/// `{value, warnings: [{message}], validation, versions}`.
///
/// - `validation` is `{status: "valid"}` or `{status: "rejected", reason:
///   "reactions" | "invalid", message}`, with the unprefixed rejection message.
/// - `versions` is [`versions_json`].
///
/// If `value` cannot be encoded it becomes `null` and a last warning says why.
/// Derived `Serialize` impls of plain structs never fail.
pub fn envelope_json<T: Serialize>(envelope: &Envelope<T>) -> Map<String, Value> {
    let mut warnings: Vec<Value> = envelope
        .warnings
        .iter()
        .map(|warning| json!({"message": warning.message}))
        .collect();
    let value = serde_json::to_value(&envelope.value).unwrap_or_else(|error| {
        warnings.push(json!({"message": format!("The result could not be encoded: {error}")}));
        Value::Null
    });
    let validation = match &envelope.validation {
        ValidationStatus::Valid => json!({"status": "valid"}),
        ValidationStatus::Rejected(rejection) => json!({
            "status": "rejected",
            "reason": match rejection {
                Rejection::Reactions(_) => "reactions",
                Rejection::Invalid(_) => "invalid",
            },
            "message": rejection.message(),
        }),
    };
    Map::from_iter([
        ("value".to_owned(), value),
        ("warnings".to_owned(), Value::Array(warnings)),
        ("validation".to_owned(), validation),
        ("versions".to_owned(), versions_json(&envelope.versions)),
    ])
}

/// An [`IdRemap`] on the wire: `[{source, inserted}]` with decimal-string IDs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct IdRemapJson(pub Vec<RemapPairJson>);

/// One `(source, inserted)` pair of an [`IdRemap`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct RemapPairJson {
    pub source: ObjectId,
    pub inserted: ObjectId,
}

impl From<&IdRemap> for IdRemapJson {
    fn from(remap: &IdRemap) -> Self {
        Self(
            remap
                .pairs
                .iter()
                .map(|&(source, inserted)| RemapPairJson {
                    source: ObjectId(source),
                    inserted: ObjectId(inserted),
                })
                .collect(),
        )
    }
}

/// An [`ExportReceipt`] on the wire: `{format, byte_len, detail}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExportReceiptJson {
    pub format: String,
    pub byte_len: usize,
    pub detail: Option<String>,
}

impl From<&ExportReceipt> for ExportReceiptJson {
    fn from(receipt: &ExportReceipt) -> Self {
        Self {
            format: receipt.format.clone(),
            byte_len: receipt.byte_len,
            detail: receipt.detail.clone(),
        }
    }
}

#[cfg(test)]
mod tests;
