//! Conservative typography for chemical names, without changing their text.
//!
//! The returned spans contain only inferred styling. All offsets refer to the
//! original UTF-8 string, and locant separators and ordinary name text stay roman.

use crate::typography::{Script, TextSpan, TextStyle};
use std::ops::Range;

const ITALIC: u8 = 1;
const SUPERSCRIPT: u8 = 2;

/// Infer italic descriptors and superscript element locants in a chemical name.
///
/// This deliberately recognizes a small set of nomenclature conventions rather
/// than interpreting the name. It never rewrites characters, including existing
/// Unicode superscripts, and omits unstyled ranges from the result.
pub fn name_markup(text: &str) -> Vec<TextSpan> {
    let mut marks = vec![0; text.len()];
    mark_parentheses(text, &mut marks);
    mark_prefixes_and_elements(text, &mut marks);
    mark_indicated_hydrogen(text, &mut marks);
    mark_fusion_descriptors(text, &mut marks);

    let mut spans: Vec<TextSpan> = vec![];
    for (start, character) in text.char_indices() {
        let mark = marks.get(start).copied().unwrap_or_default();
        if mark == 0 {
            continue;
        }
        let style = TextStyle {
            italic: mark & ITALIC != 0,
            script: if mark & SUPERSCRIPT != 0 {
                Script::Superscript
            } else {
                Script::Normal
            },
            ..TextStyle::default()
        };
        let end = start + character.len_utf8();
        if let Some(last) = spans.last_mut()
            && last.end == start
            && last.style == style
        {
            last.end = end;
        } else {
            spans.push(TextSpan { start, end, style });
        }
    }
    spans
}

fn mark_range(marks: &mut [u8], range: Range<usize>, style: u8) {
    for mark in marks.get_mut(range).unwrap_or_default() {
        *mark |= style;
    }
}

fn is_prime(character: char) -> bool {
    matches!(character, '\'' | '′' | '″' | '‴' | '⁗' | '’')
}

fn is_boundary(previous: Option<char>) -> bool {
    previous.is_none_or(|character| {
        !character.is_alphanumeric() && !is_prime(character) && character != '_'
    })
}

fn mark_parentheses(text: &str, marks: &mut [u8]) {
    let mut openings = vec![];
    for (offset, character) in text.char_indices() {
        if character == '(' {
            openings.push(offset + 1);
        } else if character == ')'
            && let Some(start) = openings.pop()
            && let Some(ranges) = stereo_set(&text[start..offset], start)
        {
            for range in ranges {
                mark_range(marks, range, ITALIC);
            }
        }
    }
}

fn stereo_set(content: &str, start: usize) -> Option<Vec<Range<usize>>> {
    let mut ranges = vec![];
    let mut offset = start;
    for component in content.split(',') {
        let trimmed = component.trim();
        let trimmed_start = offset + component.len() - component.trim_start().len();
        let mut descriptor = trimmed;
        let mut descriptor_start = trimmed_start;
        for prefix in ["rel", "rac"] {
            if descriptor == prefix {
                ranges.push(descriptor_start..descriptor_start + prefix.len());
                descriptor = "";
                break;
            }
            if let Some(remainder) = descriptor.strip_prefix(prefix)
                && let Some(remainder) = remainder.strip_prefix('-')
            {
                ranges.push(descriptor_start..descriptor_start + prefix.len());
                descriptor_start += prefix.len() + 1;
                descriptor = remainder;
                break;
            }
        }
        if descriptor.is_empty() {
            // An empty component is invalid; a standalone rel/rac is complete.
            if trimmed != "rel" && trimmed != "rac" {
                return None;
            }
        } else {
            let starred = descriptor.ends_with('*');
            let descriptor = descriptor.strip_suffix('*').unwrap_or(descriptor);
            let symbol = descriptor.chars().next_back()?;
            if !matches!(
                symbol,
                'R' | 'S' | 'E' | 'Z' | 'r' | 's' | 'M' | 'P' | 'ξ' | 'Ξ'
            ) {
                return None;
            }
            let locant = descriptor.strip_suffix(symbol)?;
            if !locant.is_empty() && !is_numeric_locant(locant) {
                return None;
            }
            let symbol_start = descriptor_start + locant.len();
            ranges.push(symbol_start..symbol_start + symbol.len_utf8() + usize::from(starred));
        }
        offset += component.len() + 1;
    }
    Some(ranges)
}

