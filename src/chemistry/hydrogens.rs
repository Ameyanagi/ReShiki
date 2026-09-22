//! Import hydrogen removal adapted from RDKit AddHs.cpp and RWMol.cpp.
//! Copyright (C) 2001-2025 Greg Landrum and other RDKit contributors.
//! BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
use super::{graph::Graph, kekulize::Direction, ranking::Metadata, sanitize};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Hydrogen removal: {0}")]
    Invalid(String),
    #[error("Hydrogen removal exceeds its work or storage limit")]
    Limit,
}
type Result<T> = std::result::Result<T, Error>;
fn at<T>(items: &[T], index: usize) -> Result<&T> {
    items
        .get(index)
        .ok_or_else(|| Error::Invalid("Missing graph item".into()))
}
fn set<T>(items: &mut [T], index: usize, value: T) -> Result<()> {
    *items
        .get_mut(index)
        .ok_or_else(|| Error::Invalid("Missing graph item".into()))? = value;
    Ok(())
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Annotations {
    /// Attachment atoms, boundary-bond ends and other protected group roles.
    pub protected_atoms: Vec<usize>,
    /// Atom and parent-atom membership, in substance-group order. Removal must
    /// never empty a group. The caller uses kept indices to remap group data.
    pub substance_groups: Vec<Vec<usize>>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub graph: Graph,
    pub metadata: Metadata,
    pub directions: Vec<Direction>,
    pub unknown_atoms: Vec<bool>,
    pub annotations: Annotations,
}
#[derive(Debug, Serialize)]
pub struct Removal {
    pub graph: Graph,
    pub metadata: Metadata,
    pub directions: Vec<Direction>,
    pub unknown_atoms: Vec<bool>,
    /// Original indices of surviving atoms/bonds, for file annotations and IDs.
    pub kept_atoms: Vec<usize>,
    pub kept_bonds: Vec<usize>,
}
#[derive(Clone, Copy)]
struct Edge {
    other: usize,
    bond: usize,
}
struct Work(usize);
impl Work {
    fn spend(&mut self, n: usize) -> Result<()> {
        self.0 = self.0.checked_sub(n).ok_or(Error::Limit)?;
        Ok(())
    }
}

/// Apply default RemoveHs with explicit-H count updates, as SMILES import does.
/// This stage does not sanitize or perceive stereo. Inferred ring annotations
/// are invalidated; graph winding, unknown-stereo flags and surviving groups
/// are retained. Errors leave the input unchanged.
pub fn remove(input: &Input) -> Result<Removal> {
    remove_inner(input, true)
}

/// Sanitizing imports perform the final chiral-H adjustment after sanitization.
pub(crate) fn before_sanitization(input: &Input) -> Result<Removal> {
    remove_inner(input, false)
}

pub(crate) fn finish_sanitization(mut state: sanitize::Sanitized) -> Result<sanitize::Sanitized> {
    let mut changed = Vec::new();
    for (id, (atom, meta)) in state
        .graph
        .atoms
        .iter_mut()
        .zip(&state.metadata.atoms)
        .enumerate()
    {
        if !atom.no_implicit && meta.chiral_tag != 0 && atom.explicit_hydrogens > 1 {
            atom.explicit_hydrogens = 0;
            changed.push(id);
        }
    }
    if !changed.is_empty() {
        let cache = state.graph.provisional_valences().map_err(Error::Invalid)?;
        for id in changed {
            set(&mut state.valences, id, *at(&cache, id)?)?;
        }
    }
    Ok(state)
}

fn remove_inner(input: &Input, finish: bool) -> Result<Removal> {
    input.graph.provisional_valences().map_err(Error::Invalid)?;
    input
        .metadata
        .validate(&input.graph)
        .map_err(Error::Invalid)?;
    let n = input.graph.atoms.len();
    let m = input.graph.bonds.len();
    if input.directions.len() != m || input.unknown_atoms.len() != n {
        return Err(Error::Invalid(
            "Annotation dimensions differ from graph".into(),
        ));
    }
    let mut work = Work(16_000_000);
    let mut protected = vec![false; n];
    work.spend(input.annotations.protected_atoms.len())?;
    for &a in &input.annotations.protected_atoms {
        set(&mut protected, a, true)?;
    }
    work.spend(input.annotations.substance_groups.len())?;
    for group in &input.annotations.substance_groups {
        work.spend(group.len())?;
        if group.iter().any(|&a| a >= n) {
            return Err(Error::Invalid("Missing substance-group atom".into()));
        }
    }
    let mut neighbors = vec![Vec::<Edge>::new(); n];
    let mut has_stereo = vec![false; n];
    for (id, b) in input.graph.bonds.iter().enumerate() {
        for (a, other) in [(b.a, b.b), (b.b, b.a)] {
            neighbors
                .get_mut(a)
                .ok_or(Error::Limit)?
                .push(Edge { other, bond: id });
            if !at(&input.metadata.bonds, id)?.stereo_atoms.is_empty() {
                set(&mut has_stereo, a, true)?;
            }
        }
    }
    let mut removed = vec![false; n];
    for (id, a) in input.graph.atoms.iter().enumerate() {
        let edges = at(&neighbors, id)?;
        if a.atomic_number != 1
            || a.isotope != 0
            || a.charge == -1
            || edges.len() != 1
            || *at(&protected, id)?
        {
            continue;
        }
        let edge = edges.first().ok_or(Error::Limit)?;
        let parent = at(&input.graph.atoms, edge.other)?;
        let meta = at(&input.metadata.atoms, edge.other)?;
        if parent.atomic_number <= 1 || matches!(meta.chiral_tag, 6..=8) {
            continue;
        }
        let parent_edges = at(&neighbors, edge.other)?;
        let defining = parent_edges.len() == 2
            && parent_edges.iter().any(|e| {
                input.graph.bonds.get(e.bond).is_some_and(|b| b.order == 2)
                    && (input
                        .metadata
                        .bonds
                        .get(e.bond)
                        .is_some_and(|b| b.stereo > 1)
                        || input
                            .directions
                            .get(edge.bond)
                            .is_some_and(|d| *d != Direction::None))
            });
        if !defining {
            set(&mut removed, id, true)?;
        }
    }
    for group in &input.annotations.substance_groups {
        if !group.is_empty() && group.iter().all(|&a| removed.get(a) == Some(&true)) {
            for &a in group {
                set(&mut removed, a, false)?;
            }
        }
    }
    let mut graph = input.graph.clone();
    let mut metadata = input.metadata.clone();
    let mut directions = input.directions.clone();
    let mut unknown = input.unknown_atoms.clone();
    let mut alive = vec![true; m];
    let mut degree = neighbors.iter().map(Vec::len).collect::<Vec<_>>();
    for id in (0..n).rev().filter(|&a| removed.get(a) == Some(&true)) {
        let edge = *at(&neighbors, id)?.first().ok_or(Error::Limit)?;
        let parent = edge.other;
        let parent_degree = *at(&degree, parent)?;
        let tag = at(&metadata.atoms, parent)?.chiral_tag;
        let direction = *at(&directions, edge.bond)?;
        let atom = graph.atoms.get_mut(parent).ok_or(Error::Limit)?;
        // Native explicit-H storage is an unsigned byte.
        atom.explicit_hydrogens = atom.explicit_hydrogens.wrapping_add(1);
        // The usual unmarked, nonchiral hub needs no repeated neighbor scans.
        let edges = if tag != 0
            || parent_degree == 2
            || direction != Direction::None
            || *at(&has_stereo, parent)?
        {
            let candidates = at(&neighbors, parent)?;
            work.spend(candidates.len())?;
            candidates
                .iter()
                .copied()
                .filter(|e| alive.get(e.bond) == Some(&true))
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        if tag != 0 {
            let position = edges
                .iter()
                .position(|e| e.bond == edge.bond)
                .ok_or(Error::Limit)?;
            if (edges.len() - position - 1) % 2 != 0 {
                let atom = metadata.atoms.get_mut(parent).ok_or(Error::Limit)?;
                if matches!(tag, 1 | 2) {
                    atom.chiral_tag = 3 - tag;
                } else if tag == 4
                    && let Some(p) = atom.chiral_permutation
                {
                    atom.chiral_permutation = Some(match p {
                        1 => 2,
                        2 => 1,
                        _ => 0,
                    });
                }
            }
        }
        if parent_degree == 2
            && let Some(other) = edges.iter().find(|e| e.bond != edge.bond)
        {
            let meta = metadata.bonds.get_mut(other.bond).ok_or(Error::Limit)?;
            if meta.stereo > 1 {
                meta.stereo = 0;
                meta.stereo_atoms.clear();
            }
        }
        let hbond = at(&graph.bonds, edge.bond)?;
        if direction == Direction::Unknown && hbond.a == parent {
            set(&mut unknown, parent, true)?;
        } else {
            if matches!(direction, Direction::Up | Direction::Down) {
                let mut other = None;
                let mut directed = false;
                for e in &edges {
                    if e.bond != edge.bond && at(&graph.bonds, e.bond)?.order == 1 {
                        if *at(&directions, e.bond)? == Direction::None {
                            other = Some(e.bond);
                        } else {
                            directed = true;
                        }
                    }
                }
                if !directed && let Some(other) = other {
                    let flip = at(&graph.bonds, other)?.a == parent && hbond.a == parent;
                    let dir = if flip {
                        if direction == Direction::Up {
                            Direction::Down
                        } else {
                            Direction::Up
                        }
                    } else {
                        direction
                    };
                    set(&mut directions, other, dir)?;
                }
            }
            if parent_degree != 2 {
                'bonds: for e in &edges {
                    let bond = at(&graph.bonds, e.bond)?;
                    let meta = metadata.bonds.get_mut(e.bond).ok_or(Error::Limit)?;
                    if bond.order != 2 || meta.stereo <= 1 {
                        continue;
                    }
                    if let Some(slot) = meta.stereo_atoms.iter().position(|&a| a == id) {
                        work.spend(edges.len())?;
                        if let Some(replacement) =
                            edges.iter().find(|a| a.other != e.other && a.other != id)
                        {
                            set(&mut meta.stereo_atoms, slot, replacement.other)?;
                            meta.stereo = match meta.stereo {
                                4 => 5,
                                5 => 4,
                                s => s,
                            };
                            break 'bonds;
                        }
                    }
                }
            }
        }
        // Removing the incident bond clears any remaining control that was not
        // replaced above. Relative cis/trans labels require those controls;
        // absolute E/Z labels are retained by the native graph operation.
        for e in &edges {
            let meta = metadata.bonds.get_mut(e.bond).ok_or(Error::Limit)?;
            if e.bond != edge.bond && meta.stereo_atoms.contains(&id) {
                if matches!(meta.stereo, 4 | 5) {
                    meta.stereo = 0;
                }
                meta.stereo_atoms.clear();
            }
        }
        set(&mut alive, edge.bond, false)?;
        set(
            &mut degree,
            parent,
            parent_degree.checked_sub(1).ok_or(Error::Limit)?,
        )?;
    }
    let kept_atoms = (0..n)
        .filter(|&a| removed.get(a) == Some(&false))
        .collect::<Vec<_>>();
    let kept_bonds = (0..m)
        .filter(|&b| alive.get(b) == Some(&true))
        .collect::<Vec<_>>();
    let mut atom_map = vec![None; n];
    let mut bond_map = vec![None; m];
    for (new, &old) in kept_atoms.iter().enumerate() {
        set(&mut atom_map, old, Some(new))?;
    }
    for (new, &old) in kept_bonds.iter().enumerate() {
        set(&mut bond_map, old, Some(new))?;
    }
    let mut new_graph = Graph {
        atoms: Vec::new(),
        bonds: Vec::new(),
    };
    let mut new_meta = Metadata::default();
    let mut new_unknown = Vec::new();
    let mut new_directions = Vec::new();
    for &id in &kept_atoms {
        let mut atom = at(&graph.atoms, id)?.clone();
        let mut meta = at(&metadata.atoms, id)?.clone();
        if finish && !atom.no_implicit && meta.chiral_tag != 0 && atom.explicit_hydrogens > 1 {
            atom.explicit_hydrogens = 0;
        }
        meta.ring_stereo = false;
        new_graph.atoms.push(atom);
        new_meta.atoms.push(meta);
        new_unknown.push(*at(&unknown, id)?);
    }
    for &id in &kept_bonds {
        let mut bond = at(&graph.bonds, id)?.clone();
        bond.a = at(&atom_map, bond.a)?.ok_or(Error::Limit)?;
        bond.b = at(&atom_map, bond.b)?.ok_or(Error::Limit)?;
        let mut meta = at(&metadata.bonds, id)?.clone();
        meta.stereo_atoms = meta
            .stereo_atoms
            .iter()
            .map(|&a| atom_map.get(a).copied().flatten())
            .collect::<Option<Vec<_>>>()
            .unwrap_or_default();
        new_graph.bonds.push(bond);
        new_meta.bonds.push(meta);
        new_directions.push(*at(&directions, id)?);
    }
    for group in &metadata.groups {
        let mut group = group.clone();
        group.atoms = group
            .atoms
            .iter()
            .filter_map(|&a| atom_map.get(a).copied().flatten())
            .collect();
        group.bonds = group
            .bonds
            .iter()
            .filter_map(|&b| bond_map.get(b).copied().flatten())
            .collect();
        if !group.atoms.is_empty() || !group.bonds.is_empty() {
            new_meta.groups.push(group);
        }
    }
    Ok(Removal {
        graph: new_graph,
        metadata: new_meta,
        directions: new_directions,
        unknown_atoms: new_unknown,
        kept_atoms,
        kept_bonds,
    })
}
