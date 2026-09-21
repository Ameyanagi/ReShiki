//! RDKit's default aromaticity model on a graph and its symmetric SSSR rings.
//! Adapted from Aromaticity.cpp and MolOps.cpp (2026.03.6).
//! Copyright (C) 2001-2023 Greg Landrum and other RDKit contributors.
//! Relative electronegativity: Copyright (C) 2001-2011 Rational Discovery LLC.
//! BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
use super::{
    ELEMENTS, Element,
    graph::{Graph, Valence, pi_electron_count},
};
use std::collections::{BTreeMap, HashMap, HashSet};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, PartialEq)]
enum Donor {
    None,
    Vacant,
    One,
    Two,
    Any,
}

pub struct Aromaticity {
    pub graph: Graph,
    pub aromatic_rings: usize,
}

fn at<T>(items: &[T], id: usize) -> Result<&T, String> {
    items
        .get(id)
        .ok_or_else(|| "Invalid aromaticity index".into())
}
fn element(number: i32) -> Result<&'static Element, String> {
    usize::try_from(number)
        .ok()
        .and_then(|i| ELEMENTS.get(i))
        .ok_or_else(|| "Invalid effective aromatic atomic number".into())
}
struct Work(usize);
impl Work {
    fn spend(&mut self, amount: usize) -> Result<(), String> {
        self.0 = self
            .0
            .checked_sub(amount)
            .ok_or("Aromaticity work limit exceeded")?;
        Ok(())
    }
}
struct Topology<'a> {
    graph: &'a Graph,
    valences: Vec<Valence>,
    neighbors: Vec<Vec<(usize, usize)>>,
    ring_bonds: Vec<Vec<usize>>,
    cyclic: Vec<bool>,
}
impl<'a> Topology<'a> {
    fn new(graph: &'a Graph, rings: &[Vec<usize>]) -> Result<Self, String> {
        let valences = graph.provisional_valences()?;
        let mut neighbors = vec![Vec::new(); graph.atoms.len()];
        let mut pairs = HashMap::new();
        for (id, bond) in graph.bonds.iter().enumerate() {
            for (a, b) in [(bond.a, bond.b), (bond.b, bond.a)] {
                neighbors
                    .get_mut(a)
                    .ok_or("Missing aromaticity endpoint")?
                    .push((b, id));
            }
            pairs.insert((bond.a.min(bond.b), bond.a.max(bond.b)), id);
        }
        let mut cyclic = vec![false; graph.bonds.len()];
        let mut ring_bonds = Vec::new();
        let mut total = 0usize;
        for ring in rings {
            total = total
                .checked_add(ring.len())
                .ok_or("Aromaticity ring limit exceeded")?;
            if ring.len() < 3
                || total > 2_000_000
                || ring.iter().collect::<HashSet<_>>().len() != ring.len()
            {
                return Err("Invalid aromaticity ring data".into());
            }
            let mut bonds = Vec::new();
            for (&a, &b) in ring.iter().zip(ring.iter().cycle().skip(1)) {
                let &bond = pairs
                    .get(&(a.min(b), a.max(b)))
                    .ok_or("Missing aromaticity ring bond")?;
                *cyclic.get_mut(bond).ok_or("Missing cyclic bond")? = true;
                bonds.push(bond);
            }
            ring_bonds.push(bonds);
        }
        Ok(Self {
            graph,
            valences,
            neighbors,
            ring_bonds,
            cyclic,
        })
    }
    fn electrons(&self, id: usize) -> Result<i32, String> {
        let atom = at(&self.graph.atoms, id)?;
        let valence = at(&self.valences, id)?;
        let adjacent = at(&self.neighbors, id)?;
        let mut zero_bonds = 0;
        for &(_, bond) in adjacent {
            zero_bonds += usize::from(at(&self.graph.bonds, bond)?.twice_contribution(id) == 0);
        }
        pi_electron_count(atom, valence, adjacent.len(), zero_bonds)
    }
    fn donor(&self, id: usize) -> Result<Donor, String> {
        let atom = at(&self.graph.atoms, id)?;
        let mut external = None;
        let mut cyclic_multiple = false;
        let mut degree = at(&self.neighbors, id)?.len() as i32 + i32::from(atom.explicit_hydrogens);
        for &(other, bond) in at(&self.neighbors, id)? {
            let contribution = at(&self.graph.bonds, bond)?.twice_contribution(id);
            if contribution == 0 {
                degree -= 1;
            }
            if contribution >= 4 {
                if *at(&self.cyclic, bond)? {
                    cyclic_multiple = true;
                } else if external.is_none() {
                    external = Some(other);
                }
            }
        }
        if atom.atomic_number == 0 {
            return Ok(if cyclic_multiple {
                Donor::One
            } else {
                Donor::Any
            });
        }
        let mut electrons = self.electrons(id)?;
        if electrons < 0 {
            return Ok(Donor::None);
        }
        if electrons == 0 {
            return Ok(if external.is_some() {
                Donor::Vacant
            } else if cyclic_multiple {
                Donor::One
            } else {
                Donor::None
            });
        }
        if let Some(other) = external {
            let number = at(&self.graph.atoms, other)?.atomic_number;
            let other_outer = element(i32::from(number))?.outer_electrons;
            let outer = element(i32::from(atom.atomic_number))?.outer_electrons;
            if other_outer > outer || other_outer == outer && number < atom.atomic_number {
                electrons -= 1;
            }
            return Ok(if electrons == 0 {
                Donor::Vacant
            } else if electrons % 2 == 1 {
                Donor::One
            } else {
                Donor::Two
            });
        }
        if electrons == 1 {
            return Ok(
                if at(&self.valences, id)?.explicit_valence as i32 != degree {
                    Donor::One
                } else if atom.charge == 1 {
                    Donor::Vacant
                } else {
                    Donor::None
                },
            );
        }
        Ok(if electrons % 2 == 1 {
            Donor::One
        } else {
            Donor::Two
        })
    }
    fn candidate(&self, id: usize, donor: Donor) -> Result<bool, String> {
        let atom = at(&self.graph.atoms, id)?;
        let number = atom.atomic_number;
        if (number > 18 && !matches!(number, 34 | 52)) || donor == Donor::None {
            return Ok(false);
        }
        let valence = at(&self.valences, id)?;
        let default = *element(i32::from(number))?
            .valences
            .first()
            .ok_or("Missing default valence")?;
        if default > 0 {
            let effective = *element(i32::from(number) - i32::from(atom.charge))?
                .valences
                .first()
                .ok_or("Missing effective valence")?;
            if valence.explicit_valence as i32 + valence.implicit_hydrogens as i32 > effective {
                return Ok(false);
            }
        }
        if atom.radical_electrons != 0 && (number != 6 || atom.charge != 0) {
            return Ok(false);
        }
        let adjacent = at(&self.neighbors, id)?;
        if valence.explicit_valence as i32 - adjacent.len() as i32 > 1 {
            let mut multiple = 0;
            for &(_, bond) in adjacent {
                multiple += usize::from(matches!(at(&self.graph.bonds, bond)?.order, 2 | 3));
            }
            if multiple > 1 {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

/// Perceive the default aromaticity model without changing explicit H counts.
/// Ring order is significant for shared atoms in macrocycles, matching RDKit.
pub fn perceive(graph: &Graph, rings: &[Vec<usize>]) -> Result<Aromaticity, String> {
    perceive_with_work(graph, rings, &mut Work(50_000_000))
}

fn perceive_with_work(
    graph: &Graph,
    rings: &[Vec<usize>],
    work: &mut Work,
) -> Result<Aromaticity, String> {
    let topology = Topology::new(graph, rings)?;
    let mut donors = vec![Donor::None; graph.atoms.len()];
    let mut seen = vec![false; graph.atoms.len()];
    let mut allowed = vec![false; graph.atoms.len()];
    let mut candidates = Vec::new();
    let mut bonds = Vec::new();
    for (index, ring) in rings.iter().enumerate() {
        let (mut all_allowed, mut all_dummy) = (true, true);
        for &id in ring {
            work.spend(1)?;
            let atom = at(&graph.atoms, id)?;
            all_dummy &= atom.atomic_number == 0;
            if !*at(&seen, id)? {
                let adjacent = at(&topology.neighbors, id)?;
                work.spend(adjacent.len() * 4 + 1)?;
                let mut donor = topology.donor(id)?;
                if donor == Donor::Two
                    && ring.len() >= 9
                    && matches!(atom.atomic_number, 8 | 16)
                    && adjacent.len() == 2
                    && atom.charge == 0
                {
                    let mut has_multiple = false;
                    for &(_, bond) in adjacent {
                        has_multiple |= matches!(at(&graph.bonds, bond)?.order, 2 | 3);
                    }
                    if !has_multiple {
                        donor = Donor::None;
                    }
                }
                *donors.get_mut(id).ok_or("Missing donor atom")? = donor;
                *allowed.get_mut(id).ok_or("Missing candidate atom")? =
                    topology.candidate(id, donor)?;
                *seen.get_mut(id).ok_or("Missing visited atom")? = true;
            }
            all_allowed &= *at(&allowed, id)?;
        }
        if all_allowed && !all_dummy {
            candidates.push(ring.as_slice());
            bonds.push(at(&topology.ring_bonds, index)?.as_slice());
        }
    }
    let neighbors = ring_neighbors(&bonds, graph.bonds.len(), work)?;
    let mut result = graph.clone();
    let mut visited = vec![false; candidates.len()];
    let mut aromatic_rings = 0;
    for root in 0..candidates.len() {
        if *at(&visited, root)? {
            continue;
        }
        let mut stack = vec![root];
        let mut fused = Vec::new();
        while let Some(ring) = stack.pop() {
            work.spend(1)?;
            if *at(&visited, ring)? {
                continue;
            }
            *visited.get_mut(ring).ok_or("Missing fused ring")? = true;
            fused.push(ring);
            stack.extend(at(&neighbors, ring)?.iter().rev().copied());
        }
        aromatic_rings += mark_fused(
            &mut result,
            &candidates,
            &bonds,
            &neighbors,
            &fused,
            &donors,
            work,
        )?;
    }
    Ok(Aromaticity {
        graph: result,
        aromatic_rings,
    })
}

fn ring_neighbors(
    rings: &[&[usize]],
    bond_count: usize,
    work: &mut Work,
) -> Result<Vec<Vec<usize>>, String> {
    let mut members = vec![Vec::new(); bond_count];
    for (id, ring) in rings.iter().enumerate().filter(|(_, r)| r.len() <= 24) {
        for &bond in *ring {
            members.get_mut(bond).ok_or("Missing ring edge")?.push(id);
        }
    }
    let mut overlap = BTreeMap::<(usize, usize), usize>::new();
    for containing in members {
        for (i, &a) in containing.iter().enumerate() {
            for &b in containing.iter().skip(i + 1) {
                work.spend(1)?;
                *overlap.entry((a, b)).or_default() += 1;
                if overlap.len() > 1_000_000 {
                    return Err("Aromaticity ring neighbor limit exceeded".into());
                }
            }
        }
    }
    let mut neighbors = vec![Vec::new(); rings.len()];
    for ((a, b), count) in overlap {
        if count == 1 {
            neighbors.get_mut(a).ok_or("Missing ring neighbor")?.push(b);
            neighbors.get_mut(b).ok_or("Missing ring neighbor")?.push(a);
        }
    }
    for list in &mut neighbors {
        list.sort_unstable();
    }
    Ok(neighbors)
}

fn connected(rings: &[usize], neighbors: &[Vec<usize>], work: &mut Work) -> Result<bool, String> {
    let root = *rings.first().ok_or("Missing ring combination")?;
    let mut stack = vec![root];
    let mut seen = HashSet::new();
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        for &other in at(neighbors, id)? {
            work.spend(1)?;
            if rings.contains(&other) && !seen.contains(&other) {
                stack.push(other);
            }
        }
    }
    Ok(seen.len() == rings.len())
}
fn huckel(atoms: &BTreeMap<usize, usize>, donors: &[Donor]) -> Result<bool, String> {
    let (mut low, mut high, mut any) = (0, 0, 0);
    for (&id, &count) in atoms {
        if count > 2 {
            continue;
        }
        let (l, h) = match at(donors, id)? {
            Donor::None | Donor::Vacant => (0, 0),
            Donor::One => (1, 1),
            Donor::Two => (2, 2),
            Donor::Any => {
                any += 1;
                (1, 2)
            }
        };
        if any > 1 {
            return Ok(false);
        }
        low += l;
        high += h;
    }
    Ok(if high >= 6 {
        low + (6 - low % 4) % 4 <= high
    } else {
        high == 2
    })
}
fn next_combination(indices: &mut [usize], count: usize) -> Result<bool, String> {
    let length = indices.len();
    for i in (0..length).rev() {
        if *at(indices, i)? < count - length + i {
            let first = *at(indices, i)? + 1;
            for (offset, value) in indices.iter_mut().skip(i).enumerate() {
                *value = first + offset;
            }
            return Ok(true);
        }
    }
    Ok(false)
}
fn mark_fused(
    graph: &mut Graph,
    rings: &[&[usize]],
    bonds: &[&[usize]],
    neighbors: &[Vec<usize>],
    fused: &[usize],
    donors: &[Donor],
    work: &mut Work,
) -> Result<usize, String> {
    let mut total_bonds = HashSet::new();
    for &ring in fused {
        total_bonds.extend(at(bonds, ring)?.iter().copied());
    }
    let mut done = HashSet::new();
    let mut aromatic = HashSet::new();
    // Retain RDKit's six-ring combination limit and its two-ring cap for
    // systems larger than 300 rings. A separate work budget bounds all cases.
    let maximum = fused.len().min(if fused.len() > 300 { 2 } else { 6 });
    for size in 1..=maximum {
        if done.len() >= total_bonds.len() {
            break;
        }
        let mut combination = (0..size).collect::<Vec<_>>();
        loop {
            work.spend(1)?;
            let current = combination
                .iter()
                .map(|&i| at(fused, i).copied())
                .collect::<Result<Vec<_>, _>>()?;
            if connected(&current, neighbors, work)? {
                let mut atom_count = BTreeMap::new();
                for &ring in &current {
                    let atoms = at(rings, ring)?;
                    work.spend(atoms.len())?;
                    for &atom in *atoms {
                        *atom_count.entry(atom).or_default() += 1;
                    }
                }
                if huckel(&atom_count, donors)? {
                    let mut bond_count = BTreeMap::<usize, usize>::new();
                    for &ring in &current {
                        for &bond in *at(bonds, ring)? {
                            *bond_count.entry(bond).or_default() += 1;
                        }
                    }
                    for (id, count) in bond_count {
                        if count != 1 {
                            continue;
                        }
                        let bond = graph.bonds.get_mut(id).ok_or("Missing aromatic bond")?;
                        bond.aromatic = true;
                        if matches!(bond.order, 1 | 2) {
                            bond.order = 4;
                            for atom in [bond.a, bond.b] {
                                graph
                                    .atoms
                                    .get_mut(atom)
                                    .ok_or("Missing aromatic atom")?
                                    .aromatic = true;
                            }
                        }
                        done.insert(id);
                    }
                    aromatic.extend(current);
                }
            }
            if !next_combination(&mut combination, fused.len())? {
                break;
            }
        }
    }
    Ok(aromatic.len())
}

/// Preserve hydrogens that disappear from the implicit cache when aromaticity
/// changes bond orders, e.g. the N-H in a pyridone. The prior counts must be from
/// the same graph immediately before aromaticity perception.
pub fn adjust_hydrogens(graph: &Graph, previous: &[Valence]) -> Result<Graph, String> {
    if previous.len() != graph.atoms.len() {
        return Err("Hydrogen cache size changed".into());
    }
    let current = graph.provisional_valences()?;
    let mut result = graph.clone();
    for ((atom, now), before) in result.atoms.iter_mut().zip(current).zip(previous) {
        let lost = before
            .implicit_hydrogens
            .saturating_sub(now.implicit_hydrogens);
        atom.explicit_hydrogens = u32::from(atom.explicit_hydrogens)
            .checked_add(lost)
            .and_then(|h| u8::try_from(h).ok())
            .ok_or("Adjusted hydrogen count exceeds supported range")?;
    }
    Ok(result)
}
