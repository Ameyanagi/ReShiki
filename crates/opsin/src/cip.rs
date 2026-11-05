//! OPSIN's constitutional and isotope CIP ordering (sequence rules 1–2).
//!
//! Port of `CipSequenceRules.java`, OPSIN 2.9.0, commit
//! `b91b610af5ab07560fedb20730d7aef46bb2bca0`.
//! Copyright Daniel Lowe and OPSIN contributors; MIT, see the crate license.
//! Adds P-92.1.4.4 averaging for isolated odd all-carbon monoanion rings
//! outside the stereocentre, only if every charge in its component is eligible.
//!
//! Each rule is exhausted over the whole ligand before the next rule is used.
//! Path-local histories terminate rings; terminal duplicate atoms represent
//! ring closures and multiple bonds. Duplicate atoms intentionally have no
//! isotope, as in upstream. No sequence rules 3–5 are inferred on a tie.
//!
//! Only actual graph bonds are traversed. Hydrogen counts and stereo-reference
//! placeholders are not virtual neighbours: callers must use the same explicit
//! hydrogen construction stage as OPSIN when hydrogen participation is needed.

use crate::graph::{Atom, AtomId, Element, Graph};
use reshiki_chemistry::stereo::cip::Fraction;
use std::cell::OnceCell;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet, VecDeque};
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
    charged_rings: OnceCell<HashMap<AtomId, Fraction>>,
}

