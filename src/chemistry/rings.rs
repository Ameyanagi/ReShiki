//! Symmetric SSSR ring perception adapted from RDKit FindRings.cpp (2026.03.6).
//! Copyright (C) 2003-2021 Greg Landrum and other RDKit contributors.
//! BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
//!
//! Preserve pruning, duplicate recovery and symmetry rules; replace recursive
//! walks and unchecked indexing with bounded, fallible operations.
mod ordering;
mod search;
#[cfg(test)]
mod tests;
use super::graph::Graph;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
type Ring = Vec<usize>;

#[derive(Debug)]
pub enum RingError {
    /// The legacy greedy pruning depends on unspecified C++ sort tie ordering,
    /// or proving its independence exceeded the bounded verification budget.
    /// Keep the backend's ring result until this case has a portable replacement.
    UnresolvedOrdering,
    Failed(String),
}
impl From<String> for RingError {
    fn from(message: String) -> Self {
        Self::Failed(message)
    }
}
impl std::fmt::Display for RingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnresolvedOrdering => {
                f.write_str("Ring pruning has unresolved equal-size ordering")
            }
            Self::Failed(message) => f.write_str(message),
        }
    }
}
impl std::error::Error for RingError {}

#[derive(Clone, Copy, Default)]
pub struct Options {
    pub include_dative: bool,
    pub include_hydrogen: bool,
}

#[derive(Debug)]
pub struct Rings {
    pub atoms: Vec<Ring>,
    pub bonds: Vec<Ring>,
    pub basis_count: usize,
    /// RDKit uses a depth-first approximation when its SSSR search is incomplete.
    pub approximate: bool,
}

fn at<T>(items: &[T], index: usize) -> Result<&T, String> {
    items
        .get(index)
        .ok_or_else(|| "Invalid ring topology index".into())
}
fn set<T>(items: &mut [T], index: usize, value: T) -> Result<(), String> {
    *items.get_mut(index).ok_or("Invalid ring topology index")? = value;
    Ok(())
}
fn invariant(ring: &[usize]) -> Vec<usize> {
    // Descending atom IDs have the same ordering as RDKit's dynamic bitsets.
    let mut result = ring.to_vec();
    result.sort_unstable_by(|a, b| b.cmp(a));
    result
}

struct Budget {
    work: usize,
    stored: usize,
}
impl Default for Budget {
    fn default() -> Self {
        Self {
            work: 50_000_000,
            stored: 2_000_000,
        }
    }
}
impl Budget {
    fn spend(&mut self, n: usize) -> Result<(), String> {
        self.work = self
            .work
            .checked_sub(n)
            .ok_or("Ring search work limit exceeded")?;
        Ok(())
    }
    fn store(&mut self, n: usize) -> Result<(), String> {
        self.stored = self
            .stored
            .checked_sub(n)
            .ok_or("Ring storage limit exceeded")?;
        Ok(())
    }
}

