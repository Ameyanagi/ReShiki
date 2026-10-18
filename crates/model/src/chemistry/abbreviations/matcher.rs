//! Iterative unsorted VF2 traversal for the connected, fixed preset queries.
//! Original target adjacency order and unique target atom sets determine output.
use super::{Error, Preset, Result, at, invalid};
use crate::chemistry::{
    graph::Graph,
    rings,
    stereo::perception::{self, RingCache, RingKind},
};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(super) struct Atom {
    pub(super) number: u8,
    aromatic: Option<bool>,
    pub(super) charge: Option<i8>,
    degree: usize,
    rings: usize,
    /// RDKit's bracket hydrogen count ([nH]); absent from bundled queries.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) hydrogens: Option<u8>,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(super) struct Bond {
    pub(super) a: usize,
    pub(super) b: usize,
    /// Zero is the implicit SMARTS single-or-aromatic predicate.
    order: u8,
}
/// Elements that RDKit's SMARTS reader types as aliphatic or aromatic even in
/// brackets. Other bracket elements (Si, Sn, Mg...) match on atomic number only.
const TYPED: [u8; 10] = [5, 6, 7, 8, 9, 15, 16, 17, 35, 53];

/// Derive the adjusted abbreviation query from a prepared `*`-first SMILES.
/// This reproduces RDKit ParseAbbreviations for the catalog's SMILES style:
/// written aromaticity, charge only when nonzero, explicit degree and ring
/// count. Implicit single-or-aromatic bonds become order zero; a single bond
/// between two aromatic atoms (the fluorene biaryl bond) stays an explicit single.
/// Bracket hydrogens become a total hydrogen count, as in RDKit.
pub(super) fn query(state: &perception::State) -> Result<(Vec<Atom>, Vec<Bond>)> {
    let graph = &state.graph;
    let mut degree = vec![0usize; graph.atoms.len()];
    for bond in &graph.bonds {
        for index in [bond.a, bond.b] {
            *degree
                .get_mut(index)
                .ok_or_else(|| invalid("Invalid preset endpoint"))? += 1;
        }
    }
    let mut rings = vec![0usize; graph.atoms.len()];
    for ring in &state.rings.atoms {
        for &index in ring {
            *rings
                .get_mut(index)
                .ok_or_else(|| invalid("Invalid preset ring"))? += 1;
        }
    }
    let atoms = graph
        .atoms
        .iter()
        .enumerate()
        .map(|(index, atom)| {
            if index == 0 {
                return Atom {
                    number: 0,
                    aromatic: None,
                    charge: None,
                    degree: degree.first().copied().unwrap_or(0),
                    rings: 0,
                    hydrogens: None,
                };
            }
            Atom {
                number: atom.atomic_number,
                aromatic: (atom.aromatic || TYPED.contains(&atom.atomic_number))
                    .then_some(atom.aromatic),
                charge: (atom.charge != 0).then_some(atom.charge),
                // RDKit keeps a bracket hydrogen count ([nH]) as a total
                // hydrogen query; it tells apart tautomers and anions.
                hydrogens: (atom.explicit_hydrogens > 0).then_some(atom.explicit_hydrogens),
                degree: degree.get(index).copied().unwrap_or(0),
                rings: rings.get(index).copied().unwrap_or(0),
            }
        })
        .collect();
    let bonds = graph
        .bonds
        .iter()
        .map(|bond| {
            let aromatic = |index: usize| {
                index != 0 && graph.atoms.get(index).is_some_and(|atom| atom.aromatic)
            };
            let order = match bond.order {
                4 => 0,
                1 if aromatic(bond.a) && aromatic(bond.b) => 1,
                1 => 0,
                order @ (2 | 3) => order,
                _ => return Err(invalid("Unsupported preset bond order")),
            };
            Ok(Bond {
                a: bond.a,
                b: bond.b,
                order,
            })
        })
        .collect::<Result<_>>()?;
    Ok((atoms, bonds))
}

