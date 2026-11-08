// SPDX-License-Identifier: MIT OR Apache-2.0
// Origin: original
// Source: ReShiki | crates/model/src/molecule_names/axis_markers.rs | 13660ba1bd3fa6cc15bca563607e98550412ffdb
// Changes: copy axis-marker helpers onto kernel State and forward an explicit Budget.

//! Private naming identity for CIP-labelled biaryl axes.
use super::{IdentityRejection, chemistry};
use crate::budget::Budget;
use chemistry::{electronic::Hybridization, stereo::cip::label, stereo::perception::State};
use std::collections::HashSet;

/// Reserved naming map for an M biaryl axis; both endpoints carry this map.
pub const AXIS_M: i32 = 900_004;
/// Reserved naming map for a P biaryl axis; both endpoints carry this map.
pub const AXIS_P: i32 = 900_008;
const REJECTED: IdentityRejection = IdentityRejection::NonclassicalStereo;

fn at<T>(items: &[T], index: usize) -> Result<&T, IdentityRejection> {
    items.get(index).ok_or(REJECTED)
}
fn at_mut<T>(items: &mut [T], index: usize) -> Result<&mut T, IdentityRejection> {
    items.get_mut(index).ok_or(REJECTED)
}

struct Axis {
    bond: usize,
    ends: [usize; 2],
    /// The lowest-index other neighbour at each end, the native CIP carrier.
    refs: [usize; 2],
}

/// Encoder and decoder admit exactly the same topology: an acyclic single
/// bond between two aromatic sp2 ring atoms that each have two other heavy
/// neighbours with distinct constitutional symmetry classes. Each atom
/// therefore ends at most one axis.
fn eligible_axes(state: &State, budget: &Budget) -> Result<Vec<Axis>, IdentityRejection> {
    let rings = chemistry::rings::fast(&state.graph).map_err(|_| REJECTED)?;
    let ring_atoms: HashSet<_> = rings.atoms.iter().flatten().copied().collect();
    let ring_bonds: HashSet<_> = rings.bonds.iter().flatten().copied().collect();
    let mut axes = Vec::new();
    'bonds: for (index, bond) in state.graph.bonds.iter().enumerate() {
        if bond.order != 1 || ring_bonds.contains(&index) {
            continue;
        }
        let mut refs = [0; 2];
        for (slot, (end, partner)) in [(bond.a, bond.b), (bond.b, bond.a)].into_iter().enumerate() {
            let mut others = vec![];
            for other in state.graph.bonds.iter().filter_map(|b| match (b.a, b.b) {
                (x, y) | (y, x) if x == end && y != partner => Some(y),
                _ => None,
            }) {
                if at(&state.graph.atoms, other)?.atomic_number > 1 {
                    others.push(other);
                }
            }
            if !at(&state.graph.atoms, end)?.aromatic
                || *at(&state.hybridizations, end)? != Hybridization::Sp2
                || !ring_atoms.contains(&end)
                || others.len() != 2
            {
                continue 'bonds;
            }
            *at_mut(&mut refs, slot)? = others.into_iter().min().ok_or(REJECTED)?;
        }
        if !chemistry::stereo::axis_ends_distinct(&state.graph, index, budget)
            .map_err(|_| REJECTED)?
        {
            continue;
        }
        axes.push(Axis {
            bond: index,
            ends: [bond.a, bond.b],
            refs,
        });
    }
    Ok(axes)
}

/// Replace ordinary atom maps and native axial winding with canonical-identity
/// markers. Only CIP-labelled M/P biaryl axes can cross this boundary. The
/// detached result retains other stereo, including explicitly unknown units.
pub fn encode_axis_markers(state: &State, budget: &Budget) -> Result<State, IdentityRejection> {
    if state.metadata.atoms.iter().any(|atom| atom.chiral_tag > 2)
        || state.metadata.bonds.iter().any(|bond| bond.stereo > 7)
    {
        return Err(REJECTED);
    }
    let mut encoded = state.clone();
    for atom in &mut encoded.metadata.atoms {
        atom.map_number = 0;
        atom.map_present = false;
    }
    if !state
        .metadata
        .bonds
        .iter()
        .any(|bond| matches!(bond.stereo, 6 | 7))
    {
        return Ok(encoded);
    }
    let eligible = eligible_axes(state, budget)?;
    let codes = label::assign(state, &Default::default())
        .map_err(|_| REJECTED)?
        .state
        .properties
        .bond_codes;
    for (bond, metadata) in state.metadata.bonds.iter().enumerate() {
        if !matches!(metadata.stereo, 6 | 7) {
            continue;
        }
        let axis = eligible
            .iter()
            .find(|axis| axis.bond == bond)
            .ok_or(REJECTED)?;
        let marker = match at(&codes, bond)?.as_deref() {
            Some("M") => AXIS_M,
            Some("P") => AXIS_P,
            _ => return Err(REJECTED),
        };
        for end in axis.ends {
            let atom = at_mut(&mut encoded.metadata.atoms, end)?;
            atom.map_number = marker;
            atom.map_present = true;
        }
        // Only the marked graph is ranked: raw winding depends on atom order.
        let metadata = at_mut(&mut encoded.metadata.bonds, bond)?;
        metadata.stereo = 0;
        metadata.stereo_atoms.clear();
        if let Some(code) = encoded.properties.bond_codes.get_mut(bond) {
            *code = None;
        }
    }
    Ok(encoded)
}

/// Decode the private naming/caption axis maps after SMILES preparation.
/// Unpaired, mismatched and nonstereogenic markers fail.
pub fn apply_axis_markers(state: &State, budget: &Budget) -> Result<State, IdentityRejection> {
    let markers: Vec<_> = state
        .metadata
        .atoms
        .iter()
        .map(|atom| atom.map_number)
        .collect();
    let marked = markers
        .iter()
        .filter(|&&map| matches!(map, AXIS_M | AXIS_P))
        .count();
    if marked == 0 {
        return Ok(state.clone());
    }
    let mut axes = vec![];
    for axis in eligible_axes(state, budget)? {
        let [a, b] = [at(&markers, axis.ends[0])?, at(&markers, axis.ends[1])?];
        if matches!(a, &AXIS_M | &AXIS_P) && a == b {
            axes.push((axis, if *a == AXIS_M { "M" } else { "P" }));
        }
    }
    if marked != 2 * axes.len() {
        return Err(REJECTED);
    }
    let mut decoded = state.clone();
    for (axis, _) in &axes {
        let metadata = at_mut(&mut decoded.metadata.bonds, axis.bond)?;
        metadata.stereo = 6;
        metadata.stereo_atoms = axis.refs.to_vec();
        for end in axis.ends {
            let atom = at_mut(&mut decoded.metadata.atoms, end)?;
            atom.map_number = 0;
            atom.map_present = false;
        }
    }
    // Install every axis before labelling: one axis can rank another's ligands.
    let labelled = label::assign(&decoded, &Default::default()).map_err(|_| REJECTED)?;
    for (axis, wanted) in &axes {
        match at(&labelled.state.properties.bond_codes, axis.bond)?.as_deref() {
            Some(code) if code == *wanted => {}
            Some("M" | "P") => at_mut(&mut decoded.metadata.bonds, axis.bond)?.stereo = 7,
            _ => return Err(REJECTED),
        }
    }
    let labelled = label::assign(&decoded, &Default::default()).map_err(|_| REJECTED)?;
    for (axis, wanted) in &axes {
        if at(&labelled.state.properties.bond_codes, axis.bond)?.as_deref() != Some(*wanted) {
            return Err(REJECTED);
        }
    }
    Ok(labelled.state)
}
