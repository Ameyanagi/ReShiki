// usage: node title.mjs <url> <outdir> — document.title after drawing, then (if the in-app dialog exists) Don't Save.
import { chromium } from '/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/browser/node_modules/playwright-core/index.mjs';
import { mkdirSync, appendFileSync } from 'node:fs';
const [url, out] = process.argv.slice(2);
mkdirSync(out, { recursive: true });
const say = (s) => { console.log(s); appendFileSync(out + '/log.txt', s + '\n'); };
const browser = await chromium.launch({ executablePath: '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome', headless: true, args: ['--use-angle=metal', '--enable-unsafe-webgpu', '--enable-gpu', '--ignore-gpu-blocklist'] });
const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 }, deviceScaleFactor: 1 });
const page = await ctx.newPage();
let dialogs = 0;
page.on('dialog', async d => { dialogs++; say('DIALOG ' + d.type() + ' ' + JSON.stringify(d.message())); await d.dismiss(); });
const wait = (ms = 500) => page.waitForTimeout(ms);
await page.goto(url);
await page.waitForFunction(() => typeof window.editor?.load === 'function', null, { timeout: 180000 });
await wait(2500);
await page.evaluate(() => document.getElementById('demo-notice')?.remove());
await page.evaluate(() => window.editor.load('CCO', 'smiles'));
await wait(600);
await page.mouse.click(1000, 700);
await page.keyboard.press('Digit6');
await wait(300);
await page.mouse.click(500, 500);
await wait(800);
const title = await page.evaluate(() => document.title);
say('document.title after drawing: ' + JSON.stringify(title));
await page.evaluate((title) => {
  const b = document.createElement('div');
  b.id = 'ev-banner';
  b.textContent = `Browser tab title (document.title) after drawing: "${title}"`;
  Object.assign(b.style, { position: 'fixed', left: '120px', right: '320px', bottom: '60px', padding: '10px 14px', background: '#fff7d6', border: '2px solid #c9a227', font: '15px/1.4 -apple-system, sans-serif', color: '#222', zIndex: 10, pointerEvents: 'none' });
  document.body.append(b);
}, title);
await page.screenshot({ path: out + '/06-title.png' });
await page.evaluate(() => document.getElementById('ev-banner')?.remove());
// Close the tab; with the in-app dialog, choose Don't Save.
await page.mouse.move(470, 27); await wait(300);
await page.mouse.click(486, 27); await wait(1000);
if (!dialogs) {
  await page.mouse.click(489, 450); await wait(1000);
  say("Don't Save -> " + JSON.stringify(await page.evaluate(() => ({ title: document.title, isDirty: window.editor.isDirty() }))));
  await page.screenshot({ path: out + '/07-dont-save.png' });
}
await browser.close();
