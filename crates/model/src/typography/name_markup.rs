//! Presentation-only typography for chemical names. Engine names stay unchanged.
use super::{Script, TextFormat, TextSpan};
use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameRun {
    Italic,
    SmallCaps,
    Superscript,
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
    "epi", "rac", "rel", "myo", "scyllo", "chiro", "gluco", "manno", "galacto", "allo", "tert",
    "sec",
];
const ELEMENTS: &[&str] = &["N", "O", "S", "P", "Se", "Te", "As", "B", "Si"];

fn is_prime(c: char) -> bool {
    matches!(c, '\'' | '′' | '″' | '‴' | '⁗')
}

fn superscript_digit(c: char) -> Option<char> {
    SUPERSCRIPTS
        .iter()
        .position(|&digit| digit == c)
        .map(|i| char::from(b'0' + i as u8))
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
    let rest = after(text, digits);
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

fn preceding_locant(text: &str, at: usize, fusion: bool) -> bool {
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
    boundary_before(text, start)
        && numeric_locant(locant)
        && (!fusion || locant.chars().any(|c| c.is_ascii_lowercase()))
}

/// Convert the few ASCII presentation encodings, retaining baseline digits so
/// every export can use the same script spans instead of font-specific glyphs.
fn display_text(name: &str) -> NameMarkup {
    let mut markup = NameMarkup {
        text: String::new(),
        runs: vec![],
    };
    let mut i = 0;
    while i < name.len() {
        let rest = after(name, i);
        let Some(c) = rest.chars().next() else { break };
        let lambda = ["lambda^", "λ^", "λ"]
            .into_iter()
            .find_map(|prefix| rest.strip_prefix(prefix).map(|tail| (prefix.len(), tail)));
        if let Some((prefix, tail)) = lambda
            && (boundary_before(name, i) || preceding_locant(name, i, false))
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
            && preceding_locant(name, i, false)
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
            && preceding_locant(name, i, true)
        {
            let digits = tail.bytes().take_while(u8::is_ascii_digit).count();
            let end = i + digits + 1;
            if digits > 0
                && (boundary_after(name, end)
                    || after(name, end)
                        .strip_prefix(['H', 'R', 'S', 'r', 's'])
                        .is_some_and(|tail| tail.starts_with(['-', ')', ',', '*'])))
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
        if superscript_digit(c).is_some()
            && (preceding_locant(name, i, false)
                || before(name, i).strip_suffix('λ').is_some_and(|prefix| {
                    prefix
                        .chars()
                        .next_back()
                        .is_none_or(|c| !c.is_alphabetic())
                }))
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
        if c == '\'' && prime_after_locant(name, i) {
            let count = rest.bytes().take_while(|&b| b == b'\'').count();
            markup.text.extend(std::iter::repeat_n('‴', count / 3));
            match count % 3 {
                1 => markup.text.push('′'),
                2 => markup.text.push('″'),
                _ => (),
            }
            i += count;
            continue;
        }
        if c == 'H'
            && matches!(after(name, i + 1).chars().next(), Some('-' | ')' | ','))
            && preceding_locant(name, i, false)
        {
            // Recognize interior locants before their script encoding is removed.
            markup
                .runs
                .push((markup.text.len()..markup.text.len() + 1, NameRun::Italic));
        }
        markup.text.push(c);
        i += c.len_utf8();
    }
    markup
}

fn prime_after_locant(text: &str, at: usize) -> bool {
    if preceding_locant(text, at, false) {
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
        "R*", "S*", "rel", "rac", "R", "S", "E", "Z", "r", "s", "M", "P", "ξ", "Ξ",
    ]
    .into_iter()
    .chain(PREFIXES.iter().copied())
    {
        if let Some(locant) = trimmed.strip_suffix(descriptor)
            && (locant.is_empty() || numeric_locant(locant))
        {
            let start = offset + locant.len();
            return Some(start..start + descriptor.len());
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
                        let range = stereo_item(part);
                        if let Some(range) = &range {
                            descriptors
                                .push((offset + range.start..offset + range.end, NameRun::Italic));
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
                    let mut letters = vec![];
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
                            let n = word.bytes().take_while(u8::is_ascii_lowercase).count();
                            let valid = n > 0
                                && (numbered || n == 1)
                                && after(word, n).chars().all(is_prime);
                            if valid {
                                letters.push((position..position + n, NameRun::Italic));
                            }
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
                let locant = rest.trim_start_matches(|c: char| {
                    c.is_ascii_digit() || superscript_digit(c).is_some()
                });
                if (PREFIXES.contains(&word)
                    && (next.is_none_or(|c| c == '-')
                        || rest
                            .strip_prefix(',')
                            .is_some_and(|tail| tail.starts_with(char::is_alphabetic))))
                    || (["o", "m", "p"].contains(&word) && next == Some('-'))
                    || (ELEMENTS.contains(&word)
                        && locant.starts_with(|c: char| matches!(c, ',' | '-') || is_prime(c)))
                {
                    markup.runs.push((i..i + n, NameRun::Italic));
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
    markup.runs.sort_by_key(|(range, _)| range.start);
    markup.runs.dedup();
    markup
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts(markup: &NameMarkup, kind: NameRun) -> Vec<&str> {
        markup
            .runs
            .iter()
            .filter(|(_, run)| *run == kind)
            .map(|(range, _)| &markup.text[range.clone()])
            .collect()
    }

    #[test]
    fn italic_name_tokens_keep_punctuation_and_locant_digits_roman() {
        for (name, expected) in [
            ("(2R,3S)-(E)-N,N-dimethyl", vec!["R", "S", "E", "N", "N"]),
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
            "polylambda^5",
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
    fn superscript_encodings_share_caption_and_unicode_ranges() {
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
}
