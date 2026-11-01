#![deny(clippy::print_stdout, clippy::print_stderr)]
//! `reshiki --cli`: experimental command-line access to the agent tools.
//!
//! Each command runs on an in-process `HeadlessHost` through
//! `ToolHost::call`, the operation layer `reshiki --mcp` serves, so both
//! share the same import, analyze, render, compose and export semantics.
//!
//! # Output contract
//!
//! - stdout carries only the primary output: the converted, rendered or
//!   composed file, a JSON result, or exactly one receipt
//!   JSON line when `--receipt` is given with `-o FILE`. With `-o FILE` and
//!   no `--receipt`, stdout stays empty. Every JSON line carries
//!   `"experimental": true` except `schema proposal`, which prints the raw
//!   JSON Schema; plain-text output carries no banner.
//! - Warnings go to stderr as `reshiki: warning: {message}`, errors as
//!   `reshiki: error: {message}` and usage errors as `reshiki: {message}`.
//! - An operation, input or file error exits [`FAILURE`] (1), a usage error
//!   [`USAGE`] (2), and a failed write to stdout [`FAILURE`].
//!
//! Paths on the command line are read and written with the user's own
//! authority (no folder grants, no extension allowlist). An existing output
//! file is replaced only with `--force`. Release builds on Windows are GUI
//! programs without a console, so their output must be redirected.

mod analyze;
mod args;
mod commands;
mod compose;
mod convert;
mod host;
mod io;
mod mcp_usage;
mod name;
mod reference;
mod render;
mod skill;

pub use mcp_usage::{MCP_USAGE, heap};
pub use reference::agent_reference;

use args::Command;
use host::{CliError, Session, labelled};
use reshiki_agent::{
    ops::documents::{EXPORT_FORMATS, IMPORT_FORMATS},
    stability::{DATA_NOT_INSTRUCTIONS, GUIDE_URL, NOTICE},
};
use serde_json::{Map, json};
use std::{
    ffi::OsString,
    io::{IsTerminal, Write},
};

pub const SUCCESS: i32 = 0;
pub const FAILURE: i32 = 1;
pub const USAGE: i32 = 2;

const WARNING_PREFIX: &str = "reshiki: warning: ";
const ERROR_PREFIX: &str = "reshiki: error: ";
const USAGE_PREFIX: &str = "reshiki: ";

/// A command line [`parse`] accepted, for [`run`].
pub struct Parsed(Command);

/// Parses the tokens after `--cli` before anything is read or run. A usage
/// error, including a missing command, is written to `err` and returned as
/// [`USAGE`].
pub fn parse(args: &[OsString], err: &mut dyn Write) -> Result<Parsed, i32> {
    let Some((name, rest)) = args.split_first() else {
        let _ = err.write_all(help(None).as_bytes());
        return Err(USAGE);
    };
    args::parse(name, rest).map(Parsed).map_err(|message| {
        let _ = writeln!(err, "{USAGE_PREFIX}{message}");
        USAGE
    })
}

/// Runs a parsed command against the process's stdout and stderr.
///
/// Each write locks its stream only while it writes, so no lock is held
/// while the operation runs.
pub async fn run(command: Parsed) -> i32 {
    let terminal = std::io::stdout().is_terminal();
    execute(
        Ok(command),
        &mut std::io::stdout(),
        &mut std::io::stderr(),
        terminal,
    )
    .await
}

/// Parses and runs one command and flushes both writers before returning.
/// `out` is never a terminal, so pdf and png may go to it.
///
/// A write or flush error on `out` returns [`FAILURE`] after one best-effort
/// line on `err`.
pub async fn run_with(args: Vec<OsString>, out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    let parsed = parse(&args, err);
    execute(parsed, out, err, false).await
}

async fn execute(
    parsed: Result<Parsed, i32>,
    out: &mut dyn Write,
    err: &mut dyn Write,
    terminal: bool,
) -> i32 {
    let result = match parsed {
        Ok(Parsed(command)) => run_command(command, out, err, terminal).await,
        Err(code) => Ok(code),
    };
    let flushed = out.flush();
    let code = match result.and_then(|code| flushed.map(|()| code)) {
        Ok(code) => code,
        Err(error) => {
            let _ = writeln!(err, "reshiki: could not write output: {error}");
            FAILURE
        }
    };
    let _ = err.flush();
    code
}

