// Runs the prototype's own initialView (copied verbatim) on its ibuprofen and caffeine data.
const fs = require("fs");
const html = fs.readFileSync(process.argv[2], "utf8");
const MOLS = JSON.parse(html.match(/const MOLS = (\{.*\});/)[1]);
const body = html.slice(html.indexOf("const mul ="), html.indexOf("/* ---------- renderer"));
eval(body.replace(/const (\w+) =/g, "var $1 =").replace("function initialView", "var initialView = function"));
for (const k of ["caffeine", "ibuprofen"]) console.log(k, JSON.stringify(initialView(MOLS[k]).map(r => r.map(v => +v.toFixed(3)))));
