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
//!   composed file, the analyze or info JSON line, or exactly one receipt
//!   JSON line when `--receipt` is given with `-o FILE`. With `-o FILE` and
//!   no `--receipt`, stdout stays empty. Every JSON line carries
//!   `"experimental": true`; plain-text output carries no banner.
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
mod compose;
mod convert;
mod host;
mod io;
mod render;

use args::{Command, Topic};
use host::{CliError, Session, labelled};
use reshiki_agent::{
    ops::{budget::Budgets, catalog::SPECS},
    stability::NOTICE,
};
use serde_json::{Value, json};
use std::{
    ffi::OsString,
    io::{IsTerminal, Write},
};

pub const SUCCESS: i32 = 0;
pub const FAILURE: i32 = 1;
pub const USAGE: i32 = 2;

const HELP: &str = "\
Usage: reshiki --cli <command> [options]

Commands:
  convert  Convert a structure or drawing to another file format
  render   Render a preview image of a structure or drawing
  compose  Lay out an assistant Proposal (JSON) and write it as a file
  analyze  Print molecular properties as one JSON line
  info     Print versions, limits and formats as one JSON line
  help     Show this help, or a command's help: reshiki --cli help <command>

Examples:
  reshiki --cli convert --smiles 'CCO' -o ethanol.mol
  reshiki --cli convert scheme.rsk -o figure.pdf
  reshiki --cli render scheme.rsk -o preview.png
  reshiki --cli analyze --smiles 'c1ccccc1O'
  reshiki --cli compose proposal.json -o scheme.svg

Output:
  stdout carries only the result: the file, one JSON line, or one receipt
  line with --receipt. With -o FILE and no --receipt it stays empty.
  Warnings and errors go to stderr.

Exit status:
  0  success
  1  the operation failed, or reading the input or writing the output failed
  2  usage error

Files named on the command line are read and written with your own
permissions. On Windows, release builds cannot write to a console window:
redirect the output, for example `reshiki --cli info > info.json`.
";

const ANALYZE_HELP: &str = "\
Usage: reshiki --cli analyze (INPUT | - | --smiles TEXT) [--from FORMAT]

Prints the molecular properties of a structure or drawing as one JSON line:
SMILES, formula, mass, exact mass, logP, TPSA, hydrogen-bond donors and
acceptors, rings, unpaired electrons, InChI and InChIKey, under
value.analysis. The input is read as for convert (reshiki --cli help convert).
";

const INFO_HELP: &str = "\
Usage: reshiki --cli info

Prints ReShiki's versions, the limits it enforces and the input and output
formats as one JSON line.
";

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
        let _ = writeln!(err, "reshiki: {message}");
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
        Command::Help(topic) => out
            .write_all(help(topic).as_bytes())
            .map_err(CliError::Stdout),
        Command::Info => info(out, err).await,
        Command::Convert(convert) => convert::run(convert, out, err, terminal).await,
        Command::Render(render) => render::run(render, out, err, terminal).await,
        Command::Compose(compose) => compose::run(compose, out, err, terminal).await,
        Command::Analyze(input) => analyze::run(input, out, err).await,
    };
    match result {
        Ok(()) => Ok(SUCCESS),
        Err(CliError::Stdout(error)) => Err(error),
        Err(CliError::Usage(message)) => {
            let _ = writeln!(err, "reshiki: {message}");
            Ok(USAGE)
        }
        Err(CliError::Tool(message) | CliError::Failed(message)) => {
            let _ = writeln!(err, "reshiki: error: {message}");
            Ok(FAILURE)
        }
    }
}

/// The help text for `topic`, or the overview, after the [`NOTICE`] line.
fn help(topic: Option<Topic>) -> String {
    let body = match topic {
        None | Some(Topic::Help) => HELP.to_owned(),
        Some(Topic::Convert) => convert_help(),
        Some(Topic::Render) => render_help(),
        Some(Topic::Compose) => compose_help(),
        Some(Topic::Analyze) => ANALYZE_HELP.to_owned(),
        Some(Topic::Info) => INFO_HELP.to_owned(),
    };
    format!("{NOTICE}\n{body}")
}

