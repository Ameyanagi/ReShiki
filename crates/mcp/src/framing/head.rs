//! Classifies one line from its JSON-RPC head. Only `jsonrpc`, `id`,
//! `method` and the presence of `result` or `error` are read: `params` and
//! every container are skipped, never materialized.
use super::{Key, MAX_ID_BYTES};
use serde::{
    Deserialize, Deserializer,
    de::{DeserializeSeed, IgnoredAny, MapAccess, SeqAccess, Visitor},
};
use std::{borrow::Cow, fmt, sync::Arc};

pub(super) const PARSE_ERROR: i64 = -32700;
pub(super) const INVALID_REQUEST: i64 = -32600;

/// What the reader does with one line.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Class {
    /// Answer with a JSON-RPC error; nothing is forwarded.
    Reply(Reply),
    /// Admit and forward the request.
    Request { key: Key, is_initialize: bool },
    /// Forward `notifications/initialized`.
    Initialized,
    /// `notifications/cancelled` for this request id.
    Cancel(Key),
    /// Drop the line without output.
    Ignore(Ignored),
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Reply {
    pub(super) code: i64,
    pub(super) message: Cow<'static, str>,
    /// Omitted from the response when None, never null.
    pub(super) id: Option<Key>,
}

impl Reply {
    fn parse_error() -> Self {
        Self {
            code: PARSE_ERROR,
            message: Cow::Borrowed("Parse error"),
            id: None,
        }
    }

    fn invalid(id: Option<Key>) -> Self {
        Self {
            code: INVALID_REQUEST,
            message: Cow::Borrowed("Invalid Request"),
            id,
        }
    }

    pub(super) fn oversize(max_line_bytes: usize, id: Option<Key>) -> Self {
        Self {
            code: INVALID_REQUEST,
            message: Cow::Owned(format!("Request exceeds {max_line_bytes} bytes")),
            id,
        }
    }

    pub(super) fn duplicate(id: Key) -> Self {
        Self {
            code: INVALID_REQUEST,
            message: Cow::Borrowed("Duplicate request id"),
            id: Some(id),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Ignored {
    /// An unknown notification, a malformed cancellation or an early
    /// `notifications/initialized`.
    Notification,
    /// Clients MUST NOT send responses over stdio.
    Response,
}

/// Classifies a complete line, without its newline.
pub(super) fn classify(line: &[u8], initialize_seen: bool) -> Class {
    // One pass that stores nothing checks the syntax, and serde_json's
    // recursion limit makes deep nesting a parse error.
    let Some(text) = std::str::from_utf8(line)
        .ok()
        .filter(|text| serde_json::from_str::<Skip>(text).is_ok())
    else {
        return Class::Reply(Reply::parse_error());
    };
    if !text
        .trim_start_matches([' ', '\t', '\r', '\n'])
        .starts_with('{')
    {
        return Class::Reply(Reply::invalid(None));
    }
    // A duplicate head field fails here.
    let Ok(head) = serde_json::from_str::<Head>(text) else {
        return Class::Reply(Reply::parse_error());
    };
    // None: no id; Some(None): an id that is not a valid request id.
    let id = head.id.map(Field::into_key);
    let version = matches!(&head.jsonrpc, Some(Field::Str(version)) if version == "2.0");
    match head.method {
        Some(Field::Str(method)) => match id {
            Some(Some(key)) if version => Class::Request {
                is_initialize: method == "initialize",
                key,
            },
            Some(key) => Class::Reply(Reply::invalid(key)),
            None => notification(&method, text, initialize_seen),
        },
        Some(_) => Class::Reply(Reply::invalid(id.flatten())),
        None if head.result || head.error => Class::Ignore(Ignored::Response),
        None => Class::Reply(Reply::invalid(id.flatten())),
    }
}

fn notification(method: &str, text: &str, initialize_seen: bool) -> Class {
    match method {
        "notifications/cancelled" => serde_json::from_str::<Cancelled>(text)
            .ok()
            .and_then(|cancelled| cancelled.params?.0)
            .and_then(Field::into_key)
            .map_or(Class::Ignore(Ignored::Notification), Class::Cancel),
        "notifications/initialized" if initialize_seen => Class::Initialized,
        _ => Class::Ignore(Ignored::Notification),
    }
}

/// The id of an oversize line, read from its first bytes. The prefix is
/// usually cut mid-value, so any error ends the probe; an id read before
/// that point is kept.
pub(super) fn probe_id(prefix: &[u8]) -> Option<Key> {
    let mut found = None;
    let mut input = serde_json::Deserializer::from_slice(prefix);
    let _ = FirstField {
        name: "id",
        found: &mut found,
    }
    .deserialize(&mut input);
    found.and_then(Field::into_key)
}

#[derive(Deserialize)]
struct Head {
    #[serde(default, deserialize_with = "some")]
    jsonrpc: Option<Field>,
    #[serde(default, deserialize_with = "some")]
    id: Option<Field>,
    #[serde(default, deserialize_with = "some")]
    method: Option<Field>,
    #[serde(default, deserialize_with = "present")]
    result: bool,
    #[serde(default, deserialize_with = "present")]
    error: bool,
}

/// A present field, even `null`, which `Option` alone would read as absent.
fn some<'de, D: Deserializer<'de>>(input: D) -> Result<Option<Field>, D::Error> {
    Field::deserialize(input).map(Some)
}

fn present<'de, D: Deserializer<'de>>(input: D) -> Result<bool, D::Error> {
    IgnoredAny::deserialize(input).map(|_| true)
}

#[derive(Deserialize)]
struct Cancelled {
    #[serde(default)]
    params: Option<CancelParams>,
}

/// `params.requestId`; params that are not an object fail to parse.
struct CancelParams(Option<Field>);

impl<'de> Deserialize<'de> for CancelParams {
    fn deserialize<D: Deserializer<'de>>(input: D) -> Result<Self, D::Error> {
        let mut found = None;
        FirstField {
            name: "requestId",
            found: &mut found,
        }
        .deserialize(input)?;
        Ok(Self(found))
    }
}

/// Any JSON value, skipped without being stored. Unlike [`IgnoredAny`],
/// which serde_json skips iteratively, it walks containers through
/// `deserialize_any`, so the recursion limit applies.
struct Skip;

impl<'de> Deserialize<'de> for Skip {
    fn deserialize<D: Deserializer<'de>>(input: D) -> Result<Self, D::Error> {
        input.deserialize_any(SkipVisitor)
    }
}

struct SkipVisitor;

impl<'de> Visitor<'de> for SkipVisitor {
    type Value = Skip;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("any JSON value")
    }

