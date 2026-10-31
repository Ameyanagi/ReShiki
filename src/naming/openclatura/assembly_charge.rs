//! Charge suffix assembly on supported chain and monocyclic parents.
use super::{
    assembly_parts::{AssemblyParts, ParentChargeItem},
    assembly_utils::parse_locant,
    nomenclature::RULES,
    rules::multipliers,
};
use std::collections::BTreeSet;
pub fn positive_parent_n_charges(parts: &AssemblyParts) -> Vec<&ParentChargeItem> {
    parts
        .parent_charges
        .iter()
        .filter(|c| c.symbol == "N" && c.charge > 0)
        .collect()
}
pub fn has_ionic_retained_parent(parts: &AssemblyParts) -> bool {
    parts
        .retained_name
        .as_ref()
        .is_some_and(|n| RULES.charges.retained_ionic_n_parents.contains_key(n))
        && !positive_parent_n_charges(parts).is_empty()
}
pub fn inferred_ionic_retained_parent(parts: &AssemblyParts) -> Option<String> {
    if parts.retained_name.is_some()
        || !parts.is_ring
        || parts.is_bicycle
        || parts.is_spiro
        || parts.is_polycycle
        || !parts.unsaturations.is_empty()
        || positive_parent_n_charges(parts).len() != 1
    {
        return None;
    }
    let aza: Vec<_> = parts
        .a_prefixes
        .iter()
        .filter(|s| s.name == "aza")
        .flat_map(|s| s.locants.iter())
        .collect();
    if aza.len() != 1 {
        return None;
    }
    RULES
        .charges
        .saturated_n_ring_ionic_parents
        .get(&parts.parent_length)
        .cloned()
}
pub fn has_retained_like_parent(parts: &AssemblyParts) -> bool {
    parts.retained_name.is_some() || inferred_ionic_retained_parent(parts).is_some()
}
pub fn single_charged_replacement_locants(parts: &AssemblyParts) -> BTreeSet<String> {
    if parts.retained_name.is_some() || inferred_ionic_retained_parent(parts).is_some() {
        return BTreeSet::new();
    }
    let n: BTreeSet<_> = positive_parent_n_charges(parts)
        .iter()
        .map(|s| s.locant.clone())
        .collect();
    let aza: Vec<_> = parts
        .a_prefixes
        .iter()
        .filter(|s| s.name == "aza")
        .flat_map(|s| s.locants.iter())
        .collect();
    if aza.len() == 1 && n.contains(aza[0]) {
        BTreeSet::from([aza[0].clone()])
    } else {
        BTreeSet::new()
    }
}
pub fn parent_charge_suffix(parts: &AssemblyParts) -> String {
    if has_ionic_retained_parent(parts) || inferred_ionic_retained_parent(parts).is_some() {
        return String::new();
    }
    let represented = single_charged_replacement_locants(parts);
    let mut locs: Vec<_> = parts
        .parent_charges
        .iter()
        .filter(|c| {
            c.charge > 0
                && ["N", "O", "S", "Se", "Te", "P", "As", "Sb", "Bi"].contains(&c.symbol.as_str())
                && !represented.contains(&c.locant)
        })
        .map(|c| c.locant.clone())
        .collect();
    locs.sort_by_key(|l| parse_locant(l));
    if locs.is_empty() {
        return String::new();
    }
    let suffix = RULES
        .charges
        .parent_charge_suffixes
        .get("N:+")
        .map_or("ium", |r| r.suffix.as_str());
    format!(
        "-{}-{}{suffix}",
        locs.join(","),
        if locs.len() > 1 {
            multipliers::basic(locs.len())
        } else {
            String::new()
        }
    )
}
pub fn append_charge_suffixes_to_terminal(parts: &AssemblyParts, terminal: &str) -> String {
    format!("{}{terminal}", parent_charge_suffix(parts))
}
