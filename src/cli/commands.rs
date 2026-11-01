//! The command grammar and help text for `reshiki --cli`.
use super::{
    ERROR_PREFIX, FAILURE, SUCCESS, USAGE, USAGE_PREFIX, WARNING_PREFIX,
    heap::{DEFAULT_HEAP_MIB, HEAP_MIB, HEAP_VARIABLE},
    input_formats, io,
    mcp_usage::{HEAP_MEANING, HEAP_UNIT, heap_environment_help},
    output_formats, render_formats,
};
use reshiki_agent::{
    ops::budget::Budgets,
    stability::{GUIDE_URL, NOTICE},
};
use reshiki_process_heap::RESOURCE_EXIT;
use serde_json::{Map, Value, json};

/// One option accepted by a command.
#[derive(Debug, Clone, Copy)]
pub(crate) struct OptionSpec {
    pub(crate) flag: &'static str,
    /// The value name, or `None` for an option that takes no value.
    pub(crate) value: Option<&'static str>,
    pub(crate) help: &'static str,
    /// The value the command uses when the option is absent, if it has one.
    pub(crate) default: Option<fn() -> Value>,
    /// The argument set on a tool this command calls.
    pub(crate) sets: Option<&'static str>,
    /// The MCP argument the CLI implements itself, without calling that tool.
    pub(crate) equivalent: Option<&'static str>,
}

/// A command's positional argument.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Operand {
    pub(crate) name: &'static str,
    pub(crate) help: &'static str,
    pub(crate) sets: Option<&'static str>,
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
    pub(crate) tools: &'static [&'static str],
    pub(crate) operand: Option<Operand>,
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
    help: "Read SMILES text instead of a file or standard input; implies import.format smiles.",
    default: None,
    sets: Some("import.text"),
    equivalent: None,
};
const FROM: OptionSpec = OptionSpec {
    flag: "--from",
    value: Some("FORMAT"),
    help: "Choose the input format.",
    default: None,
    sets: Some("import.format"),
    equivalent: None,
};
const TO: OptionSpec = OptionSpec {
    flag: "--to",
    value: Some("FORMAT"),
    help: "Choose the output format.",
    default: None,
    sets: Some("export.format"),
    equivalent: None,
};
const IMAGE_TO: OptionSpec = OptionSpec {
    help: "Choose the image format.",
    sets: Some("render.format"),
    ..TO
};
const OUTPUT: OptionSpec = OptionSpec {
    flag: "-o",
    value: Some("OUTPUT"),
    help: "Write to OUTPUT, or to standard output with -.",
    default: None,
    sets: None,
    equivalent: Some("file_save.path"),
};
const PAGES: OptionSpec = OptionSpec {
    flag: "--pages",
    value: None,
    help: "Export every publication page as one PDF.",
    default: None,
    sets: Some("export.pages"),
    equivalent: None,
};
const FORCE: OptionSpec = OptionSpec {
    flag: "--force",
    value: None,
    help: "Replace the output file if it exists.",
    default: None,
    sets: None,
    equivalent: Some("file_save.overwrite"),
};
/// render writes the preview itself; file_save writes export's files only.
const IMAGE_OUTPUT: OptionSpec = OptionSpec {
    equivalent: None,
    ..OUTPUT
};
const IMAGE_FORCE: OptionSpec = OptionSpec {
    equivalent: None,
    ..FORCE
};
const RECEIPT: OptionSpec = OptionSpec {
    flag: "--receipt",
    value: None,
    help: "Print one receipt JSON line when writing an output file.",
    default: None,
    sets: None,
    equivalent: None,
};
const WIDTH: OptionSpec = OptionSpec {
    flag: "--width",
    value: Some("N"),
    help: "Set the maximum PNG width in pixels.",
    default: Some(|| json!(Budgets::default().render.default_width)),
    sets: Some("render.max_width"),
    equivalent: None,
};
const HEIGHT: OptionSpec = OptionSpec {
    flag: "--height",
    value: Some("N"),
    help: "Set the maximum PNG height in pixels.",
    default: Some(|| json!(Budgets::default().render.default_height)),
    sets: Some("render.max_height"),
    equivalent: None,
};

