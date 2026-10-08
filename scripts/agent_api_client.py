"""A standard-library MCP stdio client for checking `reshiki --mcp`.

This is a second, independent implementation of the protocol beside the Rust
goldens: newline-delimited JSON-RPC 2.0 on a child's stdin and stdout
(https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/stdio).
Modern requests carry the per-request `_meta` of 2026-07-28
(https://modelcontextprotocol.io/specification/2026-07-28/basic/index); legacy
revisions start with `initialize`.
"""

import json
import queue
import subprocess
import threading
import time

MODERN = "2026-07-28"
LEGACY_REVISIONS = ("2025-11-25", "2025-06-18")
# The final operation catalog, in tools/list order
# (tests/fixtures/agent-contract/ops-catalog.json).
TOOLS = (
    "info",
    "document_new",
    "document_list",
    "document_close",
    "import",
    "inspect",
    "analyze",
    "render",
    "export",
    "file_open",
    "file_save",
    "compose",
    "apply",
)
CLIENT_INFO = {"name": "reshiki-package-check", "version": "1.0.0"}


class ProtocolError(ValueError):
    """The server broke the protocol, failed a request or did not answer in time."""


def modern_meta(version=MODERN):
    return {
        "io.modelcontextprotocol/protocolVersion": version,
        "io.modelcontextprotocol/clientInfo": CLIENT_INFO,
        "io.modelcontextprotocol/clientCapabilities": {},
    }


def parse_line(raw):
    """One stdout line as a JSON-RPC 2.0 message a server may send, or ProtocolError."""
    try:
        text = raw.decode("utf-8")
        if not text.endswith("\n"):
            raise ValueError("unterminated line")
        message = json.loads(text)
    except ValueError as error:
        raise ProtocolError(f"Invalid stdout line {raw[:200]!r}: {error}") from None
    if not isinstance(message, dict) or message.get("jsonrpc") != "2.0":
        raise ProtocolError(f"Not a JSON-RPC 2.0 object: {raw[:200]!r}")
    if isinstance(message.get("method"), str):
        return message
    error = message.get("error")
    if ("result" in message) == (error is not None):
        raise ProtocolError(f"Neither one result nor one error: {raw[:200]!r}")
    if error is not None and not (
        isinstance(error, dict)
        and isinstance(error.get("code"), int)
        and isinstance(error.get("message"), str)
    ):
        raise ProtocolError(f"Malformed error object: {raw[:200]!r}")
    if "result" in message and "id" not in message:
        raise ProtocolError(f"Result without an id: {raw[:200]!r}")
    return message


