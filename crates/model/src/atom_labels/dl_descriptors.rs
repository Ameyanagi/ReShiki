//! Conventional D/L names supplement CIP labels only for recognized structures.
//! This deliberately bounded recognizer does not infer a general relative name.
use crate::document::{Atom, AtomStereo, Document};
use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
};

pub(super) fn descriptors(doc: &Document) -> HashMap<u64, &'static str> {
    let mut result = HashMap::new();
    if !doc.atom_labels.dl_descriptors {
        return result;
    }
    for atom in &doc.atoms {
        if let Some(name) = amino_acid(atom, doc) {
            result.insert(atom.id, name);
        }
    }
    haworth(doc, &mut result);
    aldose(doc, &mut result);
    result.retain(|id, _| {
        doc.atom(*id)
            .is_some_and(|a| matches!(a.cip_label.as_deref(), Some("R" | "S")))
    });
    result
}

fn neighbors(doc: &Document, id: u64) -> Vec<(&Atom, u8)> {
    doc.bonds
        .iter()
        .filter_map(|bond| {
            let other = if bond.a == id {
                bond.b
            } else if bond.b == id {
                bond.a
            } else {
                return None;
            };
            Some((doc.atom(other)?, bond.order))
        })
        .collect()
}

/// Carbon hydrogen count from chemical input, independent of cached label ink.
fn carbon_hydrogens(atom: &Atom, doc: &Document) -> Option<u32> {
    if atom.element != "C" || atom.aromatic || atom.charge != 0 || atom.radical_electrons != 0 {
        return None;
    }
    let adjacent = neighbors(doc, atom.id);
    if adjacent.iter().any(|(_, order)| !(1..=3).contains(order)) {
        return None;
    }
    let valence: u32 = adjacent.iter().map(|(_, order)| u32::from(*order)).sum();
    let available = 4u32.checked_sub(valence)?.checked_sub(atom.explicit_h)?;
    let implicit = if atom.no_implicit { 0 } else { available };
    Some(
        atom.explicit_h
            + implicit
            + adjacent.iter().filter(|(a, _)| a.element == "H").count() as u32,
    )
}

/// C(=O)X of an amino acid or its residue, ester, amide or aldehyde. A ketone
/// (X = C) is not an amino-acid derivative.
fn carbonyl(atom: &Atom, center: u64, doc: &Document) -> bool {
    let adjacent = neighbors(doc, atom.id);
    adjacent
        .iter()
        .any(|(a, order)| a.element == "O" && *order == 2)
        && !adjacent
            .iter()
            .any(|(a, _)| a.id != center && a.element == "C")
}

/// Rule 1a at sphere two: duplicate multiple-bond endpoints, then complete the
/// ligand carbon's valence with H. Its bond back to the alpha carbon is omitted.
fn sphere_two(atom: &Atom, center: u64, doc: &Document) -> Option<Vec<usize>> {
    if atom.aromatic || atom.charge != 0 || atom.radical_electrons != 0 {
        return None;
    }
    let mut sphere = Vec::new();
    let mut valence = 0u32;
    for (other, order) in neighbors(doc, atom.id) {
        if !(1..=3).contains(&order) {
            return None;
        }
        valence += u32::from(order);
        let number = crate::chemistry::ELEMENTS
            .iter()
            .position(|element| element.symbol == other.element)?;
        if other.id != center {
            sphere.extend(std::iter::repeat_n(number, usize::from(order)));
        }
    }
    let available = 4u32.checked_sub(valence)?.checked_sub(atom.explicit_h)?;
    let hydrogens = atom.explicit_h + if atom.no_implicit { 0 } else { available };
    sphere.extend(std::iter::repeat_n(1, hydrogens as usize));
    if sphere.len() != 3 {
        return None;
    }
    sphere.sort_unstable_by(|a, b| b.cmp(a));
    Some(sphere)
}

