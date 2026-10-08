"""The packaged agent API check must pass a well-behaved server and fail closed."""

import base64
import json
import os
import queue
import re
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
import check_runtime_dependencies as runtime
from agent_api_client import LEGACY_REVISIONS, MODERN, TOOLS, ProtocolError


class Pipe:
    """A child's stdout or stderr: lines put by the fake, b"" at end of file."""

    def __init__(self):
        self.lines = queue.Queue()

    def readline(self):
        return self.lines.get()


class Stdin:
    def __init__(self, server):
        self.server = server
        self.buffer = b""

    def write(self, data):
        self.buffer += data
        while b"\n" in self.buffer:
            line, self.buffer = self.buffer.split(b"\n", 1)
            self.server.handle(json.loads(line))

    def flush(self):
        pass

    def close(self):
        self.server.end(0)


class Server:
    """A `reshiki --mcp` stand-in that answers like the real server unless a fault is set."""

    pid = 4242

    def __init__(self, faults, cwd):
        self.faults = faults
        self.cwd = Path(cwd)
        self.stdout, self.stderr = Pipe(), Pipe()
        self.stdin = Stdin(self)
        self.returncode = None
        self.renders = 0
        self.held = None
        self.sent = []
        label = "agent API" if "unlabelled_stderr" in faults else "agent API (experimental)"
        self.stderr.lines.put(f"reshiki-mcp: info: ReShiki {label}\n".encode())

    def send(self, message):
        self.sent.append(message)
        self.stdout.lines.put(json.dumps(message).encode() + b"\n")

    def handle(self, message):
        method, params = message.get("method"), message.get("params")
        if "id" not in message:
            if method == "notifications/cancelled" and isinstance(params, dict):
                if self.held is not None and self.held["id"] == params.get("requestId"):
                    self.held = None
            return
        version = ((params or {}).get("_meta") or {}).get(
            "io.modelcontextprotocol/protocolVersion", MODERN
        )
        if version not in (MODERN, *LEGACY_REVISIONS):
            error = {"code": -32022, "message": "Unsupported protocol version"}
            return self.send({"jsonrpc": "2.0", "id": message["id"], "error": error})
        reply = {"jsonrpc": "2.0", "id": message["id"], "result": self.result(method, params)}
        if self.held is False:
            self.held = reply
        elif reply["result"] is not None:
            self.send(reply)

    def result(self, method, params):
        if method == "server/discover":
            if "silent" in self.faults:
                return None
            if "garbage" in self.faults:
                self.stdout.lines.put(b"ReShiki started\n")
            title = "ReShiki" if "unlabelled" in self.faults else "ReShiki (experimental)"
            server = {"name": "reshiki", "title": title, "version": "1.0.0"}
            if "legacy_server_info" in self.faults:
                return {"supportedVersions": [MODERN], "serverInfo": server}
            meta = {"io.modelcontextprotocol/serverInfo": server}
            return {"supportedVersions": [MODERN, *LEGACY_REVISIONS], "_meta": meta}
        if method == "initialize":
            return {"protocolVersion": params["protocolVersion"], "capabilities": {}}
        if method == "tools/list":
            return {"tools": [{"name": name} for name in TOOLS]}
        return self.tool(params["name"], params["arguments"])

    def tool(self, name, arguments):
        def value(content):
            return {"isError": False, "structuredContent": {"value": content}, "content": []}

        def error(code, message):
            error = {"code": code, "message": message}
            return {"isError": True, "structuredContent": {"error": error}, "content": []}

        if name == "import":
            return value({"document": "doc_1"})
        if name == "analyze":
            return value({"analysis": {"formula": "C2H6O", "inchi": runtime.ETHANOL_INCHI}})
        if name == "render":
            self.renders += 1
            if self.renders == 2 and "suppress_cancelled" in self.faults:
                self.held = False
            png = base64.b64encode(runtime.PNG_SIGNATURE + b"image").decode()
            rendered = value({})
            rendered["content"] = [{"type": "image", "mimeType": "image/png", "data": png}]
            return rendered
        path = Path(arguments["path"])
        if ".." in path.parts:
            return error("invalid_arguments", f'path_invalid: invalid path "{path}"')
        if path.suffix == ".exe":
            return error("access_denied", f'extension_not_allowed: "{path}"')
        path.write_text("<svg/>")
        if "stray_file" in self.faults:
            (self.cwd / "stray.txt").write_text("outside Out/")
        return value({"receipt": {"path": str(path)}})

    def end(self, code):
        if self.returncode is None:
            self.returncode = code
            self.stdout.lines.put(b"")
            self.stderr.lines.put(b"")

    def poll(self):
        return self.returncode

    def wait(self, timeout=None):
        return self.returncode

    def kill(self):
        self.end(-9)


