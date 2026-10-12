use super::*;

#[test]
fn mcp_usage_bytes_match_the_original_launcher_help() {
    const EXPECTED: &str = "\
Experimental: ReShiki's agent tools, schemas and results may change between releases.
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
    assert_eq!(MCP_USAGE.as_bytes(), EXPECTED.as_bytes());
}
