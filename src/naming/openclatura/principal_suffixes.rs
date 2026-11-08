// SPDX-License-Identifier: MIT AND (MIT OR Apache-2.0)
// Origin: mixed
// Source: openclatura | src/openclatura/principal_suffixes.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: ReShiki | src/naming/openclatura/principal_suffixes.rs | 5f559fd482e237307a589dfcb7ea333cee0a6e13
// Copyright (c) 2026 lamalab-org
// Copyright (c) 2026 Ameyanagi and ReShiki contributors
// Changes: Ported principal-suffix word-position rendering, locally extended for suffix elision and bis(aminium), with original tests.
// Local contributions: MIT OR Apache-2.0; see NOTICE and PROVENANCE-nazuchem.toml.

//! Programmatic rendering of equivalent principal-group suffixes.
use super::{nomenclature::FunctionalGroupRule, rules::multipliers};
// Provenance: fn render_principal_suffix | adapted
// Source: openclatura | src/openclatura/principal_suffixes.py::render_principal_suffix
pub fn render_principal_suffix(rule: &FunctionalGroupRule, count: usize) -> String {
    let Some(suffix) = rule.suffix.as_ref() else {
        return String::new();
    };
    if count <= 1 {
        return suffix.clone();
    }
    let positions: &[usize] = if rule.suffix_multiplier_positions.is_empty() {
        &[0]
    } else {
        &rule.suffix_multiplier_positions
    };
    suffix
        .split_whitespace()
        .enumerate()
        .map(|(i, w)| {
            if positions.contains(&i) {
                if w == "aminium" {
                    format!("{}({w})", multipliers::complex_(count))
                } else {
                    multipliers::suffix(count, w)
                }
            } else {
                w.into()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
// Provenance: mod tests | original
// Source: ReShiki | src/naming/openclatura/principal_suffixes.rs::tests
mod tests {
    use super::super::nomenclature::RULES;
    use super::*;

    #[test]
    fn equivalent_principal_groups_use_suffix_specific_elision() {
        for (key, count, expected) in [
            ("ketone", 1, "one"),
            ("ketone", 2, "dione"),
            ("ketone", 3, "trione"),
            ("ketone", 4, "tetrone"),
            ("alcohol", 4, "tetrol"),
            ("olate", 2, "bis(olate)"),
            ("thiolate", 2, "bis(thiolate)"),
            ("selenolate", 3, "tris(selenolate)"),
            ("tellurolate", 4, "tetrakis(tellurolate)"),
            ("amine", 4, "tetramine"),
            ("aminium", 1, "aminium"),
            ("aminium", 2, "bis(aminium)"),
            ("aminium", 3, "tris(aminium)"),
            ("ring_carboxylic_acid", 4, "tetracarboxylic acid"),
        ] {
            let rule = RULES.functional_groups.get(key).unwrap();
            assert_eq!(render_principal_suffix(rule, count), expected, "{key}");
        }
    }

    #[test]
    fn suffix_multipliers_respect_word_positions() {
        let mut rule = FunctionalGroupRule {
            suffix: Some("oyl chloride".into()),
            suffix_multiplier_positions: vec![0, 1],
            ..Default::default()
        };
        assert_eq!(render_principal_suffix(&rule, 4), "tetroyl tetrachloride");
        rule.suffix_multiplier_positions = vec![1];
        assert_eq!(render_principal_suffix(&rule, 4), "oyl tetrachloride");
        rule.suffix = None;
        assert_eq!(render_principal_suffix(&rule, 4), "");
    }
}
