//! The browser editor's Web Worker for heavy jobs, as a WebAssembly module of
//! its own that `packages/editor/src/worker.js` runs. The editor's module does
//! not carry these jobs' code and data, a runaway job uses this module's
//! memory, not the editor's, and terminating the worker frees all of it.
//!
//! Jobs exchange bytes. Naming retains its desktop protocol; rendering calls
//! the normal exporters. Separate job pools retain independent heap budgets.
#![forbid(unsafe_code)]
#![cfg(target_arch = "wasm32")]

use wasm_bindgen::prelude::*;

// The desktop workers' allocator, so a job that needs more than its budget
// stops here as it does there.
#[global_allocator]
static ALLOCATOR: reshiki_process_heap::BoundedHeap = reshiki_process_heap::BoundedHeap;

/// The response to one request for `job`.
#[wasm_bindgen]
pub async fn run(job: String, request: Vec<u8>) -> Result<Vec<u8>, String> {
    match job.as_str() {
        "naming" => {
            reshiki_process_heap::begin(reshiki::naming::worker::HEAP_BYTES);
            reshiki::naming::worker::respond(&request)
        }
        "render" => {
            reshiki_process_heap::begin(reshiki::browser_render::HEAP_BYTES);
            let request = reshiki::browser_render::install_fonts(&request)?;
            reshiki::browser_render::respond(request).await
        }
        "clipboard" => {
            reshiki_process_heap::begin(reshiki::browser_render::HEAP_BYTES);
            let request = reshiki::browser_render::install_fonts(&request)?;
            reshiki::clipboard::worker_respond(request).await
        }
        _ => Err(format!("Unknown worker job {job}")),
    }
}
