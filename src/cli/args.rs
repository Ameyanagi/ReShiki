//! The `reshiki --cli` grammar:
//!
//! ```text
//! reshiki --cli convert (INPUT | - | --smiles TEXT) [--from FMT] [--to FMT] [-o OUTPUT|-] [--pages] [--force] [--receipt]
//! reshiki --cli render (INPUT | - | --smiles TEXT) [--from FMT] [--to png|svg] [-o OUTPUT|-] [--width N] [--height N] [--force]
//! reshiki --cli compose (PROPOSAL | -) [--to FMT] [-o OUTPUT|-] [--pages] [--force] [--receipt]
//! reshiki --cli analyze (INPUT | - | --smiles TEXT) [--from FMT]
//! reshiki --cli info
//! reshiki --cli help [COMMAND]
//! ```
//!
//! Tokens stay [`OsString`]s, so paths need not be UTF-8. `--` ends the
//! options, each option is allowed at most once, and every problem is a
//! usage error (exit 2) found before anything is read or run.
use super::{input_format, input_formats, output_formats, render_formats};
use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
};

/// A parsed command line.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Command {
    Help(Option<Topic>),
    Info,
    Convert(Convert),
    Render(Render),
    Compose(Compose),
    Analyze(Input),
}

/// A command `help` describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Topic {
    Convert,
    Render,
    Compose,
    Analyze,
    Info,
    Help,
}

/// Where the input comes from.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Source {
    File(PathBuf),
    Stdin,
    Smiles(String),
}

/// The input and its import format, one of the ops import formats.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Input {
    pub(crate) source: Source,
    pub(crate) format: String,
}

/// Where the output goes.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Output {
    Stdout,
    File(PathBuf),
}

/// The file convert and compose write: `format` is one of the ops export
/// formats.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Export {
    pub(crate) format: String,
    pub(crate) output: Output,
    pub(crate) pages: bool,
    pub(crate) force: bool,
    pub(crate) receipt: bool,
}

/// `convert`.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Convert {
    pub(crate) input: Input,
    pub(crate) export: Export,
}

/// `render`: `format` is one of the ops render formats; `width` and
/// `height` are `None` for the tool's default.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Render {
    pub(crate) input: Input,
    pub(crate) format: String,
    pub(crate) output: Output,
    pub(crate) width: Option<u32>,
    pub(crate) height: Option<u32>,
    pub(crate) force: bool,
}

/// `compose`: the Proposal file, or `None` for standard input.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Compose {
    pub(crate) proposal: Option<PathBuf>,
    pub(crate) export: Export,
}

/// Every option; each command takes those in its own list.
const OPTIONS: &[&str] = &[
    "--smiles",
    "--from",
    "--to",
    "-o",
    "--width",
    "--height",
    "--pages",
    "--force",
    "--receipt",
];
const CONVERT_OPTIONS: &[&str] = &[
    "--smiles",
    "--from",
    "--to",
    "-o",
    "--pages",
    "--force",
    "--receipt",
];
const RENDER_OPTIONS: &[&str] = &[
    "--smiles", "--from", "--to", "-o", "--width", "--height", "--force",
];
const COMPOSE_OPTIONS: &[&str] = &["--to", "-o", "--pages", "--force", "--receipt"];
const ANALYZE_OPTIONS: &[&str] = &["--smiles", "--from"];

/// Parses `name` and the tokens after it; `Err` is the usage message.
pub(crate) fn parse(name: &OsStr, rest: &[OsString]) -> Result<Command, String> {
    let lossy = name.to_string_lossy();
    match lossy.as_ref() {
        "help" | "--help" | "-h" => help(rest),
        "info" if rest.is_empty() => Ok(Command::Info),
        "info" => Err("info takes no arguments".into()),
        "convert" => convert(&options("convert", CONVERT_OPTIONS, rest)?),
        "render" => render(&options("render", RENDER_OPTIONS, rest)?),
        "compose" => compose(&options("compose", COMPOSE_OPTIONS, rest)?),
        "analyze" => {
            input("analyze", &options("analyze", ANALYZE_OPTIONS, rest)?).map(Command::Analyze)
        }
        _ => Err(format!(
            "unknown command `{lossy}`; run `reshiki --cli help`"
        )),
    }
}

