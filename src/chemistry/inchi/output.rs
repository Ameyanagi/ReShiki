//! Native InChI output records and detached reconstruction stages.
//!
//! Adapted from RDKit External/INCHI-API/inchi.cpp, InchiToMol.
//! Copyright (C) 2011-2025 Novartis Institutes for BioMedical Research Inc. and
//! other RDKit contributors. BSD-3-Clause; see licenses/rdkit/LICENSE.
mod assembly;
mod cleanup;
mod finish;

use crate::chemistry::{graph::Graph, stereo::perception::State};
use serde::{Deserialize, Serialize};

pub use assembly::{assemble, topology};
pub use cleanup::clean_up;
pub use finish::{Options, prepare, reconstruct};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bond {
    pub neighbor: i16,
    pub kind: i8,
    pub stereo: i8,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Atom {
    /// Preserved native fields; InchiToMol does not create a conformer.
    pub position: [f64; 3],
    pub element: String,
    pub isotopic_mass: i16,
    pub charge: i8,
    pub hydrogens: [i8; 4],
    pub radical: i8,
    /// Native adjacency order, including reverse/duplicate bond entries.
    pub bonds: Vec<Bond>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stereo {
    /// None represents the native NO_ATOM (-1) sentinel.
    pub central_atom: Option<i16>,
    pub neighbors: [i16; 4],
    pub kind: i8,
    pub parity: i8,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Output {
    pub status: i32,
    pub message: String,
    pub log: String,
    /// Native unsigned-long masks, widened across supported ABIs. RDKit's
    /// adapter does not consume these fields, but the transport retains them.
    pub warning_flags: [[u64; 2]; 2],
    pub atoms: Vec<Atom>,
    pub stereo: Vec<Stereo>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid InChI output: {0}")]
    Invalid(String),
    #[error("InChI reconstruction exceeds {0}")]
    Limit(&'static str),
    #[error("InChI reconstruction chemistry: {0}")]
    Chemistry(String),
    #[error("InChI atom cache exceeds the native signed-byte valence range")]
    NativeCacheBoundary,
    #[error(
        "Repeated InChI stereo records exceed the molecular metadata boundary at bond {bond} ({indices} atom indices)"
    )]
    RepeatedStereoRecords { bond: usize, indices: usize },
    #[error(transparent)]
    Sanitization(#[from] crate::chemistry::sanitize::Error),
    #[error(transparent)]
    Hydrogens(#[from] crate::chemistry::hydrogens::Error),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Warning {
    IgnoredRadical { atom: usize, value: i8 },
    AromaticBond { atom: usize, neighbor: usize },
    IllegalBond { atom: usize, value: i8 },
    ExtendedDoubleBond,
    MissingStereoNeighbors,
    IgnoredAllene,
    UnknownStereoType(i8),
    ConflictingBondDirections,
}

#[derive(Clone, Debug, Serialize)]
pub struct Assembly {
    /// None has the same meaning as the native null molecule, including a
    /// failed kernel status or an illegal bond type. Errors describe malformed
    /// records, resource limits, or native graph exceptions separately.
    pub state: Option<State>,
    pub warnings: Vec<Warning>,
    /// Native UNSPECIFIED bonds contribute zero valence like graph order zero,
    /// but must remain distinct from the graph's HYDROGEN bond identity.
    pub unspecified_bonds: Vec<bool>,
}

fn invalid(message: impl Into<String>) -> Error {
    Error::Invalid(message.into())
}
fn at<T>(items: &[T], index: usize) -> Result<&T, Error> {
    items
        .get(index)
        .ok_or_else(|| invalid("Missing graph item"))
}
fn at_mut<T>(items: &mut [T], index: usize) -> Result<&mut T, Error> {
    items
        .get_mut(index)
        .ok_or_else(|| invalid("Missing graph item"))
}
fn index(value: i16, count: usize) -> Result<usize, Error> {
    usize::try_from(value)
        .ok()
        .filter(|&i| i < count)
        .ok_or_else(|| invalid("Missing native atom"))
}

struct Work(usize);
impl Work {
    fn spend(&mut self, amount: usize) -> Result<(), Error> {
        self.0 = self
            .0
            .checked_sub(amount)
            .ok_or(Error::Limit("the reconstruction work budget"))?;
        Ok(())
    }
}

fn validate_assembly(input: &Assembly) -> Result<(), Error> {
    if input.warnings.len() > 1_000_000 || input.unspecified_bonds.len() > 300_000 {
        return Err(Error::Limit("the assembly annotation size"));
    }
    let Some(state) = &input.state else {
        return Ok(());
    };
    state
        .graph
        .cached_valences(Some(&state.valences))
        .map_err(Error::Chemistry)?;
    let (n, b) = (state.graph.atoms.len(), state.graph.bonds.len());
    if input.unspecified_bonds.len() != b
        || state.directions.len() != b
        || state.conjugated.len() != b
        || state.hybridizations.len() != n
        || state.properties.atoms.len() != n
        || state.properties.bond_codes.len() != b
        || state.metadata.atoms.len() != n
        || state.metadata.bonds.len() != b
    {
        return Err(invalid("Molecular annotation dimensions differ from graph"));
    }
    for (bond, metadata) in state.metadata.bonds.iter().enumerate() {
        if metadata.stereo_atoms.len() > 2 {
            return Err(Error::RepeatedStereoRecords {
                bond,
                indices: metadata.stereo_atoms.len(),
            });
        }
    }
    state
        .metadata
        .validate(&state.graph)
        .map_err(Error::Chemistry)?;
    let mut work = Work(2_000_000);
    work.spend(state.rings.atoms.len())?;
    for ring in &state.rings.atoms {
        work.spend(ring.len())?;
    }
    for atom in &state.properties.atoms {
        if let Some(code) = &atom.cip_code {
            work.spend(code.len())?;
        }
        if let Some(members) = &atom.ring_members {
            work.spend(members.len())?;
        }
    }
    for code in state.properties.bond_codes.iter().flatten() {
        work.spend(code.len())?;
    }
    Ok(())
}
struct Topology {
    edges: Vec<Vec<(usize, usize)>>,
}
impl Topology {
    fn new(graph: &Graph) -> Result<Self, Error> {
        let mut edges = vec![Vec::new(); graph.atoms.len()];
        for (id, b) in graph.bonds.iter().enumerate() {
            at_mut(&mut edges, b.a)?.push((b.b, id));
            at_mut(&mut edges, b.b)?.push((b.a, id));
        }
        Ok(Self { edges })
    }
    fn bond(&self, a: usize, b: usize) -> Result<Option<usize>, Error> {
        Ok(at(&self.edges, a)?
            .iter()
            .find_map(|&(other, bond)| (other == b).then_some(bond)))
    }
}
