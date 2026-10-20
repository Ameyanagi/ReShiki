import fs from 'node:fs';
const dir = '/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev-wasm';
const tabs = await (await fetch('http://localhost:9333/json/list')).json();
const target = tabs.find(t => t.type === 'page' && t.url === 'http://localhost:63925/examples/');
if (!target) throw new Error('Isolated demo target not found');
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((resolve,reject)=>{ws.onopen=resolve;ws.onerror=reject});
let id=0; const pending=new Map();
ws.onmessage=e=>{const m=JSON.parse(e.data); if(m.id){const p=pending.get(m.id); if(p){pending.delete(m.id);m.error?p.reject(m.error):p.resolve(m.result)}}else if(['Runtime.consoleAPICalled','Runtime.exceptionThrown','Log.entryAdded','Inspector.targetCrashed'].includes(m.method)){fs.appendFileSync(dir+'/console-events.jsonl',JSON.stringify({capturedAt:new Date().toISOString(),...m})+'\n')}};
function call(method,params={}){return new Promise((resolve,reject)=>{const n=++id;pending.set(n,{resolve,reject});ws.send(JSON.stringify({id:n,method,params}));setTimeout(()=>{if(pending.has(n)){pending.delete(n);reject(new Error('Timeout: '+method))}},20000).unref()})}
const [mode,arg]=process.argv.slice(2);
if(mode==='monitor'){
 await call('Runtime.enable'); await call('Log.enable'); console.log('Monitoring isolated demo console');
} else {
 try {
  if(mode==='eval'){const r=await call('Runtime.evaluate',{expression:arg,awaitPromise:true,returnByValue:true,replMode:true}); fs.appendFileSync(dir+'/api-results.jsonl',JSON.stringify({time:new Date().toISOString(),expression:arg,...r})+'\n');console.log(JSON.stringify(r,null,2));}
  else if(mode==='screenshot'){const r=await call('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});fs.writeFileSync(dir+'/'+arg,Buffer.from(r.data,'base64'));console.log(arg);}
  else if(mode==='labels'){const r=await call('Page.captureScreenshot',{format:'png',clip:{x:275,y:285,width:535,height:440,scale:1},captureBeyondViewport:false});fs.writeFileSync(dir+'/06-labels-closeup.png',Buffer.from(r.data,'base64'));console.log('06-labels-closeup.png');}
  else if(mode==='front'){console.log(await call('Page.bringToFront'));}
  else if(mode==='export') {const r=await call('Runtime.evaluate',{expression:arg==='svg'?'await window.editor.exportImage("svg")':'Array.from(await window.editor.exportImage("png"))',awaitPromise:true,returnByValue:true,replMode:true});if(r.exceptionDetails)throw new Error(JSON.stringify(r.exceptionDetails)); const value=r.result.value;fs.writeFileSync(dir+'/export.'+arg,arg==='svg'?value:Buffer.from(value));console.log(JSON.stringify({type:arg,length:value.length,head:arg==='svg'?value.slice(0,200):value.slice(0,16)}));}
  else throw new Error('Unknown mode');
 } finally {ws.close()}
}
