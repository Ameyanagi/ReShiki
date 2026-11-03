//! Presentation-only typography for chemical names. Engine names stay unchanged.
use super::{Script, TextFormat, TextSpan};
use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameRun {
    Italic,
    SmallCaps,
    Superscript,
    Subscript,
}

/// Display text with ordered, nonoverlapping UTF-8 byte ranges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameMarkup {
    pub text: String,
    pub runs: Vec<(Range<usize>, NameRun)>,
}

impl NameMarkup {
    /// Replace name spans while preserving the caption's base style and layout.
    pub fn apply_to(&self, format: &mut TextFormat) {
        format.spans = self
            .runs
            .iter()
            .map(|(range, run)| {
                let mut style = format.style.clone();
                style.formula = false;
                match run {
                    NameRun::Italic => style.italic = true,
                    NameRun::SmallCaps => {
                        style.italic = false;
                        style.script = Script::Normal;
                        style.size_pt = (style.size_pt * 0.8).max(4.);
                    }
                    NameRun::Superscript => style.script = Script::Superscript,
                    NameRun::Subscript => {
                        style.italic = false;
                        style.script = Script::Subscript;
                    }
                }
                TextSpan {
                    start: range.start,
                    end: range.end,
                    style,
                }
            })
            .collect();
    }

    /// Unicode display for widgets and formatted plain-text copy. Commas have
    /// no Unicode superscript equivalent, so remain ordinary commas.
    pub fn unicode(&self) -> Self {
        let mut text = String::new();
        let mut runs = vec![];
        let mut end = 0;
        for (range, run) in &self.runs {
            text.push_str(segment(&self.text, end..range.start));
            let start = text.len();
            for c in segment(&self.text, range.clone()).chars() {
                text.push(if *run == NameRun::Superscript {
                    match c {
                        '0' => '⁰',
                        '1' => '¹',
                        '2' => '²',
                        '3' => '³',
                        '4' => '⁴',
                        '5' => '⁵',
                        '6' => '⁶',
                        '7' => '⁷',
                        '8' => '⁸',
                        '9' => '⁹',
                        _ => c,
                    }
                } else if *run == NameRun::Subscript {
                    match c {
                        '0' => '₀',
                        '1' => '₁',
                        '2' => '₂',
                        '3' => '₃',
                        '4' => '₄',
                        '5' => '₅',
                        '6' => '₆',
                        '7' => '₇',
                        '8' => '₈',
                        '9' => '₉',
                        'a' => 'ₐ',
                        _ => c,
                    }
                } else {
                    c
                });
            }
            runs.push((start..text.len(), *run));
            end = range.end;
        }
        text.push_str(after(&self.text, end));
        Self { text, runs }
    }
}

fn before(text: &str, at: usize) -> &str {
    text.split_at(at).0
}

fn after(text: &str, at: usize) -> &str {
    text.split_at(at).1
}

fn segment(text: &str, range: Range<usize>) -> &str {
    before(after(text, range.start), range.end - range.start)
}

const SUPERSCRIPTS: [char; 10] = ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'];
const PREFIXES: &[&str] = &[
    "cis", "trans", "endo", "exo", "syn", "anti", "erythro", "threo", "meso", "retro", "sn", "ent",
    "epi", "rac", "rel", "myo", "scyllo", "chiro", "gluco", "manno", "galacto", "allo",
];
const ELEMENTS: &[&str] = &["N", "O", "S", "P", "Se", "Te", "As", "B", "Si"];

fn is_prime(c: char) -> bool {
    matches!(c, '\'' | '′' | '″' | '‴' | '⁗' | '’')
}

fn superscript_digit(c: char) -> Option<char> {
    SUPERSCRIPTS
        .iter()
        .position(|&digit| digit == c)
        .map(|i| char::from(b'0' + i as u8))
}

fn subscript_digit(c: char) -> Option<char> {
    "₀₁₂₃₄₅₆₇₈₉"
        .chars()
        .position(|digit| digit == c)
        .map(|i| char::from(b'0' + i as u8))
}

fn isotope_digit(c: char) -> Option<char> {
    if c.is_ascii_digit() {
        Some(c)
    } else {
        superscript_digit(c).or_else(|| subscript_digit(c))
    }
}

fn digit_run(text: &str) -> (String, usize) {
    let mut digits = String::new();
    let mut bytes = 0;
    for c in text.chars() {
        let Some(digit) = isotope_digit(c) else { break };
        digits.push(digit);
        bytes += c.len_utf8();
    }
    (digits, bytes)
}

fn nuclide_symbol(mass: &str, symbol: &str) -> bool {
    mass.parse()
        .is_ok_and(|mass| reshiki_chemistry::isotopes::is_known_nuclide(symbol, mass))
}

/// Boughton descriptors follow the parent and can themselves carry locants.
/// Requiring the descriptor to reach the end (or another isotope suffix)
/// keeps ordinary indicated hydrogen such as `-1H-indole` untouched.
fn boughton_nuclide(text: &str) -> Option<(NameMarkup, usize)> {
    let (mass, mass_bytes) = digit_run(text);
    let (symbol, symbol_bytes) = if mass.is_empty() {
        (text.starts_with('d').then_some("d")?, 1)
    } else {
        let tail = after(text, mass_bytes);
        if !tail.starts_with(|c: char| c.is_ascii_uppercase()) {
            return None;
        }
        let length = 1 + usize::from(tail.as_bytes().get(1).is_some_and(u8::is_ascii_lowercase));
        let symbol = before(tail, length);
        if !nuclide_symbol(&mass, symbol) {
            return None;
        }
        (symbol, length)
    };
    let (count, count_bytes) = digit_run(after(text, mass_bytes + symbol_bytes));
    if symbol == "d" && count.is_empty() {
        return None;
    }
    let consumed = mass_bytes + symbol_bytes + count_bytes;
    let tail = after(text, consumed);
    if !tail.is_empty()
        && !tail.strip_prefix('-').is_some_and(|tail| {
            tail.starts_with(|c: char| {
                c.is_ascii_digit() || superscript_digit(c).is_some() || c == 'd'
            })
        })
    {
        return None;
    }
    let mut runs = vec![];
    if !mass.is_empty() {
        runs.push((0..mass.len(), NameRun::Superscript));
    }
    if !count.is_empty() {
        let start = mass.len() + symbol.len();
        runs.push((start..start + count.len(), NameRun::Subscript));
    }
    Some((
        NameMarkup {
            text: format!("{mass}{symbol}{count}"),
            runs,
        },
        consumed,
    ))
}

