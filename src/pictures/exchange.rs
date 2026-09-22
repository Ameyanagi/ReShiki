//! Raster normalization for editable exchange, independent of Python/Pillow.
use super::{MAX_BYTES, Picture, decode_limited};
#[cfg(feature = "rdkit-reference")]
use crate::document::Document;
#[cfg(feature = "rdkit-reference")]
use base64::{Engine as _, engine::general_purpose::STANDARD};
use image::{DynamicImage, ImageFormat};
#[cfg(feature = "rdkit-reference")]
use serde::Deserialize;
#[cfg(feature = "rdkit-reference")]
use std::collections::HashMap;
mod tiff;

#[derive(Default)]
pub struct Budget {
    pub pixels: u64,
    pub bytes: usize,
}

/// Normalize the first frame and EXIF orientation before applying opacity.
pub fn import(
    bytes: &[u8],
    format: &str,
    opacity: f64,
    budget: &mut Budget,
) -> Result<Picture, String> {
    let format = match format {
        "PNG" => ImageFormat::Png,
        "JPEG" => ImageFormat::Jpeg,
        "TIFF" => ImageFormat::Tiff,
        "GIF" => ImageFormat::Gif,
        "BMP" => ImageFormat::Bmp,
        _ => return Err("Unsupported embedded picture format".into()),
    };
    if !opacity.is_finite() || !(0. ..=1.).contains(&opacity) {
        return Err("Invalid embedded picture opacity".into());
    }
    let remaining = 64_000_000u64.saturating_sub(budget.pixels);
    let pixels = if format == ImageFormat::Tiff {
        tiff::decode(bytes, remaining)?
    } else {
        decode_limited(bytes, Some(format), remaining)?.0
    };
    let mut pixels = match pixels {
        // Pillow clips integer grayscale samples when converting to RGBA;
        // rescaling the full 16-bit range would change existing drawings.
        DynamicImage::ImageLuma16(ref gray) => {
            image::RgbaImage::from_fn(gray.width(), gray.height(), |x, y| {
                let image::Luma([value]) = *gray.get_pixel(x, y);
                let value = value.min(255) as u8;
                image::Rgba([value, value, value, 255])
            })
        }
        DynamicImage::ImageRgb16(ref rgb) => {
            image::RgbaImage::from_fn(rgb.width(), rgb.height(), |x, y| {
                let image::Rgb([r, g, b]) = *rgb.get_pixel(x, y);
                image::Rgba([(r >> 8) as u8, (g >> 8) as u8, (b >> 8) as u8, 255])
            })
        }
        DynamicImage::ImageRgba16(ref rgba) => {
            image::RgbaImage::from_fn(rgba.width(), rgba.height(), |x, y| {
                let image::Rgba([r, g, b, a]) = *rgba.get_pixel(x, y);
                image::Rgba([
                    (r >> 8) as u8,
                    (g >> 8) as u8,
                    (b >> 8) as u8,
                    (a >> 8) as u8,
                ])
            })
        }
        DynamicImage::ImageLumaA16(ref gray) => {
            image::RgbaImage::from_fn(gray.width(), gray.height(), |x, y| {
                let image::LumaA([v, a]) = *gray.get_pixel(x, y);
                let v = (v >> 8) as u8;
                image::Rgba([v, v, v, (a >> 8) as u8])
            })
        }
        _ => pixels.to_rgba8(),
    };
    for pixel in pixels.pixels_mut() {
        let [_, _, _, alpha] = &mut pixel.0;
        *alpha = (f64::from(*alpha) * opacity).round_ties_even() as u8;
    }
    let picture = Picture::from_decoded(DynamicImage::ImageRgba8(pixels))?;
    if picture.png().len() > (64usize * 1024 * 1024).saturating_sub(budget.bytes) {
        return Err("A drawing can contain at most 64 MB of encoded pictures".into());
    }
    budget.pixels += u64::from(picture.width()) * u64::from(picture.height());
    budget.bytes += picture.png().len();
    Ok(picture)
}

/// The format represents rotation but not reflection; flip rows losslessly.
pub fn export(picture: &Picture, flip: bool) -> Result<Vec<u8>, String> {
    if !flip {
        return Ok(picture.png().to_vec());
    }
    let (pixels, _) = super::decode(picture.png())?;
    Ok(Picture::from_decoded(pixels.flipv())?.png().to_vec())
}

#[cfg(feature = "rdkit-reference")]
pub(crate) fn prepare_exports(document: &Document) -> Result<HashMap<u64, String>, String> {
    document
        .graphics
        .iter()
        .filter_map(|g| g.picture.as_ref().map(|p| (g, p)))
        .map(|(g, picture)| {
            let flip = g.axis_x.x * g.axis_y.y - g.axis_x.y * g.axis_y.x < 0.;
            Ok((g.id, STANDARD.encode(export(picture, flip)?)))
        })
        .collect()
}

/// Replace private deferred image payloads before deserializing a Document.
#[cfg(feature = "rdkit-reference")]
pub(crate) fn complete_imports(mut result: serde_json::Value) -> Result<serde_json::Value, String> {
    let Some(graphics) = result
        .get_mut("document")
        .and_then(|d| d.get_mut("graphics"))
        .and_then(|g| g.as_array_mut())
    else {
        return Ok(result);
    };
    let mut budget = Budget::default();
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Source {
        data: String,
        format: String,
        opacity: f64,
    }
    for graphic in graphics {
        let Some(source) = graphic.get("picture_source") else {
            continue;
        };
        if graphic.get("kind").and_then(|v| v.as_str()) != Some("picture") {
            return Err("Unexpected embedded picture payload".into());
        }
        let source: Source = serde_json::from_value(source.clone()).map_err(|e| e.to_string())?;
        if source.data.len() > MAX_BYTES.div_ceil(3) * 4 {
            return Err("Embedded picture exceeds 16 MB".into());
        }
        let picture = import(
            &STANDARD.decode(source.data).map_err(|e| e.to_string())?,
            &source.format,
            source.opacity,
            &mut budget,
        )?;
        let graphic = graphic.as_object_mut().ok_or("Invalid embedded picture")?;
        graphic.remove("picture_source");
        graphic.insert("picture".into(), STANDARD.encode(picture.png()).into());
    }
    Ok(result)
}
