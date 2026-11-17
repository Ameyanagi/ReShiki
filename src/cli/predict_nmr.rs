//! `reshiki --cli predict-nmr`: import, then predict NMR shifts for the whole
//! drawing through the ops tools.
use super::{
    args::PredictNmr,
    host::{CliError, Session, labelled},
};
use serde_json::json;
use std::io::Write;

/// Prints the predict_nmr result's value, labelled, as one JSON line.
pub(crate) async fn run(
    command: PredictNmr,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<(), CliError> {
    let mut session = Session::new();
    let document = session.import(command.input, err).await?;
    let arguments = json!({"document": document, "nucleus": command.nucleus, "ids": null});
    let result = session.call("predict_nmr", arguments, err).await?;
    writeln!(out, "{}", labelled(result.value)).map_err(CliError::Stdout)
}