/// The byte length of a numeric locant at the start of `text`: digits, then
/// primes with at most one lowercase letter among them (4a, 5′a, 4a′).
fn locant_len(text: &str) -> Option<usize> {
    let digits = text.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let mut letter = false;
    let rest: usize = text[digits..]
        .chars()
        .take_while(|&c| {
            is_prime(c) || (c.is_ascii_lowercase() && !std::mem::replace(&mut letter, true))
        })
        .map(char::len_utf8)
        .sum();
    Some(digits + rest)
}

fn is_numeric_locant(text: &str) -> bool {
    locant_len(text) == Some(text.len())
}

fn mark_prefixes_and_elements(text: &str, marks: &mut [u8]) {
    // Stereodescriptors may also be listed with commas (2-cis,4-trans-...).
    const PREFIXES: &[(&str, &[char])] = &[
        ("cis", &['-', ',']),
        ("trans", &['-', ',']),
        ("endo", &['-', ',']),
        ("exo", &['-', ',']),
        ("syn", &['-', ',']),
        ("anti", &['-', ',']),
        ("erythro", &['-', ',']),
        ("threo", &['-', ',']),
        ("meso", &['-', ',']),
        ("retro", &['-', ',']),
        ("tert", &['-']),
        ("sec", &['-']),
        ("o", &['-']),
        ("m", &['-']),
        ("p", &['-']),
    ];
    const ELEMENTS: &[&str] = &["Se", "Te", "As", "Si", "N", "O", "S", "P", "B"];
    let mut previous = None;
    for (start, character) in text.char_indices() {
        if is_boundary(previous) {
            let remainder = &text[start..];
            for (prefix, ends) in PREFIXES {
                if remainder
                    .strip_prefix(prefix)
                    .is_some_and(|s| s.starts_with(*ends))
                {
                    mark_range(marks, start..start + prefix.len(), ITALIC);
                    break;
                }
            }
            for element in ELEMENTS {
                let Some(after_element) = remainder.strip_prefix(element) else {
                    continue;
                };
                if !after_element.chars().next().is_some_and(|next| {
                    matches!(next, ',' | '-')
                        || is_prime(next)
                        || next.is_ascii_digit()
                        || matches!(
                            next,
                            '⁰' | '¹' | '²' | '³' | '⁴' | '⁵' | '⁶' | '⁷' | '⁸' | '⁹'
                        )
                }) {
                    continue;
                }
                let end = start + element.len();
                mark_range(marks, start..end, ITALIC);
                let digits = after_element
                    .chars()
                    .take_while(char::is_ascii_digit)
                    .count();
                mark_range(marks, end..end + digits, SUPERSCRIPT);
                break;
            }
        }
        previous = Some(character);
    }
}

fn mark_indicated_hydrogen(text: &str, marks: &mut [u8]) {
    let mut previous = None;
    for (start, character) in text.char_indices() {
        // 1H-, 4aH-, 1H,3H-, 2(1H)- and (3H,7H,9H) lists.
        if is_boundary(previous)
            && let Some(locant) = locant_len(&text[start..])
            && text[start + locant..]
                .strip_prefix('H')
                .is_some_and(|after| after.starts_with(['-', ',', ')']))
        {
            let hydrogen = start + locant;
            mark_range(marks, hydrogen..hydrogen + 1, ITALIC);
        }
        previous = Some(character);
    }
}

fn mark_fusion_descriptors(text: &str, marks: &mut [u8]) {
    let mut openings = vec![];
    for (offset, character) in text.char_indices() {
        if character == '[' {
            openings.push(offset + 1);
        } else if character == ']'
            && let Some(start) = openings.pop()
            && let Some(ranges) = fusion_set(&text[start..offset], start)
        {
            for range in ranges {
                mark_range(marks, range, ITALIC);
            }
        }
    }
}

