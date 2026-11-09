// SPDX-License-Identifier: MIT OR Apache-2.0
// Origin: adapted from Nazuchem crates/nazuchem-s2n/src/ir_input.rs at 6fcd7c4
// Copyright (c) 2026 Ameyanagi and Nazuchem contributors
// Changes: checked ReShiki graph access; drawing-import H projection and mapping.

//! Direct preparation of naming IR, preserving its atom and bond identities.
use super::{MAX_NAMING_ATOMS, chemistry, encode_axis_markers};
use chemistry::{
    graph::{Atom, Bond, Graph},
    hydrogens,
    kekulize::Direction,
    ranking::{Metadata, StereoGroup},
    sanitize,
    smiles::Prepared,
    stereo::{self, cip::configuration::parity4, perception},
};
use nazuchem_ir::{
    AxialWinding, BondOrder, Molecule, Ref, Relation, Stereo, StereoGroupKind, Winding,
};

const TETRAHEDRAL_LOSS: &str = "Native import could not retain every specified tetrahedral center; no name or graph was assigned";
const NONCLASSICAL_STEREO: &str = "Naming supports tetrahedral, ordinary alkene and biaryl axis stereochemistry; other stereo classes cannot be discarded";
const METADATA: &str = "ReShiki drawings cannot yet store relative configuration (rel, rac, or endo/exo/syn/anti on a chiral ring system), polymer or atom-label semantics. No simplified structure or single enantiomer was drawn";
// IR keeps H vertices that the former SMILES writer folded. This conservative
// raw ceiling covers every graph that can fit the worker's 64 KiB response cap;
// the app applies MAX_NAMING_ATOMS to the projected drawing-import graph.
const MAX_IR_ATOMS: usize = MAX_NAMING_ATOMS * 8;
const MAX_IR_BONDS: usize = MAX_IR_ATOMS * 4;

fn at<T>(items: &[T], index: usize) -> Result<&T, String> {
    items
        .get(index)
        .ok_or_else(|| "Invalid IR atom or bond identity".into())
}
fn at_mut<T>(items: &mut [T], index: usize) -> Result<&mut T, String> {
    items
        .get_mut(index)
        .ok_or_else(|| "Invalid IR atom or bond identity".into())
}

fn perception_state(sanitized: sanitize::Sanitized) -> perception::State {
    perception::State {
        properties: perception::Properties::unspecified(&sanitized.graph),
        graph: sanitized.graph,
        metadata: sanitized.metadata,
        directions: sanitized.directions,
        valences: sanitized.valences,
        conjugated: sanitized.conjugated,
        hybridizations: sanitized.hybridizations,
        rings: perception::RingCache {
            kind: perception::RingKind::Symmetric,
            atoms: sanitized.rings,
        },
    }
}

