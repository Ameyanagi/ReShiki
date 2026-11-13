// SPDX-License-Identifier: MIT OR Apache-2.0
// Origin: original
// Source: ReShiki | crates/model/src/molecule_names.rs | 2df788dc456793c687b934afd6d4b317e3c73ef8
// Changes: extract chemistry-only identity helpers; document admission remains in molecule_names.rs.
// Intentionally mirrored on different State types until ReShiki's chemistry and the kernel converge; kept in step by crates/model/src/molecule_names/identity_parity.rs.

//! Canonical molecular identity, independent of document selection and captions.
use super::{chemistry, encode_axis_markers};

/// Chemical semantics that cannot be discarded when associating a name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityRejection {
    UnsupportedAtomsOrBonds,
    EnhancedStereo,
    NonclassicalStereo,
    /// Preserve the SMILES writer's own diagnostic at both call sites.
    Write(String),
}

/// Shared canonical isomeric identity, with private maps for M/P biaryl axes.
/// Ordinary atom maps are excluded. The naming adapter and caption identity
/// retain their own document bounds and error messages.
pub fn canonical_identity(
    state: &chemistry::stereo::perception::State,
) -> Result<String, IdentityRejection> {
    if !state.metadata.groups.is_empty() {
        return Err(IdentityRejection::EnhancedStereo);
    }
    if state
        .graph
        .atoms
        .iter()
        .any(|atom| atom.atomic_number == 0 || atom.radical_electrons > 2)
        || state
            .graph
            .bonds
            .iter()
            .any(|bond| !(1..=4).contains(&bond.order))
    {
        return Err(IdentityRejection::UnsupportedAtomsOrBonds);
    }
    let state = unspecified_unknown_stereo(&encode_axis_markers(state)?);
    let smiles = chemistry::smiles::write::write(
        &state,
        chemistry::smiles::write::Options {
            ignore_maps: false,
            ..Default::default()
        },
    )
    .map_err(|error| IdentityRejection::Write(error.to_string()))?;
    if state
        .graph
        .atoms
        .iter()
        .any(|atom| atom.radical_electrons != 0)
    {
        // Plain SMILES encodes radicals through a valence deficit, rather than
        // an explicit electron field. Admit a radical only when that encoding
        // reconstructs every atom's exact electron count, including atoms that
        // have no radicals. Traversal order supplies the source-to-reader map.
        let reconstructed = chemistry::smiles::prepare(&smiles.text)
            .map_err(|_| IdentityRejection::UnsupportedAtomsOrBonds)?;
        if smiles.atom_order.len() != reconstructed.state.graph.atoms.len()
            || smiles
                .atom_order
                .iter()
                .zip(&reconstructed.state.graph.atoms)
                .enumerate()
                .any(|(target_index, (&source, target))| {
                    state.graph.atoms.get(source).is_none_or(|atom| {
                        let source_hydrogens = state.valences.get(source).map(|valence| {
                            u32::from(atom.explicit_hydrogens) + valence.implicit_hydrogens
                        });
                        let target_hydrogens =
                            reconstructed
                                .state
                                .valences
                                .get(target_index)
                                .map(|valence| {
                                    u32::from(target.explicit_hydrogens)
                                        + valence.implicit_hydrogens
                                });
                        atom.atomic_number != target.atomic_number
                            || atom.charge != target.charge
                            || atom.isotope != target.isotope
                            || atom.radical_electrons != target.radical_electrons
                            || source_hydrogens != target_hydrogens
                    })
                })
        {
            return Err(IdentityRejection::UnsupportedAtomsOrBonds);
        }
    }
    Ok(smiles.text)
}

/// The identity named by an unknown-configuration drawing. Clear only the
/// explicitly unknown units; specified neighbouring stereocentres remain part
/// of the identity checked by naming and generated captions.
pub fn unspecified_unknown_stereo(
    state: &chemistry::stereo::perception::State,
) -> chemistry::stereo::perception::State {
    use chemistry::kekulize::Direction;
    let unknown_atoms = unknown_atom_units(state);
    let mut state = state.clone();
    for ((property, metadata), unknown) in state
        .properties
        .atoms
        .iter_mut()
        .zip(&mut state.metadata.atoms)
        .zip(unknown_atoms)
    {
        if unknown {
            property.unknown = false;
            property.cip_code = None;
            metadata.chiral_tag = 0;
            metadata.chiral_permutation = None;
            metadata.ring_stereo = false;
        }
    }
    for (idx, metadata) in state.metadata.bonds.iter_mut().enumerate() {
        if metadata.unknown_stereo || metadata.stereo == 1 {
            metadata.unknown_stereo = false;
            metadata.stereo = 0;
            metadata.stereo_atoms.clear();
            if let Some(code) = state.properties.bond_codes.get_mut(idx) {
                *code = None;
            }
            if let Some(direction) = state.directions.get_mut(idx) {
                *direction = Direction::None;
            }
        }
    }
    state
}

/// Unknown tetrahedral anchors can be explicit properties or the first
/// endpoint of a native wavy single bond. Double-bond unknowns are separate.
pub fn unknown_atom_units(state: &chemistry::stereo::perception::State) -> Vec<bool> {
    let mut unknown: Vec<_> = state
        .properties
        .atoms
        .iter()
        .map(|atom| atom.unknown)
        .collect();
    for (idx, bond) in state.graph.bonds.iter().enumerate() {
        if bond.order == 1
            && (state.directions.get(idx) == Some(&chemistry::kekulize::Direction::Unknown)
                || state
                    .metadata
                    .bonds
                    .get(idx)
                    .is_some_and(|meta| meta.unknown_stereo))
            && let Some(anchor) = unknown.get_mut(bond.a)
        {
            *anchor = true;
        }
    }
    unknown
}
