//! The browser editor's Web Worker for heavy jobs, as a WebAssembly module of
//! its own that `packages/editor/src/worker.js` runs. The editor's module does
//! not carry these jobs' code and data, a runaway job uses this module's
//! memory, not the editor's, and terminating the worker frees all of it.
//!
//! Each job takes and returns bytes in its own desktop worker protocol. Only
//! naming runs here so far; InChI, 3D geometry and raster export can follow.
#![forbid(unsafe_code)]
#![cfg(target_arch = "wasm32")]

use wasm_bindgen::prelude::*;

// The desktop workers' allocator, so a job that needs more than its budget
// stops here as it does there.
#[global_allocator]
static ALLOCATOR: reshiki_process_heap::BoundedHeap = reshiki_process_heap::BoundedHeap;

/// The response to one request for `job`.
#[wasm_bindgen]
pub fn run(job: &str, request: &[u8]) -> Result<Vec<u8>, String> {
    match job {
        "naming" => {
            reshiki_process_heap::begin(reshiki::naming::worker::HEAP_BYTES);
            reshiki::naming::worker::respond(request)
        }
        _ => Err(format!("Unknown worker job {job}")),
    }
}
