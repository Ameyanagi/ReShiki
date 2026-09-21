//! Bounded CDX/CDXML serialization. Chemistry remains behind `ChemistryEngine`.
//! The Python codec is retained as a differential test oracle during migration.
mod charsets;
mod decode;
mod encode;
mod schema;
mod values;

pub use decode::from_cdx;
pub use encode::to_cdx;
use std::collections::HashMap;

pub const LIMIT: usize = 16 * 1024 * 1024;
const MAX_OBJECTS: usize = 100_000;
const MAX_PROPERTIES: usize = 1_000_000;
const HEADER: &[u8] = b"VjCD0100\x04\x03\x02\x01\0\0\0\0\0\0\0\0\0\0";
type Result<T> = std::result::Result<T, String>;

struct Property {
    code: u16,
    name: &'static str,
    kind: &'static str,
    variants: &'static [(&'static str, i64)],
}

const OBJECTS: &[(u16, &str)] = &[
    (0x8000, "CDXML"),
    (0x8001, "page"),
    (0x8002, "group"),
    (0x8003, "fragment"),
    (0x8004, "n"),
    (0x8005, "b"),
    (0x8006, "t"),
    (0x8007, "graphic"),
    (0x8008, "curve"),
    (0x8009, "embeddedobject"),
    (0x8011, "objecttag"),
    (0x8027, "arrow"),
];

struct Schema {
    names: HashMap<&'static str, &'static Property>,
    codes: HashMap<u16, &'static Property>,
}
impl Schema {
    fn new() -> Self {
        Self {
            names: schema::PROPERTIES.iter().map(|p| (p.name, p)).collect(),
            codes: schema::PROPERTIES.iter().map(|p| (p.code, p)).collect(),
        }
    }
}

struct Reader<'a> {
    remaining: &'a [u8],
}
impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { remaining: data }
    }
    fn take(&mut self, size: usize) -> Result<&'a [u8]> {
        let (data, rest) = self
            .remaining
            .split_at_checked(size)
            .ok_or("Truncated binary drawing")?;
        self.remaining = rest;
        Ok(data)
    }
    fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        self.take(N)?
            .try_into()
            .map_err(|_| "Invalid binary number".into())
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.array()?))
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.array()?))
    }
    fn done(&self) -> Result<()> {
        if self.remaining.is_empty() {
            Ok(())
        } else {
            Err("Unexpected bytes in binary drawing property".into())
        }
    }
}

fn append(out: &mut Vec<u8>, data: &[u8]) -> Result<()> {
    if data.len() > LIMIT.saturating_sub(out.len()) {
        return Err("Binary drawing exceeds the 16 MB limit".into());
    }
    out.extend_from_slice(data);
    Ok(())
}
fn property(out: &mut Vec<u8>, code: u16, data: &[u8]) -> Result<()> {
    append(out, &code.to_le_bytes())?;
    if let Some(size) = u16::try_from(data.len()).ok().filter(|n| *n < u16::MAX) {
        append(out, &size.to_le_bytes())?;
    } else {
        append(out, &u16::MAX.to_le_bytes())?;
        let size = u32::try_from(data.len()).map_err(|_| "Drawing property is too large")?;
        append(out, &size.to_le_bytes())?;
    }
    append(out, data)
}
fn word(value: usize) -> Result<[u8; 2]> {
    Ok(u16::try_from(value)
        .map_err(|_| "Drawing value exceeds the binary format range")?
        .to_le_bytes())
}
fn integer<T: std::str::FromStr>(text: &str) -> Result<T> {
    text.trim()
        .parse()
        .map_err(|_| "Drawing integer exceeds the binary format range".into())
}
fn finite(text: &str) -> Result<f64> {
    let n: f64 = text.trim().parse().map_err(|_| "Invalid drawing number")?;
    if !n.is_finite() {
        return Err("Non-finite drawing value".into());
    }
    Ok(n)
}
