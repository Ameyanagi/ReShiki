//! Explicit, bounded access to one pasteboard item. Runs only in a worker.
use base64::{Engine as _, engine::general_purpose::STANDARD};
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{NSPasteboard, NSPasteboardItem, NSPasteboardWriting};
use objc2_foundation::{NSArray, NSData, NSString};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const LIMIT: usize = 64 * 1024 * 1024;
#[path = "../../../src/clipboard/priority.rs"]
mod priority;
#[cfg(test)]
use priority::PICTURES;
use priority::READABLE;

const CDXML_TYPES: &[&str] = &[
    "com.revvity.cdxml",
    "com.perkinelmer.cdxml",
    "com.cambridgesoft.cdxml",
];

fn pasteboard_types(kind: &str) -> Option<&'static [&'static str]> {
    // NSPasteboardItem accepts UTI identifiers, not MIME strings. These aliases
    // are ChemDraw's declared CDXML types; the worker protocol stays portable.
    match kind {
        "chemical/x-cdxml" => Some(CDXML_TYPES),
        "public.html" => Some(&["public.html"]),
        "public.rtf" => Some(&["public.rtf"]),
        _ => READABLE
            .iter()
            .find(|&&name| name == kind)
            .map(std::slice::from_ref),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
struct Representation {
    #[serde(rename = "type")]
    kind: String,
    data: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    read_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
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
struct Response {
    representations: Vec<Representation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation: Option<String>,
}

#[cfg(test)]
fn read(board: &NSPasteboard, image_only: bool) -> Result<Vec<Representation>, String> {
    Ok(read_requested(board, image_only, &[], &[], None)?.representations)
}

fn read_requested(
    board: &NSPasteboard,
    image_only: bool,
    formats: &[String],
    skip_sources: &[String],
    generation: Option<&str>,
) -> Result<Response, String> {
    // NSPasteboard.changeCount advances when ownership changes. Supplying
    // already-promised/lazy data keeps that ownership and does not advance it.
    read_generation(
        generation,
        || board.changeCount().to_string(),
        || read_item(board, image_only, formats, skip_sources),
    )
}

/// Check before applying exclusions and after acquisition, including failures.
fn read_generation(
    expected: Option<&str>,
    mut generation: impl FnMut() -> String,
    read: impl FnOnce() -> Result<Vec<Representation>, String>,
) -> Result<Response, String> {
    priority::validate_generation(expected)?;
    let initial = generation();
    if expected.is_some_and(|expected| expected != initial) {
        return Err(priority::CLIPBOARD_CHANGED.into());
    }
    let result = read();
    if generation() != initial {
        return Err(priority::CLIPBOARD_CHANGED.into());
    }
    Ok(Response {
        representations: result?,
        generation: Some(initial),
    })
}

fn read_item(
    board: &NSPasteboard,
    image_only: bool,
    formats: &[String],
    skip_sources: &[String],
) -> Result<Vec<Representation>, String> {
    let Some(item) = board
        .pasteboardItems()
        .and_then(|items| items.firstObject())
    else {
        return Ok(Vec::new());
    };
    let available = item.types();
    read_next(
        formats,
        skip_sources,
        image_only,
        |name| available.containsObject(&NSString::from_str(name)),
        |name| {
            let data = item
                .dataForType(&NSString::from_str(name))
                .ok_or("Could not read the selected clipboard representation")?;
            if data.len() > LIMIT {
                return Err("Clipboard data exceeds the 64 MB limit".into());
            }
            Ok(data.to_vec())
        },
    )
}

fn read_next(
    formats: &[String],
    skip_sources: &[String],
    image_only: bool,
    available: impl Fn(&str) -> bool,
    mut fetch: impl FnMut(&str) -> Result<Vec<u8>, String>,
) -> Result<Vec<Representation>, String> {
    priority::validate_skip_sources(skip_sources)?;
    for name in priority::read_formats(formats, image_only)? {
        if skip_sources.iter().any(|source| source == name) || !available(name) {
            continue;
        }
        let bytes = fetch(name).and_then(|bytes| {
            if bytes.is_empty() || bytes.len() > LIMIT {
                Err("Invalid or oversized clipboard representation".into())
            } else {
                Ok(bytes)
            }
        });
        let (data, read_error) = match bytes {
            Ok(bytes) => (STANDARD.encode(bytes), None),
            Err(error) => (String::new(), Some(error.chars().take(1000).collect())),
        };
        return Ok(vec![Representation {
            kind: name.into(),
            data,
            read_error,
            source: Some(name.into()),
        }]);
    }
    Ok(Vec::new())
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
        if representation.read_error.is_some()
            || representation.source.is_some()
            || kinds.iter().any(|&kind| !names.insert(kind))
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
    priority::read_formats(&request.formats, request.operation == "read_picture")?;
    priority::validate_skip_sources(&request.skip_sources)?;
    priority::validate_generation(request.generation.as_deref())?;
    if request.operation == "write"
        && (!request.formats.is_empty()
            || request.skip_embedded
            || !request.skip_sources.is_empty()
            || request.generation.is_some())
    {
        return Err("Clipboard write requests cannot include read options".into());
    }
    if request.operation != "write" && !request.representations.is_empty() {
        return Err("Clipboard read requests cannot include data".into());
    }
    let board = NSPasteboard::generalPasteboard();
    let response = match request.operation.as_str() {
        "read" | "read_picture" => read_requested(
            &board,
            request.operation == "read_picture",
            &request.formats,
            &request.skip_sources,
            request.generation.as_deref(),
        )?,
        "write" => {
            write(&board, &request.representations)?;
            Response {
                representations: Vec::new(),
                generation: None,
            }
        }
        _ => return Err("Unsupported clipboard operation".into()),
    };
    let output = serde_json::to_vec(&response).map_err(|e| e.to_string())?;
    if output.len() > LIMIT * 2 {
        return Err("Clipboard response is too large".into());
    }
    Ok(output)
}

#[cfg(test)]
mod tests;
