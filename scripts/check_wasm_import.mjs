// Run the unprocessed Rust module in a host with no JavaScript clock support.
// Every imported function throws if used, exposing accidental browser calls.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const path = process.argv[2];
assert.ok(path, "Usage: node scripts/check_wasm_import.mjs MODULE.wasm");
const module = new WebAssembly.Module(readFileSync(path));
const entries = WebAssembly.Module.imports(module);
console.log(JSON.stringify({ node: process.version, imports: entries }));
const imports = {};
for (const entry of entries) {
  assert.equal(entry.kind, "function", `Unexpected host import: ${entry.name}`);
  (imports[entry.module] ??= {})[entry.name] = () => {
    throw new Error(`Unavailable JavaScript host import: ${entry.module}::${entry.name}`);
  };
}

let failed = false;
for (const [name, expected] of [
  ["parse_benzene", 6],
  ["import_benzene", 6],
  ["import_fused_rings", 10],
  ["import_mol", 6],
  ["import_cdxml", 2],
]) {
  const { exports } = new WebAssembly.Instance(module, imports);
  try {
    const actual = exports[name]();
    assert.equal(actual, expected);
    console.log(JSON.stringify({ name, expected, actual, ok: true }));
  } catch (error) {
    const panic = new Uint8Array(exports.panic_len());
    for (let i = 0; i < panic.length; i++) panic[i] = exports.panic_byte(i);
    console.error(JSON.stringify({ name, ok: false, panic: new TextDecoder().decode(panic) }));
    console.error(error.stack);
    failed = true;
  }
}
if (failed) process.exitCode = 1;
