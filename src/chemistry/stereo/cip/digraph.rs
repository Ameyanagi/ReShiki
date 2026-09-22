//! CIP expansion adapted from RDKit Digraph.cpp, Node.cpp and Edge.cpp.
//! Copyright (C) 2020 Schrödinger, LLC. BSD-3-Clause; see licenses/rdkit/.
#[cfg(test)]
mod tests;
mod visits;
use super::{Error, Fraction, Molecule, at, at_mut, invalid};
use crate::chemistry::{ELEMENTS, ISOTOPES, stereo::perception::State};
use std::{num::NonZeroU32, os::raw::c_char};
use visits::Visits;

pub const EXPANDED: u8 = 1;
pub const RING_DUPLICATE: u8 = 2;
pub const BOND_DUPLICATE: u8 = 4;
pub const IMPLICIT_HYDROGEN: u8 = 8;
const DUPLICATE: u8 = RING_DUPLICATE | BOND_DUPLICATE;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum Descriptor {
    #[default]
    None,
    Unknown,
    Other,
    R,
    S,
    PseudoR,
    PseudoS,
    SeqTrans,
    SeqCis,
    E,
    Z,
    M,
    P,
    PseudoM,
    PseudoP,
    SquarePlanar,
    TrigonalBipyramidal,
    Octahedral,
}

pub struct Node {
    pub atom: Option<usize>,
    pub distance: i32,
    pub fraction: Fraction,
    pub number: u8,
    pub isotope: u16,
    pub mass: f64,
    pub flags: u8,
    pub aux: Descriptor,
    edges: Vec<usize>,
    visit: Option<NonZeroU32>,
}
impl Node {
    pub fn is_duplicate(&self) -> bool {
        self.flags & DUPLICATE != 0
    }
    pub fn is_duplicate_or_h(&self) -> bool {
        self.flags & (DUPLICATE | IMPLICIT_HYDROGEN) != 0
    }
    pub fn is_expanded(&self) -> bool {
        self.flags & EXPANDED != 0
    }
    pub fn is_terminal(&self) -> bool {
        self.visit.is_none() || self.is_expanded() && self.edges.len() == 1
    }
    /// Inspect only stored edges, without forcing lazy expansion.
    pub fn stored_edges(&self) -> &[usize] {
        &self.edges
    }
}
pub struct Edge {
    pub begin: usize,
    pub end: usize,
    pub bond: Option<usize>,
    pub aux: Descriptor,
}
impl Edge {
    pub fn other(&self, node: usize) -> Result<usize, Error> {
        if self.begin == node {
            Ok(self.end)
        } else if self.end == node {
            Ok(self.begin)
        } else {
            Err(invalid("Node is not an edge endpoint"))
        }
    }
}
struct Work(usize);
impl Work {
    fn spend(&mut self, n: usize) -> Result<(), Error> {
        self.0 = self.0.checked_sub(n).ok_or(Error::Limit)?;
        Ok(())
    }
}