/// Prepare the typed graph directly, without serializing or reparsing SMILES.
///
/// Atom and bond indices remain the IR identities, including explicit hydrogen
/// vertices. Hydrogen counts and radical electrons are authoritative. Locants
/// and parent membership remain on the caller's IR; enhanced stereo groups are
/// retained for the caller's strict-import or generated-name verification gate.
pub fn from_ir(molecule: &Molecule) -> Result<Prepared, String> {
    if molecule.atoms.len() > MAX_IR_ATOMS || molecule.bonds.len() > MAX_IR_BONDS {
        return Err("The naming IR graph exceeds the supported import size".into());
    }
    molecule.validate().map_err(|error| error.to_string())?;
    if !molecule.position_variation.is_empty()
        || !molecule.polymers.is_empty()
        || molecule
            .atoms
            .iter()
            .any(|atom| atom.label.is_some() || atom.attachment_point.is_some())
    {
        return Err(METADATA.into());
    }
    let graph = Graph {
        atoms: molecule
            .atoms
            .iter()
            .map(|atom| Atom {
                atomic_number: atom.element,
                isotope: atom.isotope.unwrap_or_default(),
                charge: atom.charge,
                explicit_hydrogens: atom.hydrogens,
                no_implicit: true,
                aromatic: false,
                radical_electrons: atom.radical_electrons,
            })
            .collect(),
        bonds: molecule
            .bonds
            .iter()
            .map(|bond| {
                Ok(Bond {
                    a: bond.atoms[0] as usize,
                    b: bond.atoms[1] as usize,
                    order: match bond.order {
                        BondOrder::Single => 1,
                        BondOrder::Double => 2,
                        BondOrder::Triple => 3,
                        BondOrder::Quadruple => 6,
                        _ => return Err("Unsupported IR bond order".into()),
                    },
                    aromatic: false,
                })
            })
            .collect::<Result<_, String>>()?,
    };
    let adjacent = neighbors(&graph)?;
    let mut metadata = Metadata::unspecified(&graph);
    for (atom, meta) in molecule.atoms.iter().zip(&mut metadata.atoms) {
        if let Some(map) = atom.atom_map {
            // Match the native SMILES integer reader's admission bound.
            if map >= 2_147_483_640 {
                return Err("SMILES exceeds parser resource limits".into());
            }
            meta.map_number = map as i32;
            meta.map_present = true;
        }
    }
    for entry in &molecule.stereo {
        match entry {
            Stereo::Tetrahedral {
                center,
                neighbors,
                winding,
            } => {
                if *winding != Winding::Unknown {
                    at_mut(&mut metadata.atoms, *center as usize)?.chiral_tag =
                        tetrahedral_tag(*center as usize, neighbors, *winding, &adjacent)?;
                }
            }
            Stereo::DoubleBond { bond, relation, .. } => {
                let axis = at(&graph.bonds, *bond as usize)?;
                if *relation != Relation::Unknown
                    && graph.bonds.iter().enumerate().any(|(id, other)| {
                        id != *bond as usize
                            && other.order == 2
                            && [axis.a, axis.b]
                                .iter()
                                .any(|end| other.a == *end || other.b == *end)
                    })
                {
                    return Err("Naming does not support cumulene E/Z configuration yet; no less specific name was substituted".into());
                }
            }
            Stereo::Atropisomer {
                bond,
                refs,
                winding,
            } => {
                let meta = at_mut(&mut metadata.bonds, *bond as usize)?;
                meta.stereo = axial_tag(&graph, &adjacent, *bond as usize, refs, *winding)?;
                meta.stereo_atoms = axial_carriers(&graph, &adjacent, *bond as usize)?;
            }
            _ => return Err(NONCLASSICAL_STEREO.into()),
        }
    }
    let sanitized =
        sanitize::sanitize(&graph, &metadata, &vec![Direction::None; graph.bonds.len()])
            .map_err(|error| error.to_string())?;
    let mut state = perception_state(sanitized);
    if state.graph.atoms.len() != molecule.atoms.len()
        || state.graph.bonds.len() != molecule.bonds.len()
        || state
            .graph
            .bonds
            .iter()
            .zip(&molecule.bonds)
            .any(|(bond, source)| [bond.a as u32, bond.b as u32] != source.atoms)
    {
        return Err("Native import could not retain IR atom and bond identities".into());
    }
    for (index, ((source, atom), valence)) in molecule
        .atoms
        .iter()
        .zip(&state.graph.atoms)
        .zip(&state.valences)
        .enumerate()
    {
        if u32::from(atom.explicit_hydrogens) + valence.implicit_hydrogens
            != u32::from(source.hydrogens)
        {
            return Err(format!(
                "Native import could not retain IR hydrogen count at atom {index}"
            ));
        }
        if atom.radical_electrons != source.radical_electrons {
            return Err(format!(
                "Native import could not retain IR radical electron count at atom {index}"
            ));
        }
    }
    // Restore inferred H only when it exactly reproduces the authoritative
    // count. In particular aromatic heteroatom hydrogens may need to stay fixed.
    for (index, source) in molecule.atoms.iter().enumerate() {
        if source.hydrogens_stated {
            continue;
        }
        let atom = at_mut(&mut state.graph.atoms, index)?;
        let previous = atom.clone();
        atom.explicit_hydrogens = 0;
        atom.no_implicit = false;
        match state.graph.valences() {
            Ok(valences)
                if at(&valences, index)?.implicit_hydrogens == u32::from(source.hydrogens) =>
            {
                state.valences = valences;
            }
            _ => *at_mut(&mut state.graph.atoms, index)? = previous,
        }
    }
    state.valences = state.graph.valences()?;
    if molecule.stereo.iter().any(|entry| {
        matches!(
            entry,
            Stereo::Atropisomer {
                winding: AxialWinding::Unknown,
                ..
            }
        )
    }) {
        // A trial winding validates the ordinary biaryl topology. Discard its
        // encoding before recording Unknown; no absolute configuration is added.
        encode_axis_markers(&state).map_err(|_| NONCLASSICAL_STEREO.to_owned())?;
    }
    for entry in &molecule.stereo {
        match entry {
            Stereo::Tetrahedral {
                center,
                winding: Winding::Unknown,
                ..
            } => {
                at_mut(&mut state.properties.atoms, *center as usize)?.unknown = true;
            }
            Stereo::Atropisomer { bond, winding, .. } => {
                let meta = at_mut(&mut state.metadata.bonds, *bond as usize)?;
                if !matches!(meta.stereo, 6 | 7) {
                    return Err(NONCLASSICAL_STEREO.into());
                }
                if *winding == AxialWinding::Unknown {
                    meta.stereo = 1;
                    meta.unknown_stereo = true;
                }
            }
            _ => {}
        }
    }
    // E/Z may be the only distinction between tetrahedral ligands. Install
    // these configurations before the shared atom-validation/rank-refinement loop.
    for entry in &molecule.stereo {
        if let Stereo::DoubleBond {
            bond,
            refs,
            relation,
        } = entry
        {
            double_bond(
                at_mut(&mut state.metadata.bonds, *bond as usize)?,
                refs,
                *relation,
            )?;
        }
    }
    state = perception::perceive_bond_configurations(
        &state,
        perception::Options {
            clean: true,
            force: true,
            flag_possible: true,
        },
    )?;
    let (_, stereogenic_bonds) = perception::stereogenic_units(&state)?;
    for entry in &molecule.stereo {
        match entry {
            Stereo::Tetrahedral {
                center, winding, ..
            } => {
                if *winding != Winding::Unknown
                    && at(&state.metadata.atoms, *center as usize)?.chiral_tag == 0
                {
                    return Err(TETRAHEDRAL_LOSS.into());
                }
            }
            Stereo::DoubleBond {
                bond,
                refs,
                relation,
            } => {
                if *relation != Relation::Unknown && !stereogenic_bonds.contains(&(*bond as usize))
                {
                    return Err(NONCLASSICAL_STEREO.into());
                }
                double_bond(
                    at_mut(&mut state.metadata.bonds, *bond as usize)?,
                    refs,
                    *relation,
                )?;
            }
            Stereo::Atropisomer { bond, .. } => {
                if !stereo::axis_ends_distinct(&state.graph, *bond as usize)? {
                    return Err(NONCLASSICAL_STEREO.into());
                }
            }
            _ => return Err(NONCLASSICAL_STEREO.into()),
        }
    }
    for source in &molecule.stereo_groups {
        let (kind, id) = match source.kind {
            StereoGroupKind::And(id) => (2, id),
            StereoGroupKind::Or(id) => (1, id),
            _ => return Err(NONCLASSICAL_STEREO.into()),
        };
        let mut group = StereoGroup {
            kind,
            read_id: id,
            write_id: id,
            ..StereoGroup::default()
        };
        for &member in &source.stereo {
            match at(&molecule.stereo, member as usize)? {
                Stereo::Tetrahedral { center, .. } => group.atoms.push(*center as usize),
                Stereo::DoubleBond { bond, .. } | Stereo::Atropisomer { bond, .. } => {
                    group.bonds.push(*bond as usize)
                }
                _ => return Err(NONCLASSICAL_STEREO.into()),
            }
        }
        state.metadata.groups.push(group);
    }
    state.metadata.validate(&state.graph)?;
    Ok(Prepared {
        state,
        dummy_labels: vec![None; molecule.atoms.len()],
    })
}

