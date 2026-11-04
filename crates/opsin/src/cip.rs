//! OPSIN's constitutional and isotope CIP ordering (sequence rules 1–2).
//!
//! Port of `CipSequenceRules.java`, OPSIN 2.9.0, commit
//! `b91b610af5ab07560fedb20730d7aef46bb2bca0`.
//! Copyright Daniel Lowe and OPSIN contributors; MIT, see the crate license.
//! Adds P-92.1.4.4 averaging for singly negative odd mancude cycles, including
//! saturated fusion, when every conjugated ring anion in the graph is eligible.
//!
//! Each rule is exhausted over the whole ligand before the next rule is used.
//! Path-local histories terminate rings; terminal duplicate atoms represent
//! ring closures and multiple bonds. Duplicate atoms intentionally have no
//! isotope, as in upstream. No sequence rules 3–5 are inferred on a tie.
//!
//! Only actual graph bonds are traversed. Hydrogen counts and stereo-reference
//! placeholders are not virtual neighbours: callers must use the same explicit
//! hydrogen construction stage as OPSIN when hydrogen participation is needed.

use crate::graph::{Atom, AtomId, Graph};
use reshiki_chemistry::stereo::cip::Fraction;
use std::cell::OnceCell;
use std::cmp::Ordering;
use std::collections::{HashMap, VecDeque};
use std::fmt;

const UNRESOLVED_MESSAGE: &str = "Failed to assign CIP stereochemistry, this indicates a bug in OPSIN or a limitation in OPSIN's implementation of the sequence rules";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CipOrderingError {
    /// The ligands remain indistinguishable under OPSIN's sequence rules 1–2.
    UnresolvedTie,
    /// Invalid arena references or use of a non-neighbour as a ligand.
    InvalidGraph(String),
}

impl fmt::Display for CipOrderingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnresolvedTie => f.write_str(UNRESOLVED_MESSAGE),
            Self::InvalidGraph(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for CipOrderingError {}

/// Orders the physical neighbours of one atom from lowest to highest priority.
pub struct CipSequenceRules<'a> {
    graph: &'a Graph,
    chiral_atom: AtomId,
    negative_rings: OnceCell<HashMap<AtomId, NegativeMancudeRing>>,
}

struct NegativeMancudeRing {
    id: AtomId,
    fraction: Fraction,
}

impl<'a> CipSequenceRules<'a> {
    pub const fn new(graph: &'a Graph, chiral_atom: AtomId) -> Self {
        Self {
            graph,
            chiral_atom,
            negative_rings: OnceCell::new(),
        }
    }

    pub fn get_neighbouring_atoms_in_cip_order(&self) -> Result<Vec<AtomId>, CipOrderingError> {
        self.sort_neighbours(self.neighbours()?)
    }

    pub fn get_neighbouring_atoms_in_cip_order_ignoring_given_neighbour(
        &self,
        ignored: AtomId,
    ) -> Result<Vec<AtomId>, CipOrderingError> {
        let mut neighbours = self.neighbours()?;
        let index = neighbours
            .iter()
            .position(|&id| id == ignored)
            .ok_or_else(|| {
                CipOrderingError::InvalidGraph(format!(
                    "OPSIN bug: Atom{} was not a neighbour of the given stereogenic atom",
                    ignored.0
                ))
            })?;
        neighbours.remove(index);
        self.sort_neighbours(neighbours)
    }

    /// Compare two ligands of this atom. `Greater` means `a` has higher priority.
    /// A constitutional/isotope tie is an error, never an arbitrary ordering.
    pub fn compare_ligands(&self, a: AtomId, b: AtomId) -> Result<Ordering, CipOrderingError> {
        let neighbours = self.neighbours()?;
        if !neighbours.contains(&a) || !neighbours.contains(&b) {
            return Err(CipOrderingError::InvalidGraph(
                "CIP ligands must be neighbours of the given stereogenic atom".into(),
            ));
        }
        self.compare(a, b)
    }

    fn atom(&self, id: AtomId) -> Result<&Atom, CipOrderingError> {
        self.graph
            .atoms
            .get(id.0)
            .filter(|atom| atom.active)
            .ok_or_else(|| {
                CipOrderingError::InvalidGraph(format!("Missing or inactive CIP atom {}", id.0))
            })
    }

