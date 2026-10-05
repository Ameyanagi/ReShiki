//! Explicit, bounded access to one pasteboard item. Runs only in a worker.
use base64::{Engine as _, engine::general_purpose::STANDARD};
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{NSPasteboard, NSPasteboardItem, NSPasteboardWriting};
use objc2_foundation::{NSArray, NSData, NSString};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const LIMIT: usize = 64 * 1024 * 1024;
const PICTURES: &[&str] = &[
    "public.png",
    "public.tiff",
    "public.jpeg",
    "org.webmproject.webp",
    "public.webp",
];
const CDXML_TYPES: &[&str] = &[
    "com.revvity.cdxml",
    "com.perkinelmer.cdxml",
    "com.cambridgesoft.cdxml",
];
const READABLE: &[&str] = &[
    "dev.reshiki.drawing",
    "dev.moruno.drawing",
    "com.revvity.chemdraw.cdx-clipboard",
    "com.perkinelmer.chemdraw.cdx-clipboard",
    "com.cambridgesoft.cdx",
    "com.revvity.cdx",
    "com.perkinelmer.cdx",
    "com.revvity.cdxml",
    "com.perkinelmer.cdxml",
    "com.cambridgesoft.cdxml",
    "chemical/x-cdxml",
    "com.mdli.molfile",
    "org.opensmiles.smiles",
    "public.png",
    "public.tiff",
    "public.jpeg",
    "org.webmproject.webp",
    "public.webp",
    "public.utf8-plain-text",
    "com.adobe.pdf",
    "public.svg-image",
];

fn pasteboard_types(kind: &str) -> Option<&'static [&'static str]> {
    // NSPasteboardItem accepts UTI identifiers, not MIME strings. These aliases
    // are ChemDraw's declared CDXML types; the worker protocol stays portable.
    if kind == "chemical/x-cdxml" {
        Some(CDXML_TYPES)
    } else {
        READABLE
            .iter()
            .find(|&&name| name == kind)
            .map(std::slice::from_ref)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
struct Representation {
    #[serde(rename = "type")]
    kind: String,
    data: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    operation: String,
    #[serde(default)]
    representations: Vec<Representation>,
}

#[derive(Serialize)]
struct Response {
    representations: Vec<Representation>,
}

fn read(board: &NSPasteboard, image_only: bool) -> Result<Vec<Representation>, String> {
    let Some(item) = board
        .pasteboardItems()
        .and_then(|items| items.firstObject())
    else {
        return Ok(Vec::new());
    };
    let mut result = Vec::new();
    let mut size = 0;
    let available = item.types();
    for &name in if image_only { PICTURES } else { READABLE } {
        let kind = NSString::from_str(name);
        if !available.containsObject(&kind) {
            continue;
        }
        let Some(data) = item.dataForType(&kind) else {
            continue;
        };
        if data.len() > LIMIT - size {
            return Err("Clipboard data exceeds the 64 MB limit".into());
        }
        size += data.len();
        result.push(Representation {
            kind: name.into(),
            data: STANDARD.encode(data.to_vec()),
        });
        // Fetch only the best editable/image representation. Lazy providers may
        // have very large alternate representations that we do not need.
        if matches!(
            name,
            "dev.reshiki.drawing"
                | "dev.moruno.drawing"
                | "com.mdli.molfile"
                | "org.opensmiles.smiles"
                | "public.utf8-plain-text"
        ) || name.contains("cdx")
            || PICTURES.contains(&name)
        {
            break;
        }
    }
    Ok(result)
}

fn write(board: &NSPasteboard, representations: &[Representation]) -> Result<(), String> {
    if representations.is_empty() || representations.len() > 20 {
        return Err("No supported clipboard representations".into());
    }
    let item = NSPasteboardItem::new();
    let mut size = 0;
    let mut names = HashSet::new();
    for representation in representations {
        let kinds = pasteboard_types(&representation.kind)
            .ok_or("Invalid or oversized clipboard representation")?;
        // Count actual native aliases as well as input representations. An
        // alias collision must not silently overwrite previously prepared data.
        let remaining = (LIMIT - size) / kinds.len();
        if kinds.iter().any(|&kind| !names.insert(kind))
            || representation.data.len() > remaining.div_ceil(3) * 4
        {
            return Err("Invalid or oversized clipboard representation".into());
        }
        let bytes = STANDARD
            .decode(&representation.data)
            .map_err(|_| "Invalid clipboard encoding")?;
        if bytes.is_empty() || bytes.len() > remaining {
            return Err("Invalid or oversized clipboard representation".into());
        }
        size += bytes.len() * kinds.len();
        let data = NSData::with_bytes(&bytes);
        for &kind in kinds {
            if !item.setData_forType(&data, &NSString::from_str(kind)) {
                return Err(format!("Could not prepare clipboard data for {kind}"));
            }
        }
    }
    // Prepare every representation before replacing any existing data. Cut is
    // committed by the editor only after the worker acknowledges this write.
    let object: &ProtocolObject<dyn NSPasteboardWriting> = ProtocolObject::from_ref(&*item);
    let objects = NSArray::from_slice(&[object]);
    board.clearContents();
    if !board.writeObjects(&objects) {
        return Err("Could not write clipboard data".into());
    }
    Ok(())
}

pub fn execute(input: &[u8]) -> Result<Vec<u8>, String> {
    if input.len() > LIMIT * 2 {
        return Err("Clipboard request is too large".into());
    }
    let request: Request =
        serde_json::from_slice(input).map_err(|_| "Invalid clipboard request or data")?;
    let board = NSPasteboard::generalPasteboard();
    let representations = match request.operation.as_str() {
        "read" => read(&board, false)?,
        "read_picture" => read(&board, true)?,
        "write" => {
            write(&board, &request.representations)?;
            Vec::new()
        }
        _ => return Err("Unsupported clipboard operation".into()),
    };
    serde_json::to_vec(&Response { representations }).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests;
