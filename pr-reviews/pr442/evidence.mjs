// usage: node evidence.mjs <url> <outdir> [nofsa]
import { chromium } from '/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/browser/node_modules/playwright-core/index.mjs';
import { mkdirSync, writeFileSync, readFileSync, existsSync } from 'node:fs';
const [url, out, mode] = process.argv.slice(2);
const files = '/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev428/files';
mkdirSync(out, { recursive: true }); mkdirSync(files, { recursive: true });
const log = [];
const say = (...a) => { const s = a.join(' '); log.push(s); console.log(s); };
const browser = await chromium.launch({ channel: 'chrome', headless: true, args: ['--use-angle=metal', '--enable-unsafe-webgpu', '--enable-gpu', '--ignore-gpu-blocklist'] });
const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 }, acceptDownloads: true });
const page = await ctx.newPage();
if (mode === 'nofsa') await page.addInitScript(() => { delete window.showOpenFilePicker; });
// Headless Chrome's File System Access picker cannot be driven, so this mode
// stands in for it: it records the options and hands back file handles.
if (mode === 'stubfsa') {
  await page.exposeFunction('pickedFiles', () => next);
  await page.exposeFunction('readPicked', p => Array.from(readFileSync(p)));
  await page.addInitScript(() => {
    window.showOpenFilePicker = async options => {
      console.log('showOpenFilePicker called with ' + JSON.stringify(options));
      const paths = [].concat(await window.pickedFiles());
      return Promise.all(paths.map(async p => {
        const file = new File([new Uint8Array(await window.readPicked(p))], p.split('/').pop());
        return { kind: 'file', name: file.name, getFile: async () => file };
      }));
    };
  });
  page.on('console', m => { if (m.text().startsWith('showOpenFilePicker')) say(m.text()); });
}
page.on('console', m => { if (m.type() === 'error') say('console error:', m.text().slice(0, 300)); });
page.on('dialog', async d => { say('dialog:', d.type(), JSON.stringify(d.message())); await d.dismiss(); });
let next = null;
page.on('filechooser', async fc => { say('file chooser opened, multiple =', fc.isMultiple(), '->', next); if (next) await fc.setFiles(next); });
await page.goto(url);
await page.waitForFunction(() => typeof window.editor?.load === 'function', null, { timeout: 180000 });
await page.waitForTimeout(2500);
await page.evaluate(() => document.getElementById('demo-notice')?.remove());
say('url', url, 'showOpenFilePicker:', await page.evaluate(() => typeof window.showOpenFilePicker));
const wait = (ms = 600) => page.waitForTimeout(ms);
const smi = () => page.evaluate(() => window.editor.save('smiles').catch(e => 'ERR:' + e.message));
const shot = name => page.screenshot({ path: `${out}/${name}.png` });
const overlay = () => page.evaluate(() => document.getElementById('rfd-overlay')?.innerText ?? null);

// Test files, written with the page's own converter.
if (!existsSync(files + '/caffeine.mol')) {
  for (const [name, s, f] of [['caffeine.mol', 'Cn1c(=O)c2c(ncn2C)n(C)c1=O', 'mol'], ['aspirin.mol', 'CC(=O)Oc1ccccc1C(=O)O', 'mol'], ['naphthalene.rsk', 'c1ccc2ccccc2c1', 'rsk']]) {
    await page.evaluate(s => window.editor.load(s, 'smiles'), s); await wait(400);
    const data = await page.evaluate(f => window.editor.save(f).then(b => typeof b === 'string' ? b : Array.from(b)), f);
    writeFileSync(`${files}/${name}`, typeof data === 'string' ? data : Buffer.from(data));
  }
  await page.reload();
  await page.waitForFunction(() => typeof window.editor?.load === 'function', null, { timeout: 180000 });
  await page.waitForTimeout(2500);
  await page.evaluate(() => document.getElementById('demo-notice')?.remove());
}
const canvas = await page.evaluate(() => { const r = document.querySelector('canvas').getBoundingClientRect(); return { x: r.x + r.width / 2, y: r.y + r.height / 2 }; });

// 1. Drop a MOL file on the drawing.
const transfer = await page.evaluateHandle(([name, bytes]) => {
  const t = new DataTransfer();
  t.items.add(new File([new Uint8Array(bytes)], name, { type: '' }));
  return t;
}, ['caffeine.mol', Array.from(readFileSync(files + '/caffeine.mol'))]);
await page.mouse.move(canvas.x, canvas.y);
const target = page.locator('canvas');
await target.dispatchEvent('dragenter', { dataTransfer: transfer, clientX: canvas.x, clientY: canvas.y });
await target.dispatchEvent('dragover', { dataTransfer: transfer, clientX: canvas.x, clientY: canvas.y });
await wait(500);
await shot('01-drag-over');
const dropped = await page.evaluate(async t => {
  const e = new DragEvent('drop', { dataTransfer: t, bubbles: true, cancelable: true });
  document.querySelector('canvas').dispatchEvent(e);
  return e.defaultPrevented;
}, transfer);
await wait(2000);
say('drop: default prevented =', dropped, '; drawing ->', JSON.stringify(await smi()));
await shot('02-dropped-mol');

// 2. Open a drawing (Cmd+O).
next = files + '/naphthalene.rsk';
await page.mouse.click(199, 27); await page.mouse.move(640, 600); await wait(1500);
say('Open: rfd overlay after the chooser ->', JSON.stringify(await overlay()));
await shot('03-open-after-chooser');
await wait(1500);
say('Open: drawing in front ->', JSON.stringify(await smi()));
await shot('04-open-result');
await page.getByRole('button', { name: 'Cancel', exact: true }).click({ timeout: 1000 }).catch(() => {});
await wait();

// 3. Import > Choose file… (Insert from file), through the command palette.
next = [files + '/aspirin.mol', files + '/caffeine.mol'];
await page.mouse.click(160, 27); await wait();
await page.mouse.click(1143, 126); await wait();
await page.mouse.click(1035, 377); await page.mouse.move(640, 600); await wait(3000);
say('Insert: rfd overlay ->', JSON.stringify(await overlay()));
say('Insert: drawing ->', JSON.stringify(await smi()));
await shot('05-insert-from-file');

writeFileSync(out + '/log.txt', log.join('\n') + '\n');
await browser.close();