    fn neighbours(&self) -> Result<Vec<AtomId>, CipOrderingError> {
        let mut neighbours = Vec::new();
        for &bond_id in &self.atom(self.chiral_atom)?.bonds {
            let bond = self
                .graph
                .bonds
                .get(bond_id.0)
                .filter(|bond| bond.active)
                .ok_or_else(|| {
                    CipOrderingError::InvalidGraph(format!(
                        "Missing or inactive CIP bond {}",
                        bond_id.0
                    ))
                })?;
            let other = bond.other_atom(self.chiral_atom).ok_or_else(|| {
                CipOrderingError::InvalidGraph("CIP atom contains a nonincident bond".into())
            })?;
            self.atom(other)?;
            neighbours.push(other);
        }
        Ok(neighbours)
    }

    fn sort_neighbours(
        &self,
        mut neighbours: Vec<AtomId>,
    ) -> Result<Vec<AtomId>, CipOrderingError> {
        // A fallible stable insertion sort avoids swallowing a comparator error.
        // Atom degree is small; unlike the inner rule comparator, this comparator
        // intentionally never returns Equal for indistinguishable ligands.
        for index in 1..neighbours.len() {
            let mut position = index;
            while position > 0 {
                if self.compare(neighbours[position], neighbours[position - 1])? != Ordering::Less {
                    break;
                }
                neighbours.swap(position, position - 1);
                position -= 1;
            }
        }
        Ok(neighbours)
    }

    fn compare(&self, a: AtomId, b: AtomId) -> Result<Ordering, CipOrderingError> {
        for rule in 0..=2 {
            let a = self.real_node(a, vec![self.chiral_atom])?;
            let b = self.real_node(b, vec![self.chiral_atom])?;
            let order = compare_nodes(&a, &b, rule);
            if order != Ordering::Equal {
                return Ok(order);
            }
            let mut queue = VecDeque::from([(vec![a], vec![b])]);
            while let Some((a, b)) = queue.pop_front() {
                let a = self.next_level(a, rule)?;
                let b = self.next_level(b, rule)?;
                let order = compare_lists_of_lists(&a, &b, rule);
                if order != Ordering::Equal {
                    return Ok(order);
                }
                let a = group_same_priority(a, rule);
                let b = group_same_priority(b, rule);
                // Equal sorted neighbour lists have the same priority groups.
                if a.len() != b.len() {
                    return Err(CipOrderingError::InvalidGraph(
                        "Equal CIP neighbour lists produced unequal priority groups".into(),
                    ));
                }
                // Breadth first, with higher-priority groups enqueued first.
                queue.extend(a.into_iter().zip(b).rev());
            }
        }
        Err(CipOrderingError::UnresolvedTie)
    }

