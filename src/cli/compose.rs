//! `reshiki --cli compose`: lay out a Proposal through the ops tools, then
//! export it as convert does.
use super::{
    args::Compose,
    convert::{refuse_binary, write},
    host::{CliError, Session},
    io::read_input,
};
use reshiki_agent::ops::budget::Budgets;
use serde_json::{Value, json};
use std::io::Write;

/// Composes as `compose` says. `terminal` tells whether stdout is one.
///
/// The Proposal is read as at most [`Budgets::max_text_bytes`] of JSON and
/// passed to the compose tool with the default style, whose errors read as
/// on the assistant's path. The new document is then written as convert
/// writes its file.
pub(crate) async fn run(
    compose: Compose,
    out: &mut dyn Write,
    err: &mut dyn Write,
    terminal: bool,
) -> Result<(), CliError> {
    let Compose { proposal, export } = compose;
    refuse_binary(&export.format, &export.output, terminal)?;
    let bytes = read_input(proposal.as_deref(), Budgets::default().max_text_bytes)
        .map_err(CliError::Failed)?;
    let proposal: Value = serde_json::from_slice(&bytes)
        .map_err(|error| CliError::Failed(format!("PROPOSAL is not valid JSON: {error}")))?;
    let mut session = Session::new();
    let result = session
        .call(
            "compose",
            json!({"proposal": proposal, "style_document": null}),
            err,
        )
        .await?;
    let document = result
        .value
        .get("value")
        .and_then(|value| value.get("document"))
        .filter(|handle| handle.is_string())
        .cloned()
        .ok_or_else(|| CliError::Failed("internal error: compose returned no document".into()))?;
    write(&mut session, document, export, out, err).await
}