/// Mutable expansion rooted in an immutable molecular state. Several graphs
/// can share one molecular cache; every operation verifies its source identity.
pub struct Digraph<'a> {
    state: &'a State,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    visits: Visits,
    current: usize,
    atrop: bool,
    seen: Vec<bool>,
    rule6: Option<usize>,
    work: Work,
    failed: bool,
}
impl<'a> Digraph<'a> {
    pub fn new(mol: &Molecule<'a>, atom: usize, atrop: bool) -> Result<Self, Error> {
        at(&mol.state.graph.atoms, atom)?;
        let mut graph = Self {
            state: mol.state,
            nodes: Vec::new(),
            edges: Vec::new(),
            visits: Visits::new(mol.state.graph.atoms.len()),
            current: 0,
            atrop,
            seen: vec![false; mol.state.graph.atoms.len()],
            rule6: None,
            work: Work(50_000_000),
            failed: false,
        };
        let visit = graph.visits.set(None, atom, 1, &mut graph.work)?;
        graph.add_node(Some(atom), visit, 1, 0, None)?;
        Ok(graph)
    }
    pub(super) fn check(&self, mol: &Molecule<'_>) -> Result<(), Error> {
        if self.failed {
            return Err(invalid("CIP graph is unavailable after expansion failure"));
        }
        if !std::ptr::eq(self.state, mol.state) {
            return Err(invalid("CIP graph belongs to another molecular state"));
        }
        Ok(())
    }
    pub fn original_root(&self) -> usize {
        0
    }
    pub fn current_root(&self) -> usize {
        self.current
    }
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }
    pub fn node(&self, id: usize) -> Result<&Node, Error> {
        at(&self.nodes, id)
    }
    pub fn edge(&self, id: usize) -> Result<&Edge, Error> {
        at(&self.edges, id)
    }
    pub fn seen_atom(&self, atom: usize) -> Result<bool, Error> {
        at(&self.seen, atom).copied()
    }
    pub fn rule6_reference(&self) -> Option<usize> {
        self.rule6
    }
    pub fn set_rule6_reference(&mut self, atom: Option<usize>) -> Result<(), Error> {
        if let Some(a) = atom {
            at(&self.state.graph.atoms, a)?;
        }
        self.rule6 = atom;
        Ok(())
    }
    pub fn set_node_aux(&mut self, node: usize, aux: Descriptor) -> Result<(), Error> {
        at_mut(&mut self.nodes, node)?.aux = aux;
        Ok(())
    }
    pub fn set_edge_aux(&mut self, edge: usize, aux: Descriptor) -> Result<(), Error> {
        at_mut(&mut self.edges, edge)?.aux = aux;
        Ok(())
    }
    fn add_node(
        &mut self,
        atom: Option<usize>,
        visit: Option<NonZeroU32>,
        distance: i32,
        mut flags: u8,
        fraction: Option<Fraction>,
    ) -> Result<usize, Error> {
        self.work.spend(1)?;
        // Native expansion checks 100,000 nodes before a whole atom expansion.
        // Allow its final batch while bounding pathological high-degree input.
        if self.nodes.len() >= 500_000 {
            return Err(Error::Limit);
        }
        let source = atom.map(|i| at(&self.state.graph.atoms, i)).transpose()?;
        let number = source.map_or(1, |a| a.atomic_number);
        let isotope = if flags & DUPLICATE == 0 {
            source.map_or(0, |a| a.isotope)
        } else {
            0
        };
        let mass = if flags & DUPLICATE != 0 {
            0.
        } else if isotope == 0 {
            at(ELEMENTS, usize::from(number))?.average
        } else {
            ISOTOPES
                .binary_search_by_key(&(number, isotope), |&(n, i, _)| (n, i))
                .ok()
                .and_then(|i| ISOTOPES.get(i))
                .map_or(0., |&(_, _, mass)| mass)
        };
        if visit.is_none() || flags & DUPLICATE != 0 {
            flags |= EXPANDED;
        }
        if let Some(a) = atom {
            *at_mut(&mut self.seen, a)? = true;
        }
        let id = self.nodes.len();
        self.nodes.push(Node {
            atom,
            distance,
            fraction: fraction.unwrap_or(Fraction(u32::from(number), 1)),
            number,
            isotope,
            mass,
            flags,
            aux: Descriptor::None,
            edges: Vec::new(),
            visit,
        });
        Ok(id)
    }
    fn add_edge(&mut self, begin: usize, end: usize, bond: Option<usize>) -> Result<(), Error> {
        let id = self.edges.len();
        at_mut(&mut self.nodes, begin)?.edges.push(id);
        at_mut(&mut self.nodes, end)?.edges.push(id);
        self.edges.push(Edge {
            begin,
            end,
            bond,
            aux: Descriptor::None,
        });
        Ok(())
    }
    fn child(&mut self, begin: usize, atom: usize) -> Result<usize, Error> {
        let parent = self.node(begin)?;
        let distance = parent.distance.checked_add(1).ok_or(Error::Limit)?;
        let visit = self
            .visits
            .set(parent.visit, atom, distance as c_char, &mut self.work)?;
        self.add_node(Some(atom), visit, distance, 0, None)
    }
    fn terminal(
        &mut self,
        mol: &mut Molecule<'a>,
        begin: usize,
        atom: Option<usize>,
        flags: u8,
    ) -> Result<usize, Error> {
        let parent = self.node(begin)?;
        let distance = if flags & DUPLICATE != 0 {
            i32::from(self.visits.get(
                parent.visit,
                atom.ok_or_else(|| invalid("Missing duplicate atom"))?,
                &mut self.work,
            )?)
        } else {
            parent.distance.checked_add(1).ok_or(Error::Limit)?
        };
        let fraction = if flags & BOND_DUPLICATE != 0 {
            let f = mol.fraction(
                self.node(begin)?
                    .atom
                    .ok_or_else(|| invalid("Missing source atom"))?,
            )?;
            (f.1 > 1).then_some(f)
        } else {
            None
        };
        self.add_node(atom, None, distance, flags, fraction)
    }
    pub fn edges(&mut self, mol: &mut Molecule<'a>, node: usize) -> Result<&[usize], Error> {
        self.check(mol)?;
        if !self.node(node)?.is_expanded() {
            at_mut(&mut self.nodes, node)?.flags |= EXPANDED;
            if let Err(error) = self.expand(mol, node) {
                self.failed = true;
                return Err(error);
            }
        }
        Ok(&self.node(node)?.edges)
    }
    fn expand(&mut self, mol: &mut Molecule<'a>, begin: usize) -> Result<(), Error> {
        if self.nodes.len() >= 100_000 {
            return Err(Error::Nodes);
        }
        let parent = self.node(begin)?;
        let atom = parent
            .atom
            .ok_or_else(|| invalid("Cannot expand implicit hydrogen"))?;
        let visit = parent.visit;
        let previous = parent
            .edges
            .first()
            .map(|&e| self.edge(e))
            .transpose()?
            .filter(|edge| edge.begin != begin)
            .and_then(|edge| edge.bond);
        let origin_atom = self.node(0)?.atom;
        let negative = at(&self.state.graph.atoms, atom)?.charge < 0;
        for j in 0..at(&mol.adjacent, atom)?.len() {
            self.work.spend(1)?;
            let (neighbor, bond) = *at(at(&mol.adjacent, atom)?, j)?;
            let order = mol.bond_order(bond)?;
            let mut duplicates = order.saturating_sub(1);
            if self.visits.get(visit, neighbor, &mut self.work)? == 0 {
                let end = self.child(begin, neighbor)?;
                self.add_edge(begin, end, Some(bond))?;
                if begin == 0 && !self.atrop {
                    continue;
                }
                if negative && mol.fraction(atom)?.1 > 1 {
                    duplicates = 1;
                }
            } else if previous == Some(bond) {
                if origin_atom == Some(neighbor) && !self.atrop {
                    continue;
                }
            } else {
                let end = self.terminal(mol, begin, Some(neighbor), RING_DUPLICATE)?;
                self.add_edge(begin, end, Some(bond))?;
                if negative && mol.fraction(atom)?.1 > 1 {
                    duplicates = 1;
                }
            }
            for _ in 0..duplicates {
                let end = self.terminal(mol, begin, Some(neighbor), BOND_DUPLICATE)?;
                self.add_edge(begin, end, Some(bond))?;
            }
        }
        let hydrogens = u32::from(at(&self.state.graph.atoms, atom)?.explicit_hydrogens)
            + at(&self.state.valences, atom)?.implicit_hydrogens;
        for _ in 0..hydrogens {
            let end = self.terminal(mol, begin, None, IMPLICIT_HYDROGEN)?;
            self.add_edge(begin, end, None)?;
        }
        Ok(())
    }
    pub fn change_root(&mut self, mol: &mut Molecule<'a>, new_root: usize) -> Result<(), Error> {
        self.check(mol)?;
        self.node(new_root)?;
        let mut queue = vec![new_root];
        let mut flip = Vec::new();
        let mut index = 0;
        while let Some(&node) = queue.get(index) {
            index += 1;
            let edges = self.edges(mol, node)?.to_vec();
            self.work.spend(edges.len() + 1)?;
            for e in edges {
                let edge = self.edge(e)?;
                if edge.end == node {
                    queue.push(edge.begin);
                    flip.push(e);
                }
            }
        }
        for e in flip {
            let edge = at_mut(&mut self.edges, e)?;
            std::mem::swap(&mut edge.begin, &mut edge.end);
        }
        self.current = new_root;
        Ok(())
    }
    pub fn nodes_for_atom(
        &mut self,
        mol: &mut Molecule<'a>,
        atom: usize,
    ) -> Result<Vec<usize>, Error> {
        self.check(mol)?;
        at(&self.state.graph.atoms, atom)?;
        let mut found = Vec::new();
        let mut queue = vec![self.current];
        let mut index = 0;
        while let Some(&node) = queue.get(index) {
            index += 1;
            if self.node(node)?.atom == Some(atom) {
                found.push(node);
            }
            let edges = self.edges(mol, node)?.to_vec();
            self.work.spend(edges.len() + 1)?;
            for e in edges {
                let edge = self.edge(e)?;
                if edge.begin == node {
                    queue.push(edge.end);
                }
            }
        }
        Ok(found)
    }
    pub fn edges_to_atom(
        &mut self,
        mol: &mut Molecule<'a>,
        node: usize,
        atom: usize,
    ) -> Result<Vec<usize>, Error> {
        at(&self.state.graph.atoms, atom)?;
        self.edges(mol, node)?
            .to_vec()
            .into_iter()
            .filter_map(|e| {
                let check = || -> Result<bool, Error> {
                    let edge = self.edge(e)?;
                    let end = self.node(edge.end)?;
                    Ok(!end.is_duplicate()
                        && (end.atom == Some(atom) || self.node(edge.begin)?.atom == Some(atom)))
                };
                check().map(|yes| yes.then_some(e)).transpose()
            })
            .collect()
    }
    pub fn nonterminal_out_edges(
        &mut self,
        mol: &mut Molecule<'a>,
        node: usize,
    ) -> Result<Vec<usize>, Error> {
        self.edges(mol, node)?
            .to_vec()
            .into_iter()
            .filter_map(|e| {
                let check = || -> Result<bool, Error> {
                    let edge = self.edge(e)?;
                    Ok(edge.begin == node && !self.node(edge.end)?.is_terminal())
                };
                check().map(|yes| yes.then_some(e)).transpose()
            })
            .collect()
    }
}
