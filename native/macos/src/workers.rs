//! Standard-I/O entry points used by the application's isolated worker modes.
use std::io::{self, Read, Write};

fn exchange(limit: usize, operation: fn(&[u8]) -> Result<Vec<u8>, String>) -> Result<(), String> {
    let mut input = Vec::new();
    io::stdin()
        .take(limit as u64 + 1)
        .read_to_end(&mut input)
        .map_err(|e| e.to_string())?;
    if input.len() > limit {
        return Err("Native request is too large".into());
    }
    let output = operation(&input)?;
    io::stdout().write_all(&output).map_err(|e| e.to_string())
}

pub fn clipboard() -> Result<(), String> {
    exchange(crate::clipboard::LIMIT * 2, crate::clipboard::execute)
}

pub fn print() -> Result<(), String> {
    exchange(65536, crate::printing::execute)
}
