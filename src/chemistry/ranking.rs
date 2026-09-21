//! Canonical partition ranking adapted from RDKit new_canon.cpp/.h.
//! Copyright (C) 2014 Greg Landrum; adapted from Roger Sayle's pseudocode.
//! Atropisomer neighbor ordering from Atropisomers.cpp, Copyright (C)
//! 2004-2021 Tad hurst/CDD and other RDKit contributors.
//! BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
mod compare;
mod partition;
mod symmetry;
#[cfg(test)]
mod tests;

use super::graph::{Graph, Valence};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AtomMetadata {
    pub map_number: i32,
    /// RDKit atom winding codes: 0 unspecified, 1 CW, 2 CCW, 3..8 other geometries.
    pub chiral_tag: u8,
    #[serde(default)]
    pub chiral_permutation: Option<u32>,
    pub ring_stereo: bool,
    pub non_stereo_rank: i32,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BondMetadata {
    /// 0 none, 1 unknown, 2 Z, 3 E, 4 cis, 5 trans, 6/7 atropisomer winding.
    pub stereo: u8,
    pub stereo_atoms: Vec<usize>,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StereoGroup {
    /// 0 absolute, 1 OR, 2 AND.
    pub kind: u8,
    pub atoms: Vec<usize>,
    #[serde(default)]
    pub bonds: Vec<usize>,
    #[serde(default)]
    pub read_id: u32,
    #[serde(default)]
    pub write_id: u32,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Metadata {
    pub atoms: Vec<AtomMetadata>,
    pub bonds: Vec<BondMetadata>,
    pub groups: Vec<StereoGroup>,
}
impl Metadata {
    pub fn unspecified(graph: &Graph) -> Self {
        Self {
            atoms: vec![AtomMetadata::default(); graph.atoms.len()],
            bonds: vec![BondMetadata::default(); graph.bonds.len()],
            groups: Vec::new(),
        }
    }

    pub(crate) fn validate(&self, graph: &Graph) -> Result<(), String> {
        let n = graph.atoms.len();
        if self.atoms.len() != n
            || self.bonds.len() != graph.bonds.len()
            || self.groups.len() > 2_000_000
            || self.atoms.iter().any(|a| a.chiral_tag > 8)
            || self.bonds.iter().any(|b| {
                b.stereo > 7 || b.stereo_atoms.len() > 2 || b.stereo_atoms.iter().any(|&a| a >= n)
            })
        {
            return Err("Invalid stereo metadata".into());
        }
        // RDKit's molecule setter merges absolute groups before these passes.
        // Require that normalized representation at the graph boundary.
        if self.groups.iter().filter(|g| g.kind == 0).take(2).count() > 1 {
            return Err("Multiple unmerged absolute stereo groups".into());
        }
        let mut storage = 0usize;
        for group in &self.groups {
            storage = storage
                .checked_add(1)
                .and_then(|s| s.checked_add(group.atoms.len()))
                .and_then(|s| s.checked_add(group.bonds.len()))
                .ok_or("Stereo group storage exceeded")?;
            if group.kind > 2
                || storage > 2_000_000
                || group.atoms.iter().any(|&a| a >= n)
                || group.bonds.iter().any(|&b| b >= graph.bonds.len())
                || group.atoms.iter().collect::<HashSet<_>>().len() != group.atoms.len()
                || group.bonds.iter().collect::<HashSet<_>>().len() != group.bonds.len()
            {
                return Err("Invalid stereo group".into());
            }
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Options {
    pub break_ties: bool,
    pub include_chirality: bool,
    pub include_isotopes: bool,
    pub include_maps: bool,
    pub include_chiral_presence: bool,
    pub include_stereo_groups: bool,
    pub use_non_stereo_ranks: bool,
    pub include_ring_stereo: bool,
    /// Rank the complete graph using the fragment initializer, as Kekulize
    /// does. This is not a subset-mask or custom-symbol interface.
    pub fragment: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            break_ties: true,
            include_chirality: true,
            include_isotopes: true,
            include_maps: true,
            include_chiral_presence: false,
            include_stereo_groups: true,
            use_non_stereo_ranks: false,
            include_ring_stereo: true,
            fragment: false,
        }
    }
}
fn at<T>(values: &[T], index: usize) -> Result<&T, String> {
    values
        .get(index)
        .ok_or_else(|| "Invalid ranking index".into())
}
fn put<T>(values: &mut [T], index: usize, value: T) -> Result<(), String> {
    *values.get_mut(index).ok_or("Invalid ranking index")? = value;
    Ok(())
}
struct Work(usize);
impl Work {
    fn spend(&mut self, amount: usize) -> Result<(), String> {
        self.0 = self
            .0
            .checked_sub(amount)
            .ok_or("Canonical ranking work limit exceeded")?;
        Ok(())
    }
}
#[derive(Clone, Copy)]
struct Edge {
    other: usize,
    kind: u8,
    stereo: u8,
    controls: [Option<usize>; 4],
    rank: i32,
}
#[derive(Clone, Copy)]
enum Comparison {
    Normal,
    Chirality,
    Symmetry,
}

struct Ranker<'a> {
    graph: &'a Graph,
    metadata: &'a Metadata,
    options: Options,
    neighbors: Vec<Vec<usize>>,
    bonds: Vec<Vec<Edge>>,
    hydrogens: Vec<u32>,
    ring_counts: Vec<usize>,
    groups: Vec<Option<usize>>,
    ring_stereo: Vec<bool>,
    has_ring_neighbor: Vec<bool>,
    classes: Vec<usize>,
    // Mol initialization builds atom records in index order. Uninitialized
    // controlling-atom ranks are -1 during its initial bond-list sort.
    initializing: Option<usize>,
    neighbor_numbers: Vec<Vec<i32>>,
    revisited: Vec<Vec<i32>>,
    work: Work,
}
impl<'a> Ranker<'a> {
    fn new(
        graph: &'a Graph,
        rings: &[Vec<usize>],
        metadata: &'a Metadata,
        options: Options,
        work: Work,
        cache: Option<&[Valence]>,
    ) -> Result<Self, String> {
        let valences = graph.cached_valences(cache)?;
        let n = graph.atoms.len();
        metadata.validate(graph)?;
        let mut neighbors = vec![Vec::new(); n];
        let mut pairs = HashMap::new();
        for (id, bond) in graph.bonds.iter().enumerate() {
            neighbors
                .get_mut(bond.a)
                .ok_or("Missing ranking atom")?
                .push(bond.b);
            neighbors
                .get_mut(bond.b)
                .ok_or("Missing ranking atom")?
                .push(bond.a);
            pairs.insert((bond.a.min(bond.b), bond.a.max(bond.b)), id);
        }
        let mut ring_counts = vec![0usize; n];
        let mut storage = 0usize;
        for ring in rings {
            storage = storage
                .checked_add(ring.len())
                .ok_or("Ranking ring storage exceeded")?;
            if storage > 2_000_000
                || ring.len() < 3
                || ring.iter().collect::<HashSet<_>>().len() != ring.len()
            {
                return Err("Invalid ranking rings".into());
            }
            for (&a, &b) in ring.iter().zip(ring.iter().cycle().skip(1)) {
                if !pairs.contains_key(&(a.min(b), a.max(b))) {
                    return Err("Missing ranking ring bond".into());
                }
                *ring_counts.get_mut(a).ok_or("Missing ranking ring atom")? += 1;
            }
        }
        let mut groups = vec![None; n];
        for (id, group) in metadata.groups.iter().enumerate() {
            for &atom in &group.atoms {
                if options.include_chirality && options.include_stereo_groups && !options.fragment {
                    put(&mut groups, atom, Some(id))?;
                }
            }
        }
        let ring_stereo = metadata
            .atoms
            .iter()
            .map(|a| a.ring_stereo && matches!(a.chiral_tag, 1 | 2))
            .collect::<Vec<_>>();
        let mut has_ring_neighbor = Vec::new();
        for list in &neighbors {
            let mut has = false;
            for &atom in list {
                has |= *at(&ring_stereo, atom)?;
            }
            has_ring_neighbor.push(has);
        }
        let mut bonds = vec![Vec::new(); n];
        for (bond, meta) in graph.bonds.iter().zip(&metadata.bonds) {
            let mut controls = [None; 4];
            if options.include_chirality && matches!(meta.stereo, 4 | 5) {
                if meta.stereo_atoms.len() != 2 {
                    return Err("Missing cis/trans reference atoms".into());
                }
                for (side, atom, opposite) in [(0, bond.a, bond.b), (1, bond.b, bond.a)] {
                    let reference = *at(&meta.stereo_atoms, side)?;
                    let list = at(&neighbors, atom)?;
                    if reference == opposite || !list.contains(&reference) {
                        return Err("Invalid cis/trans reference atom".into());
                    }
                    put(&mut controls, 2 * side, Some(reference))?;
                    for &other in list {
                        if other != opposite && other != reference {
                            put(&mut controls, 2 * side + 1, Some(other))?;
                        }
                    }
                }
            } else if options.include_chirality && matches!(meta.stereo, 6 | 7) {
                for (side, atom, opposite) in [(0, bond.a, bond.b), (1, bond.b, bond.a)] {
                    let mut list = at(&neighbors, atom)?
                        .iter()
                        .copied()
                        .filter(|&a| a != opposite)
                        .collect::<Vec<_>>();
                    if list.is_empty() {
                        return Err("Missing atropisomer reference atoms".into());
                    }
                    if list.len() == 2 {
                        list.sort_unstable();
                    }
                    put(&mut controls, 2 * side, list.first().copied())?;
                    put(&mut controls, 2 * side + 1, list.get(1).copied())?;
                }
            }
            let kind = if bond.aromatic {
                12
            } else {
                match bond.order {
                    0 => 14,
                    1..=3 => bond.order,
                    4 => 12,
                    5 => 17,
                    6 => 4,
                    7 => 7,
                    _ => return Err("Unsupported ranking bond".into()),
                }
            };
            for (a, other) in [(bond.a, bond.b), (bond.b, bond.a)] {
                bonds
                    .get_mut(a)
                    .ok_or("Missing ranking bond endpoint")?
                    .push(Edge {
                        other,
                        kind,
                        stereo: if options.include_chirality {
                            meta.stereo
                        } else {
                            0
                        },
                        controls,
                        rank: 0,
                    });
            }
        }
        let hydrogens = graph
            .atoms
            .iter()
            .zip(valences)
            .map(|(a, v)| u32::from(a.explicit_hydrogens) + v.implicit_hydrogens)
            .collect();
        let mut result = Self {
            graph,
            metadata,
            options,
            neighbors,
            bonds,
            hydrogens,
            ring_counts,
            groups,
            ring_stereo,
            has_ring_neighbor,
            classes: (0..n).collect(),
            initializing: None,
            neighbor_numbers: vec![Vec::new(); n],
            revisited: vec![Vec::new(); n],
            work,
        };
        for atom in 0..n {
            if !options.fragment {
                result.initializing = Some(atom + 1);
            }
            result.sort_bonds(atom, false)?;
        }
        result.initializing = None;
        result.classes.fill(0);
        Ok(result)
    }
}

/// Rank a whole molecular graph. All caches and output are local to this call.
/// Ring membership and stereochemical metadata must describe the same graph.
pub fn rank(
    graph: &Graph,
    rings: &[Vec<usize>],
    metadata: &Metadata,
    options: Options,
) -> Result<Vec<u32>, String> {
    rank_with_work(graph, rings, metadata, options, Work(100_000_000))
}
fn rank_with_work(
    graph: &Graph,
    rings: &[Vec<usize>],
    metadata: &Metadata,
    options: Options,
    work: Work,
) -> Result<Vec<u32>, String> {
    rank_cached_with_work(graph, rings, metadata, options, work, None)
}

pub(crate) fn rank_cached(
    graph: &Graph,
    rings: &[Vec<usize>],
    metadata: &Metadata,
    options: Options,
    cache: &[Valence],
) -> Result<Vec<u32>, String> {
    rank_cached_with_work(
        graph,
        rings,
        metadata,
        options,
        Work(100_000_000),
        Some(cache),
    )
}

fn rank_cached_with_work(
    graph: &Graph,
    rings: &[Vec<usize>],
    metadata: &Metadata,
    options: Options,
    work: Work,
    cache: Option<&[Valence]>,
) -> Result<Vec<u32>, String> {
    let mut ranker = Ranker::new(graph, rings, metadata, options, work, cache)?;
    if graph.atoms.is_empty() {
        return Ok(Vec::new());
    }
    let mut partitions = partition::Partitions::new(graph.atoms.len());
    partitions.activate(&mut ranker)?;
    partitions.refine(&mut ranker, Comparison::Normal)?;
    if options.include_chirality && options.include_ring_stereo && partitions.has_ties() {
        partitions.activate(&mut ranker)?;
        partitions.refine(&mut ranker, Comparison::Chirality)?;
    }
    let (mut symmetric, mut ring_atoms, mut branching) = (0usize, 0usize, false);
    for &atom in &partitions.order {
        let count = *at(&partitions.count, atom)?;
        let rings = *at(&ranker.ring_counts, atom)?;
        if rings != 0 {
            ring_atoms += 1;
            if count > 2 {
                symmetric += count;
            }
            branching |= rings > 1 && count > 1;
        }
    }
    if partitions.has_ties() && ring_atoms > 0 && symmetric * 2 > ring_atoms && branching {
        ranker.build_symmetry_invariants()?;
        partitions.activate(&mut ranker)?;
        partitions.refine(&mut ranker, Comparison::Symmetry)?;
    }
    if options.break_ties {
        partitions.break_ties(&mut ranker)?;
    }
    ranker
        .classes
        .into_iter()
        .map(|c| u32::try_from(c).map_err(|_| "Canonical rank overflow".into()))
        .collect()
}
