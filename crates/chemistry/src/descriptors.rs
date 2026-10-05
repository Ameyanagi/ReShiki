//! Local molecular descriptors from a sanitized graph and its ring membership.
//! RDKit 2026.03.6 rules; see licenses/rdkit/NOTICE for source attribution.
//! Copyright (C) 2004-2013 Greg Landrum and Rational Discovery LLC.
//! BSD-3-Clause; see licenses/rdkit/LICENSE.
mod matcher;
#[cfg(test)]
mod tests;
mod tpsa;
use super::{RDKIT_VERSION, graph::Graph};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    sync::OnceLock,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct Descriptors {
    pub donors: u32,
    pub acceptors: u32,
    pub logp: f64,
    pub molar_refractivity: f64,
    pub tpsa: f64,
    pub tpsa_with_s_p: f64,
    pub tpsa_atoms: Vec<f64>,
    pub tpsa_s_p_atoms: Vec<f64>,
    /// Contributions include added hydrogen atoms in original atom order.
    pub crippen_atoms: Vec<[f64; 2]>,
    pub crippen_types: Vec<Option<usize>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Data {
    rdkit_version: String,
    patterns: Vec<matcher::Pattern>,
    crippen: Vec<Rule>,
    donors: usize,
    acceptors: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Rule {
    label: String,
    smarts: String,
    pattern: usize,
    logp: f64,
    mr: f64,
}
fn data() -> Result<&'static Data, String> {
    static DATA: OnceLock<Result<Data, String>> = OnceLock::new();
    DATA.get_or_init(|| {
        let data: Data = serde_json::from_str(include_str!("descriptor_data.json"))
            .map_err(|e| format!("Invalid descriptor data: {e}"))?;
        if data.rdkit_version != RDKIT_VERSION || data.crippen.len() != 110 {
            return Err("Descriptor data version mismatch".into());
        }
        matcher::validate(&data.patterns)?;
        for id in [data.donors, data.acceptors]
            .into_iter()
            .chain(data.crippen.iter().map(|r| r.pattern))
        {
            at(&data.patterns, id)?;
        }
        for rule in &data.crippen {
            if rule.label.is_empty()
                || rule.smarts.is_empty()
                || !rule.logp.is_finite()
                || !rule.mr.is_finite()
            {
                return Err("Invalid descriptor rule".into());
            }
        }
        Ok(data)
    })
    .as_ref()
    .map_err(Clone::clone)
}
fn at<T>(items: &[T], index: usize) -> Result<&T, String> {
    items
        .get(index)
        .ok_or_else(|| "Invalid descriptor topology index".into())
}
struct Work(usize);
impl Work {
    fn spend(&mut self, n: usize) -> Result<(), String> {
        self.0 = self
            .0
            .checked_sub(n)
            .ok_or("Descriptor work limit exceeded")?;
        Ok(())
    }
}
struct Node {
    number: u8,
    charge: i8,
    aromatic: bool,
    hydrogens: u32,
    degree: usize,
    valence: u32,
}
struct Edge {
    order: u8,
    aromatic: bool,
    in_ring: bool,
}
struct Target {
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    adjacent: Vec<Vec<(usize, usize)>>,
    attached_h: Vec<u32>,
    triangles: Vec<bool>,
}
impl Target {
    fn new(graph: &Graph, rings: &[Vec<usize>]) -> Result<Self, String> {
        let valences = graph.valences()?;
        let mut nodes = graph
            .atoms
            .iter()
            .zip(valences)
            .map(|(a, v)| Node {
                number: a.atomic_number,
                charge: a.charge,
                aromatic: a.aromatic,
                hydrogens: u32::from(a.explicit_hydrogens) + v.implicit_hydrogens,
                degree: 0,
                valence: v.explicit_valence + v.implicit_hydrogens,
            })
            .collect::<Vec<_>>();
        let attached_h = nodes.iter().map(|n| n.hydrogens).collect::<Vec<_>>();
        let mut adjacent = vec![Vec::new(); nodes.len()];
        let mut edges = Vec::new();
        let mut pairs = HashMap::new();
        for (i, b) in graph.bonds.iter().enumerate() {
            for (a, other) in [(b.a, b.b), (b.b, b.a)] {
                let hydrogen = at(&nodes, other)?.number == 1;
                let node = nodes.get_mut(a).ok_or("Missing descriptor endpoint")?;
                node.degree += 1;
                node.hydrogens += u32::from(hydrogen);
                adjacent
                    .get_mut(a)
                    .ok_or("Missing descriptor endpoint")?
                    .push((other, i));
            }
            pairs.insert((b.a.min(b.b), b.a.max(b.b)), i);
            edges.push(Edge {
                order: b.order,
                aromatic: b.aromatic,
                in_ring: false,
            });
        }
        for (node, &h) in nodes.iter_mut().zip(&attached_h) {
            node.degree += h as usize;
        }
        let mut triangles = vec![false; nodes.len()];
        let mut total = 0usize;
        for ring in rings {
            total = total
                .checked_add(ring.len())
                .ok_or("Descriptor ring storage limit exceeded")?;
            if ring.len() < 3
                || total > 2_000_000
                || ring.iter().collect::<HashSet<_>>().len() != ring.len()
            {
                return Err("Invalid descriptor ring data".into());
            }
            for (&a, &b) in ring.iter().zip(ring.iter().cycle().skip(1)) {
                if ring.len() == 3 {
                    *triangles.get_mut(a).ok_or("Missing ring atom")? = true;
                }
                let &edge = pairs
                    .get(&(a.min(b), a.max(b)))
                    .ok_or("Missing descriptor ring bond")?;
                edges
                    .get_mut(edge)
                    .ok_or("Missing descriptor ring bond")?
                    .in_ring = true;
            }
        }
        Ok(Self {
            nodes,
            edges,
            adjacent,
            attached_h,
            triangles,
        })
    }

    /// Graph-only equivalent of AddHs for the descriptor queries. Atom metadata
    /// needed by the queries is invariant when attached H becomes graph H.
    fn add_hydrogens(&mut self) -> Result<(), String> {
        let count = self
            .attached_h
            .iter()
            .try_fold(self.nodes.len(), |n, &h| n.checked_add(h as usize))
            .ok_or("Descriptor hydrogen limit exceeded")?;
        if count > 500_000 {
            return Err("Descriptor hydrogen expansion exceeds 500,000 atoms".into());
        }
        self.nodes.reserve(count - self.nodes.len());
        self.adjacent.reserve(count - self.adjacent.len());
        for (a, &count) in self.attached_h.iter().enumerate() {
            for _ in 0..count {
                let number = at(&self.nodes, a)?.number;
                let id = self.nodes.len();
                self.nodes.push(Node {
                    number: 1,
                    charge: 0,
                    aromatic: false,
                    hydrogens: u32::from(number == 1),
                    degree: 1,
                    valence: 1,
                });
                let edge = self.edges.len();
                self.edges.push(Edge {
                    order: 1,
                    aromatic: false,
                    in_ring: false,
                });
                self.adjacent
                    .get_mut(a)
                    .ok_or("Missing hydrogen parent")?
                    .push((id, edge));
                self.adjacent.push(vec![(a, edge)]);
            }
        }
        Ok(())
    }
}

pub fn calculate(graph: &Graph, ring_atoms: &[Vec<usize>]) -> Result<Descriptors, String> {
    let data = data()?;
    let mut target = Target::new(graph, ring_atoms)?;
    let mut work = Work(100_000_000);
    let mut matcher = matcher::Matcher::new(&target, &data.patterns, &mut work);
    let donors = matcher.roots(data.donors)?.len() as u32;
    let acceptors = matcher.roots(data.acceptors)?.len() as u32;
    let tpsa_atoms = tpsa::contributions(&target, false)?;
    let tpsa_s_p_atoms = tpsa::contributions(&target, true)?;
    target.add_hydrogens()?;
    let mut crippen_atoms = vec![[0., 0.]; target.nodes.len()];
    let mut crippen_types = vec![None; target.nodes.len()];
    let mut remaining = target.nodes.len();
    let mut matcher = matcher::Matcher::new(&target, &data.patterns, &mut work);
    for (rule_id, rule) in data.crippen.iter().enumerate() {
        for atom in matcher.roots(rule.pattern)? {
            let kind = crippen_types
                .get_mut(atom)
                .ok_or("Missing descriptor atom")?;
            if kind.is_none() {
                *kind = Some(rule_id);
                *crippen_atoms
                    .get_mut(atom)
                    .ok_or("Missing descriptor atom")? = [rule.logp, rule.mr];
                remaining -= 1;
            }
        }
        if remaining == 0 {
            break;
        }
    }
    // Preserve RDKit's original atom-order accumulation (including appended H).
    Ok(Descriptors {
        donors,
        acceptors,
        logp: crippen_atoms.iter().map(|v| v[0]).sum(),
        molar_refractivity: crippen_atoms.iter().map(|v| v[1]).sum(),
        tpsa: tpsa_atoms.iter().sum(),
        tpsa_with_s_p: tpsa_s_p_atoms.iter().sum(),
        tpsa_atoms,
        tpsa_s_p_atoms,
        crippen_atoms,
        crippen_types,
    })
}
