// SPDX-License-Identifier: MIT OR Apache-2.0
// Origin: original
// Source: ReShiki | crates/chemistry/src/stereo/allene.rs | be63a799aa03a83dc6ec74bb294f4d42b89298d3

//! Three-atom allene axes. Carrier order follows graph bond insertion order,
//! with an implicit or bracket hydrogen last at each terminal carbon.
// Copied unchanged from crates/chemistry/src/stereo/allene.rs at be63a799aa03a83dc6ec74bb294f4d42b89298d3
use crate::graph::Graph;

pub fn terminals(graph: &Graph, center: usize) -> Option<[usize; 2]> {
    let carbon = |id: usize| {
        graph.atoms.get(id).is_some_and(|a| {
            a.atomic_number == 6 && !a.aromatic && a.charge == 0 && a.radical_electrons == 0
        })
    };
    if !carbon(center) || graph.atoms.get(center)?.explicit_hydrogens != 0 {
        return None;
    }
    let mut ends = graph
        .bonds
        .iter()
        .filter(|b| b.a == center || b.b == center);
    let mut result = [0; 2];
    for end in &mut result {
        let bond = ends.next()?;
        if bond.order != 2 || bond.aromatic {
            return None;
        }
        *end = if bond.a == center { bond.b } else { bond.a };
        if !carbon(*end) {
            return None;
        }
        let atom = graph.atoms.get(*end)?;
        let mut count = 0;
        for b in graph.bonds.iter().filter(|b| b.a == *end || b.b == *end) {
            let other = if b.a == *end { b.b } else { b.a };
            if other != center {
                if b.order != 1 || b.aromatic {
                    return None;
                }
                count += 1;
            }
        }
        if !(1..=2).contains(&count)
            || usize::from(atom.explicit_hydrogens) > 2 - count
            || atom.no_implicit && count + usize::from(atom.explicit_hydrogens) != 2
        {
            return None;
        }
    }
    ends.next().is_none().then_some(result)
}

pub fn carriers(graph: &Graph, center: usize) -> Option<[Option<usize>; 4]> {
    let ends = terminals(graph, center)?;
    let mut result = [None; 4];
    for (end, pair) in ends.into_iter().zip(result.as_chunks_mut::<2>().0) {
        for (slot, bond) in pair.iter_mut().zip(
            graph
                .bonds
                .iter()
                .filter(|b| (b.a == end || b.b == end) && b.a != center && b.b != center),
        ) {
            *slot = Some(if bond.a == end { bond.b } else { bond.a });
        }
    }
    Some(result)
}

/// The drawing model supports tetrahedral tags and specified allene winding.
pub fn supported(graph: &Graph, center: usize, tag: u8, permutation: Option<u32>) -> bool {
    tag <= 2 || tag == 5 && matches!(permutation, Some(1 | 2)) && terminals(graph, center).is_some()
}
