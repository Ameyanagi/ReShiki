//! Iterative search and dummy-atom backtracking from RDKit Kekulize.cpp.
//! Copyright (C) 2001-2021 Greg Landrum and other RDKit contributors.
//! BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
use super::{Assignment, Candidates, Direction, Failure, Topology, Work, at, set};
use std::collections::{HashMap, HashSet, VecDeque};

// Kekulize.cpp's diagnostic uses the original global candidate bitset, not
// the mutated copy consumed by each search or dummy-atom permutation.
fn failed(candidates: &[bool]) -> Failure {
    let mut message = String::from("Can't kekulize mol.  Unkekulized atoms:");
    for (index, &eligible) in candidates.iter().enumerate() {
        if eligible {
            message.push(' ');
            message.push_str(&index.to_string());
        }
    }
    Failure::Chemical(message)
}

pub(super) fn fused(
    result: &mut Assignment,
    atoms: &[usize],
    candidates: Candidates,
    topology: &Topology,
    ranks: &[u32],
    max_backtracks: usize,
    work: &mut Work,
) -> Result<(), Failure> {
    let mut search = Search {
        result,
        topology,
        ranks,
        max_backtracks,
        work,
    };
    if search.attempt(atoms, candidates.eligible.clone(), candidates.done)? {
        return Ok(());
    }
    let mut switches = vec![false; candidates.questions.len()];
    if switches.is_empty() {
        return Err(failed(&candidates.eligible));
    }
    set(&mut switches, 0, true)?;
    let atom_set = atoms.iter().copied().collect::<HashSet<_>>();
    loop {
        search
            .work
            .spend(search.result.graph.bonds.len() + candidates.eligible.len())?;
        for bond in &mut search.result.graph.bonds {
            if bond.aromatic
                && bond.order != 1
                && atom_set.contains(&bond.a)
                && atom_set.contains(&bond.b)
            {
                bond.order = 1;
            }
        }
        let mut eligible = candidates.eligible.clone();
        for (&atom, &off) in candidates.questions.iter().zip(&switches) {
            if off {
                set(&mut eligible, atom, false)?;
            }
        }
        if search.attempt(atoms, eligible, Vec::new())? {
            return Ok(());
        }
        let mut carry = true;
        for bit in &mut switches {
            if *bit {
                *bit = false;
            } else {
                *bit = true;
                carry = false;
                break;
            }
        }
        if carry {
            return Err(failed(&candidates.eligible));
        }
    }
}

