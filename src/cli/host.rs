//! The in-process [`HeadlessHost`] a CLI command runs on, called through
//! [`ToolHost::call`] like `reshiki --mcp` calls it.
use super::{
    args::{Input, Source},
    io::{limit, read_input, text},
};
use reshiki_agent::ops::{
    budget::Budgets,
    headless::HeadlessHost,
    host::{Call, ToolHost},
    result::ToolResult,
    wire::{Principal, RequestId},
};
use serde_json::{Map, Value, json};
use std::io::{self, Write};

/// Why a command failed.
#[derive(Debug)]
pub(crate) enum CliError {
    /// A usage error: exit 2, as `reshiki: {message}`.
    Usage(String),
    /// A tool execution error: exit 1, as `reshiki: error: {message}`.
    Tool(String),
    /// An input, file or internal error: exit 1, as
    /// `reshiki: error: {message}`.
    Failed(String),
    /// Writing stdout failed: exit 1.
    Stdout(io::Error),
}

/// One command's host and the request IDs it hands out.
pub(crate) struct Session {
    host: HeadlessHost,
    budgets: Budgets,
    principal: Principal,
    next: i64,
}

impl Session {
    pub(crate) fn new() -> Self {
        let budgets = Budgets::default();
        Self {
            host: HeadlessHost::new(crate::updates::CURRENT_VERSION, budgets.clone()),
            budgets,
            principal: Principal::local(),
            next: 0,
        }
    }

    /// Calls `tool` with `args`, which list every property of its strict
    /// schema (`null` for an absent optional one).
    ///
    /// The result's envelope warnings go to `err` in order, as
    /// `reshiki: warning: {message}`, even for an error result. An error
    /// result is [`CliError::Tool`]; an unknown tool or a cancellation is an
    /// internal error.
    pub(crate) async fn call(
        &mut self,
        tool: &str,
        args: Value,
        err: &mut dyn Write,
    ) -> Result<ToolResult, CliError> {
        self.next = self.next.saturating_add(1);
        let call = Call {
            principal: self.principal.clone(),
            request: RequestId::Int(self.next),
            tool: tool.to_owned(),
            arguments: args,
            progress: None,
        };
        let result = self
            .host
            .call(call)
            .await
            .map_err(|error| CliError::Failed(format!("internal error: {}", error.message)))?;
        warn(&result.value, err);
        if result.is_error {
            let message = result
                .value
                .get("error")
                .and_then(|error| error.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("the operation failed");
            return Err(CliError::Tool(message.to_owned()));
        }
        Ok(result)
    }

    /// Reads `input` and imports it as a session document; returns the
    /// document's handle.
    pub(crate) async fn import(
        &mut self,
        input: Input,
        err: &mut dyn Write,
    ) -> Result<Value, CliError> {
        let Input { source, format } = input;
        let text = match source {
            Source::Smiles(text) => text,
            Source::File(path) => self.read(Some(&path), &format)?,
            Source::Stdin => self.read(None, &format)?,
        };
        let result = self
            .call("import", json!({"format": format, "text": text}), err)
            .await?;
        let handle = result
            .value
            .get("value")
            .and_then(|value| value.get("document"))
            .filter(|handle| handle.is_string());
        handle
            .cloned()
            .ok_or_else(|| CliError::Failed("internal error: import returned no document".into()))
    }

    /// The file at `path`, or standard input, as import text in `format`.
    fn read(&self, path: Option<&std::path::Path>, format: &str) -> Result<String, CliError> {
        let bytes = read_input(path, limit(format, &self.budgets)).map_err(CliError::Failed)?;
        text(format, bytes).map_err(CliError::Failed)
    }
}

/// Writes each envelope warning of a result `value` to `err`, in order.
fn warn(value: &Map<String, Value>, err: &mut dyn Write) {
    let warnings = value.get("warnings").and_then(Value::as_array);
    for warning in warnings.into_iter().flatten() {
        if let Some(message) = warning.get("message").and_then(Value::as_str) {
            let _ = writeln!(err, "reshiki: warning: {message}");
        }
    }
}

/// The `api` object every JSON result carries.
pub(crate) fn api() -> Value {
    json!({
        "stability": "experimental",
        "operation_api": crate::envelope::OPERATION_API_VERSION,
    })
}
