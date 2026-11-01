//! Name-fragment formatting helpers.
use super::{
    assembly_utils::is_fully_enclosed,
    rules::{multipliers, stems},
};
use std::collections::BTreeMap;
use std::sync::LazyLock;

pub fn strip_outer_parentheses(name: &str) -> String {
    if name.starts_with('(') && name.ends_with(')') {
        name[1..name.len() - 1].into()
    } else {
        name.into()
    }
}
static BASIC_MULTIPLIERS: LazyLock<Vec<String>> =
    LazyLock::new(|| (2..=1000).map(multipliers::basic).collect());
pub fn starts_with_multiplier(name: &str) -> bool {
    BASIC_MULTIPLIERS.iter().any(|m| name.starts_with(m))
}
pub fn is_complex_prefix(name: &str) -> bool {
    name.contains('(')
        || name.starts_with(|c: char| c.is_ascii_digit())
        || name.contains('-')
        || name.contains(' ')
        || starts_with_multiplier(name)
}
pub fn is_composite_prefix(name: &str) -> bool {
    for ending in ["ylidyne", "ylidene", "yl"] {
        if let Some(before) = name.strip_suffix(ending) {
            return before.contains("yl");
        }
    }
    name.contains("yl")
}
pub fn needs_complex_multiplier(name: &str) -> bool {
    is_composite_prefix(name)
        || (name.ends_with("yl") && name.contains("oxy"))
        || [
            "sulfanyl",
            "selanyl",
            "tellanyl",
            "phosphanyl",
            "arsanyl",
            "sulfanylidene",
            "selanylidene",
            "tellanylidene",
        ]
        .contains(&name)
}
pub fn format_multiplier(name: &str, count: usize, safe_enclose: bool) -> String {
    let complex = is_complex_prefix(name) || (count > 1 && needs_complex_multiplier(name));
    if count == 1 {
        return if (safe_enclose || complex) && !is_fully_enclosed(name) {
            format!("({name})")
        } else {
            name.into()
        };
    }
    let mult = if complex {
        multipliers::complex_(count)
    } else {
        multipliers::basic(count)
    };
    if complex && !is_fully_enclosed(name) {
        format!("{mult}({name})")
    } else {
        format!("{mult}{name}")
    }
}
pub fn count_names(names: &[String]) -> BTreeMap<String, usize> {
    let mut result = BTreeMap::new();
    for name in names {
        *result.entry(name.clone()).or_insert(0) += 1;
    }
    result
}
pub fn format_counted_prefixes(names: &[String]) -> String {
    let counts = count_names(names);
    let safe = counts.len() > 1 || counts.keys().any(|n| is_complex_prefix(n));
    counts
        .iter()
        .map(|(n, c)| format_multiplier(n, *c, safe))
        .collect()
}
pub fn format_center_ligands_by(names: &[String], sort_key: impl Fn(&str) -> String) -> String {
    let counts = count_names(names);
    let mut ordered: Vec<_> = counts.keys().collect();
    ordered.sort_by_key(|s| sort_key(s));
    let mut rendered = String::new();
    for (i, name) in ordered.iter().enumerate() {
        let count = counts[*name];
        let substituted = stems::terminal_stem(name)
            .is_some_and(|s| *name != &format!("{s}yl") && *name != &format!("cyclo{s}yl"));
        let mut ligand = if count > 1 && substituted && !is_fully_enclosed(name) {
            format!("{}({name})", multipliers::complex_(count))
        } else {
            format_multiplier(name, count, false)
        };
        let merges = count == 1
            && ordered[i + 1..]
                .iter()
                .any(|later| counts[*later] > 1 && name.ends_with(later.as_str()));
        if (merges || (i > 0 && count == 1)) && !is_fully_enclosed(&ligand) {
            ligand = format!("({ligand})");
        }
        rendered.push_str(&ligand);
    }
    rendered
}
pub fn substituted_alkoxy_prefix(branch: &str) -> Option<String> {
    if !branch.contains("hydroxy") {
        return None;
    }
    let stem = stems::terminal_stem(branch)?;
    let terminal = format!("{stem}yl");
    let prefix = branch.strip_suffix(&terminal)?;
    Some(if prefix.is_empty() {
        format!("{stem}oxy")
    } else {
        format!("({prefix}{stem}oxy)")
    })
}
pub fn oxy_prefix_from_branch(branch: &str) -> String {
    let branch = strip_outer_parentheses(branch);
    if let Some(name) = super::nomenclature::RULES
        .heteroatoms
        .alkyl_oxy_prefixes
        .get(&branch)
    {
        return name.clone();
    }
    if let Some(name) = substituted_alkoxy_prefix(&branch) {
        return name;
    }
    if is_complex_prefix(&branch) {
        format!("(({branch})oxy)")
    } else {
        format!("({branch}oxy)")
    }
}
pub fn format_element_substituent(
    stereo_prefix: &str,
    branch: &str,
    suffix: &str,
    is_double: bool,
) -> String {
    let branch = strip_outer_parentheses(branch);
    let suffix = format!("{suffix}{}", if is_double { "idene" } else { "" });
    if is_complex_prefix(&branch) {
        format!("({stereo_prefix}({branch}){suffix})")
    } else {
        format!("({stereo_prefix}{branch}{suffix})")
    }
}