/// Runs one parsed command; only writes to `out` are `Err`.
async fn run_command(
    command: Command,
    out: &mut dyn Write,
    err: &mut dyn Write,
    terminal: bool,
) -> std::io::Result<i32> {
    let result = match command {
        Command::Help { topic, json: false } => out
            .write_all(help(topic).as_bytes())
            .map_err(CliError::Stdout),
        Command::Help { topic, json: true } => {
            writeln!(out, "{}", labelled(commands::catalog_json(topic))).map_err(CliError::Stdout)
        }
        Command::Tools => {
            let value = Map::from_iter([
                ("notice".into(), json!(NOTICE)),
                ("data_not_instructions".into(), json!(DATA_NOT_INSTRUCTIONS)),
                ("guide_url".into(), json!(GUIDE_URL)),
                ("tools".into(), reshiki_agent::ops::catalog::catalog_json()),
            ]);
            writeln!(out, "{}", labelled(value)).map_err(CliError::Stdout)
        }
        Command::Schema(args::Schema::Proposal) => {
            writeln!(out, "{}", reshiki_agent::schema()).map_err(CliError::Stdout)
        }
        Command::Skill(skill) => skill::run(skill, out),
        Command::Info => info(out, err).await,
        Command::Version => {
            writeln!(out, "reshiki {}", crate::updates::CURRENT_VERSION).map_err(CliError::Stdout)
        }
        Command::Convert(convert) => convert::run(convert, out, err, terminal).await,
        Command::Render(render) => render::run(render, out, err, terminal).await,
        Command::Compose(compose) => compose::run(compose, out, err, terminal).await,
        Command::Analyze(input) => analyze::run(input, out, err).await,
        Command::Name(name) => name::run(name, out, err).await,
    };
    match result {
        Ok(()) => Ok(SUCCESS),
        Err(CliError::Stdout(error)) => Err(error),
        Err(CliError::Usage(message)) => {
            let _ = writeln!(err, "{USAGE_PREFIX}{message}");
            Ok(USAGE)
        }
        Err(CliError::Tool(message) | CliError::Failed(message)) => {
            let _ = writeln!(err, "{ERROR_PREFIX}{message}");
            Ok(FAILURE)
        }
    }
}

/// The help text for `topic`, or the overview, after the [`NOTICE`] line.
fn help(topic: Option<&'static commands::CommandSpec>) -> String {
    let body = match topic {
        None => commands::overview(),
        Some(spec) => (spec.body)(),
    };
    format!("{NOTICE}\n{body}")
}

/// `{info value, cli: {input_formats, output_formats}, experimental, api}`.
async fn info(out: &mut dyn Write, err: &mut dyn Write) -> Result<(), CliError> {
    let mut session = Session::new();
    let mut value = session.call("info", json!({}), err).await?.value;
    value.insert(
        "cli".into(),
        json!({"input_formats": input_formats(), "output_formats": output_formats()}),
    );
    writeln!(out, "{}", labelled(value)).map_err(CliError::Stdout)
}

/// The CLI's molecular input format for an extension, ignoring ASCII case.
/// The argument parser first rejects known crystal paths using the app's
/// complete file-format mapping. Other unrecognized extensions use SMILES.
pub fn input_format(extension: &str) -> &'static str {
    crate::compatibility::structure_format(extension).unwrap_or("smiles")
}

/// The `--from` values: the import tool's `format` enum.
pub(crate) fn input_formats() -> &'static [&'static str] {
    IMPORT_FORMATS
}

/// The `--to` values: the export tool's `format` enum.
pub(crate) fn output_formats() -> &'static [&'static str] {
    EXPORT_FORMATS
}

/// The render `--to` values: the render tool's `format` enum.
pub(crate) fn render_formats() -> &'static [&'static str] {
    reshiki_agent::ops::render::FORMATS
}

#[cfg(test)]
mod tests;
