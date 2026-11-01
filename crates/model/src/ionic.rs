//! Explicitly replace selected metal–heteroatom lines with separated ions.
//! This is an opt-in drawing edit; condensed text and collapsed abbreviations
//! stay intact.
use crate::document::{Document, Point};
use std::collections::{HashMap, HashSet};

/// Selected single drawn lines joining Li, Na, K, Rb, Cs, Ca, Sr or Ba to O,
/// N, S or a halogen, as (metal, partner) pairs in document order. Mg, Zn, B
/// and transition metals are left covalent, as are text labels.
pub fn eligible_bonds(document: &Document, selection: &[u64]) -> Vec<(u64, u64)> {
    let selected: HashSet<_> = selection.iter().copied().collect();
    let labels: HashSet<_> = document
        .abbreviations
        .iter()
        .flat_map(|a| a.members.iter().copied())
        .chain(crate::generic::atom_references(document))
        .collect();
    let metal = |e: &str| matches!(e, "Li" | "Na" | "K" | "Rb" | "Cs" | "Ca" | "Sr" | "Ba");
    let anion = |e: &str| matches!(e, "O" | "N" | "S" | "F" | "Cl" | "Br" | "I");
    document
        .bonds
        .iter()
        .filter(|b| b.order == 1 && !b.distance_derived)
        .filter(|b| selected.contains(&b.a) && selected.contains(&b.b))
        .filter_map(|b| {
            let (a, b) = (document.atom(b.a)?, document.atom(b.b)?);
            let (m, o) = if metal(&a.element) && anion(&b.element) {
                (a, b)
            } else if metal(&b.element) && anion(&a.element) {
                (b, a)
            } else {
                return None;
            };
            let plain = |atom: &crate::document::Atom| {
                !labels.contains(&atom.id)
                    && atom.display.variable.is_none()
                    && atom.centroid.is_empty()
                    && atom.attachment.is_none()
            };
            (plain(m) && plain(o) && document.bond_drawn(m.id, o.id)).then_some((m.id, o.id))
        })
        .collect()
}

/// Delete the eligible selected lines, add +1/−1 per line to the metal and its
/// partner, and move each metal one bond length along the sum of its outward
/// directions (it stays put when they cancel). Returns the number of lines.
pub fn make_ionic(document: &mut Document, selection: &[u64]) -> Result<usize, String> {
    let pairs = eligible_bonds(document, selection);
    if pairs.is_empty() {
        return Err(
            "Select a single bond joining Li, Na, K, Rb, Cs, Ca, Sr or Ba to O, N, S or a halogen"
                .into(),
        );
    }
    let mut candidate = document.clone();
    let mut charges = HashMap::<u64, i32>::new();
    let mut directions = HashMap::<u64, Point>::new();
    for &(metal, other) in &pairs {
        *charges.entry(metal).or_default() += 1;
        *charges.entry(other).or_default() -= 1;
        let at = |id| document.atom(id).map_or(Point::default(), |a| a.position);
        let (m, o) = (at(metal), at(other));
        let (dx, dy) = (m.x - o.x, m.y - o.y);
        let length = dx.hypot(dy);
        let direction = directions.entry(metal).or_default();
        direction.x += dx / length;
        direction.y += dy / length;
    }
    let affected: Vec<_> = charges.keys().copied().collect();
    candidate.invalidate_chemistry(&affected);
    candidate.bonds.retain(|b| {
        !pairs
            .iter()
            .any(|&(m, o)| (b.a, b.b) == (m, o) || (b.a, b.b) == (o, m))
    });
    for atom in &mut candidate.atoms {
        if let Some(delta) = charges.get(&atom.id) {
            atom.charge += delta;
            atom.display.hide_charge = false;
        }
    }
    let distance = candidate.drawing_style.bond_length_world;
    for (id, d) in directions {
        let length = d.x.hypot(d.y);
        if length > 1e-3 {
            candidate.translate(&[id], d.x / length * distance, d.y / length * distance);
        }
    }
    crate::ring_fills::prune(&mut candidate);
    crate::arrow_anchors::reconcile(&mut candidate);
    crate::atom_labels::refresh::refresh_edited(document, &mut candidate);
    candidate.validate()?;
    *document = candidate;
    Ok(pairs.len())
}

#[cfg(test)]
mod tests;
