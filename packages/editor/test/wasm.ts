// Loads the built wasm once for every test file that uses it. `bun test` runs
// the files in one process, where they share the module, so a load that must
// fail can only be checked before the first load that succeeds.
import { expect } from "bun:test";
import { readFileSync } from "node:fs";
import { init } from "../src/index.js";

// A failed load is not cached: init succeeds after it.
await expect(init(new Uint8Array([0]))).rejects.toThrow();
await init(readFileSync(new URL("../dist/wasm/reshiki_web_bg.wasm", import.meta.url)));
