// Heavy jobs run in their own WebAssembly memory. The caller terminates the
// worker on cancellation or timeout and starts a fresh one for the next job.
import load, { run } from "../dist/worker/reshiki_web_worker.js";
import { serveWorker } from "./worker-handler.js";

serveWorker(self, { load, run });
