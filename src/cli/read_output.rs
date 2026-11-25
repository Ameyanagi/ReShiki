//! `reshiki --cli read-output`: quantum chemistry results through the ops tool.
use super::{
    args::ReadOutput,
    host::{CliError, Session, labelled},
    io,
};
use reshiki_agent::ops::budget::Budgets;
use serde_json::json;
use std::io::Write;

pub(crate) async fn run(
    command: ReadOutput,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<(), CliError> {
    let mut budgets = Budgets::default();
    let limit = budgets.max_text_bytes;
    let bytes = io::read_input(Some(&command.file), limit).map_err(|message| {
        CliError::Failed(if message == format!("input exceeds {limit} bytes") {
            format!("input exceeds max_text_bytes ({limit} bytes)")
        } else {
            message
        })
    })?;
    let text = String::from_utf8_lossy(&bytes);
    // The file is already bounded. Replacement characters can expand its UTF-8 text.
    budgets.max_text_bytes = limit.max(text.len());
    let file_name = command
        .file
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    let arguments = json!({
        "text": text,
        "file_name": file_name,
        "path": null,
        "job": command.job,
    });
    let result = Session::with_budgets(budgets)
        .call("read_output", arguments, err)
        .await?;
    writeln!(out, "{}", labelled(result.value)).map_err(CliError::Stdout)
}
