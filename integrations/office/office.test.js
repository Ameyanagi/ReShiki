import test from "node:test";
import assert from "node:assert/strict";
import http from "node:http";
import https from "node:https";
import { execFileSync } from "node:child_process";
import { mkdtemp, readFile, writeFile, rm } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { randomUUID } from "node:crypto";
import { createEnvelope, validateEnvelope, decodeBase64, LIMITS } from "./protocol.js";
import { Sessions } from "./companion/sessions.js";
import { makeHandler, start } from "./companion/server.js";
import { OperationGate, applyPending, acknowledgeApplied } from "./edit-flow.js";
import { runWorker } from "./companion/worker.js";
import { HOSTS, manifest, setup } from "./setup.js";

const PNG =
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAusB9Wl2nQAAAABJRU5ErkJggg==";
const native = (value) => Buffer.from(value).toString("base64");
const envelope = async (value) =>
  createEnvelope({ version: 1, native: native(value), png: PNG, extent: [100, 200] });
const targetFor = (revision) => ({
  host: "Word",
  sessionId: randomUUID(),
  objectId: randomUUID(),
  revision,
  contentControlId: 12,
});
const deferred = () => {
  let resolve;
  const promise = new Promise((r) => {
    resolve = r;
  });
  return { promise, resolve };
};

