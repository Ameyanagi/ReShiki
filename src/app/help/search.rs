//! Fuzzy ranking over the shortcut registry and the command palette.
//!
//! Every query word must match the entry somewhere, in any order. A key typed
//! exactly as the chord shows it ranks first, then whole words in the name,
//! synonyms and description, then prefixes and substrings. A word that matches
//! none of those may still be the initials of name words ("bt"), letters in
//! order ("wdg"), or carry one typo in five or more letters ("benzne").
use super::registry::Entry;

/// What the matcher reads from a row: Help's shortcuts and the palette's
/// commands share one ranking.
pub(crate) trait Searchable {
    fn keys(&self) -> &str;
    fn name(&self) -> &str;
    fn synonyms(&self) -> String;
    fn description(&self) -> &str;
    fn scope(&self) -> &str;
}

impl Searchable for Entry {
    fn keys(&self) -> &str {
        &self.keys
    }
    fn name(&self) -> &str {
        self.name
    }
    fn synonyms(&self) -> String {
        self.synonyms.join(" ")
    }
    fn description(&self) -> &str {
        self.description
    }
    fn scope(&self) -> &str {
        self.scope.label()
    }
}

/// Modifier symbols and the words people type for them, both ways round,
/// so "shift r", "⇧r" and "Shift+R" find the same chord on every platform.
const MODIFIERS: &[(&str, &[&str])] = &[
    ("⇧", &["shift"]),
    ("⌘", &["cmd", "command", "ctrl"]),
    ("⌥", &["alt", "option", "opt"]),
    ("⌃", &["ctrl", "control"]),
    ("↩", &["enter", "return"]),
    ("⌫", &["delete", "backspace", "del"]),
    ("←", &["left"]),
    ("→", &["right"]),
    ("↑", &["up"]),
    ("↓", &["down"]),
];

fn modifier(c: char) -> bool {
    MODIFIERS.iter().any(|(symbol, _)| symbol.starts_with(c))
}

/// Splits a chord or query into keys: "⇧⌘A / Shift+R" → ⇧ ⌘ A Shift R.
fn key_tokens(text: &str) -> Vec<String> {
    let mut tokens = vec![];
    for part in text.split(|c: char| c.is_whitespace() || matches!(c, '/' | '+' | '–' | ',')) {
        let mut rest = String::new();
        for c in part.chars() {
            if modifier(c) {
                if !rest.is_empty() {
                    tokens.push(std::mem::take(&mut rest));
                }
                tokens.push(c.to_string());
            } else {
                rest.push(c);
            }
        }
        if !rest.is_empty() {
            tokens.push(rest);
        }
    }
    // A final "/" or "+" is a key, as in ⌘/ or +; elsewhere " / " separates
    // alternatives.
    for key in ["/", "+"] {
        if text.ends_with(key) {
            tokens.push(key.into());
        }
    }
    tokens
}

fn is_modifier(term: &str) -> bool {
    MODIFIERS.iter().take(4).any(|(symbol, words)| {
        term == *symbol || words.iter().any(|w| term.eq_ignore_ascii_case(w))
    })
}

