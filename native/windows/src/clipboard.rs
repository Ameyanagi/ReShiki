use super::{Owner, Result};
use anyhow::Context;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io::Cursor, time::Duration};
use windows::{
    Win32::{
        Foundation::*,
        System::{DataExchange::*, Memory::*},
    },
    core::PCWSTR,
};

const LIMIT: usize = 64 * 1024 * 1024;
const UNICODE: u32 = 13;
const DIB: u32 = 8;
const DIB_V5: u32 = 17;

#[derive(Deserialize, Serialize)]
struct Representation {
    #[serde(rename = "type")]
    kind: String,
    data: String,
}
#[derive(Deserialize)]
struct Request {
    operation: String,
    #[serde(default)]
    representations: Vec<Representation>,
}
#[derive(Serialize)]
struct Packet {
    representations: Vec<Representation>,
}

pub(super) fn format(name: &str) -> Result<u32> {
    let wide: Vec<_> = name.encode_utf16().chain(Some(0)).collect();
    // SAFETY: a bounded NUL-terminated string is alive for the call.
    let id = unsafe { RegisterClipboardFormatW(PCWSTR(wide.as_ptr())) };
    if id == 0 {
        Err(windows::core::Error::from_win32().into())
    } else {
        Ok(id)
    }
}
fn mapped(kind: &str) -> &str {
    match kind {
        "com.revvity.chemdraw.cdx-clipboard"
        | "com.perkinelmer.chemdraw.cdx-clipboard"
        | "com.cambridgesoft.cdx" => "ChemDraw Interchange Format",
        "public.png" => "PNG",
        "public.svg-image" => "image/svg+xml",
        "com.adobe.pdf" => "PDF",
        _ => kind,
    }
}
struct Open;
impl Open {
    fn new(owner: HWND) -> Result<Self> {
        for _ in 0..20 {
            // SAFETY: owner is a live window created on this thread.
            if unsafe { OpenClipboard(owner) }.is_ok() {
                return Ok(Self);
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        Err(anyhow::anyhow!(
            "The clipboard is busy in another application. Try again."
        ))
    }
}
impl Drop for Open {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseClipboard();
        }
    }
}
pub(super) struct Memory(pub(super) HGLOBAL);
impl Memory {
    pub(super) fn new(bytes: &[u8]) -> Result<Self> {
        if bytes.is_empty() || bytes.len() > LIMIT {
            return Err(anyhow::anyhow!("Invalid clipboard data size"));
        }
        // SAFETY: allocation size and source length match; RAII frees on any error.
        unsafe {
            let memory = Self(GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, bytes.len())?);
            let pointer = GlobalLock(memory.0);
            if pointer.is_null() {
                return Err(windows::core::Error::from_win32().into());
            }
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), pointer.cast(), bytes.len());
            let _ = GlobalUnlock(memory.0);
            Ok(memory)
        }
    }
}
impl Drop for Memory {
    fn drop(&mut self) {
        unsafe {
            let _ = GlobalFree(self.0);
        }
    }
}