struct Search<'a> {
    result: &'a mut Assignment,
    topology: &'a Topology,
    ranks: &'a [u32],
    max_backtracks: usize,
    work: &'a mut Work,
}
impl Search<'_> {
    fn attempt(
        &mut self,
        atoms: &[usize],
        mut eligible: Vec<bool>,
        mut done: Vec<usize>,
    ) -> Result<bool, String> {
        self.work
            .spend(self.result.graph.atoms.len() + self.result.graph.bonds.len())?;
        let in_atoms = atoms.iter().copied().collect::<HashSet<_>>();
        let mut wedge_end = HashSet::new();
        for (bond, &direction) in self.result.graph.bonds.iter().zip(&self.result.directions) {
            if direction.wedged() && in_atoms.contains(&bond.b) {
                wedge_end.insert(bond.b);
            }
        }
        let mut sorted = atoms
            .iter()
            .map(|&a| Ok((!wedge_end.contains(&a), *at(self.ranks, a)?, a)))
            .collect::<Result<Vec<_>, String>>()?;
        sorted.sort_unstable();
        let mut done_set = done.iter().copied().collect::<HashSet<_>>();
        let mut queue = VecDeque::<usize>::new();
        let mut queued = vec![0u32; self.result.graph.atoms.len()];
        let mut added = vec![false; self.result.graph.bonds.len()];
        let mut local_added = added.clone();
        let mut options = HashMap::<usize, VecDeque<(usize, usize)>>::new();
        let mut backtrack_points = Vec::new();
        let mut backtracks = 0usize;
        while done.len() < atoms.len() || !queue.is_empty() {
            self.work.spend(1)?;
            let current = if let Some(atom) = queue.pop_front() {
                let n = queued.get_mut(atom).ok_or("Missing queued atom")?;
                *n = n.checked_sub(1).ok_or("Invalid Kekulé queue state")?;
                atom
            } else {
                self.work.spend(sorted.len())?;
                sorted
                    .iter()
                    .find_map(|&(_, _, a)| (!done_set.contains(&a)).then_some(a))
                    .ok_or("Missing Kekulé starting atom")?
            };
            done.push(current);
            done_set.insert(current);
            let can_assign = *at(&eligible, current)?;
            let mut opts = if let Some(stored) = options.get(&current) {
                stored.clone()
            } else {
                let adjacent = at(&self.topology.neighbors, current)?;
                self.work.spend(adjacent.len())?;
                let mut neighbors = Vec::new();
                for &(other, bond) in adjacent {
                    if in_atoms.contains(&other) && !done_set.contains(&other) {
                        neighbors.push((*at(self.ranks, other)?, other, bond));
                    }
                }
                neighbors.sort_unstable();
                let mut ordinary = VecDeque::new();
                let mut wedged = VecDeque::new();
                for (_, other, bond) in neighbors {
                    if *at(&queued, other)? == 0 {
                        queue.push_back(other);
                        set(&mut queued, other, 1)?;
                    }
                    if can_assign
                        && *at(&eligible, other)?
                        && (at(&self.result.graph.bonds, bond)?.aromatic
                            || at(&self.result.graph.atoms, current)?.atomic_number == 0
                            || at(&self.result.graph.atoms, other)?.atomic_number == 0)
                    {
                        if at(&self.result.directions, bond)?.wedged() {
                            wedged.push_back((other, bond));
                        } else {
                            ordinary.push_back((other, bond));
                        }
                    }
                }
                ordinary.extend(wedged);
                ordinary
            };
            if !can_assign {
                continue;
            }
            if let Some((other, bond)) = opts.pop_front() {
                self.result
                    .graph
                    .bonds
                    .get_mut(bond)
                    .ok_or("Missing Kekulé bond")?
                    .order = 2;
                set(&mut self.result.directions, bond, Direction::None)?;
                set(&mut eligible, current, false)?;
                set(&mut eligible, other, false)?;
                set(&mut added, bond, true)?;
                set(&mut local_added, bond, true)?;
                match options.entry(current) {
                    std::collections::hash_map::Entry::Occupied(mut entry) => {
                        if opts.is_empty() {
                            entry.remove();
                            backtrack_points.pop();
                        } else {
                            entry.insert(opts);
                        }
                    }
                    std::collections::hash_map::Entry::Vacant(entry) => {
                        if !opts.is_empty() {
                            backtrack_points.push(current);
                            entry.insert(opts);
                        }
                    }
                }
            } else if at(&self.result.graph.atoms, current)?.atomic_number != 0 {
                if let Some(&last) = backtrack_points
                    .last()
                    .filter(|_| backtracks < self.max_backtracks)
                {
                    self.work
                        .spend(done.len() + self.result.graph.bonds.len())?;
                    let split = done
                        .iter()
                        .position(|&a| a == last)
                        .ok_or("Missing backtrack atom")?;
                    for &atom in done.iter().skip(split).rev() {
                        queue.push_front(atom);
                        let n = queued.get_mut(atom).ok_or("Missing backtrack queue atom")?;
                        *n = n.checked_add(1).ok_or("Kekulé queue overflow")?;
                    }
                    done.truncate(split);
                    done_set = done.iter().copied().collect();
                    for (bond, added) in self.result.graph.bonds.iter_mut().zip(&mut added) {
                        if *added && !done_set.contains(&bond.a) && !done_set.contains(&bond.b) {
                            *added = false;
                            bond.order = 1;
                            set(&mut eligible, bond.a, true)?;
                            set(&mut eligible, bond.b, true)?;
                        }
                    }
                    backtracks += 1;
                } else {
                    for (bond, changed) in self.result.graph.bonds.iter_mut().zip(&local_added) {
                        if *changed {
                            bond.order = 1;
                        }
                    }
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
}
