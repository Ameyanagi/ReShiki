//! Byte-bounded, versioned frames; native memory layouts never cross the pipe.
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::io::{self, Write};
pub(super) const MAX_REQUEST_BYTES: usize = 4 * 1024 * 1024;
pub(super) const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
const MAGIC: &[u8; 8] = b"RSHGEOM1";
const PROTOCOL: u16 = 1;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Request {
    pub heap_bytes: usize,
    pub operation: reshiki_geometry::Request,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Response {
    pub version: String,
    pub result: Result<reshiki_geometry::Response, String>,
}

struct Bounded {
    bytes: Vec<u8>,
    limit: usize,
}
impl Write for Bounded {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(io::Error::other("Geometry frame exceeds its byte limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub(super) fn encode(value: &impl Serialize, limit: usize) -> Result<Vec<u8>, String> {
    let mut out = Bounded {
        bytes: Vec::new(),
        limit,
    };
    out.write_all(MAGIC).map_err(|e| e.to_string())?;
    out.write_all(&PROTOCOL.to_le_bytes())
        .map_err(|e| e.to_string())?;
    out.write_all(&[0; 6]).map_err(|e| e.to_string())?;
    serde_json::to_writer(&mut out, value).map_err(|e| e.to_string())?;
    let size = u32::try_from(out.bytes.len().saturating_sub(16)).map_err(|e| e.to_string())?;
    out.bytes
        .get_mut(12..16)
        .ok_or("Missing geometry header")?
        .copy_from_slice(&size.to_le_bytes());
    Ok(out.bytes)
}
pub(super) fn decode<T: DeserializeOwned>(bytes: &[u8], limit: usize) -> Result<T, String> {
    if bytes.len() > limit {
        return Err("Geometry frame exceeds its byte limit".into());
    }
    if bytes.get(..8) != Some(MAGIC.as_slice())
        || bytes.get(8..10) != Some(PROTOCOL.to_le_bytes().as_slice())
        || bytes.get(10..12) != Some(&[0, 0])
    {
        return Err("Incompatible geometry worker protocol".into());
    }
    let size = u32::from_le_bytes(
        bytes
            .get(12..16)
            .ok_or("Truncated geometry header")?
            .try_into()
            .map_err(|_| "Invalid geometry length")?,
    ) as usize;
    let body = bytes.get(16..).ok_or("Truncated geometry body")?;
    if body.len() != size {
        return Err("Geometry frame length mismatch".into());
    }
    serde_json::from_slice(body).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frames_reject_truncation_trailing_bytes_unknown_fields_and_oversize() {
        let frame = encode(&serde_json::json!({"value":1}), 128).unwrap();
        let value: serde_json::Value = decode(&frame, 128).unwrap();
        assert_eq!(value["value"], 1);
        for n in 0..frame.len() {
            assert!(decode::<serde_json::Value>(&frame[..n], 128).is_err());
        }
        let mut extra = frame.clone();
        extra.push(0);
        assert!(decode::<serde_json::Value>(&extra, 128).is_err());
        assert!(decode::<serde_json::Value>(&frame, frame.len() - 1).is_err());
        assert!(encode(&vec!["overlimit"; 100], 32).is_err());
        assert!(
            decode::<Request>(
                &encode(&serde_json::json!({"unexpected":true}), 128).unwrap(),
                128
            )
            .is_err()
        );
    }
}
