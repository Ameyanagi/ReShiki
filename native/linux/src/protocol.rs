use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc, time::Duration};

#[path = "../../../src/clipboard/priority.rs"]
mod priority;
pub use priority::CLIPBOARD_CHANGED;
pub use priority::read_formats;

pub const LIMIT: usize = 64 * 1024 * 1024;
pub const JSON_LIMIT: usize = LIMIT * 2;
pub const MAX_FORMATS: usize = 128;
pub const TRANSFER_TIMEOUT: Duration = Duration::from_secs(5);
pub const TRANSFER_TOTAL_TIMEOUT: Duration = Duration::from_secs(60);
pub const MAX_TRANSFERS: usize = 16;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Representation {
    #[serde(rename = "type")]
    pub kind: String,
    pub data: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub operation: String,
    #[serde(default)]
    pub representations: Vec<Representation>,
    #[serde(default)]
    pub formats: Vec<String>,
    #[serde(default)]
    pub skip_embedded: bool,
    #[serde(default)]
    pub skip_sources: Vec<String>,
    #[serde(default)]
    pub generation: Option<String>,
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum Response {
    Success {
        representations: Vec<Representation>,
        #[serde(skip_serializing_if = "Option::is_none")]
        generation: Option<String>,
    },
    Failure {
        error: String,
    },
}

impl Response {
    pub fn empty() -> Self {
        Self::Success {
            representations: Vec::new(),
            generation: None,
        }
    }

    pub fn read(representation: Option<Representation>, generation: String) -> Self {
        Self::Success {
            representations: representation.into_iter().collect(),
            generation: Some(generation),
        }
    }
}

pub type Offer = BTreeMap<String, Arc<[u8]>>;

const NATIVE: &[&str] = &["application/x-reshiki-drawing+json", "dev.reshiki.drawing"];
const LEGACY: &[&str] = &["application/x-moruno-drawing+json", "dev.moruno.drawing"];
const CDX: &[&str] = &[
    "chemical/x-cdx",
    "application/x-chemdraw",
    "com.revvity.chemdraw.cdx-clipboard",
    "com.perkinelmer.chemdraw.cdx-clipboard",
    "com.cambridgesoft.cdx",
    "com.revvity.cdx",
    "com.perkinelmer.cdx",
];
const CDXML: &[&str] = &[
    "chemical/x-cdxml",
    "com.revvity.cdxml",
    "com.perkinelmer.cdxml",
    "com.cambridgesoft.cdxml",
    "com.perkinelmer.chemdraw.cdxml",
    "public.cdxml",
];
const TEXT: &[&str] = &["text/plain;charset=utf-8", "text/plain", "UTF8_STRING"];

// MIME aliases only. The app's shared portable list determines read priority.
pub const READABLE: &[(&str, &[&str])] = &[
    ("dev.reshiki.drawing", NATIVE),
    ("dev.moruno.drawing", LEGACY),
    ("com.revvity.chemdraw.cdx-clipboard", CDX),
    ("chemical/x-cdxml", CDXML),
    (
        "com.mdli.molfile",
        &["chemical/x-mdl-molfile", "com.mdli.molfile"],
    ),
    (
        "org.opensmiles.smiles",
        &["chemical/x-daylight-smiles", "org.opensmiles.smiles"],
    ),
    ("public.png", &["image/png"]),
    ("public.tiff", &["image/tiff"]),
    ("public.jpeg", &["image/jpeg"]),
    ("org.webmproject.webp", &["image/webp", "public.webp"]),
    ("public.utf8-plain-text", TEXT),
    ("com.adobe.pdf", &["application/pdf"]),
    ("public.svg-image", &["image/svg+xml"]),
];

pub fn parse_request(input: &[u8]) -> Result<Request, String> {
    if input.len() > JSON_LIMIT {
        return Err("Clipboard request is too large".into());
    }
    let request: Request =
        serde_json::from_slice(input).map_err(|e| format!("Invalid clipboard request: {e}"))?;
    if !matches!(
        request.operation.as_str(),
        "write" | "read" | "read_picture"
    ) {
        return Err("Unsupported clipboard operation".into());
    }
    if request.operation != "write" && !request.representations.is_empty() {
        return Err("Clipboard read requests cannot include data".into());
    }
    if request.operation == "write" {
        if !request.formats.is_empty()
            || request.skip_embedded
            || !request.skip_sources.is_empty()
            || request.generation.is_some()
        {
            return Err("Clipboard write requests cannot include read options".into());
        }
    } else {
        read_formats(&request.formats, request.operation == "read_picture")?;
        priority::validate_skip_sources(&request.skip_sources)?;
        priority::validate_generation(request.generation.as_deref())?;
    }
    Ok(request)
}

fn mime_types(kind: &str) -> Option<&'static [&'static str]> {
    READABLE
        .iter()
        .find_map(|(name, types)| (*name == kind || types.contains(&kind)).then_some(*types))
}

