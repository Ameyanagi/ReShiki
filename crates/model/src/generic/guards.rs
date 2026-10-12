use crate::document::Document;
use std::collections::{HashMap, HashSet};

pub(super) fn reject_affected_stereo(doc: &Document, ids: &[u64]) -> Result<(), String> {
    let affected: HashSet<_> = ids.iter().copied().collect();
    let neighbors: HashSet<_> = doc
        .bonds
        .iter()
        .filter_map(|b| {
            if affected.contains(&b.a) {
                Some(b.b)
            } else if affected.contains(&b.b) {
                Some(b.a)
            } else {
                None
            }
        })
        .collect();
    let atom_stereo = doc.atoms.iter().any(|a| {
        a.stereo.as_ref().is_some_and(|s| {
            affected.contains(&a.id) || s.neighbors.iter().any(|id| affected.contains(id))
        })
    });
    let bond_stereo = doc.bonds.iter().any(|b| {
        // Neighbor bonds count only authoritative stereo or a chemical marker,
        // a narrower rule than Bond::specifies_stereo.
        let chemical_marker = !b.projection && crate::bonds::is_stereo_marker(&b.display);
        b.specifies_stereo()
            && (affected.contains(&b.a)
                || affected.contains(&b.b)
                || b.stereo_atoms.iter().any(|id| affected.contains(id))
                || (b.stereo_authoritative || chemical_marker)
                    && (neighbors.contains(&b.a) || neighbors.contains(&b.b)))
    });
    if atom_stereo || bond_stereo {
        Err("Generic expansion involving specified atom or bond stereochemistry is unsupported; the source drawing is retained".into())
    } else {
        Ok(())
    }
}
/// Shared guard for operations that remap, delete or infer chemistry for IDs.
/// Pure coordinate transforms can preserve definitions without this guard.
pub fn intersects(doc: &Document, ids: &[u64]) -> bool {
    let selected: HashSet<_> = ids.iter().copied().collect();
    doc.generic
        .alternatives
        .iter()
        .any(|s| selected.contains(&s.atom))
        || doc.generic.links.iter().any(|s| selected.contains(&s.atom))
        || doc.generic.repeats.iter().any(|u| {
            u.bracket.is_some_and(|id| selected.contains(&id))
                || u.bracket_objects.iter().any(|id| selected.contains(id))
                || u.crossing_brackets.iter().any(|id| selected.contains(id))
                || u.members.iter().any(|id| selected.contains(id))
                || u.crossings
                    .iter()
                    .any(|c| selected.contains(&c.inside) || selected.contains(&c.outside))
        })
        || doc.atoms.iter().any(|a| {
            a.is_variable_attachment()
                && (selected.contains(&a.id) || a.centroid.iter().any(|id| selected.contains(id)))
        })
}

/// Whether IDs or their connected components reference generic chemistry.
pub fn touches(doc: &Document, ids: &[u64]) -> bool {
    intersects(doc, ids) || component_intersects(doc, ids)
}

/// Borrow chemical atom dependencies without copying alternative strings or
/// repeat metadata. Bracket graphics are presentation, not chemical components.
pub fn atom_references(doc: &Document) -> impl Iterator<Item = u64> + '_ {
    doc.generic
        .alternatives
        .iter()
        .map(|site| site.atom)
        .chain(doc.generic.links.iter().map(|site| site.atom))
        .chain(doc.generic.repeats.iter().flat_map(|unit| {
            unit.members.iter().copied().chain(
                unit.crossings
                    .iter()
                    .flat_map(|port| [port.inside, port.outside]),
            )
        }))
        .chain(
            doc.atoms
                .iter()
                .filter(|atom| atom.is_variable_attachment())
                .flat_map(|atom| std::iter::once(atom.id).chain(atom.centroid.iter().copied())),
        )
}

/// Reject unresolved chemistry only in the connected components of `ids`.
/// One borrowed reference scan and one graph index avoid cloning metadata or
/// scanning every definition again for each selected atom/component.
/// This traversal also follows centroid edges. It stays separate from
/// `IndexedGraph` because its selected-arrow seeds and early unresolved-neighbor
/// exits have different semantics.
pub fn component_intersects(doc: &Document, ids: &[u64]) -> bool {
    let unresolved: HashSet<_> = atom_references(doc).collect();
    if unresolved.is_empty() {
        return false;
    }
    let selected: HashSet<_> = ids.iter().copied().collect();
    let mut visited = selected.clone();
    // Only explicitly carried drawing links are source dependencies. An
    // unselected arrow must not connect otherwise separate chemical components.
    for arrow in doc
        .arrows
        .iter()
        .filter(|arrow| selected.contains(&arrow.id))
    {
        for anchor in arrow.start_anchor.iter().chain(arrow.end_anchor.iter()) {
            visited.extend(anchor.target.atoms());
        }
    }
    if visited.iter().any(|id| unresolved.contains(id)) {
        return true;
    }
    let mut adjacent = HashMap::<u64, Vec<u64>>::new();
    for (a, b) in doc.bonds.iter().map(|bond| (bond.a, bond.b)).chain(
        doc.atoms
            .iter()
            .flat_map(|atom| atom.centroid.iter().map(move |id| (atom.id, *id))),
    ) {
        adjacent.entry(a).or_default().push(b);
        adjacent.entry(b).or_default().push(a);
    }
    let mut pending: Vec<_> = visited.iter().copied().collect();
    while let Some(id) = pending.pop() {
        for neighbor in adjacent.get(&id).into_iter().flatten() {
            if unresolved.contains(neighbor) {
                return true;
            }
            if visited.insert(*neighbor) {
                pending.push(*neighbor);
            }
        }
    }
    false
}

#[cfg(test)]
mod tests;
