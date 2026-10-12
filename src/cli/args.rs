//! Token parsing for the grammar in [`commands::COMMANDS`], described by
//! `reshiki --cli help`.
//!
//! Tokens stay [`OsString`]s, so paths need not be UTF-8. `--` ends the
//! options, each option is allowed at most once, and every problem is a
//! usage error (exit 2) found before anything is read or run.
use super::{
    commands::{self, CommandSpec},
    input_format, input_formats, output_formats, render_formats,
};
use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
};

/// A parsed command line.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Command {
    Help {
        topic: Option<&'static CommandSpec>,
        json: bool,
    },
    Info,
    Tools,
    Schema(Schema),
    Convert(Convert),
    Render(Render),
    Compose(Compose),
    Analyze(Input),
}

/// A schema the CLI can print without a tool host.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Schema {
    Proposal,
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

/// Parses `name` and the tokens after it; `Err` is the usage message.
pub(crate) fn parse(name: &OsStr, rest: &[OsString]) -> Result<Command, String> {
    let lossy = name.to_string_lossy();
    if matches!(lossy.as_ref(), "--help" | "-h") {
        return help(rest);
    }
    let spec = commands::find(&lossy).ok_or_else(|| unknown_command(&lossy))?;
    match spec.name {
        "help" => help(rest),
        "info" if rest.is_empty() => Ok(Command::Info),
        "info" => Err("info takes no arguments".into()),
        "tools" if rest.is_empty() => Ok(Command::Tools),
        "tools" => Err("tools takes no arguments".into()),
        "schema" => schema(&options(spec, rest)?),
        "convert" => convert(&options(spec, rest)?),
        "render" => render(&options(spec, rest)?),
        "compose" => compose(&options(spec, rest)?),
        "analyze" => input(spec.name, &options(spec, rest)?).map(Command::Analyze),
        _ => Err(unknown_command(&lossy)),
    }
}

/// The usage message for a command name that is not in the table.
fn unknown_command(name: &str) -> String {
    format!("unknown command `{name}`; run `reshiki --cli help`")
}

/// The usage message for an option `command` does not take: a file command's
/// flag names the command; any other flag, such as help's --json, stays an
/// unknown option there, as it was before another command took it.
fn foreign_option(command: &str, token: &OsStr) -> String {
    match commands::FILE_COMMANDS
        .iter()
        .flat_map(|command| command.options)
        .find(|option| token == option.flag)
    {
        Some(known) => format!("{command} does not take {}", known.flag),
        None => format!("unknown option `{}`", token.to_string_lossy()),
    }
}

/// `help [--json] [COMMAND]`; without `--json`, keep the original grammar.
fn help(rest: &[OsString]) -> Result<Command, String> {
    let mut json = false;
    let mut topics = Vec::new();
    for token in rest {
        if token == "--json" {
            if json {
                return Err("--json given more than once".into());
            }
            json = true;
        } else {
            topics.push(token);
        }
    }
    let [topic] = topics.as_slice() else {
        return if topics.is_empty() {
            Ok(Command::Help { topic: None, json })
        } else {
            Err("help takes at most one command".into())
        };
    };
    let name = topic.to_string_lossy();
    let topic = commands::find(&name).ok_or_else(|| unknown_command(&name))?;
    Ok(Command::Help {
        topic: Some(topic),
        json,
    })
}

/// `schema NAME`; reuse the standard option and positional-input errors.
fn schema(options: &Options) -> Result<Command, String> {
    let name = options
        .input
        .as_ref()
        .ok_or("schema needs a name: proposal")?
        .to_string_lossy();
    match name.as_ref() {
        "proposal" => Ok(Command::Schema(Schema::Proposal)),
        _ => Err(format!("unknown schema `{name}`; supported: proposal")),
    }
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

/// Splits `tokens` into the command's options and one positional input.
fn options(spec: &CommandSpec, tokens: &[OsString]) -> Result<Options, String> {
    let command = spec.name;
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
        let Some(option) = spec.options.iter().find(|option| token == option.flag) else {
            return Err(foreign_option(command, token));
        };
        let name = option.flag;
        if seen.contains(&name) {
            return Err(format!("{name} given more than once"));
        }
        seen.push(name);
        if option.value.is_some() {
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
        } else {
            match name {
                "--pages" => options.pages = true,
                "--force" => options.force = true,
                "--receipt" => options.receipt = true,
                _ => {}
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
            if !formats.contains(&from.as_ref()) {
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
    let (format, output) = destination(options, render_formats())?;
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
    let (format, output) = destination(options, output_formats())?;
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
fn destination(options: &Options, formats: &[&str]) -> Result<(String, Output), String> {
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
    if !formats.contains(&format.as_str()) {
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
