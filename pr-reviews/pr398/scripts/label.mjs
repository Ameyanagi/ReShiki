import fs from 'node:fs';
const [dir, port, page, phase, xs, ys] = process.argv.slice(2);
const sleep = ms => new Promise(r => setTimeout(r, ms));
const target = (await (await fetch(`http://localhost:${port}/json/list`)).json()).find(t => t.type === 'page' && t.url.startsWith(page));
const ws = new WebSocket(target.webSocketDebuggerUrl); await new Promise(r => ws.onopen = r);
let id = 0; const pending = new Map(); ws.onmessage = e => { const m = JSON.parse(e.data); if (m.id) { pending.get(m.id)(m.result ?? m.error); pending.delete(m.id); } };
const call = (method, params = {}) => new Promise(r => { const n = ++id; pending.set(n, r); ws.send(JSON.stringify({ id: n, method, params })); });
const ev = async e => (await call('Runtime.evaluate', { expression: e, awaitPromise: true, returnByValue: true, replMode: true })).result?.value;
const shot = async name => fs.writeFileSync(`${dir}/${name}`, Buffer.from((await call('Page.captureScreenshot', { format: 'png' })).data, 'base64'));
if (phase === 'restore') { const { windowId } = await call('Browser.getWindowForTarget'); await call('Browser.setWindowBounds', { windowId, bounds: { width: 1400, height: 1000 } }); await sleep(1500); await shot('tmp-restore.png'); }
if (phase === 'click') { const x = +xs, y = +ys; await call('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y }); await sleep(100); await call('Input.dispatchMouseEvent', { type: 'mousePressed', x, y, button: 'left', buttons: 1, clickCount: 1 }); await sleep(60); await call('Input.dispatchMouseEvent', { type: 'mouseReleased', x, y, button: 'left', buttons: 0, clickCount: 1 }); await sleep(500); await call('Input.dispatchMouseEvent', { type: 'mouseMoved', x: 1000, y: 800 }); await sleep(300); await shot('20-input-atom-label-o.png'); console.log(JSON.stringify({ smiles: await ev('await window.editor.save("smiles")'), counter: await ev('document.getElementById("changes").textContent') })); }
if (phase === 'crop') { fs.writeFileSync(`${dir}/21-label-closeup.png`, Buffer.from((await call('Page.captureScreenshot', { format: 'png', clip: { x: +xs - 80, y: +ys - 60, width: 200, height: 120, scale: 3 } })).data, 'base64')); }
ws.close();
