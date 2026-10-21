import { chromium } from '../browser/node_modules/playwright-core/index.mjs';
import fs from 'node:fs/promises';
const dir = new URL('.', import.meta.url).pathname;
const source = await fs.readFile(dir + '../ev/kagobutsu.rsk', 'utf8');
const browser = await chromium.launch({
  executablePath: '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  headless: true,
  args: ['--use-angle=metal', '--enable-unsafe-webgpu', '--enable-gpu', '--ignore-gpu-blocklist'],
});
const origin = 'http://localhost:8093';
const context = await browser.newContext({ viewport: { width: 1400, height: 900 } });
// Grant Local Font Access the way a user clicking "Allow" would.
const cdn = [];
// Block the CDN: if the system path failed, the glyphs would stay boxes.
await context.route('https://cdn.jsdelivr.net/**', route => { cdn.push(route.request().url()); route.abort(); });
const page = await context.newPage();
const session = await context.newCDPSession(page);
const { targetInfo } = await session.send('Target.getTargetInfo');
const cdp = await browser.newBrowserCDPSession();
await cdp.send('Browser.grantPermissions', { origin, permissions: ['localFonts'], browserContextId: targetInfo.browserContextId });
const logs = [];
page.on('console', m => logs.push(`${m.type()}: ${m.text()}`));
await page.goto(origin + '/examples/');
await page.waitForFunction(() => Boolean(window.editor), undefined, { timeout: 120000 });
const permission = await page.evaluate(async () => (await navigator.permissions.query({ name: 'local-fonts' })).state);
const families = await page.evaluate(async () => {
  const fonts = await window.queryLocalFonts();
  return [...new Set(fonts.map(f => f.family))].filter(f => /Hiragino|Yu Gothic|Meiryo|Noto Sans (CJK )?JP/.test(f));
});
await page.evaluate(async data => window.editor.load(data, 'rsk'), source);
await new Promise(r => setTimeout(r, 4000));
await page.screenshot({ path: dir + 'system-font.png' });
console.log(JSON.stringify({ permission, families, cdnRequests: cdn, logs }, null, 2));
await browser.close();