pub(super) fn validate(preset: &Preset) -> Result<()> {
    if !(2..=32).contains(&preset.atoms.len())
        || preset.bonds.len() > 40
        || at(&preset.atoms, 0)?.number != 0
    {
        return Err(invalid("Invalid preset query size"));
    }
    let mut degree = vec![0; preset.atoms.len()];
    let mut pairs = HashSet::new();
    for bond in &preset.bonds {
        if bond.a == bond.b
            || bond.order > 3
            || !pairs.insert((bond.a.min(bond.b), bond.a.max(bond.b)))
        {
            return Err(invalid("Invalid preset query bond"));
        }
        for index in [bond.a, bond.b] {
            *degree
                .get_mut(index)
                .ok_or_else(|| invalid("Invalid preset endpoint"))? += 1;
        }
    }
    for (index, atom) in preset.atoms.iter().enumerate() {
        if atom.degree != *at(&degree, index)?
            || atom.number > 118
            || atom.rings > 4
            || index != 0
                && (atom.number == 0
                    || !preset
                        .bonds
                        .iter()
                        .any(|b| b.a == index && b.b < index || b.b == index && b.a < index))
        {
            return Err(invalid("Invalid preset atom or traversal"));
        }
    }
    if *at(&degree, 0)? != 1 || !pairs.contains(&(0, 1)) {
        return Err(invalid("Invalid preset attachment"));
    }
    Ok(())
}

pub(super) struct Work(usize);
impl Default for Work {
    fn default() -> Self {
        Self(100_000_000)
    }
}
impl Work {
    fn spend(&mut self, count: usize) -> Result<()> {
        self.0 = self.0.checked_sub(count).ok_or(Error::WorkLimit)?;
        Ok(())
    }
}

pub(super) struct Target<'a> {
    graph: &'a Graph,
    adjacent: Vec<Vec<usize>>,
    bonds: HashMap<(usize, usize), u8>,
    ring_counts: Vec<usize>,
    hydrogens: &'a [u32],
}
impl<'a> Target<'a> {
    /// `hydrogens` holds each atom's total (explicit plus implicit) count.
    pub(super) fn new(graph: &'a Graph, cache: &RingCache, hydrogens: &'a [u32]) -> Result<Self> {
        graph.validate().map_err(invalid)?;
        if hydrogens.len() != graph.atoms.len() {
            return Err(invalid("Hydrogen counts changed"));
        }
        let mut adjacent = vec![Vec::new(); graph.atoms.len()];
        let mut bonds = HashMap::new();
        for bond in &graph.bonds {
            for (atom, other) in [(bond.a, bond.b), (bond.b, bond.a)] {
                adjacent
                    .get_mut(atom)
                    .ok_or_else(|| invalid("Missing bond endpoint"))?
                    .push(other);
                bonds.insert((atom, other), bond.order);
            }
        }
        let fast;
        let rings = if cache.kind == RingKind::None {
            fast = rings::fast(graph).map_err(|error| invalid(error.to_string()))?;
            &fast.atoms
        } else {
            &cache.atoms
        };
        let mut total = 0usize;
        let mut ring_counts = vec![0; graph.atoms.len()];
        for ring in rings {
            total = total
                .checked_add(ring.len())
                .ok_or_else(|| invalid("Ring storage limit exceeded"))?;
            if ring.len() < 3
                || total > 2_000_000
                || ring.iter().copied().collect::<HashSet<_>>().len() != ring.len()
            {
                return Err(invalid("Invalid ring membership"));
            }
            for (&atom, &next) in ring.iter().zip(ring.iter().cycle().skip(1)) {
                if !bonds.contains_key(&(atom, next)) {
                    return Err(invalid("Missing ring bond"));
                }
                *ring_counts
                    .get_mut(atom)
                    .ok_or_else(|| invalid("Missing ring atom"))? += 1;
            }
        }
        Ok(Self {
            graph,
            adjacent,
            bonds,
            ring_counts,
            hydrogens,
        })
    }

