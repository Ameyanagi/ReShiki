//! Framed self-executable worker. Native C++ is reachable only from this process.
use super::{client, wire};
use std::io::{self, Read, Write};
fn operation() -> Result<reshiki_geometry::Response, String> {
    let mut bytes = Vec::new();
    io::stdin()
        .take(wire::MAX_REQUEST_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    let request: wire::Request = wire::decode(&bytes, wire::MAX_REQUEST_BYTES)?;
    if request.heap_bytes == 0 || request.heap_bytes > client::MAX_HEAP_BYTES {
        return Err("Invalid geometry heap budget".into());
    }
    io::stderr().flush().map_err(|e| e.to_string())?;
    reshiki_process_heap::begin(request.heap_bytes);
    request.operation.validate()?;
    reshiki_geometry::solve(&request.operation)
}
/// Enter before initializing the GUI or async runtime. The executable must
/// install the same bounded Rust allocator as the existing InChI worker.
pub fn run() {
    let result = std::thread::Builder::new()
        .name("geometry-operation".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(operation)
        .map_err(|e| format!("Could not start geometry operation: {e}"))
        .and_then(|thread| {
            thread
                .join()
                .map_err(|_| "Geometry operation panicked".to_owned())?
        });
    let response = wire::Response {
        version: reshiki_geometry::RDKIT_VERSION.into(),
        result,
    };
    match wire::encode(&response, wire::MAX_RESPONSE_BYTES) {
        Ok(bytes) => {
            if io::stdout().write_all(&bytes).is_err() {
                std::process::exit(1);
            }
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
