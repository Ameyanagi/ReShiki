//! One offline bounded rule-generation request, before GUI/runtime startup.
use super::rules;
use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};

pub(super) const WIRE_LIMIT: usize = 32_768;
pub(super) const HEAP_BYTES: usize = 96 * 1024 * 1024;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Request {
    pub protocol: u32,
    pub profile: String,
    pub smiles: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Response {
    pub protocol: u32,
    pub profile: String,
    pub input: String,
    pub result: Result<String, String>,
}
fn operation() -> Result<Response, String> {
    io::stderr().flush().map_err(|e| e.to_string())?;
    reshiki_process_heap::begin(HEAP_BYTES);
    let mut bytes = vec![];
    io::stdin()
        .take((WIRE_LIMIT + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > WIRE_LIMIT {
        return Err("Local naming request exceeds its wire limit".into());
    }
    let request: Request = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if request.protocol != 1 || request.profile != rules::PROFILE {
        return Err("Incompatible local naming rule protocol/profile".into());
    }
    Ok(Response {
        protocol: 1,
        profile: rules::PROFILE.into(),
        input: request.smiles.clone(),
        result: rules::generate(&request.smiles),
    })
}
/// The executable must install the existing BoundedHeap allocator. No API,
/// Java or runtime graphics are initialized by this native rule worker.
pub fn run() {
    match operation().and_then(|r| serde_json::to_vec(&r).map_err(|e| e.to_string())) {
        Ok(bytes) if bytes.len() <= WIRE_LIMIT => {
            if io::stdout().write_all(&bytes).is_err() {
                std::process::exit(1);
            }
        }
        Ok(_) => {
            eprintln!("Local naming response exceeds its wire limit");
            std::process::exit(1);
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
