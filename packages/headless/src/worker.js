import load, * as wasm from "../dist/wasm/reshiki_web.js";
import { createWorkerHandler } from "./worker-handler.js";

self.onmessage = createWorkerHandler(load, wasm, (message, transfer = []) => {
  self.postMessage(message, transfer);
});
