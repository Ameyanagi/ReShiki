//! Prepare the molecular graph from an editable drawing without changing it.
//! Display labels and cached hydrogen/CIP labels are not chemical input.
use super::{
    ELEMENTS, RDKIT_VERSION,
    graph::{Atom, Bond, Graph},
    kekulize::Direction,
    ranking::Metadata,
    sanitize,
    stereo::{self, Point3, perception},
};
use crate::document::Document;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
mod aromatic;
pub use aromatic::{Aromatic, Identity, aromatic_display};
mod output;
pub use output::{BondLabel, Drawing, Labels, for_drawing, for_import};
pub(crate) use output::{kekule, validate_molecule};

#[cfg(test)]
mod tests;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid drawing: {0}")]
    Drawing(String),
    #[error(transparent)]
    Sanitization(#[from] sanitize::Error),
    #[error("Drawing stereochemistry: {0}")]
    Stereo(String),
    #[error("Kekulé drawing bonds: {0}")]
    BondAssignment(String),
    #[error("Drawing wedge assignment: {0}")]
    WedgeAssignment(String),
    #[error("Ring display would change the molecular identity")]
    IdentityChanged,
}

/// Detached chemical state, with stable drawing IDs and Y-up coordinates.
#[derive(Clone, Debug, Serialize)]
pub struct Molecule {
    pub rdkit_version: &'static str,
    pub ids: Vec<u64>,
    pub positions: Vec<Point3>,
    pub state: perception::State,
}

struct Input {
    graph: Graph,
    metadata: Metadata,
    directions: Vec<Direction>,
    positions: Vec<Point3>,
    indices: HashMap<u64, usize>,
}
fn invalid(message: impl Into<String>) -> Error {
    Error::Drawing(message.into())
}
fn at<T>(items: &[T], index: usize) -> Result<&T, Error> {
    items
        .get(index)
        .ok_or_else(|| invalid("Missing graph item"))
}
fn index(indices: &HashMap<u64, usize>, id: u64) -> Result<usize, Error> {
    indices
        .get(&id)
        .copied()
        .ok_or_else(|| invalid("Missing atom ID"))
}

// Cycle parity avoids quadratic inversion counting for high-degree centers.
fn winding(current: &[u64], given: &[u64], clockwise: bool) -> Result<u8, Error> {
    let positions: HashMap<_, _> = given.iter().enumerate().map(|(i, &id)| (id, i)).collect();
    if current.len() != given.len() || positions.len() != given.len() {
        return Err(invalid("Stereocenter neighbor mapping changed"));
    }
    let permutation = current
        .iter()
        .map(|&id| index(&positions, id))
        .collect::<Result<Vec<_>, _>>()?;
    let mut visited = vec![false; permutation.len()];
    let mut odd = false;
    for start in 0..permutation.len() {
        if *at(&visited, start)? {
            continue;
        }
        let mut i = start;
        let mut length = 0;
        while !*at(&visited, i)? {
            *visited
                .get_mut(i)
                .ok_or_else(|| invalid("Missing winding index"))? = true;
            length += 1;
            i = *at(&permutation, i)?;
        }
        odd ^= length % 2 == 0;
    }
    Ok(if clockwise ^ odd { 1 } else { 2 })
}

