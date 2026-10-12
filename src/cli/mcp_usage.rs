//! Shared MCP help and heap limits for headless entry points.

use super::{FAILURE, SUCCESS, USAGE};
use heap::{DEFAULT_HEAP_MIB, HEAP_MIB, HEAP_VARIABLE};
use reshiki_agent::stability::{GUIDE_URL, NOTICE};
use reshiki_process_heap::RESOURCE_EXIT;
use std::sync::LazyLock;

/// The MCP command-line help, beginning with the agent API's experimental notice.
pub static MCP_USAGE: LazyLock<String> = LazyLock::new(|| {
    format!(
        "{NOTICE}\n{MCP_USAGE_BODY}\n\
Exit status:
  {SUCCESS:<2}  input ended and every reply was delivered
  {FAILURE:<2}  stdout failed or closed, a reply was not delivered, a request went
      unanswered, or the server could not start
  {USAGE:<2}  usage, grant or {HEAP_VARIABLE} error
  {RESOURCE_EXIT:<2}  the memory ceiling was reached

{environment}
agent-access.json grants folders too (in ReShiki's data folder; see the docs).

Documentation: {GUIDE_URL}
",
        environment = heap_environment_help(),
    )
});

const MCP_USAGE_BODY: &str = "\
Usage: reshiki --mcp [--log-level <level>] [--allow-read <folder>]... [--allow-write <folder>]...

Serves the Model Context Protocol on stdin and stdout; it is meant to be
launched by an MCP client. Diagnostics go to stderr.

Options:
  --log-level <level>     error, warn (default), info or debug
  --allow-read <folder>   Let file_open read files in <folder>; repeatable
  --allow-write <folder>  Let file_save write files in <folder>; repeatable
  -h, --help              Show this help
";

/// The `Environment:` block of the `--cli` and `--mcp` help: what
/// [`HEAP_VARIABLE`](heap::HEAP_VARIABLE) sets.
pub(super) fn heap_environment_help() -> String {
    let indent = " ".repeat(HEAP_VARIABLE.len() + 4);
    format!(
        "\
Environment:
  {HEAP_VARIABLE}  memory ceiling in MiB, {min}-{max}
{indent}(default {DEFAULT_HEAP_MIB}); an invalid value exits {USAGE}
",
        min = HEAP_MIB.start(),
        max = HEAP_MIB.end(),
    )
}

/// Heap ceiling settings shared by `--mcp` and `--cli`.
pub mod heap {
    use std::ops::RangeInclusive;

    /// The environment variable that sets the heap ceiling of `--mcp` and
    /// `--cli`.
    pub const HEAP_VARIABLE: &str = "RESHIKI_AGENT_HEAP_MB";
    /// The heap ceiling without [`HEAP_VARIABLE`]: 2 GiB.
    pub const DEFAULT_HEAP_MIB: usize = 2048;
    /// The ceilings [`HEAP_VARIABLE`] may set, in MiB.
    pub const HEAP_MIB: RangeInclusive<usize> = 256..=16384;
}

#[cfg(test)]
mod tests;
