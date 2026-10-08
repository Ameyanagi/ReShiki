"""Verify a relocated application runs chemistry without Python, uv, or a checkout."""

import argparse
import base64
import json
import os
import subprocess
import sys
import tempfile
import time
from collections import Counter
from pathlib import Path

from agent_api_client import (
    LEGACY_REVISIONS,
    MODERN,
    TOOLS,
    ProtocolError,
    StdioClient,
    modern_meta,
    result,
)

# Codex waits 10 s for a server by default (startup_timeout_sec,
# https://learn.chatgpt.com/docs/extend/mcp?surface=cli).
AGENT_STARTUP = 5.0
AGENT_STARTUP_WARNING = 2.0
AGENT_CALL = 120.0
QUARANTINE_STARTUP = 20.0
CLI_LIMIT = 120
# `--cli bogus` still running after this long means the GUI started.
CLI_GUI_LIMIT = 30
# How long worker processes may take to follow the server out.
LINGER_GRACE = 5.0
ETHANOL_INCHI = "InChI=1S/C2H6O/c1-2-3/h3H,2H2,1H3"
PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"


def verify_payload(package):
    """Reject a stale chemistry project or interpreter in a native distribution."""
    for entry in package.rglob("*"):
        relative = entry.relative_to(package)
        # These are required attribution records, not executable dependencies.
        if "Licenses" in relative.parts:
            continue
        name = entry.name.lower()
        if name in {
            "reshiki-inchi-helper",
            "reshiki-inchi-helper.exe",
            "reshiki-clipboard",
            "reshiki-print",
        }:
            raise ValueError(
                f"Separate worker executable in single-application package: {relative}"
            )
        if (
            name in {"chemistry", ".venv", "__pycache__", "uv.lock", "pyproject.toml", "pyvenv.cfg"}
            or entry.suffix.lower() in {".py", ".pyc", ".pyo", ".pyd"}
            or name in {"python", "python.exe", "python3", "python3.exe", "uv", "uv.exe"}
            or name.startswith(("libpython", "python3.", "python31", "rdkit"))
        ):
            raise ValueError(f"Python chemistry payload in native package: {relative}")


def verify_single_executable(binary, package):
    """Portable archives contain exactly one PE, ELF or Mach-O executable."""
    binary = Path(binary).resolve(strict=True)
    found = []
    for entry in Path(package).rglob("*"):
        if not entry.is_file():
            continue
        with entry.open("rb") as stream:
            header = stream.read(4)
        if header.startswith(b"MZ") or header in {
            b"\x7fELF",
            b"\xcf\xfa\xed\xfe",
            b"\xfe\xed\xfa\xcf",
        }:
            found.append(entry.resolve())
    if found != [binary]:
        raise ValueError(f"Expected only the application executable in {package}; found {found}")


def verify_macos_workers(binary):
    """Exercise packaged worker dispatch without changing the clipboard or printing."""
    with tempfile.TemporaryDirectory(prefix="ReShiki native workers ") as temporary:
        root = Path(temporary)
        requests = [
            ("--clipboard-worker", b"{}", b"Invalid clipboard request or data"),
            (
                "--clipboard-worker",
                b'{"operation":"write","representations":[]}',
                b"No supported clipboard representations",
            ),
            ("--print-worker", b"{}", b"Invalid print request"),
            (
                "--print-worker",
                json.dumps({"path": str(root / "missing.pdf"), "title": "Smoke check"}).encode(),
                b"Could not read the print snapshot",
            ),
            ("--print-worker", b"x" * 65537, b"Native request is too large"),
        ]
        for mode, request, error in requests:
            result = subprocess.run(
                [str(Path(binary).resolve()), mode],
                input=request,
                cwd=root,
                capture_output=True,
                timeout=15,
            )
            if result.returncode != 1 or result.stdout or result.stderr.strip() != error:
                raise ValueError(f"Packaged {mode} did not reject its invalid request correctly")
    print("Packaged clipboard and print worker entry points passed.")


