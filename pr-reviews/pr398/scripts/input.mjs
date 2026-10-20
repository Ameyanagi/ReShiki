// Drives the isolated Chrome via CDP with trusted Input.* events (mouse/keyboard go through Chrome's input pipeline, not DOM dispatch).
import fs from 'node:fs';
const dir = process.argv[2], port = process.argv[3], page = process.argv[4];
const sleep = ms => new Promise(r => setTimeout(r, ms));
let target;
for (let i = 0; i < 50 && !target; i++) { try { target = (await (await fetch(`http://localhost:${port}/json/list`)).json()).find(t => t.type === 'page' && t.url.startsWith(page)); } catch {} if (!target) await sleep(300); }
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
let id = 0; const pending = new Map(); const logs = [];
ws.onmessage = e => { const m = JSON.parse(e.data); if (m.id) { const p = pending.get(m.id); pending.delete(m.id); m.error ? p.reject(new Error(JSON.stringify(m.error))) : p.resolve(m.result); } else if (m.method === 'Runtime.consoleAPICalled') logs.push(`[${m.params.type}] ` + m.params.args.map(a => a.value ?? a.description).join(' ')); else if (m.method === 'Runtime.exceptionThrown') logs.push('[exception] ' + JSON.stringify(m.params.exceptionDetails).slice(0, 500)); else if (m.method === 'Log.entryAdded') logs.push(`[log ${m.params.entry.level}] ${m.params.entry.text} ${m.params.entry.url ?? ''}`); };
const call = (method, params = {}) => new Promise((resolve, reject) => { const n = ++id; pending.set(n, { resolve, reject }); ws.send(JSON.stringify({ id: n, method, params })); });
const ev = async expr => { const r = await call('Runtime.evaluate', { expression: expr, awaitPromise: true, returnByValue: true, replMode: true }); return r.exceptionDetails ? 'EXCEPTION: ' + (r.exceptionDetails.exception?.description ?? r.exceptionDetails.text) : r.result.value; };
const shot = async name => { const r = await call('Page.captureScreenshot', { format: 'png' }); fs.writeFileSync(`${dir}/${name}`, Buffer.from(r.data, 'base64')); };
const mouse = (type, x, y, extra = {}) => call('Input.dispatchMouseEvent', { type, x, y, button: 'left', buttons: type === 'mouseReleased' ? 0 : 1, clickCount: 1, ...extra });
const move = (x, y, held) => call('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y, button: held ? 'left' : 'none', buttons: held ? 1 : 0 });
const click = async (x, y) => { await move(x, y); await sleep(80); await mouse('mousePressed', x, y); await sleep(60); await mouse('mouseReleased', x, y); await sleep(300); };
const drag = async (x1, y1, x2, y2) => { await move(x1, y1); await sleep(80); await mouse('mousePressed', x1, y1); for (let i = 1; i <= 10; i++) { await move(x1 + (x2 - x1) * i / 10, y1 + (y2 - y1) * i / 10, true); await sleep(30); } await mouse('mouseReleased', x2, y2); await sleep(300); };
const key = async (mods, label) => { const base = { modifiers: mods, key: 'z', code: 'KeyZ', windowsVirtualKeyCode: 90, nativeVirtualKeyCode: 90 }; const modKey = mods === 2 ? { key: 'Control', code: 'ControlLeft', windowsVirtualKeyCode: 17 } : { key: 'Meta', code: 'MetaLeft', windowsVirtualKeyCode: 91 }; await call('Input.dispatchKeyEvent', { type: 'rawKeyDown', ...modKey, modifiers: mods }); await call('Input.dispatchKeyEvent', { type: 'rawKeyDown', ...base }); await call('Input.dispatchKeyEvent', { type: 'keyUp', ...base }); await call('Input.dispatchKeyEvent', { type: 'keyUp', ...modKey, modifiers: 0 }); await sleep(400); };
const out = {};
const state = async label => { out[label] = { smiles: await ev('await window.editor.save("smiles")'), counter: await ev('document.getElementById("changes").textContent') }; console.log(label, JSON.stringify(out[label])); };
await call('Runtime.enable'); await call('Log.enable'); await call('Page.enable'); await call('Page.bringToFront');
for (let i = 0; i < 120 && (await ev('!!window.editor')) !== true; i++) await sleep(500);
await sleep(1500);
out.backend = await ev('(async()=>{const a=navigator.gpu&&await navigator.gpu.requestAdapter();return {gpu:!!a, ctx: !!document.querySelector("#editor canvas")?.getContext("webgpu"), viewport:[innerWidth,innerHeight,devicePixelRatio]}})()');
console.log('backend', JSON.stringify(out.backend));
await ev('globalThis.__n=0; globalThis.__off=window.editor.onChange(()=>globalThis.__n++)');
await state('start'); await shot('12-input-start.png');
// bond tool (single bond icon), then drag on canvas
await click(66, 277); await drag(450, 450, 530, 450); await state('afterBond'); await shot('13-input-bond-dragged.png');
await click(66, 360); await click(750, 600); await state('afterRing'); await shot('14-input-ring-placed.png');
await move(1000, 800); await key(2); await state('afterCtrlZ'); await shot('15-input-after-ctrl-z.png');
await key(4); await state('afterCmdZ'); await shot('16-input-after-cmd-z.png');
out.secondListenerCount = await ev('globalThis.__n'); console.log('second listener n =', out.secondListenerCount);
// label: select tool, hover the bond's end atom and type "O" (hotkey label) - then check
await click(37, 277); await move(530, 450); await sleep(200);
await call('Input.dispatchKeyEvent', { type: 'keyDown', key: 'o', code: 'KeyO', text: 'o', windowsVirtualKeyCode: 79 }); await call('Input.dispatchKeyEvent', { type: 'keyUp', key: 'o', code: 'KeyO', windowsVirtualKeyCode: 79 }); await sleep(400);
await state('afterHoverO'); await shot('17-input-hover-key-o.png');
// real window resize (browser window, not container)
const { windowId } = await call('Browser.getWindowForTarget');
await call('Browser.setWindowBounds', { windowId, bounds: { width: 900, height: 650 } }); await sleep(1500);
out.small = await ev('[innerWidth, innerHeight, document.querySelector("#editor canvas").width, document.querySelector("#editor canvas").height]'); await shot('18-window-900x650.png');
await call('Browser.setWindowBounds', { windowId, bounds: { width: 1600, height: 1050 } }); await sleep(1500);
out.large = await ev('[innerWidth, innerHeight, document.querySelector("#editor canvas").width, document.querySelector("#editor canvas").height]'); await shot('19-window-1600x1050.png');
console.log('resize', JSON.stringify([out.small, out.large]));
out.logs = logs; fs.writeFileSync(`${dir}/input-results.json`, JSON.stringify(out, null, 2)); console.log('logs', logs);
ws.close();
