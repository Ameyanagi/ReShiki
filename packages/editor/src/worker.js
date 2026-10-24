// The editor's heavy jobs, such as name ↔ structure conversion, off the page's
// thread, in a WebAssembly module and memory of their own. The editor starts
// this worker on first use and terminates it when a job runs past its time
// limit or is cancelled. Takes [job, request bytes]; posts null once loaded,
// then for each job the response bytes or the failure's text.
import load, { run } from "../dist/worker/reshiki_web_worker.js";

const loading = load().then(() => self.postMessage(null));

self.onmessage = async ({ data: [job, request] }) => {
  try {
    await loading;
    self.postMessage(run(job, request));
  } catch (error) {
    self.postMessage(String(error?.message ?? error));
  }
};
