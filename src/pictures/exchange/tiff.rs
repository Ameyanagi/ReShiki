//! Preserve TIFF sample interpretation that the generic image adapter omits.
use image::{DynamicImage, RgbaImage, metadata::Orientation};
use std::io::Cursor;
use tiff::{
    ColorType,
    decoder::{Decoder, DecodingResult, Limits},
    tags::Tag,
};

pub(super) fn decode(bytes: &[u8], remaining: u64) -> Result<DynamicImage, String> {
    if bytes.len() > super::MAX_BYTES {
        return Err("Embedded picture exceeds 16 MB".into());
    }
    let mut decoder = Decoder::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let mut limits = Limits::default();
    limits.decoding_buffer_size = 128 * 1024 * 1024;
    limits.intermediate_buffer_size = 128 * 1024 * 1024;
    decoder = decoder.with_limits(limits);
    let (width, height) = decoder.dimensions().map_err(|e| e.to_string())?;
    let count = u64::from(width) * u64::from(height);
    if width == 0
        || height == 0
        || width > super::super::MAX_SIDE
        || height > super::super::MAX_SIDE
        || count > super::super::MAX_PIXELS
    {
        return Err(
            "Embedded pictures are limited to 8192 pixels per side and 16 million pixels".into(),
        );
    }
    if count > remaining {
        return Err("A drawing can contain at most 64 million picture pixels".into());
    }
    let orientation = decoder
        .find_tag_unsigned::<u8>(Tag::Orientation)
        .map_err(|e| e.to_string())?
        .and_then(Orientation::from_exif)
        .unwrap_or(Orientation::NoTransforms);
    let associated = decoder
        .find_tag_unsigned_vec::<u16>(Tag::ExtraSamples)
        .map_err(|e| e.to_string())?
        .and_then(|v| v.first().copied())
        == Some(1);
    let palette = decoder
        .find_tag_unsigned::<u16>(Tag::PhotometricInterpretation)
        .map_err(|e| e.to_string())?
        == Some(3);
    let color = decoder.colortype();
    let samples = if palette {
        let colors = decoder
            .find_tag_unsigned_vec::<u16>(Tag::ColorMap)
            .map_err(|e| e.to_string())?
            .ok_or("Palette TIFF is missing its color map")?;
        let bits = decoder
            .find_tag_unsigned_vec::<u8>(Tag::BitsPerSample)
            .map_err(|e| e.to_string())?
            .and_then(|v| v.first().copied())
            .ok_or("Missing palette TIFF bit depth")?;
        if ![1, 2, 4, 8].contains(&bits) || colors.len() != 3usize * (1usize << bits) {
            return Err("Invalid palette TIFF color map".into());
        }
        let raw = palette_as_gray(bytes)?;
        let mut indexed = Decoder::new(Cursor::new(raw)).map_err(|e| e.to_string())?;
        let mut limits = Limits::default();
        limits.decoding_buffer_size = 128 * 1024 * 1024;
        limits.intermediate_buffer_size = 128 * 1024 * 1024;
        indexed = indexed.with_limits(limits);
        let DecodingResult::U8(values) = indexed.read_image().map_err(|e| e.to_string())? else {
            return Err("Invalid palette TIFF samples".into());
        };
        let stride = (width as usize * usize::from(bits)).div_ceil(8);
        let size = 1usize << bits;
        let mut rgba = Vec::with_capacity(count as usize * 4);
        for y in 0..height as usize {
            for x in 0..width as usize {
                let bit = x * usize::from(bits);
                let byte = values
                    .get(y * stride + bit / 8)
                    .ok_or("Truncated palette TIFF pixels")?;
                let index = usize::from(
                    (byte >> (8 - usize::from(bits) - bit % 8)) & ((1u16 << bits) - 1) as u8,
                );
                for channel in 0..3 {
                    let value = colors
                        .get(channel * size + index)
                        .ok_or("Invalid palette TIFF index")?;
                    rgba.push((value >> 8) as u8);
                }
                rgba.push(255);
            }
        }
        Some(rgba)
    } else {
        match color.map_err(|e| e.to_string())? {
            ColorType::Gray(16 | 32 | 64) => {
                let values = decoder.read_image().map_err(|e| e.to_string())?;
                let mut rgba = Vec::with_capacity(count as usize * 4);
                macro_rules! gray {
                    ($v:expr) => {
                        for value in $v {
                            let v = (value as f64).clamp(0., 255.) as u8;
                            rgba.extend_from_slice(&[v, v, v, 255]);
                        }
                    };
                }
                match values {
                    DecodingResult::U8(v) => gray!(v),
                    DecodingResult::U16(v) => gray!(v),
                    DecodingResult::U32(v) => gray!(v),
                    DecodingResult::U64(v) => gray!(v),
                    DecodingResult::I8(v) => gray!(v),
                    DecodingResult::I16(v) => gray!(v),
                    DecodingResult::I32(v) => gray!(v),
                    DecodingResult::I64(v) => gray!(v),
                    DecodingResult::F32(v) => gray!(v),
                    DecodingResult::F64(v) => gray!(v),
                    DecodingResult::F16(_) => {
                        return Err("Unsupported TIFF grayscale samples".into());
                    }
                }
                Some(rgba)
            }
            kind @ (ColorType::RGB(8 | 16)
            | ColorType::RGBA(8 | 16)
            | ColorType::CMYK(8)
            | ColorType::Multiband {
                bit_depth: 8,
                num_samples: 2,
            }) => {
                let channels = match kind {
                    ColorType::RGB(_) => 3,
                    ColorType::Multiband { .. } => 2,
                    _ => 4,
                };
                let planar = decoder
                    .find_tag_unsigned::<u16>(Tag::PlanarConfiguration)
                    .map_err(|e| e.to_string())?
                    == Some(2);
                let mut decoded = DecodingResult::U8(Vec::new());
                decoder
                    .read_image_to_buffer(&mut decoded)
                    .map_err(|e| e.to_string())?;
                let values = match decoded {
                    DecodingResult::U8(v) => v,
                    DecodingResult::U16(v) => v.into_iter().map(|v| (v >> 8) as u8).collect(),
                    _ => return Err("Invalid TIFF color samples".into()),
                };
                if values.len() != count as usize * channels {
                    return Err("Incomplete TIFF color planes".into());
                }
                let mut rgba = Vec::with_capacity(count as usize * 4);
                for p in 0..count as usize {
                    let sample = |channel: usize| -> Result<u8, String> {
                        let index = if planar {
                            channel * count as usize + p
                        } else {
                            p * channels + channel
                        };
                        values
                            .get(index)
                            .copied()
                            .ok_or_else(|| "Missing TIFF sample".into())
                    };
                    match kind {
                        ColorType::CMYK(_) => {
                            let black = u16::from(sample(3)?);
                            for channel in 0..3 {
                                rgba.push(
                                    ((u16::from(255 - sample(channel)?) * (255 - black) + 127)
                                        / 255) as u8,
                                );
                            }
                            rgba.push(255);
                        }
                        ColorType::Multiband { .. } => {
                            let gray = sample(0)?;
                            rgba.extend_from_slice(&[gray, gray, gray, sample(1)?]);
                        }
                        _ => rgba.extend_from_slice(&[
                            sample(0)?,
                            sample(1)?,
                            sample(2)?,
                            if channels == 4 { sample(3)? } else { 255 },
                        ]),
                    }
                }
                Some(rgba)
            }
            _ => None,
        }
    };
    let image = if let Some(samples) = samples {
        if samples.len() != count as usize * 4 {
            return Err("Invalid TIFF pixel count".into());
        }
        let pixels = RgbaImage::from_raw(width, height, samples).ok_or("Invalid TIFF pixels")?;
        let mut image = DynamicImage::ImageRgba8(pixels);
        image.apply_orientation(orientation);
        image
    } else {
        super::decode_limited(bytes, Some(image::ImageFormat::Tiff), remaining)?.0
    };
    if associated {
        let mut rgba = image.to_rgba8();
        for pixel in rgba.pixels_mut() {
            let [r, g, b, alpha] = &mut pixel.0;
            if *alpha != 0 {
                for channel in [r, g, b] {
                    *channel = (u16::from(*channel) * 255 / u16::from(*alpha)).min(255) as u8;
                }
            }
        }
        Ok(DynamicImage::ImageRgba8(rgba))
    } else {
        Ok(image)
    }
}

