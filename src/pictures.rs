//! Self-contained raster pictures. Decode untrusted inputs once with bounded
//! dimensions, retain portable PNG data, and share image storage across Undo.
use crate::{
    document::{Document, Point},
    graphics::{Graphic, GraphicKind},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use iced::advanced::graphics::core::Bytes;
use iced::widget::image::Handle;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{
    io::Cursor,
    sync::{Arc, OnceLock},
};

pub const MAX_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_PIXELS: u64 = 16_000_000;
const MAX_SIDE: u32 = 8192;
#[derive(Clone)]
pub struct Picture(Arc<Data>);
struct Data {
    png: Bytes,
    width: u32,
    height: u32,
    handle: Handle,
    flipped: OnceLock<Result<Handle, String>>,
}
impl std::fmt::Debug for Picture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Picture")
            .field("width", &self.0.width)
            .field("height", &self.0.height)
            .field("bytes", &self.0.png.len())
            .finish()
    }
}
impl PartialEq for Picture {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0) || self.0.png == other.0.png
    }
}
impl Serialize for Picture {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&STANDARD.encode(&self.0.png))
    }
}
impl<'de> Deserialize<'de> for Picture {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        if text.len() > MAX_BYTES.div_ceil(3) * 4 {
            return Err(serde::de::Error::custom("Picture exceeds 16 MB"));
        }
        let bytes = STANDARD.decode(text).map_err(serde::de::Error::custom)?;
        let (decoded, format) = decode(&bytes).map_err(serde::de::Error::custom)?;
        if format != ImageFormat::Png {
            return Err(serde::de::Error::custom("Stored pictures must be PNG"));
        }
        Self::from_decoded(decoded).map_err(serde::de::Error::custom)
    }
}
fn decode(bytes: &[u8]) -> Result<(DynamicImage, ImageFormat), String> {
    if bytes.len() > MAX_BYTES {
        return Err("Picture exceeds 16 MB".into());
    }
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let format = reader
        .format()
        .ok_or("Choose a PNG, JPEG, TIFF or WebP picture")?;
    if !matches!(
        format,
        ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::Tiff | ImageFormat::WebP
    ) {
        return Err("Choose a PNG, JPEG, TIFF or WebP picture".into());
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().map_err(|e| e.to_string())?;
    let (width, height) = decoder.dimensions();
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err("Pictures can contain at most 16 million pixels".into());
    }
    let orientation = decoder.orientation().map_err(|e| e.to_string())?;
    let mut image = DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
    image.apply_orientation(orientation);
    Ok((image, format))
}
impl Picture {
    fn stored(bytes: Vec<u8>, width: u32, height: u32) -> Self {
        let png = Bytes::from(bytes);
        let handle = Handle::from_bytes(png.clone());
        Self(Arc::new(Data {
            png,
            width,
            height,
            handle,
            flipped: OnceLock::new(),
        }))
    }
    pub fn import(bytes: &[u8]) -> Result<Self, String> {
        let (image, _) = decode(bytes)?;
        Self::from_decoded(image)
    }
    fn from_decoded(image: DynamicImage) -> Result<Self, String> {
        let (width, height) = (image.width(), image.height());
        let mut png = Cursor::new(Vec::new());
        image
            .to_rgba8()
            .write_to(&mut png, ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        let bytes = png.into_inner();
        if bytes.len() > MAX_BYTES {
            return Err("Decoded picture exceeds the 16 MB storage limit".into());
        }
        Ok(Self::stored(bytes, width, height))
    }
    pub fn width(&self) -> u32 {
        self.0.width
    }
    pub fn height(&self) -> u32 {
        self.0.height
    }
    pub fn png(&self) -> &[u8] {
        &self.0.png
    }
    pub fn handle(&self, flip: bool) -> Option<Handle> {
        if !flip {
            return Some(self.0.handle.clone());
        }
        self.0
            .flipped
            .get_or_init(|| {
                let (pixels, _) = decode(&self.0.png)?;
                Ok(Handle::from_rgba(
                    self.width(),
                    self.height(),
                    pixels.flipv().to_rgba8().into_raw(),
                ))
            })
            .as_ref()
            .ok()
            .cloned()
    }
    pub fn graphic(&self, id: u64, center: Point) -> Graphic {
        // Start at 300 dpi, capped at 100 mm wide/tall; handles change size later.
        let scale = (crate::style::DEFAULT.world(72. / 300.)).min(
            crate::style::DEFAULT.world(100. * 72. / 25.4)
                / (self.width().max(self.height()) as f32),
        );
        let width = self.width() as f32 * scale;
        let height = self.height() as f32 * scale;
        let mut graphic = Graphic::dragged(
            id,
            GraphicKind::Picture,
            center.offset(-width / 2., -height / 2.),
            center.offset(width / 2., height / 2.),
            Default::default(),
            Default::default(),
            false,
        );
        graphic.origin = center.offset(-width / 2., -height / 2.);
        graphic.axis_x = Point::new(width, 0.);
        graphic.axis_y = Point::new(0., height);
        graphic.picture = Some(self.clone());
        graphic
    }
    pub fn document(&self) -> Document {
        let mut doc = Document::default();
        doc.graphics.push(self.graphic(1, Point::default()));
        doc
    }
}
