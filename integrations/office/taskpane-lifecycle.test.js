import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, readFile, writeFile, rm } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { createEnvelope } from "./protocol.js";
import { Sessions } from "./companion/sessions.js";
import { createHostAdapter } from "./host-adapters/index.js";
import { fakeOffice } from "./host-adapters/tests/fake-office.js";

const PNG =
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aZ1sAAAAASUVORK5CYII=";
const envelope = (text) =>
  createEnvelope({
    version: 1,
    native: Buffer.from(text).toString("base64"),
    png: PNG,
    extent: [2540, 1270],
  });
const deferred = () => {
  let resolve;
  const promise = new Promise((r) => {
    resolve = r;
  });
  return { promise, resolve };
};

async function pane(t) {
  const root = await mkdtemp(path.join(os.tmpdir(), "reshiki-pane-lifecycle-"));
  const fake = fakeOffice("Word");
  fake.dependencies.Office.onReady = async () => {};
  const original = await envelope("original");
  const inserted = await createHostAdapter(fake.dependencies).insert(original);
  let now = 1000,
    clientId,
    beforeRequest;
  const requests = [],
    timers = [],
    events = {};
  const sessions = new Sessions({
    recoveryDir: root,
    executable: "/trusted/reshiki",
    worker: async (_mode, bytes) => envelope(bytes.toString()),
    launch: async () => {},
    now: () => now,
  });
  const node = (id) => ({
    id,
    disabled: false,
    hidden: false,
    textContent: "",
    listeners: {},
    children: [],
    addEventListener(name, handler) {
      this.listeners[name] = handler;
    },
    append(child) {
      this.children.push(child);
    },
    remove() {
      this.removed = true;
    },
  });
  const nodes = new Map(
    ["support", "actions", "status", "sessions", "paste", "file", "edit", "copy", "excel-note"].map(
      (id) => [id, node(id)],
    ),
  );
  const replacement = {
    Office: fake.dependencies.Office,
    window: {
      Word: fake.dependencies.Word,
      addEventListener(name, handler) {
        events[name] = handler;
      },
    },
    document: {
      getElementById: (id) => nodes.get(id),
      querySelectorAll: () => ["paste", "file", "edit", "copy"].map((id) => nodes.get(id)),
      createElement: node,
    },
    setTimeout: (callback, delay) => {
      timers.push({ callback, delay });
      return timers.length;
    },
    fetch: async (url, options) => {
      const body = options?.body ? JSON.parse(options.body) : {};
      requests.push({ url, body });
      try {
        await beforeRequest?.(url, body);
        let value;
        if (url === "/bootstrap.json") value = { token: "test-token" };
        else if (url === "/api/client") {
          clientId = sessions.createClient(body.host);
          value = { clientId };
        } else {
          sessions.touch(body.clientId);
          if (url === "/api/heartbeat") value = { active: true };
          else if (url === "/api/edit")
            value = await sessions.start(body.clientId, body.target, body.envelope);
          else if (url === "/api/poll") value = await sessions.poll(body.sessionId, body.clientId);
          else if (url === "/api/fail")
            value = await sessions.fail(body.sessionId, body.clientId, body);
          else if (url === "/api/ack")
            value = await sessions.acknowledge(body.sessionId, body.clientId, body);
          else throw new Error(`Unexpected request ${url}`);
        }
        return { ok: true, json: async () => value };
      } catch (error) {
        return { ok: false, json: async () => ({ error: error.message }) };
      }
    },
  };
  const previous = new Map(
    Object.keys(replacement).map((key) => [key, Object.getOwnPropertyDescriptor(globalThis, key)]),
  );
  Object.assign(globalThis, replacement);
  t.after(async () => {
    events.pagehide?.();
    for (const [key, descriptor] of previous) {
      if (descriptor) Object.defineProperty(globalThis, key, descriptor);
      else delete globalThis[key];
    }
    await rm(root, { recursive: true, force: true });
  });
  await import(`./taskpane.js?lifecycle-${encodeURIComponent(t.name)}`);
  await new Promise((resolve) => setImmediate(resolve));
  await nodes.get("edit").listeners.click();
  const session = [...sessions.sessions.values()][0];
  assert.ok(session);
  return {
    fake,
    original,
    inserted,
    sessions,
    session,
    nodes,
    requests,
    events,
    advance: (milliseconds) => {
      now += milliseconds;
    },
    intercept: (callback) => {
      beforeRequest = callback;
    },
    runTimer: async (delay) => {
      const index = timers.findIndex((timer) => timer.delay === delay);
      assert.ok(index >= 0, `No scheduled ${delay} ms timer`);
      const [{ callback }] = timers.splice(index, 1);
      await callback();
    },
    save: async (requestId) => {
      const next = await envelope("edited");
      await writeFile(session.file, Buffer.from(next.native, "base64"));
      await writeFile(
        path.join(session.directory, "request.json"),
        JSON.stringify({ version: 1, sessionId: session.id, requestId, revision: next.revision }),
      );
      return next;
    },
    receipt: async () => JSON.parse(await readFile(path.join(session.directory, "ack.json"))),
    client: () => clientId,
  };
}

