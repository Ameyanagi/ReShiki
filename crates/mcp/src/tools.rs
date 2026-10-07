//! The tool catalog a [`ToolHost`](reshiki_agent::ops::host::ToolHost)
//! serves, as MCP `Tool` definitions.
//!
//! The catalog is checked once, before serving starts: names are unique and
//! follow the MCP tool-name rule, and every input schema is a JSON Schema
//! object whose `type` is `"object"`. No tool publishes an `outputSchema`:
//! once one is published, every result must conform to it.
use reshiki_agent::tool_spec::{Hints, ToolSpec, valid_name};
use rmcp::model::{Tool, ToolAnnotations};
use serde_json::Value;
use std::{collections::HashSet, sync::Arc};

#[cfg(test)]
mod tests;

/// The tools served on one connection, in the host's order.
pub(crate) struct Catalog {
    tools: Vec<Tool>,
    names: HashSet<&'static str>,
}

/// Why a host's catalog cannot be served; each names the tool's position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CatalogError {
    /// The name breaks the MCP tool-name rule.
    InvalidName(usize),
    /// An earlier tool has the same name.
    Duplicate(usize),
    /// The input schema is not an object with `"type": "object"`.
    Schema(usize),
}

impl CatalogError {
    /// The position of the offending tool.
    pub(crate) fn index(self) -> usize {
        match self {
            Self::InvalidName(index) | Self::Duplicate(index) | Self::Schema(index) => index,
        }
    }
}

impl Catalog {
    /// Checks and maps `specs`, keeping their order.
    pub(crate) fn new(specs: &'static [ToolSpec]) -> Result<Self, CatalogError> {
        let mut tools = Vec::with_capacity(specs.len());
        let mut names = HashSet::with_capacity(specs.len());
        for (index, spec) in specs.iter().enumerate() {
            if !valid_name(spec.name) {
                return Err(CatalogError::InvalidName(index));
            }
            if !names.insert(spec.name) {
                return Err(CatalogError::Duplicate(index));
            }
            let schema = match (spec.input_schema)() {
                Value::Object(schema)
                    if schema.get("type").and_then(Value::as_str) == Some("object") =>
                {
                    schema
                }
                _ => return Err(CatalogError::Schema(index)),
            };
            let mut tool = Tool::new(spec.name, spec.description, Arc::new(schema));
            tool.title = spec.title.map(str::to_owned);
            tool.annotations = spec.hints.map(annotations);
            tools.push(tool);
        }
        Ok(Self { tools, names })
    }

    /// The tool definitions, in the host's order.
    pub(crate) fn tools(&self) -> &[Tool] {
        &self.tools
    }

    /// Whether the catalog lists a tool named `name`.
    pub(crate) fn contains(&self, name: &str) -> bool {
        self.names.contains(name)
    }
}

/// Every hint is set explicitly: the MCP defaults for destructive and
/// open-world are true.
fn annotations(hints: Hints) -> ToolAnnotations {
    ToolAnnotations::from_raw(
        None,
        Some(hints.read_only),
        Some(hints.destructive),
        Some(hints.idempotent),
        Some(hints.open_world),
    )
}
