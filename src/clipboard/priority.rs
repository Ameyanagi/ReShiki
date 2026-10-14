//! Shared paste order. Workers fetch just one requested representation; the
//! editor advances this list only after importing that representation fails.
pub const CLIPBOARD_CHANGED: &str = "The clipboard changed while pasting; paste again";

/// Tokens identify a native clipboard generation, never clipboard contents.
pub fn validate_generation(generation: Option<&str>) -> Result<(), String> {
    if generation.is_some_and(|token| {
        token.is_empty()
            || token.len() > 128
            || token.bytes().any(|byte| !(b'!'..=b'~').contains(&byte))
    }) {
        return Err("Invalid clipboard generation".into());
    }
    Ok(())
}

pub const PICTURES: &[&str] = &[
    "public.png",
    "public.tiff",
    "public.jpeg",
    "org.webmproject.webp",
    "public.webp",
];

pub const READABLE: &[&str] = &[
    "dev.reshiki.drawing",
    "dev.moruno.drawing",
    "com.revvity.chemdraw.cdx-clipboard",
    "com.perkinelmer.chemdraw.cdx-clipboard",
    "com.cambridgesoft.cdx",
    "com.revvity.cdx",
    "com.perkinelmer.cdx",
    "chemical/x-cdx",
    "application/x-chemdraw",
    "com.revvity.cdxml",
    "com.perkinelmer.cdxml",
    "com.cambridgesoft.cdxml",
    "com.perkinelmer.chemdraw.cdxml",
    "chemical/x-cdxml",
    "public.cdxml",
    "com.mdli.molfile",
    "org.opensmiles.smiles",
    "public.utf8-plain-text",
    "public.png",
    "public.tiff",
    "public.jpeg",
    "org.webmproject.webp",
    "public.webp",
    "com.adobe.pdf",
    "public.svg-image",
];

/// Keep read requests bounded and make the same default apply to every worker.
pub fn read_formats(requested: &[String], picture_only: bool) -> Result<Vec<&str>, String> {
    if requested.len() > READABLE.len()
        || requested
            .iter()
            .any(|kind| !READABLE.contains(&kind.as_str()))
    {
        return Err("Unsupported or oversized clipboard format list".into());
    }
    let formats: Vec<_> = if requested.is_empty() {
        READABLE.to_vec()
    } else {
        requested.iter().map(String::as_str).collect()
    };
    Ok(formats
        .into_iter()
        .filter(|kind| !picture_only || PICTURES.contains(kind))
        .collect())
}

/// A failed transport alias is consumed once, even when several portable
/// format names map to it. This does not increase the worker's transfer budget.
pub fn validate_skip_sources(sources: &[String]) -> Result<(), String> {
    if sources.len() > 128
        || sources
            .iter()
            .any(|source| source.is_empty() || source.len() > 255 || source.contains('\0'))
    {
        return Err("Invalid or oversized clipboard source list".into());
    }
    Ok(())
}
