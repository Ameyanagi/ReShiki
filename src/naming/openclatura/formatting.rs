// SPDX-License-Identifier: MIT AND (MIT OR Apache-2.0)
// Origin: mixed; upstream-derived code with original nazuchem/ReShiki extensions.
//   openclatura 0.4.3, src/openclatura/formatting.py @ 90dc5bcb4910c196122ec0405d06a32929a8f308.
// Copyright (c) 2026 lamalab-org
// Copyright (c) 2026 Ameyanagi and ReShiki contributors
// Changes: Openclatura fragment/multiplier formatting with local preferred-prefix and enclosure rules.
// Local contributions: MIT OR Apache-2.0; see NOTICE and PROVENANCE-nazuchem.toml.

//! Name-fragment formatting helpers.
use super::{
    assembly_parts::AssemblyParts,
    assembly_utils::is_fully_enclosed,
    rules::{multipliers, stems},
};
use std::collections::BTreeMap;
use std::sync::LazyLock;

/// The phenylamino spelling of the retained anilino prefixes (P-62.2.1.1.1), for
/// contractions that read the ligands of an amino prefix, as in phenylcarbamoyl.
// Original nazuchem code (not in openclatura 0.4.3): fn phenylamino_spelling
pub fn phenylamino_spelling(name: &str) -> &str {
    match name {
        "anilino" => "phenylamino",
        "N-phenylanilino" => "diphenylamino",
        _ => name,
    }
}
// Ported from src/openclatura/formatting.py (port; upstream function mapping not recorded; local fn strip_outer_parentheses)
pub fn strip_outer_parentheses(name: &str) -> String {
    if name.starts_with('(') && name.ends_with(')') {
        name[1..name.len() - 1].into()
    } else {
        name.into()
    }
}
static BASIC_MULTIPLIERS: LazyLock<Vec<String>> =
    LazyLock::new(|| (2..=1000).map(multipliers::basic).collect());
// Adapted from src/openclatura/formatting.py (adapted; upstream function mapping not recorded; local fn starts_with_multiplier)
pub fn starts_with_multiplier(name: &str) -> bool {
    BASIC_MULTIPLIERS.iter().any(|m| name.starts_with(m))
}
// Adapted from src/openclatura/formatting.py (adapted; upstream function mapping not recorded; local fn is_complex_prefix)
pub fn is_complex_prefix(name: &str) -> bool {
    name.contains('(')
        || name.starts_with(|c: char| c.is_ascii_digit())
        || name.contains('-')
        || name.contains(' ')
        || starts_with_multiplier(name)
}
// Ported from src/openclatura/formatting.py (port; upstream function mapping not recorded; local fn is_composite_prefix)
pub fn is_composite_prefix(name: &str) -> bool {
    for ending in ["ylidyne", "ylidene", "yl"] {
        if let Some(before) = name.strip_suffix(ending) {
            return before.contains("yl");
        }
    }
    name.contains("yl")
}
// Adapted from src/openclatura/formatting.py (adapted; upstream function mapping not recorded; local fn needs_complex_multiplier)
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
/// P-16.3.4(d): keep multiplied `dec` groups distinct from longer chains.
// Original nazuchem code (not in openclatura 0.4.3): fn needs_simple_multiplier_parentheses
pub fn needs_simple_multiplier_parentheses(name: &str) -> bool {
    name.starts_with("dec")
}
// Adapted from src/openclatura/formatting.py (adapted; upstream function mapping not recorded; local fn format_multiplier)
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
    if (complex || needs_simple_multiplier_parentheses(name)) && !is_fully_enclosed(name) {
        format!("{mult}({name})")
    } else {
        format!("{mult}{name}")
    }
}

