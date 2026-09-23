//! Self-contained raster pictures. Decode untrusted inputs once with bounded
//! dimensions, retain portable PNG data, and share image storage across Undo.
pub mod exchange;
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
    decode_limited(bytes, None, MAX_PIXELS)
}
fn decode_limited(
    bytes: &[u8],
    expected: Option<ImageFormat>,
    remaining_pixels: u64,
) -> Result<(DynamicImage, ImageFormat), String> {
    if bytes.len() > MAX_BYTES {
        return Err("Picture exceeds 16 MB".into());
    }
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let format = reader
        .format()
        .ok_or("Choose a PNG, JPEG, TIFF or WebP picture")?;
    if let Some(expected) = expected {
        if format != expected {
            return Err("Embedded picture bytes do not match their declared format".into());
        }
    } else if !matches!(
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
    if u64::from(width) * u64::from(height) > remaining_pixels {
        return Err("A drawing can contain at most 64 million picture pixels".into());
    }
    let orientation = decoder.orientation().map_err(|e| e.to_string())?;
    let mut image = DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
    image.apply_orientation(orientation);
    Ok((image, format))
}
impl Picture {
    pub fn open(path: &std::path::Path) -> Result<Self, String> {
        use std::io::Read as _;
        let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let metadata = file.metadata().map_err(|e| e.to_string())?;
        if !metadata.is_file() || metadata.len() > MAX_BYTES as u64 {
            return Err("Choose a picture file no larger than 16 MB".into());
        }
        let mut bytes = Vec::new();
        file.take(MAX_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        Self::import(&bytes)
    }
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
    /// An opaque copy for consumers that may discard alpha. Composite before
    /// encoding RGB so transparent black backgrounds cannot swallow line art.
    /// The stored picture (including its transparency) stays unchanged.
    pub fn png_on_white(&self) -> Result<Vec<u8>, String> {
        let (decoded, _) = decode(self.png())?;
        let rgba = decoded.into_rgba8();
        let rgb = image::RgbImage::from_fn(rgba.width(), rgba.height(), |x, y| {
            let [r, g, b, a] = rgba.get_pixel(x, y).0;
            let alpha = u32::from(a);
            image::Rgb([r, g, b].map(|channel| {
                ((u32::from(channel) * alpha + 255 * (255 - alpha) + 127) / 255) as u8
            }))
        });
        let mut png = Cursor::new(Vec::new());
        rgb.write_to(&mut png, ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        let bytes = png.into_inner();
        if bytes.len() > MAX_BYTES {
            return Err("Opaque picture exceeds the 16 MB storage limit".into());
        }
        Ok(bytes)
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

/// Resize around the picture's center, retaining rotation and reflection.
pub fn resize(graphic: &mut Graphic, width: f32, height: f32) -> Result<(), String> {
    graphic.validate()?;
    if graphic.picture.is_none()
        || !(0.01..=1_000_000.).contains(&width)
        || !(0.01..=1_000_000.).contains(&height)
    {
        return Err("Picture dimensions must be positive and finite".into());
    }
    let center = graphic.origin.offset(
        (graphic.axis_x.x + graphic.axis_y.x) / 2.,
        (graphic.axis_x.y + graphic.axis_y.y) / 2.,
    );
    let x = width / graphic.axis_x.distance(Point::default());
    let y = height / graphic.axis_y.distance(Point::default());
    let mut next = graphic.clone();
    next.axis_x = Point::new(graphic.axis_x.x * x, graphic.axis_x.y * x);
    next.axis_y = Point::new(graphic.axis_y.x * y, graphic.axis_y.y * y);
    next.origin = center.offset(
        -(next.axis_x.x + next.axis_y.x) / 2.,
        -(next.axis_x.y + next.axis_y.y) / 2.,
    );
    next.validate()?;
    *graphic = next;
    Ok(())
}

/// PNG clipboard images may carry publication resolution. Honor it instead of
/// treating a high-resolution drawing as a low-resolution photograph.
pub fn clipboard_document(bytes: &[u8]) -> Result<Document, String> {
    let picture = Picture::import(bytes)?;
    let mut doc = picture.document();
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        let reader = png::Decoder::new(Cursor::new(bytes))
            .read_info()
            .map_err(|e| e.to_string())?;
        if let Some(dims) = reader
            .info()
            .pixel_dims
            .filter(|d| d.unit == png::Unit::Meter && d.xppu > 0 && d.yppu > 0)
            && let Some(g) = doc.graphics.first_mut()
        {
            let width = crate::style::DEFAULT
                .world(picture.width() as f32 * 72. / (dims.xppu as f32 * 0.0254));
            let height = crate::style::DEFAULT
                .world(picture.height() as f32 * 72. / (dims.yppu as f32 * 0.0254));
            resize(g, width, height)?;
        }
    }
    doc.validate()?;
    Ok(doc)
}