fn read(id: u32) -> Result<Vec<u8>> {
    // SAFETY: called only while Open owns the clipboard. GlobalSize bounds the
    // slice, which is copied before unlocking; the clipboard retains ownership.
    unsafe {
        let memory = HGLOBAL(GetClipboardData(id)?.0);
        let size = GlobalSize(memory);
        if size == 0 || size > LIMIT {
            return Err(anyhow::anyhow!("Clipboard data exceeds 64 MB"));
        }
        let pointer = GlobalLock(memory);
        if pointer.is_null() {
            return Err(windows::core::Error::from_win32().into());
        }
        let data = std::slice::from_raw_parts(pointer.cast::<u8>(), size).to_vec();
        let _ = GlobalUnlock(memory);
        Ok(data)
    }
}
fn decode(data: &str) -> Result<Vec<u8>> {
    if data.len() > LIMIT.div_ceil(3) * 4 {
        return Err(anyhow::anyhow!("Clipboard data exceeds 64 MB"));
    }
    let bytes = STANDARD.decode(data)?;
    if bytes.is_empty() || bytes.len() > LIMIT {
        return Err(anyhow::anyhow!("Invalid clipboard data size"));
    }
    Ok(bytes)
}
pub(super) fn bitmap(data: &[u8]) -> Result<image::RgbaImage> {
    let mut reader = image::ImageReader::new(Cursor::new(data)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(320_000_000);
    limits.max_image_width = Some(32768);
    limits.max_image_height = Some(32768);
    reader.limits(limits);
    let image = reader.decode()?.into_rgba8();
    if u64::from(image.width()) * u64::from(image.height()) > 80_000_000 {
        return Err(anyhow::anyhow!("Clipboard picture is too large"));
    }
    Ok(image)
}
pub(super) fn to_dib(data: &[u8]) -> Result<Vec<u8>> {
    let image = bitmap(data)?;
    let (w, h) = image.dimensions();
    let stride = (w as usize * 3 + 3) & !3;
    let size = 40 + stride * h as usize;
    if size > LIMIT {
        return Err(anyhow::anyhow!(
            "Clipboard bitmap exceeds 64 MB; use file export for this drawing"
        ));
    }
    let mut result = vec![0; size];
    for (offset, value) in [(0, 40u32), (4, w), (8, h), (20, (size - 40) as u32)] {
        result[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    result[12..14].copy_from_slice(&1u16.to_le_bytes());
    result[14..16].copy_from_slice(&24u16.to_le_bytes());
    let dimensions = png::Decoder::new(Cursor::new(data))
        .read_info()
        .ok()
        .and_then(|r| r.info().pixel_dims);
    let (x, y) = dimensions
        .filter(|d| d.unit == png::Unit::Meter)
        .map(|d| (d.xppu, d.yppu))
        .unwrap_or((3780, 3780));
    result[24..28].copy_from_slice(&x.to_le_bytes());
    result[28..32].copy_from_slice(&y.to_le_bytes());
    for (x, y, p) in image.enumerate_pixels() {
        let at = 40 + (h - y - 1) as usize * stride + x as usize * 3;
        for (channel, source) in [2, 1, 0].into_iter().enumerate() {
            result[at + channel] =
                ((u32::from(p[source]) * u32::from(p[3]) + 255 * (255 - u32::from(p[3])) + 127)
                    / 255) as u8;
        }
    }
    Ok(result)
}
fn from_dib(data: &[u8]) -> Result<Vec<u8>> {
    if data.len() < 40 {
        return Err(anyhow::anyhow!("Truncated clipboard bitmap"));
    }
    let word = |at: usize| -> Result<u32> {
        let end = at.checked_add(4).context("Invalid bitmap header offset")?;
        let bytes = data.get(at..end).context("Truncated clipboard bitmap")?;
        Ok(u32::from_le_bytes(bytes.try_into()?))
    };
    let header = word(0)? as usize;
    let bits = u16::from_le_bytes([data[14], data[15]]);
    let compression = word(16)?;
    if header < 40
        || header > data.len()
        || ![0, 3, 6].contains(&compression)
        || ![1, 4, 8, 16, 24, 32].contains(&bits)
    {
        return Err(anyhow::anyhow!("Unsupported clipboard bitmap"));
    }
    let colors = word(32)? as usize;
    let colors = if colors == 0 && bits <= 8 {
        1usize << bits
    } else {
        colors
    };
    if colors > 256 {
        return Err(anyhow::anyhow!("Unsupported clipboard palette"));
    }
    let offset = header
        + colors * 4
        + if header == 40 {
            match compression {
                3 => 12,
                6 => 16,
                _ => 0,
            }
        } else {
            0
        };
    if offset > data.len() {
        return Err(anyhow::anyhow!("Truncated clipboard bitmap pixels"));
    }
    let mut bmp = Vec::with_capacity(data.len() + 14);
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&((data.len() + 14) as u32).to_le_bytes());
    bmp.extend_from_slice(&[0; 4]);
    bmp.extend_from_slice(&((offset + 14) as u32).to_le_bytes());
    bmp.extend_from_slice(data);
    let image = bitmap(&bmp)?;
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, image.width(), image.height());
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let (x, y) = (word(24)? as i32, word(28)? as i32);
    if x > 0 && y > 0 {
        encoder.set_pixel_dims(Some(png::PixelDimensions {
            xppu: x as u32,
            yppu: y as u32,
            unit: png::Unit::Meter,
        }));
    }
    encoder.write_header()?.write_image_data(image.as_raw())?;
    Ok(bytes)
}
fn write(representations: Vec<Representation>, embedded: bool) -> Result<()> {
    if representations.is_empty() || representations.len() > 32 {
        return Err(anyhow::anyhow!("No clipboard representations"));
    }
    let mut formats = BTreeMap::new();
    for rep in representations {
        if rep.kind.is_empty() || rep.kind.len() > 128 || rep.kind.contains('\0') {
            return Err(anyhow::anyhow!("Invalid clipboard format"));
        }
        let bytes = decode(&rep.data)?;
        if rep.kind == "public.png" {
            formats.insert(DIB, to_dib(&bytes)?);
        }
        let (id, bytes) = if rep.kind == "public.utf8-plain-text" {
            (
                UNICODE,
                std::str::from_utf8(&bytes)?
                    .encode_utf16()
                    .chain(Some(0))
                    .flat_map(u16::to_le_bytes)
                    .collect(),
            )
        } else {
            (format(mapped(&rep.kind))?, bytes)
        };
        formats.entry(id).or_insert(bytes);
    }
    if formats.values().map(Vec::len).sum::<usize>() > LIMIT {
        return Err(anyhow::anyhow!(
            "Combined clipboard representations exceed 64 MB"
        ));
    }
    if embedded && super::ole::enabled() {
        return super::ole::copy(formats);
    }
    let allocated: Result<Vec<_>> = formats
        .into_iter()
        .map(|(id, bytes)| Ok((id, Memory::new(&bytes)?)))
        .collect();
    let allocated = allocated?;
    let owner = Owner::new()?;
    let _open = Open::new(owner.0)?;
    // SAFETY: own the clipboard and all buffers. Windows takes ownership only
    // after each successful SetClipboardData; RAII frees untransferred buffers.
    unsafe {
        EmptyClipboard()?;
        for (id, memory) in allocated {
            SetClipboardData(id, HANDLE(memory.0.0))?;
            std::mem::forget(memory);
        }
    }
    Ok(())
}
fn read_packet(picture_only: bool) -> Result<Packet> {
    if let Some(document) = super::ole::read_own(picture_only)? {
        return Ok(Packet {
            representations: vec![Representation {
                kind: if picture_only {
                    "public.png"
                } else {
                    "dev.reshiki.drawing"
                }
                .into(),
                data: STANDARD.encode(document),
            }],
        });
    }
    let owner = Owner::new()?;
    let _open = Open::new(owner.0)?;
    let mut formats = Vec::new();
    if !picture_only {
        formats.extend([
            ("dev.reshiki.drawing", "dev.reshiki.drawing"),
            ("dev.moruno.drawing", "dev.moruno.drawing"),
            (
                "ChemDraw Interchange Format",
                "com.revvity.chemdraw.cdx-clipboard",
            ),
            ("ChemDraw XML", "public.cdxml"),
            ("chemical/x-cdxml", "public.cdxml"),
            ("MDLCT", "com.mdli.molfile"),
            ("SMILES", "org.opensmiles.smiles"),
        ]);
    }
    formats.extend([
        ("PNG", "public.png"),
        ("image/png", "public.png"),
        ("JFIF", "public.jpeg"),
        ("TIFF", "public.tiff"),
        ("WebP", "org.webmproject.webp"),
    ]);
    let packet = |kind: &str, bytes: Vec<u8>| Packet {
        representations: vec![Representation {
            kind: kind.into(),
            data: STANDARD.encode(bytes),
        }],
    };
    for (name, kind) in formats {
        let id = format(name)?;
        // SAFETY: read-only format query, while holding the clipboard.
        if unsafe { IsClipboardFormatAvailable(id) }.is_ok() {
            let mut bytes = read(id)?;
            if kind.starts_with("dev.") || kind == "public.cdxml" || kind == "org.opensmiles.smiles"
            {
                while bytes.last() == Some(&0) {
                    bytes.pop();
                }
            }
            return Ok(packet(kind, bytes));
        }
    }
    for id in [DIB_V5, DIB] {
        if unsafe { IsClipboardFormatAvailable(id) }.is_ok() {
            return Ok(packet("public.png", from_dib(&read(id)?)?));
        }
    }
    if !picture_only && unsafe { IsClipboardFormatAvailable(UNICODE) }.is_ok() {
        let bytes = read(UNICODE)?;
        if bytes.len() % 2 != 0 {
            return Err(anyhow::anyhow!("Invalid clipboard Unicode text"));
        }
        let utf16: Vec<_> = bytes
            .chunks_exact(2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .take_while(|c| *c != 0)
            .collect();
        return Ok(packet(
            "public.utf8-plain-text",
            String::from_utf16(&utf16)?.into_bytes(),
        ));
    }
    Ok(Packet {
        representations: vec![],
    })
}
pub(super) fn invoke(bytes: &[u8]) -> Result<Vec<u8>> {
    if bytes.len() > LIMIT * 2 {
        return Err(anyhow::anyhow!("Clipboard request is too large"));
    }
    let request: Request = serde_json::from_slice(bytes)?;
    let result = match request.operation.as_str() {
        "write" | "write_embedded" => {
            write(
                request.representations,
                request.operation == "write_embedded",
            )?;
            Packet {
                representations: vec![],
            }
        }
        "read" => read_packet(false)?,
        "read_picture" => read_packet(true)?,
        _ => return Err(anyhow::anyhow!("Unknown clipboard operation")),
    };
    Ok(serde_json::to_vec(&result)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_clipboard_unicode_native_priority_and_invalid_write() {
        let rep = |kind: &str, bytes: &[u8]| Representation {
            kind: kind.into(),
            data: STANDARD.encode(bytes),
        };
        let text = "日本語 CCO".as_bytes();
        write(
            vec![
                rep("public.utf8-plain-text", text),
                rep("dev.reshiki.drawing", b"{\"version\":15}"),
            ],
            false,
        )
        .unwrap();
        let packet = read_packet(false).unwrap();
        assert_eq!(packet.representations[0].kind, "dev.reshiki.drawing");
        write(vec![rep("public.utf8-plain-text", text)], false).unwrap();
        assert_eq!(
            STANDARD
                .decode(&read_packet(false).unwrap().representations[0].data)
                .unwrap(),
            text
        );
        assert!(
            write(
                vec![Representation {
                    kind: "public.png".into(),
                    data: "!".into()
                }],
                false
            )
            .is_err()
        );
        assert_eq!(
            STANDARD
                .decode(&read_packet(false).unwrap().representations[0].data)
                .unwrap(),
            text
        );
        write(vec![rep("com.cambridgesoft.cdx", b"VjCD0100\0")], false).unwrap();
        assert_eq!(
            read_packet(false).unwrap().representations[0].kind,
            "com.revvity.chemdraw.cdx-clipboard"
        );
    }
    #[test]
    fn dib_roundtrip_keeps_pixels_and_resolution() {
        let image = image::RgbaImage::from_pixel(20, 10, image::Rgba([30, 70, 150, 255]));
        let mut png = Vec::new();
        let mut encoder = png::Encoder::new(&mut png, 20, 10);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_pixel_dims(Some(png::PixelDimensions {
            xppu: 11811,
            yppu: 11811,
            unit: png::Unit::Meter,
        }));
        encoder
            .write_header()
            .unwrap()
            .write_image_data(image.as_raw())
            .unwrap();
        let result = from_dib(&to_dib(&png).unwrap()).unwrap();
        assert_eq!(bitmap(&result).unwrap(), image);
        assert_eq!(
            png::Decoder::new(Cursor::new(result))
                .read_info()
                .unwrap()
                .info()
                .pixel_dims
                .unwrap()
                .xppu,
            11811
        );
        assert!(from_dib(&[0; 40]).is_err());
        for length in 0..40 {
            assert!(from_dib(&vec![0; length]).is_err());
        }
    }
}
