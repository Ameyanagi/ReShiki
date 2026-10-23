// usage: node evidence.mjs <url> <outdir>
import { chromium } from '/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/browser/node_modules/playwright-core/index.mjs';
import { mkdirSync, writeFileSync } from 'node:fs';
const [url, out] = process.argv.slice(2);
mkdirSync(out, { recursive: true });
const log = [];
const say = (...a) => { const s = a.join(' '); log.push(s); console.log(s); };
const browser = await chromium.launch({ executablePath: '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome', headless: true, args: ['--use-angle=metal', '--enable-unsafe-webgpu', '--enable-gpu', '--ignore-gpu-blocklist'] });
const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 }, deviceScaleFactor: 1 });
const page = await ctx.newPage();
page.on('console', m => { if (m.type() === 'error') say('console error:', m.text()); });
const dialogs = [];
page.on('dialog', async d => {
  dialogs.push({ type: d.type(), message: d.message() });
  say('DIALOG', d.type(), JSON.stringify(d.message()));
  // Stay on the page / Cancel.
  await d.dismiss();
});
const wait = (ms = 500) => page.waitForTimeout(ms);
const ready = async () => {
  await page.waitForFunction(() => typeof window.editor?.load === 'function', null, { timeout: 180000 });
  await wait(2500);
  await page.evaluate(() => document.getElementById('demo-notice')?.remove());
};
const state = () => page.evaluate(() => ({
  title: document.title,
  canvasAlt: document.querySelector('canvas')?.getAttribute('alt'),
  isDirty: typeof window.editor?.isDirty === 'function' ? window.editor.isDirty() : 'no isDirty()',
}));
const banner = (text) => page.evaluate((text) => {
  document.getElementById('ev-banner')?.remove();
  const b = document.createElement('div');
  b.id = 'ev-banner';
  b.textContent = text;
  Object.assign(b.style, { position: 'fixed', left: '120px', right: '320px', bottom: '60px', padding: '10px 14px', background: '#fff7d6', border: '2px solid #c9a227', font: '13px/1.4 -apple-system, sans-serif', color: '#222', zIndex: 10, whiteSpace: 'pre-wrap', pointerEvents: 'none' });
  document.body.append(b);
}, text);
const draw = async () => {
  await page.evaluate(() => window.editor.load('CCO', 'smiles'));
  await wait(600);
  await page.mouse.click(1000, 700);
  await page.keyboard.press('Digit6');
  await wait(300);
  await page.mouse.click(500, 500);
  await wait(800);
};

await page.goto(url);
await ready();
say('url', url);
say('loaded:', JSON.stringify(await state()));
if (await page.evaluate(() => typeof window.editor.onDirtyChange === 'function')) {
  await page.evaluate(() => { window.dirtyEvents = []; window.titleEvents = []; window.editor.onDirtyChange(d => window.dirtyEvents.push(d)); window.editor.onTitleChange(t => window.titleEvents.push(t)); });
}

// 1. Draw: the tab shows Untitled •.
await draw();
say('after drawing:', JSON.stringify(await state()));
await page.screenshot({ path: out + '/01-drawn.png' });

// 2. Reload.
const before = dialogs.length;
let reloaded = 'reloaded';
await page.reload({ timeout: 8000 }).catch(e => { reloaded = 'reload did not happen: ' + e.message.split('\n')[0]; });
await wait(1500);
const shown = dialogs.slice(before);
say('reload ->', reloaded, '| dialogs:', JSON.stringify(shown));
if (shown.length) {
  await banner(`Reload pressed → the browser fired a "${shown[0].type}" dialog ("Leave site? Changes you made may not be saved."). Playwright answered "Cancel", so the page stayed and the drawing is still here.\n${JSON.stringify(await state())}`);
} else {
  await ready();
  await banner(`Reload pressed → no dialog; the page reloaded and the drawing is gone.\n${JSON.stringify(await state())}`);
}
await page.screenshot({ path: out + '/02-after-reload.png' });
await page.evaluate(() => document.getElementById('ev-banner')?.remove());

// 3. Close the dirty in-app tab with its ×.
if (!shown.length) await draw();
say('before close:', JSON.stringify(await state()));
const d0 = dialogs.length;
await page.mouse.move(470, 27);
await wait(300);
await page.mouse.click(486, 27);
await wait(1200);
const closeDialogs = dialogs.slice(d0);
say('close tab ->', JSON.stringify(closeDialogs));
if (closeDialogs.length) {
  await banner(`Tab × pressed → ${closeDialogs.length} browser dialogs, both answered Cancel:\n` + closeDialogs.map((d, i) => `${i + 1}. ${d.type}(): ${d.message}`).join('\n') + `\nresult: ${JSON.stringify(await state())}`);
}
await page.screenshot({ path: out + '/03-close-tab.png' });
await page.evaluate(() => document.getElementById('ev-banner')?.remove());
if (!closeDialogs.length) {
  // The in-app dialog: Escape cancels and keeps the tab.
  await page.keyboard.press('Escape');
  await wait(600);
  say('Escape ->', JSON.stringify(await state()));
  await page.screenshot({ path: out + '/04-cancelled.png' });
  // Don't Save closes the tab.
  await page.mouse.move(470, 27);
  await wait(300);
  await page.mouse.click(486, 27);
  await wait(800);
  await page.getByText ? null : null;
  await page.screenshot({ path: out + '/05-asked-again.png' });
  writeFileSync(out + '/dialog-probe.txt', 'see 05');
}
say('dirty events:', JSON.stringify(await page.evaluate(() => window.dirtyEvents ?? null)));
say('title events:', JSON.stringify(await page.evaluate(() => window.titleEvents ?? null)));
writeFileSync(out + '/log.txt', log.join('\n') + '\n');
await browser.close();
