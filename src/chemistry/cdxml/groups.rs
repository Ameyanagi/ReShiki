//! Logical groups from engine/groups_exchange.py::read_groups.
use super::{Error, ObjectMapEntry, Result, association::invalid, tree::Tree};
use crate::grouping::Group;
use std::collections::HashMap;

/// Read groups in reverse source order, assigning checked consecutive IDs.
/// Mapping keys are root-inclusive element ordinals in this unchanged XML.
/// Mapped elements stop descent, including explicitly empty mappings; repeated
/// entries use the last value. Members may identify any drawing object, not only
/// atoms. Redundant memberships merge their integral flag without consuming IDs.
/// No source, mapping or existing drawing is changed on success or failure.
pub fn read_groups(xml: &str, object_map: &[ObjectMapEntry], first_id: u64) -> Result<Vec<Group>> {
    if object_map.len() > 100_000 {
        return Err(Error::Limit);
    }
    let tree = Tree::parse_import(xml)?;
    let order = tree.descendants(0)?;
    let mut mapped = HashMap::new();
    let mut members = 0usize;
    for entry in object_map {
        let node = *order
            .get(entry.source)
            .ok_or_else(|| invalid("Group object-map ordinal is outside the source document"))?;
        members = members.checked_add(entry.atoms.len()).ok_or(Error::Limit)?;
        if members > 1_000_000 {
            return Err(Error::Limit);
        }
        mapped.insert(node, entry.atoms.as_slice());
    }
    let mut result: Vec<Group> = Vec::new();
    let mut seen = HashMap::new();
    for &element in order.iter().rev() {
        if tree.node(element)?.tag != "group" {
            continue;
        }
        let mut ids = Vec::new();
        let mut pending = vec![element];
        while let Some(node) = pending.pop() {
            tree.spend(1)?;
            if let Some(members) = mapped.get(&node) {
                tree.spend(members.len())?;
                ids.extend_from_slice(members);
                if ids.len() > 1_000_000 {
                    return Err(Error::Limit);
                }
            } else {
                pending.extend(tree.node(node)?.children.iter().rev().copied());
            }
        }
        let sorting = ids
            .len()
            .checked_mul(
                usize::try_from(ids.len().max(1).ilog2())
                    .map_err(|_| Error::Limit)?
                    .saturating_add(1),
            )
            .ok_or(Error::Limit)?;
        tree.spend(sorting)?;
        ids.sort_unstable();
        ids.dedup();
        if ids.len() < 2 {
            continue;
        }
        let integral = tree.node(element)?.attr("Integral").unwrap_or("no") == "yes";
        if let Some(&index) = seen.get(&ids) {
            let group: &mut Group = result
                .get_mut(index)
                .ok_or_else(|| invalid("Missing deduplicated group"))?;
            group.integral |= integral;
            continue;
        }
        let id = first_id
            .checked_add(u64::try_from(result.len()).map_err(|_| Error::Limit)?)
            .ok_or_else(|| invalid("Group ID exceeds the unsigned document range"))?;
        seen.insert(ids.clone(), result.len());
        result.push(Group {
            id,
            members: ids,
            integral,
        });
    }
    Ok(result)
}
