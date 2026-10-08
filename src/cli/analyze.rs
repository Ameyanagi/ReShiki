//! `reshiki --cli analyze`: import, then analyze the whole drawing through
//! the ops tools.
use super::{
    args::Input,
    host::{CliError, Session, labelled},
};
use serde_json::json;
use std::io::Write;

/// Prints the analyze result's value, labelled, as one JSON line.
pub(crate) async fn run(
    input: Input,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<(), CliError> {
    let mut session = Session::new();
    let document = session.import(input, err).await?;
    let result = session
        .call("analyze", json!({"document": document, "ids": null}), err)
        .await?;
    writeln!(out, "{}", labelled(result.value)).map_err(CliError::Stdout)
}