    fn atom_matches(&self, query: &Atom, target: usize, dummy: bool) -> Result<bool> {
        let atom = at(&self.graph.atoms, target)?;
        let hydrogens = *at(self.hydrogens, target)?;
        if dummy {
            return Ok(atom.atomic_number != 0);
        }
        Ok(query.number == atom.atomic_number
            && query.aromatic.is_none_or(|value| value == atom.aromatic)
            && query.charge.is_none_or(|value| value == atom.charge)
            && query.degree == at(&self.adjacent, target)?.len()
            && query.rings == *at(&self.ring_counts, target)?
            && query
                .hydrogens
                .is_none_or(|count| u32::from(count) == hydrogens))
    }

    pub(super) fn matches(&self, preset: &Preset, work: &mut Work) -> Result<Vec<Vec<usize>>> {
        let mut mapping = vec![None; preset.atoms.len()];
        let mut frames = vec![Candidates::All(0)];
        let mut unique = HashSet::new();
        let mut result = Vec::new();
        while let Some(frame) = frames.last_mut() {
            work.spend(1)?;
            let candidate = frame.next(self)?;
            let depth = frames.len() - 1;
            *mapping
                .get_mut(depth)
                .ok_or_else(|| invalid("Missing query mapping"))? = None;
            let Some(candidate) = candidate else {
                frames.pop();
                continue;
            };
            if mapping.contains(&Some(candidate))
                || !self.atom_matches(at(&preset.atoms, depth)?, candidate, depth == 0)?
            {
                continue;
            }
            let mut compatible = true;
            for edge in &preset.bonds {
                let other = if edge.a == depth {
                    edge.b
                } else if edge.b == depth {
                    edge.a
                } else {
                    continue;
                };
                let Some(mapped) = *at(&mapping, other)? else {
                    continue;
                };
                work.spend(1)?;
                let Some(&order) = self.bonds.get(&(candidate, mapped)) else {
                    compatible = false;
                    break;
                };
                if !(edge.order == 0 && matches!(order, 1 | 4)
                    || edge.order != 0 && edge.order == order)
                {
                    compatible = false;
                    break;
                }
            }
            if !compatible {
                continue;
            }
            *mapping
                .get_mut(depth)
                .ok_or_else(|| invalid("Missing query mapping"))? = Some(candidate);
            if depth + 1 == mapping.len() {
                let complete = mapping
                    .iter()
                    .copied()
                    .map(|atom| atom.ok_or_else(|| invalid("Incomplete query mapping")))
                    .collect::<Result<Vec<_>>>()?;
                let mut key = complete.clone();
                key.sort_unstable();
                if unique.insert(key) {
                    result.push(complete);
                    // This is the native SubstructMatch default, not our safety budget.
                    if result.len() == 1000 {
                        break;
                    }
                }
            } else {
                let next = depth + 1;
                // Every pinned query introduces the next lowest vertex from an
                // earlier one. The first query bond controls adjacency order.
                let parent = preset
                    .bonds
                    .iter()
                    .find_map(|edge| {
                        let other = if edge.a == next {
                            edge.b
                        } else if edge.b == next {
                            edge.a
                        } else {
                            return None;
                        };
                        mapping.get(other).copied().flatten()
                    })
                    .ok_or_else(|| invalid("Disconnected query traversal"))?;
                frames.push(Candidates::Neighbors(parent, 0));
            }
        }
        Ok(result)
    }
}
enum Candidates {
    All(usize),
    Neighbors(usize, usize),
}
impl Candidates {
    fn next(&mut self, target: &Target<'_>) -> Result<Option<usize>> {
        match self {
            Self::All(index) => {
                if *index >= target.graph.atoms.len() {
                    return Ok(None);
                }
                let result = *index;
                *index += 1;
                Ok(Some(result))
            }
            Self::Neighbors(atom, index) => {
                let result = at(&target.adjacent, *atom)?.get(*index).copied();
                if result.is_some() {
                    *index += 1;
                }
                Ok(result)
            }
        }
    }
}