/// `help [COMMAND]`.
fn help(rest: &[OsString]) -> Result<Command, String> {
    let [topic] = rest else {
        return if rest.is_empty() {
            Ok(Command::Help(None))
        } else {
            Err("help takes at most one command".into())
        };
    };
    let topic = match topic.to_string_lossy().as_ref() {
        "convert" => Topic::Convert,
        "render" => Topic::Render,
        "compose" => Topic::Compose,
        "analyze" => Topic::Analyze,
        "info" => Topic::Info,
        "help" => Topic::Help,
        other => {
            return Err(format!(
                "unknown command `{other}`; run `reshiki --cli help`"
            ));
        }
    };
    Ok(Command::Help(Some(topic)))
}

/// The tokens after a command, each option at most once.
#[derive(Default)]
struct Options {
    /// INPUT, PROPOSAL or `-`.
    input: Option<OsString>,
    smiles: Option<OsString>,
    from: Option<OsString>,
    to: Option<OsString>,
    output: Option<OsString>,
    width: Option<OsString>,
    height: Option<OsString>,
    pages: bool,
    force: bool,
    receipt: bool,
}

/// Splits `tokens` into options from `allowed` and one positional input.
fn options(command: &str, allowed: &[&str], tokens: &[OsString]) -> Result<Options, String> {
    let mut options = Options::default();
    let mut seen: Vec<&str> = Vec::new();
    let mut ended = false;
    let mut tokens = tokens.iter();
    while let Some(token) = tokens.next() {
        if !ended && token == "--" {
            ended = true;
            continue;
        }
        if ended || token == "-" || !token.as_encoded_bytes().starts_with(b"-") {
            if options.input.is_some() {
                return Err(format!(
                    "{command} takes one input; unexpected `{}`",
                    token.to_string_lossy()
                ));
            }
            options.input = Some(token.clone());
            continue;
        }
        let Some(&name) = OPTIONS.iter().find(|name| token == **name) else {
            return Err(format!("unknown option `{}`", token.to_string_lossy()));
        };
        if !allowed.contains(&name) {
            return Err(format!("{command} does not take {name}"));
        }
        if seen.contains(&name) {
            return Err(format!("{name} given more than once"));
        }
        seen.push(name);
        match name {
            "--pages" => options.pages = true,
            "--force" => options.force = true,
            "--receipt" => options.receipt = true,
            _ => {
                // The next token is the value, whatever it looks like.
                let value = tokens
                    .next()
                    .cloned()
                    .ok_or_else(|| format!("{name} needs a value"))?;
                match name {
                    "--smiles" => options.smiles = Some(value),
                    "--from" => options.from = Some(value),
                    "--to" => options.to = Some(value),
                    "--width" => options.width = Some(value),
                    "--height" => options.height = Some(value),
                    _ => options.output = Some(value),
                }
            }
        }
    }
    Ok(options)
}

/// The one input source and its format.
fn input(command: &str, options: &Options) -> Result<Input, String> {
    let source = match (&options.input, &options.smiles) {
        (Some(_), Some(_)) => {
            return Err(format!(
                "{command} takes one input: INPUT, - or --smiles, not both"
            ));
        }
        (None, None) => {
            return Err(format!(
                "{command} needs an input: a file, - for standard input, or --smiles TEXT"
            ));
        }
        (Some(path), None) if path == "-" => Source::Stdin,
        (Some(path), None) => Source::File(PathBuf::from(path)),
        (None, Some(text)) => Source::Smiles(
            text.to_str()
                .ok_or("--smiles text must be UTF-8")?
                .to_owned(),
        ),
    };
    let format = match (&source, &options.from) {
        (Source::Smiles(_), Some(_)) => {
            return Err("--smiles cannot be combined with --from".into());
        }
        (Source::Smiles(_), None) => "smiles".to_owned(),
        (Source::Stdin, None) => {
            return Err("reading standard input (-) needs --from FORMAT".into());
        }
        (Source::File(path), None) => input_format(&extension(path)).to_owned(),
        (_, Some(from)) => {
            let from = from.to_string_lossy();
            let formats = input_formats();
            if !formats.iter().any(|format| *format == from) {
                return Err(format!(
                    "unknown input format `{from}`; supported: {}",
                    formats.join(", ")
                ));
            }
            from.into_owned()
        }
    };
    Ok(Input { source, format })
}

