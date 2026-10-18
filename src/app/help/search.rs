//! Fuzzy ranking over the shortcut registry.
//!
//! Every query word must match the entry somewhere. A key typed exactly as
//! the chord shows it ranks first, then whole words in the name, synonyms and
//! description, then prefixes, substrings and finally letters in order.
use super::registry::Entry;

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
    // A bare "/" or "+" is a key, not only a separator.
    for key in ["/", "+"] {
        if text.split_whitespace().any(|part| part == key) || text.ends_with(key) {
            tokens.push(key.into());
        }
    }
    tokens
}

/// The chord's keys and every spelling of its modifiers, lower-cased beside
/// the original so case can break ties ("r" before "R").
fn chord_terms(keys: &str) -> Vec<String> {
    let mut terms = key_tokens(keys);
    for token in terms.clone() {
        for (symbol, words) in MODIFIERS {
            if token == *symbol || words.iter().any(|w| token.eq_ignore_ascii_case(w)) {
                terms.push((*symbol).into());
                terms.extend(words.iter().map(|w| (*w).into()));
            }
        }
    }
    terms
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

fn token_score(entry: &Entry, chord: &[String], token: &str) -> u32 {
    let lower = token.to_lowercase();
    let mut best = 0;
    for term in chord {
        if term == token {
            best = best.max(100);
        } else if term.to_lowercase() == lower {
            best = best.max(80);
        }
    }
    let name = entry.name.to_lowercase();
    let synonyms = entry.synonyms.join(" ").to_lowercase();
    let description = entry.description.to_lowercase();
    let scope = entry.scope.label().to_lowercase();
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
    if best == 0 && lower.chars().count() > 2 && subsequence(&lower, &name) {
        // Fewer words in the name means a closer match.
        best = 4 + 4 / words(&name).count().max(1) as u32;
    }
    best
}

/// Query words: whitespace-separated, with modifier symbols split off ("⇧r").
fn query_tokens(query: &str) -> Vec<String> {
    let mut tokens = vec![];
    for part in query.split_whitespace() {
        let mut rest = String::new();
        for c in part.chars() {
            if modifier(c) {
                if !rest.is_empty() {
                    tokens.push(std::mem::take(&mut rest));
                }
                tokens.push(c.to_string());
            } else if c == '+' && !rest.is_empty() {
                tokens.push(std::mem::take(&mut rest));
            } else {
                rest.push(c);
            }
        }
        if !rest.is_empty() {
            tokens.push(rest);
        }
    }
    tokens
}

/// The score of one entry, or None when some query word matches nothing.
pub(crate) fn score(entry: &Entry, query: &str) -> Option<u32> {
    let chord = chord_terms(&entry.keys);
    let mut total = 0;
    for token in query_tokens(query) {
        match token_score(entry, &chord, &token) {
            0 => return None,
            score => total += score,
        }
    }
    let query = query.trim().to_lowercase();
    let name = entry.name.to_lowercase();
    if !query.is_empty() && name == query {
        total += 40;
    } else if query.chars().count() > 1 && name.starts_with(&query) {
        total += 20;
    }
    Some(total)
}

/// Registry indexes in ranked order; an empty query keeps registry order.
pub(crate) fn rank(entries: &[Entry], query: &str) -> Vec<usize> {
    let mut scored: Vec<_> = entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| score(entry, query).map(|score| (score, index)))
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().map(|(_, index)| index).collect()
}