// Ported from src/openclatura/formatting.py (port; upstream function mapping not recorded; local fn count_names)
pub fn count_names(names: &[String]) -> BTreeMap<String, usize> {
    let mut result = BTreeMap::new();
    for name in names {
        *result.entry(name.clone()).or_insert(0) += 1;
    }
    result
}
// Ported from src/openclatura/formatting.py (port; upstream function mapping not recorded; local fn format_counted_prefixes)
pub fn format_counted_prefixes(names: &[String]) -> String {
    let counts = count_names(names);
    let safe = counts.len() > 1 || counts.keys().any(|n| is_complex_prefix(n));
    counts
        .iter()
        .map(|(n, c)| format_multiplier(n, *c, safe))
        .collect()
}
// Adapted from src/openclatura/formatting.py (adapted; upstream function mapping not recorded; local fn format_center_ligands_by)
pub fn format_center_ligands_by<K: Ord>(names: &[String], sort_key: impl Fn(&str) -> K) -> String {
    let counts = count_names(names);
    let mut ordered: Vec<_> = counts.keys().collect();
    ordered.sort_by_key(|s| sort_key(s));
    let mut rendered = String::new();
    for (i, name) in ordered.iter().enumerate() {
        let count = counts[*name];
        let substituted = stems::terminal_stem(name)
            .is_some_and(|s| *name != &format!("{s}yl") && *name != &format!("cyclo{s}yl"));
        let complex =
            is_complex_prefix(name) || count > 1 && (substituted || needs_complex_multiplier(name));
        let multiplier = if count == 1 {
            String::new()
        } else if complex {
            multipliers::complex_(count)
        } else {
            multipliers::basic(count)
        };
        let merges = count == 1
            && ordered[i + 1..]
                .iter()
                .any(|later| counts[*later] > 1 && name.ends_with(later.as_str()));
        // P-16.5.1.3.1: later distinct ligands are enclosed, with a simple
        // multiplying prefix outside the parentheses: ethyldi(methyl).
        if (complex || merges || i > 0) && !is_fully_enclosed(name) {
            rendered.push_str(&format!("{multiplier}({name})"));
        } else {
            rendered.push_str(&format!("{multiplier}{name}"));
        }
    }
    rendered
}
// Adapted from src/openclatura/formatting.py (adapted; upstream function mapping not recorded; local fn substituted_alkoxy_prefix)
pub fn substituted_alkoxy_prefix(branch: &str, parts: &AssemblyParts) -> Option<String> {
    // P-63.2.2.2: the retained parent, not a suffix of a nested prefix, must
    // be methyl, ethyl, propyl, butyl or phenyl. tert-Butoxy is not substitutable.
    if parts.attachment_order != 1
        || parts.attachment_locant != "1"
        || parts.is_acyl_substituent
        || parts.principal_group.is_some()
        || parts.retained_substituent_name.is_some()
        || !parts.a_prefixes.is_empty()
        || !parts.parent_charges.is_empty()
        || !parts.hydro_operations.is_empty()
    {
        return None;
    }
    let stem = if parts.is_ring {
        (parts.retained_name.as_deref() == Some("benzene") && !parts.is_ring_assembly())
            .then_some("phen")?
    } else {
        if parts.retained_name.is_some() || !parts.unsaturations.is_empty() {
            return None;
        }
        match parts.parent_length {
            1 => "meth",
            2 => "eth",
            3 => "prop",
            4 => "but",
            _ => return None,
        }
    };
    let branch = strip_outer_parentheses(branch);
    let terminal = format!("{stem}yl");
    let prefix = branch.strip_suffix(&terminal)?;
    Some(if prefix.is_empty() {
        format!("{stem}oxy")
    } else {
        format!("({prefix}{stem}oxy)")
    })
}
// Adapted from src/openclatura/formatting.py (adapted; upstream function mapping not recorded; local fn oxy_prefix_from_branch)
pub fn oxy_prefix_from_branch(branch: &str) -> String {
    let branch = strip_outer_parentheses(branch);
    if let Some(name) = super::nomenclature::RULES
        .heteroatoms
        .alkyl_oxy_prefixes
        .get(&branch)
    {
        return name.clone();
    }
    format_element_substituent("", &branch, "oxy", false)
}
// Adapted from src/openclatura/formatting.py (adapted; upstream function mapping not recorded; local fn format_element_substituent)
pub fn format_element_substituent(
    stereo_prefix: &str,
    branch: &str,
    suffix: &str,
    is_double: bool,
) -> String {
    let branch = strip_outer_parentheses(branch);
    let suffix = format!("{suffix}{}", if is_double { "idene" } else { "" });
    // An acyl prefix that names a parent hydride, as methanesulfonyl or
    // ethanethioyl, is enclosed although simple (P-16.5.1.4).
    let names_hydride = [
        "sulfonyl",
        "sulfinyl",
        "selenonyl",
        "seleninyl",
        "telluronyl",
        "tellurinyl",
        "carbonyl",
        "thioyl",
        "selenoyl",
        "telluroyl",
    ]
    .iter()
    .filter_map(|acyl| branch.strip_suffix(acyl))
    .map(|stem| stem.strip_suffix("carbo").unwrap_or(stem))
    .any(|stem| {
        ["ane", "ene", "yne", "ine"]
            .iter()
            .any(|end| stem.ends_with(end))
    });
    if is_complex_prefix(&branch) || names_hydride {
        format!("({stereo_prefix}({branch}){suffix})")
    } else {
        format!("({stereo_prefix}{branch}{suffix})")
    }
}

#[cfg(test)]
mod tests {
    use super::format_multiplier;

    // Original nazuchem code (not in openclatura 0.4.3): fn simple_dec_prefixes_keep_basic_multipliers_and_parentheses
    #[test]
    fn simple_dec_prefixes_keep_basic_multipliers_and_parentheses() {
        for (name, count, expected) in [
            ("decyl", 1, "decyl"),
            ("decyl", 2, "di(decyl)"),
            ("decylidene", 2, "di(decylidene)"),
            ("decyl", 3, "tri(decyl)"),
            ("ethylidene", 2, "diethylidene"),
        ] {
            assert_eq!(format_multiplier(name, count, false), expected);
        }
    }

    // Original nazuchem code (not in openclatura 0.4.3): fn oxy_prefix_keeps_parent_hydride_acyl_enclosed
    #[test]
    fn oxy_prefix_keeps_parent_hydride_acyl_enclosed() {
        // P-16.5.1.4: (methanethioyl)oxy, as in (cyclohexanecarbonyl)sulfanyl (P-57.4).
        assert_eq!(
            super::oxy_prefix_from_branch("(methanethioyl)"),
            "((methanethioyl)oxy)"
        );
        assert_eq!(super::oxy_prefix_from_branch("pentyl"), "(pentyloxy)");
        assert_eq!(super::oxy_prefix_from_branch("ethyl"), "ethoxy");
    }
}
