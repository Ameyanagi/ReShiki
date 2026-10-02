//! Bounded subprocess protocol for the optional LibreOffice UNO extension.
//!
//! No paths or executable names are accepted from documents. The extension
//! starts this installed executable and passes native drawing bytes on stdin.
use crate::{document::Document, export, scene};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::Serialize;
use std::io::{Read, Write};

const LIMIT: usize = 64 * 1024 * 1024;

#[derive(Serialize)]
struct Preview {
    version: u32,
    native: String,
    png: String,
    /// Native drawing size in hundredths of a millimeter (UNO MapUnit).
    extent: [i32; 2],
}

fn preview(bytes: &[u8]) -> Result<Preview, String> {
    if bytes.len() > LIMIT {
        return Err("Embedded drawing exceeds 64 MB".into());
    }
    let doc = Document::from_json(bytes)?;
    let png = export::clipboard_png(&doc)?;
    if png.len() > LIMIT {
        return Err("Embedded preview exceeds 64 MB".into());
    }
    let (lo, hi) = scene::bounds(&scene::primitives(&doc));
    let scale = crate::style::DEFAULT.points_per_world() * 2540.0 / 72.0;
    let extent = [hi.x - lo.x, hi.y - lo.y]
        .map(|size| (size * scale).round().clamp(1.0, i32::MAX as f32) as i32);
    Ok(Preview {
        version: 1,
        native: STANDARD.encode(bytes),
        png: STANDARD.encode(png),
        extent,
    })
}

fn input() -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(LIMIT as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > LIMIT {
        return Err("Embedded drawing exceeds 64 MB".into());
    }
    Ok(bytes)
}

/// Returns `None` for an ordinary app launch; workers run before GUI startup.
pub fn run(argument: Option<&std::ffi::OsStr>) -> Option<Result<(), String>> {
    let mode = argument.and_then(|arg| arg.to_str())?;
    if !matches!(
        mode,
        "--libreoffice-preview" | "--libreoffice-clipboard" | "--libreoffice-copy"
    ) {
        return None;
    }
    Some((|| {
        let bytes = if mode == "--libreoffice-clipboard" {
            let runtime = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
            runtime.block_on(crate::clipboard::libreoffice::read())?
        } else {
            input()?
        };
        if mode == "--libreoffice-copy" {
            let runtime = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
            runtime.block_on(crate::clipboard::libreoffice::write(&bytes))?;
            std::io::stdout()
                .write_all(b"{}\n")
                .map_err(|e| e.to_string())
        } else {
            serde_json::to_writer(std::io::stdout(), &preview(&bytes)?).map_err(|e| e.to_string())
        }
    })())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_preserves_native_bytes_and_physical_extent() {
        let bytes = include_bytes!("../tests/fixtures/ui-drawn-ethanol.reshiki");
        let packet = preview(bytes).unwrap();
        assert_eq!(STANDARD.decode(packet.native).unwrap(), bytes);
        let png = STANDARD.decode(packet.png).unwrap();
        let reader = png::Decoder::new(std::io::Cursor::new(png))
            .read_info()
            .unwrap();
        let info = reader.info();
        let resolution = info.pixel_dims.unwrap();
        assert!(
            (info.width as f64 * 100_000.0 / resolution.xppu as f64 - packet.extent[0] as f64)
                .abs()
                < 4.0
        );
        assert!(
            (info.height as f64 * 100_000.0 / resolution.yppu as f64 - packet.extent[1] as f64)
                .abs()
                < 4.0
        );
    }

    #[test]
    fn malformed_drawings_are_not_given_a_preview() {
        assert!(preview(b"not a drawing").is_err());
        assert!(preview(br#"{"version":999999}"#).is_err());
    }
}
