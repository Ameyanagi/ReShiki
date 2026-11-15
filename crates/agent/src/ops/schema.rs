//! The `schema` tool: input schemas too large for the tool list, fetched
//! only by a client that needs them.
//!
//! compose's sketch diagram is about 10,000 characters of schema that most
//! sessions never use, so compose lists `sketch` as a plain object and this
//! tool returns its fields. The in-app assistant still gets the whole
//! Proposal schema from [`crate::schema`].
use super::{catalog::arguments, error::OpError, result::ToolResult};
use crate::{
    envelope::Versions,
    tool_spec::{Hints, ToolSpec},
};
use serde::Deserialize;
use serde_json::{Value, json};

/// The `name` values the schema tool accepts.
pub const NAMES: &[&str] = &["sketch"];

pub const SCHEMA: ToolSpec = ToolSpec {
    name: "schema",
    title: Some("Input schema"),
    description: "Return an input schema that the tool list leaves out to stay small. sketch is the JSON Schema of compose's proposal.sketch, with the drawing rules in its description; fetch it once, before the first sketch.",
    input_schema: schema,
    hints: Some(Hints {
        read_only: true,
        destructive: false,
        idempotent: true,
        open_world: false,
    }),
};

fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {"name": {"type": "string", "enum": NAMES}},
        "required": ["name"],
        "additionalProperties": false,
    })
}

/// A schema the tool returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Name {
    Sketch,
}

/// The `schema` arguments.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Schema {
    name: Name,
}

pub(crate) fn decode(args: Value) -> Result<Schema, OpError> {
    arguments(args)
}

/// Where the sketch object is in the assistant's Proposal schema.
pub(crate) const SKETCH: &str = "/properties/sketch/anyOf/1";

/// The sketch object's schema, exactly as the assistant's Proposal schema
/// states it.
pub fn sketch() -> Value {
    crate::schema().pointer(SKETCH).cloned().unwrap_or_default()
}

/// `{name, schema}` in a successful envelope.
pub(crate) fn run(args: &Schema, versions: &Versions) -> ToolResult {
    let (name, schema) = match args.name {
        Name::Sketch => ("sketch", sketch()),
    };
    ToolResult::valid(
        json!({"name": name, "schema": schema}),
        Vec::new(),
        versions.clone(),
    )
}
