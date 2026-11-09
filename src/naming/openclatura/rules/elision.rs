// SPDX-License-Identifier: MIT
// Origin: port
// Source: openclatura | src/openclatura/rules/elision.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Copyright (c) 2026 lamalab-org

pub fn is_vowel_start(s: &str) -> bool {
    s.chars()
        .next()
        .is_some_and(|c| "aeiouy".contains(c.to_ascii_lowercase()))
}
pub fn elide_terminal_a(prefix: &str, following: &str) -> String {
    let p = if prefix.ends_with('a') && is_vowel_start(following) {
        &prefix[..prefix.len() - 1]
    } else {
        prefix
    };
    format!("{p}{following}")
}
