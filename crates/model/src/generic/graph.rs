//! Checked, ordered atom adjacency for finite generic drawing operations.
use super::geometry::{at, at_mut};
use crate::document::Document;
use std::collections::HashMap;

/// Atom indices plus adjacency in caller edge order. Traversals pop a stack and
/// visit neighbors in that order; Plan::finish places components in this order.
pub(super) struct IndexedGraph {
    pub(super) indices: HashMap<u64, usize>,
    pub(super) adjacent: Vec<Vec<usize>>,
}

impl IndexedGraph {
    /// Index atoms only, so callers can resolve their own IDs before edges.
    pub(super) fn index(doc: &Document) -> Self {
        Self {
            indices: doc
                .atoms
                .iter()
                .enumerate()
                .map(|(i, atom)| (atom.id, i))
                .collect(),
            adjacent: vec![Vec::new(); doc.atoms.len()],
        }
    }

    /// Resolve both endpoints of every edge, then keep the ones `keep` accepts.
    pub(super) fn connect(
        &mut self,
        edges: impl Iterator<Item = (u64, u64)>,
        missing_msg: &str,
        keep: impl Fn(usize, usize) -> bool,
    ) -> Result<(), String> {
        for (a, b) in edges {
            let i = *self.indices.get(&a).ok_or(missing_msg)?;
            let j = *self.indices.get(&b).ok_or(missing_msg)?;
            if keep(i, j) {
                at_mut(&mut self.adjacent, i)?.push(j);
                at_mut(&mut self.adjacent, j)?.push(i);
            }
        }
        Ok(())
    }

    pub(super) fn from_edges(
        doc: &Document,
        edges: impl Iterator<Item = (u64, u64)>,
        missing_msg: &str,
    ) -> Result<Self, String> {
        let mut graph = Self::index(doc);
        graph.connect(edges, missing_msg, |_, _| true)?;
        Ok(graph)
    }

    /// Atoms reachable from `start`, never crossing `blocked_edge` either way.
    pub(super) fn reach(
        &self,
        start: usize,
        blocked_edge: Option<(usize, usize)>,
    ) -> Result<Vec<bool>, String> {
        let mut found = vec![false; self.adjacent.len()];
        *at_mut(&mut found, start)? = true;
        let mut pending = vec![start];
        while let Some(i) = pending.pop() {
            for &j in at(&self.adjacent, i)? {
                if blocked_edge == Some((i, j)) || blocked_edge == Some((j, i)) {
                    continue;
                }
                if !*at(&found, j)? {
                    *at_mut(&mut found, j)? = true;
                    pending.push(j);
                }
            }
        }
        Ok(found)
    }

    /// Per-atom component labels and components in atom-scan order.
    pub(super) fn components(&self) -> Result<(Vec<usize>, Vec<Vec<usize>>), String> {
        let mut labels = vec![usize::MAX; self.adjacent.len()];
        let mut components = Vec::new();
        for start in 0..self.adjacent.len() {
            if *at(&labels, start)? != usize::MAX {
                continue;
            }
            let label = components.len();
            *at_mut(&mut labels, start)? = label;
            let mut pending = vec![start];
            let mut component = Vec::new();
            while let Some(i) = pending.pop() {
                component.push(i);
                for &j in at(&self.adjacent, i)? {
                    if *at(&labels, j)? == usize::MAX {
                        *at_mut(&mut labels, j)? = label;
                        pending.push(j);
                    }
                }
            }
            components.push(component);
        }
        Ok((labels, components))
    }
}

#[cfg(test)]
mod tests;
