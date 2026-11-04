//! `PreProcessor` and the normalization portion of `StringTools`, OPSIN 2.9.0.
//! Phane superscript handling: upstream main, 42e87ac6.
use crate::ParsingError;
use regex::Regex;
use std::sync::LazyLock;

static JAVA_DECIMAL_DIGIT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\A\p{Nd}\z").expect("Java decimal digit category"));
static ASCII_XI_PREFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:\A|[^A-Za-z0-9'])([1-9][0-9]*(?:'*[a-gA-C]?|[a-gA-C]'+))?\z")
        .expect("ASCII xi descriptor prefix")
});

// The pinned grammar accepts cage descriptors as prefixes, while the suffix
// spelling shares its locant with both the descriptor and suffix.
static CAGE_NAME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)bicyclo\[(?:2\.2\.1|3\.2\.1)\]").expect("Supported bicyclic cage")
});
static CAGE_LOCANT_DESCRIPTORS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)[0-9]+(?:-(?:endo|exo|syn|anti))?(?:,[0-9]+(?:-(?:endo|exo|syn|anti))?)*")
        .expect("Cage locant descriptors")
});

fn normalize_cage_descriptors(input: &str) -> String {
    input
        .split(' ')
        .map(|word| {
            if !CAGE_NAME.is_match(word) {
                return word.to_string();
            }
            // A descriptor belongs to the innermost group containing it. In
            // particular, a cage in another substituent must not enable this
            // rewrite or receive the resulting stereochemistry prefix.
            let mut scopes = vec![(0, None, Vec::new())];
            let mut scope_at = vec![0; word.len()];
            let mut stack = vec![(0, ' ')];
            for (index, ch) in word.char_indices() {
                scope_at[index] = stack.last().unwrap().0;
                match ch {
                    '(' | '[' | '{' => {
                        let scope = scopes.len();
                        scopes.push((index + ch.len_utf8(), None, Vec::new()));
                        stack.push((scope, ch));
                    }
                    ')' | ']' | '}' => {
                        let opener = match ch {
                            ')' => '(',
                            ']' => '[',
                            _ => '{',
                        };
                        if stack.last().unwrap().1 != opener {
                            return word.to_string();
                        }
                        stack.pop();
                    }
                    _ => {}
                }
            }
            if stack.len() != 1 {
                return word.to_string();
            }
            for cage in CAGE_NAME.find_iter(word) {
                scopes[scope_at[cage.start()]].1.get_or_insert(cage.start());
            }
            let mut replacements = Vec::new();
            for terms in CAGE_LOCANT_DESCRIPTORS.find_iter(word) {
                let scope = &mut scopes[scope_at[terms.start()]];
                let Some(cage_start) = scope.1 else {
                    continue;
                };
                if !word[terms.end()..].starts_with('-')
                    || (terms.start() != scope.0 && !word[..terms.start()].ends_with('-'))
                    || (terms.start() < cage_start && !terms.as_str().contains(','))
                {
                    continue;
                }
                let mut locants = Vec::new();
                let mut defined = Vec::new();
                for term in terms.as_str().split(',') {
                    if let Some((locant, _)) = term.split_once('-') {
                        locants.push(locant);
                        defined.push(term);
                    } else {
                        locants.push(term);
                    }
                }
                if defined.is_empty() {
                    continue;
                }
                scope.2.extend(defined);
                replacements.push((terms.start(), terms.end(), locants.join(",")));
            }
            for (start, _, descriptors) in scopes {
                if !descriptors.is_empty() {
                    replacements.push((start, start, format!("({})-", descriptors.join(","))));
                }
            }
            replacements.sort_by_key(|replacement| (replacement.0, replacement.1));
            let mut rewritten = String::new();
            let mut cursor = 0;
            for (start, end, replacement) in replacements {
                rewritten.push_str(&word[cursor..start]);
                rewritten.push_str(&replacement);
                cursor = end;
            }
            rewritten.push_str(&word[cursor..]);
            rewritten
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn preprocess(name: &str) -> Result<String, ParsingError> {
    // Java String.trim removes only characters <= U+0020.
    let name = name.trim_matches(|c| c <= '\u{20}');
    if name.is_empty() {
        return Err(ParsingError("Input chemical name was blank!".into()));
    }
    let chars: Vec<char> = name.chars().collect();
    let mut replaced = String::with_capacity(name.len());
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '$'
            && let Some(replacement) = chars.get(i + 1).and_then(|ch| dollar_greek(*ch))
        {
            replaced.push_str(replacement);
            i += 2;
            continue;
        }
        if matches!(ch, '.' | '&') {
            let terminator = if ch == '.' { '.' } else { ';' };
            let limit = (i + 9).min(chars.len());
            if let Some(end) = (i + 1..limit).find(|&j| chars[j] == terminator) {
                let enclosed = chars[i + 1..end].iter().collect::<String>().to_lowercase();
                if let Some(replacement) = enclosed_greek(&enclosed, ch == '.') {
                    replaced.push_str(replacement);
                    i = end + 1;
                    continue;
                }
            }
        }
        if let Some(digit) = superscript_digit_to_digit(ch) {
            if i > 0 && superscript_digit_to_digit(chars[i - 1]).is_some() {
                replaced.push(digit);
            } else if i > 0 && is_java_digit(chars[i - 1]) {
                replaced.push('^');
                replaced.push(digit);
            } else {
                replaced.push(ch);
            }
            i += 1;
            continue;
        }
        if ch == '<' {
            let remaining = &chars[i..];
            if starts_with_sup_tag(remaining, "<sup>") {
                replaced.push('^');
                i += 5;
                continue;
            }
            if starts_with_sup_tag(remaining, "</sup>") {
                i += 6;
                continue;
            }
        }
        if matches!(ch, 's' | 'S')
            && i + 4 < chars.len()
            && chars[i + 1..i + 5]
                .iter()
                .collect::<String>()
                .eq_ignore_ascii_case("ulph")
        {
            replaced.push_str("sulf");
            i += 5;
            continue;
        }
        // Reserve Xi/xI for the Unicode descriptors introduced during
        // normalization. Normalize descriptor tokens, preserving words such
        // as OXIME. A numeric locant also permits hyphenless 5XIcholestane.
        if matches!(ch, 'x' | 'X')
            && chars.get(i + 1).is_some_and(|c| matches!(c, 'i' | 'I'))
            && let Some(prefix) = ASCII_XI_PREFIX.captures(&replaced)
            && (prefix.get(1).is_some()
                || chars.get(i + 2).is_none_or(|c| !c.is_ascii_alphanumeric()))
        {
            replaced.push_str("xi");
            i += 2;
            continue;
        }
        replaced.push(ch);
        i += 1;
    }
    Ok(normalize_cage_descriptors(&normalize_representation(
        &replaced,
    )?))
}

fn superscript_digit_to_digit(ch: char) -> Option<char> {
    Some(match ch {
        '\u{2070}' => '0',
        '\u{b9}' => '1',
        '\u{b2}' => '2',
        '\u{b3}' => '3',
        '\u{2074}'..='\u{2079}' => char::from_u32('4' as u32 + ch as u32 - 0x2074)?,
        _ => return None,
    })
}

fn starts_with_sup_tag(remaining: &[char], tag: &str) -> bool {
    remaining.len() >= tag.len()
        && remaining
            .iter()
            .zip(tag.chars())
            .all(|(&actual, expected)| {
                // Java regionMatches(true) uses Unicode simple case conversion.
                // Long s is the only non-ASCII equivalent in these two tag names.
                actual.eq_ignore_ascii_case(&expected) || (actual == '\u{17f}' && expected == 's')
            })
}

fn is_java_digit(ch: char) -> bool {
    // Character.isDigit(charAt(i - 1)) observes a single UTF-16 code unit.
    ch as u32 <= 0xffff && JAVA_DECIMAL_DIGIT.is_match(ch.encode_utf8(&mut [0; 4]))
}

fn normalize_representation(input: &str) -> Result<String, ParsingError> {
    let mut normalized = String::with_capacity(input.len());
    for (index, ch) in input.char_indices() {
        match ch {
            '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' => {
                if !normalized.ends_with(' ') {
                    normalized.push(' ');
                }
            }
            '`' => normalized.push('\''),
            '"' => normalized.push_str("''"),
            '\u{2014}' => {
                if index + ch.len_utf8() < input.len() {
                    if !normalized.ends_with(' ') {
                        normalized.push(' ');
                    }
                    normalized.push_str("- ");
                } else {
                    normalized.push('-');
                }
            }
            ch if ch.is_ascii() => {
                if ch > '\u{1f}' {
                    normalized.push(ch);
                }
            }
            ch => {
                let replacement = non_ascii(ch)
                    .ok_or_else(|| ParsingError(format!("Unrecognised unicode character: {ch}")))?;
                normalized.push_str(if normalized.ends_with(' ') {
                    replacement.strip_prefix(' ').unwrap_or(replacement)
                } else {
                    replacement
                });
            }
        }
    }
    Ok(normalized)
}

fn dollar_greek(ch: char) -> Option<&'static str> {
    Some(match ch {
        'a' => "alpha",
        'b' => "beta",
        'g' => "gamma",
        'd' => "delta",
        'e' => "epsilon",
        'l' => "lambda",
        _ => return None,
    })
}

fn enclosed_greek(text: &str, dot: bool) -> Option<&'static str> {
    Some(match text {
        "alpha" => "alpha",
        "beta" => "beta",
        "gamma" => "gamma",
        "delta" => "delta",
        "epsilon" => "epsilon",
        "zeta" => "zeta",
        "eta" => "eta",
        "lambda" => "lambda",
        "xi" => "xi",
        "omega" => "omega",
        "a" if dot => "alpha",
        "b" if dot => "beta",
        "g" if dot => "gamma",
        "d" if dot => "delta",
        "e" if dot => "epsilon",
        "l" if dot => "lambda",
        "x" if dot => "xi",
        "fwdarw" if dot => "->",
        _ => return None,
    })
}

include!("preprocess_characters.rs");

#[cfg(test)]
mod tests {
    #[test]
    fn whitespace_normalization_preserves_one_leading_space() {
        assert_eq!(super::normalize_representation("\t \u{a0}").unwrap(), " ");
    }
}
