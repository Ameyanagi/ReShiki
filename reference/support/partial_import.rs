//! #1138 imports SMILES features that the frozen worker refused, with a
//! warning naming what was dropped or assumed. A bare dummy's implicit CX
//! query marker needs no warning when no explicit query constraint remains.
//! Atoms with three or four radical electrons import as written instead:
//! support/frozen_radicals.py lifts that refusal, so their imports are
//! compared in full.

/// The frozen refusal is one that partial import replaces, and the import
/// reports a partial-import warning.
pub fn replaces_failure(format: &str, failure: Option<&str>, warnings: &[String]) -> bool {
    matches!(format, "smiles" | "rsmi")
        && (matches!(
            failure,
            Some(
                "Query atoms are not supported yet; import was cancelled."
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

/// The frozen query refusal needs no warning only if replacing dummy atoms
/// removes all unsupported features, as checked by the strict reader.
pub fn implicit_dummy_query(
    format: &str,
    text: &str,
    failure: Option<&str>,
    warnings: &[String],
) -> bool {
    let query_failure = match format {
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
    };
    if !query_failure || !warnings.is_empty() {
        return false;
    }
    let text = with_uranium_atoms(text);
    match format {
        // Strict reading also accepts features that partial import drops with
        // a notice (SP/TB/OH tags, more than two radical electrons, `~`/`Z`
        // bonds to a removed H), so those must not be dropped either.
        "smiles" => {
            reshiki::chemistry::smiles::read(&text).is_ok()
                && reshiki::chemistry::smiles::read_partial(&text)
                    .is_ok_and(|imported| imported.dropped.is_empty())
        }
        "rsmi" => reshiki::chemistry::reaction::read_smiles(&text).is_ok(),
        _ => false,
    }
}

/// Replace each dummy atom while preserving atom indices and the CX suffix.
fn with_uranium_atoms(text: &str) -> String {
    let end = text.find('|').unwrap_or(text.len());
    let mut replaced = String::with_capacity(text.len());
    let mut bracketed = false;
    for c in text[..end].chars() {
        match c {
            '[' => bracketed = true,
            ']' => bracketed = false,
            _ => (),
        }
        if c == '*' {
            replaced.push_str(if bracketed { "U" } else { "[U]" });
        } else {
            replaced.push(c);
        }
    }
    replaced.push_str(&text[end..]);
    replaced
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
        assert!(implicit_dummy_query("rsmi", "*>>*", reaction, &[]));
        assert!(!implicit_dummy_query("smiles", "*", reaction, &[]));
        for (format, text) in [("smiles", "*"), ("rsmi", "*>>*")] {
            assert!(implicit_dummy_query(format, text, failure, &[]));
            assert!(!implicit_dummy_query(format, text, None, &[]));
            assert!(!implicit_dummy_query(
                format,
                text,
                Some("Query atoms are not supported yet; import was cancelled. Extra failure"),
                &[],
            ));
            assert!(!implicit_dummy_query(
                format,
                text,
                failure,
                &["Warning".into()],
            ));
        }
        assert!(!implicit_dummy_query("mol", "*", failure, &[]));
    }

    #[test]
    fn uranium_substitution_preserves_brackets_framing_and_cx_suffix() {
        for (text, replaced) in [
            ("C**", "C[U][U]"),
            ("[*:1].[2*].[*]", "[U:1].[2U].[U]"),
            ("  *C > [*:1] > [2*]C", "  [U]C > [U:1] > [2U]C"),
            (
                "*C>>*C|(0,0,;1,0,;0,0,;1,0,)|",
                "[U]C>>[U]C|(0,0,;1,0,;0,0,;1,0,)|",
            ),
            (
                "* |atomProp:0.key.*| title *",
                "[U] |atomProp:0.key.*| title *",
            ),
            ("CCO", "CCO"),
        ] {
            assert_eq!(with_uranium_atoms(text), replaced);
        }
    }

    #[test]
    fn implicit_dummy_query_requires_strict_acceptance_after_substitution() {
        let failure = Some("Query atoms are not supported yet; import was cancelled.");
        for text in [
            "* |(0,0,)|",
            "* |atomProp:0.molAtomMapNumber.1|",
            "C* |atomProp:0._UnknownStereo.bad|",
        ] {
            assert!(implicit_dummy_query("smiles", text, failure, &[]), "{text}");
        }
        for text in [
            "CCO |u:0|",
            "* |u:0|",
            "* |$Q_e$|",
            "* |$star_e$|",
            "CC* |rb:2:2|",
            "*~[H] |(0,0,;1,0,)|",
            "*.[CH] |(0,0,;1,0,)|",
            "*.F[Pt@SP1](Cl)(Br)I |(0,0,;1,0,;2,0,;3,0,;4,0,;5,0,)|",
        ] {
            assert!(
                !implicit_dummy_query("smiles", text, failure, &[]),
                "{text}"
            );
        }
        let reaction = Some("Query reaction atoms are not supported yet");
        assert!(implicit_dummy_query(
            "rsmi",
            "*C>>*C |(0,0,;1,0,;0,0,;1,0,)|",
            reaction,
            &[],
        ));
        assert!(!implicit_dummy_query(
            "rsmi",
            "*C>>*C |u:0,2|",
            reaction,
            &[],
        ));
    }
}
