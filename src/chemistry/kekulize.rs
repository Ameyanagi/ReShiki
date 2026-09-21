//! Bounded Kekulé bond assignment from RDKit Kekulize.cpp (2026.03.6).
//! Copyright (C) 2001-2021 Greg Landrum and other RDKit contributors.
//! Early-element classification from Atom.cpp, Copyright (C) 2001-2024.
//! BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
mod search;
#[cfg(test)]
mod tests;
use super::{
    ELEMENTS,
    graph::{Graph, Valence},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    None,
    Wedge,
    Hash,
    Down,
    Up,
    EitherDouble,
    Unknown,
}
impl Direction {
    fn wedged(self) -> bool {
        matches!(self, Self::Wedge | Self::Hash)
    }
}

pub struct Options<'a> {
    pub clear_aromaticity: bool,
    /// None uses atom order, as in the first sanitization attempt. Canonical
    /// assignment requires separately computed ranks; this kernel does not
    /// claim to implement canonical ranking or its stereochemistry rules.
    pub ranks: Option<&'a [u32]>,
    pub max_backtracks: usize,
}
impl Default for Options<'_> {
    fn default() -> Self {
        Self {
            clear_aromaticity: true,
            ranks: None,
            max_backtracks: 100,
        }
    }
}
#[derive(Debug, Serialize)]
pub struct Assignment {
    pub graph: Graph,
    pub directions: Vec<Direction>,
}
fn at<T>(items: &[T], index: usize) -> Result<&T, String> {
    items
        .get(index)
        .ok_or_else(|| "Invalid Kekulé topology index".into())
}
fn set<T>(items: &mut [T], index: usize, value: T) -> Result<(), String> {
    *items
        .get_mut(index)
        .ok_or("Invalid Kekulé topology index")? = value;
    Ok(())
}
struct Work(usize);
impl Work {
    fn spend(&mut self, amount: usize) -> Result<(), String> {
        self.0 = self
            .0
            .checked_sub(amount)
            .ok_or("Kekulé work limit exceeded")?;
        Ok(())
    }
}
struct Topology {
    neighbors: Vec<Vec<(usize, usize)>>,
    members: Vec<Vec<usize>>,
    ring_bonds: Vec<Vec<usize>>,
    valences: Vec<Valence>,
}
impl Topology {
    fn new(graph: &Graph, rings: &[Vec<usize>]) -> Result<Self, String> {
        let valences = graph.provisional_valences()?;
        let mut neighbors = vec![Vec::new(); graph.atoms.len()];
        let mut pairs = HashMap::new();
        for (id, bond) in graph.bonds.iter().enumerate() {
            for (a, b) in [(bond.a, bond.b), (bond.b, bond.a)] {
                neighbors
                    .get_mut(a)
                    .ok_or("Missing Kekulé atom")?
                    .push((b, id));
            }
            pairs.insert((bond.a.min(bond.b), bond.a.max(bond.b)), id);
        }
        let mut members = vec![Vec::new(); graph.atoms.len()];
        let mut ring_bonds = Vec::new();
        let mut storage = 0usize;
        for (id, ring) in rings.iter().enumerate() {
            storage = storage
                .checked_add(ring.len())
                .ok_or("Kekulé ring storage exceeded")?;
            if ring.len() < 3
                || storage > 2_000_000
                || ring.iter().collect::<HashSet<_>>().len() != ring.len()
            {
                return Err("Invalid Kekulé ring data".into());
            }
            let mut bonds = Vec::new();
            for (&a, &b) in ring.iter().zip(ring.iter().cycle().skip(1)) {
                let &bond = pairs
                    .get(&(a.min(b), a.max(b)))
                    .ok_or("Missing Kekulé ring bond")?;
                members
                    .get_mut(a)
                    .ok_or("Missing Kekulé ring atom")?
                    .push(id);
                bonds.push(bond);
            }
            ring_bonds.push(bonds);
        }
        Ok(Self {
            neighbors,
            members,
            ring_bonds,
            valences,
        })
    }
    fn aromatic_atoms(&self, graph: &Graph) -> Result<Vec<bool>, String> {
        let mut result = graph.atoms.iter().map(|a| a.aromatic).collect::<Vec<_>>();
        for bond in &graph.bonds {
            if bond.aromatic || bond.order == 4 {
                set(&mut result, bond.a, true)?;
                set(&mut result, bond.b, true)?;
            }
        }
        Ok(result)
    }
}
fn early_atom(number: u8) -> bool {
    matches!(number, 3..=5 | 11..=13 | 19..=22 | 30..=32 | 37..=41 | 48..=51 | 55..=61 | 72..=73 | 80..=83 | 87..=93 | 104..=118)
}
struct Candidates {
    eligible: Vec<bool>,
    questions: Vec<usize>,
    done: Vec<usize>,
}
fn candidates(
    graph: &mut Graph,
    atoms: &[usize],
    rings: &[Vec<usize>],
    topology: &Topology,
    work: &mut Work,
) -> Result<Candidates, String> {
    work.spend(graph.atoms.len() + graph.bonds.len())?;
    let aromatic = topology.aromatic_atoms(graph)?;
    let mut result = Candidates {
        eligible: vec![false; graph.atoms.len()],
        questions: Vec::new(),
        done: Vec::new(),
    };
    let mut relevant = false;
    for &id in atoms {
        relevant |= at(&graph.atoms, id)?.atomic_number == 0 || *at(&aromatic, id)?;
    }
    if !relevant {
        return Ok(result);
    }
    let mut non_candidate = Vec::new();
    for ring in rings {
        work.spend(ring.len())?;
        let mut candidate = false;
        for &atom in ring {
            candidate |= *at(&aromatic, atom)? && at(&topology.members, atom)?.len() == 1;
        }
        non_candidate.push(!candidate);
    }
    let mut in_atoms = vec![false; graph.atoms.len()];
    let mut make_single = Vec::new();
    for &id in atoms {
        // The reference grows this mask in ring-union order. Dummy candidate
        // selection deliberately observes only atoms already encountered.
        set(&mut in_atoms, id, true)?;
        let atom = at(&graph.atoms, id)?;
        if atom.atomic_number != 0 && !*at(&aromatic, id)? {
            result.done.push(id);
            continue;
        }
        let adjacent = at(&topology.neighbors, id)?;
        work.spend(adjacent.len())?;
        let (mut sbo, mut ignored, mut ordinary_neighbors) = (0i32, 0i32, 0usize);
        for &(other, bond) in adjacent {
            let target = at(&graph.atoms, other)?;
            if target.atomic_number != 0 && !target.aromatic && *at(&in_atoms, other)? {
                ordinary_neighbors += 1;
            }
            let edge = at(&graph.bonds, bond)?;
            if edge.aromatic && matches!(edge.order, 1 | 2 | 4) {
                sbo += 1;
                make_single.push(bond);
            } else {
                let contribution = match edge.order {
                    0 => 0,
                    1..=3 => i32::from(edge.order),
                    4 | 7 => 2,
                    5 if edge.b == id => 1,
                    5 => 0,
                    6 => 4,
                    _ => return Err("Unsupported Kekulé bond".into()),
                };
                sbo += contribution;
                ignored += i32::from(contribution == 0);
            }
        }
        let members = at(&topology.members, id)?;
        let mut non_candidates = 0;
        for &ring in members {
            non_candidates += usize::from(*at(&non_candidate, ring)?);
        }
        if atom.atomic_number == 0
            && ordinary_neighbors < members.len()
            && non_candidates < members.len()
        {
            set(&mut result.eligible, id, true)?;
            result.questions.push(id);
            continue;
        }
        let valence = at(&topology.valences, id)?;
        let hydrogens = i32::from(atom.explicit_hydrogens) + valence.implicit_hydrogens as i32;
        sbo += hydrogens;
        let allowed = at(ELEMENTS, usize::from(atom.atomic_number))?.valences;
        let mut charge = i32::from(atom.charge);
        if early_atom(atom.atomic_number) || atom.atomic_number == 6 && charge > 0 {
            charge = -charge;
        }
        let mut default = *allowed.first().ok_or("Missing Kekulé valence")? + charge;
        let total_valence = valence.explicit_valence as i32 + valence.implicit_hydrogens as i32;
        let radicals = i32::from(atom.radical_electrons);
        let degree = adjacent.len() as i32 + valence.implicit_hydrogens as i32 - ignored;
        for &next in allowed.iter().skip(1) {
            if total_valence <= default || next <= 0 {
                break;
            }
            default = next + charge;
        }
        if total_valence == 5
            && sbo == 4
            && default == 3
            && degree == 3
            && radicals == 0
            && charge == 0
            && hydrogens == 0
            && matches!(atom.atomic_number, 7 | 15 | 33)
        {
            default = 5;
        }
        if degree + radicals < default
            && (default == sbo + 1 + radicals
                || radicals == 0 && atom.no_implicit && default == sbo + 2)
        {
            set(&mut result.eligible, id, true)?;
        }
    }
    for bond in make_single {
        graph
            .bonds
            .get_mut(bond)
            .ok_or("Missing aromatic bond")?
            .order = 1;
    }
    Ok(result)
}

