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
    parts.retained_name.is_some()
        || parts
            .parent_hydride
            .as_ref()
            .is_some_and(|parent| parent.is_fusion_parent())
        || parts.parent_hydride.is_none() && parts.fusion_plan.is_some()
        || inferred_ionic_retained_parent(parts).is_some()
}
fn fusion_plan(parts: &AssemblyParts) -> Option<&super::fusion::model::FusionParentPlan> {
    if let Some(parent) = &parts.parent_hydride {
        parent
            .is_fusion_parent()
            .then_some(parent.fusion_plan.as_ref())
            .flatten()
    } else {
        parts.fusion_plan.as_ref()
    }
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
    if let Some(suffix) = fusion_parent_charge_suffix(parts) {
        return suffix;
    }
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

fn fusion_parent_charge_suffix(parts: &AssemblyParts) -> Option<String> {
    let plan = fusion_plan(parts).filter(|plan| plan.audit.confirmed())?;
    if !plan
        .charge_operations
        .iter()
        .any(|op| op.observed_charge == -1)
    {
        return None;
    }
    let nitrogen_anion = plan
        .charge_operations
        .iter()
        .any(|op| op.symbol == "N" && op.observed_charge == -1);
    let mut result = String::new();
    for sign in if nitrogen_anion { [-1, 1] } else { [1, -1] } {
        let mut locants: Vec<_> = plan
            .charge_operations
            .iter()
            .filter(|op| op.observed_charge == sign)
            .filter_map(|op| {
                parts
                    .parent_atom_ids_by_locant
                    .iter()
                    .find(|(_, id)| **id == op.atom_id)
                    .map(|(locant, _)| locant.clone())
            })
            .collect();
        if locants.is_empty() {
            continue;
        }
        locants.sort_by_key(|l| parse_locant(l));
        let suffix = RULES
            .charges
            .parent_charge_suffixes
            .get(if sign > 0 { "N:+" } else { "*:-" })
            .map_or(if sign > 0 { "ium" } else { "ide" }, |rule| {
                rule.suffix.as_str()
            });
        result.push_str(&format!(
            "-{}-{}{suffix}",
            locants.join(","),
            if locants.len() > 1 {
                multipliers::basic(locants.len())
            } else {
                String::new()
            }
        ));
    }
    Some(result)
}

/// Check the graph-bound charge proof before any parent suffix is rendered.
pub fn prepare_fusion_charge_assembly(parts: &mut AssemblyParts) -> Result<(), String> {
    let Some(plan) = fusion_plan(parts) else {
        return Ok(());
    };
    if !plan.audit.confirmed() {
        return Err("openclatura: fusion charge assembly requires a confirmed plan".into());
    }
    let selected: std::collections::BTreeMap<_, _> = parts
        .parent_atom_ids_by_locant
        .iter()
        .map(|(l, &id)| (id, l.clone()))
        .collect();
    if !plan
        .numbering
        .string_input_locant_maps()
        .contains(&selected)
    {
        return Err(
            "openclatura: fusion parent charge spelling requires an audited complete numbering"
                .into(),
        );
    }
    let expected: BTreeSet<_> = plan
        .charge_operations
        .iter()
        .map(|op| {
            (
                op.atom_id,
                selected.get(&op.atom_id).cloned(),
                op.symbol.clone(),
                op.observed_charge,
            )
        })
        .collect();
    let observed: BTreeSet<_> = parts
        .parent_charges
        .iter()
        .filter_map(|charge| {
            charge.atom_id.map(|id| {
                (
                    id,
                    Some(charge.locant.clone()),
                    charge.symbol.clone(),
                    charge.charge,
                )
            })
        })
        .collect();
    if expected != observed {
        return Err(
            "openclatura: fusion parent charge spelling does not match its graph-bound operations"
                .into(),
        );
    }
    if !plan
        .charge_operations
        .iter()
        .any(|op| op.observed_charge == -1)
    {
        return Ok(());
    }
    let negative_c: BTreeSet<_> = parts
        .parent_charges
        .iter()
        .filter(|charge| charge.symbol == "C" && charge.charge == -1)
        .filter_map(|charge| charge.atom_id)
        .collect();
    let mut consumed = BTreeSet::new();
    parts.hydro_operations.retain(|op| {
        let consume = op.key == "added_hydrogen"
            && !op.atom_ids.is_empty()
            && op.atom_ids.iter().all(|id| negative_c.contains(id));
        if consume {
            consumed.extend(op.locants.iter().cloned());
        }
        !consume
    });
    parts
        .indicated_hydrogens
        .retain(|locant| !consumed.contains(locant));
    if parts
        .principal_group
        .as_ref()
        .is_some_and(|g| g.key == "ketone")
        && let Some(group) = parts.principal_group.take()
    {
        let mut prefix = super::assembly_parts::SubstituentItem::new(
            RULES
                .functional_groups
                .get(&group.key)
                .and_then(|rule| rule.prefix.as_ref())
                .map_or("oxo", String::as_str),
            group.locants,
        );
        prefix.atom_ids = group.atom_ids;
        prefix.bond_ids = group.bond_ids;
        prefix.charge_atom_ids = group.charge_atom_ids;
        parts.substituents.push(prefix);
    }
    Ok(())
}
pub fn append_charge_suffixes_to_terminal(parts: &AssemblyParts, terminal: &str) -> String {
    format!("{}{terminal}", parent_charge_suffix(parts))
}
