//! Shared, bounded, versioned process protocol. No native memory layouts cross
//! this boundary; both sides reject unknown fields and trailing bytes.
use super::{generator, kernel, output};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::io::{self, Write};

pub const PROTOCOL: u16 = 3;
pub const RESOURCE_EXIT: i32 = 75;
const MAGIC: &[u8; 8] = b"RSHINCHI";

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub heap_bytes: usize,
    pub operation: Operation,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Operation {
    Generate(Box<kernel::Molecule>),
    Read {
        inchi: String,
        options: output::Options,
    },
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub version: String,
    pub result: Result<Reply, String>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Reply {
    Generated(kernel::Generated),
    Imported(Box<kernel::Imported>),
}

struct Bounded {
    bytes: Vec<u8>,
    limit: usize,
}
impl Write for Bounded {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(io::Error::other("InChI frame exceeds its byte limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub fn encode(value: &impl Serialize, limit: usize) -> Result<Vec<u8>, String> {
    let mut out = Bounded {
        bytes: Vec::new(),
        limit,
    };
    out.write_all(MAGIC).map_err(|e| e.to_string())?;
    out.write_all(&PROTOCOL.to_le_bytes())
        .map_err(|e| e.to_string())?;
    out.write_all(&[0; 6]).map_err(|e| e.to_string())?;
    serde_json::to_writer(&mut out, value).map_err(|e| e.to_string())?;
    let length = u32::try_from(out.bytes.len().saturating_sub(16)).map_err(|e| e.to_string())?;
    out.bytes
        .get_mut(12..16)
        .ok_or("Missing frame header")?
        .copy_from_slice(&length.to_le_bytes());
    Ok(out.bytes)
}
pub fn decode<T: DeserializeOwned>(bytes: &[u8], limit: usize) -> Result<T, String> {
    if bytes.len() > limit {
        return Err("InChI frame exceeds its byte limit".into());
    }
    if bytes.get(..8) != Some(MAGIC.as_slice())
        || bytes.get(8..10) != Some(PROTOCOL.to_le_bytes().as_slice())
        || bytes.get(10..12) != Some(&[0, 0])
    {
        return Err("Incompatible InChI helper protocol".into());
    }
    let count = u32::from_le_bytes(
        bytes
            .get(12..16)
            .ok_or("Truncated frame header")?
            .try_into()
            .map_err(|_| "Invalid frame length")?,
    ) as usize;
    let body = bytes.get(16..).ok_or("Truncated frame body")?;
    if body.len() != count {
        return Err("InChI frame length mismatch".into());
    }
    serde_json::from_slice(body).map_err(|e| e.to_string())
}
impl Request {
    pub fn validate(&self) -> Result<(), String> {
        if self.heap_bytes == 0 || self.heap_bytes > generator::MAX_HEAP_BYTES {
            return Err("Invalid helper heap budget".into());
        }
        match &self.operation {
            Operation::Generate(m) => {
                super::input::prepare(&m.state, m.positions.as_deref())
                    .map_err(|e| e.to_string())?;
            }
            Operation::Read { inchi, .. } if inchi.len() > generator::MAX_INCHI_BYTES => {
                return Err("InChI text exceeds its byte limit".into());
            }
            Operation::Read { .. } => {}
        }
        Ok(())
    }
}