/// The extension as the app reads it: empty unless it is UTF-8.
fn extension(path: &Path) -> String {
    path.extension()
        .and_then(OsStr::to_str)
        .unwrap_or_default()
        .to_owned()
}

/// `convert`: the input, then the file to write.
fn convert(options: &Options) -> Result<Command, String> {
    let input = input("convert", options)?;
    let export = export(options)?;
    Ok(Command::Convert(Convert { input, export }))
}

/// `render`: the input, the image format and destination, then the size.
fn render(options: &Options) -> Result<Command, String> {
    let input = input("render", options)?;
    let (format, output) = destination(options, &render_formats())?;
    Ok(Command::Render(Render {
        input,
        format,
        output,
        width: side("--width", options.width.as_deref())?,
        height: side("--height", options.height.as_deref())?,
        force: options.force,
    }))
}

/// `compose`: the Proposal file or `-`, then the file to write.
fn compose(options: &Options) -> Result<Command, String> {
    let proposal = match &options.input {
        None => {
            return Err("compose needs a proposal: a JSON file, or - for standard input".into());
        }
        Some(path) if path == "-" => None,
        Some(path) => Some(PathBuf::from(path)),
    };
    let export = export(options)?;
    Ok(Command::Compose(Compose { proposal, export }))
}

/// A `--width` or `--height` value: a `u32`, which the render tool then
/// checks against its limits.
fn side(name: &str, value: Option<&OsStr>) -> Result<Option<u32>, String> {
    let parse = |value: &OsStr| {
        let number = value.to_str().and_then(|text| text.parse().ok());
        number.ok_or_else(|| {
            format!(
                "{name} must be a whole number of pixels, not `{}`",
                value.to_string_lossy()
            )
        })
    };
    value.map(parse).transpose()
}

/// The file convert and compose write: the format and destination, then
/// `--pages` for pdf only and `--receipt` only with `-o FILE`.
fn export(options: &Options) -> Result<Export, String> {
    let (format, output) = destination(options, &output_formats())?;
    if options.pages && format != "pdf" {
        return Err("--pages is valid only with pdf output".into());
    }
    if options.receipt && output == Output::Stdout {
        return Err("--receipt needs -o FILE".into());
    }
    Ok(Export {
        format,
        output,
        pages: options.pages,
        force: options.force,
        receipt: options.receipt,
    })
}

/// Where the output goes and its format, one of `formats`: `--to`, or the
/// extension of `-o FILE` (`.smi` is smiles).
fn destination(options: &Options, formats: &[String]) -> Result<(String, Output), String> {
    let output = match &options.output {
        Some(path) if path == "-" => Output::Stdout,
        Some(path) if path.is_empty() => return Err("-o needs a file name".into()),
        Some(path) => Output::File(PathBuf::from(path)),
        None => Output::Stdout,
    };
    let format = match (&options.to, &output) {
        (Some(to), _) => to.to_string_lossy().into_owned(),
        (None, Output::File(path)) => match extension_lossy(path) {
            Some(extension) if extension == "smi" => "smiles".to_owned(),
            Some(extension) => extension,
            None => {
                return Err(format!(
                    "cannot tell the output format of `{}`; pass --to FORMAT",
                    path.display()
                ));
            }
        },
        (None, Output::Stdout) => {
            return Err("writing to standard output needs --to FORMAT".into());
        }
    };
    if !formats.contains(&format) {
        return Err(format!(
            "unsupported output format `{format}`; supported: {}",
            formats.join(", ")
        ));
    }
    Ok((format, output))
}

/// The lowercased extension of an output path, if it has one.
fn extension_lossy(path: &Path) -> Option<String> {
    let extension = path.extension()?.to_string_lossy().to_ascii_lowercase();
    (!extension.is_empty()).then_some(extension)
}