/// Each " / " alternative of a chord, as its keys and every spelling of its
/// modifiers, lower-cased beside the original so case can break ties ("r"
/// before "R"). A bare key after a modified one shares its modifiers, as in
/// ⌘⇧⌥ L / C / R.
fn chord_terms(keys: &str) -> Vec<Vec<String>> {
    let modified = key_tokens(keys).iter().any(|t| is_modifier(t));
    let mut modifiers: Vec<String> = vec![];
    keys.split(" / ")
        .map(|alternative| {
            let mut terms = key_tokens(alternative);
            let own: Vec<String> = terms.iter().filter(|t| is_modifier(t)).cloned().collect();
            if own.is_empty() {
                terms.extend(modifiers.iter().cloned());
            } else {
                modifiers = own;
            }
            // Ranges such as ⇧3–⇧8 or ⌘1–⌘8 include the keys between their ends.
            let chars: Vec<char> = alternative.chars().collect();
            for (i, _) in chars.iter().enumerate().filter(|(_, c)| **c == '–') {
                let first = i.checked_sub(1).and_then(|j| chars.get(j)?.to_digit(10));
                let last = chars
                    .iter()
                    .skip(i + 1)
                    .find(|c| !modifier(**c) && !c.is_alphabetic() && **c != '+')
                    .and_then(|c| c.to_digit(10));
                if let (Some(first), Some(last)) = (first, last) {
                    terms.extend((first + 1..last).map(|d| d.to_string()));
                }
            }
            // An uppercase contextual key is Shift-letter, for that key only:
            // in "C / l", Shift+L is not Chlorine. Chords with modifiers, such
            // as ⌘C, already spell out Shift when they use it.
            let shifted = !modified
                && terms
                    .iter()
                    .any(|t| t.len() == 1 && t.chars().all(|c| c.is_ascii_uppercase()));
            if shifted {
                terms.push("⇧".into());
            }
            for token in terms.clone() {
                for (symbol, words) in MODIFIERS {
                    if token == *symbol || words.iter().any(|w| token.eq_ignore_ascii_case(w)) {
                        terms.push((*symbol).into());
                        terms.extend(words.iter().map(|w| (*w).into()));
                    }
                }
            }
            terms
        })
        .collect()
}

fn words(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
}

/// In-order letters, as "wdg" in "wedge".
fn subsequence(needle: &str, haystack: &str) -> bool {
    let mut chars = haystack.chars();
    needle.chars().all(|n| chars.any(|h| h == n))
}

fn token_score(entry: &impl Searchable, chord: &[String], token: &str) -> u32 {
    let lower = token.to_lowercase();
    let mut best = 0;
    for term in chord {
        if term == token {
            best = best.max(100);
        } else if term.to_lowercase() == lower {
            best = best.max(80);
        }
    }
    let name = entry.name().to_lowercase();
    let synonyms = entry.synonyms().to_lowercase();
    let description = entry.description().to_lowercase();
    let scope = entry.scope().to_lowercase();
    let fields: [(&str, u32); 4] = [(&name, 4), (&synonyms, 3), (&description, 2), (&scope, 1)];
    for (text, weight) in fields {
        for word in words(text) {
            if word == lower {
                best = best.max(15 * weight);
            } else if word.starts_with(&lower) {
                best = best.max(10 * weight);
            }
        }
        // Single letters would match nearly everything mid-word.
        if lower.chars().count() > 1 && text.contains(&lower) {
            best = best.max(5 * weight);
        }
    }
    if best == 0 && lower.chars().count() > 1 {
        best = loose_score(&lower, &name, &synonyms, &description);
    }
    best
}

/// Looser matches for a word that matched nothing more directly.
fn loose_score(lower: &str, name: &str, synonyms: &str, description: &str) -> u32 {
    let name_words: Vec<String> = words(name).collect();
    // Initials of consecutive words, as "bt" in "Benzene tool".
    let initials: String = name_words.iter().filter_map(|w| w.chars().next()).collect();
    if initials.contains(lower) {
        return 25;
    }
    let mut best = 0;
    // Letters in order within one word, as "wdg" in "wedge". A name with
    // fewer words is a closer match.
    if lower.chars().count() > 2 {
        let in_word = |text: &str| words(text).any(|word| subsequence(lower, &word));
        if in_word(name) {
            best = 4 + 4 / name_words.len().max(1) as u32;
        } else if in_word(synonyms) {
            best = 3;
        } else if in_word(description) {
            best = 2;
        }
    }
    // One wrong, missing, extra or swapped letter in five or more, as
    // "benzne" for "benzene".
    if lower.chars().count() >= 5 {
        let typo = |text: &str| words(text).any(|word| one_typo(lower, &word));
        if typo(name) {
            best = best.max(8);
        } else if typo(synonyms) {
            best = best.max(6);
        }
    }
    // Letters in order across the name from a word's start, as "bentl".
    let first = lower.chars().next().unwrap_or_default();
    let across = (0..name_words.len())
        .filter_map(|i| name_words.get(i..))
        .any(|rest| {
            rest.first().is_some_and(|w| w.starts_with(first)) && subsequence(lower, &rest.concat())
        });
    if best == 0 && across {
        best = 1;
    }
    best
}

