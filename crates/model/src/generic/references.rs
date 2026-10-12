//! Guard inherited drawing references against changed components.
use super::{
    Work, drawing_size,
    geometry::{at, at_mut},
};
use crate::document::Document;
use std::collections::{HashMap, HashSet};

/// Native captions and attached endpoints cannot silently survive chemical or
/// moved-component changes. This structural dependency check does not verify
/// arbitrary saved name/SMILES claims or invoke name/identifier generation.
pub(super) fn protect_references(doc: &Document, work: &mut Work) -> Result<(), String> {
    let members = doc
        .molecule_names
        .iter()
        .map(|n| n.atoms.len())
        .chain(doc.depth_appearance.iter().map(|s| s.atoms.len()))
        .chain(doc.atoms.iter().map(|a| a.centroid.len()))
        .chain(doc.generic.repeats.iter().map(|r| {
            r.members
                .len()
                .saturating_add(r.crossings.len().saturating_mul(2))
        }))
        .fold(0usize, usize::saturating_add);
    work.charge(
        drawing_size(doc)
            .saturating_add(members)
            .saturating_add(doc.arrows.len().saturating_mul(2))
            .saturating_mul(32),
    )?;
    let index: HashMap<_, _> = doc
        .atoms
        .iter()
        .enumerate()
        .map(|(i, a)| (a.id, i))
        .collect();
    let mut adjacent = vec![Vec::new(); doc.atoms.len()];
    for b in &doc.bonds {
        let a = *index
            .get(&b.a)
            .ok_or("Missing source dependency endpoint")?;
        let z = *index
            .get(&b.b)
            .ok_or("Missing source dependency endpoint")?;
        at_mut(&mut adjacent, a)?.push(z);
        at_mut(&mut adjacent, z)?.push(a);
    }
    let mut seeds: HashSet<_> = doc
        .generic
        .alternatives
        .iter()
        .map(|s| s.atom)
        .chain(doc.generic.links.iter().map(|s| s.atom))
        .chain(doc.generic.repeats.iter().flat_map(|r| {
            r.members
                .iter()
                .copied()
                .chain(r.crossings.iter().flat_map(|c| [c.inside, c.outside]))
        }))
        .collect();
    for a in doc.atoms.iter().filter(|a| a.is_variable_attachment()) {
        seeds.insert(a.id);
        seeds.extend(a.centroid.iter().copied());
    }
    let mut labels = vec![usize::MAX; doc.atoms.len()];
    let mut sizes = Vec::new();
    let mut changed = vec![false; doc.atoms.len()];
    for start in 0..doc.atoms.len() {
        if *at(&labels, start)? != usize::MAX {
            continue;
        }
        let label = sizes.len();
        let mut pending = vec![start];
        let mut component = Vec::new();
        *at_mut(&mut labels, start)? = label;
        while let Some(i) = pending.pop() {
            component.push(i);
            for &j in at(&adjacent, i)? {
                if *at(&labels, j)? == usize::MAX {
                    *at_mut(&mut labels, j)? = label;
                    pending.push(j);
                }
            }
        }
        let touched = component
            .iter()
            .any(|&i| doc.atoms.get(i).is_some_and(|a| seeds.contains(&a.id)));
        for &i in &component {
            *at_mut(&mut changed, i)? = touched;
        }
        sizes.push(component.len());
    }
    for name in &doc.molecule_names {
        let first = *index
            .get(name.atoms.first().ok_or("Empty named component")?)
            .ok_or("Missing named atom")?;
        let label = *at(&labels, first)?;
        if name.atoms.len() != *at(&sizes, label)?
            || name
                .atoms
                .iter()
                .any(|id| index.get(id).is_none_or(|&i| labels.get(i) != Some(&label)))
        {
            return Err("Generated drawings require complete inherited named components; the source drawing is retained".into());
        }
        if doc
            .annotations
            .iter()
            .find(|a| a.id == name.annotation)
            .is_none_or(|a| a.text != name.text)
        {
            return Err("Inherited generated-caption text does not match its association; the source drawing is retained".into());
        }
        if *at(&changed, first)? {
            return Err(reference_error("generated name captions"));
        }
    }
    let changed_ids: HashSet<_> = doc
        .atoms
        .iter()
        .zip(&changed)
        .filter(|(_, flag)| **flag)
        .map(|(a, _)| a.id)
        .collect();
    if doc
        .arrows
        .iter()
        .flat_map(|a| a.start_anchor.iter().chain(a.end_anchor.iter()))
        .any(|anchor| anchor_touches(anchor, &changed_ids))
    {
        return Err(reference_error("attached arrows"));
    }
    // No clone-membership/frozen-weight policy exists in current materializers.
    // Retain rather than silently create unpainted replicas or widen a scope.
    if doc
        .depth_appearance
        .iter()
        .any(|s| s.atoms.iter().any(|id| changed_ids.contains(id)))
    {
        return Err(reference_error("depth appearance scopes"));
    }
    Ok(())
}

pub(super) fn anchor_touches(anchor: &crate::arrow_anchors::Anchor, ids: &HashSet<u64>) -> bool {
    anchor.target.atoms().any(|id| ids.contains(&id))
}

pub(super) fn reference_error(kind: &str) -> String {
    format!(
        "Generic drawing expansion involving {kind} on a changed component is unsupported; the source drawing is retained"
    )
}

#[cfg(test)]
mod tests;
