// node fonts.mjs <url> <out>: Japanese text needs the fallback font; reload and
// see whether it is downloaded again.
import { start } from './lib.mjs';
const [url, out] = process.argv.slice(2);
const s = await start(out);
s.page.on('request', r => { if (r.url().includes('jsdelivr')) s.say('font download:', r.url()); });
const cdxml = `<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE CDXML SYSTEM "http://www.cambridgesoft.com/xml/cdxml.dtd"><CDXML><page id="1"><t id="2" p="100 100"><s font="3" size="20">反応 ベンゼン</s></t></page></CDXML>`;
await s.open(url);
s.say('-- first visit');
await s.page.evaluate(x => window.editor.load(x, 'cdxml'), cdxml);
await s.wait(6000);
await s.shot('f1-japanese');
s.say('stored:', await s.stored());
s.say('-- reload');
await s.reload();
await s.page.evaluate(x => window.editor.load(x, 'cdxml'), cdxml);
await s.wait(6000);
await s.shot('f2-japanese-after-reload');
s.say('stored:', await s.stored());
await s.finish();