/// Whether `typed` is `word`, or its start, but for one typo.
fn one_typo(typed: &str, word: &str) -> bool {
    let typed: Vec<char> = typed.chars().collect();
    let word: Vec<char> = word.chars().collect();
    let n = typed.len();
    // The start may be a letter shorter or longer than what was typed.
    [word.len(), n.saturating_sub(1), n, n + 1]
        .into_iter()
        .filter_map(|len| word.get(..len))
        .any(|start| one_edit(&typed, start))
}

/// One letter changed, missing, extra or swapped with the next.
fn one_edit(a: &[char], b: &[char]) -> bool {
    let same = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let (a, b) = (
        a.get(same..).unwrap_or_default(),
        b.get(same..).unwrap_or_default(),
    );
    a.get(1..) == b.get(1..)
        || a.get(1..) == Some(b)
        || b.get(1..) == Some(a)
        || matches!((a, b), ([x, y, r @ ..], [p, q, s @ ..]) if x == q && y == p && r == s)
}

/// Query words: whitespace-separated, with modifier symbols split off ("⇧r").
fn query_tokens(query: &str) -> Vec<String> {
    let mut tokens = vec![];
    for part in query.split_whitespace() {
        let mut rest = String::new();
        // "+" after a word or modifier separates, as in "shift+R" or "⇧+R".
        let mut after_modifier = false;
        for c in part.chars() {
            if modifier(c) {
                if !rest.is_empty() {
                    tokens.push(std::mem::take(&mut rest));
                }
                tokens.push(c.to_string());
                after_modifier = true;
            } else if c == '+' && (after_modifier || !rest.is_empty()) {
                if !rest.is_empty() {
                    tokens.push(std::mem::take(&mut rest));
                }
                after_modifier = false;
            } else {
                rest.push(c);
                after_modifier = false;
            }
        }
        if !rest.is_empty() {
            tokens.push(rest);
        }
    }
    tokens
}

/// The score of one entry, or None when some query word matches nothing.
pub(crate) fn score(entry: &impl Searchable, query: &str) -> Option<u32> {
    let tokens = query_tokens(query);
    // Every query word must match within one alternative of the chord.
    let mut total = chord_terms(entry.keys())
        .iter()
        .filter_map(|chord| {
            tokens
                .iter()
                .map(|token| Some(token_score(entry, chord, token)).filter(|s| *s > 0))
                .sum::<Option<u32>>()
        })
        .max()?;
    let query = query.trim().to_lowercase();
    let name = entry.name().to_lowercase();
    if !query.is_empty() && name == query {
        total += 40;
    } else if query.chars().count() > 1 && name.starts_with(&query) {
        total += 20;
    }
    Some(total)
}

/// Whether every query word starts a word of the name or is a key of the
/// chord, not only a synonym or description match: "ring" names Ring tool but
/// not New drawing, and "v" is Select tool's key. The palette ranks these first.
pub(crate) fn in_name_or_keys(entry: &impl Searchable, query: &str) -> bool {
    let keys: Vec<String> = (chord_terms(entry.keys()).concat().iter())
        .map(|term| term.to_lowercase())
        .collect();
    query_tokens(query).iter().all(|token| {
        let token = token.to_lowercase();
        keys.contains(&token) || words(entry.name()).any(|word| word.starts_with(&token))
    })
}

/// Registry indexes in ranked order; an empty query keeps registry order.
pub(crate) fn rank(entries: &[impl Searchable], query: &str) -> Vec<usize> {
    let mut scored: Vec<_> = entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| score(entry, query).map(|score| (score, index)))
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().map(|(_, index)| index).collect()
}