const JSON: OptionSpec = OptionSpec {
    flag: "--json",
    value: None,
    help: "Print the command catalog as one JSON line.",
    default: None,
    sets: None,
    equivalent: None,
};

const NAME_JSON: OptionSpec = OptionSpec {
    help: "Print the name and its status as one JSON line.",
    ..JSON
};

const INPUT: Operand = Operand {
    name: "INPUT",
    help: "Read a file, or standard input with -; --smiles TEXT supplies text instead.",
    sets: Some("import.text"),
};

const CONVERT: CommandSpec = CommandSpec {
    name: "convert",
    summary: "Convert a structure or drawing to another file format",
    usage: "reshiki --cli convert (INPUT | - | --smiles TEXT) [--from FORMAT] [--to FORMAT]
                             [-o OUTPUT|-] [--pages] [--force] [--receipt]",
    options: &[SMILES, FROM, TO, OUTPUT, PAGES, FORCE, RECEIPT],
    tools: &["import", "export"],
    operand: Some(INPUT),
    body: convert_help,
};

const RENDER: CommandSpec = CommandSpec {
    name: "render",
    summary: "Render a preview image of a structure or drawing",
    usage: "reshiki --cli render (INPUT | - | --smiles TEXT) [--from FORMAT] [--to FORMAT]
                            [-o OUTPUT|-] [--width N] [--height N] [--force]",
    options: &[
        SMILES,
        FROM,
        IMAGE_TO,
        IMAGE_OUTPUT,
        WIDTH,
        HEIGHT,
        IMAGE_FORCE,
    ],
    tools: &["import", "render"],
    operand: Some(INPUT),
    body: render_help,
};

const COMPOSE: CommandSpec = CommandSpec {
    name: "compose",
    summary: "Lay out an assistant Proposal (JSON) and write it as a file",
    usage: "reshiki --cli compose (PROPOSAL | -) [--to FORMAT] [-o OUTPUT|-] [--pages]
                             [--force] [--receipt]",
    options: &[TO, OUTPUT, PAGES, FORCE, RECEIPT],
    tools: &["compose", "export"],
    operand: Some(Operand {
        name: "PROPOSAL",
        help: "Read a Proposal JSON file, or standard input with -; replace_ids must be empty.",
        sets: Some("compose.proposal"),
    }),
    body: compose_help,
};

const ANALYZE: CommandSpec = CommandSpec {
    name: "analyze",
    summary: "Print molecular properties as one JSON line",
    usage: "reshiki --cli analyze (INPUT | - | --smiles TEXT) [--from FORMAT]",
    options: &[SMILES, FROM],
    tools: &["import", "analyze"],
    operand: Some(INPUT),
    body: analyze_help,
};

const NAME: CommandSpec = CommandSpec {
    name: "name",
    summary: "Print the systematic name of a structure",
    usage: "reshiki --cli name (INPUT | - | --smiles TEXT) [--from FORMAT] [--json]",
    options: &[SMILES, FROM, NAME_JSON],
    tools: &["import"],
    operand: Some(INPUT),
    body: name_help,
};

/// The commands whose flags another command rejects by name ("X does not
/// take FLAG"); other flags are unknown options to the commands without them.
pub(crate) const FILE_COMMANDS: &[CommandSpec] = &[CONVERT, RENDER, COMPOSE, ANALYZE];

const INFO: CommandSpec = CommandSpec {
    name: "info",
    summary: "Print versions, limits and formats as one JSON line",
    usage: "reshiki --cli info",
    options: &[],
    tools: &["info"],
    operand: None,
    body: info_help,
};

const TOOLS: CommandSpec = CommandSpec {
    name: "tools",
    summary: "Print the agent tool catalog as one JSON line",
    usage: "reshiki --cli tools",
    options: &[],
    tools: &[],
    operand: None,
    body: tools_help,
};

