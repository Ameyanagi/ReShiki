// Opt-in mounted-editor recovery checks against real IndexedDB and Web Locks.
// Run with Bun after building packages/editor/dist; see the recovery QA note.
import assert from "node:assert/strict";
import { readFile, readdir, mkdir, writeFile } from "node:fs/promises";
import { resolve, join, sep } from "node:path";

const playwright = await import(process.env.RESHIKI_QA_PLAYWRIGHT ?? "playwright");
const packageRoot = resolve(process.env.RESHIKI_QA_RECOVERY_PACKAGE ?? "packages/editor");
const fixtures = process.env.RESHIKI_RECOVERY_QA_INPUT;
assert(fixtures, "Set RESHIKI_RECOVERY_QA_INPUT to disposable generated fixtures");
const output = resolve(
  process.env.RESHIKI_QA_RECOVERY_BROWSER_OUTPUT ?? "artifacts/recovery-browser",
);
await mkdir(output, { recursive: true });
const names = (await readdir(join(fixtures, "data-3/recovery")))
  .filter((name) => name.endsWith(".json"))
  .sort();
const snapshots = await Promise.all(
  names.map(async (name) =>
    JSON.parse(await readFile(join(fixtures, "data-3/recovery", name), "utf8")),
  ),
);
assert.equal(snapshots.length, 3);
const closedOwner = "00000000000000000000000000";
const liveOwner = "11111111111111111111111111";
const html = `<!doctype html><html><head><meta charset="utf-8"><style>html,body,#editor{width:100%;height:100%;margin:0;overflow:hidden}</style></head><body><div id="editor"></div><script type="module">
window.qaUnhandled ??= [];
window.addEventListener('unhandledrejection', event => window.qaUnhandled.push(String(event.reason)));
import { mount } from '/package/src/editor.js';
try { window.editor = await mount(document.querySelector('#editor')); window.qaReady = true; }
catch(error) { window.qaFailure = String(error); throw error; }
</script></body></html>`;
const server = Bun.serve({
  hostname: "127.0.0.1",
  port: 0,
  async fetch(request) {
    const path = new URL(request.url).pathname;
    if (path === "/app") return new Response(html, { headers: { "Content-Type": "text/html" } });
    if (path === "/seed")
      return new Response("<!doctype html><title>Disposable recovery test storage</title>", {
        headers: { "Content-Type": "text/html" },
      });
    if (!path.startsWith("/package/")) return new Response("Not found", { status: 404 });
    const file = resolve(packageRoot, decodeURIComponent(path.slice("/package/".length)));
    if (!file.startsWith(packageRoot + sep)) return new Response("Not found", { status: 404 });
    const body = Bun.file(file);
    return (await body.exists()) ? new Response(body) : new Response("Not found", { status: 404 });
  },
});
const origin = String(server.url).replace(/\/$/, "");

async function seed(page, protectedOne = false) {
  await page.goto(`${origin}/seed`);
  await page.evaluate(
    async ({ snapshots, closedOwner, liveOwner, protectedOne }) => {
      const database = await new Promise((resolve, reject) => {
        const request = indexedDB.open("reshiki", 1);
        request.onupgradeneeded = () => request.result.createObjectStore("files");
        request.onsuccess = () => resolve(request.result);
        request.onerror = () => reject(request.error);
      });
      await new Promise((resolve, reject) => {
        const transaction = database.transaction("files", "readwrite");
        snapshots.forEach((snapshot, index) => {
          const owner = protectedOne && index === 0 ? liveOwner : closedOwner;
          transaction
            .objectStore("files")
            .put(
              new TextEncoder().encode(JSON.stringify(snapshot)),
              `recovery/${owner}-${index}.json`,
            );
        });
        transaction.oncomplete = resolve;
        transaction.onabort = () => reject(transaction.error);
      });
      database.close();
    },
    { snapshots, closedOwner, liveOwner, protectedOne },
  );
}

async function kept(page) {
  return page.evaluate(async () => {
    const database = await new Promise((resolve, reject) => {
      const request = indexedDB.open("reshiki", 1);
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error);
    });
    const keys = await new Promise((resolve, reject) => {
      const request = database.transaction("files").objectStore("files").getAllKeys();
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error);
    });
    database.close();
    return keys.filter((key) => String(key).startsWith("recovery/")).sort();
  });
}

async function settle(page) {
  await page.evaluate(
    () => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))),
  );
  await page.waitForTimeout(150);
}