/// Recognize a complete parenthesised nuclide descriptor before treating its
/// H as indicated hydrogen. Nuclide symbols are roman (P-81.2), while their
/// mass and count use separate, exportable script spans.
fn isotope_group(text: &str) -> Option<(NameMarkup, usize)> {
    let body = text.strip_prefix('(')?;
    let close = body.find(')')?;
    let body = before(body, close);
    let normalized: String = body
        .chars()
        .map(|c| {
            superscript_digit(c)
                .or_else(|| subscript_digit(c))
                .unwrap_or(c)
        })
        .collect();
    let mut runs = vec![];
    let mut start = 0;
    while start < normalized.len() {
        let mut found = None;
        for (relative, _) in after(&normalized, start).char_indices() {
            let at = start + relative;
            let tail = after(&normalized, at);
            let mass_len = tail.bytes().take_while(u8::is_ascii_digit).count();
            if mass_len == 0 {
                continue;
            }
            let prefix = segment(&normalized, start..at);
            if !prefix.is_empty()
                && !prefix.strip_suffix('-').is_some_and(|locants| {
                    locants
                        .split(',')
                        .all(|locant| numeric_locant(locant) || ELEMENTS.contains(&locant))
                })
            {
                continue;
            }
            let symbol_tail = after(tail, mass_len);
            let Some(first) = symbol_tail.chars().next() else {
                continue;
            };
            if !first.is_ascii_uppercase() {
                continue;
            }
            let symbol_len = 1 + usize::from(
                symbol_tail
                    .as_bytes()
                    .get(1)
                    .is_some_and(u8::is_ascii_lowercase),
            );
            let symbol = before(symbol_tail, symbol_len);
            if !nuclide_symbol(before(tail, mass_len), symbol) {
                continue;
            }
            let count_len = after(symbol_tail, symbol_len)
                .bytes()
                .take_while(u8::is_ascii_digit)
                .count();
            let end = at + mass_len + symbol_len + count_len;
            if end != normalized.len() && !after(&normalized, end).starts_with(',') {
                continue;
            }
            found = Some((at, mass_len, symbol_len, count_len, end));
            break;
        }
        let (at, mass_len, symbol_len, count_len, end) = found?;
        runs.push((at + 1..at + mass_len + 1, NameRun::Superscript));
        if count_len > 0 {
            let count = at + mass_len + symbol_len + 1;
            runs.push((count..count + count_len, NameRun::Subscript));
        }
        start = end + usize::from(end < normalized.len());
        if start == normalized.len() && end < normalized.len() {
            return None;
        }
    }
    if runs.is_empty() {
        return None;
    }
    Some((
        NameMarkup {
            text: format!("({normalized})"),
            runs,
        },
        close + 2,
    ))
}

fn isotope_at(name: &str, at: usize) -> Option<(NameMarkup, usize)> {
    let rest = after(name, at);
    if before(name, at).ends_with('-')
        && let Some(isotope) = boughton_nuclide(rest)
    {
        return Some(isotope);
    }
    let indicated_hydrogen = before(name, at).ends_with(|c: char| c.is_ascii_digit())
        && rest
            .strip_prefix('(')
            .and_then(|body| body.split_once(')'))
            .is_some_and(|(body, tail)| {
                tail.starts_with('-')
                    && body
                        .split(',')
                        .all(|locant| locant.strip_suffix('H').is_some_and(numeric_locant))
            });
    if indicated_hydrogen {
        None
    } else {
        isotope_group(rest)
    }
}

/// Roman nuclide symbols in normalized display text.
fn nuclide_symbol_ranges(name: &str) -> Vec<Range<usize>> {
    let mut symbols = vec![];
    let mut at = 0;
    while let Some(c) = after(name, at).chars().next() {
        if let Some((isotope, consumed)) = isotope_at(name, at) {
            for (range, run) in &isotope.runs {
                if *run == NameRun::Superscript {
                    let length = after(&isotope.text, range.end)
                        .bytes()
                        .take_while(u8::is_ascii_alphabetic)
                        .count();
                    symbols.push(at + range.end..at + range.end + length);
                }
            }
            at += consumed;
        } else {
            at += c.len_utf8();
        }
    }
    symbols
}

fn boundary_before(text: &str, at: usize) -> bool {
    before(text, at)
        .chars()
        .next_back()
        .is_none_or(|c| !c.is_alphanumeric() && !is_prime(c))
}

fn boundary_after(text: &str, at: usize) -> bool {
    after(text, at)
        .chars()
        .next()
        .is_none_or(|c| !c.is_alphanumeric())
}

/// Numeric locant, optionally carrying a fusion suffix and primes.
fn numeric_locant(text: &str) -> bool {
    let digits = text.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return false;
    }
    let rest = after(text, digits).trim_start_matches(is_prime);
    let letters = rest.bytes().take_while(u8::is_ascii_lowercase).count();
    let tail = after(rest, letters);
    let tail = if let Some(interior) = tail.strip_prefix('^') {
        let digits = interior.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            return false;
        }
        after(interior, digits)
    } else {
        let digits = tail
            .chars()
            .take_while(|&c| superscript_digit(c).is_some() || (letters > 0 && c.is_ascii_digit()))
            .map(char::len_utf8)
            .sum();
        after(tail, digits)
    };
    tail.chars().all(is_prime)
}

fn preceding_locant(text: &str, at: usize) -> bool {
    let start = before(text, at)
        .char_indices()
        .rev()
        .find(|(_, c)| {
            !c.is_ascii_alphanumeric()
                && !is_prime(*c)
                && superscript_digit(*c).is_none()
                && *c != '^'
        })
        .map_or(0, |(i, c)| i + c.len_utf8());
    let locant = segment(text, start..at);
    boundary_before(text, start) && numeric_locant(locant)
}

