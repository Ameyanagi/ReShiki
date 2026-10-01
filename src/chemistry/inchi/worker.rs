//! One bounded, cancellable InChI operation using the pure Rust Cargo dependency.
use super::{generator, kernel, wire};
use std::io::{self, Read, Write};

fn operation() -> Result<wire::Reply, String> {
    let mut bytes = Vec::new();
    io::stdin()
        .take(generator::MAX_REQUEST_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    let request: wire::Request = wire::decode(&bytes, generator::MAX_REQUEST_BYTES)?;
    if request.heap_bytes == 0 || request.heap_bytes > generator::MAX_HEAP_BYTES {
        return Err("Invalid helper heap budget".into());
    }
    // Initialize standard I/O before any allocator failure may report to it.
    io::stderr().flush().map_err(|e| e.to_string())?;
    reshiki_process_heap::begin(request.heap_bytes);
    request.validate()?;
    match request.operation {
        wire::Operation::Generate(molecule) => {
            kernel::generate(&molecule).map(wire::Reply::Generated)
        }
        wire::Operation::Read { inchi, options } => kernel::read(&inchi, options)
            .map(Box::new)
            .map(wire::Reply::Imported),
    }
}
/// Handle one framed request, then terminate the worker process.
/// The caller must install `reshiki_process_heap::BoundedHeap` as its allocator.
pub fn run() {
    // The kernel recursively processes large molecular graphs. Windows' main
    // thread stack is too small for the supported large-MOL corpus in debug
    // builds. Give both executable entry points the same explicit stack while
    // retaining the process-wide heap budget and parent-enforced deadline.
    let result = std::thread::Builder::new()
        .name("inchi-operation".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(operation)
        .map_err(|e| format!("Could not start InChI operation: {e}"))
        .and_then(|thread| {
            thread
                .join()
                .map_err(|_| "InChI operation panicked".to_owned())?
        });
    let response = wire::Response {
        version: kernel::VERSION.into(),
        result,
    };
    match wire::encode(&response, generator::MAX_RESPONSE_BYTES) {
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
