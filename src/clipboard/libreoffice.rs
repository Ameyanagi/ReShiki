//! The optional LibreOffice extension exchanges only ReShiki's native data.
use super::{Document, NATIVE, Representation, invoke};

pub async fn read() -> Result<Vec<u8>, String> {
    let packet = invoke("read", &[]).await?;
    let item = packet
        .representations
        .into_iter()
        .find(|item| item.kind == NATIVE || item.kind == "dev.moruno.drawing")
        .ok_or("Copy an editable drawing in ReShiki before using Paste ReShiki Drawing")?;
    let bytes = item.bytes()?;
    Document::from_json(&bytes)?;
    Ok(bytes)
}

pub async fn write(bytes: &[u8]) -> Result<(), String> {
    let doc = Document::from_json(bytes)?;
    let png = crate::export::clipboard_png(&doc)?;
    let svg = crate::export::clipboard_drawing(&doc, "svg")?;
    let items = [
        Representation::new(NATIVE, bytes),
        Representation::new("public.png", &png),
        Representation::new("public.svg-image", &svg),
    ];
    invoke("write", &items).await?;
    Ok(())
}
