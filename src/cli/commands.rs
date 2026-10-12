//! The command grammar and help text for `reshiki --cli`.
use super::{input_formats, io, output_formats, render_formats};
use reshiki_agent::ops::budget::Budgets;

/// One option accepted by a command.
#[derive(Debug, Clone, Copy)]
pub(crate) struct OptionSpec {
    pub(crate) flag: &'static str,
    /// The value name, or `None` for an option that takes no value.
    pub(crate) value: Option<&'static str>,
    #[expect(
        dead_code,
        reason = "Reserved for the command catalog; existing help keeps its prose."
    )]
    pub(crate) help: &'static str,
}

/// One command's grammar and help.
#[derive(Debug)]
pub(crate) struct CommandSpec {
    pub(crate) name: &'static str,
    pub(crate) summary: &'static str,
    /// Exact text after `Usage: `, including any continuation-line wrap.
    /// `help` has its own invocation here, but its body prints the overview's
    /// generic usage instead, as do `help help` and the help aliases.
    pub(crate) usage: &'static str,
    pub(crate) options: &'static [OptionSpec],
    pub(crate) body: fn() -> String,
}

// Command names identify specs; function pointers are not compared.
impl PartialEq for CommandSpec {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Eq for CommandSpec {}

const SMILES: OptionSpec = OptionSpec {
    flag: "--smiles",
    value: Some("TEXT"),
    help: "Read SMILES text instead of a file or standard input.",
};
const FROM: OptionSpec = OptionSpec {
    flag: "--from",
    value: Some("FORMAT"),
    help: "Choose the input format.",
};
const TO: OptionSpec = OptionSpec {
    flag: "--to",
    value: Some("FORMAT"),
    help: "Choose the output format.",
};
const IMAGE_TO: OptionSpec = OptionSpec {
    help: "Choose the image format.",
    ..TO
};
const OUTPUT: OptionSpec = OptionSpec {
    flag: "-o",
    value: Some("OUTPUT"),
    help: "Write to OUTPUT, or to standard output with -.",
};
const PAGES: OptionSpec = OptionSpec {
    flag: "--pages",
    value: None,
    help: "Export every publication page as one PDF.",
};
const FORCE: OptionSpec = OptionSpec {
    flag: "--force",
    value: None,
    help: "Replace the output file if it exists.",
};
const RECEIPT: OptionSpec = OptionSpec {
    flag: "--receipt",
    value: None,
    help: "Print one receipt JSON line when writing an output file.",
};
const WIDTH: OptionSpec = OptionSpec {
    flag: "--width",
    value: Some("N"),
    help: "Set the maximum PNG width in pixels.",
};
const HEIGHT: OptionSpec = OptionSpec {
    flag: "--height",
    value: Some("N"),
    help: "Set the maximum PNG height in pixels.",
};

const CONVERT: CommandSpec = CommandSpec {
    name: "convert",
    summary: "Convert a structure or drawing to another file format",
    usage: "reshiki --cli convert (INPUT | - | --smiles TEXT) [--from FORMAT] [--to FORMAT]
                             [-o OUTPUT|-] [--pages] [--force] [--receipt]",
    options: &[SMILES, FROM, TO, OUTPUT, PAGES, FORCE, RECEIPT],
    body: convert_help,
};

const RENDER: CommandSpec = CommandSpec {
    name: "render",
    summary: "Render a preview image of a structure or drawing",
    usage: "reshiki --cli render (INPUT | - | --smiles TEXT) [--from FORMAT] [--to FORMAT]
                            [-o OUTPUT|-] [--width N] [--height N] [--force]",
    options: &[SMILES, FROM, IMAGE_TO, OUTPUT, WIDTH, HEIGHT, FORCE],
    body: render_help,
};

const COMPOSE: CommandSpec = CommandSpec {
    name: "compose",
    summary: "Lay out an assistant Proposal (JSON) and write it as a file",
    usage: "reshiki --cli compose (PROPOSAL | -) [--to FORMAT] [-o OUTPUT|-] [--pages]
                             [--force] [--receipt]",
    options: &[TO, OUTPUT, PAGES, FORCE, RECEIPT],
    body: compose_help,
};

const ANALYZE: CommandSpec = CommandSpec {
    name: "analyze",
    summary: "Print molecular properties as one JSON line",
    usage: "reshiki --cli analyze (INPUT | - | --smiles TEXT) [--from FORMAT]",
    options: &[SMILES, FROM],
    body: analyze_help,
};

const INFO: CommandSpec = CommandSpec {
    name: "info",
    summary: "Print versions, limits and formats as one JSON line",
    usage: "reshiki --cli info",
    options: &[],
    body: info_help,
};

const HELP: CommandSpec = CommandSpec {
    name: "help",
    summary: "Show this help, or a command's help: reshiki --cli help <command>",
    usage: "reshiki --cli help [COMMAND]",
    options: &[],
    body: overview,
};

/// Commands in overview-help order; option order is the parsing grammar.
pub(crate) const COMMANDS: &[CommandSpec] = &[CONVERT, RENDER, COMPOSE, ANALYZE, INFO, HELP];

/// The command called `name`; the `--help` and `-h` aliases are not entries.
pub(crate) fn find(name: &str) -> Option<&'static CommandSpec> {
    COMMANDS.iter().find(|spec| spec.name == name)
}

/// The overview help, `help`'s body: the command list comes from [`COMMANDS`].
pub(crate) fn overview() -> String {
    let width = COMMANDS
        .iter()
        .map(|spec| spec.name.len())
        .max()
        .unwrap_or_default();
    let mut text = OVERVIEW_START.to_owned();
    for spec in COMMANDS {
        text.push_str(&format!("  {:width$}  {}\n", spec.name, spec.summary));
    }
    text.push_str(OVERVIEW_END);
    text
}

const OVERVIEW_START: &str = "\
Usage: reshiki --cli <command> [options]

Commands:
";

const OVERVIEW_END: &str = "\n\
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
Prints the molecular properties of a structure or drawing as one JSON line:
SMILES, formula, mass, exact mass, logP, TPSA, hydrogen-bond donors and
acceptors, rings, unpaired electrons, InChI and InChIKey, under
value.analysis. The input is read as for convert (reshiki --cli help convert).
";

const INFO_HELP: &str = "\
Prints ReShiki's versions, the limits it enforces and the input and output
formats as one JSON line.
";

fn convert_help() -> String {
    let budgets = Budgets::default();
    format!(
        "\
Usage: {usage}

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
        usage = CONVERT.usage,
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
Usage: {usage}

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
        usage = RENDER.usage,
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
Usage: {usage}

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
        usage = COMPOSE.usage,
        text = Budgets::default().max_text_bytes,
        outputs = output_formats().join(", "),
    )
}

fn analyze_help() -> String {
    format!("Usage: {}\n\n{ANALYZE_HELP}", ANALYZE.usage)
}

fn info_help() -> String {
    format!("Usage: {}\n\n{INFO_HELP}", INFO.usage)
}