struct Topology {
    adjacency: Vec<Vec<(usize, usize)>>,
    edges: Vec<(usize, usize)>,
    by_pair: HashMap<(usize, usize), usize>,
}
impl Topology {
    fn new(graph: &Graph) -> Result<Self, String> {
        graph.validate()?;
        let mut adjacency = vec![Vec::new(); graph.atoms.len()];
        let mut edges = Vec::with_capacity(graph.bonds.len());
        let mut by_pair = HashMap::new();
        for (index, b) in graph.bonds.iter().enumerate() {
            adjacency
                .get_mut(b.a)
                .ok_or("Missing ring endpoint")?
                .push((b.b, index));
            adjacency
                .get_mut(b.b)
                .ok_or("Missing ring endpoint")?
                .push((b.a, index));
            edges.push((b.a, b.b));
            by_pair.insert((b.a.min(b.b), b.a.max(b.b)), index);
        }
        Ok(Self {
            adjacency,
            edges,
            by_pair,
        })
    }
    fn neighbors(&self, id: usize) -> Result<&[(usize, usize)], String> {
        Ok(at(&self.adjacency, id)?.as_slice())
    }
    fn bonds(&self, ring: &[usize]) -> Result<Ring, String> {
        if ring.len() < 3 {
            return Err("Invalid ring size".into());
        }
        ring.iter()
            .zip(ring.iter().cycle().skip(1))
            .map(|(&a, &b)| {
                self.by_pair
                    .get(&(a.min(b), a.max(b)))
                    .copied()
                    .ok_or_else(|| "Missing ring bond".into())
            })
            .collect()
    }
    fn fragments(&self) -> Result<Vec<Vec<usize>>, String> {
        let mut seen = vec![false; self.adjacency.len()];
        let mut result = Vec::new();
        for id in 0..seen.len() {
            if *at(&seen, id)? {
                continue;
            }
            set(&mut seen, id, true)?;
            let mut pending = vec![id];
            let mut fragment = Vec::new();
            while let Some(id) = pending.pop() {
                fragment.push(id);
                for &(neighbor, _) in self.neighbors(id)? {
                    if !*at(&seen, neighbor)? {
                        set(&mut seen, neighbor, true)?;
                        pending.push(neighbor);
                    }
                }
            }
            fragment.sort_unstable();
            result.push(fragment);
        }
        Ok(result)
    }
}

struct State {
    active: Vec<bool>,
    degrees: Vec<usize>,
    ring_atoms: Vec<bool>,
    ring_bonds: Vec<bool>,
    seen: BTreeSet<Vec<usize>>,
    extras: Vec<Ring>,
    budget: Budget,
    ordering_resolved: bool,
}
impl State {
    fn trim(
        &mut self,
        top: &Topology,
        candidate: usize,
        changed: &mut VecDeque<usize>,
    ) -> Result<(), String> {
        for &(other, bond) in top.neighbors(candidate)? {
            if !*at(&self.active, bond)? {
                continue;
            }
            let degree = *at(&self.degrees, other)?;
            if degree <= 2 {
                changed.push_back(other);
            }
            set(&mut self.active, bond, false)?;
            set(
                &mut self.degrees,
                other,
                degree.checked_sub(1).ok_or("Invalid ring degree")?,
            )?;
            let degree = *at(&self.degrees, candidate)?;
            set(
                &mut self.degrees,
                candidate,
                degree.checked_sub(1).ok_or("Invalid ring degree")?,
            )?;
        }
        Ok(())
    }
    fn record(&mut self, top: &Topology, ring: &[usize]) -> Result<(), String> {
        for &id in ring {
            set(&mut self.ring_atoms, id, true)?;
        }
        for id in top.bonds(ring)? {
            set(&mut self.ring_bonds, id, true)?;
        }
        Ok(())
    }
    fn add(&mut self, rings: &mut Vec<Ring>, ring: Ring) -> Result<bool, String> {
        if !self.seen.insert(invariant(&ring)) {
            return Ok(false);
        }
        self.budget.store(ring.len())?;
        rings.push(ring);
        Ok(true)
    }
    fn degree_two(&self, top: &Topology, fragment: &[usize]) -> Result<Vec<usize>, String> {
        let mut forbidden = HashSet::new();
        let mut result = Vec::new();
        for &root in fragment {
            if *at(&self.degrees, root)? != 2 || !forbidden.insert(root) {
                continue;
            }
            result.push(root);
            let mut pending = vec![root];
            while let Some(id) = pending.pop() {
                for &(other, bond) in top.neighbors(id)? {
                    if *at(&self.active, bond)?
                        && *at(&self.degrees, other)? == 2
                        && forbidden.insert(other)
                    {
                        pending.push(other);
                    }
                }
            }
        }
        Ok(result)
    }

