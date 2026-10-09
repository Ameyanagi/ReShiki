//! Framed self-executable worker. Rust geometry runs only in this process.
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
    let capacity = resolved_capacity(request.heap_bytes, request.capacity)?;
    request.operation.validate_capacity(capacity)?;
    let response = reshiki_geometry::solve(&request.operation)?;
    if response.coordinates.len() > capacity.coordinates {
        return Err("Geometry result exceeds its machine-budget coordinate capacity".into());
    }
    Ok(response)
}
fn resolved_capacity(
    heap_bytes: usize,
    requested: Option<reshiki_geometry::Capacity>,
) -> Result<reshiki_geometry::Capacity, String> {
    let memory_capacity = reshiki_geometry::Capacity::for_heap(heap_bytes);
    let capacity = requested.unwrap_or_else(|| {
        let legacy = reshiki_geometry::Capacity::default();
        reshiki_geometry::Capacity {
            atoms: legacy.atoms.min(memory_capacity.atoms),
            bonds: legacy.bonds.min(memory_capacity.bonds),
            coordinates: legacy.coordinates.min(memory_capacity.coordinates),
        }
    });
    capacity.validate()?;
    if capacity.atoms > memory_capacity.atoms
        || capacity.bonds > memory_capacity.bonds
        || capacity.coordinates > memory_capacity.coordinates
    {
        return Err("Geometry capacity exceeds its heap-budget envelope".into());
    }
    Ok(capacity)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_capacity_and_explicit_capacity_cannot_bypass_the_heap_envelope() {
        let low = reshiki_geometry::Capacity::for_heap(64 * 1024 * 1024);
        assert_eq!(resolved_capacity(64 * 1024 * 1024, None).unwrap(), low);
        assert!(
            resolved_capacity(
                64 * 1024 * 1024,
                Some(reshiki_geometry::Capacity::default())
            )
            .is_err()
        );
        let high = reshiki_geometry::Capacity::for_heap(512 * 1024 * 1024);
        assert_eq!(
            resolved_capacity(512 * 1024 * 1024, Some(high)).unwrap(),
            high
        );
        assert_eq!(
            resolved_capacity(512 * 1024 * 1024, None).unwrap(),
            reshiki_geometry::Capacity::default()
        );
        assert!(
            resolved_capacity(
                512 * 1024 * 1024,
                Some(reshiki_geometry::Capacity { atoms: 0, ..high })
            )
            .is_err()
        );
    }
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