def _sanitized_environment(root, user_data=None):
    """The environment of a packaged run: no PATH, Python, uv, checkout or
    external helper, and home, cache and data folders inside `root`."""
    empty_path = root / "Empty PATH"
    empty_path.mkdir()
    environment = dict(os.environ)
    for key in (
        "RESHIKI_INCHI_HELPER",
        "MORUNO_INCHI_HELPER",
        "VIRTUAL_ENV",
        "PYTHONPATH",
        "PYTHONHOME",
        "UV_PROJECT_ENVIRONMENT",
    ):
        environment.pop(key, None)
    environment.update(
        PATH=str(empty_path),
        HOME=str(root / "Home"),
        USERPROFILE=str(root / "Home"),
        XDG_CACHE_HOME=str(root / "Cache"),
        XDG_DATA_HOME=str(root / "Data"),
        APPDATA=str(root / "Roaming"),
        LOCALAPPDATA=str(root / "Local"),
        UV_CACHE_DIR=str(root / "UV cache"),
        UV_OFFLINE="1",
        PYTHONNOUSERSITE="1",
    )
    for prefix in ("RESHIKI", "MORUNO"):
        for key, value in {
            "PYTHON": root / "Missing Python",
            "REFERENCE_PYTHON": root / "Missing reference Python",
            "UV": root / "Missing uv",
            "ROOT": root / "No checkout",
            "RUNTIME_DIR": root / "Chemistry runtime",
            "DATA_DIR": user_data if user_data is not None else root / "User data",
        }.items():
            environment[f"{prefix}_{key}"] = str(value)
    return environment


def verify_runtime(binary, package, *, user_data=None):
    package = Path(package).resolve(strict=True)
    binary = Path(binary).resolve(strict=True)
    if not binary.is_relative_to(package):
        raise ValueError("Runtime check executable must belong to the package")
    verify_payload(package)
    with tempfile.TemporaryDirectory(prefix="ReShiki native runtime ") as temporary:
        root = Path(temporary)
        environment = _sanitized_environment(root, user_data)
        # Both launches must run from the shipped binaries. UV_OFFLINE is an extra
        # guard against uv downloads, not a claim that this is a network sandbox.
        for launch in range(2):
            response = subprocess.run(
                [str(binary), "--engine-check"],
                cwd=root,
                env=environment,
                capture_output=True,
                text=True,
                timeout=120,
                check=True,
            )
            analysis = json.loads(response.stdout).get("analysis", {})
            if analysis.get("formula") != "C2H6O" or analysis.get("smiles") != "CCO":
                raise ValueError(
                    f"Native chemistry check did not return ethanol (launch {launch + 1})"
                )
            if (root / "Chemistry runtime").exists() or (root / "UV cache").exists():
                raise ValueError("Native chemistry check created a Python chemistry environment")
            verify_payload(root)
        verify_payload(package)
    print("Native chemistry passed twice with Python, uv, and the checkout unavailable.")


def _exit_limit(binary):
    """Seconds a server may take to exit after EOF: 10, or 20 for Windows ARM64."""
    with Path(binary).open("rb") as stream:
        header = stream.read(64)
        if header[:2] == b"MZ" and len(header) == 64:
            stream.seek(int.from_bytes(header[60:64], "little"))
            if stream.read(6) == b"PE\0\0" + (0xAA64).to_bytes(2, "little"):
                return 20
    return 10


def _value(response, tool):
    if response.get("isError") is not False:
        raise ValueError(f"Packaged agent API {tool} failed: {response}")
    return (response.get("structuredContent") or {}).get("value") or {}


def _tool_error(response, tool):
    if response.get("isError") is not True:
        raise ValueError(f"Packaged agent API accepted {tool}: {response}")
    return (response.get("structuredContent") or {}).get("error") or {}