test("pane retries a locked target only for a new native Save and retries a lost failure receipt", async (t) => {
  const f = await pane(t);
  f.fake.object().cannotEdit = true;
  await f.save("locked-save");
  let dropFailure = true;
  f.intercept((url) => {
    if (url === "/api/fail" && dropFailure) {
      dropFailure = false;
      throw new Error("lost failure response");
    }
  });
  const before = f.fake.state.mutations.length;
  await f.runTimer(500);
  assert.equal(f.fake.state.mutations.length, before);
  assert.equal(f.session.pending.requestId, "locked-save");
  await f.runTimer(500);
  assert.equal(f.session.pending, null);
  assert.equal(f.fake.state.mutations.length, before);
  await assert.rejects(f.receipt(), /ENOENT/);
  assert.match(f.nodes.get("sessions").children[0].children[0].textContent, /save again/);
  f.fake.object().cannotEdit = false;
  const next = await f.save("unlocked-save");
  await f.runTimer(500);
  assert.equal((await f.receipt()).requestId, "unlocked-save");
  assert.equal(
    (await createHostAdapter(f.fake.dependencies).readSelected()).envelope.revision,
    next.revision,
  );
});

test("pane blocks uncertain partial writes and leaves later native saves unacknowledged", async (t) => {
  const f = await pane(t);
  await f.save("partial-save");
  f.fake.failAfter("word.replace", () => {
    f.fake.object().tag = "changed-by-another-editor";
  });
  await f.runTimer(500);
  assert.equal(f.requests.find((request) => request.url === "/api/fail").body.retryable, false);
  const mutations = f.fake.state.mutations.length;
  const polls = f.requests.filter((request) => request.url === "/api/poll").length;
  await f.save("later-save");
  await f.runTimer(500);
  assert.equal(f.fake.state.mutations.length, mutations);
  assert.equal(f.requests.filter((request) => request.url === "/api/poll").length, polls);
  await assert.rejects(f.receipt(), /ENOENT/);
});

for (const stage of ["Office update", "save acknowledgement"]) {
  test(`pane heartbeats during a ${stage} longer than the client lease`, async (t) => {
    const f = await pane(t);
    await f.save("long-save");
    const entered = deferred(),
      held = deferred();
    if (stage === "Office update") {
      const run = f.fake.dependencies.Word.run;
      let delay = true;
      f.fake.dependencies.Word.run = async (callback) => {
        if (delay) {
          delay = false;
          entered.resolve();
          await held.promise;
        }
        return run(callback);
      };
    } else {
      f.intercept(async (url) => {
        if (url === "/api/ack") {
          entered.resolve();
          await held.promise;
        }
      });
    }
    const polling = f.runTimer(500);
    await entered.promise;
    assert.equal(f.nodes.get("edit").disabled, true);
    for (let i = 0; i < 7; i++) {
      f.advance(5000);
      await f.runTimer(5000);
    }
    assert.equal(f.requests.filter((request) => request.url === "/api/heartbeat").length, 8);
    assert.equal(f.sessions.touch(f.client()).host, "Word");
    held.resolve();
    await polling;
    assert.equal((await f.receipt()).accepted, true);
    assert.equal(f.nodes.get("edit").disabled, false);
    f.events.pagehide();
    const count = f.requests.length;
    await f.runTimer(5000);
    await f.runTimer(500);
    assert.equal(f.requests.length, count, "pagehide stops both loops");
  });
}
