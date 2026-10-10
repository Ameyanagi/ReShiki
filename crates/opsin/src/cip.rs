//! OPSIN's constitutional and isotope CIP ordering (sequence rules 1–2).
//!
//! Port of `CipSequenceRules.java`, OPSIN 2.9.0, commit
//! `b91b610af5ab07560fedb20730d7aef46bb2bca0`.
//! Copyright Daniel Lowe and OPSIN contributors; MIT, see the crate license.
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
use std::cmp::Ordering;
use std::collections::VecDeque;
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
}

impl<'a> CipSequenceRules<'a> {
    pub const fn new(graph: &'a Graph, chiral_atom: AtomId) -> Self {
        Self { graph, chiral_atom }
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

    fn real_node(&self, id: AtomId, visited: Vec<AtomId>) -> Result<Node, CipOrderingError> {
        let atom = self.atom(id)?;
        Ok(Node {
            atom: Some(id),
            element: atom.element,
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
        for &bond_id in &self.atom(id)?.bonds {
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
            let origin = node.visited.iter().position(|&ancestor| ancestor == other);
            if other != self.chiral_atom {
                // Higher-order bonds to the root itself do not create duplicates
                // (P-91.1.4.2.4), including e.g. the oxygen of a sulfoxide.
                for _ in 1..bond.order {
                    neighbours.push(Node::ghost(
                        other_atom.element,
                        visited.clone(),
                        origin.unwrap_or(node.visited.len() + 1),
                    ));
                }
            }
            if other != previous {
                if let Some(origin) = origin {
                    neighbours.push(Node::ghost(other_atom.element, visited.clone(), origin));
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
    element: Element,
    isotope: Option<u32>,
    visited: Vec<AtomId>,
    duplicate_origin: Option<usize>,
}

impl Node {
    fn ghost(element: Element, visited: Vec<AtomId>, origin: usize) -> Self {
        Self {
            atom: None,
            element,
            isotope: None,
            visited,
            duplicate_origin: Some(origin),
        }
    }
}

fn compare_nodes(a: &Node, b: &Node, rule: u8) -> Ordering {
    let order = a.element.atomic_number().cmp(&b.element.atomic_number());
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