fn convert_help() -> String {
    let budgets = Budgets::default();
    format!(
        "\
Usage: reshiki --cli convert (INPUT | - | --smiles TEXT) [--from FORMAT] [--to FORMAT]
                             [-o OUTPUT|-] [--pages] [--force] [--receipt]

Converts a structure or drawing, making the file as the app's Export does.

Input, exactly one of:
  INPUT          A file. Without --from, its extension picks the format:
                 .rsk, .reshiki and .moruno are ReShiki drawings; .mol, .rxn,
                 .rsmi, .cdxml, .cdx and .inchi are read as such; anything
                 else is read as SMILES.
  -              Standard input; needs --from.
  --smiles TEXT  SMILES text; cannot be combined with --from.

Options:
  --from FORMAT  The input format: {inputs}.
  --to FORMAT    The output format: {outputs}.
                 Without --to, the extension of OUTPUT picks it (.smi is smiles).
  -o OUTPUT      Write the file to OUTPUT. Without -o, or with -o -, the file
                 goes to stdout and needs --to; pdf and png are never written
                 to a terminal.
  --pages        Export every publication page as one PDF (pdf only).
  --force        Replace OUTPUT if it exists; otherwise that is an error.
  --receipt      With -o OUTPUT, print one JSON line to stdout:
                 {{\"receipt\": {{\"format\", \"byte_len\", \"detail\"}}, \"api\": {{…}}}}.

Inputs are at most {text} bytes ({cdx} for cdx), so a ReShiki drawing with
large embedded pictures that the app opens can be too large here. rsk, rxn,
rsmi, cdx and emf files cannot be written.
",
        inputs = input_formats().join(", "),
        outputs = output_formats().join(", "),
        text = budgets.max_text_bytes,
        cdx = io::limit("cdx", &budgets),
    )
}

fn render_help() -> String {
    let render = Budgets::default().render;
    format!(
        "\
Usage: reshiki --cli render (INPUT | - | --smiles TEXT) [--from FORMAT] [--to FORMAT]
                            [-o OUTPUT|-] [--width N] [--height N] [--force]

Renders the preview the assistant looks at: png on white, scaled to fit
--width × --height, or svg with a transparent surround. For figures to
publish, use convert. The input is read as for convert
(reshiki --cli help convert).

Options:
  --to FORMAT  The image format: {formats}.
               Without --to, the extension of OUTPUT picks it.
  -o OUTPUT    Write the image to OUTPUT. Without -o, or with -o -, it goes
               to stdout and needs --to; png is never written to a terminal.
  --width N    The widest a png may be, in pixels (default {width}).
  --height N   The tallest a png may be, in pixels (default {height}).
               Each is {min} to {max}, and a png has at most {pixels} pixels.
  --force      Replace OUTPUT if it exists; otherwise that is an error.
",
        formats = render_formats().join(", "),
        width = render.default_width,
        height = render.default_height,
        min = render.min_side,
        max = render.max_side,
        pixels = render.max_pixels,
    )
}

fn compose_help() -> String {
    format!(
        "\
Usage: reshiki --cli compose (PROPOSAL | -) [--to FORMAT] [-o OUTPUT|-] [--pages]
                             [--force] [--receipt]

Lays out a Proposal as ReShiki's assistant composes its drafts, in the
default style, then writes the drawing as convert does.

PROPOSAL is a JSON file, or - for standard input, of at most {text} bytes.
It follows the assistant's Proposal schema, which `reshiki --mcp` publishes
with its compose tool; replace_ids must be empty.

Options:
  --to FORMAT  The output format: {outputs}.
               Without --to, the extension of OUTPUT picks it (.smi is smiles).
  -o OUTPUT    Write the file to OUTPUT. Without -o, or with -o -, the file
               goes to stdout and needs --to; pdf and png are never written
               to a terminal.
  --pages      Export every publication page as one PDF (pdf only).
  --force      Replace OUTPUT if it exists; otherwise that is an error.
  --receipt    With -o OUTPUT, print one receipt JSON line to stdout, as
               convert does.

A ReShiki drawing (.rsk) cannot be written.
",
        text = Budgets::default().max_text_bytes,
        outputs = output_formats().join(", "),
    )
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

/// The import format the app picks for a file with `extension`, as
/// `prepare` in src/app/files.rs does: a native extension is `reshiki`; mol,
/// rxn, rsmi, cdxml, cdx and inchi are themselves; anything else, including
/// smi, smiles and no extension, is `smiles`. Case is ignored.
pub fn input_format(extension: &str) -> &'static str {
    let extension = extension.to_ascii_lowercase();
    if crate::compatibility::is_native_extension(&extension) {
        return "reshiki";
    }
    match extension.as_str() {
        "mol" => "mol",
        "rxn" => "rxn",
        "rsmi" => "rsmi",
        "cdxml" => "cdxml",
        "cdx" => "cdx",
        "inchi" => "inchi",
        _ => "smiles",
    }
}

/// The `--from` values: the import tool's `format` enum.
pub(crate) fn input_formats() -> Vec<String> {
    schema_formats("import")
}

/// The `--to` values: the export tool's `format` enum.
pub(crate) fn output_formats() -> Vec<String> {
    schema_formats("export")
}

/// The render `--to` values: the render tool's `format` enum.
pub(crate) fn render_formats() -> Vec<String> {
    schema_formats("render")
}

/// `properties.format.enum` of the ops tool `tool`'s input schema.
fn schema_formats(tool: &str) -> Vec<String> {
    let schema = SPECS
        .iter()
        .find(|spec| spec.name == tool)
        .map(|spec| (spec.input_schema)());
    let formats = schema
        .as_ref()
        .and_then(|schema| schema.pointer("/properties/format/enum"))
        .and_then(Value::as_array);
    formats
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests;