/// Normalize presentation encodings, retaining baseline digits and letters so
/// every export can use the same script spans instead of font-specific glyphs.
fn display_text(name: &str) -> NameMarkup {
    let mut stereo = vec![];
    if name.contains('ₐ') {
        parenthesized_stereo(name, &mut stereo);
    }
    let mut markup = NameMarkup {
        text: String::new(),
        runs: vec![],
    };
    let mut i = 0;
    while i < name.len() {
        let rest = after(name, i);
        let Some(c) = rest.chars().next() else { break };
        if let Some((isotope, consumed)) = isotope_at(name, i) {
            let offset = markup.text.len();
            markup.text.push_str(&isotope.text);
            markup.runs.extend(
                isotope
                    .runs
                    .into_iter()
                    .map(|(range, run)| (range.start + offset..range.end + offset, run)),
            );
            i += consumed;
            continue;
        }
        let lambda = ["lambda^", "lambda", "λ^", "λ"]
            .into_iter()
            .find_map(|prefix| rest.strip_prefix(prefix).map(|tail| (prefix.len(), tail)));
        if let Some((prefix, tail)) = lambda
            && (boundary_before(name, i) || preceding_locant(name, i))
        {
            let digits = tail.bytes().take_while(u8::is_ascii_digit).count();
            if digits > 0 && boundary_after(name, i + prefix + digits) {
                markup.text.push('λ');
                let start = markup.text.len();
                markup.text.push_str(before(tail, digits));
                markup
                    .runs
                    .push((start..markup.text.len(), NameRun::Superscript));
                i += prefix + digits;
                continue;
            }
        }
        if let Some(tail) = rest.strip_prefix("^{")
            && preceding_locant(name, i)
            && let Some(close) = tail.find('}')
        {
            let body = before(tail, close);
            if body
                .split(',')
                .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
                && boundary_after(name, i + close + 3)
            {
                let start = markup.text.len();
                markup.text.push_str(body);
                markup
                    .runs
                    .push((start..markup.text.len(), NameRun::Superscript));
                i += close + 3;
                continue;
            }
        }
        if let Some(tail) = rest.strip_prefix('^')
            && preceding_locant(name, i)
        {
            let digits = tail.bytes().take_while(u8::is_ascii_digit).count();
            let end = i + digits + 1;
            if digits > 0
                && (boundary_after(name, end)
                    || [
                        "H", "R", "S", "r", "s", "Ra", "Sa", "ra", "sa", "Rₐ", "Sₐ", "rₐ", "sₐ",
                    ]
                    .into_iter()
                    .any(|descriptor| {
                        after(name, end)
                            .strip_prefix(descriptor)
                            .is_some_and(|tail| {
                                tail.is_empty() || tail.starts_with(['-', ')', ',', '*'])
                            })
                    }))
            {
                let start = markup.text.len();
                markup.text.push_str(before(tail, digits));
                markup
                    .runs
                    .push((start..markup.text.len(), NameRun::Superscript));
                i += digits + 1;
                continue;
            }
        }
        // An element locant (N¹-) normalizes like its ASCII spelling (N1-).
        let word = before(name, i).trim_end_matches(|c: char| c.is_ascii_alphabetic());
        if superscript_digit(c).is_some()
            && (preceding_locant(name, i)
                || before(name, i).strip_suffix('λ').is_some_and(|prefix| {
                    prefix
                        .chars()
                        .next_back()
                        .is_none_or(|c| !c.is_alphabetic())
                })
                || (ELEMENTS.contains(&segment(name, word.len()..i))
                    && boundary_before(name, word.len())
                    && rest
                        .trim_start_matches(|c| superscript_digit(c).is_some())
                        .starts_with(|c: char| matches!(c, ',' | '-') || is_prime(c))))
        {
            let start = markup.text.len();
            while i < name.len() {
                let Some(digit) = after(name, i).chars().next() else {
                    break;
                };
                if let Some(digit) = superscript_digit(digit) {
                    markup.text.push(digit);
                } else if digit == ','
                    && after(name, i + 1)
                        .chars()
                        .next()
                        .is_some_and(|c| superscript_digit(c).is_some())
                {
                    markup.text.push(',');
                } else {
                    break;
                }
                i += digit.len_utf8();
            }
            markup
                .runs
                .push((start..markup.text.len(), NameRun::Superscript));
            continue;
        }
        if matches!(c, '\'' | '’') && prime_after_locant(name, i) {
            let bytes = rest.len() - rest.trim_start_matches(['\'', '’']).len();
            let count = before(rest, bytes).chars().count();
            markup.text.extend(std::iter::repeat_n('‴', count / 3));
            match count % 3 {
                1 => markup.text.push('′'),
                2 => markup.text.push('″'),
                _ => (),
            }
            i += bytes;
            continue;
        }
        if c == 'H'
            && matches!(after(name, i + 1).chars().next(), Some('-' | ')' | ','))
            && preceding_locant(name, i)
        {
            // Recognize interior locants before their script encoding is removed.
            markup
                .runs
                .push((markup.text.len()..markup.text.len() + 1, NameRun::Italic));
        }
        markup.text.push(
            if c == 'ₐ'
                && stereo
                    .iter()
                    .any(|(range, run)| range.start == i && *run == NameRun::Subscript)
            {
                'a'
            } else {
                c
            },
        );
        i += c.len_utf8();
    }
    markup
}

fn prime_after_locant(text: &str, at: usize) -> bool {
    if preceding_locant(text, at) {
        return true;
    }
    let start = before(text, at)
        .char_indices()
        .rev()
        .find(|(_, c)| !c.is_ascii_alphabetic())
        .map_or(0, |(i, c)| i + c.len_utf8());
    let word = segment(text, start..at);
    (ELEMENTS.contains(&word) && boundary_before(text, start))
        || (word.len() == 1
            && word.bytes().all(|b| b.is_ascii_lowercase())
            && before(text, start)
                .chars()
                .next_back()
                .is_some_and(|c| matches!(c, '[' | '-')))
}

fn stereo_item(text: &str) -> Option<Range<usize>> {
    let trimmed = text.trim();
    let offset = text.len() - text.trim_start().len();
    for descriptor in [
        "R*", "S*", "rel", "rac", "Ra", "Sa", "ra", "sa", "Rₐ", "Sₐ", "rₐ", "sₐ", "R", "S", "E",
        "Z", "r", "s", "M", "P", "ξ", "Ξ",
    ]
    .into_iter()
    .chain(PREFIXES.iter().copied())
    {
        if let Some(locant) = trimmed.strip_suffix(descriptor)
            && (locant.is_empty() || numeric_locant(locant))
        {
            let start = offset + locant.len();
            return Some(start..start + descriptor.trim_end_matches(['a', 'ₐ']).len());
        }
    }
    None
}

