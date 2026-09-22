//! Fragment copying and atom/bond ordering from RDKit MolOps.cpp.
//! Copyright (C) 2001-2025 Greg Landrum and other RDKit contributors.
//! BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
use super::{Error, State, at, invalid, put};
use crate::chemistry::{
    graph::Graph,
    ranking::{Metadata, StereoGroup},
    stereo::perception::{Properties, RingCache},
};
use std::collections::BTreeMap;

pub(super) struct Fragment {
    pub state: State,
    pub atoms: Vec<usize>,
    pub bonds: Vec<usize>,
}
pub(super) fn split(input: &State) -> Result<Vec<Fragment>, Error> {
    let n = input.graph.atoms.len();
    let mut adjacent = vec![Vec::new(); n];
    for bond in &input.graph.bonds {
        put(&mut adjacent, bond.a)?.push(bond.b);
        put(&mut adjacent, bond.b)?.push(bond.a);
    }
    let mut groups = vec![None; n];
    let mut count = 0;
    for start in 0..n {
        if at(&groups, start)?.is_some() {
            continue;
        }
        *put(&mut groups, start)? = Some(count);
        let mut stack = vec![start];
        while let Some(atom) = stack.pop() {
            for &neighbor in at(&adjacent, atom)? {
                if at(&groups, neighbor)?.is_none() {
                    *put(&mut groups, neighbor)? = Some(count);
                    stack.push(neighbor);
                }
            }
        }
        count += 1;
    }
    if count == 1 {
        return Ok(vec![Fragment {
            state: input.clone(),
            atoms: (0..n).collect(),
            bonds: (0..input.graph.bonds.len()).collect(),
        }]);
    }
    let mut result = (0..count)
        .map(|_| Fragment {
            state: State {
                graph: Graph {
                    atoms: Vec::new(),
                    bonds: Vec::new(),
                },
                metadata: Metadata::default(),
                directions: Vec::new(),
                valences: Vec::new(),
                conjugated: Vec::new(),
                hybridizations: Vec::new(),
                rings: RingCache::default(),
                properties: Properties::default(),
            },
            atoms: Vec::new(),
            bonds: Vec::new(),
        })
        .collect::<Vec<_>>();
    let mut indices = vec![0; n];
    let group = |a| at(&groups, a)?.ok_or_else(|| invalid("Atom has no component"));
    for (a, atom) in input.graph.atoms.iter().enumerate() {
        let part = put(&mut result, group(a)?)?;
        *put(&mut indices, a)? = part.atoms.len();
        part.atoms.push(a);
        part.state.graph.atoms.push(atom.clone());
        let mut meta = at(&input.metadata.atoms, a)?.clone();
        meta.ring_stereo = false;
        part.state.metadata.atoms.push(meta);
        part.state.valences.push(*at(&input.valences, a)?);
        part.state
            .hybridizations
            .push(*at(&input.hybridizations, a)?);
        let mut props = at(&input.properties.atoms, a)?.clone();
        props.cip_rank = None;
        props.ring_candidate = None;
        props.ring_members = None;
        part.state.properties.atoms.push(props);
    }
    let mut bond_indices = vec![0; input.graph.bonds.len()];
    for (b, bond) in input.graph.bonds.iter().enumerate() {
        let id = group(bond.a)?;
        if group(bond.b)? != id {
            return Err(invalid("Bond crosses components"));
        }
        let part = put(&mut result, id)?;
        *put(&mut bond_indices, b)? = part.bonds.len();
        part.bonds.push(b);
        let mut copied = bond.clone();
        copied.a = *at(&indices, bond.a)?;
        copied.b = *at(&indices, bond.b)?;
        part.state.graph.bonds.push(copied);
        let mut meta = at(&input.metadata.bonds, b)?.clone();
        meta.stereo_atoms = meta
            .stereo_atoms
            .iter()
            .map(|&a| {
                if group(a)? != id {
                    return Err(invalid("Stereo control crosses components"));
                }
                at(&indices, a).copied()
            })
            .collect::<Result<_, _>>()?;
        part.state.metadata.bonds.push(meta);
        part.state.directions.push(*at(&input.directions, b)?);
        part.state.conjugated.push(*at(&input.conjugated, b)?);
        part.state
            .properties
            .bond_codes
            .push(at(&input.properties.bond_codes, b)?.clone());
    }
    for original in &input.metadata.groups {
        let mut copies = BTreeMap::<usize, StereoGroup>::new();
        let empty = || StereoGroup {
            atoms: Vec::new(),
            bonds: Vec::new(),
            kind: original.kind,
            read_id: original.read_id,
            write_id: original.write_id,
        };
        for &a in &original.atoms {
            copies
                .entry(group(a)?)
                .or_insert_with(empty)
                .atoms
                .push(*at(&indices, a)?);
        }
        for &b in &original.bonds {
            let id = group(at(&input.graph.bonds, b)?.a)?;
            copies
                .entry(id)
                .or_insert_with(empty)
                .bonds
                .push(*at(&bond_indices, b)?);
        }
        for (id, copied) in copies {
            put(&mut result, id)?.state.metadata.groups.push(copied);
        }
    }
    Ok(result)
}
