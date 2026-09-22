//! Atom, bond, and stereochemical comparisons from RDKit new_canon.cpp/.h.
//! Copyright (C) 2014 Greg Landrum. BSD-3-Clause; licenses/rdkit/.
use super::{Comparison, Edge, Ranker, at, put};
use std::{
    cmp::Ordering,
    collections::{BTreeSet, HashMap, HashSet},
};

macro_rules! compare_field {
    ($a:expr, $b:expr) => {{
        let a = $a;
        let b = $b;
        let order = a.cmp(&b);
        if order != Ordering::Equal {
            return Ok(order);
        }
    }};
}
impl Ranker<'_> {
    fn control_rank(&self, atom: usize) -> Result<i32, String> {
        if self.initializing.is_some_and(|n| atom >= n) {
            return Ok(-1);
        }
        i32::try_from(*at(&self.classes, atom)?)
            .map_err(|_| "Canonical control rank overflow".into())
    }
    fn normalized_stereo(&self, edge: &Edge) -> Result<u8, String> {
        if !matches!(edge.stereo, 4 | 5) {
            return Ok(edge.stereo);
        }
        let mut flip = false;
        for side in [0, 2] {
            let primary = at(&edge.controls, side)?.ok_or("Missing stereo control atom")?;
            if let Some(other) = *at(&edge.controls, side + 1)? {
                flip ^= self.control_rank(other)? > self.control_rank(primary)?;
            }
        }
        Ok(if flip {
            if edge.stereo == 4 { 5 } else { 4 }
        } else {
            edge.stereo
        })
    }
    fn bond_compare(&mut self, a: &Edge, b: &Edge) -> Result<Ordering, String> {
        self.work.spend(1)?;
        compare_field!(a.kind, b.kind);
        compare_field!(a.stereo, b.stereo);
        compare_field!(a.rank, b.rank);
        if a.stereo != 0 && b.stereo != 0 {
            compare_field!(self.normalized_stereo(a)?, self.normalized_stereo(b)?);
        }
        Ok(Ordering::Equal)
    }
    pub(super) fn sort_bonds(&mut self, atom: usize, update: bool) -> Result<(), String> {
        let mut edges = std::mem::take(
            self.bonds
                .get_mut(atom)
                .ok_or("Missing ranking neighbors")?,
        );
        self.work.spend(edges.len())?;
        if update {
            for edge in &mut edges {
                edge.rank = self.control_rank(edge.other)?;
            }
        }
        for i in 1..edges.len() {
            let value = *at(&edges, i)?;
            let mut j = i;
            while j > 0 && self.bond_compare(&value, at(&edges, j - 1)?)? == Ordering::Greater {
                let previous = *at(&edges, j - 1)?;
                put(&mut edges, j, previous)?;
                j -= 1;
            }
            put(&mut edges, j, value)?;
        }
        put(&mut self.bonds, atom, edges)
    }
    fn chiral_rank(&mut self, atom: usize) -> Result<u8, String> {
        let neighbors = at(&self.neighbors, atom)?;
        self.work.spend(neighbors.len())?;
        let mut seen = HashSet::new();
        let mut ranks = Vec::new();
        for &other in neighbors {
            let rank = *at(&self.classes, other)?;
            if !seen.insert(rank) {
                return Ok(0);
            }
            ranks.push(rank);
        }
        let tag = at(&self.metadata.atoms, atom)?.chiral_tag;
        if !matches!(tag, 1 | 2) {
            return Ok(0);
        }
        let mut odd = false;
        for (i, &rank) in ranks.iter().enumerate() {
            self.work.spend(ranks.len() - i - 1)?;
            for &next in ranks.iter().skip(i + 1) {
                odd ^= rank > next;
            }
        }
        let value = if tag == 1 { 2 } else { 1 };
        Ok(if odd { 3 - value } else { value })
    }
    fn ring_code(&mut self, atom: usize) -> Result<u32, String> {
        if !*at(&self.has_ring_neighbor, atom)? {
            return Ok(0);
        }
        let neighbors = at(&self.neighbors, atom)?;
        self.work.spend(neighbors.len())?;
        let mut code = 0u32;
        for &other in neighbors {
            if *at(&self.ring_stereo, other)? {
                // The native invariant is an unsigned 32-bit accumulation.
                code = code.wrapping_add(
                    (*at(&self.classes, other)? as u32)
                        .wrapping_mul(10_000)
                        .wrapping_add(1),
                );
            }
        }
        Ok(code)
    }
    fn base_compare(&mut self, i: usize, j: usize) -> Result<Ordering, String> {
        self.work.spend(1)?;
        compare_field!(*at(&self.classes, i)?, *at(&self.classes, j)?);
        let (a, b) = (at(&self.graph.atoms, i)?, at(&self.graph.atoms, j)?);
        let (ma, mb) = (at(&self.metadata.atoms, i)?, at(&self.metadata.atoms, j)?);
        if self.options.use_non_stereo_ranks && !self.options.fragment {
            compare_field!(ma.non_stereo_rank, mb.non_stereo_rank);
        }
        let mapa = if self.options.include_maps || a.atomic_number == 0 {
            ma.map_number
        } else {
            0
        };
        let mapb = if self.options.include_maps || b.atomic_number == 0 {
            mb.map_number
        } else {
            0
        };
        compare_field!(mapa, mapb);
        compare_field!(at(&self.neighbors, i)?.len(), at(&self.neighbors, j)?.len());
        compare_field!(a.atomic_number, b.atomic_number);
        if self.options.include_isotopes {
            compare_field!(a.isotope, b.isotope);
        }
        compare_field!(*at(&self.hydrogens, i)?, *at(&self.hydrogens, j)?);
        // RDKit compares formal charges through unsigned integer temporaries.
        compare_field!(i32::from(a.charge) as u32, i32::from(b.charge) as u32);
        if self.options.include_chiral_presence {
            compare_field!(ma.chiral_tag != 0, mb.chiral_tag != 0);
        }
        if self.options.include_chirality {
            let (ga, gb) = (*at(&self.groups, i)?, *at(&self.groups, j)?);
            match (ga, gb) {
                (Some(_), None) => return Ok(Ordering::Greater),
                (None, Some(_)) => return Ok(Ordering::Less),
                (Some(ga), Some(gb)) => {
                    let (a, b) = (
                        at(&self.metadata.groups, ga)?,
                        at(&self.metadata.groups, gb)?,
                    );
                    compare_field!(a.kind, b.kind);
                    if ga != gb {
                        self.work.spend(a.atoms.len() + b.atoms.len())?;
                        let ca = a
                            .atoms
                            .iter()
                            .map(|&a| at(&self.classes, a).copied())
                            .collect::<Result<BTreeSet<_>, _>>()?;
                        let cb = b
                            .atoms
                            .iter()
                            .map(|&b| at(&self.classes, b).copied())
                            .collect::<Result<BTreeSet<_>, _>>()?;
                        compare_field!(ca, cb);
                    } else if a.kind == 0 {
                        compare_field!(self.chiral_rank(i)?, self.chiral_rank(j)?);
                    }
                }
                (None, None) => {
                    compare_field!(ma.chiral_tag != 0, mb.chiral_tag != 0);
                    if ma.chiral_tag != 0 {
                        compare_field!(self.chiral_rank(i)?, self.chiral_rank(j)?);
                    }
                }
            }
        }
        if self.options.include_chirality
            && (self.options.include_ring_stereo || self.options.fragment)
        {
            compare_field!(self.ring_code(i)?, self.ring_code(j)?);
        }
        Ok(Ordering::Equal)
    }
    fn neighbor_swaps(&mut self, atom: usize) -> Result<Vec<(i32, u8)>, String> {
        let mut result = Vec::new();
        let in_ring = *at(&self.ring_counts, atom)? != 0;
        let bonds = at(&self.bonds, atom)?.clone();
        self.work.spend(bonds.len())?;
        for edge in bonds {
            let tag = at(&self.metadata.atoms, edge.other)?.chiral_tag;
            if !in_ring || tag == 0 {
                result.push((edge.rank, 0));
                continue;
            }
            let ids = at(&self.neighbors, edge.other)?;
            self.work.spend(ids.len())?;
            let mut seen = HashSet::new();
            let mut duplicate = false;
            for &other in ids {
                if other != atom && !seen.insert(*at(&self.classes, other)?) {
                    duplicate = true;
                }
            }
            if duplicate {
                result.push((edge.rank, 0));
                continue;
            }
            let positions = ids
                .iter()
                .enumerate()
                .map(|(i, &a)| (a, i))
                .collect::<HashMap<_, _>>();
            let mut probe = vec![*positions.get(&atom).ok_or("Missing chiral neighbor")?];
            for bond in at(&self.bonds, edge.other)? {
                if bond.other != atom {
                    probe.push(
                        *positions
                            .get(&bond.other)
                            .ok_or("Missing chiral permutation atom")?,
                    );
                }
            }
            let mut odd = false;
            for (i, &p) in probe.iter().enumerate() {
                self.work.spend(probe.len() - i - 1)?;
                for &q in probe.iter().skip(i + 1) {
                    odd ^= p > q;
                }
            }
            if matches!(tag, 1 | 2) {
                result.push((edge.rank, if (tag == 1) == odd { 2 } else { 1 }));
            }
        }
        result.sort_unstable();
        Ok(result)
    }
    pub(super) fn compare(
        &mut self,
        i: usize,
        j: usize,
        comparison: Comparison,
    ) -> Result<Ordering, String> {
        self.work.spend(1)?;
        if matches!(comparison, Comparison::Normal) {
            let result = self.base_compare(i, j)?;
            if result != Ordering::Equal {
                return Ok(result);
            }
        } else if matches!(comparison, Comparison::Symmetry) {
            self.work.spend(
                at(&self.neighbor_numbers, i)?.len()
                    + at(&self.neighbor_numbers, j)?.len()
                    + at(&self.revisited, i)?.len()
                    + at(&self.revisited, j)?.len(),
            )?;
            compare_field!(
                at(&self.neighbor_numbers, i)?,
                at(&self.neighbor_numbers, j)?
            );
            compare_field!(at(&self.revisited, i)?, at(&self.revisited, j)?);
        }
        self.sort_bonds(i, true)?;
        self.sort_bonds(j, true)?;
        let (ni, nj) = (at(&self.bonds, i)?.len(), at(&self.bonds, j)?.len());
        for position in 0..ni.min(nj) {
            let (a, b) = (
                *at(at(&self.bonds, i)?, position)?,
                *at(at(&self.bonds, j)?, position)?,
            );
            let result = self.bond_compare(&a, &b)?;
            if result != Ordering::Equal {
                return Ok(result);
            }
        }
        if matches!(comparison, Comparison::Chirality) {
            let a = self.neighbor_swaps(i)?;
            let b = self.neighbor_swaps(j)?;
            for ((_, a), (_, b)) in a.iter().zip(&b) {
                compare_field!(a, b);
            }
        } else {
            compare_field!(ni, nj);
        }
        Ok(Ordering::Equal)
    }
}