async function click(page, x, y) {
  await page.locator("canvas").first().focus();
  await page.mouse.move(x, y);
  await settle(page);
  await page.mouse.click(x, y, { delay: 50 });
  await settle(page);
}

async function mounted(page) {
  await page.goto(`${origin}/app`);
  await page.waitForFunction(() => window.qaReady || window.qaFailure, undefined, {
    timeout: 30000,
  });
  assert.equal(await page.evaluate(() => window.qaFailure), undefined);
  await settle(page);
}

async function waitCount(page, count) {
  for (let attempt = 0; attempt < 100; attempt++) {
    const keys = await kept(page);
    if (keys.length === count) return keys;
    await page.waitForTimeout(100);
  }
  assert.equal((await kept(page)).length, count);
}

const report = [];
try {
  for (const name of ["chromium", "webkit"]) {
    const browser = await playwright[name].launch({ headless: true });
    try {
      for (const scenario of process.argv.includes("--capture-only")
        ? ["success"]
        : [
            "success",
            "abort-retry",
            "query-retry",
            "query-restore-all",
            "query-restore-selected",
            "live-owner",
            "rejected-lock",
            "restore-selected",
          ]) {
        const context = await browser.newContext({
          viewport: { width: 1280, height: 820 },
          deviceScaleFactor: 1,
        });
        const page = await context.newPage();
        const errors = [];
        const checks = {};
        page.on("pageerror", (error) => errors.push(String(error)));
        try {
          await seed(page, scenario === "live-owner");
          let owner;
          if (scenario === "live-owner") {
            owner = await context.newPage();
            await owner.goto(`${origin}/seed`);
            await owner.evaluate((session) => {
              navigator.locks.request(`reshiki-session:${session}`, () => new Promise(() => {}));
            }, liveOwner);
            await owner.waitForFunction(
              async (session) =>
                (await navigator.locks.query()).held.some(
                  (lock) => lock.name === `reshiki-session:${session}`,
                ),
              liveOwner,
            );
          }
          if (scenario === "abort-retry") {
            await page.addInitScript(() => {
              window.qaDiscardAborts = 0;
              const remove = IDBObjectStore.prototype.delete;
              IDBObjectStore.prototype.delete = function (key) {
                const request = remove.call(this, key);
                if (String(key).startsWith("recovery/") && window.qaDiscardAborts === 0) {
                  window.qaDiscardAborts++;
                  this.transaction.abort();
                }
                return request;
              };
            });
          }
          if (scenario.startsWith("query-")) {
            await page.addInitScript(() => {
              const prototype = Object.getPrototypeOf(navigator.locks);
              const query = prototype.query;
              window.qaOwnerQueries = 0;
              window.qaOwnerQueryRejects = 0;
              window.qaRejectNextOwnerQuery = false;
              Object.defineProperty(prototype, "query", {
                value() {
                  window.qaOwnerQueries++;
                  if (window.qaRejectNextOwnerQuery) {
                    window.qaRejectNextOwnerQuery = false;
                    window.qaOwnerQueryRejects++;
                    return Promise.reject(
                      new DOMException("Injected transient owner query failure", "AbortError"),
                    );
                  }
                  return query.call(this);
                },
              });
            });
          }
          if (scenario === "rejected-lock") {
            await page.addInitScript(() => {
              const request = navigator.locks.request.bind(navigator.locks);
              Object.defineProperty(navigator.locks, "request", {
                value(name, ...args) {
                  return String(name).startsWith("reshiki-session:")
                    ? Promise.reject(
                        new DOMException("Injected asynchronous ownership rejection", "AbortError"),
                      )
                    : request(name, ...args);
                },
              });
            });
          }
          await mounted(page);
          if (scenario.startsWith("query-")) {
            await page.evaluate(() => {
              window.qaRejectNextOwnerQuery = true;
            });
          }
          await page.screenshot({ path: join(output, `${name}-${scenario}-initial.png`) });
          if (process.argv.includes("--capture-only")) continue;
          const before = await kept(page);
          assert.equal(before.length, 3);
          if (scenario === "rejected-lock") {
            await click(page, 542, 464);
            await page.keyboard.press("1");
            await page.waitForFunction(() => window.editor.isDirty());
            // Cross the five-second autosave interval while this page remains
            // live. No recovery owner may be published without its lock.
            await page.waitForTimeout(6500);
            assert.equal((await kept(page)).length, 3);
            checks.keptAfterRejectedOwnerAutosave = 3;
            checks.unprotectedDraftsPublished = 0;
            assert.equal(
              JSON.parse(await page.evaluate(() => window.editor.save("rsk"))).atoms.length,
              2,
            );
            await page.screenshot({
              path: join(output, `${name}-rejected-owner-unsaved-drawing.png`),
            });
            const healthy = await context.newPage();
            healthy.on("pageerror", (error) => errors.push(String(error)));
            await mounted(healthy);
            await healthy.screenshot({
              path: join(output, `${name}-healthy-page-after-rejected-autosave.png`),
            });
            await click(healthy, 221, 800);
            await click(healthy, 867, 497);
            await waitCount(healthy, 0);
            assert.equal(
              JSON.parse(await page.evaluate(() => window.editor.save("rsk"))).atoms.length,
              2,
              "Discard on a healthy page must not change the live rejected owner's drawing",
            );
            assert.deepEqual(await healthy.evaluate(() => window.qaUnhandled), []);
            await healthy.close();
          } else if (scenario === "restore-selected") {
            await click(page, 156, 800);
            await click(page, 500, 285);
            await page.screenshot({ path: join(output, `${name}-selected-picker.png`) });
            await click(page, 868, 617);
            const drawing = JSON.parse(await page.evaluate(() => window.editor.save("rsk")));
            assert.equal(drawing.atoms.length, snapshots[0].document.atoms.length);
          } else {
            await click(page, 221, 800);
            await page.screenshot({ path: join(output, `${name}-${scenario}-confirmation.png`) });
            await click(page, 867, 497);
            if (scenario === "abort-retry" || scenario.startsWith("query-")) {
              await page.waitForFunction(
                () => window.qaDiscardAborts === 1 || window.qaOwnerQueryRejects === 1,
              );
              await settle(page);
              assert.equal(
                (await kept(page)).length,
                3,
                "An aborted transaction must keep all files",
              );
              checks.keptAfterFailure = 3;
              checks.injectedFailures = await page.evaluate(
                () => window.qaDiscardAborts ?? window.qaOwnerQueryRejects,
              );
              await page.screenshot({
                path: join(output, `${name}-${scenario}-offer-retained.png`),
              });
              if (scenario.startsWith("query-restore-")) {
                if (scenario === "query-restore-selected") {
                  await click(page, 500, 335);
                  await click(page, 868, 617);
                } else {
                  await click(page, 776, 617);
                }
                const drawing = JSON.parse(await page.evaluate(() => window.editor.save("rsk")));
                assert.equal(
                  drawing.atoms.length,
                  snapshots[0].document.atoms.length,
                  "Restore must reverify owners without losing the offered draft",
                );
                checks.restoredAfterOwnerQueryFailure = drawing.atoms.length;
              } else {
                await click(page, 458, 617);
                await page.screenshot({
                  path: join(output, `${name}-${scenario}-retry-confirmation.png`),
                });
                await click(page, 867, 497);
              }
            }
            if (!scenario.startsWith("query-restore-")) {
              const keys = await waitCount(page, scenario === "live-owner" ? 1 : 0);
              if (scenario === "live-owner")
                assert(keys[0].includes(liveOwner), "The open page's draft must remain");
            }
          }
          await settle(page);
          await page.screenshot({ path: join(output, `${name}-${scenario}-final.png`) });
          assert.deepEqual(
            await page.evaluate(() => window.qaUnhandled),
            [],
            "No unhandled Promise rejection",
          );
          assert.deepEqual(errors, [], "No page-level runtime errors");
          report.push({
            browser: name,
            version: browser.version(),
            scenario,
            before: before.length,
            after: (await kept(page)).length,
            unhandled: [],
            result: "passed",
            ...checks,
          });
        } catch (error) {
          await page.screenshot({ path: join(output, `${name}-${scenario}-failure.png`) });
          await writeFile(
            join(output, `${name}-${scenario}-failure.json`),
            JSON.stringify(
              await page.evaluate(() => ({
                ready: window.qaReady,
                failure: window.qaFailure,
                queries: window.qaOwnerQueries,
                rejected: window.qaOwnerQueryRejects,
                armed: window.qaRejectNextOwnerQuery,
                unhandled: window.qaUnhandled,
              })),
              null,
              2,
            ),
          );
          throw error;
        } finally {
          await context.close();
        }
      }
    } finally {
      await browser.close();
    }
  }
  await writeFile(join(output, "results.json"), JSON.stringify(report, null, 2) + "\n");
  console.log(JSON.stringify(report, null, 2));
} finally {
  server.stop(true);
}
