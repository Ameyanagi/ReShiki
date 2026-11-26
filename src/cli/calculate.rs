//! `reshiki --cli calculate`: a compact xTB summary through the ops tool.
use super::{
    args::Calculate,
    host::{CliError, Session, labelled},
    io,
};
use reshiki_agent::ops::budget::Budgets;
use serde_json::json;
use std::io::Write;

pub(crate) async fn run(
    command: Calculate,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<(), CliError> {
    let bytes = io::read_input(Some(&command.file), Budgets::default().max_text_bytes)
        .map_err(CliError::Failed)?;
    let xyz = String::from_utf8(bytes)
        .map_err(|_| CliError::Failed("The XYZ input must be UTF-8.".into()))?;
    let arguments = json!({
        "xyz": xyz,
        "task": command.task,
        "method": command.method,
        "charge": command.charge,
        "uhf": command.uhf,
        "solvent": command.solvent,
        "conventions": command.conventions,
        "electronic_temperature_k": command.electronic_temperature_k,
        "accuracy": command.accuracy,
        "max_iterations": command.max_iterations,
        "threads": command.threads,
        "convergence": command.convergence,
        "fixed_atoms": command.fixed_atoms,
    });
    let result = Session::new().call("calculate", arguments, err).await?;
    writeln!(out, "{}", labelled(result.value)).map_err(CliError::Stdout)
}
