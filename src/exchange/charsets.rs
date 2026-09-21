//! Exact legacy codepage tables, generated from Python's standard codecs.
//! Each file starts with 256 little-endian u32 single-byte mappings, followed
//! by sorted (u16 byte-pair, u32 Unicode scalar) records. 0xffffffff is undefined.
//! These are data only; no Python process is involved in decoding.
use super::{Reader, Result};

pub(super) fn decode(bytes: &[u8], charset: u16) -> Result<String> {
    let table: &[u8] = match charset {
        10000 => include_bytes!("charsets/mac_roman.bin"),
        1252 => include_bytes!("charsets/cp1252.bin"),
        1251 => include_bytes!("charsets/cp1251.bin"),
        932 => include_bytes!("charsets/cp932.bin"),
        936 => include_bytes!("charsets/gbk.bin"),
        949 => include_bytes!("charsets/cp949.bin"),
        950 => include_bytes!("charsets/big5.bin"),
        _ => return Err(format!("Unsupported drawing text charset {charset}")),
    };
    let (single, pairs) = table
        .split_at_checked(1024)
        .ok_or("Invalid built-in charset table")?;
    let mut iter = bytes.iter().copied();
    let mut text = String::new();
    while let Some(byte) = iter.next() {
        let at = usize::from(byte) * 4;
        let record = single
            .get(at..at + 4)
            .ok_or("Invalid built-in charset entry")?;
        let point = Reader::new(record).u32()?;
        let point = if point == u32::MAX {
            let trail = iter.next().ok_or("Invalid binary drawing text encoding")?;
            lookup(pairs, u16::from_be_bytes([byte, trail]))?
        } else {
            point
        };
        text.push(char::from_u32(point).ok_or("Invalid binary drawing text encoding")?);
    }
    Ok(text)
}
fn lookup(pairs: &[u8], code: u16) -> Result<u32> {
    let (mut low, mut high) = (0, pairs.len() / 6);
    while low < high {
        let mid = low + (high - low) / 2;
        let at = mid * 6;
        let mut r = Reader::new(
            pairs
                .get(at..at + 6)
                .ok_or("Invalid built-in charset pair")?,
        );
        match r.u16()?.cmp(&code) {
            std::cmp::Ordering::Equal => return r.u32(),
            std::cmp::Ordering::Less => low = mid + 1,
            std::cmp::Ordering::Greater => high = mid,
        }
    }
    Err("Invalid binary drawing text encoding".into())
}