    fn negative_rings(&self) -> &HashMap<AtomId, NegativeMancudeRing> {
        self.negative_rings.get_or_init(|| {
            let mut rings = HashMap::new();
            for candidate in self
                .graph
                .atoms
                .iter()
                .filter(|a| a.active && a.charge == -1)
            {
                if candidate
                    .bonds
                    .iter()
                    .any(|&b| self.graph.bond(b).order > 1)
                {
                    continue;
                }
                let Some(mut current) = candidate
                    .bonds
                    .iter()
                    .filter(|&&b| crate::cycle_detector::is_bond_in_cycle(self.graph, b))
                    .filter_map(|&b| self.graph.bond(b).other_atom(candidate.id))
                    .find(|&a| {
                        self.graph
                            .atom(a)
                            .bonds
                            .iter()
                            .any(|&b| self.graph.bond(b).order == 2)
                    })
                else {
                    continue;
                };
                let mut ring = vec![candidate.id];
                let mut previous = candidate.id;
                // Follow alternating double/single bonds around the pi cycle.
                // Any ineligible candidate disables averaging throughout the graph.
                while current != candidate.id {
                    if ring.contains(&current) {
                        return HashMap::new();
                    }
                    ring.push(current);
                    let atom = self.graph.atom(current);
                    let mut multiple_bonds = atom
                        .bonds
                        .iter()
                        .map(|&b| self.graph.bond(b))
                        .filter(|b| b.order > 1);
                    let Some(double) = multiple_bonds.next() else {
                        return HashMap::new();
                    };
                    if atom.charge != 0
                        || double.order != 2
                        || multiple_bonds.next().is_some()
                        || !crate::cycle_detector::is_bond_in_cycle(self.graph, double.id)
                    {
                        return HashMap::new();
                    }
                    let Some(mut next) = double.other_atom(current) else {
                        return HashMap::new();
                    };
                    if next == previous {
                        let mut next_atoms = atom
                            .bonds
                            .iter()
                            .map(|&b| self.graph.bond(b))
                            .filter(|b| {
                                b.order == 1
                                    && crate::cycle_detector::is_bond_in_cycle(self.graph, b.id)
                            })
                            .filter_map(|b| b.other_atom(current))
                            .filter(|&a| {
                                a == candidate.id
                                    || self
                                        .graph
                                        .atom(a)
                                        .bonds
                                        .iter()
                                        .any(|&b| self.graph.bond(b).order == 2)
                            });
                        let Some(single_partner) = next_atoms.next() else {
                            return HashMap::new();
                        };
                        if next_atoms.next().is_some() {
                            return HashMap::new();
                        }
                        next = single_partner;
                    }
                    previous = current;
                    current = next;
                }
                // A stereocentre inside the ring suppresses duplicates of its own
                // bonds, so averaging would split the two ring directions.
                if ring.contains(&self.chiral_atom) {
                    return HashMap::new();
                }
                // Unsaturated fusion adds contributors beyond this odd cycle.
                for &atom in &ring {
                    for &b in &self.graph.atom(atom).bonds {
                        if !crate::cycle_detector::is_bond_in_cycle(self.graph, b) {
                            continue;
                        }
                        if self.graph.bond(b).other_atom(atom).is_some_and(|a| {
                            !ring.contains(&a)
                                && self
                                    .graph
                                    .atom(a)
                                    .bonds
                                    .iter()
                                    .any(|&b| self.graph.bond(b).order > 1)
                        }) {
                            return HashMap::new();
                        }
                    }
                }
                let n = ring.len();
                for (i, &atom) in ring.iter().enumerate() {
                    let sum: u32 = [ring[(i + n - 1) % n], ring[(i + 1) % n]]
                        .iter()
                        .map(|&a| u32::from(self.graph.atom(a).element.atomic_number()))
                        .sum();
                    // In the n maximal contributors this atom is charged once
                    // and double-bonded to each neighbour (n - 1) / 2 times.
                    let fraction = Fraction((n as u32 - 1) * sum, 2 * n as u32);
                    rings.insert(
                        atom,
                        NegativeMancudeRing {
                            id: candidate.id,
                            fraction,
                        },
                    );
                }
            }
            rings
        })
    }

    fn real_node(&self, id: AtomId, visited: Vec<AtomId>) -> Result<Node, CipOrderingError> {
        let atom = self.atom(id)?;
        Ok(Node {
            atom: Some(id),
            atomic_number: Fraction(u32::from(atom.element.atomic_number()), 1),
            isotope: atom.isotope,
            visited,
            duplicate_origin: None,
        })
    }

    fn next_level(&self, nodes: Vec<Node>, rule: u8) -> Result<Vec<Vec<Node>>, CipOrderingError> {
        let mut neighbours = nodes
            .into_iter()
            .map(|node| self.next_atoms(node, rule))
            .collect::<Result<Vec<_>, _>>()?;
        neighbours.sort_by(|a, b| compare_lists(a, b, rule));
        Ok(neighbours)
    }