def cli(command, **kwargs):
    """`reshiki --cli` as the release binary answers it."""
    name = command[2]
    if name == "convert":
        (Path(kwargs["cwd"]) / command[-1]).write_text("\nM  END\n", encoding="utf-8")
        return subprocess.CompletedProcess(command, 0, "", "")
    if name == "analyze":
        line = {"experimental": True, "value": {"analysis": {"formula": "C2H6O"}}}
        return subprocess.CompletedProcess(command, 0, json.dumps(line), "")
    if name == "help":
        return subprocess.CompletedProcess(command, 0, "Experimental: tools may change\n", "")
    return subprocess.CompletedProcess(command, 2, "", "reshiki: unknown command\n")


class PackagedAgentApiTests(unittest.TestCase):
    def verify(self, *faults, running=()):
        """Runs verify_agent_api against Server with `faults`; returns its sessions."""
        servers = []

        def popen(command, **kwargs):
            servers.append(Server(set(faults), kwargs["cwd"]))
            return servers[-1]

        with tempfile.TemporaryDirectory() as temporary:
            package = Path(temporary)
            binary = package / "reshiki"
            binary.write_bytes(b"\x7fELF")
            with (
                patch("agent_api_client.subprocess.Popen", side_effect=popen),
                patch("check_runtime_dependencies.subprocess.run", side_effect=cli),
                patch("check_runtime_dependencies._listening_sockets", return_value=[]),
                patch(
                    "check_runtime_dependencies._running_executables",
                    return_value=[(7, str(binary.resolve()))] if running else [],
                ),
                patch("check_runtime_dependencies.AGENT_STARTUP", 0.5),
                patch("check_runtime_dependencies.LINGER_GRACE", 0),
            ):
                runtime.verify_agent_api(binary, package)
        return servers

    def test_well_behaved_server_and_cli_pass(self):
        servers = self.verify()
        # The granted session, one per legacy revision and, on Unix, one through a symlink.
        self.assertEqual(len(servers), 1 + len(LEGACY_REVISIONS) + (os.name != "nt"))
        self.assertTrue(all(server.returncode == 0 for server in servers))

    def test_both_cancellation_outcomes_pass(self):
        for faults, renders in (((), 2), (("suppress_cancelled",), 1)):
            with self.subTest(faults=faults):
                sent = self.verify(*faults)[0].sent
                images = [message for message in sent if message.get("result", {}).get("content")]
                self.assertEqual(len(images), renders)

    def test_protocol_label_file_process_and_startup_faults_fail(self):
        for fault, error, message in (
            ("garbage", ProtocolError, "Invalid stdout line"),
            # A modern server names itself in _meta, not in a legacy serverInfo.
            ("legacy_server_info", ValueError, "not labelled experimental"),
            ("unlabelled", ValueError, "not labelled experimental"),
            ("unlabelled_stderr", ValueError, "stderr is unexpected"),
            ("stray_file", ValueError, "outside Out/: \\['stray.txt'\\]"),
            ("lingering", ValueError, "Processes outlived"),
            ("silent", ProtocolError, "No answer"),
        ):
            with self.subTest(fault=fault), self.assertRaisesRegex(error, message):
                self.verify(fault, running=fault == "lingering")

    def test_tools_match_the_operation_catalog(self):
        catalog = json.loads(
            (ROOT / "tests/fixtures/agent-contract/ops-catalog.json").read_text(encoding="utf-8")
        )
        self.assertEqual(TOOLS, tuple(tool["name"] for tool in catalog))

    def test_revisions_match_the_server(self):
        source = (ROOT / "crates/mcp/src/server.rs").read_text(encoding="utf-8")
        supported = source.split("pub const SUPPORTED", 1)[1].split("];", 1)[0]
        versions = [
            "-".join(match) for match in re.findall(r"V_(\d{4})_(\d{2})_(\d{2})", supported)
        ]
        self.assertEqual(versions, [MODERN, *LEGACY_REVISIONS])


if __name__ == "__main__":
    unittest.main()
