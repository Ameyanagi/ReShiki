//! The browser editor's name and structure conversion: the desktop naming
//! worker as a WebAssembly module of its own, which
//! `packages/editor/src/naming-worker.js` runs in a Web Worker. The editor's
//! module does not carry the OPSIN data, and a runaway parse uses this
//! module's memory, not the editor's.
#![forbid(unsafe_code)]
#![cfg(target_arch = "wasm32")]

use reshiki::naming::worker;
use wasm_bindgen::prelude::*;

// The desktop worker's allocator and budget, so a parse that needs more
// stops here as it does there.
#[global_allocator]
static ALLOCATOR: reshiki_process_heap::BoundedHeap = reshiki_process_heap::BoundedHeap;

/// The response to one serialized naming request, as the desktop worker
/// answers one on its standard input.
#[wasm_bindgen]
pub fn respond(request: &[u8]) -> Result<Vec<u8>, String> {
    reshiki_process_heap::begin(worker::HEAP_BYTES);
    worker::respond(request)
}