async function fixture(t, options = {}) {
  const root = await mkdtemp(path.join(os.tmpdir(), "reshiki-office-test-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const launches = [],
    worker = async (_mode, bytes) => envelope(bytes.toString());
  const sessions = new Sessions({
    recoveryDir: root,
    executable: "/trusted/reshiki",
    worker,
    launch: async (file) => {
      launches.push(file);
    },
    ...options,
  });
  const clientId = sessions.createClient("Word"),
    original = await envelope("original"),
    target = targetFor(original.revision);
  return { root, sessions, clientId, original, target, launches };
}
async function nativeSave(f, started, text, requestId = randomUUID()) {
  const next = await envelope(text);
  await writeFile(started.recoveryFile, Buffer.from(next.native, "base64"));
  await writeFile(
    path.join(path.dirname(started.recoveryFile), "request.json"),
    JSON.stringify({
      version: 1,
      sessionId: started.sessionId,
      requestId,
      revision: next.revision,
    }),
  );
  return { next, requestId };
}
function ackBody(pending, extra = {}) {
  return {
    requestId: pending.requestId,
    previousTarget: pending.target,
    target: { ...pending.target, revision: pending.envelope.revision },
    ...extra,
  };
}

test("envelope rejects altered bytes, malformed base64, oversized input, invalid PNG and extents", async () => {
  const value = await envelope("native");
  assert.deepEqual(await validateEnvelope(value), value);
  await assert.rejects(validateEnvelope({ ...value, native: native("tampered") }), /revision/);
  assert.throws(() => decodeBase64("YR==", 20), /Noncanonical/);
  assert.throws(() => decodeBase64("AA==\n", 20), /Invalid/);
  assert.throws(() => decodeBase64("AAAA", 2), /oversized/);
  await assert.rejects(validateEnvelope({ ...value, png: native("not an image") }), /PNG/);
  await assert.rejects(validateEnvelope({ ...value, extent: [0, 1] }), /extent/);
  await assert.rejects(validateEnvelope({ ...value, command: "/evil" }), /envelope/);
  assert.equal(LIMITS.nativeBytes, 16777216);
});

test("maximum-size native base64 does not overflow the JavaScript regex stack", () => {
  const input = Buffer.alloc(LIMITS.nativeBytes, 65).toString("base64");
  const decoded = decodeBase64(input, LIMITS.nativeBytes);
  assert.equal(decoded.length, LIMITS.nativeBytes);
  assert.equal(decoded[0], 65);
  assert.equal(decoded.at(-1), 65);
});

test("native save cannot be ACKed as another request, object, session, revision or container", async (t) => {
  const f = await fixture(t),
    started = await f.sessions.start(f.clientId, f.target, f.original);
  const { next } = await nativeSave(f, started, "edited");
  const { pending } = await f.sessions.poll(started.sessionId, f.clientId);
  for (const patch of [
    { requestId: "wrong" },
    { previousTarget: { ...f.target, objectId: randomUUID() } },
    { target: { ...f.target, revision: next.revision, objectId: randomUUID() } },
    { target: { ...f.target, revision: next.revision, sessionId: randomUUID() } },
    { target: f.target },
    { target: { ...f.target, revision: next.revision, contentControlId: 13 } },
  ]) {
    await assert.rejects(
      f.sessions.acknowledge(started.sessionId, f.clientId, ackBody(pending, patch)),
    );
  }
  await assert.rejects(
    readFile(path.join(path.dirname(started.recoveryFile), "ack.json")),
    /ENOENT/,
  );
  const body = ackBody(pending);
  await f.sessions.acknowledge(started.sessionId, f.clientId, body);
  const receipt = JSON.parse(
    await readFile(path.join(path.dirname(started.recoveryFile), "ack.json")),
  );
  assert.equal(receipt.revision, next.revision);
  assert.equal(receipt.requestId, pending.requestId);
  assert.equal(receipt.accepted, true);
  assert.deepEqual(await f.sessions.acknowledge(started.sessionId, f.clientId, body), {
    accepted: true,
  });
  assert.equal((await f.sessions.poll(started.sessionId, f.clientId)).pending, null);
});

test("identical native bytes saved again require a new request ACK", async (t) => {
  const f = await fixture(t),
    started = await f.sessions.start(f.clientId, f.target, f.original);
  await nativeSave(f, started, "edited", "first");
  const first = (await f.sessions.poll(started.sessionId, f.clientId)).pending;
  await f.sessions.acknowledge(started.sessionId, f.clientId, ackBody(first));
  await nativeSave(f, started, "edited", "second");
  const second = (await f.sessions.poll(started.sessionId, f.clientId)).pending;
  assert.equal(second.requestId, "second");
  assert.equal(second.envelope.revision, first.envelope.revision);
  assert.equal(second.target.revision, first.envelope.revision);
});

test("Finish releases active capacity; same-pane reopen is isolated from old editor late saves", async (t) => {
  const f = await fixture(t),
    first = await f.sessions.start(f.clientId, f.target, f.original);
  await nativeSave(f, first, "accepted before close");
  const { pending } = await f.sessions.poll(first.sessionId, f.clientId);
  const ack = ackBody(pending);
  await f.sessions.acknowledge(first.sessionId, f.clientId, ack);
  await f.sessions.finish(first.sessionId, f.clientId);
  assert.equal(f.sessions.sessions.size, 0);
  assert.equal(
    JSON.parse(await readFile(path.join(path.dirname(first.recoveryFile), "session.json"))).closed,
    true,
  );
  assert.equal((await f.sessions.finish(first.sessionId, f.clientId)).finished, true);
  const second = await f.sessions.start(f.clientId, ack.target, pending.envelope);
  assert.notEqual(second.sessionId, first.sessionId);
  assert.notEqual(second.recoveryFile, first.recoveryFile);
  await nativeSave(f, first, "late old native edit", "late");
  assert.equal((await f.sessions.poll(first.sessionId, f.clientId)).finished, true);
  await assert.rejects(
    f.sessions.acknowledge(first.sessionId, f.clientId, { ...ack, requestId: "late" }),
    /Unknown/,
  );
  assert.equal((await f.sessions.poll(second.sessionId, f.clientId)).pending, null);
  assert.equal(await readFile(first.recoveryFile, "utf8"), "late old native edit");
  assert.equal(await readFile(second.recoveryFile, "utf8"), "accepted before close");
});

test("actual native exit retires a completed edit and permits same-pane reopen", async (t) => {
  let onExit;
  const f = await fixture(t, {
    launch: async (_file, exited) => {
      onExit = exited;
    },
  });
  const first = await f.sessions.start(f.clientId, f.target, f.original);
  await nativeSave(f, first, "edited");
  const { pending } = await f.sessions.poll(first.sessionId, f.clientId);
  const body = ackBody(pending);
  await f.sessions.acknowledge(first.sessionId, f.clientId, body);
  onExit();
  await f.sessions.transition;
  assert.equal((await f.sessions.poll(first.sessionId, f.clientId)).finished, true);
  const reopened = await f.sessions.start(f.clientId, body.target, pending.envelope);
  assert.notEqual(reopened.sessionId, first.sessionId);
  const otherPane = f.sessions.createClient("Word");
  await assert.rejects(f.sessions.poll(first.sessionId, otherPane), /Unknown/);
  await f.sessions.nativeExited("already-removed-or-failed-launch");
});

test("native exit retains an unread save and lost final ACK is replayable after automatic retirement", async (t) => {
  let onExit;
  const f = await fixture(t, {
    launch: async (_file, exited) => {
      onExit = exited;
    },
  });
  const first = await f.sessions.start(f.clientId, f.target, f.original);
  await nativeSave(f, first, "last edit");
  onExit();
  await f.sessions.transition;
  assert.equal(f.sessions.sessions.size, 1, "unread request still needs host confirmation");
  const { pending } = await f.sessions.poll(first.sessionId, f.clientId);
  assert.ok(pending);
  await f.sessions.nativeExited(first.sessionId);
  assert.equal(f.sessions.sessions.size, 1, "pending ACK must retain the reservation");
  const body = ackBody(pending);
  await f.sessions.acknowledge(first.sessionId, f.clientId, body);
  assert.equal(f.sessions.sessions.size, 0);
  assert.deepEqual(await f.sessions.acknowledge(first.sessionId, f.clientId, body), {
    accepted: true,
  });
  await assert.rejects(
    f.sessions.acknowledge(first.sessionId, f.sessions.createClient("Word"), body),
    /Unknown/,
  );
  assert.equal((await f.sessions.poll(first.sessionId, f.clientId)).finished, true);
  assert.equal(await readFile(first.recoveryFile, "utf8"), "last edit");
});

test("concurrent starts reserve logical object and never launch it twice", async (t) => {
  const held = deferred(),
    entered = deferred();
  let count = 0;
  const f = await fixture(t, {
    worker: async (_mode, bytes) => {
      count++;
      entered.resolve();
      await held.promise;
      return envelope(bytes.toString());
    },
  });
  const first = f.sessions.start(f.clientId, f.target, f.original);
  await entered.promise;
  const second = f.sessions.start(f.clientId, f.target, f.original);
  const rejected = assert.rejects(second, /already/);
  held.resolve();
  await first;
  await rejected;
  assert.equal(count, 1);
  assert.equal(f.launches.length, 1);
});

test("overlapping polls serialize; delayed worker cannot republish an ACKed request", async (t) => {
  const held = deferred(),
    entered = deferred();
  let count = 0;
  const f = await fixture(t, {
    worker: async (_mode, bytes) => {
      count++;
      if (count === 2) {
        entered.resolve();
        await held.promise;
      }
      return envelope(bytes.toString());
    },
  });
  const started = await f.sessions.start(f.clientId, f.target, f.original);
  await nativeSave(f, started, "edited");
  const a = f.sessions.poll(started.sessionId, f.clientId);
  await entered.promise;
  const b = f.sessions.poll(started.sessionId, f.clientId);
  held.resolve();
  const resultA = await a,
    resultB = await b;
  assert.deepEqual(resultA, resultB);
  assert.equal(count, 2);
  await f.sessions.acknowledge(started.sessionId, f.clientId, ackBody(resultA.pending));
  assert.equal((await f.sessions.poll(started.sessionId, f.clientId)).pending, null);
});

test("worker failure, failed host mutation and expired pane retain recovery and never ACK", async (t) => {
  let now = 1000,
    failed = false;
  const f = await fixture(t, {
    now: () => now,
    worker: async (_mode, bytes) => {
      if (failed) throw new Error("bad native");
      return envelope(bytes.toString());
    },
  });
  const started = await f.sessions.start(f.clientId, f.target, f.original);
  await nativeSave(f, started, "edited");
  failed = true;
  await assert.rejects(f.sessions.poll(started.sessionId, f.clientId), /bad native/);
  assert.equal(await readFile(started.recoveryFile, "utf8"), "edited");
  failed = false;
  const { pending } = await f.sessions.poll(started.sessionId, f.clientId);
  await f.sessions.fail(started.sessionId, f.clientId, {
    requestId: pending.requestId,
    error: "Object deleted",
  });
  await assert.rejects(
    readFile(path.join(path.dirname(started.recoveryFile), "ack.json")),
    /ENOENT/,
  );
  now += 30001;
  await assert.rejects(
    f.sessions.acknowledge(started.sessionId, f.clientId, ackBody(pending)),
    /expired/,
  );
  assert.equal(await readFile(started.recoveryFile, "utf8"), "edited");
  const otherPane = f.sessions.createClient("Word");
  await assert.rejects(f.sessions.poll(started.sessionId, otherPane), /Unknown/);
});

test("click while poll is delayed cannot own the adapter or clear polling ownership", async () => {
  const gate = new OperationGate(),
    held = deferred();
  let updates = 0,
    clicks = 0;
  const polling = gate.run(async () => {
    await held.promise;
    updates++;
  });
  assert.equal(
    await gate.run(async () => {
      clicks++;
    }),
    false,
  );
  assert.equal(gate.busy, true);
  held.resolve();
  await polling;
  assert.equal(updates, 1);
  assert.equal(clicks, 0);
  assert.equal(gate.busy, false);
  assert.equal(
    await gate.run(async () => {
      clicks++;
    }),
    true,
  );
  assert.equal(clicks, 1);
});

test("dropped first ACK response retries exact receipt without a second Office mutation", async (t) => {
  const f = await fixture(t),
    started = await f.sessions.start(f.clientId, f.target, f.original);
  await nativeSave(f, started, "edited");
  const { pending } = await f.sessions.poll(started.sessionId, f.clientId);
  let writes = 0;
  const edit = { applied: null };
  const adapter = {
    update: async (target, value) => {
      writes++;
      return { target: { ...target, revision: value.revision } };
    },
    read: async (target) => ({ target, envelope: pending.envelope }),
  };
  await applyPending(edit, pending, adapter);
  await assert.rejects(
    acknowledgeApplied(edit, async (body) => {
      await f.sessions.acknowledge(started.sessionId, f.clientId, body);
      throw new Error("response lost");
    }),
    /lost/,
  );
  assert.ok(edit.applied);
  await applyPending(edit, pending, adapter);
  await acknowledgeApplied(edit, (body) =>
    f.sessions.acknowledge(started.sessionId, f.clientId, body),
  );
  assert.equal(writes, 1);
  assert.equal(edit.applied, null);
});

test("changed readback never becomes an applied/ACKable update", async () => {
  const next = await envelope("new"),
    wrong = await envelope("wrong"),
    edit = { applied: null };
  const target = targetFor(next.revision);
  await assert.rejects(
    applyPending(
      edit,
      { requestId: "one", target, envelope: next },
      { update: async () => ({ target }), read: async () => ({ target, envelope: wrong }) },
    ),
    /read back/,
  );
  assert.equal(edit.applied, null);
});

test("HTTP API rejects hostile Host, missing/null/wrong Origin, token and content type; stale fail is awaited", async (t) => {
  const f = await fixture(t),
    token = "abc123",
    worker = async (_mode, bytes) => envelope(bytes.toString());
  let handler;
  const server = http.createServer((req, res) => handler(req, res));
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  t.after(
    () =>
      new Promise((resolve) => {
        server.closeAllConnections();
        server.close(resolve);
      }),
  );
  const port = server.address().port,
    origin = `http://localhost:${port}`,
    host = `localhost:${port}`;
  handler = makeHandler({ origin, sessions: f.sessions, worker, token });
  const request = (method, route, headers, body) =>
    new Promise((resolve, reject) => {
      const req = http.request(
        { hostname: "127.0.0.1", port, path: route, method, headers },
        (res) => {
          const chunks = [];
          res.on("data", (chunk) => chunks.push(chunk));
          res.on("end", () =>
            resolve({
              status: res.statusCode,
              json: async () => JSON.parse(Buffer.concat(chunks)),
            }),
          );
        },
      );
      req.on("error", reject);
      req.end(body);
    });
  const send = (body, override = {}, route = "/api/heartbeat") =>
    request(
      "POST",
      route,
      {
        Host: host,
        Origin: origin,
        "X-ReShiki-Token": token,
        "Content-Type": "application/json",
        ...override,
      },
      JSON.stringify(body),
    );
  for (const headers of [
    { Host: "evil.test" },
    { Origin: "" },
    { Origin: "null" },
    { Origin: "https://evil.test" },
    { "X-ReShiki-Token": "wrong" },
  ])
    assert.equal((await send({ clientId: f.clientId }, headers)).status, 403);
  assert.equal(
    (await send({ clientId: f.clientId }, { "Content-Type": "text/plain" })).status,
    415,
  );
  assert.equal((await send({ clientId: f.clientId })).status, 200);
  const started = await f.sessions.start(f.clientId, f.target, f.original);
  const failure = await send(
    { clientId: f.clientId, sessionId: started.sessionId, requestId: "stale", error: "x" },
    {},
    "/api/fail",
  );
  assert.equal(failure.status, 400);
  assert.match((await failure.json()).error, /Stale/);
  const bootstrap = await request("GET", "/bootstrap.json", {
    Host: host,
    "Sec-Fetch-Site": "cross-site",
  });
  assert.equal(bootstrap.status, 403);
});

test("Office launch metadata loads only task-pane HTML and cannot relax API or resource boundaries", async (t) => {
  const f = await fixture(t),
    token = "private-taskpane-token";
  let handler,
    workerCalls = 0;
  const server = http.createServer((req, res) => handler(req, res));
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  t.after(
    () =>
      new Promise((resolve) => {
        server.closeAllConnections();
        server.close(resolve);
      }),
  );
  const port = server.address().port,
    origin = `http://localhost:${port}`,
    host = `localhost:${port}`;
  handler = makeHandler({
    origin,
    sessions: f.sessions,
    token,
    worker: () => {
      workerCalls++;
      throw new Error("Launch metadata must never run a worker");
    },
  });
  const request = (route, method = "GET", extraHeaders = {}) =>
    new Promise((resolve, reject) => {
      const req = http.request(
        {
          hostname: "127.0.0.1",
          port,
          path: route,
          method,
          headers: {
            Host: host,
            Origin: origin,
            "X-ReShiki-Token": token,
            "X-ReShiki-Bootstrap": "1",
            "Sec-Fetch-Site": "same-origin",
            "Content-Type": "application/json",
            ...extraHeaders,
          },
        },
        (res) => {
          const chunks = [];
          res.on("data", (chunk) => chunks.push(chunk));
          res.on("end", () =>
            resolve({
              status: res.statusCode,
              headers: res.headers,
              body: Buffer.concat(chunks).toString("utf8"),
            }),
          );
        },
      );
      req.on("error", reject);
      req.end(method === "POST" ? JSON.stringify({ clientId: f.clientId }) : undefined);
    });
  const html = await readFile(new URL("./taskpane.html", import.meta.url), "utf8");
  // First URL is the actual Word for Mac request that previously returned 403.
  // Microsoft also documents the Win32 pattern in OfficeDev's debugger README.
  const queries = [
    "?_host_Info=Word$Mac$16.01$en-US$$$$0",
    "?_host_Info=Excel%24Win32%2416.01%24en-US%24%24%24%240",
    "?_host_info=PowerPoint$Mac$16.01$ja-JP$$$$0",
  ];
  for (const resource of ["/", "/taskpane.html"]) {
    for (const query of ["", ...queries]) {
      const response = await request(resource + query);
      assert.equal(response.status, 200, resource + query);
      assert.equal(response.body, html);
      assert.match(response.headers["content-type"], /^text\/html/);
      assert.equal(response.headers["cache-control"], "no-store");
      assert.ok(!response.body.includes(token));
    }
  }
  // The opaque value never enters HTML, session identity, native arguments or
  // authorization. Even HTML-shaped text is inert and receives identical bytes.
  const opaque = await request(
    "/taskpane.html?_host_Info=" + encodeURIComponent("<script>evil()</script>&host=Word"),
  );
  assert.equal(opaque.status, 200);
  assert.equal(opaque.body, html);
  for (const query of [
    "?unknown=1",
    queries[0] + "&redirect=https://evil.test",
    queries[0] + "&_host_Info=Excel",
    queries[0] + "&_host_info=Excel",
    queries[0] + "&",
    "?_host_Info=",
    "?_HOST_INFO=Word",
    "?%5Fhost_Info=Word",
    "?_host_Info=" + "a".repeat(2048),
    "?_host_Info=%",
    "?_host_Info=%GG",
    "?_host_Info=%E0%A4%A",
    "?_host_Info=%00",
    "?_host_Info=%0A",
    "?_host_Info=%7F",
  ]) {
    const response = await request("/taskpane.html" + query);
    assert.equal(response.status, 403, query);
    assert.match(response.body, /Invalid request URL/);
  }
  for (const resource of [
    "/bootstrap.json",
    "/api/heartbeat",
    "/api/client",
    "/taskpane.js",
    "/taskpane.css",
    "/host-adapters/word.js",
    "/assets/icon-32.png",
    "/companion/server.js",
    "/unknown",
  ]) {
    const response = await request(resource + queries[0]);
    assert.equal(response.status, 403, resource);
    assert.ok(!response.body.includes(token));
  }
  for (const method of ["POST", "HEAD"]) {
    assert.equal((await request("/taskpane.html" + queries[0], method)).status, 403);
  }
  assert.equal((await request("/api/heartbeat" + queries[0], "POST")).status, 403);
  assert.equal(
    (await request("/taskpane.html" + queries[0], "GET", { Host: "evil.test" })).status,
    403,
  );
  assert.equal((await request("http://evil.test/taskpane.html" + queries[0])).status, 403);
  assert.equal((await request("/taskpane.html" + queries[0] + "#extra")).status, 403);
  assert.equal(
    (await request(`http://user:password@${host}/taskpane.html${queries[0]}`)).status,
    403,
  );
  assert.equal((await request("/companion/server.js")).status, 404);
  assert.equal(workerCalls, 0);
  assert.equal(f.sessions.clients.size, 1);
  assert.equal(f.sessions.sessions.size, 0);
});

test("worker rejects unknown modes, impossible input sizes, launch failures and timeout", async (t) => {
  assert.throws(() => runWorker("/not-used", "--arbitrary-command"), /Unsupported/);
  assert.throws(
    () => runWorker("/not-used", "--libreoffice-preview", { length: LIMITS.nativeBytes + 1 }),
    /limit/,
  );
  await assert.rejects(runWorker("/does-not-exist/reshiki", "--libreoffice-preview"), /ENOENT/);
  // A controlled executable simulates a hung worker without spawning a GUI.
  if (process.platform !== "win32") {
    const dir = await mkdtemp(path.join(os.tmpdir(), "reshiki-hung-worker-"));
    t.after(() => rm(dir, { recursive: true, force: true }));
    const file = path.join(dir, "hung");
    await writeFile(file, "#!/bin/sh\nexec sleep 2\n", { mode: 0o700 });
    await assert.rejects(
      runWorker(file, "--libreoffice-preview", Buffer.alloc(0), { timeoutMs: 20 }),
      /timed out/,
    );
  }
});

test("manifests declare only the relevant stable host requirement and local HTTPS", () => {
  for (const host of HOSTS) {
    const xml = manifest(host);
    assert.ok(xml.includes(`Host Name="${host.host}"`));
    assert.ok(xml.includes(`Set Name="${host.api}" MinVersion="${host.version}"`));
    assert.ok(xml.includes("https://localhost:43127/taskpane.html"));
    assert.ok(xml.includes("<Permissions>ReadWriteDocument</Permissions>"));
  }
  assert.throws(() => manifest(HOSTS[0], 0), /port/);
});

test("setup and HTTPS startup use only supplied localhost certificate/config and refuse overwrite", async (t) => {
  try {
    execFileSync("openssl", ["version"], { stdio: "ignore" });
  } catch {
    t.skip("OpenSSL is unavailable; run the HTTPS fixture on a development machine with OpenSSL");
    return;
  }
  const root = await mkdtemp(path.join(os.tmpdir(), "reshiki-office-tls-test-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const cert = path.join(root, "localhost.crt"),
    key = path.join(root, "localhost.key");
  execFileSync(
    "openssl",
    [
      "req",
      "-x509",
      "-newkey",
      "rsa:2048",
      "-nodes",
      "-days",
      "1",
      "-subj",
      "/CN=localhost",
      "-addext",
      "subjectAltName=DNS:localhost",
      "-keyout",
      key,
      "-out",
      cert,
    ],
    { stdio: "ignore" },
  );
  const probe = http.createServer();
  await new Promise((resolve) => probe.listen(0, "127.0.0.1", resolve));
  const port = probe.address().port;
  await new Promise((resolve) => probe.close(resolve));
  const directory = path.join(root, "installation");
  const options = { executable: process.execPath, cert, key, directory, port };
  const cnOnly = path.join(root, "cn-only.crt"),
    cnKey = path.join(root, "cn-only.key");
  execFileSync(
    "openssl",
    [
      "req",
      "-x509",
      "-newkey",
      "rsa:2048",
      "-nodes",
      "-days",
      "1",
      "-subj",
      "/CN=localhost",
      "-keyout",
      cnKey,
      "-out",
      cnOnly,
    ],
    { stdio: "ignore" },
  );
  await assert.rejects(
    setup({ ...options, cert: cnOnly, key: cnKey }),
    /subject alternative names/,
  );
  await setup(options);
  await assert.rejects(setup(options), /EEXIST/);
  const config = JSON.parse(await readFile(path.join(directory, "config.json")));
  const running = await start(config);
  t.after(
    () =>
      new Promise((resolve) => {
        running.server.closeAllConnections();
        running.server.close(resolve);
      }),
  );
  assert.equal(running.server.address().address, "127.0.0.1");
  const result = await new Promise((resolve, reject) => {
    const req = https.request(
      {
        hostname: "127.0.0.1",
        servername: "localhost",
        port,
        path: "/bootstrap.json",
        ca: execFileSync("openssl", ["x509", "-in", cert, "-outform", "PEM"]),
        headers: {
          Host: `localhost:${port}`,
          "X-ReShiki-Bootstrap": "1",
          "Sec-Fetch-Site": "same-origin",
        },
      },
      (res) => {
        const chunks = [];
        res.on("data", (chunk) => chunks.push(chunk));
        res.on("end", () =>
          resolve({ status: res.statusCode, data: JSON.parse(Buffer.concat(chunks)) }),
        );
      },
    );
    req.on("error", reject);
    req.end();
  });
  assert.equal(result.status, 200);
  assert.match(result.data.token, /^[0-9a-f]{64}$/);
  for (const host of HOSTS)
    assert.ok(
      (await readFile(path.join(directory, "manifests", `${host.name}.xml`), "utf8")).includes(
        `localhost:${port}`,
      ),
    );
});