    fn next_atoms(&self, node: Node, rule: u8) -> Result<Vec<Node>, CipOrderingError> {
        let Some(id) = node.atom else {
            // Ghost atoms are terminal: they do not acquire their original's bonds.
            return Ok(Vec::new());
        };
        let previous =
            node.visited.last().copied().ok_or_else(|| {
                CipOrderingError::InvalidGraph("CIP path has no predecessor".into())
            })?;
        let mut visited = node.visited.clone();
        visited.push(id);
        let mut neighbours = Vec::new();
        let atom = self.atom(id)?;
        let parts = self.negative_rings();
        let part = parts.get(&id);
        let negative = atom.charge < 0 && part.is_some();
        let mut added_bond_duplicate = false;
        for &bond_id in &atom.bonds {
            let bond = self
                .graph
                .bonds
                .get(bond_id.0)
                .filter(|bond| bond.active)
                .ok_or_else(|| {
                    CipOrderingError::InvalidGraph(format!(
                        "Missing or inactive CIP bond {}",
                        bond_id.0
                    ))
                })?;
            let other = bond.other_atom(id).ok_or_else(|| {
                CipOrderingError::InvalidGraph("CIP atom contains a nonincident bond".into())
            })?;
            let other_atom = self.atom(other)?;
            let other_number = Fraction(u32::from(other_atom.element.atomic_number()), 1);
            let origin = node.visited.iter().position(|&ancestor| ancestor == other);
            let averaged =
                part.filter(|part| parts.get(&other).is_some_and(|other| part.id == other.id));
            if other != self.chiral_atom {
                // Higher-order bonds to the root itself do not create duplicates
                // (P-91.1.4.2.4), including e.g. the oxygen of a sulfoxide.
                // A negative mancude atom needs one averaged duplicate even
                // without a double bond, including entry from outside its ring.
                let duplicates =
                    if negative && averaged.is_some() && other != previous && !added_bond_duplicate
                    {
                        1
                    } else {
                        bond.order.saturating_sub(1)
                    };
                for _ in 0..duplicates {
                    neighbours.push(Node::ghost(
                        averaged.map_or(other_number, |part| part.fraction),
                        visited.clone(),
                        // An averaged duplicate stands for every resonance position: it stays a
                        // duplicate for rule 1b, but never a ring closure to one drawn partner.
                        origin
                            .filter(|_| averaged.is_none())
                            .unwrap_or(node.visited.len() + 1),
                    ));
                    added_bond_duplicate = true;
                }
            }
            if other != previous {
                if let Some(origin) = origin {
                    neighbours.push(Node::ghost(other_number, visited.clone(), origin));
                } else {
                    neighbours.push(self.real_node(other, visited.clone())?);
                }
            }
        }
        neighbours.sort_by(|a, b| compare_nodes(a, b, rule));
        Ok(neighbours)
    }
}

#[derive(Clone)]
struct Node {
    /// None distinguishes a terminal ghost from its real source atom.
    atom: Option<AtomId>,
    atomic_number: Fraction,
    isotope: Option<u32>,
    visited: Vec<AtomId>,
    duplicate_origin: Option<usize>,
}

impl Node {
    fn ghost(atomic_number: Fraction, visited: Vec<AtomId>, origin: usize) -> Self {
        Self {
            atom: None,
            atomic_number,
            isotope: None,
            visited,
            duplicate_origin: Some(origin),
        }
    }
}

fn compare_nodes(a: &Node, b: &Node, rule: u8) -> Ordering {
    let Fraction(an, ad) = a.atomic_number;
    let Fraction(bn, bd) = b.atomic_number;
    let order = (u64::from(an) * u64::from(bd)).cmp(&(u64::from(bn) * u64::from(ad)));
    if order != Ordering::Equal || rule == 0 {
        return order;
    }
    let order = match (a.duplicate_origin, b.duplicate_origin) {
        (Some(a), Some(b)) => b.cmp(&a), // The duplicate closer to the root wins.
        (Some(_), None) => Ordering::Greater,
        (None, Some(_)) => Ordering::Less,
        (None, None) => Ordering::Equal,
    };
    if order != Ordering::Equal || rule == 1 {
        return order;
    }
    // Upstream ranks any specified isotope above unspecified isotope, even if
    // its mass is below the element's most abundant naturally occurring isotope.
    a.isotope.cmp(&b.isotope)
}

fn compare_lists(a: &[Node], b: &[Node], rule: u8) -> Ordering {
    a.iter()
        .rev()
        .zip(b.iter().rev())
        .map(|(a, b)| compare_nodes(a, b, rule))
        .find(|order| *order != Ordering::Equal)
        .unwrap_or_else(|| a.len().cmp(&b.len()))
}

fn compare_lists_of_lists(a: &[Vec<Node>], b: &[Vec<Node>], rule: u8) -> Ordering {
    a.iter()
        .rev()
        .zip(b.iter().rev())
        .map(|(a, b)| compare_lists(a, b, rule))
        .find(|order| *order != Ordering::Equal)
        .unwrap_or_else(|| a.len().cmp(&b.len()))
}