    fn find_degree_two(
        &mut self,
        top: &Topology,
        rings: &mut Vec<Ring>,
        candidates: &[usize],
    ) -> Result<(), String> {
        let mut duplicate_candidates = BTreeMap::<Vec<usize>, Vec<usize>>::new();
        let mut duplicates = HashMap::<usize, Vec<usize>>::new();
        for &candidate in candidates {
            let small = search::smallest(top, candidate, &self.active, &[], &mut self.budget)?;
            let empty = small.is_empty();
            for ring in small {
                let key = invariant(&ring);
                let previous = duplicate_candidates.entry(key.clone()).or_default();
                if self.seen.contains(&key) {
                    for &other in previous.iter() {
                        duplicates.entry(candidate).or_default().push(other);
                        duplicates.entry(other).or_default().push(candidate);
                    }
                } else {
                    self.record(top, &ring)?;
                    self.add(rings, ring)?;
                }
                previous.push(candidate);
            }
            if empty {
                let mut changed = VecDeque::from([candidate]);
                while let Some(id) = changed.pop_front() {
                    self.trim(top, id, &mut changed)?;
                }
            }
        }
        for candidates in duplicate_candidates.values().filter(|v| v.len() > 1) {
            let mut found = Vec::new();
            let mut minimum = usize::MAX;
            for &candidate in candidates {
                self.budget.spend(self.active.len() + self.degrees.len())?;
                let active = self.active.clone();
                let degrees = self.degrees.clone();
                let mut changed = VecDeque::new();
                for &id in duplicates
                    .get(&candidate)
                    .ok_or("Missing duplicate ring candidate")?
                {
                    self.trim(top, id, &mut changed)?;
                }
                let small = search::smallest(top, candidate, &self.active, &[], &mut self.budget)?;
                self.active = active;
                self.degrees = degrees;
                for ring in small {
                    minimum = minimum.min(ring.len());
                    found.push(ring);
                }
            }
            for ring in found.into_iter().filter(|r| r.len() == minimum) {
                self.add(rings, ring)?;
            }
        }
        Ok(())
    }

    fn find_degree_three(
        &mut self,
        top: &Topology,
        rings: &mut Vec<Ring>,
        candidate: usize,
    ) -> Result<(), String> {
        let small = search::smallest(top, candidate, &self.active, &[], &mut self.budget)?;
        for ring in &small {
            self.add(rings, ring.clone())?;
        }
        if small.len() >= 3 {
            return Ok(());
        }
        let neighbors = top
            .neighbors(candidate)?
            .iter()
            .filter_map(|&(id, bond)| {
                self.active
                    .get(bond)
                    .copied()
                    .unwrap_or(false)
                    .then_some(id)
            })
            .take(3)
            .collect::<Vec<_>>();
        let [n1, n2, n3] = neighbors.as_slice() else {
            return Err("Missing degree-three ring neighbors".into());
        };
        let forbidden = match small.as_slice() {
            [first, second] => vec![
                *[n1, n2, n3]
                    .iter()
                    .find(|&&n| first.contains(n) && second.contains(n))
                    .ok_or("Missing shared ring neighbor")?,
            ],
            [ring] => {
                if !ring.contains(n1) {
                    vec![n3, n2]
                } else if !ring.contains(n2) {
                    vec![n3, n1]
                } else if !ring.contains(n3) {
                    vec![n2, n1]
                } else {
                    return Err("Missing alternative ring path".into());
                }
            }
            _ => Vec::new(),
        };
        for &id in forbidden {
            for ring in search::smallest(top, candidate, &self.active, &[id], &mut self.budget)? {
                self.add(rings, ring)?;
            }
        }
        Ok(())
    }

