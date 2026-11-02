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

#[path = "../../../src/clipboard/priority.rs"]
mod priority;

const LIMIT: usize = 64 * 1024 * 1024;
const UNICODE: u32 = 13;
const DIB: u32 = 8;
const DIB_V5: u32 = 17;
const CDX_KINDS: &[&str] = &[
    "com.revvity.chemdraw.cdx-clipboard",
    "com.perkinelmer.chemdraw.cdx-clipboard",
    "com.cambridgesoft.cdx",
    "com.revvity.cdx",
    "com.perkinelmer.cdx",
    "chemical/x-cdx",
    "application/x-chemdraw",
];
const CDXML_KINDS: &[&str] = &[
    "com.revvity.cdxml",
    "com.perkinelmer.cdxml",
    "com.cambridgesoft.cdxml",
    "com.perkinelmer.chemdraw.cdxml",
    "chemical/x-cdxml",
    "public.cdxml",
];

#[cfg(test)]
pub(super) static CLIPBOARD_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[derive(Deserialize, Serialize)]
struct Representation {
    #[serde(rename = "type")]
    kind: String,
    data: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    read_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    embedded: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source: Option<String>,
}
#[derive(Deserialize)]
struct Request {
    operation: String,
    #[serde(default)]
    representations: Vec<Representation>,
    #[serde(default)]
    formats: Vec<String>,
    #[serde(default)]
    skip_embedded: bool,
    #[serde(default)]
    skip_sources: Vec<String>,
    #[serde(default)]
    generation: Option<String>,
}
#[derive(Serialize)]
struct Packet {
    representations: Vec<Representation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation: Option<String>,
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
    if CDX_KINDS.contains(&kind) {
        return "ChemDraw Interchange Format";
    }
    match kind {
        "public.png" => "PNG",
        "public.svg-image" => "image/svg+xml",
        "public.html" => "HTML Format",
        "public.rtf" => "Rich Text Format",
        "com.adobe.pdf" => "PDF",
        // MOL text is not MDLCT's length-prefixed binary line encoding.
        "com.mdli.molfile" => "chemical/x-mdl-molfile",
        "org.opensmiles.smiles" => "SMILES",
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

fn cf_html(fragment: &str) -> Vec<u8> {
    const PREFIX: &str = "<html><body><!--StartFragment-->";
    const SUFFIX: &str = "<!--EndFragment--></body></html>";
    let header = |start_html, end_html, start_fragment, end_fragment| {
        format!(
            "Version:0.9\r\nStartHTML:{start_html:010}\r\nEndHTML:{end_html:010}\r\n\
             StartFragment:{start_fragment:010}\r\nEndFragment:{end_fragment:010}\r\n"
        )
    };
    // Fixed-width ASCII offsets count UTF-8 bytes from the start of the header.
    let start_html = header(0, 0, 0, 0).len();
    let start_fragment = start_html + PREFIX.len();
    let end_fragment = start_fragment + fragment.len();
    let end_html = end_fragment + SUFFIX.len();
    let mut bytes = header(start_html, end_html, start_fragment, end_fragment).into_bytes();
    bytes.extend_from_slice(PREFIX.as_bytes());
    bytes.extend_from_slice(fragment.as_bytes());
    bytes.extend_from_slice(SUFFIX.as_bytes());
    bytes.push(0);
    bytes
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
fn prepare_formats(
    representations: Vec<Representation>,
    embedded: bool,
) -> Result<BTreeMap<u32, Vec<u8>>> {
    if representations.is_empty() || representations.len() > 32 {
        return Err(anyhow::anyhow!("No clipboard representations"));
    }
    let mut formats = BTreeMap::new();
    for rep in representations {
        if rep.read_error.is_some() || rep.embedded.is_some() || rep.source.is_some() {
            return Err(anyhow::anyhow!(
                "Read metadata is not valid for clipboard writes"
            ));
        }
        if rep.kind.is_empty() || rep.kind.len() > 128 || rep.kind.contains('\0') {
            return Err(anyhow::anyhow!("Invalid clipboard format"));
        }
        let bytes = decode(&rep.data)?;
        // OLE carries its own cached PNG/EMF preview. It removes standalone
        // bitmaps so Word keeps the object editable; don't allocate a DIB
        // only to discard it (or reject Copy because that unused DIB is huge).
        if rep.kind == "public.png" && !embedded {
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
        } else if rep.kind == "public.html" {
            (
                format(mapped(&rep.kind))?,
                cf_html(std::str::from_utf8(&bytes)?),
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
    Ok(formats)
}
fn write(representations: Vec<Representation>, embedded: bool) -> Result<()> {
    let embedded = embedded && super::ole::enabled();
    let formats = prepare_formats(representations, embedded)?;
    if embedded {
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
fn registered_formats(kind: &str) -> Vec<&str> {
    let aliases: &[&str] = if CDX_KINDS.contains(&kind) {
        &["ChemDraw Interchange Format"]
    } else if CDXML_KINDS.contains(&kind) {
        &["ChemDraw XML", "chemical/x-cdxml"]
    } else {
        match kind {
            "com.mdli.molfile" => &["chemical/x-mdl-molfile", "MDLCT"],
            "org.opensmiles.smiles" => &["SMILES"],
            "public.png" => &["PNG", "image/png"],
            "public.tiff" => &["TIFF", "image/tiff"],
            "public.jpeg" => &["JFIF", "JPEG", "image/jpeg"],
            "org.webmproject.webp" | "public.webp" => &["WebP", "image/webp"],
            "com.adobe.pdf" => &["PDF", "application/pdf"],
            "public.svg-image" => &["image/svg+xml"],
            "public.utf8-plain-text" => return vec![],
            _ => &[],
        }
    };
    aliases
        .iter()
        .copied()
        .chain(std::iter::once(kind))
        .collect()
}

fn unicode_text(bytes: &[u8]) -> Result<Vec<u8>> {
    let (pairs, remainder) = bytes.as_chunks::<2>();
    if !remainder.is_empty() {
        return Err(anyhow::anyhow!("Invalid clipboard Unicode text"));
    }
    let utf16: Vec<_> = pairs
        .iter()
        .map(|&pair| u16::from_le_bytes(pair))
        .take_while(|c| *c != 0)
        .collect();
    Ok(String::from_utf16(&utf16)?.into_bytes())
}

fn native_sources(kind: &str) -> Vec<&str> {
    if kind == "public.utf8-plain-text" {
        return vec!["CF_UNICODETEXT"];
    }
    let mut sources = registered_formats(kind);
    if kind == "public.png" {
        sources.extend(["CF_DIBV5", "CF_DIB"]);
    }
    sources
}

fn source_id(source: &str) -> Result<u32> {
    match source {
        "CF_UNICODETEXT" => Ok(UNICODE),
        "CF_DIBV5" => Ok(DIB_V5),
        "CF_DIB" => Ok(DIB),
        _ => format(source),
    }
}

struct ReadCandidate {
    source: String,
    bytes: Result<Vec<u8>>,
}

fn read_kind_with(
    kind: &str,
    skipped: &[String],
    mut available: impl FnMut(&str) -> Result<bool>,
    mut acquire: impl FnMut(&str) -> Result<Vec<u8>>,
) -> Result<Option<ReadCandidate>> {
    for source in native_sources(kind) {
        if skipped.iter().any(|skipped| skipped == source) || !available(source)? {
            continue;
        }
        let bytes = acquire(source).and_then(|mut bytes| match source {
            "CF_UNICODETEXT" => unicode_text(&bytes),
            "CF_DIBV5" | "CF_DIB" => from_dib(&bytes),
            _ => {
                if kind.starts_with("dev.")
                    || CDXML_KINDS.contains(&kind)
                    || kind == "org.opensmiles.smiles"
                {
                    while bytes.last() == Some(&0) {
                        bytes.pop();
                    }
                }
                Ok(bytes)
            }
        });
        return Ok(Some(ReadCandidate {
            source: source.into(),
            bytes,
        }));
    }
    Ok(None)
}

fn read_kind(kind: &str, skipped: &[String]) -> Result<Option<ReadCandidate>> {
    read_kind_with(
        kind,
        skipped,
        |source| {
            let id = source_id(source)?;
            // SAFETY: read-only format query, while holding the clipboard.
            Ok(unsafe { IsClipboardFormatAvailable(id) }.is_ok())
        },
        |source| read(source_id(source)?),
    )
}

fn read_result(
    kind: &str,
    result: Result<Vec<u8>>,
    embedded: Option<bool>,
    source: Option<String>,
) -> Packet {
    let result = result.and_then(|bytes| {
        if bytes.len() > LIMIT {
            Err(anyhow::anyhow!("Clipboard data exceeds 64 MB"))
        } else {
            Ok(bytes)
        }
    });
    let (data, read_error) = match result {
        Ok(bytes) => (STANDARD.encode(bytes), None),
        Err(error) => (
            String::new(),
            Some(error.to_string().chars().take(1000).collect()),
        ),
    };
    Packet {
        representations: vec![Representation {
            kind: kind.into(),
            data,
            read_error,
            embedded,
            source,
        }],
        generation: None,
    }
}

fn read_requested(
    formats: &[&str],
    mut acquire: impl FnMut(&str) -> Result<Option<ReadCandidate>>,
) -> Packet {
    for &kind in formats {
        match acquire(kind) {
            Ok(None) => continue,
            Ok(Some(candidate)) => {
                return read_result(kind, candidate.bytes, None, Some(candidate.source));
            }
            Err(error) => return read_result(kind, Err(error), None, None),
        }
    }
    Packet {
        representations: vec![],
        generation: None,
    }
}

fn read_packet_with(
    formats: &[&str],
    picture_only: bool,
    skip_embedded: bool,
    embedded: impl FnOnce(bool) -> Result<Option<super::ole::EmbeddedData>>,
    ordinary: impl FnOnce(&[&str]) -> Result<Packet>,
) -> Result<Packet> {
    let Some(&first) = formats.first() else {
        return Ok(Packet {
            representations: vec![],
            generation: None,
        });
    };
    if !skip_embedded {
        match embedded(picture_only) {
            Ok(Some(document)) => {
                let (kind, bytes) = document.into_parts();
                if formats.contains(&kind) {
                    return Ok(read_result(kind, Ok(bytes), Some(true), None));
                }
            }
            Err(error) => return Ok(read_result(first, Err(error), Some(true), None)),
            Ok(None) => {}
        }
    }
    ordinary(formats)
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct ClipboardGeneration {
    owner: usize,
    formats: i32,
}

fn clipboard_generation() -> ClipboardGeneration {
    // SAFETY: read-only metadata queries require no open clipboard or pointers.
    unsafe {
        ClipboardGeneration {
            // A null HWND is a valid ownerless clipboard state. The Windows
            // binding reports it as Err; keep zero for ownership comparisons.
            owner: GetClipboardOwner().ok().map_or(0, |owner| owner.0 as usize),
            formats: CountClipboardFormats(),
        }
    }
}

fn read_generation_with(
    expected: Option<&str>,
    mut generation: impl FnMut() -> ClipboardGeneration,
    acquire: impl FnOnce() -> Result<Packet>,
) -> Result<Packet> {
    priority::validate_generation(expected).map_err(anyhow::Error::msg)?;
    let initial = generation();
    let token = format!("windows:{}:{}", initial.owner, initial.formats);
    if expected.is_some_and(|expected| expected != token) {
        return Err(anyhow::Error::msg(priority::CLIPBOARD_CHANGED));
    }
    let result = acquire();
    // Delayed rendering fills an advertised format without changing its owner
    // or format count, but can advance GetClipboardSequenceNumber. Never use
    // that sequence to abort a paste. A same-owner re-copy with the same format
    // count is indistinguishable (even if the formats differ), as with an X11
    // owner that lacks TIMESTAMP. OLE can release the clipboard between reads;
    // an owner/count change must abort rather than fall through with stale
    // exclusions, including when acquisition failed.
    if generation() != initial {
        return Err(anyhow::Error::msg(priority::CLIPBOARD_CHANGED));
    }
    let mut packet = result?;
    packet.generation = Some(token);
    Ok(packet)
}

fn read_packet(
    picture_only: bool,
    requested: &[String],
    skip_embedded: bool,
    skip_sources: &[String],
    generation: Option<&str>,
) -> Result<Packet> {
    read_generation_with(generation, clipboard_generation, || {
        let formats =
            priority::read_formats(requested, picture_only).map_err(anyhow::Error::msg)?;
        priority::validate_skip_sources(skip_sources).map_err(anyhow::Error::msg)?;
        read_packet_with(
            &formats,
            picture_only,
            skip_embedded,
            super::ole::read_embedded,
            |formats| {
                let owner = Owner::new()?;
                let _open = Open::new(owner.0)?;
                Ok(read_requested(formats, |kind| {
                    read_kind(kind, skip_sources)
                }))
            },
        )
    })
}
pub(super) fn invoke(bytes: &[u8]) -> Result<Vec<u8>> {
    if bytes.len() > LIMIT * 2 {
        return Err(anyhow::anyhow!("Clipboard request is too large"));
    }
    let request: Request = serde_json::from_slice(bytes)?;
    let result = match request.operation.as_str() {
        "write" | "write_embedded" => {
            if !request.formats.is_empty()
                || request.skip_embedded
                || !request.skip_sources.is_empty()
                || request.generation.is_some()
            {
                return Err(anyhow::anyhow!(
                    "Read options are not valid for clipboard writes"
                ));
            }
            write(
                request.representations,
                request.operation == "write_embedded",
            )?;
            Packet {
                representations: vec![],
                generation: None,
            }
        }
        "read" | "read_picture" => {
            if !request.representations.is_empty() {
                return Err(anyhow::anyhow!(
                    "Read requests cannot contain representations"
                ));
            }
            read_packet(
                request.operation == "read_picture",
                &request.formats,
                request.skip_embedded,
                &request.skip_sources,
                request.generation.as_deref(),
            )?
        }
        _ => return Err(anyhow::anyhow!("Unknown clipboard operation")),
    };
    Ok(serde_json::to_vec(&result)?)
}

#[cfg(test)]
mod tests;
