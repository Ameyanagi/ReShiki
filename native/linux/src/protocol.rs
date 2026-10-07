use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc, time::Duration};

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
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub operation: String,
    #[serde(default)]
    pub representations: Vec<Representation>,
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum Response {
    Success {
        representations: Vec<Representation>,
    },
    Failure {
        error: String,
    },
}

impl Response {
    pub fn empty() -> Self {
        Self::read(None)
    }

    pub fn read(representation: Option<Representation>) -> Self {
        Self::Success {
            representations: representation.into_iter().collect(),
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
];
const TEXT: &[&str] = &["text/plain;charset=utf-8", "text/plain", "UTF8_STRING"];

// Ordered identically to the native drawing/image/text preference in the app.
pub const READABLE: &[(&str, &[&str], bool)] = &[
    ("dev.reshiki.drawing", NATIVE, false),
    ("dev.moruno.drawing", LEGACY, false),
    ("com.revvity.chemdraw.cdx-clipboard", CDX, false),
    ("chemical/x-cdxml", &["chemical/x-cdxml"], false),
    (
        "com.mdli.molfile",
        &["chemical/x-mdl-molfile", "com.mdli.molfile"],
        false,
    ),
    (
        "org.opensmiles.smiles",
        &["chemical/x-daylight-smiles", "org.opensmiles.smiles"],
        false,
    ),
    ("public.png", &["image/png"], true),
    ("public.tiff", &["image/tiff"], true),
    ("public.jpeg", &["image/jpeg"], true),
    ("public.webp", &["image/webp"], true),
    ("public.utf8-plain-text", TEXT, false),
    ("com.adobe.pdf", &["application/pdf"], false),
    ("public.svg-image", &["image/svg+xml"], false),
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
    Ok(request)
}

fn mime_types(kind: &str) -> Option<&'static [&'static str]> {
    if CDX.contains(&kind) {
        return Some(CDX);
    }
    READABLE
        .iter()
        .find_map(|(name, types, _)| (*name == kind || types.contains(&kind)).then_some(*types))
}

pub fn prepare_offer(representations: &[Representation]) -> Result<Offer, String> {
    if representations.is_empty() || representations.len() > MAX_FORMATS {
        return Err("Clipboard write requires 1–128 supported representations".into());
    }
    let mut offer = Offer::new();
    let mut total = 0_usize;
    for representation in representations {
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

pub fn choose_type(
    available: impl Fn(&str) -> bool,
    picture_only: bool,
) -> Option<(&'static str, &'static str)> {
    READABLE
        .iter()
        .filter(|(_, _, picture)| !picture_only || *picture)
        .find_map(|(kind, types, _)| {
            types
                .iter()
                .find(|mime| available(mime))
                .map(|mime| (*kind, *mime))
        })
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
    })
}

#[cfg(test)]
mod tests;