/// Apply the drawing import's ordinary-H removal to a detached prepared IR.
///
/// The returned mapping gives each surviving atom's index in the original IR,
/// so the caller can project parent locants without a SMILES traversal. Isotopic
/// H stays a vertex. The original prepared state retains all IR identities.
pub fn name_import_projection(input: &Prepared) -> Result<(Prepared, Vec<usize>), String> {
    let removed = hydrogens::remove(&hydrogens::Input {
        graph: input.state.graph.clone(),
        metadata: input.state.metadata.clone(),
        directions: input.state.directions.clone(),
        unknown_atoms: input
            .state
            .properties
            .atoms
            .iter()
            .map(|atom| atom.unknown)
            .collect(),
        annotations: hydrogens::Annotations::default(),
    })
    .map_err(|error| error.to_string())?;
    if removed.kept_atoms.len() == input.state.graph.atoms.len() {
        return Ok((input.clone(), removed.kept_atoms));
    }
    let sanitized = sanitize::sanitize(&removed.graph, &removed.metadata, &removed.directions)
        .map_err(|error| error.to_string())?;
    let mut state = perception_state(sanitized);
    for (property, unknown) in state.properties.atoms.iter_mut().zip(removed.unknown_atoms) {
        property.unknown = unknown;
    }
    state = perception::perceive_bond_configurations(
        &state,
        perception::Options {
            clean: true,
            force: true,
            flag_possible: true,
        },
    )?;
    // Keep reference-relative configurations authoritative across later
    // fragment copies. Those copies discard computed perception caches, while
    // IR imports have no slash directions from which to reconstruct E/Z.
    for meta in &mut state.metadata.bonds {
        meta.stereo = match meta.stereo {
            2 => 4,
            3 => 5,
            other => other,
        };
    }
    for (before, after) in removed.metadata.atoms.iter().zip(&state.metadata.atoms) {
        if matches!(before.chiral_tag, 1 | 2) && after.chiral_tag == 0 {
            return Err(TETRAHEDRAL_LOSS.into());
        }
    }
    let dummy_labels = removed
        .kept_atoms
        .iter()
        .map(|&index| at(&input.dummy_labels, index).cloned())
        .collect::<Result<_, _>>()?;
    Ok((
        Prepared {
            state,
            dummy_labels,
        },
        removed.kept_atoms,
    ))
}