const SCHEMA: CommandSpec = CommandSpec {
    name: "schema",
    summary: "Print a JSON Schema: reshiki --cli schema proposal",
    usage: "reshiki --cli schema NAME",
    options: &[],
    tools: &[],
    operand: Some(Operand {
        name: "NAME",
        help: "The schema name: proposal.",
        sets: None,
    }),
    body: schema_help,
};

const SKILL_FOLDER: OptionSpec = OptionSpec {
    value: Some("FOLDER"),
    help: "Write the skill folder NAME inside FOLDER, which must exist.",
    equivalent: None,
    ..OUTPUT
};
const SKILL_NAME: OptionSpec = OptionSpec {
    flag: "--name",
    value: Some("NAME"),
    help: "Name the skill and its folder.",
    default: Some(|| json!(DEFAULT_SKILL_NAME)),
    sets: None,
    equivalent: None,
};
const SKILL_EXECUTABLE: OptionSpec = OptionSpec {
    flag: "--executable",
    value: Some("PATH"),
    help: "Absolute path of the ReShiki executable the skill runs; default: this executable.",
    default: None,
    sets: None,
    equivalent: None,
};
const SKILL_FORCE: OptionSpec = OptionSpec {
    help: "Replace the generated files of an existing skill folder; other files stay.",
    equivalent: None,
    ..FORCE
};
const SKILL_RECEIPT: OptionSpec = OptionSpec {
    help: "Print one receipt JSON line after writing the skill.",
    ..RECEIPT
};

/// The skill name without `--name`.
pub(crate) const DEFAULT_SKILL_NAME: &str = "reshiki";

const SKILL: CommandSpec = CommandSpec {
    name: "skill",
    summary: "Write an Agent Skill folder that teaches AI agents to use ReShiki",
    usage: "reshiki --cli skill -o FOLDER [--name NAME] [--executable PATH]
                           [--force] [--receipt]",
    options: &[
        SKILL_FOLDER,
        SKILL_NAME,
        SKILL_EXECUTABLE,
        SKILL_FORCE,
        SKILL_RECEIPT,
    ],
    tools: &[],
    operand: None,
    body: skill_help,
};

const HELP: CommandSpec = CommandSpec {
    name: "help",
    summary: "Show this help, or a command's help: reshiki --cli help <command>",
    usage: "reshiki --cli help [--json] [COMMAND]",
    options: &[JSON],
    tools: &[],
    operand: Some(Operand {
        name: "COMMAND",
        help: "Show one command's help or catalog entry; omit for every command.",
        sets: None,
    }),
    body: overview,
};

/// Commands in overview-help order; option order is the parsing grammar.
pub(crate) const COMMANDS: &[CommandSpec] = &[
    CONVERT, RENDER, COMPOSE, ANALYZE, NAME, INFO, TOOLS, SCHEMA, SKILL, HELP,
];

const OVERVIEW_USAGE: &str = "reshiki --cli <command> [options]";
const OPTIONS_RULE: &str = "Options are allowed once each; -- ends the options.";
const STDOUT_CONTRACT: &str = "\
stdout carries only the result: the file, one JSON line, or one receipt
  line with --receipt. With -o FILE and no --receipt it stays empty.";
const SUCCESS_MEANING: &str = "success";
const FAILURE_MEANING: &str =
    "the operation failed, or reading the input or writing the output failed";
const USAGE_MEANING: &str = "usage error";
const RESOURCE_MEANING: &str = "the memory ceiling was reached";