def _check_tools(client):
    listed = result(client.call("tools/list", {}, AGENT_CALL)).get("tools", [])
    names = tuple(tool.get("name") for tool in listed)
    if names != TOOLS:
        raise ValueError(f"Packaged agent API lists {names}, not {TOOLS}")


def _ethanol(client):
    """Imports and analyzes ethanol, whose InChI relaunches the executable."""
    imported = client.tool("import", {"format": "smiles", "text": "CCO"}, AGENT_CALL)
    document = _value(imported, "import").get("document")
    analyzed = client.tool("analyze", {"document": document, "ids": None}, AGENT_CALL)
    analysis = _value(analyzed, "analyze").get("analysis") or {}
    if analysis.get("formula") != "C2H6O" or analysis.get("inchi") != ETHANOL_INCHI:
        raise ValueError(f"Packaged agent API did not analyze ethanol: {analysis}")
    return document


def _finish(client, binary, cancelled=None):
    """Ends a session: exit 0 at the end of input, and through the end of stdout
    one response to each request, except at most one to the `cancelled` one, and
    no request or notification from the server (P1 sends none)."""
    if client.close(_exit_limit(binary)) != 0:
        raise ValueError("Packaged MCP server did not exit 0 at the end of its input")
    messages = [json.loads(line) for line in client.lines]
    initiated = [message for message in messages if "method" in message]
    if initiated:
        raise ValueError(f"Packaged MCP server sent requests or notifications: {initiated}")
    answered = Counter(message.get("id") for message in messages)
    if cancelled is not None and answered.pop(cancelled, 0) > 1:
        raise ValueError("Packaged MCP server answered a cancelled request twice")
    expected = Counter(request for request in client.request_ids if request != cancelled)
    if answered != expected:
        raise ValueError(
            f"Packaged MCP server sent responses {dict(answered)}, not one to each of "
            f"{sorted(expected)}"
        )


def _listening_sockets(pid):
    """Lines for the TCP sockets `pid` listens on, or None without a tool to list them."""
    try:
        if sys.platform == "darwin":
            listed = subprocess.run(
                ["lsof", "-nP", "-a", "-p", str(pid), "-iTCP", "-sTCP:LISTEN"],
                capture_output=True,
                text=True,
                timeout=60,
            )
            # lsof exits 1 when nothing matches.
            if listed.returncode not in {0, 1}:
                return None
            return [line for line in listed.stdout.splitlines()[1:] if line.strip()]
        if sys.platform.startswith("linux"):
            listed = subprocess.run(
                ["ss", "-ltnp"], capture_output=True, text=True, timeout=60, check=True
            )
            return [line for line in listed.stdout.splitlines() if f"pid={pid}," in line]
        if sys.platform == "win32":
            listed = subprocess.run(
                ["netstat", "-ano"], capture_output=True, text=True, timeout=120, check=True
            )
            return [
                line
                for line in listed.stdout.splitlines()
                if "LISTENING" in line.split() and line.split()[-1] == str(pid)
            ]
    except (OSError, subprocess.SubprocessError):
        return None
    return None


def _running_executables():
    """(pid, executable path) of the processes this user can see."""
    if sys.platform == "darwin":
        listed = subprocess.run(
            ["ps", "-axo", "pid=,comm="], capture_output=True, text=True, timeout=60, check=True
        )
        pairs = [line.split(None, 1) for line in listed.stdout.splitlines()]
    elif sys.platform == "win32":
        listed = subprocess.run(
            [
                "powershell",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                'Get-Process | Where-Object Path | ForEach-Object { "$($_.Id) $($_.Path)" }',
            ],
            capture_output=True,
            text=True,
            timeout=120,
            check=True,
        )
        pairs = [line.split(None, 1) for line in listed.stdout.splitlines()]
    else:
        pairs = []
        for link in Path("/proc").glob("[0-9]*/exe"):
            try:
                pairs.append([link.parent.name, os.readlink(link)])
            except OSError:
                continue
    return [(int(pid), path.strip()) for pid, path in (pair for pair in pairs if len(pair) == 2)]


