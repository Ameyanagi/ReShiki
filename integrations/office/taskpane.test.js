import test from "node:test";
import assert from "node:assert/strict";
import { createEnvelope } from "./protocol.js";
import { fakeOffice } from "./host-adapters/tests/fake-office.js";

test("Excel pane keeps insertion available but never re-enables or launches unsupported native editing", async (t) => {
  const fake = fakeOffice("Excel");
  fake.state.desktop = false;
  fake.dependencies.Office.onReady = async () => {};
  const ids = [
    "support",
    "actions",
    "status",
    "sessions",
    "paste",
    "file",
    "edit",
    "copy",
    "excel-note",
  ];
  const nodes = new Map(
    ids.map((id) => [
      id,
      {
        id,
        disabled: false,
        hidden: false,
        textContent: "",
        listeners: {},
        addEventListener(name, handler) {
          this.listeners[name] = handler;
        },
      },
    ]),
  );
  const events = {},
    timers = [],
    requests = [];
  const value = await createEnvelope({
    version: 1,
    native: Buffer.from("opaque native").toString("base64"),
    png: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aZ1sAAAAASUVORK5CYII=",
    extent: [2540, 1270],
  });
  const replacement = {
    Office: fake.dependencies.Office,
    window: {
      Excel: fake.dependencies.Excel,
      addEventListener(name, handler) {
        events[name] = handler;
      },
    },
    document: {
      getElementById: (id) => nodes.get(id),
      querySelectorAll: () => ["paste", "file", "edit", "copy"].map((id) => nodes.get(id)),
    },
    setTimeout: (callback) => {
      timers.push(callback);
      return timers.length;
    },
    fetch: async (url, options) => {
      requests.push({ url, body: options?.body });
      const result =
        url === "/bootstrap.json"
          ? { token: "test-token" }
          : url === "/api/client"
            ? { clientId: "test-client" }
            : url === "/api/heartbeat"
              ? { active: true }
              : url === "/api/clipboard/read"
                ? { envelope: value }
                : url === "/api/clipboard/write"
                  ? { copied: true }
                  : null;
      assert.ok(result, `Unexpected API request: ${url}`);
      return { ok: true, json: async () => result };
    },
  };
  const previous = new Map(
    Object.keys(replacement).map((key) => [key, Object.getOwnPropertyDescriptor(globalThis, key)]),
  );
  Object.assign(globalThis, replacement);
  t.after(() => {
    events.pagehide?.();
    for (const [key, descriptor] of previous) {
      if (descriptor) Object.defineProperty(globalThis, key, descriptor);
      else delete globalThis[key];
    }
  });
  await import("./taskpane.js?excel-capability-regression");
  const drain = () => new Promise((resolve) => setImmediate(resolve));
  await drain();
  assert.equal(nodes.get("edit").disabled, true);
  assert.equal(nodes.get("paste").disabled, false);
  assert.match(nodes.get("support").textContent, /ExcelApiDesktop/);
  await nodes.get("paste").listeners.click();
  assert.equal(fake.state.objects.filter((object) => !object.deleted).length, 1);
  assert.equal(
    nodes.get("edit").disabled,
    true,
    "insertion cleanup must retain the edit-only capability restriction",
  );
  await nodes.get("copy").listeners.click();
  assert.equal(nodes.get("edit").disabled, true);
  // Invoking a handler directly bypasses the DOM disabled state and proves the
  // event's own guard also prevents a native launch.
  await nodes.get("edit").listeners.click();
  assert.equal(
    requests.some((request) => request.url === "/api/edit"),
    false,
  );
  await timers.shift()();
  await drain();
  assert.equal(nodes.get("edit").disabled, true, "poll cleanup must retain the edit restriction");
  assert.equal(nodes.get("paste").disabled, false);
});