pub fn prepare_offer(representations: &[Representation]) -> Result<Offer, String> {
    if representations.is_empty() || representations.len() > MAX_FORMATS {
        return Err("Clipboard write requires 1–128 supported representations".into());
    }
    let mut offer = Offer::new();
    let mut total = 0_usize;
    for representation in representations {
        if representation.read_error.is_some() {
            return Err("Clipboard writes cannot include read failures".into());
        }
        let types = mime_types(&representation.kind).ok_or_else(|| {
            format!(
                "Unsupported Linux clipboard format: {}",
                representation.kind
            )
        })?;
        if representation.data.is_empty() || representation.data.len() > LIMIT.div_ceil(3) * 4 {
            return Err("Invalid or oversized clipboard representation".into());
        }
        let bytes = STANDARD
            .decode(&representation.data)
            .map_err(|_| "Invalid clipboard encoding")?;
        if bytes.is_empty() || bytes.len() > LIMIT {
            return Err("Clipboard data exceeds 64 MB or is empty".into());
        }
        if types == TEXT {
            std::str::from_utf8(&bytes).map_err(|_| "Invalid clipboard UTF-8 text")?;
        }
        let existing = types.iter().find_map(|kind| offer.get(*kind));
        let data = if let Some(existing) = existing {
            if existing.as_ref() != bytes {
                return Err("Conflicting clipboard representations for the same MIME type".into());
            }
            Arc::clone(existing)
        } else {
            total = total
                .checked_add(bytes.len())
                .ok_or("Clipboard data is too large")?;
            if total > LIMIT {
                return Err("Combined clipboard representations exceed 64 MB".into());
            }
            Arc::from(bytes)
        };
        for &kind in types {
            offer.insert(kind.to_owned(), Arc::clone(&data));
        }
    }
    Ok(offer)
}

pub fn choose_type<'a>(
    available: impl Fn(&str) -> bool,
    formats: &[&'a str],
) -> Option<(&'a str, &'static str)> {
    formats.iter().find_map(|kind| {
        mime_types(kind)?
            .iter()
            .find(|mime| available(mime))
            .map(|mime| (*kind, *mime))
    })
}

/// Preserve portable request order while avoiding repeated MIME aliases.
pub fn requested_mime_types(formats: &[&str], skip_sources: &[String]) -> Vec<String> {
    let mut requested = Vec::new();
    for kind in formats {
        if let Some(types) = mime_types(kind) {
            for mime in types {
                if !skip_sources.iter().any(|source| source == mime)
                    && !requested.iter().any(|requested| requested == mime)
                {
                    requested.push((*mime).to_owned());
                }
            }
        }
    }
    requested
}

pub fn read_failure(kind: &str, mime: &str, error: impl AsRef<str>) -> Representation {
    Representation {
        kind: kind.into(),
        data: String::new(),
        read_error: Some(error.as_ref().chars().take(1000).collect()),
        source: Some(mime.into()),
    }
}

pub fn selected_representation(kind: &str, mime: &str, bytes: &[u8]) -> Representation {
    match representation(kind, bytes) {
        Ok(mut representation) => {
            representation.source = Some(mime.into());
            representation
        }
        Err(error) => read_failure(kind, mime, error),
    }
}

pub fn representation(kind: &str, bytes: &[u8]) -> Result<Representation, String> {
    if bytes.is_empty() {
        return Err("The selected clipboard representation is empty".into());
    }
    if bytes.len() > LIMIT {
        return Err("Clipboard data exceeds 64 MB".into());
    }
    Ok(Representation {
        kind: kind.into(),
        data: STANDARD.encode(bytes),
        read_error: None,
        source: None,
    })
}

#[cfg(test)]
mod tests;
