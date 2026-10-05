//! Ring-neighborhood invariants from RDKit new_canon.cpp.
//! Copyright (C) 2014 Greg Landrum. BSD-3-Clause; licenses/rdkit/.
use super::{Ranker, at, put};

impl Ranker<'_> {
    pub(super) fn build_symmetry_invariants(&mut self) -> Result<(), String> {
        let n = self.graph.atoms.len();
        let mut storage = 0usize;
        for root in 0..n {
            if *at(&self.ring_counts, root)? == 0 {
                continue;
            }
            self.work.spend(n)?;
            let mut visited = vec![false; n];
            let mut last = vec![false; n];
            let mut current = vec![false; n];
            let mut revisited = vec![0i32; n];
            let mut level = vec![root];
            let mut numbers = Vec::new();
            let mut revisits = Vec::new();
            while !level.is_empty() {
                let mut next = Vec::new();
                self.work.spend(level.len())?;
                for &atom in &level {
                    if *at(&self.ring_counts, atom)? == 0 {
                        continue;
                    }
                    put(&mut last, atom, true)?;
                    put(&mut visited, atom, true)?;
                    let neighbors = at(&self.neighbors, atom)?;
                    self.work.spend(neighbors.len())?;
                    for &other in neighbors {
                        if !*at(&visited, other)? {
                            put(&mut current, other, true)?;
                            put(&mut visited, other, true)?;
                            next.push(other);
                        }
                    }
                }
                let mut modified = Vec::new();
                for &atom in &next {
                    let neighbors = at(&self.neighbors, atom)?;
                    self.work.spend(neighbors.len())?;
                    for &other in neighbors {
                        if *at(&current, other)? || *at(&last, other)? {
                            let value = revisited
                                .get_mut(other)
                                .ok_or("Missing symmetry neighbor")?;
                            if *value == 0 {
                                modified.push(other);
                            }
                            *value += 1;
                        }
                    }
                }
                for &atom in &level {
                    put(&mut last, atom, false)?;
                }
                for &atom in &next {
                    put(&mut last, atom, true)?;
                    put(&mut current, atom, false)?;
                }
                let mut values = Vec::new();
                for atom in modified {
                    values.push(*at(&revisited, atom)?);
                    put(&mut revisited, atom, 0)?;
                }
                values.sort_unstable();
                storage = storage
                    .checked_add(values.len() + 3)
                    .ok_or("Canonical invariant storage exceeded")?;
                if storage > 8_000_000 {
                    return Err("Canonical invariant storage exceeded".into());
                }
                revisits.extend(values);
                revisits.push(-1);
                numbers.push(
                    i32::try_from(next.len()).map_err(|_| "Canonical neighbor count overflow")?,
                );
                numbers.push(-1);
                level = next;
            }
            put(&mut self.neighbor_numbers, root, numbers)?;
            put(&mut self.revisited, root, revisits)?;
        }
        Ok(())
    }
}