fn fusion_set(content: &str, start: usize) -> Option<Vec<Range<usize>>> {
    let mut ranges = vec![];
    let mut offset = start;
    for component in content.split(':') {
        let (locants, letters) = component.split_once('-')?;
        if !locants.chars().any(|character| character.is_ascii_digit())
            || !locants.chars().all(|character| {
                character.is_ascii_digit() || character == ',' || is_prime(character)
            })
        {
            return None;
        }
        let length = letters.chars().take_while(char::is_ascii_lowercase).count();
        if length == 0 || !letters.get(length..)?.chars().all(is_prime) {
            return None;
        }
        let letters_start = offset + locants.len() + 1;
        ranges.push(letters_start..letters_start + length);
        offset += component.len() + 1;
    }
    Some(ranges)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn styled(text: &str) -> Vec<(&str, Script)> {
        let spans = name_markup(text);
        for span in &spans {
            assert!(span.start < span.end);
            assert!(text.is_char_boundary(span.start));
            assert!(text.is_char_boundary(span.end));
            assert!(span.style.italic || span.style.script != Script::Normal);
            assert_eq!(
                span.style,
                TextStyle {
                    italic: span.style.italic,
                    script: span.style.script,
                    ..TextStyle::default()
                }
            );
        }
        for pair in spans.windows(2) {
            assert!(pair[0].end <= pair[1].start);
            assert!(pair[0].end != pair[1].start || pair[0].style != pair[1].style);
        }
        spans
            .iter()
            .map(|span| (&text[span.start..span.end], span.style.script))
            .collect()
    }

    fn italic(text: &str, expected: &[&str]) {
        assert_eq!(
            styled(text),
            expected
                .iter()
                .map(|part| (*part, Script::Normal))
                .collect::<Vec<_>>()
        );
        assert!(name_markup(text).iter().all(|span| span.style.italic));
    }

    #[test]
    fn requested_name_examples() {
        italic("(2R,3S)-…", &["R", "S"]);
        italic("N,N-dimethylmethanamine", &["N", "N"]);
        italic("quinolin-2(1H)-one", &["H"]);
        italic("furo[2,3-b]pyridine", &["b"]);
        italic("tert-butyl…", &["tert"]);
        italic("(E)-but-2-ene", &["E"]);
        let text = "N1-ethyl-N2-methylethane-1,2-diamine";
        assert_eq!(
            styled(text),
            vec![
                ("N", Script::Normal),
                ("1", Script::Superscript),
                ("N", Script::Normal),
                ("2", Script::Superscript),
            ]
        );
        for span in name_markup(text) {
            assert_eq!(span.style.italic, span.style.script == Script::Normal);
        }
    }

    #[test]
    fn parenthesized_stereo_sets_keep_locants_and_punctuation_roman() {
        italic(
            "(R,S,E,Z,r,s,M,P,R*,ξ,Ξ)-name",
            &["R", "S", "E", "Z", "r", "s", "M", "P", "R*", "ξ", "Ξ"],
        );
        italic("(rel-1R,2S)-(rac)-name", &["rel", "R", "S", "rac"]);
        italic("(rac-2aR, 12′S)-name", &["rac", "R", "S"]);
        italic("(1′R,5′aS)-name", &["R", "S"]);
        italic("((2R,3S))-name", &["R", "S"]);
    }

    #[test]
    fn stereo_prefixes_may_be_comma_separated() {
        italic("2-cis,4-trans-octa-2,4-diene", &["cis", "trans"]);
        italic("cis,cis-muconic acid", &["cis", "cis"]);
        assert!(name_markup("tert,name").is_empty());
    }

    #[test]
    fn prefixes_require_a_word_boundary_and_following_hyphen() {
        for prefix in [
            "cis", "trans", "endo", "exo", "syn", "anti", "erythro", "threo", "meso", "retro",
            "tert", "sec", "o", "m", "p",
        ] {
            italic(&format!("{prefix}-name"), &[prefix]);
            italic(&format!("name-{prefix}-name"), &[prefix]);
            assert!(name_markup(&format!("name{prefix}-name")).is_empty());
            assert!(name_markup(prefix).is_empty());
        }
    }

    #[test]
    fn element_locants_are_standalone_and_preserve_primes() {
        for element in ["N", "O", "S", "P", "Se", "Te", "As", "B", "Si"] {
            italic(&format!("{element},{element}-name"), &[element, element]);
            italic(
                &format!("({element}′,{element}'-name)"),
                &[element, element],
            );
            assert!(name_markup(&format!("word{element}-name")).is_empty());
            assert!(name_markup(&format!("{element}ame")).is_empty());
        }
        italic("N″,N‴,N⁗,N’-name", &["N", "N", "N", "N"]);
        assert!(name_markup("Na-name Os-name Sixty-name NH-name").is_empty());
    }

    #[test]
    fn ascii_element_locant_digits_are_superscript_only() {
        let text = "Se12,N3′-name";
        assert_eq!(
            styled(text),
            vec![
                ("Se", Script::Normal),
                ("12", Script::Superscript),
                ("N", Script::Normal),
                ("3", Script::Superscript)
            ]
        );
        let spans = name_markup(text);
        assert_eq!((spans[0].start, spans[0].end), (0, 2));
        assert_eq!((spans[1].start, spans[1].end), (2, 4));
        assert!(!spans[1].style.italic);
        assert!(!spans[3].style.italic);
        italic("N¹-ethyl-N²-methyl", &["N", "N"]);
        italic(
            "N⁰,N¹,N²,N³,N⁴,N⁵,N⁶,N⁷,N⁸,N⁹-name",
            &["N", "N", "N", "N", "N", "N", "N", "N", "N", "N"],
        );
    }

    #[test]
    fn indicated_hydrogen_requires_numeric_locants() {
        italic("1H-imidazole", &["H"]);
        italic("name-12H-name", &["H"]);
        italic("name(12H)", &["H"]);
        italic("tetrahydro-1H,3H-furo[3,4-c]furan", &["H", "H", "c"]);
        italic("1H-purine-2,6,8(3H,7H,9H)-trione", &["H", "H", "H", "H"]);
        italic("4aH-carbazole 5′H-name", &["H", "H"]);
        for text in [
            "H-name",
            "NH-name",
            "CH-name",
            "name1H-name",
            "1H",
            "name(1Hname)",
            "(H)",
        ] {
            assert!(name_markup(text).is_empty(), "{text}");
        }
    }

    #[test]
    fn fusion_letter_runs_merge_but_punctuation_stays_roman() {
        italic("furo[2,3-b:4′,5′-cd′]pyridine", &["b", "cd"]);
        for text in [
            "furo[2,3]pyridine",
            "furo[a-b]pyridine",
            "furo[2,3-broken word]pyridine",
            "furo[2,3-b",
            "furo2,3-b]pyridine",
        ] {
            assert!(name_markup(text).is_empty(), "{text}");
        }
    }

    #[test]
    fn ordinary_names_and_unrecognized_stereo_remain_plain() {
        for text in [
            "",
            "ethanol",
            "cyclohexane",
            "isopropyl",
            "2-methylpropan-2-ol",
            "sulfanyl",
            "iso-propyl",
            "cyclo-hexane",
            "cisplatin",
            "mesomeric",
            "Naphthalene",
            "(helloR)-name",
            "(2R,word)-name",
            "R-name E-name",
        ] {
            assert!(name_markup(text).is_empty(), "{text}");
        }
        italic("(2R)-name (unclosedS", &["R"]);
    }

    #[test]
    fn unicode_offsets_are_original_utf8_boundaries() {
        let text = "β-(2ξ,3Ξ)-N12-α";
        assert_eq!(
            styled(text),
            vec![
                ("ξ", Script::Normal),
                ("Ξ", Script::Normal),
                ("N", Script::Normal),
                ("12", Script::Superscript)
            ]
        );
        let spans = name_markup(text);
        assert_eq!((spans[0].start, spans[0].end), (5, 7));
        assert_eq!((spans[1].start, spans[1].end), (9, 11));
    }
}
