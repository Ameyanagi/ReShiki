//! Validate EMF framing before retaining or passing a picture to native playback.
//! This checks the file envelope, not every drawing instruction. Native playback
//! of imported files runs in a separate, time and memory limited Windows worker.
//! Sources: MS-EMF §§2.2.9, 2.3.3, 2.3.4; MS-EMFPLUS §2.3.
#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable,
        clippy::todo,
        clippy::unimplemented,
        clippy::indexing_slicing
    )
)]

pub const MAX_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_RECORDS: usize = 100_000;
pub const MAX_PIXELS: u64 = 16_000_000;
pub const MAX_SIDE: u32 = 8192;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dimensions {
    pub width_pt: f32,
    pub height_pt: f32,
}

fn u32_at(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let value: [u8; 4] = bytes
        .get(offset..offset.saturating_add(4))
        .and_then(|v| v.try_into().ok())
        .ok_or("Truncated EMF record")?;
    Ok(u32::from_le_bytes(value))
}

fn range(bytes: &[u8], offset: u32, count: u32, unit: usize, minimum: usize) -> Result<(), String> {
    // With no entries the offset is unused. GDI commonly writes 16 for an
    // empty EOF palette; no array is dereferenced in that case.
    if count == 0 {
        return Ok(());
    }
    let start = offset as usize;
    let length = (count as usize)
        .checked_mul(unit)
        .ok_or("Invalid EMF array size")?;
    if start < minimum || length == 0 || bytes.get(start..start.saturating_add(length)).is_none() {
        return Err("EMF array lies outside its record".into());
    }
    Ok(())
}

fn comment(bytes: &[u8], plus_records: &mut usize) -> Result<(), String> {
    let count = u32_at(bytes, 8)? as usize;
    let data = bytes
        .get(12..12usize.saturating_add(count))
        .ok_or("Truncated EMF comment")?;
    if !data.starts_with(b"EMF+") {
        return Ok(());
    }
    let mut rest = data.get(4..).ok_or("Truncated EMF+ comment")?;
    while !rest.is_empty() {
        *plus_records += 1;
        if *plus_records > MAX_RECORDS || rest.len() < 12 {
            return Err("Invalid or excessive EMF+ records".into());
        }
        let kind = u32_at(rest, 0)? & 0xffff;
        let size = u32_at(rest, 4)? as usize;
        let data_size = u32_at(rest, 8)? as usize;
        if !(0x4001..=0x403a).contains(&kind)
            || size < 12
            || !size.is_multiple_of(4)
            || size > rest.len()
            || data_size > size - 12
        {
            return Err("Invalid EMF+ record framing".into());
        }
        rest = rest.get(size..).ok_or("Truncated EMF+ record")?;
    }
    Ok(())
}

/// Original physical frame in points. Arbitrary frame origins are retained by
/// the EMF itself; insertion uses its extent, never the reference display DPI.
pub fn validate(bytes: &[u8]) -> Result<Dimensions, String> {
    if bytes.len() < 108 || bytes.len() > MAX_BYTES || !bytes.len().is_multiple_of(4) {
        return Err("EMF must be a complete file no larger than 16 MB".into());
    }
    let header_size = u32_at(bytes, 4)? as usize;
    if u32_at(bytes, 0)? != 1
        || header_size < 88
        || !header_size.is_multiple_of(4)
        || header_size > bytes.len()
        || u32_at(bytes, 40)? != 0x464d_4520
        || u32_at(bytes, 48)? as usize != bytes.len()
    {
        return Err("Invalid EMF header or file length".into());
    }
    let records = u32_at(bytes, 52)? as usize;
    if !(2..=MAX_RECORDS).contains(&records) {
        return Err("EMF record limit exceeded".into());
    }
    let header = bytes.get(..header_size).ok_or("Truncated EMF header")?;
    range(header, u32_at(header, 64)?, u32_at(header, 60)?, 2, 88)?;
    // Description data can begin at 88 in the original header. Determine the
    // fixed header length from the first variable field, not Size alone.
    let description = if u32_at(header, 60)? == 0 {
        0
    } else {
        u32_at(header, 64)? as usize
    };
    let mut fixed_size = if description == 0 {
        header_size
    } else {
        description.min(header_size)
    };
    if fixed_size >= 100 {
        if u32_at(header, 96)? != 0 {
            return Err("OpenGL EMF pictures are not supported".into());
        }
        let pixel_offset = u32_at(header, 92)?;
        let pixel_count = u32_at(header, 88)?;
        range(header, pixel_offset, pixel_count, 1, 100)?;
        if pixel_count != 0 {
            fixed_size = fixed_size.min(pixel_offset as usize);
        }
        if fixed_size < 100 {
            return Err("Invalid EMF header extension".into());
        }
    }
    let extent = |lo, hi| -> Result<f32, String> {
        let low = i64::from(u32_at(header, lo)? as i32);
        let high = i64::from(u32_at(header, hi)? as i32);
        let points = (high - low) as f64 * 72. / 2540.;
        if !(0.1..=2880.).contains(&points) {
            return Err("EMF physical dimensions must be between 0.1 pt and 40 inches".into());
        }
        Ok(points as f32)
    };
    let dimensions = Dimensions {
        width_pt: extent(24, 32)?,
        height_pt: extent(28, 36)?,
    };
    let mut rest = bytes;
    let (mut found, mut plus_records) = (0usize, 0usize);
    while !rest.is_empty() {
        let kind = u32_at(rest, 0)?;
        let size = u32_at(rest, 4)? as usize;
        // EMR 69 and 117 are reserved; OpenGL escape records 102/103/105
        // are not supported as imported Office/scientific pictures.
        if !(1..=122).contains(&kind)
            || matches!(kind, 69 | 102 | 103 | 105 | 117)
            || size < 8
            || !size.is_multiple_of(4)
            || size > rest.len()
            || kind == 1 && found != 0
        {
            return Err("Invalid or unsupported EMF record".into());
        }
        found += 1;
        if found > records {
            return Err("EMF record count does not match its header".into());
        }
        let record = rest.get(..size).ok_or("Truncated EMF record")?;
        if kind == 70 {
            comment(record, &mut plus_records)?;
        }
        if kind == 14 {
            if size < 20 || size != rest.len() || u32_at(record, size - 4)? as usize != size {
                return Err("Invalid EMF end record".into());
            }
            if u32_at(record, 8)? != u32_at(header, 68)? {
                return Err("EMF palette count does not match its header".into());
            }
            range(
                record.get(..size - 4).ok_or("Truncated EMF palette")?,
                u32_at(record, 12)?,
                u32_at(record, 8)?,
                4,
                16,
            )?;
            if found != records {
                return Err("EMF record count does not match its header".into());
            }
            return Ok(dimensions);
        }
        rest = rest.get(size..).ok_or("Truncated EMF record")?;
    }
    Err("EMF has no end record".into())
}

/// A 1200 DPI portable preview, reduced uniformly to existing picture limits.
pub fn preview_size(d: Dimensions) -> (u32, u32) {
    let width = f64::from(d.width_pt) * 1200. / 72.;
    let height = f64::from(d.height_pt) * 1200. / 72.;
    let scale = 1_f64
        .min(f64::from(MAX_SIDE) / width.max(height))
        .min((MAX_PIXELS as f64 / (width * height)).sqrt());
    (
        (width * scale).floor().max(1.) as u32,
        (height * scale).floor().max(1.) as u32,
    )
}

#[cfg(test)]
mod tests;
