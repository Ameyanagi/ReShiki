//! Protocol-neutral tool definitions. Each agent protocol (Codex dynamic
//! tools today) maps them to its own wire shape.
use serde_json::Value;

#[derive(Debug, Clone, Copy)]
pub struct ToolSpec {
    pub name: &'static str,
    pub title: Option<&'static str>,
    pub description: &'static str,
    pub input_schema: fn() -> Value,
    pub hints: Option<Hints>,
}

/// MCP ToolAnnotations hints. The MCP spec defaults differ from `false`
/// (destructive and open_world default to true), so ReShiki sets every hint
/// explicitly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hints {
    pub read_only: bool,
    pub destructive: bool,
    pub idempotent: bool,
    pub open_world: bool,
}

/// MCP tool-name rule: 1 to 128 characters of `[A-Za-z0-9_.-]`.
pub fn valid_name(name: &str) -> bool {
    (1..=128).contains(&name.len())
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'))
}

#[cfg(test)]
mod tests;
