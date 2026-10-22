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
await page.goto(url);
await page.waitForFunction(() => typeof window.editor?.load === 'function', null, { timeout: 180000 });
await page.waitForTimeout(2500);
// Hide the preview note so it does not cover the canvas.
await page.evaluate(() => document.getElementById('demo-notice')?.remove());
say('url', url, 'navigator.platform', await page.evaluate(() => navigator.platform));
const smi = () => page.evaluate(() => window.editor.save('smiles').catch(e => 'ERR:' + e.message));
const wait = (ms = 400) => page.waitForTimeout(ms);
const reset = async (s = 'c1ccccc1CCO') => { await page.evaluate(s => window.editor.load(s, 'smiles'), s); await wait(600); await page.mouse.click(1000, 700); await wait(200); };
const cdp = await ctx.newCDPSession(page);
// A real Mac Option chord: the browser reports the typed character (≈ for ⌥X).
const send = (type, key, code, vk, modifiers) => cdp.send('Input.dispatchKeyEvent', { type, key, code, windowsVirtualKeyCode: vk, modifiers });
const chord = async (key, code, vk) => {
  await send('rawKeyDown', 'Meta', 'MetaLeft', 91, 4); await send('rawKeyDown', 'Alt', 'AltLeft', 18, 5);
  await send('rawKeyDown', key, code, vk, 5); await send('keyUp', key, code, vk, 5);
  await send('keyUp', 'Alt', 'AltLeft', 18, 4); await send('keyUp', 'Meta', 'MetaLeft', 91, 0);
};

// 1. Cmd+A, then Delete, Cmd+Z, Cmd+Shift+Z.
await reset();
await page.keyboard.press('Meta+a'); await wait();
await page.screenshot({ path: out + '/01-cmd-a.png' });
await page.keyboard.press('Backspace'); await wait();
say('Cmd+A, Backspace ->', JSON.stringify(await smi()));
await page.keyboard.press('Meta+z'); await wait();
say('Cmd+Z ->', JSON.stringify(await smi()));
await page.screenshot({ path: out + '/02-cmd-z.png' });
await page.keyboard.press('Meta+Shift+z'); await wait();
say('Cmd+Shift+Z ->', JSON.stringify(await smi()));

// 2. Cmd+A, Cmd+C, then Cmd+V pastes a copy (two molecules).
await ctx.grantPermissions(['clipboard-read', 'clipboard-write'], { origin: new URL(url).origin });
await reset('CCO');
await page.evaluate(() => navigator.clipboard.writeText('SENTINEL'));
await page.keyboard.press('Meta+a'); await wait();
await page.keyboard.press('Meta+c'); await wait(800);
say('Cmd+C clipboard ->', JSON.stringify((await page.evaluate(() => navigator.clipboard.readText())).slice(0, 40)));
await page.mouse.click(1000, 700); await wait();
await page.keyboard.press('Meta+v'); await wait(1200);
say('Cmd+V ->', JSON.stringify(await smi()));

// 3. Cmd+O opens the file picker; Cmd+S saves (downloads).
await reset('CCO');
const chooser = page.waitForEvent('filechooser', { timeout: 3000 }).then(() => 'file chooser opened', () => 'no file chooser');
await page.keyboard.press('Meta+o');
say('Cmd+O ->', await chooser);
await page.getByRole('button', { name: 'Cancel', exact: true }).click({ timeout: 2000 }).catch(() => {}); await wait();
await page.mouse.click(1000, 700); await wait();
const download = page.waitForEvent('download', { timeout: 3000 }).then(d => 'download ' + d.suggestedFilename(), () => 'no download');
await page.keyboard.press('Meta+s');
say('Cmd+S ->', await download);

// 4. Shift+6 picks the six-membered ring.
await reset();
await page.keyboard.press('Shift+Digit6'); await wait();
await page.screenshot({ path: out + '/03-shift-6.png', clip: { x: 0, y: 60, width: 420, height: 620 } });

// 5. ⌘⌥X (crosshair) as a Mac sends it: key "≈".
await reset();
await chord('≈', 'KeyX', 88); await wait();
await page.mouse.move(640, 400); await wait();
await page.screenshot({ path: out + '/04-cmd-option-x.png' });

// 6. The command palette and Help, opened with ⌘ shortcuts, show ⌘ labels.
await reset();
await page.keyboard.press('Meta+Shift+Slash'); await wait(800);
await page.screenshot({ path: out + '/05-help.png' });
await page.keyboard.press('Escape'); await wait();
await page.keyboard.type(':'); await wait(600);
await page.keyboard.type('undo'); await wait(600);
await page.screenshot({ path: out + '/06-palette.png' });
await page.keyboard.press('Escape'); await wait();

// 7. Context menu on a selection shows ⌘ labels; Control-click opens it.
await reset('c1ccccc1');
await page.keyboard.press('Meta+a'); await wait();
const canvas = await page.evaluate(() => { const r = document.querySelector('canvas').getBoundingClientRect(); return { x: r.x + r.width / 2, y: r.y + r.height / 2 }; });
await page.keyboard.down('Control');
await page.mouse.click(canvas.x, canvas.y); await wait(800);
await page.keyboard.up('Control'); await wait(300);
await page.screenshot({ path: out + '/07-control-click.png' });
await page.keyboard.press('Escape'); await wait();

// 8. Shift released while the page had focus is not stuck when the editor returns.
await reset('CCO');
await page.keyboard.down('Shift'); await wait(200);
await page.evaluate(() => { const b = document.createElement('input'); b.id = 'outside'; document.body.append(b); b.focus(); });
await wait(200);
await page.keyboard.up('Shift'); await wait(200);
await page.mouse.click(1000, 700); await wait(200);
await page.keyboard.press('Digit6'); await wait(400);
await page.screenshot({ path: out + '/08-after-blur-6.png', clip: { x: 0, y: 60, width: 420, height: 620 } });

// 9. Undo tooltip label.
await reset('CCO');
await page.keyboard.press('Meta+a'); await page.keyboard.press('Backspace'); await wait();
await page.mouse.move(334, 27); await wait(1500);
await page.screenshot({ path: out + '/09-undo-tooltip.png', clip: { x: 200, y: 0, width: 400, height: 120 } });

// 10. The browser binding for New: Control+N on a Mac adds a tab.
await page.mouse.click(1000, 700); await wait(200);
await page.keyboard.press('Control+n'); await wait(800);
await page.screenshot({ path: out + '/10-control-n.png', clip: { x: 0, y: 0, width: 800, height: 60 } });

writeFileSync(out + '/log.txt', log.join('\n') + '\n');
await browser.close();