/// Machine-readable help from the same specs the parser and prose help use.
pub(crate) fn catalog_json(topic: Option<&CommandSpec>) -> Map<String, Value> {
    let commands: Vec<_> = COMMANDS
        .iter()
        .filter(|spec| topic.is_none_or(|topic| *spec == topic))
        .map(|spec| {
            let operand = spec.operand.map(|operand| {
                json!({
                    "name": operand.name, "help": operand.help, "sets": operand.sets,
                })
            });
            let options: Vec<_> = spec
                .options
                .iter()
                .map(|option| {
                    json!({
                        "flag": option.flag, "value": option.value,
                        "default": option.default.map(|default| default()),
                        "help": option.help, "sets": option.sets,
                        "equivalent": option.equivalent,
                    })
                })
                .collect();
            json!({
                "name": spec.name, "summary": spec.summary, "usage": one_line(spec.usage),
                "tools": spec.tools, "operand": operand, "options": options,
            })
        })
        .collect();
    Map::from_iter([
        ("notice".into(), json!(NOTICE)),
        ("guide_url".into(), json!(GUIDE_URL)),
        ("usage".into(), json!(OVERVIEW_USAGE)),
        ("commands".into(), json!(commands)),
        ("options_rule".into(), json!(OPTIONS_RULE)),
        (
            "output".into(),
            json!({
                "stdout": one_line(STDOUT_CONTRACT),
                "stderr": {"warning": WARNING_PREFIX, "error": ERROR_PREFIX, "usage": USAGE_PREFIX},
            }),
        ),
        (
            "exit_codes".into(),
            json!([
                {"code": SUCCESS, "meaning": SUCCESS_MEANING},
                {"code": FAILURE, "meaning": FAILURE_MEANING},
                {"code": USAGE, "meaning": USAGE_MEANING},
                {"code": RESOURCE_EXIT,
                 "meaning": format!("{RESOURCE_MEANING} (see {HEAP_VARIABLE})")},
            ]),
        ),
        (
            "environment".into(),
            json!([{
                "name": HEAP_VARIABLE, "meaning": HEAP_MEANING, "unit": HEAP_UNIT,
                "min": HEAP_MIB.start(), "max": HEAP_MIB.end(), "default": DEFAULT_HEAP_MIB,
                "invalid_exit": USAGE,
            }]),
        ),
    ])
}

/// `text` with its help-layout line breaks and indentation folded to spaces.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

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
    let mut text = format!("Usage: {OVERVIEW_USAGE}\n\nCommands:\n");
    for spec in COMMANDS {
        text.push_str(&format!("  {:width$}  {}\n", spec.name, spec.summary));
    }
    let end = format!(
        "\n\
{OPTIONS_RULE}

Examples:
  reshiki --cli convert --smiles 'CCO' -o ethanol.mol
  reshiki --cli convert scheme.rsk -o figure.pdf
  reshiki --cli render scheme.rsk -o preview.png
  reshiki --cli analyze --smiles 'c1ccccc1O'
  reshiki --cli name --smiles 'CC(=O)O'
  reshiki --cli compose proposal.json -o scheme.svg

Output:
  {STDOUT_CONTRACT}
  Warnings and errors go to stderr.

Exit status:
  {SUCCESS:<2}  {SUCCESS_MEANING}
  {FAILURE:<2}  {FAILURE_MEANING}
  {USAGE:<2}  {USAGE_MEANING}
  {RESOURCE_EXIT:<2}  {RESOURCE_MEANING} (see {HEAP_VARIABLE})

{environment}
Files named on the command line are read and written with your own
permissions. On Windows, release builds cannot write to a console window:
redirect the output, for example `reshiki --cli info > info.json`.

Documentation: {GUIDE_URL}
",
        environment = heap_environment_help(),
    );
    text.push_str(&end);
    text
}

const ANALYZE_HELP: &str = "\
Prints the molecular properties of a structure or drawing as one JSON line:
SMILES, formula, mass, exact mass, logP, TPSA, hydrogen-bond donors and
acceptors, rings, unpaired electrons, InChI and InChIKey, under
value.analysis. The input is read as for convert (reshiki --cli help convert).
";

const INFO_HELP: &str = "\
Prints ReShiki's versions, the limits it enforces and the input and output
formats as one JSON line.

grants and files describe the file tools of reshiki --mcp; the command line
uses neither.
";

/// The agent reference's Proposal section: the schema and an example.
const PROPOSAL_URL: &str = "https://reshiki.com/developer/agent-reference/#proposal";

