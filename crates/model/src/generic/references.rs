//! Guard inherited drawing references against changed components.
use super::{
    Work, drawing_size, errors::retained_text, geometry::at, graph::IndexedGraph,
    guards::atom_references,
};
use crate::{document::Document, molecule_names::MoleculeName};
use std::collections::HashSet;

/// Native captions and attached endpoints cannot silently survive chemical or
/// moved-component changes. This structural dependency check does not verify
/// arbitrary saved name/SMILES claims or invoke name/identifier generation.
pub(super) fn protect_references(doc: &Document, work: &mut Work) -> Result<(), String> {
    const INCOMPLETE_NAME: &str =
        retained_text!("Generated drawings require complete inherited named components");
    const CAPTION_MISMATCH: &str =
        retained_text!("Inherited generated-caption text does not match its association");
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
    let graph = IndexedGraph::from_edges(
        doc,
        doc.bonds.iter().map(|bond| (bond.a, bond.b)),
        "Missing source dependency endpoint",
    )?;
    let seeds: HashSet<_> = atom_references(doc).collect();
    let (labels, components) = graph.components()?;
    let sizes: Vec<_> = components.iter().map(Vec::len).collect();
    let changed_ids: HashSet<_> = components
        .iter()
        .filter(|component| {
            component
                .iter()
                .any(|&i| doc.atoms.get(i).is_some_and(|a| seeds.contains(&a.id)))
        })
        .flat_map(|component| component.iter())
        .filter_map(|&i| doc.atoms.get(i).map(|a| a.id))
        .collect();
    reject_changed_references(doc, &changed_ids, |name| {
        let first = *graph
            .indices
            .get(name.atoms.first().ok_or("Empty named component")?)
            .ok_or("Missing named atom")?;
        let label = *at(&labels, first)?;
        if name.atoms.len() != *at(&sizes, label)?
            || name.atoms.iter().any(|id| {
                graph
                    .indices
                    .get(id)
                    .is_none_or(|&i| labels.get(i) != Some(&label))
            })
        {
            return Err(INCOMPLETE_NAME.into());
        }
        if doc
            .annotations
            .iter()
            .find(|a| a.id == name.annotation)
            .is_none_or(|a| a.text != name.text)
        {
            return Err(CAPTION_MISMATCH.into());
        }
        Ok(())
    })
}

/// Refuse name captions, attached arrows and depth scopes on changed atoms, in
/// that order. `check_name` runs before each name's changed test so callers
/// keep their per-name error order (protect_references, depiction::Plan::new).
pub(super) fn reject_changed_references(
    doc: &Document,
    changed_ids: &HashSet<u64>,
    mut check_name: impl FnMut(&MoleculeName) -> Result<(), String>,
) -> Result<(), String> {
    for name in &doc.molecule_names {
        check_name(name)?;
        // protect_references' check_name proves the name covers one whole
        // component, and `changed_ids` holds whole components, so testing any
        // atom equals its former first-atom test.
        if name.atoms.iter().any(|id| changed_ids.contains(id)) {
            return Err(reference_error("generated name captions"));
        }
    }
    if doc
        .arrows
        .iter()
        .flat_map(|a| a.start_anchor.iter().chain(a.end_anchor.iter()))
        .any(|anchor| anchor_touches(anchor, changed_ids))
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
    if doc
        .models
        .iter()
        .any(|m| m.linked_ids(doc).iter().any(|id| changed_ids.contains(id)))
    {
        return Err(reference_error("3D models"));
    }
    Ok(())
}

pub(super) fn anchor_touches(anchor: &crate::arrow_anchors::Anchor, ids: &HashSet<u64>) -> bool {
    anchor.target.atoms().any(|id| ids.contains(&id))
}

pub(super) fn reference_error(kind: &str) -> String {
    format!(
        retained_text!(
            "Generic drawing expansion involving {kind} on a changed component is unsupported"
        ),
        kind = kind,
    )
}

#[cfg(test)]
mod tests;