def _check_lingering(roots):
    """No process whose executable lies under `roots` remains."""
    prefixes = {
        os.path.normcase(str(path)).rstrip("\\/") + os.sep
        for root in roots
        for path in (Path(root), Path(root).resolve())
    }
    deadline = time.monotonic() + LINGER_GRACE
    while True:
        lingering = [
            (pid, path)
            for pid, path in _running_executables()
            if any(os.path.normcase(path).startswith(prefix) for prefix in prefixes)
        ]
        if not lingering:
            return
        if time.monotonic() >= deadline:
            raise ValueError(f"Processes outlived the packaged agent checks: {lingering}")
        time.sleep(0.2)


def _inventory(root):
    entries = {}
    for entry in root.rglob("*"):
        if entry.is_symlink():
            entries[entry.relative_to(root)] = ("link", os.readlink(entry))
        elif entry.is_dir():
            entries[entry.relative_to(root)] = ("folder",)
        else:
            status = entry.stat()
            entries[entry.relative_to(root)] = ("file", status.st_size, status.st_mtime_ns)
    return entries


def _main_session(binary, root, environment, clients):
    """The 2026-07-28 session with folder grants; returns the listening-socket result."""
    client = StdioClient(
        [binary, "--mcp", "--log-level", "info", "--allow-read", "In", "--allow-write", "Out"],
        cwd=root,
        env=environment,
    )
    clients.append(client)
    discovered = result(client.discover(AGENT_STARTUP))
    startup = time.monotonic() - client.started
    if startup > AGENT_STARTUP:
        raise ValueError(f"Packaged MCP server answered discover after {startup:.1f} s")
    if startup > AGENT_STARTUP_WARNING:
        print(f"Warning: the packaged MCP server answered discover after {startup:.1f} s.")
    if MODERN not in discovered.get("supportedVersions", []):
        raise ValueError(f"Packaged MCP server does not support {MODERN}: {discovered}")
    server = (discovered.get("_meta") or {}).get("io.modelcontextprotocol/serverInfo") or {}
    if "experimental" not in str(server.get("title", "")):
        raise ValueError(f"Packaged MCP server is not labelled experimental: {discovered}")
    _check_tools(client)
    document = _ethanol(client)
    render = {
        "document": document,
        "format": "png",
        "max_width": None,
        "max_height": None,
        "ids": None,
    }
    rendered = client.tool("render", render, AGENT_CALL)
    _value(rendered, "render")
    images = [
        item.get("data", "") for item in rendered.get("content", []) if item.get("type") == "image"
    ]
    if len(images) != 1 or not base64.b64decode(images[0]).startswith(PNG_SIGNATURE):
        raise ValueError("Packaged agent API render did not return one PNG image")

    def save(path):
        arguments = {
            "document": document,
            "path": str(path),
            "format": None,
            "overwrite": False,
            "pages": None,
        }
        return client.tool("file_save", arguments, AGENT_CALL)

    _value(save(root / "Out/ethanol.svg"), "file_save")
    if not (root / "Out/ethanol.svg").is_file():
        raise ValueError("Packaged agent API file_save did not write Out/ethanol.svg")
    escape = _tool_error(save(f"{root / 'In'}{os.sep}..{os.sep}escape.svg"), "an escaping path")
    if not str(escape.get("message", "")).startswith("path_invalid:"):
        raise ValueError(f"Escaping file_save failed differently: {escape}")
    if (root / "escape.svg").exists():
        raise ValueError("Escaping file_save created a file")
    executable = _tool_error(save(root / "Out/x.exe"), "an .exe file")
    if executable.get("code") != "access_denied" or not str(
        executable.get("message", "")
    ).startswith("extension_not_allowed:"):
        raise ValueError(f"file_save of an .exe failed differently: {executable}")

    # A cancelled request gets no response, or one complete response when it
    # finished first (https://modelcontextprotocol.io/specification/2026-07-28/basic/patterns/cancellation).
    cancelled = client.start_request(
        "tools/call", {"name": "render", "arguments": render, "_meta": modern_meta()}
    )
    client.notify("notifications/cancelled", {"requestId": cancelled, "reason": "package check"})
    result(client.discover(AGENT_CALL))
    # A malformed notification gets no output (crates/mcp/src/framing.rs).
    seen = len(client.lines)
    client.notify("notifications/cancelled", ["malformed"])
    sentinel = client.start_request("server/discover", {"_meta": modern_meta()})
    result(client.receive(sentinel, AGENT_CALL))
    since = [json.loads(line) for line in client.lines[seen:]]
    if [message.get("id") for message in since if message.get("id") != cancelled] != [sentinel]:
        raise ValueError(f"Packaged MCP server answered a malformed notification: {since}")
    unsupported = client.call("server/discover", {}, AGENT_CALL, version="1900-01-01")
    if (unsupported.get("error") or {}).get("code") != -32022:
        raise ValueError(f"Unsupported protocol version was not -32022: {unsupported}")

    sockets = _listening_sockets(client.pid)
    if sockets:
        raise ValueError(f"Packaged MCP server listens on TCP: {sockets}")
    # Also catches output the malformed notification caused after the sentinel.
    _finish(client, binary, cancelled)
    return "unavailable" if sockets is None else "none"


