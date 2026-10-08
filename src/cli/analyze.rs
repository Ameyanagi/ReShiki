//! `reshiki --cli analyze`: import, then analyze the whole drawing through
//! the ops tools.
use super::{
    args::Input,
    host::{CliError, Session, api},
};
use serde_json::{Value, json};
use std::io::Write;

/// Prints the analyze result's value plus `api` as one JSON line.
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
    let mut value = result.value;
    value.insert("api".into(), api());
    writeln!(out, "{}", Value::Object(value)).map_err(CliError::Stdout)
}
