// SPDX-License-Identifier: MIT
// Origin: adapted
// Source: openclatura | src/openclatura/rules/stems.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Copyright (c) 2026 lamalab-org
// Changes: Adapted openclatura established chain-stem constants, numerical generation and terminal-alkyl stem lookup.

use super::multipliers;
const STEMS: [&str; 31] = [
    "", "meth", "eth", "prop", "but", "pent", "hex", "hept", "oct", "non", "dec", "undec", "dodec",
    "tridec", "tetradec", "pentadec", "hexadec", "heptadec", "octadec", "nonadec", "icos",
    "henicos", "docos", "tricos", "tetracos", "pentacos", "hexacos", "heptacos", "octacos",
    "nonacos", "triacont",
];
pub fn stem_for(length: usize) -> String {
    assert!((1..=1000).contains(&length));
    if length <= 30 {
        STEMS[length].to_owned()
    } else {
        multipliers::numerical_term(length)
            .trim_end_matches('a')
            .to_owned()
    }
}
pub fn terminal_stem(name: &str) -> Option<String> {
    (1..=1000)
        .map(stem_for)
        .filter(|s| name.ends_with(&format!("{s}yl")))
        .max_by_key(|s| s.len())
}
