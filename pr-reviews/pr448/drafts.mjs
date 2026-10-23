// node drafts.mjs <url> <out>: an unsaved drawing, then a reload, a second
// tab while the first is open, and a new tab after the first closes.
import { start } from './lib.mjs';
const [url, out] = process.argv.slice(2);
const s = await start(out);
const { page, wait, shot, say, ctx } = s;
const ready = async p => {
  await p.waitForFunction(() => typeof window.editor?.load === 'function', null, { timeout: 180000 });
  await p.waitForTimeout(2500);
  await p.evaluate(() => document.getElementById('demo-notice')?.remove());
};
await s.open(url);

// 1. Draw a ring and a chain: the drawing has unsaved changes.
await page.mouse.click(900, 300);
await s.palette('Benzene tool');
await page.mouse.click(500, 420);
await wait(500);
await page.keyboard.press('Escape');
await wait(300);
await shot('01-unsaved-drawing');
say('drawing:', await s.smi());
await wait(2500); // the draft catches up within about a second
say('stored:', await s.stored());

// 2. Reload.
await s.reload();
await shot('02-after-reload');
say('after reload, stored:', await s.stored());

// 3. Restore.
await page.mouse.click(Number(process.env.RESTORE_X ?? 150), Number(process.env.RESTORE_Y ?? 703));
await wait(1500);
await shot('03-restored');
say('restored drawing:', await s.smi());
await wait(2500);
say('stored:', await s.stored());

// 4. A second tab while the first is open offers nothing.
const other = await ctx.newPage();
await other.goto(url);
await ready(other);
await other.screenshot({ path: `${out}/04-second-tab-while-first-open.png` });
await other.close();

// 5. Close the first tab; the next one offers its draft.
await page.close();
const next = await ctx.newPage();
await next.goto(url);
await ready(next);
await next.screenshot({ path: `${out}/05-new-tab-after-first-closed.png` });
await s.finish();
