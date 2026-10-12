use super::*;

#[test]
fn mcp_usage_bytes_pin_help_exit_status_environment_and_docs() {
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

Exit status:
  0   input ended and every reply was delivered
  1   stdout failed or closed, a reply was not delivered, a request went
      unanswered, or the server could not start
  2   usage, grant or RESHIKI_AGENT_HEAP_MB error
  75  the memory ceiling was reached

Environment:
  RESHIKI_AGENT_HEAP_MB  memory ceiling in MiB, 256-16384
                         (default 2048); an invalid value exits 2

agent-access.json grants folders too (in ReShiki's data folder; see the docs).

Documentation: https://reshiki.com/guide/agents/
";
    assert_eq!(MCP_USAGE.as_bytes(), EXPECTED.as_bytes());
    assert_eq!(MCP_USAGE.lines().next(), Some(NOTICE));
    let documentation = format!("Documentation: {GUIDE_URL}");
    assert_eq!(MCP_USAGE.lines().last(), Some(documentation.as_str()));
}
