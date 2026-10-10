//! Ordered assembly views from OPSIN 2.9.0 `BuildResults.java`.
//! Copyright Daniel Lowe and OPSIN contributors; MIT (see LICENSE).
//! Out atoms retain arena identities, so removal updates their owning fragment.

use crate::{
    graph::{AtomId, FragmentId, Graph, GraphError, OutAtom, OutAtomId},
    parse_tree::{Arena, NodeId},
};

#[derive(Debug, Clone, Default)]
pub struct BuildResults {
    pub fragments: Vec<FragmentId>,
    pub out_atoms: Vec<OutAtomId>,
    pub functional_atoms: Vec<AtomId>,
}

impl BuildResults {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn from_node(arena: &Arena, graph: &Graph, node: NodeId) -> Result<Self, GraphError> {
        let mut result = Self::new();
        for group in arena.descendants_named(node, "group") {
            let fragment = arena[group]
                .fragment
                .ok_or_else(|| GraphError("Group has no constructed fragment".into()))?;
            if !result.fragments.contains(&fragment) {
                result.fragments.push(fragment);
            }
            result
                .out_atoms
                .extend(graph.fragment(fragment).out_atoms.iter().map(|out| out.id));
            if let Some(parent) = arena[group].parent {
                let next = next_group(arena, group);
                if arena[parent].name == "root" || next.is_none() {
                    result
                        .functional_atoms
                        .extend(&graph.fragment(fragment).functional_atoms);
                }
            }
        }
        Ok(result)
    }
    pub fn out_atom<'a>(&self, graph: &'a Graph, index: usize) -> Result<&'a OutAtom, GraphError> {
        let id = *self
            .out_atoms
            .get(index)
            .ok_or_else(|| GraphError("Missing assembly out atom".into()))?;
        graph
            .fragments
            .iter()
            .flat_map(|fragment| &fragment.out_atoms)
            .find(|out| out.id == id)
            .ok_or_else(|| GraphError("Assembly out atom has already been consumed".into()))
    }
    pub fn remove_out_atom(
        &mut self,
        graph: &mut Graph,
        index: usize,
    ) -> Result<OutAtom, GraphError> {
        let out = self.out_atom(graph, index)?.clone();
        let owner = graph.atom(out.atom).fragment;
        let position = graph
            .fragment(owner)
            .out_atoms
            .iter()
            .position(|candidate| candidate.id == out.id)
            .ok_or_else(|| GraphError("Out atom missing from owning fragment".into()))?;
        graph.remove_out_atom(owner, position);
        self.out_atoms.remove(index);
        Ok(out)
    }
    pub fn remove_all_out_atoms(&mut self, graph: &mut Graph) -> Result<(), GraphError> {
        for index in (0..self.out_atoms.len()).rev() {
            self.remove_out_atom(graph, index)?;
        }
        Ok(())
    }
    pub fn remove_functional_atom(
        &mut self,
        graph: &mut Graph,
        index: usize,
    ) -> Result<AtomId, GraphError> {
        let atom = *self
            .functional_atoms
            .get(index)
            .ok_or_else(|| GraphError("Missing assembly functional atom".into()))?;
        let fragment = graph.atom(atom).fragment;
        let position = graph
            .fragment(fragment)
            .functional_atoms
            .iter()
            .position(|&candidate| candidate == atom)
            .ok_or_else(|| GraphError("Functional atom missing from owning fragment".into()))?;
        graph
            .fragment_mut(fragment)
            .functional_atoms
            .remove(position);
        self.functional_atoms.remove(index);
        Ok(atom)
    }
    pub fn atom_by_id(&self, graph: &Graph, atom: AtomId) -> Result<AtomId, GraphError> {
        self.fragments
            .iter()
            .any(|&fragment| graph.fragment(fragment).atoms.contains(&atom))
            .then_some(atom)
            .ok_or_else(|| GraphError(format!("No fragment contained this id: {}", atom.0)))
    }
    pub fn merge(&mut self, other: Self) {
        self.out_atoms.extend(other.out_atoms);
        self.functional_atoms.extend(other.functional_atoms);
        for fragment in other.fragments {
            if !self.fragments.contains(&fragment) {
                self.fragments.push(fragment);
            }
        }
    }
    pub fn charge(&self, graph: &Graph) -> i32 {
        self.fragments
            .iter()
            .flat_map(|&fragment| &graph.fragment(fragment).atoms)
            .map(|&atom| graph.atom(atom).charge)
            .sum()
    }
}

fn next_group(arena: &Arena, mut node: NodeId) -> Option<NodeId> {
    if arena[node].name == "group" {
        node = arena[node].parent?;
    }
    loop {
        let parent = arena[node].parent?;
        if arena[parent].name == "molecule" {
            return None;
        }
        if let Some(mut next) = arena.next_sibling(node) {
            while let Some(&child) = arena[next].children.first() {
                next = child;
            }
            let scope = arena[next].parent?;
            if let Some(group) = arena.first_child_named(scope, "group") {
                return Some(group);
            }
            node = next;
        } else {
            node = parent;
        }
    }
}
