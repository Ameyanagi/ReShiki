// Opt-in mounted-editor checks using real IndexedDB and fresh browser profiles.
// Build packages/editor/dist first. See docs/changes/qa-remember-appearance.md.
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { resolve, sep } from "node:path";

const playwright = await import(process.env.RESHIKI_QA_PLAYWRIGHT ?? "playwright");
const packageRoot = resolve(process.env.RESHIKI_QA_APPEARANCE_PACKAGE ?? "packages/editor");
const output = resolve(process.env.RESHIKI_QA_APPEARANCE_OUTPUT ?? "artifacts/appearance-browser");
await mkdir(output, { recursive: true });
const html = `<!doctype html><html><head><meta charset="utf-8"><style>html,body,#editor{width:100%;height:100%;margin:0;overflow:hidden}</style></head><body><div id="editor"></div><script type="module">
import { mount } from '/package/src/editor.js';
try { window.editor = await mount(document.querySelector('#editor'), {cjkFont:false}); window.qaReady = true; }
catch(error) { window.qaFailure = String(error); throw error; }
</script></body></html>`;
const server = Bun.serve({
  hostname: "127.0.0.1",
  port: 0,
  async fetch(request) {
    const path = new URL(request.url).pathname;
    if (path === "/app") return new Response(html, { headers: { "Content-Type": "text/html" } });
    if (!path.startsWith("/package/")) return new Response("Not found", { status: 404 });
    const file = resolve(packageRoot, decodeURIComponent(path.slice("/package/".length)));
    if (!file.startsWith(packageRoot + sep)) return new Response("Not found", { status: 404 });
    const body = Bun.file(file);
    return (await body.exists()) ? new Response(body) : new Response("Not found", { status: 404 });
  },
});
const origin = String(server.url).replace(/\/$/, "");

async function ready(page) {
  await page.waitForFunction(() => window.qaReady || window.qaFailure, undefined, {
    timeout: 60000,
  });
  assert.equal(await page.evaluate(() => window.qaFailure), undefined);
  await page.locator("canvas").first().waitFor();
  await page.waitForTimeout(150);
}
async function command(page, query) {
  await page.locator("canvas").first().focus();
  await page.keyboard.press("Escape");
  await page.keyboard.type(":");
  await page.waitForTimeout(100);
  await page.keyboard.type(query);
  await page.waitForTimeout(100);
  await page.keyboard.press("Enter");
  await page.evaluate(() => new Promise((done) => requestAnimationFrame(done)));
}
async function drawing(page) {
  return page.evaluate(async () => {
    const doc = JSON.parse(await window.editor.save("rsk"));
    // Native JSON omits the default light canvas.
    doc.canvas_theme ??= "light";
    return doc;
  });
}
async function preference(page) {
  return page.evaluate(async () => {
    const db = await new Promise((resolve, reject) => {
      const open = indexedDB.open("reshiki", 1);
      open.onsuccess = () => resolve(open.result);
      open.onerror = () => reject(open.error);
    });
    try {
      const bytes = await new Promise((resolve, reject) => {
        const get = db.transaction("files").objectStore("files").get("appearance.json");
        get.onsuccess = () => resolve(get.result);
        get.onerror = () => reject(get.error);
      });
      return bytes ? JSON.parse(new TextDecoder().decode(bytes)) : null;
    } finally {
      db.close();
    }
  });
}

const report = [];
try {
  for (const name of ["chromium", "webkit"]) {
    const browser = await playwright[name].launch({ headless: true });
    try {
      const context = await browser.newContext({
        viewport: { width: 1280, height: 820 },
        deviceScaleFactor: 1,
      });
      const page = await context.newPage();
      const errors = [];
      page.on("pageerror", (error) => errors.push(String(error)));
      page.on("dialog", (dialog) => dialog.accept());
      await page.goto(`${origin}/app`);
      await ready(page);
      const initial = await drawing(page);
      assert.equal(initial.canvas_theme, "light");
      assert.equal(await page.evaluate(() => window.editor.isDirty()), false);

      // Reload after the actual command reaches the drawing, without waiting for
      // a storage poll or artificial durability delay before navigation.
      await command(page, "Canvas: Dark");
      assert.equal((await drawing(page)).canvas_theme, "dark");
      await page.reload();
      await ready(page);
      assert.equal((await drawing(page)).canvas_theme, "dark");
      assert.equal(await page.evaluate(() => window.editor.isDirty()), false);
      assert.equal((await preference(page)).canvas_theme, "dark");
      await page.screenshot({ path: `${output}/${name}-dark-reload.png` });

      for (const [label, mode] of [
        ["Light", "light"],
        ["Dark", "dark"],
        ["Match canvas", "match_canvas"],
      ]) {
        await command(page, `Appearance: ${label}`);
        await page.reload();
        await ready(page);
        assert.equal((await preference(page)).mode, mode);
        assert.equal((await drawing(page)).canvas_theme, "dark");
        assert.equal(await page.evaluate(() => window.editor.isDirty()), false);
      }

      // Document brightness remains independent when an existing drawing loads.
      await page.evaluate((doc) => window.editor.load(JSON.stringify(doc), "rsk"), initial);
      assert.equal((await drawing(page)).canvas_theme, "light");
      assert.equal((await preference(page)).canvas_theme, "dark");
      await command(page, "New drawing");
      assert.equal((await drawing(page)).canvas_theme, "dark");
      assert.equal(await page.evaluate(() => window.editor.isDirty()), false);

      await command(page, "Canvas: Light");
      assert.equal((await drawing(page)).canvas_theme, "light");
      await page.reload();
      await ready(page);
      assert.equal((await drawing(page)).canvas_theme, "light");
      assert.equal((await preference(page)).canvas_theme, "light");
      assert.equal(await page.evaluate(() => window.editor.isDirty()), false);
      await page.screenshot({ path: `${output}/${name}-light-reload.png` });

      // A refused preference transaction keeps the old durable value. Retrying
      // the same explicit canvas choice saves it once storage works again.
      await page.evaluate(() => {
        window.qaAppearanceAborts = 0;
        const put = IDBObjectStore.prototype.put;
        IDBObjectStore.prototype.put = function (value, key) {
          const request = put.call(this, value, key);
          if (key === "appearance.json" && window.qaAppearanceAborts === 0) {
            window.qaAppearanceAborts++;
            this.transaction.abort();
          }
          return request;
        };
      });
      await command(page, "Canvas: Dark");
      await page.waitForFunction(() => window.qaAppearanceAborts === 1);
      await page.waitForTimeout(150);
      assert.equal((await drawing(page)).canvas_theme, "dark");
      assert.equal((await preference(page)).canvas_theme, "light");
      await command(page, "Canvas: Dark");
      await page.reload();
      await ready(page);
      assert.equal((await drawing(page)).canvas_theme, "dark");
      assert.equal((await preference(page)).canvas_theme, "dark");
      assert.deepEqual(errors, []);
      report.push({
        browser: name,
        version: browser.version(),
        checks: [
          "dark command → immediate reload → clean dark drawing",
          "Light, Dark and Match canvas interface choices survive reload independently",
          "existing light drawing retains its canvas without replacing dark default",
          "New uses remembered dark brightness",
          "light command → immediate reload → clean light drawing",
          "aborted IDB preference write retains old value; same-value retry survives reload",
        ],
        saved: await preference(page),
        errors,
      });
      await context.close();
    } finally {
      await browser.close();
    }
  }
  await writeFile(`${output}/results.json`, JSON.stringify(report, null, 2) + "\n");
  console.log(JSON.stringify(report, null, 2));
} finally {
  server.stop();
}
