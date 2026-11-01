//! Prefix grouping, alphabetization and protective parentheses.
use super::{
    assembly_parts::AssemblyParts,
    assembly_utils::{is_fully_enclosed, needs_hyphen, parse_locant},
    formatting::{is_complex_prefix, needs_complex_multiplier, starts_with_multiplier},
    locant_elision::{retained_parent_attachment_is_ambiguous, substituent_locant_set_is_unique},
    nomenclature::RULES,
    rules::multipliers,
};
use std::collections::BTreeMap;

fn ignored_prefix_len(text: &str) -> usize {
    // The registry's prefix pattern recognizes locant lists and italic descriptors.
    let Some(end) = text.find(['-', ')']) else {
        return 0;
    };
    let prefix = &text[..end];
    let words = [
        "sec", "tert", "t", "s", "d", "l", "m", "o", "p", "alpha", "beta", "gamma",
    ];
    let stereo = !prefix.is_empty() && prefix.chars().all(|c| "ezrs".contains(c));
    let locants = prefix.split(',').all(|s| {
        let digits = s.chars().take_while(|c| c.is_ascii_digit()).count();
        if digits > 0 {
            s[digits..].chars().all(|c| c.is_ascii_lowercase())
        } else {
            s.chars().next().is_some_and(|c| "nospmc".contains(c))
                && s[1..].chars().all(|c| c == '\'')
        }
    });
    if words.contains(&prefix) || stereo || locants {
        end + text[end..]
            .chars()
            .take_while(|c| *c == '-' || *c == ')')
            .count()
    } else {
        0
    }
}
pub fn substituent_sort_key(name: &str) -> String {
    let lowercase = name.to_lowercase();
    let mut text = lowercase.as_str();
    loop {
        text = text.trim_start_matches(['(', '[', '{', ')']);
        let n = ignored_prefix_len(text);
        if n == 0 {
            return text.to_owned();
        }
        text = &text[n..];
    }
}
fn group_offers_second_position(key: &str) -> bool {
    RULES
        .functional_groups
        .get(key)
        .and_then(|r| r.suffix.as_ref())
        .is_some_and(|s| {
            RULES
                .assembly
                .suffix_nitrogen_markers
                .iter()
                .any(|m| s.contains(m))
        })
}
pub fn substituent_locant_string(
    parts: &AssemblyParts,
    name: &str,
    locs: &[String],
    grouped_count: usize,
) -> String {
    if parts.elided_substituent_locants.contains(name) {
        return String::new();
    }
    let imino_beside = grouped_count > 1 && parts.substituents.iter().any(|s| s.name == "imino");
    if parts.parent_length == 1
        && parts.retained_name.as_deref() != Some("guanidine")
        && !imino_beside
        && locs.iter().all(|l| l.chars().all(|c| c.is_ascii_digit()))
        && parts.a_prefixes.is_empty()
        && parts
            .principal_group
            .as_ref()
            .is_none_or(|g| !group_offers_second_position(&g.key))
    {
        return String::new();
    }
    if parts.parent_length == 2
        && !parts.is_ring
        && !parts.is_substituent
        && parts.principal_group.is_none()
        && parts.a_prefixes.is_empty()
        && locs.iter().all(|l| l.chars().all(|c| c.is_ascii_digit()))
    {
        let total: usize = parts.substituents.iter().map(|s| s.locants.len()).sum();
        let capacity = parts
            .unsaturations
            .first()
            .map_or(6, |u| if u.bond_key == "double" { 4 } else { 2 });
        let simple = parts
            .substituents
            .iter()
            .all(|s| !is_complex_prefix(&s.name));
        // P-14.3.4.2(b): a monosubstituted ethane cites no locant, as in (methylsulfanyl)ethane.
        if grouped_count == 1 && (total == 1 || simple && total == capacity) {
            return String::new();
        }
    }
    let ambiguous = super::retained_specs::retained_parent_spec(parts.retained_name.as_deref())
        .is_some()
        && retained_parent_attachment_is_ambiguous(parts, locs);
    if locs == ["1"]
        && parts.is_ring
        && parts.principal_group.is_none()
        && parts.unsaturations.is_empty()
        && !parts.is_substituent
        && parts.a_prefixes.is_empty()
        && grouped_count == 1
        && !parts.is_bicycle
        && !parts.is_spiro
        && !parts.is_polycycle
        && !parts.spiro_prefix_composition
        && !ambiguous
    {
        return String::new();
    }
    if substituent_locant_set_is_unique(parts, locs, grouped_count) {
        return String::new();
    }
    locs.join(",")
}
fn outer_parentheses_are_redundant(name: &str) -> bool {
    let inner = &name[1..name.len() - 1];
    let mut depth = 0_i32;
    let mut groups = Vec::new();
    for (i, c) in inner.char_indices() {
        if c == '(' {
            if depth == 0 {
                groups.push((i, 0));
            }
            depth += 1;
        } else if c == ')' {
            depth -= 1;
            if depth < 0 {
                return false;
            }
            if depth == 0 {
                let Some(group) = groups.last_mut() else {
                    return false;
                };
                group.1 = i;
            }
        }
    }
    if depth != 0 || groups.len() > 1 {
        return false;
    }
    if groups.is_empty() {
        return !starts_with_multiplier(inner);
    }
    let (start, end) = groups[0];
    let only_locants = start == 0
        || inner[..start]
            .trim_end_matches(|c: char| c.is_ascii_digit() || ",'-".contains(c))
            .is_empty();
    only_locants && !starts_with_multiplier(&inner[end + 1..])
}
pub fn format_substituent_prefixes(parts: &AssemblyParts) -> String {
    let mut grouped: BTreeMap<&str, Vec<_>> = BTreeMap::new();
    let mut names = Vec::new();
    for sub in &parts.substituents {
        let items = grouped.entry(sub.name.as_str()).or_default();
        if items.is_empty() {
            names.push(sub.name.as_str());
        }
        items.push(sub);
    }
    // Python's dict preserves first citation order when alphabetical keys tie.
    names.sort_by_key(|s| substituent_sort_key(s));
    let mut rendered = String::new();
    for name in names {
        let items = &grouped[name];
        let optional = items.iter().all(|i| i.outer_parentheses_optional);
        let mut locs: Vec<_> = items
            .iter()
            .flat_map(|i| i.locants.iter().cloned())
            .collect();
        locs.sort_by_key(|l| parse_locant(l));
        let per = if name.contains("diyl") && !name.contains("ylidene") {
            2
        } else {
            1
        };
        let count = (if locs.is_empty() {
            items.len()
        } else {
            locs.len()
        } / per)
            .max(1);
        let complex = is_complex_prefix(name) || (count > 1 && needs_complex_multiplier(name));
        let mult = if count > 1 {
            if complex {
                multipliers::complex_(count)
            } else {
                multipliers::basic(count)
            }
        } else {
            String::new()
        };
        let loc = substituent_locant_string(parts, name, &locs, grouped.len());
        let mut used = name.to_owned();
        if !parts.is_substituent
            && parts.principal_group.is_none()
            && count == 1
            && loc.is_empty()
            && grouped.len() == 1
            && optional
            && is_fully_enclosed(name)
            && outer_parentheses_are_redundant(name)
        {
            used = name[1..name.len() - 1].into();
        }
        if complex && !is_fully_enclosed(&used) {
            // Upstream divergence: bracket a diazenyl sibling to preserve its attachment to the parent.
            if count > 1 || !loc.is_empty() || (grouped.len() > 1 && name == "diazenyl") {
                used = format!("({used})");
            }
        } else if loc.is_empty()
            && grouped.len() > 1
            && !is_fully_enclosed(&used)
            && !["fluoro", "chloro", "bromo", "iodo"].contains(&name)
        {
            used = format!("({used})");
        }
        let piece = if loc.is_empty() {
            format!("{mult}{used}")
        } else {
            format!("{loc}-{mult}{used}")
        };
        if needs_hyphen(&rendered, &piece) {
            rendered.push('-');
        }
        rendered.push_str(&piece);
    }
    rendered
}
pub fn format_replacement_prefixes(parts: &AssemblyParts) -> String {
    if super::assembly_charge::inferred_ionic_retained_parent(parts).is_some() {
        return String::new();
    }
    let charged = super::assembly_charge::single_charged_replacement_locants(parts);
    let mut groups: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for item in &parts.a_prefixes {
        let name =
            if item.name == "aza" && item.locants.first().is_some_and(|l| charged.contains(l)) {
                RULES
                    .charges
                    .replacement_charge_prefixes
                    .get("aza:+")
                    .unwrap_or(&item.name)
            } else {
                &item.name
            };
        groups
            .entry(name)
            .or_default()
            .extend(item.locants.iter().map(String::as_str));
    }
    let mut groups: Vec<_> = groups.into_iter().collect();
    groups.sort_by_key(|(name, _)| {
        RULES
            .assembly
            .replacement_prefix_order
            .get(*name)
            .copied()
            .unwrap_or(99)
    });
    groups
        .into_iter()
        .map(|(name, mut locs)| {
            locs.sort_by_key(|l| parse_locant(l));
            let mult = if locs.len() > 1 {
                multipliers::basic(locs.len())
            } else {
                String::new()
            };
            format!("{}-{mult}{name}", locs.join(","))
        })
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    #[test]
    fn sibling_diazenyl_keeps_its_carbonyl_attachment() {
        assert_eq!(
            super::super::name_smiles("C1=CC(=C2C=C(NC2=C1)C(=O)N=N)CN=[N+]=[N-]")
                .map(|name| name.text)
                .as_deref(),
            Ok("[4-(azidomethyl)-1H-indol-2-yl](diazenyl)methanone"),
        );
    }
}