fn parenthesized_stereo(text: &str, runs: &mut Vec<(Range<usize>, NameRun)>) {
    let mut starts = vec![];
    for (i, c) in text.char_indices() {
        match c {
            '(' => starts.push(i + 1),
            ')' => {
                if let Some(start) = starts.pop() {
                    let body = segment(text, start..i);
                    let mut offset = start;
                    let mut descriptors = vec![];
                    let valid = body.split([',', '/']).all(|part| {
                        // A leading rel-/rac- is already italic as a prefix word.
                        let lead = part.len() - part.trim_start().len();
                        let skip = ["rel-", "rac-"]
                            .into_iter()
                            .find(|prefix| after(part, lead).starts_with(prefix))
                            .map_or(0, |prefix| lead + prefix.len());
                        let range = stereo_item(after(part, skip));
                        if let Some(range) = &range {
                            let end = offset + skip + range.end;
                            descriptors.push((offset + skip + range.start..end, NameRun::Italic));
                            if let Some(a) = after(text, end).chars().next()
                                && matches!(a, 'a' | 'ₐ')
                            {
                                descriptors.push((end..end + a.len_utf8(), NameRun::Subscript));
                            }
                        }
                        offset += part.len() + 1;
                        range.is_some()
                    });
                    if valid {
                        runs.extend(descriptors);
                    }
                }
            }
            _ => (),
        }
    }
}

fn fusion_letters(text: &str, runs: &mut Vec<(Range<usize>, NameRun)>) {
    let mut starts = vec![];
    for (i, c) in text.char_indices() {
        match c {
            '[' => starts.push(i + 1),
            ']' => {
                if let Some(start) = starts.pop() {
                    let mut offset = start;
                    let mut letters: Vec<(Range<usize>, NameRun)> = vec![];
                    let valid = segment(text, start..i).split(':').all(|part| {
                        let (edge, edge_start, numbered) =
                            if let Some((locants, edge)) = part.split_once('-') {
                                if !locants.split(',').all(numeric_locant) {
                                    return false;
                                }
                                (edge, offset + locants.len() + 1, true)
                            } else {
                                (part, offset, false)
                            };
                        let mut position = edge_start;
                        let valid = edge.split(',').all(|word| {
                            let mut previous = None;
                            let mut count = 0;
                            let valid = word.char_indices().all(|(at, c)| {
                                if !c.is_ascii_lowercase() {
                                    return previous.is_some() && is_prime(c);
                                }
                                if previous.is_some_and(|previous| c as u32 != previous as u32 + 1)
                                {
                                    return false;
                                }
                                let start = position + at;
                                if let Some((range, _)) = letters.last_mut()
                                    && range.end == start
                                {
                                    range.end += 1;
                                } else {
                                    letters.push((start..start + 1, NameRun::Italic));
                                }
                                previous = Some(c);
                                count += 1;
                                true
                            }) && count > 0
                                && (numbered || count == 1);
                            position += word.len() + 1;
                            valid
                        });
                        offset += part.len() + 1;
                        valid
                    });
                    if valid {
                        runs.extend(letters);
                    }
                }
            }
            _ => (),
        }
    }
}

