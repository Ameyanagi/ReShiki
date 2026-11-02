//! Opt-in nitrogen charge separation using the same cleanup as analysis.
use crate::{
    chemistry::{self, graph},
    document::Document,
};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy)]
struct Change {
    nitrogen: u64,
    other: u64,
    bond: usize,
    order: u8,
}

/// Selected neutral nitrogen centers that can be redrawn as N+–O− or
/// N+=N−, in atom order. IDs select centers; neighbors need not be selected.
/// Collapsed labels, generic atoms and crystal distance graphs are excluded.
pub fn eligible_atoms(document: &Document, atoms: &[u64]) -> Vec<u64> {
    let Ok(changes) = changes(document, atoms) else {
        return Vec::new();
    };
    let centers: HashSet<_> = changes.iter().map(|change| change.nitrogen).collect();
    document
        .atoms
        .iter()
        .filter_map(|atom| centers.contains(&atom.id).then_some(atom.id))
        .collect()
}

/// Redraw eligible selected nitrogen centers with explicit formal charges.
/// Nitrogen oxides lose one N=O order, azides lose one N≡N order, and
/// N->O oxide arrows become plain single bonds. Sulfur, phosphorus and metal
/// datives are untouched. Hydrogen inputs and coordinates are preserved.
///
/// Returns the number of changed centers, or zero when none are eligible.
/// The caller owns the single undo transaction. Errors leave the source intact.
pub fn draw_as_charge_separated(document: &mut Document, atoms: &[u64]) -> Result<usize, String> {
    let changes = changes(document, atoms)?;
    if changes.is_empty() {
        return Ok(0);
    }
    let mut candidate = document.clone();
    let centers: HashSet<_> = changes.iter().map(|change| change.nitrogen).collect();
    // This redraw retains molecular identity. Retain captured stereo on
    // neighboring bonds, including the C=N bond in a nitrone.
    crate::atom_labels::clear_computed(&mut candidate);
    for change in changes {
        let nitrogen = candidate
            .atom_mut(change.nitrogen)
            .ok_or("Missing nitrogen in depiction edit")?;
        nitrogen.charge = 1;
        nitrogen.display.hide_charge = false;
        let other = candidate
            .atom_mut(change.other)
            .ok_or("Missing dipolar partner in depiction edit")?;
        other.charge = -1;
        other.display.hide_charge = false;
        let bond = candidate
            .bonds
            .get_mut(change.bond)
            .ok_or("Missing bond in depiction edit")?;
        bond.order = change.order;
        bond.display = "plain".into();
        bond.secondary_display = None;
        bond.ring_arc = false;
        bond.projection = false;
        bond.stereo = None;
        bond.stereo_atoms.clear();
        bond.stereo_authoritative = false;
    }
    crate::ring_fills::prune(&mut candidate);
    crate::atom_labels::refresh::refresh_edited(document, &mut candidate);
    crate::arrow_anchors::reconcile(&mut candidate);
    candidate.validate()?;
    *document = candidate;
    Ok(centers.len())
}

fn changes(document: &Document, atoms: &[u64]) -> Result<Vec<Change>, String> {
    let selected: HashSet<_> = atoms.iter().copied().collect();
    if !document
        .atoms
        .iter()
        .any(|atom| atom.element == "N" && atom.charge == 0 && selected.contains(&atom.id))
    {
        return Ok(Vec::new());
    }
    document.validate()?;
    // A bond to a drawing anchor is not a chemical valence contribution, so
    // eligibility is not inferred from an anchor's neighborhood.
    let anchors: HashSet<_> = crate::generic::atom_references(document)
        .chain(document.atoms.iter().filter_map(|atom| {
            (!atom.centroid.is_empty()
                || atom.attachment.is_some()
                || atom.display.variable.is_some())
            .then_some(atom.id)
        }))
        .collect();
    let excluded: HashSet<_> = document
        .abbreviations
        .iter()
        .flat_map(|label| label.members.iter().copied())
        .chain(crate::models3d::crystal::analysis_atoms(document))
        .chain(document.bonds.iter().flat_map(|bond| {
            (anchors.contains(&bond.a) || anchors.contains(&bond.b))
                .then_some([bond.a, bond.b])
                .into_iter()
                .flatten()
        }))
        .chain(anchors.iter().copied())
        .collect();
    // Valence-only input ignores unrelated drawing anchors and generic metadata.
    let indices: HashMap<_, _> = document
        .atoms
        .iter()
        .enumerate()
        .map(|(index, atom)| (atom.id, index))
        .collect();
    let atoms = document
        .atoms
        .iter()
        .map(|atom| {
            Ok(graph::Atom {
                atomic_number: chemistry::ELEMENTS
                    .iter()
                    .position(|element| element.symbol == atom.element)
                    .and_then(|number| u8::try_from(number).ok())
                    .ok_or("Unknown element in depiction edit")?,
                isotope: atom.isotope.try_into().map_err(|_| "Invalid isotope")?,
                charge: atom.charge.try_into().map_err(|_| "Invalid charge")?,
                explicit_hydrogens: atom
                    .explicit_h
                    .try_into()
                    .map_err(|_| "Invalid hydrogen count")?,
                no_implicit: atom.no_implicit,
                aromatic: atom.aromatic,
                radical_electrons: atom.radical_electrons,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let bonds = document
        .bonds
        .iter()
        .map(|bond| {
            Ok(graph::Bond {
                a: *indices.get(&bond.a).ok_or("Missing bond endpoint")?,
                b: *indices.get(&bond.b).ok_or("Missing bond endpoint")?,
                order: bond.order,
                aromatic: bond.order == 4,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let graph = graph::Graph { atoms, bonds };
    let normalized = chemistry::normalize::functional_groups(&graph)?;
    let mut result = Vec::new();
    for (index, (before, after)) in graph.bonds.iter().zip(&normalized.bonds).enumerate() {
        if before.order == after.order {
            continue;
        }
        for (center, other) in [(before.a, before.b), (before.b, before.a)] {
            let nitrogen = document.atoms.get(center).ok_or("Missing nitrogen")?;
            let partner = document.atoms.get(other).ok_or("Missing dipolar partner")?;
            if nitrogen.element != "N"
                || nitrogen.charge != 0
                || !selected.contains(&nitrogen.id)
                || excluded.contains(&nitrogen.id)
                || excluded.contains(&partner.id)
                || normalized.atoms.get(center).map(|atom| atom.charge) != Some(1)
                || normalized.atoms.get(other).map(|atom| atom.charge) != Some(-1)
            {
                continue;
            }
            result.push(Change {
                nitrogen: nitrogen.id,
                other: partner.id,
                bond: index,
                order: after.order,
            });
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests;