fn amino_acid(atom: &Atom, doc: &Document) -> Option<&'static str> {
    let cip = atom.cip_label.as_deref()?;
    if !matches!(cip, "R" | "S") || carbon_hydrogens(atom, doc)? != 1 {
        return None;
    }
    let heavy: Vec<_> = neighbors(doc, atom.id)
        .into_iter()
        .filter(|(a, _)| a.element != "H")
        .collect();
    if heavy.len() != 3
        || heavy.iter().any(|(_, order)| *order != 1)
        || heavy.iter().filter(|(a, _)| a.element == "N").count() != 1
        || heavy.iter().filter(|(a, _)| a.element == "C").count() != 2
    {
        return None;
    }
    let carbons: Vec<_> = heavy
        .iter()
        .filter_map(|(a, _)| (a.element == "C").then_some(*a))
        .collect();
    let [first, second] = carbons.as_slice() else {
        return None;
    };
    let (carbonyl, sidechain) = match (
        carbonyl(first, atom.id, doc),
        carbonyl(second, atom.id, doc),
    ) {
        (true, false) => (*first, *second),
        (false, true) => (*second, *first),
        _ => return None,
    };
    let carbonyl_higher =
        match sphere_two(carbonyl, atom.id, doc)?.cmp(&sphere_two(sidechain, atom.id, doc)?) {
            Ordering::Greater => true,
            Ordering::Less => false,
            // This small recognizer deliberately stops at sphere two. A tie needs
            // deeper CIP rules; omitting D/L is safer than guessing from R/S.
            Ordering::Equal => return None,
        };
    Some(if (cip == "S") == carbonyl_higher {
        "L"
    } else {
        "D"
    })
}

/// Compare parity in a chemical neighbor order, including a drawn H last.
fn counterclockwise(atom: &Atom, ordered: &[u64], doc: &Document) -> Option<bool> {
    let mut reference = ordered.to_vec();
    let hydrogens: Vec<_> = neighbors(doc, atom.id)
        .into_iter()
        .filter_map(|(a, order)| (a.element == "H" && order == 1).then_some(a.id))
        .collect();
    if hydrogens.len() > 1 {
        return None;
    }
    reference.extend(hydrogens);
    let stored = atom.stereo.as_ref()?;
    if !matches!(stored.winding.as_str(), "cw" | "ccw")
        || stored.neighbors.len() != reference.len()
        || stored.neighbors.iter().any(|id| !reference.contains(id))
    {
        return None;
    }
    Some(crate::fischer::equivalent(
        stored,
        &AtomStereo {
            winding: "ccw".into(),
            neighbors: reference,
        },
    ))
}

fn haworth(doc: &Document, result: &mut HashMap<u64, &'static str>) {
    let recognized = crate::haworth::interchange::recognize(doc);
    let ring_neighbors = |id| -> Vec<u64> {
        recognized
            .bonds
            .iter()
            .filter_map(|&i| {
                let bond = doc.bonds.get(i)?;
                if bond.a == id {
                    Some(bond.b)
                } else if bond.b == id {
                    Some(bond.a)
                } else {
                    None
                }
            })
            .collect()
    };
    for oxygen in doc
        .atoms
        .iter()
        .filter(|a| a.element == "O" && recognized.atoms.contains(&a.id))
    {
        let mut ring = HashSet::new();
        let mut pending = vec![oxygen.id];
        while let Some(id) = pending.pop() {
            if ring.insert(id) {
                pending.extend(ring_neighbors(id));
            }
        }
        if !matches!(ring.len(), 5 | 6)
            || ring.iter().any(|id| {
                doc.atom(*id)
                    .is_none_or(|a| a.element != "C" && a.id != oxygen.id)
            })
        {
            continue;
        }
        let oxygen_neighbors = ring_neighbors(oxygen.id);
        let [first, second] = oxygen_neighbors.as_slice() else {
            continue;
        };
        let anomeric = |id| {
            neighbors(doc, id)
                .iter()
                .any(|(a, order)| !ring.contains(&a.id) && a.element == "O" && *order == 1)
        };
        let mut terminal = match (anomeric(*first), anomeric(*second)) {
            (true, false) => *second,
            (false, true) => *first,
            _ => continue,
        };
        let mut next = oxygen.id;
        let mut previous = match ring_neighbors(terminal).into_iter().find(|id| *id != next) {
            Some(id) => id,
            None => continue,
        };
        let Some(mut atom) = doc.atom(terminal) else {
            continue;
        };
        let mut branches: Vec<_> = neighbors(doc, terminal)
            .into_iter()
            .filter(|(a, _)| !ring.contains(&a.id) && a.element != "H")
            .collect();
        let stepped = branches.is_empty() && carbon_hydrogens(atom, doc) == Some(2);
        if stepped {
            // An aldopentopyranose closes through terminal CH2OH: its last
            // configurational carbon is the preceding carbon, bearing OH.
            next = terminal;
            terminal = previous;
            previous = match ring_neighbors(terminal).into_iter().find(|id| *id != next) {
                Some(id) => id,
                None => continue,
            };
            let Some(candidate) = doc.atom(terminal) else {
                continue;
            };
            atom = candidate;
            branches = neighbors(doc, terminal)
                .into_iter()
                .filter(|(a, _)| !ring.contains(&a.id) && a.element != "H")
                .collect();
        }
        let [(branch, 1)] = branches.as_slice() else {
            continue;
        };
        if carbon_hydrogens(atom, doc) != Some(1)
            || branch.element != if stepped { "O" } else { "C" }
        {
            continue;
        }
        if let Some(ccw) = counterclockwise(atom, &[previous, next, branch.id], doc) {
            result.insert(atom.id, if ccw != stepped { "D" } else { "L" });
        }
    }
}

