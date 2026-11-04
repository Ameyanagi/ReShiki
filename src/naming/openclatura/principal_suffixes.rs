//! Programmatic rendering of equivalent principal-group suffixes.
use super::{nomenclature::FunctionalGroupRule, rules::multipliers};
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
                multipliers::suffix(count, w)
            } else {
                w.into()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
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
            ("amine", 4, "tetramine"),
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
