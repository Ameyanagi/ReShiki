//! Private naming identity for CIP-labelled biaryl axes.
use super::{IdentityRejection, chemistry};
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
fn eligible_axes(state: &State) -> Result<Vec<Axis>, IdentityRejection> {
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
        if !chemistry::stereo::axis_ends_distinct(&state.graph, index).map_err(|_| REJECTED)? {
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
pub fn encode_axis_markers(state: &State) -> Result<State, IdentityRejection> {
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
    let eligible = eligible_axes(state)?;
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
pub fn apply_axis_markers(state: &State) -> Result<State, IdentityRejection> {
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
    for axis in eligible_axes(state)? {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::molecule_names::canonical_identity;

    fn binol(display: &str) -> State {
        chemistry::document::prepare(&chemistry::document::atrop_tests::binol(display))
            .unwrap()
            .state
    }

    fn decode(text: &str) -> Result<State, IdentityRejection> {
        apply_axis_markers(&chemistry::smiles::prepare(text).unwrap().state)
    }

    fn aryl_link(state: &State) -> usize {
        state
            .graph
            .bonds
            .iter()
            .position(|bond| {
                bond.order == 1
                    && !bond.aromatic
                    && state.graph.atoms[bond.a].aromatic
                    && state.graph.atoms[bond.b].aromatic
            })
            .unwrap()
    }

    fn assert_roundtrips(state: &State, axis_count: usize) {
        let identity = canonical_identity(state).unwrap();
        let encoded = encode_axis_markers(state).unwrap();
        assert!(
            encoded
                .metadata
                .bonds
                .iter()
                .all(|b| !matches!(b.stereo, 6 | 7))
        );
        assert_eq!(
            encoded
                .metadata
                .atoms
                .iter()
                .filter(|atom| { matches!(atom.map_number, AXIS_M | AXIS_P) })
                .count(),
            2 * axis_count
        );
        for root in (0..state.graph.atoms.len()).step_by(3) {
            // Starting each traversal at a different atom changes read atom
            // indices, bond insertion order and native axial carriers.
            let text = chemistry::smiles::write::write(
                &encoded,
                chemistry::smiles::write::Options {
                    canonical: false,
                    root: Some(root),
                    ..Default::default()
                },
            )
            .unwrap()
            .text;
            let restored = decode(&text).unwrap();
            assert_eq!(canonical_identity(&restored).unwrap(), identity, "{text}");
            assert_eq!(
                restored
                    .metadata
                    .bonds
                    .iter()
                    .filter(|b| matches!(b.stereo, 6 | 7))
                    .count(),
                axis_count
            );
            assert!(
                restored
                    .metadata
                    .atoms
                    .iter()
                    .all(|atom| atom.map_number == 0)
            );
        }
        let mut repeated = identity.clone();
        for _ in 0..3 {
            repeated = canonical_identity(&decode(&repeated).unwrap()).unwrap();
            assert_eq!(repeated, identity);
        }
    }

    #[test]
    fn axis_identity_is_canonical_for_binol_permutations_and_endpoint_reversal() {
        for (display, marker) in [("hash", AXIS_M), ("wedge", AXIS_P)] {
            let source = binol(display);
            let identity = canonical_identity(&source).unwrap();
            assert_eq!(identity.matches(&format!(":{marker}")).count(), 2);
            assert_roundtrips(&source, 1);
            for shift in [0, 1, 5, 11] {
                let mut doc = chemistry::document::atrop_tests::binol(display);
                doc.atoms.rotate_left(shift);
                doc.atoms.reverse();
                doc.bonds.reverse();
                let permuted = chemistry::document::prepare(&doc).unwrap().state;
                assert_eq!(canonical_identity(&permuted).unwrap(), identity);
            }
            let mut reversed = source;
            let axis = reversed
                .graph
                .bonds
                .iter()
                .position(|bond| (bond.a == 0 && bond.b == 10) || (bond.a == 10 && bond.b == 0))
                .unwrap();
            let bond = &mut reversed.graph.bonds[axis];
            std::mem::swap(&mut bond.a, &mut bond.b);
            reversed.metadata.bonds[axis].stereo_atoms.reverse();
            assert_eq!(canonical_identity(&reversed).unwrap(), identity);
        }
        assert_ne!(
            canonical_identity(&binol("hash")),
            canonical_identity(&binol("wedge"))
        );
        let plain = canonical_identity(&binol("plain")).unwrap();
        assert!(!plain.contains(":90000"));
    }

    #[test]
    fn two_axes_and_component_permutations_retain_their_helicities() {
        for input in [
            "Oc1ccccc1-c1cc(C)cc(-c2ccccc2O)c1",
            "Oc1ccccc1-c1cc(C)cc(-c2ccccc2F)c1",
        ] {
            let mut state = chemistry::smiles::prepare(input).unwrap().state;
            let axes = eligible_axes(&state).unwrap();
            assert_eq!(axes.len(), 2, "{input}");
            for (axis, stereo) in axes.iter().zip([6, 7]) {
                state.metadata.bonds[axis.bond].stereo = stereo;
                state.metadata.bonds[axis.bond].stereo_atoms = axis.refs.to_vec();
            }
            assert_roundtrips(&state, 2);
        }
        let m = canonical_identity(&binol("hash")).unwrap();
        let p = canonical_identity(&binol("wedge")).unwrap();
        let forward = decode(&format!("{m}.[Na+].{p}.[Cl-]")).unwrap();
        let reverse = decode(&format!("[Cl-].{p}.[Na+].{m}")).unwrap();
        assert_eq!(canonical_identity(&forward), canonical_identity(&reverse));
        assert_roundtrips(&forward, 2);
    }

    #[test]
    fn malformed_or_unlabelled_axis_markers_are_refused() {
        let encoded = encode_axis_markers(&binol("hash")).unwrap();
        let marked: Vec<_> = encoded
            .metadata
            .atoms
            .iter()
            .enumerate()
            .filter_map(|(index, atom)| (atom.map_number == AXIS_M).then_some(index))
            .collect();
        let mut unpaired = encoded.clone();
        unpaired.metadata.atoms[marked[0]].map_number = 0;
        let mut mismatched = encoded.clone();
        mismatched.metadata.atoms[marked[0]].map_number = AXIS_P;
        for state in [unpaired, mismatched] {
            assert!(matches!(
                apply_axis_markers(&state),
                Err(IdentityRejection::NonclassicalStereo)
            ));
        }
        for text in [
            "[CH3:900004][CH3:900004]",
            "[CH2:900004]=[CH2:900004]",
            "[cH:900004]1[cH:900004]cccc1",
            "[c:900004]1ccccc1-[c:900004]1ccccc1",
        ] {
            assert!(
                matches!(decode(text), Err(IdentityRejection::NonclassicalStereo)),
                "{text}"
            );
        }
    }

    #[test]
    fn equivalent_axis_ends_are_ineligible_for_encoding_and_decoding() {
        for smiles in [
            "c1ccccc1-c1ccccc1",
            "OC1=C(C=CC=C1)[C-]1C=CC=C1",
            "[Na+].CC1=C(C=CC=C1)[C-]1C=CC=C1",
        ] {
            let mut state = chemistry::smiles::prepare(smiles).unwrap().state;
            let axis = aryl_link(&state);
            assert!(eligible_axes(&state).unwrap().is_empty(), "{smiles}");
            state.metadata.bonds[axis].stereo = 6;
            assert_eq!(canonical_identity(&state), Err(REJECTED), "{smiles}");
            state.metadata.bonds[axis].stereo = 0;
            let bond = &state.graph.bonds[axis];
            for end in [bond.a, bond.b] {
                state.metadata.atoms[end].map_number = AXIS_M;
            }
            assert!(
                matches!(apply_axis_markers(&state), Err(REJECTED)),
                "{smiles}"
            );
        }
    }

    #[test]
    fn unknown_stereo_does_not_hide_an_unsupported_raw_configuration() {
        let plain = chemistry::smiles::prepare("CCO").unwrap().state;
        let mut atom = plain.clone();
        atom.metadata.atoms[0].chiral_tag = 3;
        atom.properties.atoms[0].unknown = true;
        assert_eq!(
            canonical_identity(&atom),
            Err(IdentityRejection::NonclassicalStereo)
        );
        for stereo in [6, 7, 8] {
            let mut bond = plain.clone();
            bond.metadata.bonds[0].stereo = stereo;
            bond.metadata.bonds[0].unknown_stereo = true;
            assert_eq!(
                canonical_identity(&bond),
                Err(IdentityRejection::NonclassicalStereo)
            );
        }
    }

    #[test]
    fn pseudo_axes_and_allenes_remain_unsupported() {
        let mut pseudo = chemistry::smiles::prepare("c1([C@H](F)Cl)cccc([C@@H](F)Cl)c1-c1ccccc1O")
            .unwrap()
            .state;
        assert!(eligible_axes(&pseudo).unwrap().is_empty());
        let axis = aryl_link(&pseudo);
        pseudo.metadata.bonds[axis].stereo = 6;
        let assigned = chemistry::stereo::cip::label::assign(&pseudo, &Default::default()).unwrap();
        assert!(matches!(
            assigned.state.properties.bond_codes[axis].as_deref(),
            Some("m" | "p")
        ));
        assert_eq!(
            canonical_identity(&pseudo),
            Err(IdentityRejection::NonclassicalStereo)
        );
        let allene = chemistry::smiles::prepare("FC=[C@AL1]=CCl").unwrap().state;
        assert_eq!(
            canonical_identity(&allene),
            Err(IdentityRejection::NonclassicalStereo)
        );
    }
}
