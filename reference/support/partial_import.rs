//! #1138 imports SMILES features that the frozen worker refused, with a
//! warning naming what was dropped or assumed. A bare dummy's implicit CX
//! query marker needs no warning when the suffix leaves its import unchanged.

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

/// The frozen worker refused a query atom, but partial import needed no warning.
/// This allowance must also compare the import with the suffix-free input.
pub fn implicit_dummy_query(format: &str, failure: Option<&str>, warnings: &[String]) -> bool {
    warnings.is_empty()
        && match format {
            "smiles" => failure == Some("Query atoms are not supported yet; import was cancelled."),
            // The frozen reaction importer's own refusal (reference/engine/reactions.py).
            "rsmi" => matches!(
                failure,
                Some(
                    "Query atoms are not supported yet; import was cancelled."
                        | "Query reaction atoms are not supported yet"
                )
            ),
            _ => false,
        }
}

/// The SMILES or reaction body before its CX suffix.
pub fn without_cx_suffix(text: &str) -> &str {
    &text[..text.find([' ', '\t']).unwrap_or(text.len())]
}

/// Ignoring an implicit query marker must otherwise leave the complete import unchanged.
pub fn matches_without_cx_suffix(
    actual: &reshiki::engine::Response,
    plain: &reshiki::engine::Response,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        serde_json::to_value(actual)? == serde_json::to_value(plain)?,
        "The import differs from that of the input without its CX suffix"
    );
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn implicit_dummy_query_requires_exact_refusal_format_and_no_warnings() {
        let failure = Some("Query atoms are not supported yet; import was cancelled.");
        let reaction = Some("Query reaction atoms are not supported yet");
        assert!(implicit_dummy_query("rsmi", reaction, &[]));
        assert!(!implicit_dummy_query("smiles", reaction, &[]));
        for format in ["smiles", "rsmi"] {
            assert!(implicit_dummy_query(format, failure, &[]));
            assert!(!implicit_dummy_query(format, None, &[]));
            assert!(!implicit_dummy_query(
                format,
                Some("Query atoms are not supported yet; import was cancelled. Extra failure"),
                &[],
            ));
            assert!(!implicit_dummy_query(format, failure, &["Warning".into()]));
        }
        assert!(!implicit_dummy_query("mol", failure, &[]));
    }

    #[test]
    fn cx_suffix_comparison_keeps_the_whole_response() {
        for (text, plain) in [
            ("* |rb:0:*|", "*"),
            ("*>>C\t|rb:0:*|", "*>>C"),
            ("* |atomProp:0.key.value with spaces|", "*"),
            ("*", "*"),
        ] {
            assert_eq!(without_cx_suffix(text), plain);
        }
        let plain = reshiki::engine::Response {
            document: None,
            analysis: None,
            output: None,
            engine_version: String::new(),
            warnings: Vec::new(),
        };
        assert!(matches_without_cx_suffix(&plain, &plain).is_ok());
        let mut changed = plain.clone();
        changed.output = Some("Changed output".into());
        assert!(matches_without_cx_suffix(&changed, &plain).is_err());
        changed.output = None;
        changed.warnings.push("Warning".into());
        assert!(matches_without_cx_suffix(&changed, &plain).is_err());
    }
}
