use anyhow::Context;
use reshiki::chemistry::stereo::cip::digraph::Digraph;
use serde_json::{Value, json};
use std::collections::{HashMap, hash_map::Entry};
pub struct View {
    pub nodes: Vec<usize>,
    pub edges: Vec<usize>,
    pub ids: HashMap<usize, usize>,
    pub eids: HashMap<usize, usize>,
}
impl View {
    pub fn new(g: &Digraph<'_>) -> anyhow::Result<Self> {
        let mut v = Self {
            nodes: vec![g.original_root()],
            edges: Vec::new(),
            ids: HashMap::from([(g.original_root(), 0)]),
            eids: HashMap::new(),
        };
        let mut i = 0;
        while let Some(&node) = v.nodes.get(i) {
            i += 1;
            for &e in g.node(node)?.stored_edges() {
                if let Entry::Vacant(entry) = v.eids.entry(e) {
                    entry.insert(v.edges.len());
                    v.edges.push(e);
                }
                let other = g.edge(e)?.other(node)?;
                if let Entry::Vacant(entry) = v.ids.entry(other) {
                    entry.insert(v.nodes.len());
                    v.nodes.push(other);
                }
            }
        }
        anyhow::ensure!(
            v.nodes.len() == g.node_count() && v.edges.len() == g.edge_count(),
            "Disconnected CIP expansion"
        );
        Ok(v)
    }
    pub fn edge_ids(&self, edges: &[usize]) -> anyhow::Result<Vec<usize>> {
        edges
            .iter()
            .map(|e| self.eids.get(e).copied().context("Missing edge ID"))
            .collect()
    }
    pub fn node_id(&self, node: usize) -> anyhow::Result<usize> {
        self.ids.get(&node).copied().context("Missing node ID")
    }
    pub fn snapshot(&self, g: &Digraph<'_>, atoms: usize) -> anyhow::Result<Value> {
        let nodes = self
            .nodes
            .iter()
            .map(|&id| -> anyhow::Result<Value> {
                let n = g.node(id)?;
                let distance = if n.is_duplicate() {
                    i32::from(n.distance as u8)
                } else {
                    n.distance
                };
                Ok(json!([
                    n.atom.map_or(-1, |a| a as i64),
                    distance,
                    n.fraction.0,
                    n.fraction.1,
                    n.number,
                    n.isotope,
                    n.mass.to_bits(),
                    n.flags,
                    n.is_terminal(),
                    n.aux as u8,
                    self.edge_ids(n.stored_edges())?
                ]))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let edges = self
            .edges
            .iter()
            .map(|&id| -> anyhow::Result<Value> {
                let e = g.edge(id)?;
                Ok(json!([
                    self.node_id(e.begin)?,
                    self.node_id(e.end)?,
                    e.bond.map_or(-1, |b| b as i64),
                    e.aux as u8
                ]))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let seen = (0..atoms)
            .map(|a| g.seen_atom(a))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(
            json!({"root":self.node_id(g.current_root())?, "nodes":nodes, "edges":edges, "seen":seen, "rule6":g.rule6_reference().map_or(-1, |a| a as i64)}),
        )
    }
}
