// Name ↔ structure conversion for the editor, off the page's thread, in a
// WebAssembly module and memory of its own. The editor starts this worker on
// the first conversion, and terminates it when a request runs past its time
// limit or is cancelled. Posts null once loaded, then for each request the
// response bytes or the failure's text.
import load, { respond } from "../dist/naming/reshiki_naming_web.js";

const loading = load().then(() => self.postMessage(null));

self.onmessage = async ({ data }) => {
  try {
    await loading;
    self.postMessage(respond(data));
  } catch (error) {
    self.postMessage(String(error?.message ?? error));
  }
};