impl<'a> CipSequenceRules<'a> {
    pub const fn new(graph: &'a Graph, chiral_atom: AtomId) -> Self {
        Self {
            graph,
            chiral_atom,
            charged_rings: OnceCell::new(),
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

    fn charged_rings(&self) -> &HashMap<AtomId, Fraction> {
        self.charged_rings.get_or_init(|| {
            let mut rings = HashMap::new();
            let mut seen = HashSet::new();
            let mut pending = vec![self.chiral_atom];
            while let Some(id) = pending.pop() {
                if !seen.insert(id) {
                    continue;
                }
                pending.extend(self.graph.neighbours(id));
                let candidate = self.graph.atom(id);
                if candidate.charge == 0 {
                    continue;
                }
                if candidate.element != Element::C
                    || candidate.charge != -1
                    || candidate
                        .bonds
                        .iter()
                        .any(|&b| self.graph.bond(b).order > 1)
                {
                    return HashMap::new();
                }
                let mut ring = Vec::new();
                let mut previous = None;
                let mut current = id;
                loop {
                    if ring.contains(&current) {
                        return HashMap::new();
                    }
                    ring.push(current);
                    let atom = self.graph.atom(current);
                    let cyclic: Vec<_> = atom
                        .bonds
                        .iter()
                        .copied()
                        .filter(|&b| crate::cycle_detector::is_bond_in_cycle(self.graph, b))
                        .collect();
                    if atom.element != Element::C
                        || current == self.chiral_atom
                        || cyclic.len() != 2
                    {
                        return HashMap::new();
                    }
                    if current != id {
                        let mut multiple = atom
                            .bonds
                            .iter()
                            .map(|&b| self.graph.bond(b))
                            .filter(|b| b.order > 1);
                        let Some(double) = multiple.next() else {
                            return HashMap::new();
                        };
                        if atom.charge != 0
                            || double.order != 2
                            || !cyclic.contains(&double.id)
                            || multiple.next().is_some()
                        {
                            return HashMap::new();
                        }
                    }
                    let next = cyclic
                        .iter()
                        .filter_map(|&b| self.graph.bond(b).other_atom(current))
                        .find(|&a| Some(a) != previous)
                        .unwrap();
                    if next == id {
                        break;
                    }
                    previous = Some(current);
                    current = next;
                }
                let n = ring.len() as u32;
                if n.is_multiple_of(2) {
                    return HashMap::new();
                }
                for atom in ring {
                    rings.insert(atom, Fraction(6 * (n - 1), n));
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
        let parts = self.charged_rings();
        let part = parts.get(&id);
        let charged = atom.charge != 0 && part.is_some();
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
            // Eligible rings are isolated; a cyclic bond cannot join two of them.
            let averaged = part.filter(|_| {
                parts.contains_key(&other)
                    && crate::cycle_detector::is_bond_in_cycle(self.graph, bond_id)
            });
            if other != self.chiral_atom {
                // Higher-order bonds to the root itself do not create duplicates
                // (P-91.1.4.2.4), including e.g. the oxygen of a sulfoxide.
                // The anion needs one averaged duplicate even
                // without a double bond, including entry from outside its ring.
                let duplicates = if charged
                    && averaged.is_some()
                    && other != previous
                    && !added_bond_duplicate
                {
                    1
                } else {
                    bond.order.saturating_sub(1)
                };
                for _ in 0..duplicates {
                    neighbours.push(Node::ghost(
                        averaged.copied().unwrap_or(other_number),
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
    // Protium and unlabelled H have equal priority (P-92.3). Otherwise upstream
    // ranks specified isotopes above unspecified ones, even at a lower mass.
    let a_isotope = a.isotope.filter(|&mass| an != ad || mass != 1);
    let b_isotope = b.isotope.filter(|&mass| bn != bd || mass != 1);
    a_isotope.cmp(&b_isotope)
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
            "OC([C-]1C=CC=C1)[C-]1C=CC=C1",
        ] {
            assert_ring_duplicates(smiles, 7..12, &[Fraction(24, 5); 5]);
        }
        // Directly connected eligible rings still get one duplicate per atom.
        let smiles = "OC(C)C1=CC=C[C-]1[C-]1C=CC=C1";
        assert_ring_duplicates(smiles, 3..8, &[Fraction(24, 5); 5]);
        assert_ring_duplicates(smiles, 8..13, &[Fraction(24, 5); 5]);
    }

    #[test]
    fn any_ineligible_charge_in_the_component_disables_all_averaging() {
        for excluded in [
            "[C-]1C=CC2=CC=CC=C21",     // Unsaturated fusion.
            "[C-]1C=CC2=C1CCC2",        // Saturated fusion.
            "[C-]1C=CC=C1[NH3+]",       // A second ion outside the ring.
            "[C-]1C=CC=C1[N+](=O)[O-]", // Nitro substituent.
            "[N-]1C=CC=C1",             // Hetero anion.
            "[C-]1C=CC=N1",             // Hetero ring with a carbon anion.
            "C1=C[N+](C)(C)C=C1",       // Ammonium diene (W21).
            "C1=C[NH2+]C=C1",           // Ammonium diene (W27).
            "[C+]1C=CC=CC=C1",          // Tropylium.
            "[C+]1C=C1",                // Cyclopropenylium.
            "[O-]",                     // Acyclic ions in this component.
            "[NH3+]",
            "[CH2-]",
            "[CH-]1CCCC1",
            "[C-]1C=C[NH+]=C1",  // Another charge in the cycle.
            "[C-]1C=CC1",        // Even, incompletely unsaturated ring.
            "[C-]1C=CC(=O)C=C1", // The double bond leaves the cycle.
            "C1=CC=CC=[O+]1",    // The charged atom carries a double bond.
            "C1=CC=CC=[NH+]1",
            "[C-2]1C=CC=C1", // Only singly charged cycles are eligible.
            "[C+2]1C=CC=C1",
            "[C+]1C=CC2=CC=CC=C21", // Unsaturated cationic fusion.
        ] {
            for smiles in [
                format!("OC([C-]1C=CC=C1){excluded}"),
                format!("OC({excluded})[C-]1C=CC=C1"),
            ] {
                let mut graph = Graph::default();
                crate::smiles::build_fragment(&mut graph, &smiles, "", "").unwrap();
                let rules = CipSequenceRules::new(&graph, AtomId(1));
                assert!(rules.charged_rings().is_empty(), "{smiles}");
            }
        }
    }

    #[test]
    fn charges_in_other_components_do_not_disable_ring_averaging() {
        for spectator in [
            "[Na+]",
            "[C+]1C=CC=CC=C1",
            "[N-]1C=CC=C1",
            "[Cl-]",
            "Cl(=O)(=O)(=O)[O-]",
            "[CH-]1CCCC1",
            "[NH4+]",
            "[CH+]1CCCC1",
        ] {
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
    fn charged_part_keeps_exocyclic_oxygen_duplicate_and_origin() {
        // Neither ring has n maximal contributors: its carbonyl carbons keep
        // their double bond outside the ring.
        for smiles in ["OC1=CC(=O)C=C[CH-]1", "OC1=CC(=O)C(=O)C=C[CH-]1"] {
            let mut graph = Graph::default();
            let fragment = crate::smiles::build_fragment(&mut graph, smiles, "", "").unwrap();
            graph.make_hydrogens_explicit(fragment).unwrap();
            let rules = CipSequenceRules::new(&graph, AtomId(0));
            assert!(rules.charged_rings().is_empty(), "{smiles}");
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
    fn eligible_ring_keeps_external_carbonyl_duplicate_and_origin() {
        let mut graph = Graph::default();
        crate::smiles::build_fragment(&mut graph, "OC([C-]1C=CC=C1)C=O", "", "").unwrap();
        let rules = CipSequenceRules::new(&graph, AtomId(1));
        assert_eq!(rules.charged_rings().len(), 5);
        for visited in [vec![AtomId(1)], vec![AtomId(1), AtomId(8)]] {
            let carbonyl = rules.real_node(AtomId(7), visited.clone()).unwrap();
            let neighbours = rules.next_atoms(carbonyl, 0).unwrap();
            let duplicates: Vec<_> = neighbours
                .iter()
                .filter(|n| n.atom.is_none() && n.atomic_number == Fraction(8, 1))
                .collect();
            assert_eq!(duplicates.len(), 1);
            let duplicate = duplicates[0];
            assert_eq!(
                duplicate.duplicate_origin,
                Some(if visited.len() == 1 { 2 } else { 1 })
            );
        }
    }

    #[test]
    fn fused_and_multiply_charged_parts_keep_drawn_duplicates() {
        for smiles in [
            "C[C@H](O)[C-]1C=CC2=CC=CC=C21",
            "C[C@H](O)[C-]1C=C[CH-]C=C[CH-]1",
        ] {
            let mut graph = Graph::default();
            let fragment = crate::smiles::build_fragment(&mut graph, smiles, "", "").unwrap();
            graph.make_hydrogens_explicit(fragment).unwrap();
            let rules = CipSequenceRules::new(&graph, AtomId(1));
            assert!(rules.charged_rings().is_empty(), "{smiles}");
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