fn double_bond(
    meta: &mut chemistry::ranking::BondMetadata,
    refs: &[u32; 2],
    relation: Relation,
) -> Result<(), String> {
    meta.stereo = match relation {
        Relation::Cis => 4,
        Relation::Trans => 5,
        Relation::Unknown => 1,
        _ => return Err(NONCLASSICAL_STEREO.into()),
    };
    meta.stereo_atoms = refs.iter().map(|&atom| atom as usize).collect();
    meta.unknown_stereo = relation == Relation::Unknown;
    Ok(())
}

fn neighbors(graph: &Graph) -> Result<Vec<Vec<usize>>, String> {
    let mut adjacent = vec![Vec::new(); graph.atoms.len()];
    for bond in &graph.bonds {
        at_mut(&mut adjacent, bond.a)?.push(bond.b);
        at_mut(&mut adjacent, bond.b)?.push(bond.a);
    }
    Ok(adjacent)
}

fn tetrahedral_tag(
    center: usize,
    refs: &[Ref; 4],
    winding: Winding,
    adjacent: &[Vec<usize>],
) -> Result<u8, String> {
    let neighbors = at(adjacent, center)?;
    let mut carriers = neighbors.iter().copied().map(Some).collect::<Vec<_>>();
    if carriers.len() < 4 {
        carriers.push(Some(center));
    }
    if carriers.len() < 4 {
        carriers.push(None);
    }
    let refs = refs
        .iter()
        .map(|reference| match reference {
            Ref::Atom(atom) => Ok(Some(*atom as usize)),
            Ref::ImplicitHydrogen => Ok(Some(center)),
            Ref::LonePair if neighbors.len() == 3 => Ok(Some(center)),
            Ref::LonePair => Ok(None),
            _ => Err(NONCLASSICAL_STEREO.to_owned()),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let parity = parity4(&refs, &carriers).map_err(|error| error.to_string())?;
    if parity == 0 {
        return Err(TETRAHEDRAL_LOSS.into());
    }
    match winding {
        Winding::Clockwise => Ok(if parity == 2 { 1 } else { 2 }),
        Winding::Anticlockwise => Ok(if parity == 2 { 2 } else { 1 }),
        _ => Err(NONCLASSICAL_STEREO.into()),
    }
}

fn axial_carriers(
    graph: &Graph,
    adjacent: &[Vec<usize>],
    bond: usize,
) -> Result<Vec<usize>, String> {
    let axis = at(&graph.bonds, bond)?;
    [(axis.a, axis.b), (axis.b, axis.a)]
        .into_iter()
        .map(|(end, other)| {
            let refs = at(adjacent, end)?
                .iter()
                .copied()
                .filter(|&atom| atom != other)
                .collect::<Vec<_>>();
            if refs.len() != 2 {
                return Err(NONCLASSICAL_STEREO.into());
            }
            refs.into_iter()
                .min()
                .ok_or_else(|| NONCLASSICAL_STEREO.into())
        })
        .collect()
}

fn axial_tag(
    graph: &Graph,
    adjacent: &[Vec<usize>],
    bond: usize,
    refs: &[Ref; 4],
    winding: AxialWinding,
) -> Result<u8, String> {
    let carriers = axial_carriers(graph, adjacent, bond)?;
    if refs
        .iter()
        .any(|reference| !matches!(reference, Ref::Atom(_)))
    {
        return Err(NONCLASSICAL_STEREO.into());
    }
    let mut clockwise = match winding {
        AxialWinding::Clockwise | AxialWinding::Unknown => true,
        AxialWinding::Anticlockwise => false,
        _ => return Err(NONCLASSICAL_STEREO.into()),
    };
    for side in 0..2 {
        if let Ref::Atom(reference) = at(refs, side * 2)? {
            clockwise ^= *reference as usize != *at(&carriers, side)?;
        }
    }
    Ok(if clockwise { 6 } else { 7 })
}

#[cfg(test)]
mod tests;