/// Recognize chemical-name tokens, never substrings of ordinary words.
pub fn name_markup(name: &str) -> NameMarkup {
    let mut markup = display_text(name);
    let text = &markup.text;
    let mut i = 0;
    while i < text.len() {
        let Some(c) = after(text, i).chars().next() else {
            break;
        };
        if c.is_ascii_alphabetic() {
            let n = after(text, i)
                .bytes()
                .take_while(u8::is_ascii_alphabetic)
                .count();
            let word = segment(text, i..i + n);
            let next = after(text, i + n).chars().next();
            if boundary_before(text, i) {
                let rest = after(text, i + n);
                let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
                let locant = rest.trim_start_matches(|c: char| {
                    c.is_ascii_digit() || superscript_digit(c).is_some()
                });
                let prefix = PREFIXES.contains(&word)
                    || (word == "glycero"
                        && ["D-", "L-", "DL-"].into_iter().any(|prefix| {
                            before(text, i)
                                .strip_suffix(prefix)
                                .is_some_and(|head| boundary_before(text, head.len()))
                        }));
                if (prefix
                    && (next == Some('-')
                        || (next.is_none() && before(text, i).ends_with('-'))
                        || rest
                            .strip_prefix(',')
                            .is_some_and(|tail| tail.starts_with(char::is_alphanumeric))))
                    || (["tert", "sec", "o", "m", "p"].contains(&word) && next == Some('-'))
                {
                    markup.runs.push((i..i + n, NameRun::Italic));
                } else if ELEMENTS.contains(&word)
                    && locant.starts_with(|c: char| matches!(c, ',' | '-') || is_prime(c))
                {
                    markup.runs.push((i..i + n, NameRun::Italic));
                    if digits > 0 {
                        markup
                            .runs
                            .push((i + n..i + n + digits, NameRun::Superscript));
                    }
                } else if ["D", "L", "DL"].contains(&word) && matches!(next, Some('-' | ',')) {
                    markup.runs.push((i..i + n, NameRun::SmallCaps));
                }
            }
            i += n;
        } else {
            i += c.len_utf8();
        }
    }
    parenthesized_stereo(text, &mut markup.runs);
    fusion_letters(text, &mut markup.runs);
    let nuclides = nuclide_symbol_ranges(text);
    markup.runs.retain(|(range, run)| {
        *run != NameRun::Italic
            || !nuclides
                .iter()
                .any(|symbol| range.start < symbol.end && symbol.start < range.end)
    });
    markup.runs.sort_by_key(|(range, _)| range.start);
    markup.runs.dedup();
    markup
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_ranges(markup: &NameMarkup) {
        let mut end = 0;
        for (range, _) in &markup.runs {
            assert!(range.start < range.end);
            assert!(end <= range.start);
            assert!(markup.text.is_char_boundary(range.start));
            assert!(markup.text.is_char_boundary(range.end));
            end = range.end;
        }
    }

    fn parts(markup: &NameMarkup, kind: NameRun) -> Vec<&str> {
        assert_ranges(markup);
        markup
            .runs
            .iter()
            .filter(|(_, run)| *run == kind)
            .map(|(range, _)| &markup.text[range.clone()])
            .collect()
    }

    fn styled(markup: &NameMarkup) -> Vec<(&str, NameRun)> {
        assert_ranges(markup);
        markup
            .runs
            .iter()
            .map(|(range, run)| (&markup.text[range.clone()], *run))
            .collect()
    }

    fn italic(name: &str, expected: &[&str]) {
        let markup = name_markup(name);
        assert_eq!(
            styled(&markup),
            expected
                .iter()
                .map(|part| (*part, NameRun::Italic))
                .collect::<Vec<_>>(),
            "{name}"
        );
    }

    #[test]
    fn italic_name_tokens_keep_punctuation_and_locant_digits_roman() {
        for (name, expected) in [
            ("(2R,3S)-(E)-N,N-dimethyl", vec!["R", "S", "E", "N", "N"]),
            ("(2Ξ,3R)-3-chlorobutan-2-ol", vec!["Ξ", "R"]),
            ("(1ξ,4ξ)-cyclohexane-1,4-diol", vec!["ξ", "ξ"]),
            ("((3aR*,8bS*)-tert-butyl)", vec!["R*", "S*", "tert"]),
            (
                "(r,s,M,P,ξ,Ξ,rel,rac)-hexane",
                vec!["r", "s", "M", "P", "ξ", "Ξ", "rel", "rac"],
            ),
            ("quinolin-2(1H)-one", vec!["H"]),
            ("4aH-furo[3,4-b:5,6-b']pyridine", vec!["H", "b", "b"]),
            ("benzo[b]phosphindole", vec!["b"]),
            ("furo[2,3-b]pyridine", vec!["b"]),
            ("tetrahydro-1H,3H-furo[3,4-c]furan", vec!["H", "H", "c"]),
            ("trans,trans-muconic acid", vec!["trans", "trans"]),
            ("(3a^1R,4S)-indene", vec!["R", "S"]),
            ("(3a¹R)-indene", vec!["R"]),
            ("N1,N1-dimethyl", vec!["N", "N"]),
            (
                "O-ethyl-Se2-Te,As-B-Si-N′-S-oxide",
                vec!["O", "Se", "Te", "As", "B", "Si", "N", "S"],
            ),
        ] {
            let markup = name_markup(name);
            assert_eq!(parts(&markup, NameRun::Italic), expected, "{name}");
            let mut format = TextFormat::default();
            markup.apply_to(&mut format);
            format.validate(&markup.text).unwrap();
        }
        for token in PREFIXES {
            let markup = name_markup(&format!("{token}-butyl"));
            assert_eq!(parts(&markup, NameRun::Italic), [*token]);
        }
        for token in ["o", "m", "p"] {
            let markup = name_markup(&format!("{token}-xylene"));
            assert_eq!(parts(&markup, NameRun::Italic), [token]);
        }
    }

    #[test]
    fn ordinary_words_and_non_name_carets_are_untouched() {
        for name in [
            "methanol",
            "isopropyl",
            "cyclohexane",
            "bicyclo[2.2.1]heptane",
            "Sn-chloride",
            "Ne-Ar",
            "Sc-compound",
            "DLactate",
            "cisplatin",
            "tertiary amine",
            "Naphthalene",
            "(Some prose)",
            "[methyl]",
            "someone's compound",
            "x^2",
            "x^{1,5}",
            "3a^foo",
            "lambda^x",
            "lambdax",
            "polylambda^5",
            "polylambda5",
            "poly5lambda^6",
            "polyλ5",
            "poly5λ6",
            "µN'",
            "0^{1,,5}",
            "0^{1,5",
            "3a^1word",
            "1H2O",
            "vitamin B12",
            "N2",
            "S8 ring",
            "anti, syn and",
        ] {
            let markup = name_markup(name);
            assert_eq!(markup.text, name, "{name}");
            assert!(markup.runs.is_empty(), "{name}: {:?}", markup.runs);
        }
    }

    #[test]
    fn prime_groups_and_small_capitals_are_display_only() {
        let raw = "N,N'-D-gluco-L-threo-DL-alanine-1,1''-biphenyl-2'''-3''''-4'''''-5''''''";
        let markup = name_markup(raw);
        assert_eq!(
            markup.text,
            "N,N′-D-gluco-L-threo-DL-alanine-1,1″-biphenyl-2‴-3‴′-4‴″-5‴‴"
        );
        assert_eq!(parts(&markup, NameRun::SmallCaps), ["D", "L", "DL"]);
        for (name, descriptor) in [("L-alanine", "L"), ("D-glucose", "D")] {
            assert_eq!(parts(&name_markup(name), NameRun::SmallCaps), [descriptor]);
        }
        assert_eq!(name_markup("1,1'-biphenyl").text, "1,1′-biphenyl");
    }

    #[test]
    fn right_single_quotes_normalize_only_after_locants() {
        for (name, expected) in [
            ("N’-methyl", "N′-methyl"),
            ("1’", "1′"),
            ("1’,1’’-biphenyl", "1′,1″-biphenyl"),
            ("β-N’'-D-gluco", "β-N″-D-gluco"),
            ("(1’R,2S)-name", "(1′R,2S)-name"),
            ("someone’s compound", "someone’s compound"),
        ] {
            let markup = name_markup(name);
            assert_eq!(markup.text, expected, "{name}");
            assert_eq!(markup, name_markup(expected), "{name}");
            assert_ranges(&markup);
        }
    }

    #[test]
    fn superscript_encodings_share_caption_and_unicode_ranges() {
        for (name, text, superscript, unicode) in [
            (
                "hexafluoro-lambda6-sulfane",
                "hexafluoro-λ6-sulfane",
                "6",
                "hexafluoro-λ⁶-sulfane",
            ),
            ("1lambda4-thiopyran", "1λ4-thiopyran", "4", "1λ⁴-thiopyran"),
        ] {
            let markup = name_markup(name);
            assert_eq!(markup.text, text, "{name}");
            assert_eq!(
                parts(&markup, NameRun::Superscript),
                [superscript],
                "{name}"
            );
            assert_eq!(markup.unicode().text, unicode, "{name}");
        }
        let markup = name_markup("(R)-5lambda^6-tricyclo[3.2.2.0^{1,5}]-3a^12-N'-D-gluco");
        assert_eq!(markup.text, "(R)-5λ6-tricyclo[3.2.2.01,5]-3a12-N′-D-gluco");
        assert_eq!(parts(&markup, NameRun::Superscript), ["6", "1,5", "12"]);
        let unicode = markup.unicode();
        assert_eq!(unicode.text, "(R)-5λ⁶-tricyclo[3.2.2.0¹,⁵]-3a¹²-N′-D-gluco");
        assert_eq!(parts(&unicode, NameRun::Italic), ["R", "N", "gluco"]);
        assert_eq!(parts(&unicode, NameRun::SmallCaps), ["D"]);
        assert_eq!(parts(&unicode, NameRun::Superscript), ["⁶", "¹,⁵", "¹²"]);
        assert_eq!(unicode.unicode(), unicode);
        assert_eq!(name_markup(&unicode.text), markup);
        assert_eq!(name_markup("9a¹²").unicode().text, "9a¹²");
        let interior = name_markup("3a¹'-4a²H-furo[2,3-b]pyridine");
        assert_eq!(interior.unicode().text, "3a¹′-4a²H-furo[2,3-b]pyridine");
        assert_eq!(parts(&interior, NameRun::Italic), ["H", "b"]);
        assert_eq!(name_markup(&interior.unicode().text), interior);
        assert_eq!(name_markup("3a^1H-").unicode().text, "3a¹H-");
    }

    #[test]
    fn numeric_primary_locants_share_caption_and_unicode_ranges() {
        for (name, text, superscripts, unicode) in [
            (
                "1^1,2^1:2^4,3^1-terphenyl",
                "11,21:24,31-terphenyl",
                vec!["1", "1", "4", "1"],
                "1¹,2¹:2⁴,3¹-terphenyl",
            ),
            (
                "2^5-phenyl-1^1,2^1:2^4,3^1-terphenyl",
                "25-phenyl-11,21:24,31-terphenyl",
                vec!["5", "1", "1", "4", "1"],
                "2⁵-phenyl-1¹,2¹:2⁴,3¹-terphenyl",
            ),
            (
                "[1^1,2^1:2^4,3^1-terphenyl]-1^4-ol",
                "[11,21:24,31-terphenyl]-14-ol",
                vec!["1", "1", "4", "1", "4"],
                "[1¹,2¹:2⁴,3¹-terphenyl]-1⁴-ol",
            ),
            ("1^4-yl", "14-yl", vec!["4"], "1⁴-yl"),
            ("1^2H", "12H", vec!["2"], "1²H"),
        ] {
            let markup = name_markup(name);
            assert_eq!(markup.text, text, "{name}");
            assert_eq!(parts(&markup, NameRun::Superscript), superscripts, "{name}");
            assert!(parts(&markup, NameRun::Italic).is_empty(), "{name}");
            let display = markup.unicode();
            assert_eq!(display.text, unicode, "{name}");
            assert_ranges(&display);
            assert_eq!(name_markup(&display.text), markup, "{name}");
            let mut format = TextFormat::default();
            markup.apply_to(&mut format);
            format.validate(&markup.text).unwrap();
            assert!(
                format
                    .spans
                    .iter()
                    .all(|span| span.style.script == Script::Superscript)
            );
        }
    }

    #[test]
    fn applying_runs_replaces_spans_and_preserves_base_style_and_layout() {
        let markup = name_markup("N-D-gluco-1lambda^5");
        let mut format = TextFormat::default();
        format.style.family = "Example".into();
        format.style.bold = true;
        format.style.italic = true;
        format.style.formula = true;
        format.style.script = Script::Subscript;
        format.style.size_pt = 12.;
        format.width_pt = Some(80.);
        let base = format.style.clone();
        markup.apply_to(&mut format);
        assert_eq!(format.style, base);
        assert_eq!(format.width_pt, Some(80.));
        assert!(
            format.spans.iter().all(|span| !span.style.formula
                && span.style.bold
                && span.style.family == "Example")
        );
        let small = format.at(markup.text.find('D').unwrap());
        assert!(!small.italic);
        assert_eq!(small.script, Script::Normal);
        assert!((small.size_pt - 9.6).abs() < 0.001);
        assert_eq!(format.at(markup.text.len() - 1).script, Script::Superscript);
        let first = format.clone();
        markup.apply_to(&mut format);
        assert_eq!(format, first);
        format.validate(&markup.text).unwrap();
        format.style.size_pt = 4.;
        markup.apply_to(&mut format);
        format.validate(&markup.text).unwrap();
    }

    #[test]
    fn isotope_mass_and_count_are_separate_roman_script_spans() {
        for (name, unicode, masses, counts) in [
            ("(14C)methane", "(¹⁴C)methane", vec!["14"], vec![]),
            ("(2H1)methane", "(²H₁)methane", vec!["2"], vec!["1"]),
            (
                "(S)-(1-2H1)ethan-1-ol",
                "(S)-(1-²H₁)ethan-1-ol",
                vec!["2"],
                vec!["1"],
            ),
            (
                "(1,1-2H2,18O)methanol",
                "(1,1-²H₂,¹⁸O)methanol",
                vec!["2", "18"],
                vec!["2"],
            ),
            (
                "(N-2H1,15N)methanamine",
                "(N-²H₁,¹⁵N)methanamine",
                vec!["2", "15"],
                vec!["1"],
            ),
        ] {
            let markup = name_markup(name);
            assert_eq!(markup.text, name);
            assert_eq!(parts(&markup, NameRun::Superscript), masses);
            assert_eq!(parts(&markup, NameRun::Subscript), counts);
            assert_eq!(markup.unicode().text, unicode);
            assert_eq!(name_markup(unicode), markup);
            let mut format = TextFormat::default();
            markup.apply_to(&mut format);
            format.validate(&markup.text).unwrap();
            for (range, run) in &markup.runs {
                let style = format.at(range.start);
                if matches!(run, NameRun::Superscript | NameRun::Subscript) {
                    assert!(!style.italic);
                }
            }
        }
        assert_eq!(
            parts(&name_markup("(2H1)methane"), NameRun::Italic),
            Vec::<&str>::new()
        );
        assert_eq!(
            parts(&name_markup("(S)-(N-2H1)ethanamide"), NameRun::Italic),
            ["S", "N"]
        );
        assert_eq!(name_markup("methanol-d4").unicode().text, "methanol-d₄");
        assert_eq!(name_markup("methanol-d₄"), name_markup("methanol-d4"));
        assert_eq!(
            name_markup("ethan-1-ol-2-13C").unicode().text,
            "ethan-1-ol-2-¹³C"
        );
        assert_eq!(
            name_markup("ethane-1-13C-1,1-d2").unicode().text,
            "ethane-1-¹³C-1,1-d₂"
        );
        for text in ["(13C prose)", "(1,2)", "(18Ox)", "(13C,)", "1H2O"] {
            assert!(name_markup(text).runs.is_empty(), "{text}");
        }
    }

    #[test]
    fn nuclide_symbols_remain_roman_beside_stereodescriptors() {
        for name in ["(34S)", "(31P)", "(1-2H1,34S)"] {
            let markup = name_markup(name);
            assert_eq!(
                parts(&markup, NameRun::Italic),
                Vec::<&str>::new(),
                "{name}"
            );
            let mut format = TextFormat::default();
            markup.apply_to(&mut format);
            format.validate(&markup.text).unwrap();
            assert_eq!(name_markup(&markup.unicode().text), markup);
        }
        assert_eq!(
            parts(&name_markup("(2S)-(34S)butane"), NameRun::Italic),
            ["S"]
        );
        assert_eq!(
            parts(&name_markup("(2S)butane"), NameRun::Superscript),
            Vec::<&str>::new()
        );
        assert_eq!(
            parts(&name_markup("(2P)-(31P)phosphane"), NameRun::Italic),
            ["P"]
        );
    }

    #[test]
    fn requested_name_examples() {
        italic("(2R,3S)-…", &["R", "S"]);
        italic("N,N-dimethylmethanamine", &["N", "N"]);
        italic("quinolin-2(1H)-one", &["H"]);
        italic("furo[2,3-b]pyridine", &["b"]);
        italic("tert-butyl…", &["tert"]);
        italic("(E)-but-2-ene", &["E"]);
        let markup = name_markup("N1-ethyl-N2-methylethane-1,2-diamine");
        assert_eq!(
            styled(&markup),
            vec![
                ("N", NameRun::Italic),
                ("1", NameRun::Superscript),
                ("N", NameRun::Italic),
                ("2", NameRun::Superscript),
            ]
        );
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
    fn axial_descriptors_have_italic_letters_and_upright_subscript_a() {
        for (name, unicode, expected) in [
            (
                "(Ra)-penta-2,3-diene",
                "(Rₐ)-penta-2,3-diene",
                vec![("R", NameRun::Italic), ("a", NameRun::Subscript)],
            ),
            (
                "(1Ra)-name",
                "(1Rₐ)-name",
                vec![("R", NameRun::Italic), ("a", NameRun::Subscript)],
            ),
            (
                "(Sa,2R)-name",
                "(Sₐ,2R)-name",
                vec![
                    ("S", NameRun::Italic),
                    ("a", NameRun::Subscript),
                    ("R", NameRun::Italic),
                ],
            ),
            (
                "β-(rel-1′ra, 2sa/M,P)-name",
                "β-(rel-1′rₐ, 2sₐ/M,P)-name",
                vec![
                    ("rel", NameRun::Italic),
                    ("r", NameRun::Italic),
                    ("a", NameRun::Subscript),
                    ("s", NameRun::Italic),
                    ("a", NameRun::Subscript),
                    ("M", NameRun::Italic),
                    ("P", NameRun::Italic),
                ],
            ),
        ] {
            let markup = name_markup(name);
            assert_eq!(markup.text, name);
            assert_eq!(styled(&markup), expected, "{name}");
            let displayed = markup.unicode();
            assert_eq!(displayed.text, unicode, "{name}");
            assert_ranges(&displayed);
            assert_eq!(displayed.unicode(), displayed);
            assert_eq!(name_markup(unicode), markup);
            let mut format = TextFormat::default();
            format.style.italic = true;
            markup.apply_to(&mut format);
            format.validate(&markup.text).unwrap();
            for (range, run) in &markup.runs {
                let style = format.at(range.start);
                assert_eq!(style.italic, *run == NameRun::Italic, "{name}");
                if *run == NameRun::Subscript {
                    assert_eq!(style.script, Script::Subscript);
                }
            }
        }
        let interior = name_markup("(3a^1Ra)-name");
        assert_eq!(interior.unicode().text, "(3a¹Rₐ)-name");
        assert_eq!(name_markup(&interior.unicode().text), interior);
        assert_eq!(name_markup("(3a^1Rₐ)-name"), interior);
        assert_eq!(
            name_markup("N'-(Sₐ,2R)-name"),
            name_markup("N′-(Sa,2R)-name")
        );
        for name in [
            "(Ra,word)-name",
            "(helloSa)-name",
            "Ra-name",
            "Rₐ-name",
            "(Rₐ,word)-name",
        ] {
            let markup = name_markup(name);
            assert_eq!(markup.text, name, "{name}");
            assert!(markup.runs.is_empty(), "{name}");
        }
    }

    #[test]
    fn stereo_prefixes_may_be_comma_separated() {
        italic("2-cis,4-trans-octa-2,4-diene", &["cis", "trans"]);
        italic("cis,cis-muconic acid", &["cis", "cis"]);
        assert!(name_markup("tert,name").runs.is_empty());
    }

    #[test]
    fn prefixes_require_a_word_boundary_and_following_hyphen() {
        for prefix in [
            "cis", "trans", "endo", "exo", "syn", "anti", "erythro", "threo", "meso", "retro",
            "tert", "sec", "o", "m", "p",
        ] {
            italic(&format!("{prefix}-name"), &[prefix]);
            italic(&format!("name-{prefix}-name"), &[prefix]);
            assert!(name_markup(&format!("name{prefix}-name")).runs.is_empty());
            assert!(name_markup(prefix).runs.is_empty());
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
            assert!(name_markup(&format!("word{element}-name")).runs.is_empty());
            assert!(name_markup(&format!("{element}ame")).runs.is_empty());
        }
        italic("N″,N‴,N⁗,N’-name", &["N", "N", "N", "N"]);
        assert!(
            name_markup("Na-name Os-name Sixty-name NH-name")
                .runs
                .is_empty()
        );
    }

    #[test]
    fn ascii_element_locant_digits_are_superscript_only() {
        let markup = name_markup("Se12,N3′-name");
        assert_eq!(
            styled(&markup),
            vec![
                ("Se", NameRun::Italic),
                ("12", NameRun::Superscript),
                ("N", NameRun::Italic),
                ("3", NameRun::Superscript)
            ]
        );
        assert_eq!(markup.runs[0].0, 0..2);
        assert_eq!(markup.runs[1].0, 2..4);
        let unicode = name_markup("N¹-ethyl-N²-methyl");
        assert_eq!(unicode, name_markup("N1-ethyl-N2-methyl"));
        assert_eq!(parts(&unicode, NameRun::Italic), ["N", "N"]);
        let markup = name_markup("N⁰,N¹,N²,N³,N⁴,N⁵,N⁶,N⁷,N⁸,N⁹-name");
        assert_eq!(markup.text, "N0,N1,N2,N3,N4,N5,N6,N7,N8,N9-name");
        assert_eq!(parts(&markup, NameRun::Italic), ["N"; 10]);
        assert_eq!(
            parts(&markup, NameRun::Superscript),
            ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"]
        );
        // Format Chemical Name reparses its own Unicode display.
        let markup = name_markup("N1-ethyl-N2-methylethane-1,2-diamine");
        assert_eq!(name_markup(&markup.unicode().text), markup);
        for text in ["(tert)-butyl", "(sec)"] {
            assert!(name_markup(text).runs.is_empty(), "{text}");
        }
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
            assert!(name_markup(text).runs.is_empty(), "{text}");
        }
    }

    #[test]
    fn fusion_letter_runs_merge_but_punctuation_stays_roman() {
        italic("furo[2,3-b:4′,5′-cd′]pyridine", &["b", "cd"]);
        italic("[1,2-a]", &["a"]);
        italic("[2,1-b:3,4-b']", &["b", "b"]);
        italic("[1,2,3-de]", &["de"]);
        italic("[1,2,3,4-def′]", &["def"]);
        italic("[1,2,3-d'e]", &["d", "e"]);
        italic("[1,2,3-d'e']", &["d", "e"]);
        italic("[1,2,3,4-de'f']", &["de", "f"]);
        italic("[1,2,3,4-d′e″f‴]", &["d", "e", "f"]);
        for text in [
            "furo[2,3]pyridine",
            "furo[a-b]pyridine",
            "furo[2,3-broken word]pyridine",
            "furo[2,3-b",
            "furo2,3-b]pyridine",
            "[1,1'-biphenyl]",
            "[2,2'-bipyridin]",
            "[1,1'-bi(cyclohexan)]",
            "[1,1'-binaphthalene]",
            "[1,2-df]",
            "[1,2-d'f']",
        ] {
            assert!(name_markup(text).runs.is_empty(), "{text}");
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
            assert!(name_markup(text).runs.is_empty(), "{text}");
        }
        italic("(2R)-name (unclosedS", &["R"]);
    }

    #[test]
    fn unicode_offsets_are_display_utf8_boundaries() {
        let markup = name_markup("β-(2ξ,3Ξ)-N12-α");
        assert_eq!(
            styled(&markup),
            vec![
                ("ξ", NameRun::Italic),
                ("Ξ", NameRun::Italic),
                ("N", NameRun::Italic),
                ("12", NameRun::Superscript)
            ]
        );
        assert_eq!(markup.runs[0].0, 5..7);
        assert_eq!(markup.runs[1].0, 9..11);
    }

    #[test]
    fn isotopes_share_caption_scripts_with_normalized_display_text() {
        for (text, expected) in [
            (
                "(S)-(1-2H1,34S)ethan-1-ol",
                vec![
                    ("S", NameRun::Italic),
                    ("2", NameRun::Superscript),
                    ("1", NameRun::Subscript),
                    ("34", NameRun::Superscript),
                ],
            ),
            ("(31P)phosphane", vec![("31", NameRun::Superscript)]),
            (
                "(1,1-2H2,18O)methanol",
                vec![
                    ("2", NameRun::Superscript),
                    ("2", NameRun::Subscript),
                    ("18", NameRun::Superscript),
                ],
            ),
            ("methanol-d4", vec![("4", NameRun::Subscript)]),
            ("ethan-1-ol-2-13C", vec![("13", NameRun::Superscript)]),
            (
                "β-(²H₁)methane",
                vec![("2", NameRun::Superscript), ("1", NameRun::Subscript)],
            ),
        ] {
            let markup = name_markup(text);
            assert_eq!(styled(&markup), expected, "{text}");
        }
    }

    #[test]
    fn repeated_element_locants_style_digits_as_superscript() {
        assert_eq!(
            styled(&name_markup("N1,N1-dimethyl")),
            [
                ("N", NameRun::Italic),
                ("1", NameRun::Superscript),
                ("N", NameRun::Italic),
                ("1", NameRun::Superscript),
            ]
        );
    }

    #[test]
    fn unnumbered_fusion_edge_remains_italic() {
        italic("benzo[b]thiophene", &["b"]);
    }

    #[test]
    fn racemic_prefix_remains_italic_outside_stereo_parentheses() {
        italic("rac-(2R)-butan-2-ol", &["rac", "R"]);
    }

    #[test]
    fn additional_configuration_prefixes_remain_italic() {
        italic("myo-inositol", &["myo"]);
    }

    #[test]
    fn carbohydrate_configuration_remains_small_caps() {
        assert_eq!(
            styled(&name_markup("D-glucose")),
            [("D", NameRun::SmallCaps)]
        );
    }

    #[test]
    fn glycero_is_italic_only_after_a_carbohydrate_configuration() {
        for configuration in ["D", "L", "DL"] {
            assert_eq!(
                styled(&name_markup(&format!(
                    "{configuration}-glycero-D-gluco-heptose"
                ))),
                [
                    (configuration, NameRun::SmallCaps),
                    ("glycero", NameRun::Italic),
                    ("D", NameRun::SmallCaps),
                    ("gluco", NameRun::Italic),
                ]
            );
        }
        italic("sn-glycero-3-phosphocholine", &["sn"]);
        for name in ["glycero-heptose", "wordD-glycero-heptose"] {
            assert!(name_markup(name).runs.is_empty(), "{name}");
        }
    }

    #[test]
    fn terminal_prefix_requires_a_preceding_hyphen() {
        assert!(name_markup("cis").runs.is_empty());
        assert_eq!(
            styled(&name_markup("name-D-gluco")),
            [("D", NameRun::SmallCaps), ("gluco", NameRun::Italic)]
        );
    }

    #[test]
    fn tert_prefix_requires_a_following_hyphen() {
        assert!(name_markup("tert,butyl").runs.is_empty());
    }

    #[test]
    fn fusion_locants_preserve_primes_and_interior_scripts() {
        for name in ["(3a^1R/4a′S)-name", "(3a¹R/12′S)-name", "(3a12R/5′aS)-name"] {
            assert_eq!(parts(&name_markup(name), NameRun::Italic), ["R", "S"]);
        }
    }

    #[test]
    fn isotope_counts_remain_subscript_beside_element_locants() {
        assert_eq!(
            styled(&name_markup("(15N2,18O)methanamine")),
            [
                ("15", NameRun::Superscript),
                ("2", NameRun::Subscript),
                ("18", NameRun::Superscript),
            ]
        );
    }
}
