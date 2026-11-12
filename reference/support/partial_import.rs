//! #1138 imports SMILES features that the frozen worker refused, with a
//! warning naming what was dropped or assumed.

/// The frozen refusal is one that partial import replaces, and the import
/// reports a partial-import warning.
pub fn replaces_failure(format: &str, failure: Option<&str>, warnings: &[String]) -> bool {
    matches!(format, "smiles" | "rsmi")
        && (matches!(
            failure,
            Some(
                "More than two unpaired electrons on an atom are not supported yet"
                    | "Query atoms are not supported yet; import was cancelled."
                    | "This bond type is not supported yet."
                    | "This stereochemistry class is not supported yet."
            )
        ) || format == "rsmi"
            && matches!(
                failure,
                // The frozen reaction importer's own refusal of query atoms
                // (reference/engine/reactions.py), such as CX u, rb or s.
                Some("Query reaction atoms are not supported yet")
                // Reaction sanitization reads a CX _chiralPermutation string
                // before the frozen worker checks its unsupported SP/TB/OH tag.
                // A malformed value fails its unsigned cast. Partial import
                // drops that configuration and reports the omission instead.
                // std::bad_any_cast::what differs between libc++, libstdc++
                // and MSVC.
                | Some("bad any cast" | "bad any_cast" | "Bad any_cast")
            ))
        && warnings
            .iter()
            .any(|warning| reshiki::engine::native_import::is_partial_import_warning(warning))
}

const ALLENE: &str = "The allene stereo tag (@AL) at ";

/// Whether the import names a dropped invalid `@AL` tag (#892).
pub fn drops_allene_tag(warnings: &[String]) -> bool {
    warnings.iter().any(|warning| warning.starts_with(ALLENE))
}

/// The input without the `@AL` tags in its SMILES body, whose import is the
/// expectation for a partial import that drops them. The frozen worker's
/// response cannot serve: on `[Pt@ALH]` it also loses the stated bracket H.
/// An input that also has a supported allene tag fails the comparison.
pub fn without_allene_tags(text: &str) -> String {
    let end = text.find(char::is_whitespace).unwrap_or(text.len());
    let (mut plain, mut rest) = (String::new(), &text[..end]);
    while let Some(at) = rest.find("@AL") {
        plain.push_str(&rest[..at]);
        rest = rest[at + 3..].trim_start_matches(|c: char| c.is_ascii_digit());
    }
    plain + rest + &text[end..]
}

/// The partial import equals the tag-free import apart from its `@AL` notices.
pub fn matches_without_allene_tags(
    actual: &reshiki::engine::Response,
    plain: &reshiki::engine::Response,
) -> anyhow::Result<()> {
    let mut actual = actual.clone();
    actual
        .warnings
        .retain(|warning| !warning.starts_with(ALLENE));
    anyhow::ensure!(
        serde_json::to_value(actual)? == serde_json::to_value(plain)?,
        "The import differs from that of the input without @AL tags"
    );
    Ok(())
}
