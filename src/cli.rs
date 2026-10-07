#![deny(clippy::print_stdout, clippy::print_stderr)]
//! `reshiki --cli`: experimental command-line access to the agent tools.
//!
//! Results go to `out` and diagnostics to `err`. Each command returns a
//! process exit code: [`SUCCESS`], [`FAILURE`] or [`USAGE`].

use std::{
    ffi::OsString,
    io::{self, Write},
};

pub const SUCCESS: i32 = 0;
pub const FAILURE: i32 = 1;
pub const USAGE: i32 = 2;

const HELP: &str = "\
Experimental: ReShiki's agent tools, schemas and results may change between releases.
Usage: reshiki --cli <command> [options]

Commands:
  help    Show this help
  info    Print version information as one JSON line
";

/// Runs one command against the process's stdout and stderr.
pub async fn run(args: Vec<OsString>) -> i32 {
    let mut out = io::stdout().lock();
    let mut err = io::stderr().lock();
    run_with(args, &mut out, &mut err).await
}

/// Runs one command and flushes both writers before returning.
///
/// A write or flush error on `out` returns [`FAILURE`] after one best-effort
/// line on `err`.
pub async fn run_with(args: Vec<OsString>, out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    let result = command(&args, out, err);
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

/// Dispatches on the first token; only writes to `out` can fail.
fn command(args: &[OsString], out: &mut dyn Write, err: &mut dyn Write) -> io::Result<i32> {
    let Some((name, rest)) = args.split_first() else {
        let _ = err.write_all(HELP.as_bytes());
        return Ok(USAGE);
    };
    if name == "help" || name == "--help" || name == "-h" {
        if !rest.is_empty() {
            let _ = writeln!(err, "reshiki: help takes no arguments");
            return Ok(USAGE);
        }
        out.write_all(HELP.as_bytes())?;
    } else if name == "info" {
        if !rest.is_empty() {
            let _ = writeln!(err, "reshiki: info takes no arguments");
            return Ok(USAGE);
        }
        writeln!(out, "{}", info())?;
    } else {
        let _ = writeln!(
            err,
            "reshiki: unknown command `{}`; run `reshiki --cli help`",
            name.to_string_lossy()
        );
        return Ok(USAGE);
    }
    Ok(SUCCESS)
}

fn info() -> serde_json::Value {
    let versions = crate::envelope::Versions::current(crate::updates::CURRENT_VERSION);
    serde_json::json!({
        "app": versions.app,
        "operation_api": versions.operation_api,
        "engine_protocol": versions.engine_protocol,
        "document": versions.document,
        "platform": std::env::consts::OS,
        "api": {
            "stability": "experimental",
            "operation_api": crate::envelope::OPERATION_API_VERSION,
        },
    })
}

#[cfg(test)]
mod tests;