    fn remove_extra(&mut self, top: &Topology, rings: &mut Vec<Ring>) -> Result<(), String> {
        rings.sort_by_key(Vec::len);
        let bonds = rings
            .iter()
            .map(|r| top.bonds(r).map(|v| v.into_iter().collect::<HashSet<_>>()))
            .collect::<Result<Vec<_>, _>>()?;
        let mut available = vec![true; rings.len()];
        let mut keep = vec![false; rings.len()];
        let mut union = HashSet::new();
        for (i, ring) in bonds.iter().enumerate() {
            self.budget.spend(ring.len())?;
            if ring.is_subset(&union) {
                set(&mut available, i, false)?;
            }
            if !*at(&available, i)? {
                continue;
            }
            union.extend(ring);
            set(&mut keep, i, true)?;
            let mut consider = BTreeSet::new();
            for (j, other) in bonds.iter().enumerate().skip(i + 1) {
                self.budget.spend(1)?;
                if *at(&available, j)? && other.len() == ring.len() {
                    consider.insert(j);
                }
            }
            while !consider.is_empty() {
                let mut best = None;
                for &j in &consider {
                    let other = at(&bonds, j)?;
                    self.budget.spend(other.len())?;
                    let overlap = other.intersection(&union).count();
                    if best.is_none_or(|(_, n)| overlap > n) {
                        best = Some((j, overlap));
                    }
                }
                let (j, _) = best.ok_or("Missing overlapping ring")?;
                consider.remove(&j);
                let other = at(&bonds, j)?;
                if !other.is_subset(&union) {
                    set(&mut keep, j, true)?;
                    union.extend(other);
                }
                set(&mut available, j, false)?;
            }
        }
        let original = std::mem::take(rings);
        self.ordering_resolved &= ordering::independent(&bonds, &keep, &mut self.budget);
        for (ring, keep) in original.into_iter().zip(keep) {
            if keep {
                rings.push(ring);
            } else {
                self.extras.push(ring);
            }
        }
        Ok(())
    }

    fn basis(&mut self, top: &Topology) -> Result<(Vec<Ring>, bool), String> {
        let mut result = Vec::new();
        for fragment in top.fragments()? {
            if fragment.len() < 3 {
                continue;
            }
            let mut changed = VecDeque::new();
            let (mut all_degrees, mut active_degrees) = (0usize, 0usize);
            for &id in &fragment {
                all_degrees += top.neighbors(id)?.len();
                let degree = *at(&self.degrees, id)?;
                active_degrees += degree;
                if degree < 2 {
                    changed.push_back(id);
                }
            }
            if all_degrees / 2 < fragment.len() {
                continue;
            }
            let bond_count = active_degrees / 2;
            let expected = bond_count as isize - fragment.len() as isize + 1;
            let mut done = HashSet::new();
            let mut done_count = 0usize;
            let mut rings = Vec::new();
            while done_count <= fragment.len() - 3 {
                self.budget.spend(fragment.len())?;
                while let Some(id) = changed.pop_front() {
                    if done.insert(id) {
                        done_count += 1;
                        self.trim(top, id, &mut changed)?;
                    }
                }
                let candidates = self.degree_two(top, &fragment)?;
                if !candidates.is_empty() {
                    self.find_degree_two(top, &mut rings, &candidates)?;
                    for id in candidates {
                        done.insert(id);
                        done_count += 1;
                        self.trim(top, id, &mut changed)?;
                    }
                } else if done_count <= fragment.len() - 3 {
                    let candidate = fragment
                        .iter()
                        .copied()
                        .find(|&id| self.degrees.get(id) == Some(&3));
                    let Some(id) = candidate else {
                        break;
                    };
                    self.find_degree_three(top, &mut rings, id)?;
                    done.insert(id);
                    done_count += 1;
                    self.trim(top, id, &mut changed)?;
                }
            }
            if (rings.len() as isize) < expected {
                let mut dead = HashSet::new();
                loop {
                    // Match RDKit's fragment recovery scan, including its bound
                    // on the global bond list. Fast fallback follows below.
                    let mut candidate = None;
                    for (i, &(a, b)) in top.edges.iter().enumerate().take(bond_count) {
                        if !*at(&self.ring_bonds, i)?
                            && !dead.contains(&i)
                            && *at(&self.ring_atoms, a)?
                            && *at(&self.ring_atoms, b)?
                        {
                            candidate = Some((i, a, b));
                            break;
                        }
                    }
                    let Some((i, a, b)) = candidate else {
                        break;
                    };
                    if let Some(ring) = search::connection(
                        top,
                        a,
                        b,
                        &self.ring_atoms,
                        &self.seen,
                        &mut self.budget,
                    )? {
                        self.record(top, &ring)?;
                        self.add(&mut rings, ring)?;
                    } else {
                        dead.insert(i);
                    }
                }
                if (rings.len() as isize) < expected {
                    return Ok((search::fast(top, &mut self.budget)?, true));
                }
            }
            if (rings.len() as isize) > expected {
                self.remove_extra(top, &mut rings)?;
            }
            result.extend(rings);
        }
        Ok((result, false))
    }

