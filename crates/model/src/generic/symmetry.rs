//! Exact, bounded chemical equivalence for completed generic products.
//!
//! This consumes the preparation already required by expansion. Coordinates,
//! drawing IDs, presentation and computed CIP labels never enter the identity.
//! A fingerprint only rejects candidates; an injective graph mapping, including
//! structural stereo, must succeed before a later product can be discarded.
use super::Work;
use crate::chemistry::document::Molecule;
use std::{
    collections::{HashMap, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
    mem::size_of,
};

#[derive(Default)]
pub(super) struct Deduplicator {
    representatives: Vec<Identity>,
}

impl Deduplicator {
    /// Keep the first representative in materialization order. All storage and
    /// search use the caller's remaining expansion allowance; there is no new
    /// per-product budget and no unbounded canonical/automorphism search.
    pub(super) fn insert(&mut self, molecule: &Molecule, work: &mut Work) -> Result<bool, String> {
        let candidate = Identity::new(molecule, work)?;
        for previous in &self.representatives {
            work.charge(1)?;
            if candidate.fingerprint == previous.fingerprint
                && candidate.equivalent(previous, work)?
            {
                return Ok(false);
            }
        }
        // Vec growth can allocate twice the occupied handles and copy the old
        // storage. Debit growth before pushing, without making normal appends
        // quadratic in the number of unique products.
        if self.representatives.len() == self.representatives.capacity() {
            let new_capacity = self.representatives.len().max(2).saturating_mul(2);
            work.charge(
                new_capacity
                    .saturating_add(self.representatives.len())
                    .saturating_mul(size_of::<Identity>()),
            )?;
            self.representatives
                .try_reserve_exact(new_capacity - self.representatives.len())
                .map_err(|_| "Generic expansion identity storage is unavailable".to_owned())?;
        } else {
            work.charge(size_of::<Identity>())?;
        }
        self.representatives.push(candidate);
        Ok(true)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct AtomKey {
    number: u8,
    isotope: u16,
    charge: i8,
    radical: u8,
    aromatic: bool,
    hydrogens: u32,
    map: i32,
    map_present: bool,
    chiral: bool,
    unknown: bool,
    // Aromatic bonds use 4; dative and hydrogen interactions include direction.
    neighborhood: [u32; 10],
}

#[derive(Clone, Copy)]
struct Link {
    atom: usize,
    bond: usize,
}

#[derive(Clone, Copy)]
struct Edge {
    a: usize,
    b: usize,
    order: u8,
    aromatic: bool,
    stereo: u8,
    controls: Option<[usize; 2]>,
    unknown: bool,
}

impl Edge {
    fn kind_at(self, atom: usize) -> usize {
        match self.order {
            5 if self.b == atom => 8,
            0 if self.b == atom => 9,
            order => usize::from(order),
        }
    }

    fn stereo_class(self) -> u8 {
        self.stereo.min(2)
    }

    fn same_side(self) -> bool {
        matches!(self.stereo, 2 | 4)
    }
}

#[derive(Clone, Copy)]
struct SearchAtom {
    atom: usize,
    parent: Option<usize>,
}

struct Identity {
    atoms: Vec<AtomKey>,
    winding: Vec<u8>,
    edges: Vec<Edge>,
    // Preserve preparation's bond insertion order for tetrahedral parity.
    adjacent: Vec<Vec<Link>>,
    search: Vec<SearchAtom>,
    fingerprint: (usize, usize, u64, u64),
}

fn invalid() -> String {
    "Expanded structure has unsupported chemical identity metadata".into()
}

fn at<T>(items: &[T], index: usize) -> Result<&T, String> {
    items.get(index).ok_or_else(invalid)
}

fn at_mut<T>(items: &mut [T], index: usize) -> Result<&mut T, String> {
    items.get_mut(index).ok_or_else(invalid)
}

impl Identity {
    fn new(molecule: &Molecule, work: &mut Work) -> Result<Self, String> {
        let state = &molecule.state;
        let n = state.graph.atoms.len();
        let m = state.graph.bonds.len();
        // Reserve before any indexing buffers, adjacency storage, hash table or
        // owned identity is allocated. The allowance includes temporary roots,
        // degrees, visited flags and the iterative spanning-forest stack.
        let atom_storage = size_of::<AtomKey>()
            .saturating_mul(3)
            .saturating_add(size_of::<Vec<Link>>())
            .saturating_add(size_of::<SearchAtom>())
            .saturating_add(size_of::<usize>().saturating_mul(8));
        let edge_storage = size_of::<Edge>()
            .saturating_add(size_of::<Link>().saturating_mul(2))
            .saturating_add(16);
        work.charge(
            n.saturating_mul(atom_storage)
                .saturating_add(m.saturating_mul(edge_storage)),
        )?;
        if state.metadata.atoms.len() != n
            || state.metadata.bonds.len() != m
            || state.valences.len() != n
            || state.properties.atoms.len() != n
            || !state.metadata.groups.is_empty()
            || state.metadata.atoms.iter().any(|a| a.chiral_tag > 2)
            || state.metadata.bonds.iter().any(|b| b.stereo > 5)
        {
            // Document preparation currently excludes these classes. Never
            // silently use a plain graph if that contract grows in the future.
            return Err(invalid());
        }
        let mut degrees = vec![0usize; n];
        for bond in &state.graph.bonds {
            if bond.a >= n || bond.b >= n || bond.a == bond.b || bond.order > 7 {
                return Err(invalid());
            }
            *at_mut(&mut degrees, bond.a)? += 1;
            *at_mut(&mut degrees, bond.b)? += 1;
        }
        let mut adjacent: Vec<Vec<Link>> = degrees
            .iter()
            .map(|&degree| Vec::with_capacity(degree))
            .collect();
        let mut edges = Vec::with_capacity(m);
        for (id, (bond, stereo)) in state
            .graph
            .bonds
            .iter()
            .zip(&state.metadata.bonds)
            .enumerate()
        {
            let controls = match stereo.stereo_atoms.as_slice() {
                [] => None,
                &[a, b] if a < n && b < n => Some([a, b]),
                _ => return Err(invalid()),
            };
            edges.push(Edge {
                a: bond.a,
                b: bond.b,
                order: if bond.aromatic { 4 } else { bond.order },
                aromatic: bond.aromatic,
                stereo: stereo.stereo,
                controls,
                unknown: stereo.unknown_stereo,
            });
            at_mut(&mut adjacent, bond.a)?.push(Link {
                atom: bond.b,
                bond: id,
            });
            at_mut(&mut adjacent, bond.b)?.push(Link {
                atom: bond.a,
                bond: id,
            });
        }
        let mut atoms = Vec::with_capacity(n);
        let mut winding = Vec::with_capacity(n);
        for (index, atom) in state.graph.atoms.iter().enumerate() {
            let meta = at(&state.metadata.atoms, index)?;
            let mut neighborhood = [0u32; 10];
            for link in at(&adjacent, index)? {
                let kind = at(&edges, link.bond)?.kind_at(index);
                *at_mut(&mut neighborhood, kind)? += 1;
            }
            atoms.push(AtomKey {
                number: atom.atomic_number,
                isotope: atom.isotope,
                charge: atom.charge,
                radical: atom.radical_electrons,
                aromatic: atom.aromatic,
                hydrogens: u32::from(atom.explicit_hydrogens)
                    .checked_add(at(&state.valences, index)?.implicit_hydrogens)
                    .ok_or_else(invalid)?,
                map: meta.map_number,
                map_present: meta.map_present || meta.map_number != 0,
                chiral: meta.chiral_tag != 0,
                unknown: at(&state.properties.atoms, index)?.unknown,
                neighborhood,
            });
            winding.push(meta.chiral_tag);
        }
        // Commutative hashes are only a rejection filter. Collisions still go
        // through exact graph and stereo comparison below.
        let (mut sum, mut xor) = (0u64, 0u64);
        for atom in &atoms {
            let mut hash = DefaultHasher::new();
            atom.hash(&mut hash);
            let value = hash.finish();
            sum = sum.wrapping_add(value);
            xor ^= value.rotate_left(17);
        }
        let fingerprint = (n, m, sum, xor);
        let mut counts = HashMap::<AtomKey, usize>::with_capacity(n);
        for &atom in &atoms {
            *counts.entry(atom).or_default() += 1;
        }
        let mut roots: Vec<_> = (0..n).collect();
        work.charge(
            n.saturating_mul(n.max(1).ilog2() as usize + 1)
                .saturating_mul(16),
        )?;
        roots.sort_unstable_by_key(|&i| {
            // Roots and all keys were constructed together above. Wrapping the
            // checked key in Some preserves their ordering without panicking
            // inside this infallible sorting callback.
            let atom = atoms.get(i)?;
            Some((
                *counts.get(atom)?,
                std::cmp::Reverse(adjacent.get(i)?.len()),
                *atom,
                i,
            ))
        });
        let mut seen = vec![false; n];
        let mut search = Vec::with_capacity(n);
        let mut stack = Vec::with_capacity(n);
        for root in roots {
            if *at(&seen, root)? {
                continue;
            }
            *at_mut(&mut seen, root)? = true;
            stack.push(SearchAtom {
                atom: root,
                parent: None,
            });
            while let Some(current) = stack.pop() {
                search.push(current);
                for link in at(&adjacent, current.atom)?.iter().rev() {
                    if !*at(&seen, link.atom)? {
                        *at_mut(&mut seen, link.atom)? = true;
                        stack.push(SearchAtom {
                            atom: link.atom,
                            parent: Some(current.atom),
                        });
                    }
                }
            }
        }
        Ok(Self {
            atoms,
            winding,
            edges,
            adjacent,
            search,
            fingerprint,
        })
    }

    fn equivalent(&self, other: &Self, work: &mut Work) -> Result<bool, String> {
        let n = self.atoms.len();
        if n != other.atoms.len() || self.edges.len() != other.edges.len() {
            return Ok(false);
        }
        work.charge(n.saturating_mul(size_of::<usize>().saturating_mul(2) + 1))?;
        let mut mapping = vec![usize::MAX; n];
        let mut occupied = vec![false; n];
        let mut cursors = vec![0usize; n];
        let mut depth = 0usize;
        loop {
            work.charge(1)?;
            if depth == n {
                if self.stereo_matches(other, &mapping, work)? {
                    return Ok(true);
                }
            } else {
                let current = *at(&self.search, depth)?;
                let parent_neighbors = current
                    .parent
                    .map(|parent| at(&other.adjacent, *at(&mapping, parent)?))
                    .transpose()?;
                let candidates = parent_neighbors.map_or(n, Vec::len);
                let mut chosen = None;
                let cursor = at_mut(&mut cursors, depth)?;
                while *cursor < candidates {
                    work.charge(16)?;
                    let index = *cursor;
                    *cursor += 1;
                    let target = match parent_neighbors {
                        Some(links) => at(links, index)?.atom,
                        None => index,
                    };
                    if !*at(&occupied, target)?
                        && at(&self.atoms, current.atom)? == at(&other.atoms, target)?
                        && self.mapped_edges_match(current.atom, other, target, &mapping, work)?
                    {
                        chosen = Some(target);
                        break;
                    }
                }
                if let Some(target) = chosen {
                    *at_mut(&mut mapping, current.atom)? = target;
                    *at_mut(&mut occupied, target)? = true;
                    depth += 1;
                    if depth < n {
                        *at_mut(&mut cursors, depth)? = 0;
                    }
                    continue;
                }
            }
            if depth == 0 {
                return Ok(false);
            }
            depth -= 1;
            let atom = at(&self.search, depth)?.atom;
            *at_mut(&mut occupied, *at(&mapping, atom)?)? = false;
            *at_mut(&mut mapping, atom)? = usize::MAX;
        }
    }

    fn mapped_edges_match(
        &self,
        atom: usize,
        other: &Self,
        target: usize,
        mapping: &[usize],
        work: &mut Work,
    ) -> Result<bool, String> {
        let neighbors = at(&self.adjacent, atom)?;
        let target_neighbors = at(&other.adjacent, target)?;
        work.charge(neighbors.len())?;
        for link in neighbors {
            let mapped = *at(mapping, link.atom)?;
            if mapped == usize::MAX {
                continue;
            }
            work.charge(target_neighbors.len())?;
            let Some(found) = target_neighbors.iter().find(|link| link.atom == mapped) else {
                return Ok(false);
            };
            let a = *at(&self.edges, link.bond)?;
            let b = *at(&other.edges, found.bond)?;
            if a.order != b.order
                || a.aromatic != b.aromatic
                || a.unknown != b.unknown
                || a.stereo_class() != b.stereo_class()
                || a.kind_at(atom) != b.kind_at(target)
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn stereo_matches(
        &self,
        other: &Self,
        mapping: &[usize],
        work: &mut Work,
    ) -> Result<bool, String> {
        work.charge(self.atoms.len())?;
        for (atom, &tag) in self.winding.iter().enumerate() {
            if tag == 0 {
                continue;
            }
            let target = *at(mapping, atom)?;
            let neighbors = at(&self.adjacent, atom)?;
            let target_neighbors = at(&other.adjacent, target)?;
            let degree = neighbors.len();
            // Debit both neighbor-position lookup and inversion counting before
            // their temporary storage or quadratic scans. Prepared ordinary
            // tetrahedral atoms have at most four explicit neighbors.
            work.charge(
                degree
                    .saturating_mul(size_of::<usize>())
                    .saturating_add(degree.saturating_mul(degree).saturating_mul(2)),
            )?;
            let mut permutation = Vec::with_capacity(degree);
            for link in neighbors {
                let mapped = *at(mapping, link.atom)?;
                let Some(position) = target_neighbors.iter().position(|item| item.atom == mapped)
                else {
                    return Ok(false);
                };
                permutation.push(position);
            }
            let mut odd = false;
            for (i, &position) in permutation.iter().enumerate() {
                for &next in permutation.iter().skip(i + 1) {
                    odd ^= position > next;
                }
            }
            if (tag != *at(&other.winding, target)?) != odd {
                return Ok(false);
            }
        }
        work.charge(self.edges.len())?;
        for &edge in &self.edges {
            if edge.stereo < 2 {
                continue;
            }
            let target = *at(mapping, edge.a)?;
            let mapped_end = *at(mapping, edge.b)?;
            let target_neighbors = at(&other.adjacent, target)?;
            work.charge(target_neighbors.len())?;
            let Some(link) = target_neighbors.iter().find(|link| link.atom == mapped_end) else {
                return Ok(false);
            };
            let found = *at(&other.edges, link.bond)?;
            let (Some([left, right]), Some(b)) = (edge.controls, found.controls) else {
                if edge.stereo != found.stereo
                    || edge.controls.is_some() != found.controls.is_some()
                {
                    return Ok(false);
                }
                continue;
            };
            let [target_left, target_right] = if found.a == target {
                b
            } else {
                let [left, right] = b;
                [right, left]
            };
            let a = [*at(mapping, left)?, *at(mapping, right)?];
            let [mapped_left, mapped_right] = a;
            if self.ordinary_alkene(edge, work)? && other.ordinary_alkene(found, work)? {
                if (edge.same_side()
                    ^ (mapped_left != target_left)
                    ^ (mapped_right != target_right))
                    != found.same_side()
                {
                    return Ok(false);
                }
            } else if edge.stereo != found.stereo || a != [target_left, target_right] {
                // Preparation retains some nonstandard supplied stereo tags.
                // Never assume arbitrary alternate controls lie opposite.
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn ordinary_alkene(&self, edge: Edge, work: &mut Work) -> Result<bool, String> {
        if edge.order != 2 {
            return Ok(false);
        }
        let Some([left, right]) = edge.controls else {
            return Ok(false);
        };
        for (endpoint, partner, control) in [(edge.a, edge.b, left), (edge.b, edge.a, right)] {
            let neighbors = at(&self.adjacent, endpoint)?;
            work.charge(neighbors.len())?;
            let mut count = 0usize;
            let mut found = false;
            for link in neighbors {
                if link.atom != partner {
                    count += 1;
                    found |= link.atom == control;
                }
            }
            if !found || count > 2 {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests;