    fn visit_bool<E>(self, _: bool) -> Result<Skip, E> {
        Ok(Skip)
    }

    fn visit_i64<E>(self, _: i64) -> Result<Skip, E> {
        Ok(Skip)
    }

    fn visit_u64<E>(self, _: u64) -> Result<Skip, E> {
        Ok(Skip)
    }

    fn visit_f64<E>(self, _: f64) -> Result<Skip, E> {
        Ok(Skip)
    }

    fn visit_str<E>(self, _: &str) -> Result<Skip, E> {
        Ok(Skip)
    }

    fn visit_unit<E>(self) -> Result<Skip, E> {
        Ok(Skip)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Skip, A::Error> {
        while seq.next_element::<Skip>()?.is_some() {}
        Ok(Skip)
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Skip, A::Error> {
        while map.next_entry::<Skip, Skip>()?.is_some() {}
        Ok(Skip)
    }
}

/// A JSON value reduced to what an id, method or version needs. Strings and
/// numbers are kept; everything else is skipped without being stored.
enum Field {
    Int(i64),
    Str(String),
    Other,
}

impl Field {
    /// An integer within i64, or a string of at most [`MAX_ID_BYTES`].
    fn into_key(self) -> Option<Key> {
        match self {
            Self::Int(value) => Some(Key::Int(value)),
            Self::Str(text) if text.len() <= MAX_ID_BYTES => Some(Key::Str(Arc::from(text))),
            Self::Str(_) | Self::Other => None,
        }
    }
}

impl<'de> Deserialize<'de> for Field {
    fn deserialize<D: Deserializer<'de>>(input: D) -> Result<Self, D::Error> {
        input.deserialize_any(FieldVisitor)
    }
}

struct FieldVisitor;

impl<'de> Visitor<'de> for FieldVisitor {
    type Value = Field;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("any JSON value")
    }

    fn visit_bool<E>(self, _: bool) -> Result<Field, E> {
        Ok(Field::Other)
    }

    fn visit_i64<E>(self, value: i64) -> Result<Field, E> {
        Ok(Field::Int(value))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Field, E> {
        Ok(i64::try_from(value).map_or(Field::Other, Field::Int))
    }

    fn visit_f64<E>(self, _: f64) -> Result<Field, E> {
        Ok(Field::Other)
    }

    fn visit_str<E>(self, value: &str) -> Result<Field, E> {
        Ok(Field::Str(value.to_owned()))
    }

    fn visit_string<E>(self, value: String) -> Result<Field, E> {
        Ok(Field::Str(value))
    }

    fn visit_unit<E>(self) -> Result<Field, E> {
        Ok(Field::Other)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<Field, A::Error> {
        IgnoredAny.visit_seq(seq).map(|_| Field::Other)
    }

    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Field, A::Error> {
        IgnoredAny.visit_map(map).map(|_| Field::Other)
    }
}

/// Stores the first value named `name` of a JSON object in `found` as soon
/// as it is read, so a later error does not lose it, and skips the rest.
struct FirstField<'a> {
    name: &'static str,
    found: &'a mut Option<Field>,
}

impl<'de> DeserializeSeed<'de> for FirstField<'_> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, input: D) -> Result<(), D::Error> {
        input.deserialize_map(self)
    }
}

impl<'de> Visitor<'de> for FirstField<'_> {
    type Value = ();

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a JSON object")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        while let Some(name) = map.next_key::<String>()? {
            if name == self.name && self.found.is_none() {
                *self.found = Some(map.next_value()?);
            } else {
                map.next_value::<IgnoredAny>()?;
            }
        }
        Ok(())
    }
}
