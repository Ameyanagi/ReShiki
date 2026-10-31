//! `PreProcessor` and the normalization portion of `StringTools`, OPSIN 2.9.0.
use crate::ParsingError;

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
        replaced.push(ch);
        i += 1;
    }
    normalize_representation(&replaced)
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
