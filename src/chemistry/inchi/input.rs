//! Owned input for the InChI 1.07.3 generator; no kernel or FFI is called here.
//!
//! Adapted from RDKit External/INCHI-API/inchi.cpp, MolToInchi and rCleanUp.
//! Copyright (C) 2011-2025 Novartis Institutes for BioMedical Research Inc. and
//! other RDKit contributors. BSD-3-Clause; see licenses/rdkit/INCHI-ADAPTER.
use crate::chemistry::{
    ELEMENTS,
    graph::{Graph, Valence},
    kekulize::{self, Direction},
    rings,
    stereo::{
        Point3,
        perception::{RingKind, State},
    },
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

const MAX_ATOMS: usize = i16::MAX as usize;
const MAX_NEIGHBORS: usize = 20;

/// A checked equivalent of a native adapter return before the kernel is called.
/// Its original public result is an empty identifier. The adapter never writes
/// the native return-code field on this path, so there is no defined status.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum NativeEmpty {
    #[error("Atom {atom} exceeds {stored_bonds} stored neighbors")]
    TooManyNeighbors { atom: usize, stored_bonds: usize },
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Native InChI adapter returned an empty identifier: {0}")]
    NativeEmpty(NativeEmpty),
    #[error("Invalid InChI input: {0}")]
    Invalid(String),
    #[error("InChI input exceeds {0}")]
    Limit(&'static str),
    #[error("InChI Kekulé assignment failed: {0}")]
    Kekule(String),
    #[error(transparent)]
    Rings(#[from] rings::RingError),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bond {
    pub neighbor: i16,
    pub kind: i8,
    pub stereo: i8,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Atom {
    pub position: [f64; 3],
    pub element: String,
    pub isotopic_mass: i16,
    pub charge: i8,
    pub hydrogens: [i8; 4],
    pub radical: i8,
    /// Each undirected bond occurs once, on its lower-indexed atom. Order is
    /// the original bond insertion order, not sorted neighbor order.
    pub bonds: Vec<Bond>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stereo {
    /// None is InChI's NO_ATOM (-1), used by double-bond records.
    pub central_atom: Option<i16>,
    pub neighbors: [i16; 4],
    /// Native InChI code: 1 double bond, 2 tetrahedral.
    pub kind: i8,
    /// Native InChI code: 1 odd, 2 even, 3 unknown.
    pub parity: i8,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Input {
    /// Preserve whether an all-zero array came from an actual conformer.
    pub has_coordinates: bool,
    pub atoms: Vec<Atom>,
    pub stereo: Vec<Stereo>,
}

fn invalid(message: impl Into<String>) -> Error {
    Error::Invalid(message.into())
}
fn at<T>(values: &[T], index: usize) -> Result<&T, Error> {
    values
        .get(index)
        .ok_or_else(|| invalid("Missing graph item"))
}
fn at_mut<T>(values: &mut [T], index: usize) -> Result<&mut T, Error> {
    values
        .get_mut(index)
        .ok_or_else(|| invalid("Missing graph item"))
}
fn atom_id(index: usize) -> Result<i16, Error> {
    i16::try_from(index).map_err(|_| Error::Limit("the signed 16-bit atom index range"))
}

struct Topology {
    neighbors: Vec<Vec<usize>>,
    edges: Vec<Vec<(usize, usize)>>,
}
impl Topology {
    fn new(graph: &Graph) -> Result<Self, Error> {
        let mut neighbors = vec![Vec::new(); graph.atoms.len()];
        let mut edges = vec![Vec::new(); graph.atoms.len()];
        for (id, bond) in graph.bonds.iter().enumerate() {
            for (a, b) in [(bond.a, bond.b), (bond.b, bond.a)] {
                at_mut(&mut neighbors, a)?.push(b);
                at_mut(&mut edges, a)?.push((b, id));
            }
        }
        Ok(Self { neighbors, edges })
    }
}

/// Prepare a detached molecule for the pinned native generator. `positions` is
/// its first conformer, in molecule coordinates, or None if no conformer exists.
/// Graph caches and ring order must belong to `state`. The source is unchanged.
/// The generator's chemical acceptance and 1,023-atom limit are separate from
/// this adapter's signed 16-bit storage limit.
pub fn prepare(state: &State, positions: Option<&[Point3]>) -> Result<Input, Error> {
    let count = state.graph.atoms.len();
    if count > MAX_ATOMS {
        return Err(Error::Limit("the signed 16-bit input count range"));
    }
    state
        .graph
        .cached_valences(Some(&state.valences))
        .map_err(invalid)?;
    state.metadata.validate(&state.graph).map_err(invalid)?;
    if state.directions.len() != state.graph.bonds.len()
        || positions.is_some_and(|p| {
            p.len() != count
                || p.iter()
                    .any(|p| !p.x.is_finite() || !p.y.is_finite() || !p.z.is_finite())
        })
    {
        return Err(invalid("Coordinate or direction dimensions changed"));
    }
    let perceived;
    let ring_atoms = if state.rings.kind == RingKind::None {
        perceived = rings::perceive(&state.graph, rings::Options::default())?;
        perceived
            .atoms
            .get(..perceived.basis_count)
            .ok_or_else(|| invalid("Invalid ring basis"))?
    } else {
        &state.rings.atoms
    };
    let assignment = kekulize::assign_cached(
        &state.graph,
        ring_atoms,
        &state.directions,
        kekulize::Options {
            clear_aromaticity: false,
            ..Default::default()
        },
        &state.valences,
    )
    .map_err(Error::Kekule)?;
    let mut graph = assignment.assignment.graph;
    let topology = Topology::new(&graph)?;
    reverse_cleanup(&mut graph, &topology)?;
    let cache = assignment.cache;
    let refreshed = graph.refresh_implicit(&cache).map_err(invalid)?;
    let mut result = Input {
        has_coordinates: positions.is_some(),
        atoms: Vec::with_capacity(count),
        stereo: Vec::new(),
    };
    for (i, atom) in graph.atoms.iter().enumerate() {
        let element = ELEMENTS
            .get(usize::from(atom.atomic_number))
            .ok_or_else(|| invalid("Invalid element"))?;
        let isotopic_mass = if atom.isotope == 0 {
            0
        } else {
            // The native ABI narrows this expression to signed 16-bit storage.
            (10_000 + i32::from(atom.isotope) - (element.average + 0.5) as i32) as i16
        };
        let hydrogens = if atom.radical_electrons == 0
            && matches!(atom.atomic_number, 6 | 7 | 8 | 9 | 17 | 35 | 53)
        {
            -1
        } else {
            // Native num_iso_H is signed char. Preserve its defined stored
            // representation; unsupported chemistry is rejected by the kernel.
            (u32::from(atom.explicit_hydrogens) + at(&cache, i)?.implicit_hydrogens) as i8
        };
        let position = positions
            .map(|p| at(p, i).map(|p| [p.x, p.y, p.z]))
            .transpose()?
            .unwrap_or([0.0; 3]);
        result.atoms.push(Atom {
            position,
            element: element.symbol.into(),
            isotopic_mass,
            charge: atom.charge,
            hydrogens: [hydrogens, 0, 0, 0],
            radical: 0,
            bonds: Vec::new(),
        });
        let tag = at(&state.metadata.atoms, i)?.chiral_tag;
        if matches!(tag, 1 | 2) {
            add_tetrahedron(&graph, &topology, &refreshed, i, tag, &mut result.stereo)?;
        }
    }
    for (i, bond) in graph.bonds.iter().enumerate() {
        let (left, right, sign) = if bond.a < bond.b {
            (bond.a, bond.b, 1)
        } else {
            (bond.b, bond.a, -1)
        };
        let target = at_mut(&mut result.atoms, left)?;
        if target.bonds.len() >= MAX_NEIGHBORS {
            return Err(Error::NativeEmpty(NativeEmpty::TooManyNeighbors {
                atom: left,
                stored_bonds: target.bonds.len(),
            }));
        }
        let stereo = match at(&assignment.assignment.directions, i)? {
            Direction::Wedge => sign,
            Direction::Hash => sign * 6,
            Direction::EitherDouble => 3,
            Direction::Unknown => sign * 4,
            _ => 0,
        };
        target.bonds.push(Bond {
            neighbor: atom_id(right)?,
            kind: if (1..=3).contains(&bond.order) {
                bond.order as i8
            } else {
                0
            },
            stereo,
        });
        let metadata = at(&state.metadata.bonds, i)?;
        if metadata.stereo > 1 && metadata.stereo_atoms.len() >= 2 {
            let mut outside = [
                *at(&metadata.stereo_atoms, 0)?,
                *at(&metadata.stereo_atoms, 1)?,
            ];
            if !at(&topology.neighbors, left)?.contains(&outside[0]) {
                outside.swap(0, 1);
            }
            result.stereo.push(Stereo {
                central_atom: None,
                neighbors: [
                    atom_id(outside[0])?,
                    atom_id(left)?,
                    atom_id(right)?,
                    atom_id(outside[1])?,
                ],
                kind: 1,
                parity: if matches!(metadata.stereo, 2 | 4) {
                    1
                } else {
                    2
                },
            });
        } else if metadata.stereo == 1 {
            at_mut(&mut result.atoms, left)?.position = at(&result.atoms, right)?.position;
            let a = at(&topology.neighbors, left)?.iter().find(|&&a| a != right);
            let b = at(&topology.neighbors, right)?.iter().find(|&&a| a != left);
            if let (Some(&a), Some(&b)) = (a, b) {
                result.stereo.push(Stereo {
                    central_atom: None,
                    neighbors: [atom_id(a)?, atom_id(left)?, atom_id(right)?, atom_id(b)?],
                    kind: 1,
                    parity: 3,
                });
            }
        }
    }
    if result.stereo.len() > MAX_ATOMS {
        return Err(Error::Limit("the signed 16-bit stereo count range"));
    }
    Ok(result)
}

fn add_tetrahedron(
    graph: &Graph,
    topology: &Topology,
    cache: &[Valence],
    index: usize,
    tag: u8,
    result: &mut Vec<Stereo>,
) -> Result<(), Error> {
    let neighbors = at(&topology.neighbors, index)?;
    let total_degree = neighbors.len() as u32
        + u32::from(at(&graph.atoms, index)?.explicit_hydrogens)
        + at(cache, index)?.implicit_hydrogens;
    if !(3..=4).contains(&total_degree) {
        return Ok(());
    }
    // Lower graph degrees can pass the native total-degree check but leave
    // uninitialized neighbor slots. Never reproduce that undefined input.
    if !(3..=4).contains(&neighbors.len()) {
        return Err(invalid(
            "Tetrahedral annotation has fewer than three graph neighbors",
        ));
    }
    let ids = if neighbors.len() == 3 {
        [
            index,
            *at(neighbors, 0)?,
            *at(neighbors, 1)?,
            *at(neighbors, 2)?,
        ]
    } else {
        [
            *at(neighbors, 0)?,
            *at(neighbors, 1)?,
            *at(neighbors, 2)?,
            *at(neighbors, 3)?,
        ]
    };
    result.push(Stereo {
        central_atom: Some(atom_id(index)?),
        neighbors: [
            atom_id(ids[0])?,
            atom_id(ids[1])?,
            atom_id(ids[2])?,
            atom_id(ids[3])?,
        ],
        kind: 2,
        parity: if (neighbors.len() == 4) == (tag == 1) {
            2
        } else {
            1
        },
    });
    Ok(())
}

// The reference uses ordinary atom/bond matching of O(-)-Cl(+3)(-O(-))(-O(-))-O.
// Ordinary neutral O matches any oxygen charge; the three negative O atoms and
// chlorine require their exact nonzero charge. VF2 starts the first oxygen in
// atom order and follows the target's insertion-ordered neighbors thereafter.
// Save the unique atom sets (up to the native 1,000-match limit) before mutation.
fn reverse_cleanup(graph: &mut Graph, topology: &Topology) -> Result<(), Error> {
    let mut matches = Vec::new();
    let mut unique = BTreeSet::new();
    let mut work = 2_000_000usize;
    'search: for (first, atom) in graph.atoms.iter().enumerate() {
        if atom.atomic_number != 8 || atom.charge != -1 {
            continue;
        }
        for &(center, initial_bond) in at(&topology.edges, first)? {
            if at(&graph.atoms, center)?.atomic_number != 17
                || at(&graph.atoms, center)?.charge != 3
                || at(&graph.bonds, initial_bond)?.order != 1
            {
                continue;
            }
            let candidates = at(&topology.edges, center)?
                .iter()
                .copied()
                .filter(|&(n, b)| {
                    graph.atoms.get(n).is_some_and(|a| a.atomic_number == 8)
                        && graph.bonds.get(b).is_some_and(|b| b.order == 1)
                })
                .collect::<Vec<_>>();
            for &(second, _) in &candidates {
                if second == first || at(&graph.atoms, second)?.charge != -1 {
                    continue;
                }
                for &(third, _) in &candidates {
                    if third == first || third == second || at(&graph.atoms, third)?.charge != -1 {
                        continue;
                    }
                    for &(fourth, _) in &candidates {
                        work = work
                            .checked_sub(1)
                            .ok_or(Error::Limit("the cleanup matching work budget"))?;
                        if [first, second, third].contains(&fourth) {
                            continue;
                        }
                        let matched = [first, center, second, third, fourth];
                        let mut key = matched;
                        key.sort_unstable();
                        if unique.insert(key) {
                            matches.push(matched);
                            if matches.len() == 1000 {
                                break 'search;
                            }
                        }
                    }
                }
            }
        }
    }
    for matched in matches {
        let center = matched[1];
        if at(&graph.atoms, center)?.charge != 3 {
            break;
        }
        let mut neutral = None;
        for (slot, &id) in matched.iter().enumerate().filter(|(slot, _)| *slot != 1) {
            if at(&graph.atoms, id)?.charge == 0 {
                if neutral.is_some() {
                    return Ok(());
                }
                neutral = Some(slot);
            }
        }
        for (slot, &id) in matched.iter().enumerate().filter(|(slot, _)| *slot != 1) {
            if neutral == Some(slot) {
                continue;
            }
            let bond = at(&topology.edges, center)?
                .iter()
                .find(|&&(n, _)| n == id)
                .map(|&(_, b)| b)
                .ok_or_else(|| invalid("Missing cleanup bond"))?;
            let negative = neutral.is_none() && slot == 0;
            at_mut(&mut graph.bonds, bond)?.order = if negative { 1 } else { 2 };
            at_mut(&mut graph.atoms, id)?.charge = if negative { -1 } else { 0 };
        }
        at_mut(&mut graph.atoms, center)?.charge = 0;
    }
    Ok(())
}
