//! CIP inputs where ReShiki deliberately departs from the pinned RDKit (#1037).
//! The comparison still checks every other input exactly, and an affected
//! input must still succeed or fail as RDKit does; these rules have their own
//! tests in `crates/chemistry`.
use reshiki::chemistry::{graph::Graph, ranking::Metadata, stereo::allene};

/// #992: ¹H ranks as H for CIP (P-92.3); RDKit ranks it by isotope mass.
pub fn protium(graph: &Graph) -> bool {
    graph
        .atoms
        .iter()
        .any(|a| a.atomic_number == 1 && a.isotope == 1)
}

/// The intended change that makes RDKit's answer for this input obsolete.
pub fn difference(graph: &Graph, metadata: &Metadata) -> Option<&'static str> {
    if protium(graph) {
        return Some("#992 protium");
    }
    // #901: a negative mancude ring averages its duplicate atomic numbers over
    // the whole ring (P-92.1.4.4); RDKit uses an atom-order running fraction.
    // As in stereo/cip/mancude.rs, the anion has only single (or aromatic)
    // ring bonds, and it is conjugated: two of its neighbours carry a double
    // or aromatic bond. A saturated ring anion such as [CH-]1CCCC1 has
    // nothing to average and is compared exactly.
    let unsaturated = |atom: usize| {
        graph
            .bonds
            .iter()
            .any(|b| (b.a == atom || b.b == atom) && (b.aromatic || matches!(b.order, 2 | 4)))
    };
    if graph.atoms.iter().enumerate().any(|(i, a)| {
        let bonds = || graph.bonds.iter().filter(move |b| b.a == i || b.b == i);
        a.charge == -1
            && matches!(a.atomic_number, 6 | 7 | 14 | 15 | 32 | 33)
            && bonds().all(|b| !matches!(b.order, 2 | 3))
            && bonds()
                .filter(|b| unsaturated(if b.a == i { b.b } else { b.a }))
                .count()
                >= 2
            && in_ring(graph, i)
    }) {
        return Some("#901 negative mancude ring");
    }
    // #892: extended-tetrahedral (allene) configurations; RDKit drops them.
    if metadata
        .atoms
        .iter()
        .enumerate()
        .any(|(i, a)| a.chiral_tag == 5 && allene::supported(graph, i, 5, a.chiral_permutation))
    {
        return Some("#892 allene");
    }
    None
}

/// Whether some bond of the atom closes a cycle.
fn in_ring(graph: &Graph, atom: usize) -> bool {
    graph
        .bonds
        .iter()
        .enumerate()
        .filter(|(_, b)| b.a == atom || b.b == atom)
        .any(|(skip, bond)| {
            let target = if bond.a == atom { bond.b } else { bond.a };
            let mut seen = vec![false; graph.atoms.len()];
            let mut pending = vec![atom];
            seen[atom] = true;
            while let Some(current) = pending.pop() {
                for (i, b) in graph.bonds.iter().enumerate() {
                    let next = if b.a == current {
                        b.b
                    } else if b.b == current {
                        b.a
                    } else {
                        continue;
                    };
                    if i != skip && !seen[next] {
                        if next == target {
                            return true;
                        }
                        seen[next] = true;
                        pending.push(next);
                    }
                }
            }
            false
        })
}
