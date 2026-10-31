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
                format!("{}{w}", multipliers::basic(count))
            } else {
                w.into()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