    fn symmetric(
        &mut self,
        top: &Topology,
        mut rings: Vec<Ring>,
        approximate: bool,
    ) -> Result<Rings, String> {
        let basis_count = rings.len();
        let basis_bonds = rings
            .iter()
            .map(|r| top.bonds(r))
            .collect::<Result<Vec<_>, _>>()?;
        let mut counts = vec![0usize; top.edges.len()];
        for ring in &basis_bonds {
            for &bond in ring {
                *counts.get_mut(bond).ok_or("Missing ring bond")? += 1;
            }
        }
        for extra in &self.extras {
            let extra_bonds = top.bonds(extra)?.into_iter().collect::<HashSet<_>>();
            for ring in &basis_bonds {
                if ring.len() != extra_bonds.len() {
                    continue;
                }
                self.budget.spend(ring.len())?;
                let (mut shared, mut replaced) = (false, true);
                for &bond in ring {
                    let count = *at(&counts, bond)?;
                    if count == 1 || !shared {
                        if extra_bonds.contains(&bond) {
                            shared = true;
                        } else if count == 1 {
                            replaced = false;
                        }
                    }
                }
                if shared && replaced {
                    rings.push(extra.clone());
                    break;
                }
            }
        }
        let bonds = rings
            .iter()
            .map(|r| top.bonds(r))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Rings {
            atoms: rings,
            bonds,
            basis_count,
            approximate,
        })
    }
}

/// Depth-first cycle basis used when canonical ranking needs ring membership
/// before full ring perception. Includes every bond type, including dative and
/// hydrogen bonds. These cycles are not necessarily smallest or symmetric.
pub fn fast(graph: &Graph) -> Result<Rings, String> {
    let top = Topology::new(graph)?;
    let atoms = search::fast(&top, &mut Budget::default())?;
    let bonds = atoms
        .iter()
        .map(|ring| top.bonds(ring))
        .collect::<Result<_, _>>()?;
    Ok(Rings {
        basis_count: atoms.len(),
        atoms,
        bonds,
        approximate: true,
    })
}

pub fn perceive(graph: &Graph, options: Options) -> Result<Rings, RingError> {
    let top = Topology::new(graph)?;
    let active = graph
        .bonds
        .iter()
        .map(|b| {
            (b.order != 5 || options.include_dative) && (b.order != 0 || options.include_hydrogen)
        })
        .collect::<Vec<_>>();
    let degrees = top
        .adjacency
        .iter()
        .map(|neighbors| {
            neighbors
                .iter()
                .filter(|&&(_, bond)| active.get(bond) == Some(&true))
                .count()
        })
        .collect();
    let mut state = State {
        active,
        degrees,
        ring_atoms: vec![false; graph.atoms.len()],
        ring_bonds: vec![false; graph.bonds.len()],
        seen: BTreeSet::new(),
        extras: Vec::new(),
        budget: Budget::default(),
        ordering_resolved: true,
    };
    let (basis, approximate) = state.basis(&top)?;
    if !state.ordering_resolved {
        return Err(RingError::UnresolvedOrdering);
    }
    Ok(state.symmetric(&top, basis, approximate)?)
}
