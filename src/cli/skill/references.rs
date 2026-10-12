//! Skill references rendered from live command help, operation contracts and
//! MCP setup options. No reference depends on a checked-in documentation page.
use super::{
    PROPOSAL_EXAMPLE, PROPOSAL_SCHEMA,
    template::{code, fence, posix_word, powershell_call},
};
use crate::cli::{
    MCP_USAGE, commands,
    reference::{
        budget_section, cell, command_sections, error_sections, file_formats_section, row, string,
        tool_section,
    },
};
use reshiki_agent::ops::{
    budget::Budgets,
    catalog::{RESULT_CONVENTIONS, catalog_json},
};
use serde_json::{Value, json};

/// A placeholder the user replaces with an existing, absolute folder path.
const AGENT_FOLDER: &str = "AGENT_FOLDER";
/// ReShiki's command line in every setup snippet.
const MCP_ARGS: [&str; 3] = ["--mcp", "--allow-write", AGENT_FOLDER];
/// Registers the stdio server for every project of the user.
const CLAUDE_ADD: &str = "claude mcp add --transport stdio --scope user reshiki --";
/// Codex waits 10 s for a server to start by default; a cold start can take longer.
const CODEX_STARTUP_SECONDS: u32 = 20;

/// Every command's help, verbatim, with its overview link in this file.
pub(super) fn cli() -> String {
    let mut text = String::from("# Command line\n\n");
    command_sections(&mut text, "##");
    text
}

/// Tool parameters, hints and results, plus the local Proposal schema location.
pub(super) fn tools() -> String {
    let mut text = format!("# MCP tools\n\n{RESULT_CONVENTIONS}\n\n");
    let catalog = catalog_json();
    for tool in catalog.as_array().into_iter().flatten() {
        tool_section(&mut text, tool, "##");
    }
    text.push_str(&format!(
        "## Proposal\n\n`compose` lays out a Proposal. Its JSON Schema is {PROPOSAL_SCHEMA} in \
this skill's folder, which `reshiki --cli schema proposal` also prints, and \
{PROPOSAL_EXAMPLE} is a complete example.\n",
    ));
    text
}

/// Default budgets, operation and file errors, formats, exits and heap ceiling.
pub(super) fn limits() -> String {
    let mut text = String::from("# Limits and errors\n\n");
    budget_section(&mut text);
    error_sections(&mut text);
    file_formats_section(&mut text);
    let catalog = commands::catalog_json(None);
    text.push_str(
        "\n## Command-line exit codes\n\n\
Reported by `reshiki --cli help --json`. See setup.md for MCP usage and exit status.\n\n\
| Exit | Meaning |\n| --- | --- |\n",
    );
    for entry in catalog
        .get("exit_codes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        row(
            &mut text,
            &[value(entry, "code"), cell(string(entry, "meaning"))],
        );
    }
    text.push_str(
        "\n## Heap ceiling\n\n\
The same environment variable controls the `--cli` and `--mcp` processes.\n\n\
| Variable | Meaning | Unit | Minimum | Maximum | Default | Invalid exit |\n\
| --- | --- | --- | --- | --- | --- | --- |\n",
    );
    for entry in catalog
        .get("environment")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        row(
            &mut text,
            &[
                code(string(entry, "name")),
                cell(string(entry, "meaning")),
                cell(string(entry, "unit")),
                value(entry, "min"),
                value(entry, "max"),
                value(entry, "default"),
                value(entry, "invalid_exit"),
            ],
        );
    }
    text
}

/// Client configurations with the generated skill's executable filled in.
/// The folder remains a visible placeholder: only the user chooses grants.
pub(super) fn setup(executable: &str) -> String {
    let mut text = format!(
        "# MCP client setup\n\n\
ReShiki executable: {}\n\n\
Use absolute paths. Replace `{AGENT_FOLDER}` with an existing dedicated folder for agent work, \
and quote that folder for your shell. `~`, `$HOME` and `%LOCALAPPDATA%` are not expanded in \
JSON or TOML. ReShiki refuses grants of the home folder, its parent folders, its own data \
folder and the folder it is installed in.\n\n\
`--allow-write` lets `file_save` write in a granted folder. A write grant does not include \
reading: add `--allow-read` with that folder to let `file_open` reopen saved files. Folders \
are read when the server starts; a missing or refused folder stops it with exit code 2. \
Restart the client after changing grants. The user must choose and configure these \
grants.\n\n\
## Claude Code\n\nIn a POSIX shell:\n\n",
        code(executable),
    );
    let server = |executable: &str| format!("{CLAUDE_ADD} {executable} {}\n", MCP_ARGS.join(" "));
    fence(&mut text, "sh", &server(&posix_word(executable)));
    text.push_str("In PowerShell:\n\n");
    // Here the executable is an argument to `claude`, not a PowerShell call.
    let call = powershell_call(executable);
    fence(
        &mut text,
        "powershell",
        &server(call.trim_start_matches("& ")),
    );
    text.push_str(
        "Everything after the required `--` separator is ReShiki's command line. Prefer user \
scope: project-scoped servers from `.mcp.json` can load without asking in `claude -p` and SDK \
sessions. `CLAUDE_PROJECT_DIR` grants nothing. Run `/mcp` in a Claude Code session to inspect \
the server and its tools.\n\n\
## Claude Desktop\n\n\
Open **Settings → Developer → Edit Config** and add this entry under `mcpServers` in \
`claude_desktop_config.json`. Keep any other server entries.\n\n",
    );
    let desktop = json!({
        "mcpServers": {
            "reshiki": {"command": executable, "args": MCP_ARGS},
        },
    });
    // Value's alternate Display uses serde_json's pretty serializer, which
    // escapes quotes and Windows backslashes rather than interpolating JSON.
    fence(&mut text, "json", &format!("{desktop:#}\n"));
    text.push_str(
        "The config is at `~/Library/Application Support/Claude/claude_desktop_config.json` \
on macOS and `%APPDATA%\\Claude\\claude_desktop_config.json` on Windows. Quit and reopen \
Claude Desktop to start the server.\n\n\
## Codex\n\nAdd this entry to `~/.codex/config.toml`:\n\n",
    );
    let toml_args: Vec<_> = MCP_ARGS.iter().map(|arg| json!(arg).to_string()).collect();
    let codex = format!(
        "[mcp_servers.reshiki]\ncommand = {}\nargs = [{}]\n\
startup_timeout_sec = {CODEX_STARTUP_SECONDS}\ntool_timeout_sec = {}\n",
        json!(executable),
        toml_args.join(", "),
        Budgets::default().op_deadline.as_secs(),
    );
    fence(&mut text, "toml", &codex);
    text.push_str(
        "These quoted strings preserve the executable path, including backslashes on Windows. \
The tool timeout matches ReShiki's default operation deadline, so a large layout or render \
can finish. Restart Codex after changing the configuration.\n\n\
## MCP command-line help\n\n",
    );
    fence(&mut text, "text", &MCP_USAGE);
    text
}

fn value(entry: &Value, key: &str) -> String {
    entry.get(key).map(Value::to_string).unwrap_or_default()
}
