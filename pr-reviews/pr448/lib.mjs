// Shared helpers for driving the browser editor build in Chrome.
import { chromium } from '/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/browser/node_modules/playwright-core/index.mjs';
import { mkdirSync, writeFileSync } from 'node:fs';

export async function start(out, { scale = 2, width = 1280, height = 800 } = {}) {
  mkdirSync(out, { recursive: true });
  const log = [];
  const say = (...a) => { const s = a.join(' '); log.push(s); console.log(s); };
  const browser = await chromium.launch({
    channel: 'chrome',
    headless: true,
    args: [
      `--force-device-scale-factor=${scale}`,
      `--window-size=${width},${height}`,
      '--use-angle=metal', '--enable-unsafe-webgpu', '--enable-gpu', '--ignore-gpu-blocklist',
    ],
  });
  const ctx = await browser.newContext({ viewport: null, acceptDownloads: true });
  const page = await ctx.newPage();
  page.on('console', m => { if (m.type() === 'error') say('console error:', m.text().slice(0, 300)); });
  page.on('dialog', async d => { say('dialog:', d.type(), JSON.stringify(d.message())); await d.dismiss(); });
  const wait = (ms = 600) => page.waitForTimeout(ms);
  const ready = async () => {
    await page.waitForFunction(() => typeof window.editor?.load === 'function', null, { timeout: 180000 });
    await wait(2500);
    await page.evaluate(() => document.getElementById('demo-notice')?.remove());
  };
  const open = async url => { await page.goto(url); await ready(); };
  const reload = async () => { await page.reload(); await ready(); };
  const shot = name => page.screenshot({ path: `${out}/${name}.png` });
  const smi = () => page.evaluate(() => window.editor.save('smiles').catch(e => 'ERR:' + e.message));
  // Run a command through the command palette (":" on the canvas).
  const palette = async (command, { pick = true, focus = false } = {}) => {
    if (focus) { await page.mouse.click(640, 520); await wait(200); }
    await page.keyboard.type(':');
    await wait(300);
    await page.keyboard.type(command, { delay: 15 });
    await wait(400);
    if (pick) { await page.keyboard.press('Enter'); await wait(800); }
  };
  // The keys in the editor's IndexedDB store, or why there are none.
  const stored = () => page.evaluate(() => new Promise(resolve => {
    const r = indexedDB.open('reshiki');
    r.onupgradeneeded = () => { r.transaction.abort(); resolve('no reshiki database'); };
    r.onerror = () => resolve('no reshiki database');
    r.onsuccess = () => {
      const db = r.result;
      if (!db.objectStoreNames.contains('files')) { db.close(); return resolve('no files store'); }
      const q = db.transaction('files').objectStore('files').getAllKeys();
      q.onsuccess = () => { db.close(); resolve(q.result.join(', ') || '(empty)'); };
    };
  }));
  const finish = async () => { writeFileSync(out + '/log.txt', log.join('\n') + '\n'); await browser.close(); };
  return { browser, ctx, page, say, wait, open, reload, shot, smi, palette, stored, finish };
}
