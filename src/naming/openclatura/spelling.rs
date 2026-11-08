// SPDX-License-Identifier: MIT AND (MIT OR Apache-2.0)
// Origin: mixed
// Source: openclatura | src/openclatura/rules/elision.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | src/openclatura/suffix_stack.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: ReShiki | src/naming/openclatura/spelling.rs | 5f559fd482e237307a589dfcb7ea333cee0a6e13
// Copyright (c) 2026 lamalab-org
// Copyright (c) 2026 Ameyanagi and ReShiki contributors
// Changes: Refactored openclatura terminal-e/anion-suffix spelling into structured Rust composition, with original local regression tests.
// Local contributions: MIT OR Apache-2.0; see NOTICE and PROVENANCE-nazuchem.toml.

//! Text-only suffix composition at parent and derivative boundaries.
use super::rules::multipliers;

/// Append a word or a `-locants-word` suffix, eliding a final `e` before a
/// vowel or `y`. The locant run may contain letters, primes or added hydrogen,
/// but never a hyphen.
///
/// P-16.7.1(a), P-72.5.3, P-72.6.3 and P-74.1.1: for example,
/// `pent-1-yn-1-id-5-oate`, `cyclohexan-1-ide-4-sulfonate` and `methanidyl`.
// Provenance: fn join | adapted
// Source: openclatura | src/openclatura/rules/elision.py::elide_terminal_e
// Source: openclatura | src/openclatura/suffix_stack.py::module scope (mapping uncertain)
pub(super) fn join(head: &str, tail: &str) -> String {
    let word = tail
        .strip_prefix('-')
        .and_then(|run| run.split_once('-'))
        .map_or(tail, |(_, word)| word);
    let head = if word.starts_with(['a', 'e', 'i', 'o', 'u', 'y']) {
        head.strip_suffix('e').unwrap_or(head)
    } else {
        head
    };
    format!("{head}{tail}")
}

/// Render locanted anion operations or the bare suffix for an empty run.
// Provenance: fn anion_suffix | adapted
// Source: openclatura | src/openclatura/rules/elision.py::module scope (mapping uncertain)
// Source: openclatura | src/openclatura/suffix_stack.py::_operation_text
// Source: openclatura | src/openclatura/suffix_stack.py::suffix_operation_spelling
pub(super) fn anion_suffix(locants: &[String]) -> String {
    let suffix = if locants.len() > 1 {
        format!("{}ide", multipliers::basic(locants.len()))
    } else {
        "ide".into()
    };
    if !locants.is_empty() {
        format!("-{}-{suffix}", locants.join(","))
    } else {
        suffix
    }
}

#[cfg(test)]
// Provenance: mod tests | original
// Source: ReShiki | src/naming/openclatura/spelling.rs::tests
mod tests {
    use super::*;

    #[test]
    fn suffix_join_uses_the_immediately_following_word() {
        for (head, tail, expected) in [
            ("methane", "ide", "methanide"),
            ("methane", "diide", "methanediide"),
            ("piperazine", "-1,4-diide", "piperazine-1,4-diide"),
            ("azanediide", "yl", "azanediidyl"),
            ("pyrazole", "-2-ium-1-ide", "pyrazol-2-ium-1-ide"),
            ("-2-ylium", "-1-ide", "-2-ylium-1-ide"),
            ("-1-ide", "-3-uide", "-1-id-3-uide"),
            ("-1,2-diide", "-3-uide", "-1,2-diid-3-uide"),
            ("-1-ide", "-3,4-diuide", "-1-ide-3,4-diuide"),
            ("propan-3-ide", "-1,2-diyl", "propan-3-ide-1,2-diyl"),
            ("pent-1-yn-1-ide", "-5-oate", "pent-1-yn-1-id-5-oate"),
            ("pyridine", "-1(2H)-ide", "pyridin-1(2H)-ide"),
            ("1H-pyrrol-1-ide", "-3-yl", "1H-pyrrol-1-id-3-yl"),
            ("naphthalene", "-4a'-ide", "naphthalen-4a'-ide"),
            (
                "pyrrolidin-1-ide",
                "-2,4-dione",
                "pyrrolidin-1-ide-2,4-dione",
            ),
            ("methane", "", "methane"),
        ] {
            assert_eq!(join(head, tail), expected, "{head} + {tail}");
        }
    }

    #[test]
    fn anion_suffix_formats_locants_and_multiplicity() {
        for (locants, expected) in [
            (vec!["1"], "-1-ide"),
            (vec!["1", "4"], "-1,4-diide"),
            (vec!["1", "3", "5"], "-1,3,5-triide"),
            (vec![], "ide"),
        ] {
            let locants = locants.into_iter().map(str::to_owned).collect::<Vec<_>>();
            assert_eq!(anion_suffix(&locants), expected);
        }
    }
}