/// Change only the first IFD's inline photometric tag so the TIFF decoder can
/// decompress indices. Offsets, compression and the source palette stay intact.
fn palette_as_gray(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let little = match bytes.get(..2) {
        Some(b"II") => true,
        Some(b"MM") => false,
        _ => return Err("Invalid TIFF byte order".into()),
    };
    fn number(bytes: &[u8], start: usize, length: usize, little: bool) -> Result<u64, String> {
        let end = start.checked_add(length).ok_or("Invalid TIFF offset")?;
        let data = bytes.get(start..end).ok_or("Truncated TIFF directory")?;
        let mut value = 0u64;
        if little {
            for b in data.iter().rev() {
                value = (value << 8) | u64::from(*b);
            }
        } else {
            for b in data {
                value = (value << 8) | u64::from(*b);
            }
        }
        Ok(value)
    }
    let big = number(bytes, 2, 2, little)? == 43;
    let offset = if big {
        number(bytes, 8, 8, little)?
    } else {
        number(bytes, 4, 4, little)?
    };
    let offset = usize::try_from(offset).map_err(|_| "Invalid TIFF directory offset")?;
    let count_bytes = if big { 8 } else { 2 };
    let count = number(bytes, offset, count_bytes, little)?;
    let size = if big { 20 } else { 12 };
    if count > (bytes.len() / size) as u64 {
        return Err("Invalid TIFF directory count".into());
    }
    let mut at = offset
        .checked_add(count_bytes)
        .ok_or("Invalid TIFF directory offset")?;
    let mut output = bytes.to_vec();
    for _ in 0..count {
        let end = at.checked_add(size).ok_or("Invalid TIFF entry offset")?;
        let entry = output.get_mut(at..end).ok_or("Truncated TIFF directory")?;
        if number(entry, 0, 2, little)? == 262 {
            if number(entry, 2, 2, little)? != 3
                || number(entry, 4, if big { 8 } else { 4 }, little)? != 1
            {
                return Err("Invalid TIFF photometric tag".into());
            }
            let at = if big { 12 } else { 8 };
            let value = entry
                .get_mut(at..at + 2)
                .ok_or("Truncated TIFF photometric tag")?;
            value.copy_from_slice(&if little {
                1u16.to_le_bytes()
            } else {
                1u16.to_be_bytes()
            });
            return Ok(output);
        }
        at = end;
    }
    Err("Missing TIFF photometric tag".into())
}
