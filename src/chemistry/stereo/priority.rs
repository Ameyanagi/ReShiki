//! Legacy CIP atom priorities from RDKit Chirality.cpp (2026.03.6).
//! Copyright (C) 2004-2024 Greg Landrum and other RDKit contributors.
//! BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
//! These are the reference's approximate priorities, not full CIP labels.
use crate::chemistry::{
    ELEMENTS,
    graph::{Graph, Valence},
    ranking::Metadata,
};
use std::{cmp::Ordering, ops::Range};

#[cfg(test)]
mod tests;

struct Work(usize);
impl Work {
    fn spend(&mut self, amount: usize) -> Result<(), String> {
        self.0 = self
            .0
            .checked_sub(amount)
            .ok_or("CIP ranking work limit exceeded")?;
        Ok(())
    }
}
fn at<T>(values: &[T], index: usize) -> Result<&T, String> {
    values
        .get(index)
        .ok_or_else(|| "Invalid CIP ranking index".into())
}

/// Rank substituents using the pinned legacy stereo-perception algorithm.
/// Existing atom winding is ignored. This does not assign R/S or E/Z labels.
pub fn atom_priorities(graph: &Graph, metadata: &Metadata) -> Result<Vec<u32>, String> {
    atom_priorities_cached(graph, metadata, None)
}

pub(crate) fn atom_priorities_cached(
    graph: &Graph,
    metadata: &Metadata,
    cached: Option<&[Valence]>,
) -> Result<Vec<u32>, String> {
    priorities(graph, metadata, cached, &mut Work(50_000_000))
}

fn priorities(
    graph: &Graph,
    metadata: &Metadata,
    cached: Option<&[Valence]>,
    work: &mut Work,
) -> Result<Vec<u32>, String> {
    let cache = graph.cached_valences(cached)?;
    metadata.validate(graph)?;
    let n = graph.atoms.len();
    if n == 0 {
        return Ok(Vec::new());
    }
    let mut entries = Vec::with_capacity(n);
    for (atom, meta) in graph.atoms.iter().zip(&metadata.atoms) {
        work.spend(1)?;
        let mut mass = 0;
        if atom.isotope != 0 {
            mass = i32::from(atom.isotope)
                - i32::from(at(ELEMENTS, usize::from(atom.atomic_number))?.common_isotope);
            if mass >= 0 {
                mass += 1;
            }
        }
        mass = (mass + 512).max(0) % 1024;
        let map = if meta.map_present || meta.map_number != 0 {
            meta.map_number
        } else {
            -1
        };
        // More-negative values convert an unsigned long through an out-of-range
        // double-to-int cast in the native code. Atom maps cannot be negative;
        // retain only the reference's -1 sentinel. Widen map + 1 so INT_MAX
        // follows the modulo rule without signed overflow.
        if map < -1 {
            return Err("Atom map is outside the defined CIP ranking range".into());
        }
        let map_part = ((i64::from(map) + 1) % 1024) as i32;
        let invariant = ((i32::from(atom.atomic_number) << 10 | mass) << 10) | map_part;
        entries.push(vec![invariant]);
    }
    let mut order: Vec<_> = (0..n).collect();
    sort(&entries, &mut order, work)?;
    let mut ranks = vec![0; n];
    let (mut classes, mut tied) = segments(&entries, &order, &mut ranks, work)?;
    for ((entry, atom), &rank) in entries.iter_mut().zip(&graph.atoms).zip(&ranks) {
        *entry = vec![i32::from(atom.atomic_number), rank as i32];
    }
    let mut degree = vec![0usize; n];
    for bond in &graph.bonds {
        for atom in [bond.a, bond.b] {
            *degree.get_mut(atom).ok_or("Missing CIP atom degree")? += 1;
        }
    }
    // Unlike the native fixed 16-slot array, this remains safe for high-degree
    // atoms. A bond to P of degree 3/4 has a special duplicate-atom weight.
    let mut neighbors = vec![Vec::new(); n];
    let mut storage = 3 * n;
    for bond in &graph.bonds {
        work.spend(1)?;
        for (atom, other) in [(bond.a, bond.b), (bond.b, bond.a)] {
            let weight = if bond.order == 2
                && at(&graph.atoms, other)?.atomic_number == 15
                && matches!(*at(&degree, other)?, 3 | 4)
            {
                1usize
            } else {
                match bond.order {
                    0 => 0,
                    1 | 5 => 2,
                    2 => 4,
                    3 => 6,
                    4 | 7 => 3,
                    6 => 8,
                    _ => return Err("Invalid CIP bond type".into()),
                }
            };
            storage += weight;
            neighbors
                .get_mut(atom)
                .ok_or("Missing CIP neighbors")?
                .push((other, weight));
        }
    }
    let hydrogens: Vec<_> = graph
        .atoms
        .iter()
        .zip(&cache)
        .map(|(a, v)| usize::from(a.explicit_hydrogens) + v.implicit_hydrogens as usize)
        .collect();
    for &count in &hydrogens {
        storage += count;
    }
    if storage > 8_000_000 {
        return Err("CIP ranking storage limit exceeded".into());
    }
    for _ in 0..n / 2 + 1 {
        if tied.is_empty() {
            break;
        }
        work.spend(storage)?;
        for (i, entry) in entries.iter_mut().enumerate() {
            // Only scalar rank/weight pairs are sorted; equal ranks produce the
            // same repeated values regardless of insertion order.
            let mut weighted = at(&neighbors, i)?
                .iter()
                .map(|&(other, weight)| Ok((*at(&ranks, other)?, weight)))
                .collect::<Result<Vec<_>, String>>()?;
            work.spend(
                weighted
                    .len()
                    .saturating_mul(weighted.len().max(1).ilog2() as usize + 1),
            )?;
            weighted.sort_unstable_by_key(|pair| std::cmp::Reverse(pair.0));
            for (rank, count) in weighted {
                entry.extend(std::iter::repeat_n(rank as i32 + 1, count));
            }
            entry.resize(entry.len() + *at(&hydrogens, i)?, 0);
        }
        for range in tied {
            sort(
                &entries,
                order.get_mut(range).ok_or("Invalid CIP tied segment")?,
                work,
            )?;
        }
        let previous = classes;
        (classes, tied) = segments(&entries, &order, &mut ranks, work)?;
        if classes <= previous {
            break;
        }
        for (entry, &rank) in entries.iter_mut().zip(&ranks) {
            entry.resize(3, 0);
            *entry.get_mut(2).ok_or("Missing CIP rank slot")? = rank as i32;
        }
    }
    Ok(ranks)
}

