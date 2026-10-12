//! Shared MCP help and heap limits for headless entry points.

use reshiki_agent::stability::NOTICE;
use std::sync::LazyLock;

/// The MCP command-line help, beginning with the agent API's experimental notice.
pub static MCP_USAGE: LazyLock<String> = LazyLock::new(|| format!("{NOTICE}\n{MCP_USAGE_BODY}"));

const MCP_USAGE_BODY: &str = "\
Usage: reshiki --mcp [--log-level <level>] [--allow-read <folder>]... [--allow-write <folder>]...

Serves the Model Context Protocol on stdin and stdout; it is meant to be
launched by an MCP client. Diagnostics go to stderr.

Options:
  --log-level <level>     error, warn (default), info or debug
  --allow-read <folder>   Let file_open read files in <folder>; repeatable
  --allow-write <folder>  Let file_save write files in <folder>; repeatable
  -h, --help              Show this help

Folders listed in agent-access.json in ReShiki's data folder are granted too.
";

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
