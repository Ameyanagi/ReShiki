//! Validate EMF framing before retaining or passing a picture to native playback.
//! This checks the file envelope, not every drawing instruction. Native playback
//! of imported files runs in a separate, time and memory limited Windows worker.
//! Sources: MS-EMF §§2.2.9, 2.3.3, 2.3.4; MS-EMFPLUS §2.3.
#![forbid(unsafe_code)]
#![cfg_attr(not(test), deny(clippy::indexing_slicing, clippy::unwrap_used))]

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
mod tests {
    use super::*;
    fn file() -> Vec<u8> {
        let mut bytes = vec![0; 108];
        for (offset, value) in [
            (0, 1),
            (4, 88),
            (24, 100),
            (28, 200),
            (32, 2640),
            (36, 1470),
            (40, 0x464d4520),
            (44, 0x10000),
            (48, 108),
            (52, 2),
            (56, 1),
            (88, 14),
            (92, 20),
            (104, 20),
        ] {
            bytes[offset..offset + 4].copy_from_slice(&u32::to_le_bytes(value));
        }
        bytes
    }
    #[test]
    fn physical_extent_uses_frame_not_reference_resolution() {
        assert_eq!(
            validate(&file()).unwrap(),
            Dimensions {
                width_pt: 72.,
                height_pt: 36.
            }
        );
        assert_eq!(preview_size(validate(&file()).unwrap()), (1200, 600));
        let (w, h) = preview_size(Dimensions {
            width_pt: 2880.,
            height_pt: 1440.,
        });
        assert!(w <= MAX_SIDE && h <= MAX_SIDE && u64::from(w) * u64::from(h) <= MAX_PIXELS);
    }
    #[test]
    fn real_windows_vector_and_raster_fixture_has_its_authored_physical_size() {
        for bytes in [
            &include_bytes!("../../../docs/changes/fixtures/emf-import/controlled-spectrum.emf")[..],
            &include_bytes!(
                "../../../docs/changes/fixtures/emf-import/controlled-spectrum-shifted.emf"
            )[..],
        ] {
            let dimensions = validate(bytes).unwrap();
            assert!((dimensions.width_pt * 25.4 / 72. - 100.).abs() < 0.001);
            assert!((dimensions.height_pt * 25.4 / 72. - 60.).abs() < 0.001);
            assert_eq!(preview_size(dimensions), (4724, 2834));
        }
    }
    #[test]
    fn malformed_files_fail_before_native_playback() {
        let valid = file();
        for end in 0..valid.len() {
            assert!(validate(&valid[..end]).is_err());
        }
        for (offset, value) in [
            (4, 84),
            (4, 89),
            (40, 0),
            (48, 104),
            (52, 3),
            (52, 100001),
            (32, 100),
            (60, 10),
            (68, 1),
            (88, 0),
            (88, 1),
            (92, 16),
            (104, 16),
        ] {
            let mut bytes = valid.clone();
            bytes[offset..offset + 4].copy_from_slice(&u32::to_le_bytes(value));
            assert!(
                validate(&bytes).is_err(),
                "accepted offset{offset} value{value}"
            );
        }
    }
    #[test]
    fn empty_palette_offsets_are_ignored_but_entries_cannot_overlap_size_last() {
        let mut bytes = file();
        bytes[100..104].copy_from_slice(&16u32.to_le_bytes());
        assert!(validate(&bytes).is_ok());
        bytes[96..100].copy_from_slice(&1u32.to_le_bytes());
        assert!(validate(&bytes).is_err());
    }
    #[test]
    fn original_description_and_extended_header_arrays_use_actual_field_offsets() {
        let extended = |size: usize| {
            let mut bytes = file();
            bytes.splice(88..88, vec![0; size - 88]);
            bytes[4..8].copy_from_slice(&(size as u32).to_le_bytes());
            let length = bytes.len() as u32;
            bytes[48..52].copy_from_slice(&length.to_le_bytes());
            bytes
        };
        // An original 88-byte header plus a description is not an extended
        // header, even when the whole record is longer than 108 bytes.
        let mut described = extended(112);
        described[60..64].copy_from_slice(&12u32.to_le_bytes());
        described[64..68].copy_from_slice(&88u32.to_le_bytes());
        described[88..112].copy_from_slice(&[
            65, 0, 66, 0, 67, 0, 68, 0, 69, 0, 70, 0, 71, 0, 72, 0, 73, 0, 74, 0, 0, 0, 0, 0,
        ]);
        assert!(validate(&described).is_ok());
        for size in [100, 108] {
            let mut bytes = extended(size);
            // An unused pixel-format offset must not be dereferenced.
            bytes[92..96].copy_from_slice(&16u32.to_le_bytes());
            assert!(validate(&bytes).is_ok());
            bytes[88..92].copy_from_slice(&40u32.to_le_bytes());
            assert!(validate(&bytes).is_err());
            bytes[88..92].copy_from_slice(&0u32.to_le_bytes());
            bytes[96..100].copy_from_slice(&1u32.to_le_bytes());
            assert!(validate(&bytes).is_err());
        }
    }
    #[test]
    fn comment_payloads_and_plus_records_are_bounded() {
        let mut bytes = file();
        bytes.truncate(88);
        bytes.extend(
            [70u32, 28, 16, 0x2b464d45, 0x4002, 12, 0]
                .into_iter()
                .flat_map(u32::to_le_bytes),
        );
        bytes.extend([14u32, 20, 0, 0, 20].into_iter().flat_map(u32::to_le_bytes));
        let len = bytes.len() as u32;
        bytes[48..52].copy_from_slice(&len.to_le_bytes());
        bytes[52..56].copy_from_slice(&3u32.to_le_bytes());
        assert!(validate(&bytes).is_ok());
        bytes[108..112].copy_from_slice(&0u32.to_le_bytes());
        assert!(validate(&bytes).is_err());
    }
}