class StdioClient:
    """A child process spoken to over newline-delimited JSON-RPC.

    Reader threads move stdout lines and stderr chunks to queues, so every wait
    has a timeout. Every stdout line is recorded in `lines` and must be a
    JSON-RPC message (`parse_line`). `revision` is None for 2026-07-28, whose
    requests carry `_meta`, or the revision `initialize` negotiated.
    """

    def __init__(self, command, *, cwd=None, env=None):
        self.started = time.monotonic()
        self.process = subprocess.Popen(
            [str(value) for value in command],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            cwd=cwd,
            env=env,
        )
        stdin = self.process.stdin
        if stdin is None:
            raise ProtocolError("The server's stdin is not a pipe")
        self._stdin = stdin
        self.revision = None
        self.lines = []
        self.unclaimed = []
        self.eof = False
        self._next_id = 0
        self._stdout = queue.Queue()
        self._stderr = queue.Queue()
        self._stderr_text = []
        self._stderr_closed = False
        for stream, sink in (
            (self.process.stdout, self._stdout),
            (self.process.stderr, self._stderr),
        ):
            threading.Thread(target=self._pump, args=(stream, sink), daemon=True).start()

    @property
    def pid(self):
        return self.process.pid

    @staticmethod
    def _pump(stream, sink):
        try:
            for line in iter(stream.readline, b""):
                sink.put(line)
        except (OSError, ValueError):
            pass
        sink.put(None)

    def _take(self, deadline, waiting_for):
        try:
            raw = self._stdout.get(timeout=max(0.0, deadline - time.monotonic()))
        except queue.Empty:
            raise ProtocolError(f"No answer to {waiting_for} in time") from None
        if raw is None:
            self.eof = True
            raise ProtocolError(f"stdout closed before {waiting_for}")
        self.lines.append(raw.decode("utf-8", "replace"))
        return parse_line(raw)

    def send(self, message):
        self._stdin.write(json.dumps(message, separators=(",", ":")).encode("utf-8") + b"\n")
        self._stdin.flush()

    def notify(self, method, params=None):
        message = {"jsonrpc": "2.0", "method": method}
        if params is not None:
            message["params"] = params
        self.send(message)

    def receive(self, request_id, timeout):
        """The message with `request_id`; others are kept in `unclaimed`."""
        for message in self.unclaimed:
            if message.get("id") == request_id:
                self.unclaimed.remove(message)
                return message
        deadline = time.monotonic() + timeout
        while True:
            message = self._take(deadline, f"request {request_id!r}")
            if message.get("id") == request_id:
                return message
            self.unclaimed.append(message)

    def start_request(self, method, params):
        """Sends a request without waiting; returns its id."""
        self._next_id += 1
        self.send({"jsonrpc": "2.0", "id": self._next_id, "method": method, "params": params})
        return self._next_id

    def request(self, method, params, timeout):
        return self.receive(self.start_request(method, params), timeout)

    def call(self, method, params, timeout, *, version=MODERN):
        """A request, with the 2026-07-28 `_meta` unless `initialize` chose a legacy revision."""
        if self.revision is None:
            params = dict(params, _meta=modern_meta(version))
        return self.request(method, params, timeout)

    def discover(self, timeout):
        return self.call("server/discover", {}, timeout)

    def initialize(self, revision, timeout):
        """The legacy handshake: initialize, then notifications/initialized."""
        response = self.request(
            "initialize",
            {"protocolVersion": revision, "capabilities": {}, "clientInfo": CLIENT_INFO},
            timeout,
        )
        if result(response).get("protocolVersion") != revision:
            raise ProtocolError(f"initialize {revision} answered {response}")
        self.revision = revision
        self.notify("notifications/initialized")
        return response

    def tool(self, name, arguments, timeout):
        """The result of a tools/call, which may be a tool error (isError)."""
        return result(self.call("tools/call", {"name": name, "arguments": arguments}, timeout))

    def stderr(self, timeout=0.0):
        """The stderr read so far, after waiting up to `timeout` for it to close."""
        deadline = time.monotonic() + timeout
        while not self._stderr_closed:
            try:
                chunk = self._stderr.get(timeout=max(0.0, deadline - time.monotonic()))
            except queue.Empty:
                break
            if chunk is None:
                self._stderr_closed = True
            else:
                self._stderr_text.append(chunk.decode("utf-8", "replace"))
        return "".join(self._stderr_text)

    def close(self, timeout):
        """Closes stdin; returns the exit code once the process exited and stdout
        reached end of file within `timeout`. Lines read meanwhile are checked."""
        deadline = time.monotonic() + timeout
        try:
            self._stdin.close()
        except OSError:
            pass
        try:
            code = self.process.wait(timeout=max(0.0, deadline - time.monotonic()))
        except subprocess.TimeoutExpired:
            raise ProtocolError(f"The server did not exit within {timeout} s of EOF") from None
        while not self.eof:
            try:
                self.unclaimed.append(self._take(deadline, "end of stdout"))
            except ProtocolError:
                if not self.eof:
                    raise
        return code

    def kill(self):
        if self.process.poll() is None:
            self.process.kill()
            self.process.wait()


def result(response):
    """The `result` of a response, or ProtocolError for an error response."""
    if "result" not in response:
        raise ProtocolError(f"Request failed: {response}")
    return response["result"]