/// Combine equal sibling lists before splitting their union by atom priority.
/// Distinct sibling-list priorities remain separate even when individual atom
/// priorities coincide; flattening all neighbours here changes CIP results.
fn group_same_priority(lists: Vec<Vec<Node>>, rule: u8) -> Vec<Vec<Node>> {
    let mut input = lists.into_iter().peekable();
    let mut output = Vec::new();
    while let Some(mut combined) = input.next() {
        let mut equivalents = Vec::new();
        while input
            .peek()
            .is_some_and(|next| compare_lists(&combined, next, rule) == Ordering::Equal)
        {
            if let Some(next) = input.next() {
                equivalents.push(next);
            }
        }
        combined.extend(equivalents.into_iter().flatten());
        combined.sort_by(|a, b| compare_nodes(a, b, rule));
        let mut group: Vec<Node> = Vec::new();
        for node in combined {
            if group
                .last()
                .is_some_and(|last| compare_nodes(last, &node, rule) != Ordering::Equal)
            {
                output.push(std::mem::take(&mut group));
            }
            group.push(node);
        }
        if !group.is_empty() {
            output.push(group);
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fractional_atomic_numbers_compare_exactly() {
        let node = |n, d| Node::ghost(Fraction(n, d), Vec::new(), 0);
        assert_eq!(compare_nodes(&node(24, 5), &node(6, 1), 0), Ordering::Less);
        assert_eq!(
            compare_nodes(&node(24, 5), &node(48, 10), 0),
            Ordering::Equal
        );
    }

    #[test]
    fn each_cyclopentadienide_atom_has_one_averaged_bond_duplicate() {
        for smiles in [
            "O[C@H](C1=CC=CC1)C1=C[CH-]C=C1",
            "O[C@H](C1=CC=CC1)[C-]1C=CC=C1",
        ] {
            assert_ring_duplicates(smiles, 7..12, &[Fraction(24, 5); 5]);
        }
    }

    #[test]
    fn saturated_fusion_preserves_one_averaged_duplicate_per_pi_cycle_atom() {
        let smiles = "O[C@H]([C-]1C=CC=C1)[C-]1C=CC2=C1CCC2";
        assert_ring_duplicates(smiles, 2..7, &[Fraction(24, 5); 5]);
        assert_ring_duplicates(smiles, 7..12, &[Fraction(24, 5); 5]);

        let mut graph = Graph::default();
        crate::smiles::build_fragment(&mut graph, smiles, "", "").unwrap();
        let rules = CipSequenceRules::new(&graph, AtomId(1));
        let rings = rules.negative_rings();
        assert_eq!(rings.len(), 10);
        for id in 2..12 {
            assert_eq!(rings[&AtomId(id)].id, AtomId(if id < 7 { 2 } else { 7 }));
        }
    }

    #[test]
    fn ineligible_conjugated_ring_anion_disables_all_averaging() {
        for excluded in [
            "[C-]1C=CC2=CC=CC=C21", // Unsaturated fusion.
            "[C-]1C=C[NH+]=C1",     // Another charge in the cycle.
            "[C-]1C=CC1",           // Alternation cannot close the cycle.
            "[C-]1C=CC(=O)C=C1",    // The double bond leaves the cycle.
        ] {
            for smiles in [
                format!("OC([C-]1C=CC=C1){excluded}"),
                format!("OC({excluded})[C-]1C=CC=C1"),
            ] {
                let mut graph = Graph::default();
                crate::smiles::build_fragment(&mut graph, &smiles, "", "").unwrap();
                let rules = CipSequenceRules::new(&graph, AtomId(1));
                assert!(rules.negative_rings().is_empty(), "{smiles}");
            }
        }
    }

    #[test]
    fn nonconjugated_anions_do_not_disable_ring_averaging() {
        for spectator in ["[Cl-]", "Cl(=O)(=O)(=O)[O-]", "[CH-]1CCCC1"] {
            let smiles = format!("OC([C-]1C=CC=C1).{spectator}");
            assert_ring_duplicates(&smiles, 2..7, &[Fraction(24, 5); 5]);
        }
    }

    #[test]
    fn each_cyclopropenide_atom_has_one_integral_averaged_bond_duplicate() {
        for smiles in ["O[C@H](C1=CC1)C1=C[CH-]1", "O[C@H](C1=CC1)[C-]1C=C1"] {
            assert_ring_duplicates(smiles, 5..8, &[Fraction(4, 1); 3]);
        }
    }

    #[test]
    fn pyrrolide_duplicates_average_each_atoms_ring_neighbours() {
        for smiles in ["OC(C1=CC=C[N-]1)", "OC([C-]1C=CC=N1)"] {
            assert_ring_duplicates(
                smiles,
                2..7,
                &[
                    Fraction(26, 5),
                    Fraction(24, 5),
                    Fraction(24, 5),
                    Fraction(26, 5),
                    Fraction(24, 5),
                ],
            );
        }
    }

    fn assert_ring_duplicates(smiles: &str, ring: std::ops::Range<usize>, fractions: &[Fraction]) {
        let mut graph = Graph::default();
        let fragment = crate::smiles::build_fragment(&mut graph, smiles, "", "").unwrap();
        graph.make_hydrogens_explicit(fragment).unwrap();
        let rules = CipSequenceRules::new(&graph, AtomId(1));
        let mut pending = vec![
            rules
                .real_node(AtomId(ring.start), vec![AtomId(1)])
                .unwrap(),
        ];
        while let Some(node) = pending.pop() {
            let id = node.atom.unwrap();
            let depth = node.visited.len() + 1;
            let neighbours = rules.next_atoms(node, 0).unwrap();
            if ring.contains(&id.0) {
                let expected = Node::ghost(fractions[id.0 - ring.start], Vec::new(), depth);
                let duplicates: Vec<_> = neighbours
                    .iter()
                    .filter(|n| {
                        n.atom.is_none() && compare_nodes(n, &expected, 0) == Ordering::Equal
                    })
                    .collect();
                assert_eq!(duplicates.len(), 1, "{smiles}: atom {}", id.0);
                assert_eq!(duplicates[0].duplicate_origin, Some(depth));
            }
            pending.extend(neighbours.into_iter().filter(|n| n.atom.is_some()));
        }
    }

    #[test]
    fn negative_part_keeps_exocyclic_oxygen_duplicate_and_origin() {
        // Neither ring has n maximal contributors: its carbonyl carbons keep
        // their double bond outside the ring.
        for smiles in ["OC1=CC(=O)C=C[CH-]1", "OC1=CC(=O)C(=O)C=C[CH-]1"] {
            let mut graph = Graph::default();
            let fragment = crate::smiles::build_fragment(&mut graph, smiles, "", "").unwrap();
            graph.make_hydrogens_explicit(fragment).unwrap();
            let rules = CipSequenceRules::new(&graph, AtomId(0));
            assert!(rules.negative_rings().is_empty(), "{smiles}");
            let carbonyl = rules
                .real_node(AtomId(3), vec![AtomId(0), AtomId(1), AtomId(2)])
                .unwrap();
            let neighbours = rules.next_atoms(carbonyl, 0).unwrap();
            let duplicates: Vec<_> = neighbours.iter().filter(|n| n.atom.is_none()).collect();
            assert_eq!(duplicates.len(), 1);
            assert_eq!(duplicates[0].atomic_number, Fraction(8, 1));
            assert_eq!(duplicates[0].duplicate_origin, Some(4));
        }
    }

    #[test]
    fn fused_and_multiply_charged_negative_parts_keep_drawn_duplicates() {
        for smiles in [
            "C[C@H](O)[C-]1C=CC2=CC=CC=C21",
            "C[C@H](O)[C-]1C=C[CH-]C=C[CH-]1",
        ] {
            let mut graph = Graph::default();
            let fragment = crate::smiles::build_fragment(&mut graph, smiles, "", "").unwrap();
            graph.make_hydrogens_explicit(fragment).unwrap();
            let rules = CipSequenceRules::new(&graph, AtomId(1));
            assert!(rules.negative_rings().is_empty(), "{smiles}");
            let charged = rules.real_node(AtomId(3), vec![AtomId(1)]).unwrap();
            assert!(
                rules
                    .next_atoms(charged, 0)
                    .unwrap()
                    .iter()
                    .all(|n| n.atom.is_some())
            );
        }
    }
}