def _session(binary, root, environment, clients, revision=None):
    """A session without grants: initialize for a legacy `revision`, else discover."""
    client = StdioClient([binary, "--mcp", "--log-level", "info"], cwd=root, env=environment)
    clients.append(client)
    if revision is None:
        result(client.discover(AGENT_CALL))
    else:
        client.initialize(revision, AGENT_CALL)
    _check_tools(client)
    _ethanol(client)
    _finish(client, binary)


def _verify_cli(binary, root, environment):
    def cli(*args, limit=CLI_LIMIT):
        try:
            completed = subprocess.run(
                [str(binary), "--cli", *args],
                cwd=root,
                env=environment,
                stdin=subprocess.DEVNULL,
                capture_output=True,
                encoding="utf-8",
                errors="replace",
                timeout=limit,
            )
        except subprocess.TimeoutExpired:
            raise ValueError(
                f"reshiki --cli {args[0]} still ran after {limit} s; the GUI may have started"
            ) from None
        if "panicked" in completed.stderr:
            raise ValueError(f"reshiki --cli {args[0]} panicked: {completed.stderr}")
        return completed

    converted = cli("convert", "--smiles", "CCO", "-o", str(Path("Out", "e.mol")))
    molfile = root / "Out/e.mol"
    if converted.returncode != 0 or "M  END" not in (
        molfile.read_text(encoding="utf-8") if molfile.is_file() else ""
    ):
        raise ValueError(f"reshiki --cli convert failed: {converted.stderr}")
    analyzed = cli("analyze", "--smiles", "CCO")
    try:
        analysis = json.loads(analyzed.stdout)
    except ValueError:
        analysis = None
    if not isinstance(analysis, dict):
        analysis = {}
    formula = ((analysis.get("value") or {}).get("analysis") or {}).get("formula")
    if analyzed.returncode != 0 or analysis.get("experimental") is not True or formula != "C2H6O":
        raise ValueError(f"reshiki --cli analyze failed: {analyzed.stdout} {analyzed.stderr}")
    helped = cli("help")
    if helped.returncode != 0 or "Experimental" not in helped.stdout:
        raise ValueError("reshiki --cli help does not say Experimental")
    bogus = cli("bogus", limit=CLI_GUI_LIMIT)
    if bogus.returncode != 2:
        raise ValueError(f"reshiki --cli bogus exited {bogus.returncode}, not 2")