fn build(document: &Document) -> Result<Input, Error> {
    if document.atoms.len() > 100_000 || document.bonds.len() > 300_000 {
        return Err(invalid("Molecular graph exceeds the atom or bond limit"));
    }
    document.validate().map_err(Error::Drawing)?;
    let indices = document
        .atoms
        .iter()
        .enumerate()
        .map(|(i, a)| (a.id, i))
        .collect::<HashMap<_, _>>();
    let atoms = document
        .atoms
        .iter()
        .map(|a| {
            let number = ELEMENTS
                .iter()
                .position(|e| e.symbol == a.element)
                .ok_or_else(|| invalid(format!("Unknown element {}", a.element)))?;
            Ok(Atom {
                atomic_number: u8::try_from(number)
                    .map_err(|_| invalid("Invalid atomic number"))?,
                isotope: u16::try_from(a.isotope)
                    .map_err(|_| invalid("Isotope exceeds its supported range"))?,
                charge: i8::try_from(a.charge)
                    .map_err(|_| invalid("Charge exceeds its supported range"))?,
                explicit_hydrogens: u8::try_from(a.explicit_h)
                    .map_err(|_| invalid("Explicit hydrogen count exceeds its supported range"))?,
                no_implicit: a.no_implicit,
                aromatic: a.aromatic,
                radical_electrons: a.radical_electrons,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let bonds = document
        .bonds
        .iter()
        .map(|b| {
            Ok(Bond {
                a: index(&indices, b.a)?,
                b: index(&indices, b.b)?,
                order: b.order,
                aromatic: b.order == 4,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let mut graph = Graph { atoms, bonds };
    graph.validate().map_err(Error::Drawing)?;
    let mut neighbors = vec![Vec::new(); graph.atoms.len()];
    let mut single = vec![false; graph.atoms.len()];
    for b in &graph.bonds {
        for (a, other) in [(b.a, b.b), (b.b, b.a)] {
            neighbors
                .get_mut(a)
                .ok_or_else(|| invalid("Missing neighbors"))?
                .push(at(&document.atoms, other)?.id);
            if b.order == 1 {
                *single
                    .get_mut(a)
                    .ok_or_else(|| invalid("Missing single bond flag"))? = true;
            }
            if b.order == 4 {
                graph
                    .atoms
                    .get_mut(a)
                    .ok_or_else(|| invalid("Missing aromatic atom"))?
                    .aromatic = true;
            }
        }
    }
    for b in &graph.bonds {
        if b.order == 0
            && (at(&graph.atoms, b.a)?.atomic_number != 1
                || !matches!(at(&graph.atoms, b.b)?.atomic_number, 7 | 8 | 9 | 16)
                || at(&graph.atoms, b.b)?.charge > 0
                || !*at(&single, b.a)?)
        {
            return Err(invalid(
                "A hydrogen bond must start at a covalently bound explicit H and end at an acceptor (N, O, F or S)",
            ));
        }
    }
    let mut metadata = Metadata::unspecified(&graph);
    for ((a, m), neighbors) in document
        .atoms
        .iter()
        .zip(&mut metadata.atoms)
        .zip(neighbors)
    {
        m.map_number = i32::try_from(a.map_num)
            .map_err(|_| invalid("Atom map exceeds its supported range"))?;
        m.map_present = a.map_num != 0;
        if let Some(stereo) = &a.stereo {
            m.chiral_tag = winding(&neighbors, &stereo.neighbors, stereo.winding == "cw")?;
        }
    }
    let directions = document
        .bonds
        .iter()
        .map(|b| {
            if b.order == 1 || b.order == 2 && b.display == "wavy" {
                match b.display.as_str() {
                    "wedge" | "hollow_wedge" | "bold" => Direction::Wedge,
                    "hash" | "hashed" => Direction::Hash,
                    "wavy" => Direction::Unknown,
                    _ => Direction::None,
                }
            } else {
                Direction::None
            }
        })
        .collect();
    let positions = document
        .atoms
        .iter()
        .map(|a| Point3 {
            x: f64::from(a.position.x) / 28.0,
            y: -f64::from(a.position.y) / 28.0,
            z: 0.0,
        })
        .collect();
    Ok(Input {
        graph,
        metadata,
        directions,
        positions,
        indices,
    })
}

/// Restore explicit winding, sanitize, read drawing geometry and assign the
/// reference's legacy stereo. Full CIP labeling and identifiers are separate.
/// Errors retain their stage/cause; the original document is never changed.
pub fn prepare(document: &Document) -> Result<Molecule, Error> {
    let input = build(document)?;
    let sanitized = sanitize::sanitize(&input.graph, &input.metadata, &input.directions)?;
    for (requested, actual) in document.atoms.iter().zip(&sanitized.graph.atoms) {
        if requested.radical_electrons != 0
            && requested.radical_electrons != actual.radical_electrons
        {
            return Err(invalid(
                "Radical count conflicts with the atom valence or explicit hydrogens",
            ));
        }
    }
    let mut drawn = stereo::from_directions(
        &sanitized.graph,
        &sanitized.metadata,
        &sanitized.directions,
        Some(&input.positions),
        false,
    )
    .map_err(Error::Stereo)?;
    let edges: HashSet<_> = drawn
        .graph
        .bonds
        .iter()
        .flat_map(|b| [(b.a, b.b), (b.b, b.a)])
        .collect();
    for ((item, meta), bond) in document
        .bonds
        .iter()
        .zip(&mut drawn.metadata.bonds)
        .zip(&drawn.graph.bonds)
    {
        let code = match item.stereo.as_deref() {
            Some("z") => Some(2),
            Some("e") => Some(3),
            Some("cis") => Some(4),
            Some("trans") => Some(5),
            _ => None,
        };
        if let Some(code) = code {
            let controls = item
                .stereo_atoms
                .iter()
                .map(|&id| index(&input.indices, id))
                .collect::<Result<Vec<_>, _>>()?;
            if controls.len() != 2
                || !edges.contains(&(bond.a, *at(&controls, 0)?))
                || !edges.contains(&(bond.b, *at(&controls, 1)?))
            {
                return Err(invalid(
                    "Bond stereo references must be attached to the corresponding endpoints",
                ));
            }
            meta.stereo_atoms = controls;
            meta.stereo = code;
        } else if item.order == 2 && item.display == "wavy" {
            meta.stereo = 1;
        }
    }
    let geometry = stereo::detect_bond_stereo(
        &drawn.graph,
        &drawn.metadata,
        &sanitized.directions,
        Some(&input.positions),
        &sanitized.rings,
    )
    .map_err(Error::Stereo)?;
    let properties = perception::Properties::unspecified(&drawn.graph);
    let state = perception::perceive(
        &perception::State {
            graph: drawn.graph,
            metadata: geometry.metadata,
            directions: geometry.directions,
            valences: drawn.valences,
            conjugated: sanitized.conjugated,
            hybridizations: sanitized.hybridizations,
            rings: perception::RingCache {
                kind: perception::RingKind::Symmetric,
                atoms: sanitized.rings,
            },
            properties,
        },
        perception::Options {
            clean: false,
            force: true,
            flag_possible: false,
        },
    )
    .map_err(Error::Stereo)?;
    if state.graph.atoms.iter().any(|a| a.radical_electrons > 2)
        || state.metadata.atoms.iter().any(|a| a.chiral_tag > 2)
        || state.metadata.bonds.iter().any(|b| b.stereo > 5)
        || !state.metadata.groups.is_empty()
    {
        return Err(invalid(
            "This radical count or stereochemistry class is not supported yet",
        ));
    }
    Ok(Molecule {
        rdkit_version: RDKIT_VERSION,
        ids: document.atoms.iter().map(|a| a.id).collect(),
        positions: input.positions,
        state,
    })
}
