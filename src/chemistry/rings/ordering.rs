//! Certify that equal-size greedy pruning choices have one ring result.
//!
//! RDKit uses std::sort with only ring size as its comparator. Different C++
//! libraries can permute ties differently and even change the symmetric ring
//! count. Explore all effective greedy choices before trusting a Rust result.
use super::Budget;
use std::collections::{BTreeSet, HashMap, HashSet};

fn symmetric(bonds: &[HashSet<usize>], chosen: u128, budget: &mut Budget) -> Option<u128> {
    let mut counts = HashMap::<usize, usize>::new();
    for (i, ring) in bonds.iter().enumerate() {
        budget.spend(ring.len()).ok()?;
        if chosen & (1u128.checked_shl(i as u32)?) != 0 {
            for &bond in ring {
                *counts.entry(bond).or_default() += 1;
            }
        }
    }
    let mut result = chosen;
    for (i, extra) in bonds.iter().enumerate() {
        let bit = 1u128.checked_shl(i as u32)?;
        if chosen & bit != 0 {
            continue;
        }
        for (j, ring) in bonds.iter().enumerate() {
            budget.spend(ring.len()).ok()?;
            if chosen & (1u128.checked_shl(j as u32)?) == 0 || ring.len() != extra.len() {
                continue;
            }
            if !ring.is_disjoint(extra)
                && ring
                    .iter()
                    .all(|b| counts.get(b) != Some(&1) || extra.contains(b))
            {
                result |= bit;
                break;
            }
        }
    }
    Some(result)
}

pub(super) fn independent(bonds: &[HashSet<usize>], keep: &[bool], global: &mut Budget) -> bool {
    // Larger searches keep the reference result; never guess that ties are safe.
    if bonds.len() > 128 || bonds.len() != keep.len() {
        return false;
    }
    let mut local = Budget {
        work: global.work.min(250_000),
        stored: 0,
    };
    let before = local.work;
    let answer = verify(bonds, keep, &mut local).unwrap_or(false);
    global.work -= before - local.work;
    answer
}

fn verify(bonds: &[HashSet<usize>], keep: &[bool], budget: &mut Budget) -> Option<bool> {
    let expected_basis = keep.iter().enumerate().try_fold(0u128, |mask, (i, &yes)| {
        Some(mask | if yes { 1u128.checked_shl(i as u32)? } else { 0 })
    })?;
    let expected_sym = symmetric(bonds, expected_basis, budget)?;
    let mut pending = vec![0u128];
    let mut visited = BTreeSet::new();
    while let Some(chosen) = pending.pop() {
        budget.spend(1).ok()?;
        if !visited.insert(chosen) {
            continue;
        }
        let mut union = HashSet::new();
        for (i, ring) in bonds.iter().enumerate() {
            budget.spend(ring.len()).ok()?;
            if chosen & (1u128.checked_shl(i as u32)?) != 0 {
                union.extend(ring.iter().copied());
            }
        }
        let mut candidates = Vec::new();
        for (i, ring) in bonds.iter().enumerate() {
            budget.spend(ring.len()).ok()?;
            if !ring.is_subset(&union) {
                candidates.push((i, ring));
            }
        }
        let Some((_, first)) = candidates.first() else {
            if chosen.count_ones() != expected_basis.count_ones()
                || symmetric(bonds, chosen, budget)? != expected_sym
            {
                return Some(false);
            }
            continue;
        };
        let size = first.len();
        candidates.retain(|(_, ring)| ring.len() == size);
        let mut started = false;
        for (i, ring) in bonds.iter().enumerate() {
            if ring.len() == size && chosen & (1u128.checked_shl(i as u32)?) != 0 {
                started = true;
            }
        }
        // First ring of a size group may be any ring. Later choices maximize
        // overlap, with ties decided by the unspecified original sort order.
        let mut scored = Vec::new();
        for (i, ring) in candidates {
            budget.spend(ring.len()).ok()?;
            scored.push((
                i,
                if started {
                    ring.intersection(&union).count()
                } else {
                    0
                },
            ));
        }
        let best = scored.iter().map(|&(_, score)| score).max()?;
        for (i, score) in scored.into_iter().rev() {
            if score == best {
                pending.push(chosen | 1u128.checked_shl(i as u32)?);
            }
        }
        if pending.len() > 65_536 {
            return None;
        }
    }
    Some(true)
}
