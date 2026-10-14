//! Molecular scope and native dependencies for explicit expansion commands.
use super::{
    Limits, Work, drawing_size,
    probe::{self, Stage},
};
use crate::document::Document;
use std::collections::{HashMap, HashSet};

const WHOLE_MOLECULES: &str =
    "Select whole molecules, or clear the selection to expand the whole drawing";

/// Resolve the molecular component and its native drawing dependencies with the
/// same bounded traversal used to prepare an explicit component expansion.
pub fn component_scope(doc: &Document, hit: &[u64]) -> Result<Vec<u64>, String> {
    let limits = Limits::default();
    limits.check(doc, Some(hit))?;
    component(doc, hit, &mut Work::new(limits.max_work))
}

// Collection lengths are O(1). Debit them before inspecting any record, including
// the group and abbreviation passes performed by drawing_size.
fn record_count(doc: &Document) -> usize {
    doc.atoms
        .len()
        .saturating_add(doc.bonds.len())
        .saturating_add(doc.annotations.len())
        .saturating_add(doc.arrows.len())
        .saturating_add(doc.graphics.len())
        .saturating_add(doc.groups.len())
        .saturating_add(doc.abbreviations.len())
        .saturating_add(doc.molecule_names.len())
        .saturating_add(doc.ring_fills.len())
        .saturating_add(doc.depth_appearance.len())
        .saturating_add(doc.reactions.len())
        .saturating_add(doc.generic.alternatives.len())
        .saturating_add(doc.generic.links.len())
        .saturating_add(doc.generic.repeats.len())
}

fn charge_counting(doc: &Document, selected: &[u64], work: &mut Work) -> Result<(), String> {
    work.charge(
        record_count(doc)
            .saturating_mul(2)
            .saturating_add(selected.len()),
    )?;
    probe::enter(Stage::ScopeCount);
    Ok(())
}

pub(super) fn check_molecules(
    doc: &Document,
    selected: &[u64],
    work: &mut Work,
) -> Result<(), String> {
    charge_counting(doc, selected, work)?;
    let refs = doc
        .abbreviations
        .iter()
        .map(|g| g.members.len().saturating_add(1))
        .chain(doc.atoms.iter().map(|a| a.centroid.len()))
        .fold(0usize, usize::saturating_add);
    work.charge(
        drawing_size(doc)
            .saturating_add(refs)
            .saturating_add(selected.len())
            .saturating_mul(32),
    )?;
    probe::enter(Stage::ScopeTraverse);
    let selected = crate::attachments::selection(doc, selected);
    let ids: HashSet<_> = doc
        .expand_abbreviation_selection(&selected)
        .into_iter()
        .collect();
    if doc
        .bonds
        .iter()
        .any(|b| ids.contains(&b.a) != ids.contains(&b.b))
        || doc
            .atoms
            .iter()
            .any(|a| ids.contains(&a.id) && a.centroid.iter().any(|id| !ids.contains(id)))
    {
        return Err(WHOLE_MOLECULES.into());
    }
    Ok(())
}

pub(super) fn component(doc: &Document, hit: &[u64], work: &mut Work) -> Result<Vec<u64>, String> {
    charge_counting(doc, hit, work)?;
    let mut refs = doc
        .generic
        .repeats
        .iter()
        .map(|r| {
            r.members
                .len()
                .saturating_add(r.crossings.len().saturating_mul(2))
                .saturating_add(r.bracket_objects.len())
                .saturating_add(1)
        })
        .chain(doc.atoms.iter().map(|a| a.centroid.len()))
        .chain(doc.groups.iter().map(|g| g.members.len()))
        .chain(
            doc.abbreviations
                .iter()
                .map(|g| g.members.len().saturating_add(1)),
        )
        .chain(
            doc.molecule_names
                .iter()
                .map(|n| n.atoms.len().saturating_add(1)),
        )
        .chain(
            doc.ring_fills
                .iter()
                .map(|g| g.atoms.len().saturating_add(1)),
        )
        .chain(
            doc.depth_appearance
                .iter()
                .map(|g| g.atoms.len().saturating_add(1)),
        )
        .chain(doc.arrows.iter().map(|a| {
            a.start_anchor
                .iter()
                .chain(a.end_anchor.iter())
                .map(|anchor| anchor.target.atoms().count())
                .sum::<usize>()
        }))
        .fold(0usize, usize::saturating_add);
    for reaction in &doc.reactions {
        let participants = reaction
            .reactants
            .len()
            .saturating_add(reaction.products.len())
            .saturating_add(reaction.agents.len());
        work.charge(participants)?;
        probe::enter(Stage::ScopeParticipants);
        refs = refs
            .saturating_add(reaction.annotations.len())
            .saturating_add(1);
        for participant in reaction
            .reactants
            .iter()
            .chain(&reaction.products)
            .chain(&reaction.agents)
        {
            refs = refs.saturating_add(participant.atoms.len().saturating_add(1));
        }
    }
    work.charge(
        drawing_size(doc)
            .saturating_add(refs)
            .saturating_add(hit.len())
            .saturating_mul(64),
    )?;
    probe::enter(Stage::ScopeTraverse);
    let mut adjacent = HashMap::<u64, Vec<u64>>::new();
    fn connect(adjacent: &mut HashMap<u64, Vec<u64>>, ids: impl IntoIterator<Item = u64>) {
        let mut ids = ids.into_iter();
        if let Some(first) = ids.next() {
            for id in ids {
                adjacent.entry(first).or_default().push(id);
                adjacent.entry(id).or_default().push(first);
            }
        }
    }
    for bond in &doc.bonds {
        connect(&mut adjacent, [bond.a, bond.b]);
    }
    for atom in &doc.atoms {
        connect(
            &mut adjacent,
            std::iter::once(atom.id).chain(atom.centroid.iter().copied()),
        );
    }
    for members in doc
        .groups
        .iter()
        .map(|g| g.members.as_slice())
        .chain(doc.abbreviations.iter().map(|g| g.members.as_slice()))
        .chain(doc.ring_fills.iter().map(|g| g.atoms.as_slice()))
        .chain(doc.depth_appearance.iter().map(|g| g.atoms.as_slice()))
    {
        connect(&mut adjacent, members.iter().copied());
    }
    for name in &doc.molecule_names {
        connect(
            &mut adjacent,
            std::iter::once(name.annotation).chain(name.atoms.iter().copied()),
        );
    }
    for unit in &doc.generic.repeats {
        connect(
            &mut adjacent,
            unit.bracket
                .into_iter()
                .chain(unit.bracket_objects.iter().copied())
                .chain(unit.members.iter().copied())
                .chain(unit.crossings.iter().flat_map(|c| [c.inside, c.outside])),
        );
    }
    for arrow in &doc.arrows {
        connect(
            &mut adjacent,
            std::iter::once(arrow.id).chain(
                arrow
                    .start_anchor
                    .iter()
                    .chain(arrow.end_anchor.iter())
                    .flat_map(|a| a.target.atoms()),
            ),
        );
    }
    for reaction in &doc.reactions {
        connect(&mut adjacent, reaction.ids());
    }
    let mut chosen: HashSet<_> = hit.iter().copied().collect();
    let mut pending = hit.to_vec();
    while let Some(id) = pending.pop() {
        for id in adjacent.get(&id).into_iter().flatten() {
            if chosen.insert(*id) {
                pending.push(*id);
            }
        }
    }
    let selected: Vec<_> = doc.object_ids().filter(|id| chosen.contains(id)).collect();
    if selected.is_empty() {
        return Err("Choose a molecule to expand".into());
    }
    Ok(selected)
}

#[cfg(test)]
mod tests;
