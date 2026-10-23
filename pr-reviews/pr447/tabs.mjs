// node tabs.mjs <url> <out>: two tabs; a setting saved in one reaches the
// other without a reload, and saving in the second keeps the first's change.
import { start } from './lib.mjs';
const [url, out] = process.argv.slice(2);
const s = await start(out);
const { page, wait, say, ctx } = s;
const ready = async p => {
  await p.waitForFunction(() => typeof window.editor?.load === 'function', null, { timeout: 180000 });
  await p.waitForTimeout(2500);
  await p.evaluate(() => document.getElementById('demo-notice')?.remove());
};
const palette = async (p, command) => {
  await p.mouse.click(900, 300);
  await p.waitForTimeout(200);
  await p.keyboard.type(':');
  await p.waitForTimeout(300);
  await p.keyboard.type(command, { delay: 15 });
  await p.waitForTimeout(400);
  await p.keyboard.press('Enter');
  await p.waitForTimeout(1200);
};
const appearance = p => p.evaluate(() => new Promise(resolve => {
  const r = indexedDB.open('reshiki');
  r.onsuccess = () => {
    const q = r.result.transaction('files').objectStore('files').get('appearance.json');
    q.onsuccess = () => { r.result.close(); resolve(q.result ? new TextDecoder().decode(q.result) : null); };
  };
}));
await s.open(url);
const other = await ctx.newPage();
await other.goto(url);
await ready(other);

// Tab A turns on dark mode; tab B, open all along, follows.
await page.bringToFront();
await palette(page, 'Appearance: Dark');
await other.bringToFront();
await other.waitForTimeout(1500);
await other.screenshot({ path: `${out}/01-tab-b-follows-tab-a.png` });
// Tab B turns off smart guides; dark mode stays.
await palette(other, 'Toggle smart guides');
await other.screenshot({ path: `${out}/02-tab-b-changes-another-setting.png` });
say('stored appearance:', await appearance(other));
await s.finish();
