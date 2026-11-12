//! Editable drawing snapshots carried by ordinary PNG images.
use crate::document::Document;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use flate2::{Compression, Crc, bufread::ZlibDecoder, write::ZlibEncoder};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};

/// Larger envelopes need the caller's explicit export-size decision.
pub const EMBED_LIMIT: usize = 2 * 1024 * 1024;
const DECODE_LIMIT: usize = 64 * 1024 * 1024;
const MAGIC: &[u8; 4] = b"RSKD";
const VERSION: u8 = 1;
const HEADER_LEN: usize = 4 + 1 + 4 + 32;
const PNG: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
const KEYWORD: &[u8] = b"dev.reshiki.drawing";

/// Preserve the visible figure while removing editor history and provenance.
pub fn visible(doc: &Document) -> Document {
    let mut doc = doc.clone();
    doc.recent_colors.clear();
    doc.page_layout = None;
    for model in &mut doc.models {
        model.notes.clear();
    }
    if let Some(theme) = &mut doc.custom_theme {
        theme.author.clear();
        theme.source.clear();
        theme.license.clear();
    }
    doc
}

/// Compact JSON, its length and digest, followed by a zlib stream.
pub fn encode(doc: &Document) -> Result<Vec<u8>, String> {
    let json = serde_json::to_vec(&visible(doc)).map_err(|e| e.to_string())?;
    let length = u32::try_from(json.len()).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(MAGIC);
    bytes.push(VERSION);
    bytes.extend_from_slice(&length.to_le_bytes());
    bytes.extend_from_slice(&Sha256::digest(&json));
    let mut encoder = ZlibEncoder::new(bytes, Compression::default());
    encoder.write_all(&json).map_err(|e| e.to_string())?;
    encoder.finish().map_err(|e| e.to_string())
}

/// Check all envelope limits before deserializing untrusted drawing content.
pub fn decode(bytes: &[u8]) -> Result<Document, String> {
    if !bytes.starts_with(MAGIC) {
        return Err("Invalid embedded drawing signature".into());
    }
    let version = bytes.get(4).ok_or("Truncated embedded drawing header")?;
    if *version > VERSION {
        return Err(
            "This drawing needs a newer version of ReShiki. Update ReShiki to open it.".into(),
        );
    }
    if *version != VERSION {
        return Err(format!("Unsupported embedded drawing version {version}"));
    }
    let length = bytes
        .get(5..9)
        .and_then(|n| n.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or("Truncated embedded drawing header")? as usize;
    if length > DECODE_LIMIT {
        return Err("Embedded drawing exceeds 64 MiB".into());
    }
    let digest = bytes
        .get(9..HEADER_LEN)
        .ok_or("Truncated embedded drawing header")?;
    let compressed = bytes
        .get(HEADER_LEN..)
        .ok_or("Truncated embedded drawing header")?;
    let mut decoder = ZlibDecoder::new(compressed);
    let mut json = Vec::new();
    decoder
        .by_ref()
        .take(length as u64 + 1)
        .read_to_end(&mut json)
        .map_err(|e| format!("Invalid embedded drawing compression: {e}"))?;
    if json.len() != length {
        return Err("Embedded drawing length does not match its header".into());
    }
    let actual: [u8; 32] = Sha256::digest(&json).into();
    if actual.as_slice() != digest {
        return Err("Embedded drawing checksum does not match".into());
    }
    // from_json validates before and after its native-format migrations.
    Document::from_json(&json)
}

/// Insert an uncompressed iTXt chunk immediately after the PNG's IHDR chunk.
pub fn embed(format: &str, mut bytes: Vec<u8>, envelope: &[u8]) -> Result<Vec<u8>, String> {
    match format {
        "png" => {}
        _ => return Err("Embedded drawings require PNG output".into()),
    }
    if !bytes.starts_with(PNG) || bytes.get(8..16) != Some(b"\0\0\0\rIHDR") || bytes.len() < 33 {
        return Err("Invalid PNG header".into());
    }
    let mut data = Vec::new();
    data.extend_from_slice(KEYWORD);
    // Keyword terminator, compression flag/method, language and translated key.
    data.extend_from_slice(&[0; 5]);
    data.extend_from_slice(STANDARD.encode(envelope).as_bytes());
    let length = u32::try_from(data.len()).map_err(|_| "Embedded PNG chunk is too large")?;
    let tail = bytes.split_off(33);
    bytes.extend_from_slice(&length.to_be_bytes());
    bytes.extend_from_slice(b"iTXt");
    bytes.extend_from_slice(&data);
    bytes.extend_from_slice(&crc(b"iTXt", &data).to_be_bytes());
    bytes.extend_from_slice(&tail);
    Ok(bytes)
}

/// Find a drawing without decoding pixels: None when there is none, an error
/// when one is present but damaged.
pub fn read(bytes: &[u8]) -> Option<Result<Document, String>> {
    let mut rest = bytes.strip_prefix(PNG)?;
    while let Some((header, after)) = rest.split_first_chunk::<8>() {
        let (length, kind) = header.split_at(4);
        let length = u32::from_be_bytes(length.try_into().ok()?) as usize;
        if kind == b"IEND" {
            return None;
        }
        if kind == b"iTXt"
            && after
                .strip_prefix(KEYWORD)
                .is_some_and(|after| after.starts_with(b"\0"))
        {
            let (Some(data), Some(checksum)) = (after.get(..length), after.get(length..length + 4))
            else {
                return Some(Err("Truncated embedded PNG chunk".into()));
            };
            if checksum != crc(kind, data).to_be_bytes() {
                return Some(Err("Embedded PNG chunk checksum does not match".into()));
            }
            return Some(text(data.get(KEYWORD.len() + 1..).unwrap_or_default()));
        }
        rest = after.get(length + 4..)?;
    }
    None
}

fn crc(kind: &[u8], data: &[u8]) -> u32 {
    let mut crc = Crc::new();
    crc.update(kind);
    crc.update(data);
    crc.sum()
}

/// The iTXt fields after the keyword: uncompressed flag and method, an empty
/// language tag and translated keyword, then the base64 envelope.
fn text(fields: &[u8]) -> Result<Document, String> {
    let text = fields
        .strip_prefix(b"\0\0\0\0")
        .ok_or("Unsupported embedded PNG text fields")?;
    decode(
        &STANDARD
            .decode(text)
            .map_err(|e| format!("Invalid embedded drawing encoding: {e}"))?,
    )
}

#[cfg(test)]
mod tests;