fn compare(entries: &[Vec<i32>], a: usize, b: usize, work: &mut Work) -> Result<Ordering, String> {
    let (a, b) = (at(entries, a)?, at(entries, b)?);
    for (left, right) in a.iter().zip(b) {
        work.spend(1)?;
        let result = left.cmp(right);
        if result != Ordering::Equal {
            return Ok(result);
        }
    }
    work.spend(1)?;
    Ok(a.len().cmp(&b.len()))
}

// Fallible iterative mergesort: budget exhaustion can stop during comparisons,
// rather than using an inconsistent comparator or finishing unbounded work.
fn sort(entries: &[Vec<i32>], order: &mut [usize], work: &mut Work) -> Result<(), String> {
    let n = order.len();
    let mut scratch = order.to_vec();
    let mut width = 1;
    while width < n {
        work.spend(n)?;
        for start in (0..n).step_by(width * 2) {
            let middle = (start + width).min(n);
            let end = (middle + width).min(n);
            let (mut left, mut right) = (start, middle);
            for out in start..end {
                let take_left = right == end
                    || (left < middle
                        && compare(entries, *at(order, left)?, *at(order, right)?, work)?
                            != Ordering::Greater);
                let source = if take_left {
                    let v = left;
                    left += 1;
                    v
                } else {
                    let v = right;
                    right += 1;
                    v
                };
                *scratch.get_mut(out).ok_or("Missing CIP sort slot")? = *at(order, source)?;
            }
        }
        order.copy_from_slice(&scratch);
        width *= 2;
    }
    Ok(())
}

fn segments(
    entries: &[Vec<i32>],
    order: &[usize],
    ranks: &mut [u32],
    work: &mut Work,
) -> Result<(usize, Vec<Range<usize>>), String> {
    let Some(&first) = order.first() else {
        return Ok((0, Vec::new()));
    };
    let mut previous = first;
    let mut rank = 0;
    let mut start = None;
    let mut tied = Vec::new();
    *ranks.get_mut(first).ok_or("Missing CIP rank")? = 0;
    for (i, &atom) in order.iter().enumerate().skip(1) {
        if compare(entries, previous, atom, work)? == Ordering::Equal {
            if start.is_none() {
                start = Some(i - 1);
            }
        } else {
            rank += 1;
            previous = atom;
            if let Some(start) = start.take() {
                // The reference's closed tied segment includes the next atom.
                tied.push(start..i + 1);
            }
        }
        *ranks.get_mut(atom).ok_or("Missing CIP rank")? = rank;
    }
    if let Some(start) = start {
        tied.push(start..order.len());
    }
    Ok((rank as usize + 1, tied))
}
