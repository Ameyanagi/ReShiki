// SPDX-License-Identifier: MIT AND (MIT OR Apache-2.0)
// Origin: mixed; upstream-derived code with original nazuchem/ReShiki extensions.
//   openclatura 0.4.3, src/openclatura/assembly_utils.py @ 90dc5bcb4910c196122ec0405d06a32929a8f308.
// Copyright (c) 2026 lamalab-org
// Copyright (c) 2026 Ameyanagi and ReShiki contributors
// Changes: Openclatura grammar helpers with local compound-locant and stereo-hyphen fixes.
// Local contributions: MIT OR Apache-2.0; see NOTICE and PROVENANCE-nazuchem.toml.

//! Small grammar utilities shared by the plain-name formatters.
// Adapted from src/openclatura/assembly_utils.py (adapted; upstream function mapping not recorded; local fn parse_locant)
pub fn parse_locant(text: &str) -> (u8, usize, String) {
    let before = text.split('(').next().unwrap_or(text);
    let digit_count = before.bytes().take_while(u8::is_ascii_digit).count();
    if let Some((primary, secondary)) = before.split_once('^') {
        let n = secondary.bytes().take_while(u8::is_ascii_digit).count();
        if let (Ok(primary), Ok(secondary_number)) =
            (primary.parse::<usize>(), secondary[..n].parse::<usize>())
        {
            // P-14.3.1: compare composite locants by primary locant, then superscript.
            return (
                1,
                primary,
                format!("^{secondary_number:020}{}", &secondary[n..]),
            );
        }
    }
    if digit_count > 0
        && before[digit_count..]
            .chars()
            .all(|c| c.is_ascii_alphabetic() || c == '\'')
    {
        return (
            1,
            before[..digit_count].parse().unwrap_or(0),
            format!(
                "{}{}",
                before[digit_count..].replace('\'', ""),
                "'".repeat(before[digit_count..].matches('\'').count())
            ),
        );
    }
    let digits = text.trim_start_matches(|c: char| !c.is_ascii_digit());
    let digit_count = digits.bytes().take_while(u8::is_ascii_digit).count();
    if digit_count > 0 {
        return (
            1,
            digits[..digit_count].parse().unwrap_or(0),
            text.to_owned(),
        );
    }
    (0, 0, text.to_owned())
}
// Adapted from src/openclatura/assembly_utils.py (adapted; upstream function mapping not recorded; local fn needs_hyphen)
pub fn needs_hyphen(left: &str, right: &str) -> bool {
    if left.is_empty() || right.is_empty() {
        return false;
    }
    if right.starts_with(|c: char| c.is_ascii_digit())
        || left.ends_with(|c: char| c.is_ascii_digit())
    {
        return true;
    }
    if right
        .strip_prefix(['N', 'O', 'S', 'P'])
        .is_some_and(|tail| tail.trim_start_matches('\'').starts_with([',', '-']))
    {
        return true;
    }
    // A stereo descriptor such as `(1R,2E)` opening the right-hand side.
    right
        .strip_prefix('(')
        .and_then(|s| s.split_once(')'))
        .is_some_and(|(inner, _)| {
            inner.split(',').all(|term| {
                let letters = term.trim_start_matches(|c: char| c.is_ascii_digit());
                letters.len() < term.len()
                    && letters.ends_with(['R', 'S', 'E', 'Z', 'M', 'P'])
                    && letters
                        .chars()
                        .all(|c| c.is_ascii_alphabetic() || c == '\'')
            })
        })
}
// Ported from src/openclatura/assembly_utils.py (port; upstream function mapping not recorded; local fn is_fully_enclosed)
pub fn is_fully_enclosed(text: &str) -> bool {
    if !text.starts_with('(') || !text.ends_with(')') {
        return false;
    }
    let mut depth = 0_i32;
    for (i, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => (),
        }
        if depth == 0 && i < text.len() - 1 {
            return false;
        }
        if depth < 0 {
            return false;
        }
    }
    depth == 0
}

#[cfg(test)]
mod tests {
    use super::{needs_hyphen, parse_locant};

    // Original nazuchem code (not in openclatura 0.4.3): fn assembly_locants_compare_primes_and_superscripts_numerically
    #[test]
    fn assembly_locants_compare_primes_and_superscripts_numerically() {
        let mut primed = ["2'", "1'", "2", "1"];
        primed.sort_by_key(|locant| parse_locant(locant));
        assert_eq!(primed, ["1", "1'", "2", "2'"]);
        let mut fusion = ["3'a", "3a", "3'", "3"];
        fusion.sort_by_key(|locant| parse_locant(locant));
        assert_eq!(fusion, ["3", "3'", "3a", "3'a"]);
        let mut composite = ["2^1", "1^10", "1^2", "2^2"];
        composite.sort_by_key(|locant| parse_locant(locant));
        assert_eq!(composite, ["1^2", "1^10", "2^1", "2^2"]);
    }

    // Original nazuchem code (not in openclatura 0.4.3): fn only_a_leading_stereo_descriptor_takes_a_hyphen
    #[test]
    fn only_a_leading_stereo_descriptor_takes_a_hyphen() {
        assert!(needs_hyphen("methyl", "(1R,2E)-but-2-enyl"));
        assert!(needs_hyphen("methyl", "(5'E)-1'-butyl"));
        assert!(needs_hyphen("methyl", "(1M)-biphenyl-2-yl"));
        assert!(needs_hyphen("methyl", "(1'P,2R)-biphenyl-2-yl"));
        assert!(!needs_hyphen(
            "(cyclopentyl)",
            "(4-(((3R)-thiolan-3-yl)sulfanyl)pyridin-3-yl)"
        ));
        assert!(!needs_hyphen("methyl", "SO-x"));
    }
}