/// The two chain carbons, OH oxygen and explicit H of an open-chain sugar
/// carbon, as drawn in a Fischer projection.
fn chain_carbon(doc: &Document, id: u64) -> Option<([u64; 2], u64, u64)> {
    let adjacent = neighbors(doc, id);
    if adjacent.iter().any(|(_, order)| *order != 1) {
        return None;
    }
    let of = |element: &str| -> Vec<u64> {
        adjacent
            .iter()
            .filter(|(a, _)| a.element == element)
            .map(|(a, _)| a.id)
            .collect()
    };
    match (of("C").as_slice(), of("O").as_slice(), of("H").as_slice()) {
        ([a, b], [o], [h]) if adjacent.len() == 4 => Some(([*a, *b], *o, *h)),
        _ => None,
    }
}

/// Open-chain aldoses (e.g. Fischer objects) whose chain runs from CHO through
/// CH(OH) carbons to CH2OH. The configurational atom is the stereocentre next
/// to CH2OH; its D/L comes from stored parity and connectivity, so the label
/// does not depend on how the projection is turned on the page.
fn aldose(doc: &Document, result: &mut HashMap<u64, &'static str>) {
    for atom in doc.atoms.iter().filter(|a| a.stereo.is_some()) {
        let Some(([first, second], oxygen, _)) = chain_carbon(doc, atom.id) else {
            continue;
        };
        for (lower, upper) in [(first, second), (second, first)] {
            let Some(terminal) = doc.atom(upper) else {
                continue;
            };
            if carbon_hydrogens(terminal, doc) != Some(2)
                || !neighbors(doc, upper)
                    .iter()
                    .any(|(a, order)| a.element == "O" && *order == 1)
            {
                continue;
            }
            let (mut previous, mut current) = (atom.id, lower);
            let mut visited = HashSet::from([atom.id]);
            while let Some(([a, b], _, _)) = chain_carbon(doc, current) {
                if !visited.insert(current) {
                    break;
                }
                let next = if a == previous { b } else { a };
                (previous, current) = (current, next);
            }
            let Some(aldehyde) = doc.atom(current) else {
                continue;
            };
            let other: Vec<_> = neighbors(doc, current)
                .into_iter()
                .filter(|(a, _)| a.id != previous && a.element != "H")
                .collect();
            if visited.contains(&current)
                || carbon_hydrogens(aldehyde, doc) != Some(1)
                || !matches!(other.as_slice(), [(a, 2)] if a.element == "O")
            {
                continue;
            }
            // D: with CHO above, the configurational OH lies on the right.
            if let Some(ccw) = counterclockwise(atom, &[lower, oxygen, upper], doc) {
                result.insert(atom.id, if ccw { "D" } else { "L" });
            }
        }
    }
}

#[cfg(test)]
mod tests;
