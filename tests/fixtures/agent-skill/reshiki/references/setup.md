# MCP client setup

ReShiki executable: `/opt/reshiki/reshiki`

Use absolute paths. Replace `AGENT_FOLDER` with an existing dedicated folder for agent work, and quote that folder for your shell. `~`, `$HOME` and `%LOCALAPPDATA%` are not expanded in JSON or TOML. ReShiki refuses grants of the home folder, its parent folders, its own data folder and the folder it is installed in.

`--allow-write` lets `file_save` write in a granted folder. A write grant does not include reading: add `--allow-read` with that folder to let `file_open` reopen saved files. Folders are read when the server starts; a missing or refused folder stops it with exit code 2. Restart the client after changing grants. The user must choose and configure these grants.

## Claude Code

In a POSIX shell:

```sh
claude mcp add --transport stdio --scope user reshiki -- '/opt/reshiki/reshiki' --mcp --allow-write AGENT_FOLDER
```

In PowerShell:

```powershell
claude mcp add --transport stdio --scope user reshiki -- '/opt/reshiki/reshiki' --mcp --allow-write AGENT_FOLDER
```

Everything after the required `--` separator is ReShiki's command line. Prefer user scope: project-scoped servers from `.mcp.json` can load without asking in `claude -p` and SDK sessions. `CLAUDE_PROJECT_DIR` grants nothing. Run `/mcp` in a Claude Code session to inspect the server and its tools.

## Claude Desktop

Open **Settings → Developer → Edit Config** and add this entry under `mcpServers` in `claude_desktop_config.json`. Keep any other server entries.

```json
{
  "mcpServers": {
    "reshiki": {
      "args": [
        "--mcp",
        "--allow-write",
        "AGENT_FOLDER"
      ],
      "command": "/opt/reshiki/reshiki"
    }
  }
}
```

The config is at `~/Library/Application Support/Claude/claude_desktop_config.json` on macOS and `%APPDATA%\Claude\claude_desktop_config.json` on Windows. Quit and reopen Claude Desktop to start the server.

## Codex

Add this entry to `~/.codex/config.toml`:

```toml
[mcp_servers.reshiki]
command = "/opt/reshiki/reshiki"
args = ["--mcp", "--allow-write", "AGENT_FOLDER"]
startup_timeout_sec = 20
tool_timeout_sec = 120
```

These quoted strings preserve the executable path, including backslashes on Windows. The tool timeout matches ReShiki's default operation deadline, so a large layout or render can finish. Restart Codex after changing the configuration.

## MCP command-line help

```text
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
```