/// Convert and compose print the export receipt's detail as a warning.
const EXPORT_DETAIL_HELP: &str = "\
Export details, such as PNG size and dpi, are printed to stderr as a
`reshiki: warning:` line; this does not mean the command failed.
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
                 .rsmi, .cdxml, .cdx and .inchi are read as such. Crystal files
                 (.cif, .pdb, .ent, .vasp, .poscar, .contcar, .xyz, .extxyz,
                 or POSCAR/CONTCAR names without a molecular extension)
                 are not supported by the CLI, including with --from auto.
                 Other names are read as SMILES.
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

{EXPORT_DETAIL_HELP}",
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

PROPOSAL is a JSON file, or - for standard input, of at most {text} bytes;
its replace_ids must be empty. The Proposal schema and an example are at
{PROPOSAL_URL}
Run `reshiki --cli schema proposal` for the schema.

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

{EXPORT_DETAIL_HELP}",
        usage = COMPOSE.usage,
        text = Budgets::default().max_text_bytes,
        outputs = output_formats().join(", "),
    )
}

fn analyze_help() -> String {
    format!("Usage: {}\n\n{ANALYZE_HELP}", ANALYZE.usage)
}

fn name_help() -> String {
    let body = "\
Prints the systematic name of one connected molecule. The input is read as
for convert (reshiki --cli help convert). The name is generated locally by
the openclatura port and printed only after the OPSIN port reconstructs
the same structure.

Options:
  --json  Print the name and its status as one JSON line.

Without --json, prints only the name followed by a newline. With --json,
value contains name, preferred, status, reason and detail:
  preferred      true only when status is preferred.
  status         preferred = the preferred IUPAC name (IUPAC 2013);
                 not_preferred = systematic, known not to be the preferred
                 IUPAC name, with the reason;
                 unverified = systematic, preference not verified.
  reason         The snake_case naming-rule reason, or null for preferred.
  detail         The status detail sentence.
";
    format!("Usage: {}\n\n{body}", NAME.usage)
}

fn info_help() -> String {
    format!("Usage: {}\n\n{INFO_HELP}", INFO.usage)
}

fn tools_help() -> String {
    let body = "\
Prints one JSON line with the experimental notice, the data-not-instructions
rule, the guide URL and every operation tool reshiki --mcp serves, including
its title, description, inputSchema and hints. The catalog is versioned by
api.operation_api. Run `reshiki --cli help --json` for the commands as JSON.
";
    format!("Usage: {}\n\n{body}", TOOLS.usage)
}

fn schema_help() -> String {
    let body = "\
NAME is proposal: the Proposal JSON Schema compose reads, the same schema
reshiki --mcp publishes. It is printed as one JSON line without the
experimental label, so it can be saved and used as a schema file.
";
    format!("Usage: {}\n\n{body}", SCHEMA.usage)
}

fn skill_help() -> String {
    let body = "\
Writes an Agent Skill that teaches AI agents to use ReShiki: the folder
FOLDER/NAME with SKILL.md, references/cli.md, references/mcp-tools.md,
references/limits-and-errors.md, references/setup.md,
assets/proposal-schema.json and assets/examples/esterification.json.
Review the files before you install the skill. Nothing is sent over the
network.

Options:
  -o FOLDER          The folder that holds skills, such as ~/.claude/skills,
                     .claude/skills, ~/.agents/skills or .agents/skills. It
                     must exist; it is never created.
  --name NAME        The skill's name and folder name (default reshiki):
                     1 to 64 lowercase letters, digits and hyphens, with no
                     leading, trailing or double hyphen. It must not contain
                     claude or anthropic.
  --executable PATH  The absolute path of the ReShiki executable that the
                     recipes and setup snippets run (default: this
                     executable, as the system reports its path).
  --force            Replace the generated files if FOLDER/NAME exists; other
                     files in it are kept. A symbolic link in place of
                     FOLDER/NAME or one of its generated folders is refused.
  --receipt          Print one JSON line to stdout after writing:
                     {\"skill\": {\"path\", \"name\", \"files\"}, \"api\": {…}}.

Without --force, an existing FOLDER/NAME is an error and nothing is
written. A failure part-way leaves the files already written. Without
--receipt, stdout stays empty.
";
    format!("Usage: {}\n\n{body}", SKILL.usage)
}