def _verify_quarantined(binary):
    """A quarantined copy of the app must still serve MCP (macOS signed builds)."""
    app = next((parent for parent in binary.parents if parent.suffix == ".app"), None)
    if app is None:
        raise ValueError("The quarantine check needs an executable inside an app bundle")
    with tempfile.TemporaryDirectory(prefix="ReShiki quarantine check ") as temporary:
        root = Path(temporary).resolve()
        copy = root / app.name
        subprocess.run(["ditto", str(app), str(copy)], check=True, timeout=600)
        inner = copy / binary.relative_to(app)
        stamp = f"0081;{int(time.time()):x};ReShiki package check;"
        for target in (copy, inner):
            subprocess.run(
                ["xattr", "-w", "com.apple.quarantine", stamp, str(target)], check=True, timeout=60
            )
        environment = _sanitized_environment(root)
        client = None
        try:
            try:
                client = StdioClient([inner, "--mcp"], cwd=root, env=environment)
                result(client.discover(QUARANTINE_STARTUP))
            except (OSError, ProtocolError):
                print(
                    "Gatekeeper blocked or delayed the quarantined executable: an MCP client "
                    "starting a downloaded copy may fail the same way (open question).",
                    file=sys.stderr,
                )
                raise
            _finish(client, inner)
        finally:
            if client is not None:
                client.kill()
        _check_lingering([copy])
    print("Quarantined app copy served MCP.")


def verify_agent_api(binary, package, *, quarantine=False):
    """Drive the packaged `--mcp` server and `--cli` from a sanitized environment.

    Checks the startup time, labels, catalog, chemistry through the relocated
    executable, render, folder grants, cancellation, malformed input, version
    errors, every advertised legacy revision and exit at EOF; then that only
    Out/ changed, that the server listened on no TCP socket (or that no tool
    could tell), and that no process from the package or temporary folder
    remains. `quarantine` also launches a quarantined copy of the app.
    """
    package = Path(package).resolve(strict=True)
    binary = Path(binary).resolve(strict=True)
    if not binary.is_relative_to(package):
        raise ValueError("Agent API check executable must belong to the package")
    with tempfile.TemporaryDirectory(prefix="ReShiki agent check ") as temporary:
        root = Path(temporary).resolve()
        environment = _sanitized_environment(root)
        for folder in ("In", "Out"):
            (root / folder).mkdir()
        link = None
        if os.name != "nt":
            (root / "bin").mkdir()
            link = root / "bin" / binary.name
            link.symlink_to(binary)
        before = _inventory(root)
        clients = []
        try:
            sockets = _main_session(binary, root, environment, clients)
            for revision in LEGACY_REVISIONS:
                _session(binary, root, environment, clients, revision)
            if link is not None:
                _session(link, root, environment, clients)
            _verify_cli(binary, root, environment)
        finally:
            for client in clients:
                client.kill()
        for client in clients:
            stderr = client.stderr(timeout=10)
            if "experimental" not in stderr or "panicked" in stderr:
                raise ValueError(f"Packaged MCP server stderr is unexpected: {stderr}")
        for key in ("HOME", "APPDATA", "LOCALAPPDATA"):
            folder = Path(environment[key])
            if folder.exists() and any(
                name in entry.name.lower()
                for entry in folder.rglob("*")
                for name in ("reshiki", "moruno")
            ):
                raise ValueError(f"Packaged agent checks created a ReShiki data folder in {key}")
        after = _inventory(root)
        changed = sorted(
            str(path)
            for path in before.keys() | after.keys()
            if path.parts[:1] != ("Out",) and before.get(path) != after.get(path)
        )
        if changed:
            raise ValueError(f"Packaged agent checks changed files outside Out/: {changed}")
        _check_lingering([package, root])
    if quarantine:
        _verify_quarantined(binary)
    print(f"Packaged MCP server and CLI passed; listening sockets: {sockets}.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--package", type=Path, required=True)
    parser.add_argument("--binary", type=Path, required=True)
    args = parser.parse_args()
    verify_runtime(args.binary, args.package)


if __name__ == "__main__":
    main()