fn fused_neighbors(
    rings: &[Vec<usize>],
    topology: &Topology,
    original_ids: &[usize],
    work: &mut Work,
) -> Result<Vec<Vec<usize>>, String> {
    let mut members = HashMap::<usize, Vec<usize>>::new();
    for (id, &original) in original_ids.iter().enumerate() {
        for &bond in at(&topology.ring_bonds, original)? {
            members.entry(bond).or_default().push(id);
        }
    }
    let mut pairs = BTreeSet::new();
    for ids in members.values() {
        for (i, &a) in ids.iter().enumerate() {
            for &b in ids.iter().skip(i + 1) {
                work.spend(1)?;
                pairs.insert((a, b));
                if pairs.len() > 1_000_000 {
                    return Err("Kekulé fused-ring storage exceeded".into());
                }
            }
        }
    }
    let mut neighbors = vec![Vec::new(); rings.len()];
    for (a, b) in pairs {
        neighbors.get_mut(a).ok_or("Missing fused ring")?.push(b);
        neighbors.get_mut(b).ok_or("Missing fused ring")?.push(a);
    }
    for list in &mut neighbors {
        list.sort_unstable();
    }
    Ok(neighbors)
}

/// Assign single/double bonds without mutating the caller's graph or direction
/// metadata. Ranks are an explicit dependency, not a call to the Python worker.
pub fn assign(
    graph: &Graph,
    rings: &[Vec<usize>],
    directions: &[Direction],
    options: Options<'_>,
) -> Result<Assignment, String> {
    assign_with_work(graph, rings, directions, options, &mut Work(50_000_000))
}
fn assign_with_work(
    graph: &Graph,
    rings: &[Vec<usize>],
    directions: &[Direction],
    options: Options<'_>,
    work: &mut Work,
) -> Result<Assignment, String> {
    let topology = Topology::new(graph, rings)?;
    if directions.len() != graph.bonds.len() {
        return Err("Kekulé bond direction count changed".into());
    }
    let ranks = if let Some(ranks) = options.ranks {
        if ranks.len() != graph.atoms.len()
            || ranks.iter().any(|&r| r as usize >= graph.atoms.len())
        {
            return Err("Invalid Kekulé atom ranks".into());
        }
        ranks.to_vec()
    } else {
        (0..graph.atoms.len() as u32).collect()
    };
    let mut result = Assignment {
        graph: graph.clone(),
        directions: directions.to_vec(),
    };
    let aromatic = topology.aromatic_atoms(graph)?;
    if !aromatic.iter().any(|&a| a) && !graph.bonds.iter().any(|b| b.aromatic) {
        return Ok(result);
    }
    let mut wedged_atoms = vec![false; graph.atoms.len()];
    for (bond, &dir) in graph.bonds.iter().zip(directions) {
        if dir.wedged() {
            set(&mut wedged_atoms, bond.a, true)?;
        }
    }
    let mut reordered = VecDeque::new();
    for (id, ring) in rings.iter().enumerate() {
        let mut ordinary = false;
        let mut start = None;
        for (i, &atom) in ring.iter().enumerate() {
            ordinary |= at(&graph.atoms, atom)?.atomic_number != 0;
            if start.is_none() && *at(&wedged_atoms, atom)? {
                start = Some(i);
            }
        }
        if ordinary {
            let mut rotated = ring.clone();
            if let Some(start) = start {
                rotated.rotate_left(start);
                reordered.push_front((id, rotated));
            } else {
                reordered.push_back((id, rotated));
            }
        }
    }
    let (original_ids, ordered): (Vec<_>, Vec<_>) = reordered.into_iter().unzip();
    let neighbors = fused_neighbors(&ordered, &topology, &original_ids, work)?;
    let mut visited = vec![false; ordered.len()];
    for root in 0..ordered.len() {
        if *at(&visited, root)? {
            continue;
        }
        let mut stack = vec![root];
        let mut atoms = Vec::new();
        let mut seen_atoms = HashSet::new();
        while let Some(ring) = stack.pop() {
            work.spend(1)?;
            if *at(&visited, ring)? {
                continue;
            }
            set(&mut visited, ring, true)?;
            for &atom in at(&ordered, ring)? {
                work.spend(1)?;
                if seen_atoms.insert(atom) {
                    atoms.push(atom);
                }
            }
            stack.extend(at(&neighbors, ring)?.iter().rev().copied());
        }
        let candidates = candidates(&mut result.graph, &atoms, rings, &topology, work)?;
        search::fused(
            &mut result,
            &atoms,
            candidates,
            &topology,
            &ranks,
            options.max_backtracks,
            work,
        )?;
    }
    if options.clear_aromaticity {
        for bond in &mut result.graph.bonds {
            bond.aromatic = false;
        }
        let mut refreshed = Vec::new();
        for (id, atom) in result.graph.atoms.iter_mut().enumerate() {
            if !atom.aromatic {
                continue;
            }
            if at(&topology.members, id)?.is_empty() {
                return Err(format!("Non-ring atom {} marked aromatic", id + 1));
            }
            atom.aromatic = false;
            if matches!(atom.atomic_number, 7 | 15)
                && atom.charge == 0
                && atom.explicit_hydrogens == 1
            {
                atom.no_implicit = false;
                atom.explicit_hydrogens = 0;
                refreshed.push(id);
            }
        }
        // The reference retains its original cache except for these N/P atoms.
        // Compare only cache entries it actually recalculates in this pass.
        if !refreshed.is_empty() {
            let after = result.graph.provisional_valences()?;
            for id in refreshed {
                let old = at(&topology.valences, id)?;
                let new = at(&after, id)?;
                if old.explicit_valence + old.implicit_hydrogens
                    != new.explicit_valence + new.implicit_hydrogens
                {
                    return Err("Kekulé assignment changed atom valence".into());
                }
            }
        }
    }
    Ok(result)
}
