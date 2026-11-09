// SPDX-License-Identifier: MIT AND (MIT OR Apache-2.0)
// Origin: mixed
// Source: openclatura | src/openclatura/small_ring_stereo.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: ReShiki | src/naming/openclatura/small_ring_stereo.rs | 5f559fd482e237307a589dfcb7ea333cee0a6e13
// Copyright (c) 2026 lamalab-org
// Copyright (c) 2026 Ameyanagi and ReShiki contributors
// Changes: Adapted openclatura local numbered-ring isotope CIP fallback, using the native graph adapter, with original Rust regression tests.
// Local contributions: MIT OR Apache-2.0; see NOTICE and PROVENANCE-nazuchem.toml.

//! Scoped small-ring descriptors, including upstream's local isotope tie break.
use super::{
    assembly_parts::AssemblyParts,
    graph_io::assign_local_cip_with_locant_labels,
    molecule::{AtomSet, Molecule},
};
use std::collections::BTreeMap;

// Provenance: fn scoped_small_ring_stereo_features | adapted
// Source: openclatura | src/openclatura/small_ring_stereo.py::scoped_small_ring_stereo_features
pub fn scoped_small_ring_stereo_features(
    mol: &Molecule,
    parts: &AssemblyParts,
    numbered_path: &[usize],
    get_loc: impl Fn(usize) -> String,
) -> Vec<(String, String)> {
    if !parts.is_substituent || !parts.is_simple_ring() || !(3..=7).contains(&numbered_path.len()) {
        return Vec::new();
    }
    let raw_atoms = numbered_path
        .iter()
        .copied()
        .filter(|idx| {
            mol.atoms.get(idx).is_some_and(|a| {
                a.stereo.is_none() && matches!(a.raw_stereo.as_deref(), Some("CW" | "CCW"))
            })
        })
        .collect::<Vec<_>>();
    if raw_atoms.len() != 2 {
        return Vec::new();
    }
    let parent_set: AtomSet = numbered_path.iter().copied().collect();
    if raw_atoms.iter().all(|&idx| {
        mol.get_neighbors(idx)
            .iter()
            .filter(|n| !parent_set.contains(n))
            .count()
            == 1
    }) {
        return Vec::new();
    }
    let mut locants = BTreeMap::new();
    let mut numeric_locants = BTreeMap::new();
    for &idx in numbered_path {
        let locant = get_loc(idx);
        if locant.is_empty() || !locant.bytes().all(|c| c.is_ascii_digit()) {
            return Vec::new();
        }
        let Ok(number) = locant.parse::<usize>() else {
            return Vec::new();
        };
        locants.insert(idx, locant);
        numeric_locants.insert(idx, number);
    }
    let Ok(local_cip) = assign_local_cip_with_locant_labels(mol, &numeric_locants, &raw_atoms)
    else {
        return Vec::new();
    };
    if local_cip.keys().copied().collect::<AtomSet>() != raw_atoms.iter().copied().collect() {
        return Vec::new();
    }
    let mut sorted = raw_atoms;
    sorted.sort_by_key(|idx| numeric_locants.get(idx).copied());
    sorted
        .into_iter()
        .filter_map(|idx| {
            let locant = locants.get(&idx)?.clone();
            let descriptor = if let Some(accurate) = mol.accurate_cip.get(&idx) {
                accurate.clone()
            } else {
                let local = local_cip.get(&idx)?;
                if locant != parts.attachment_locant {
                    local.to_lowercase()
                } else {
                    local.clone()
                }
            };
            Some((locant, descriptor))
        })
        .collect()
}

#[cfg(test)]
// Provenance: mod tests | original
// Source: ReShiki | src/naming/openclatura/small_ring_stereo.rs::tests
mod tests {
    use super::*;
    use crate::naming::openclatura::graph_io::read_smiles;

    #[test]
    fn modern_cip_precedes_the_scoped_local_isotope_fallback() {
        // Upstream _assign_local_cip_with_locant_labels independently gives
        // R/S at atoms 4/7; modern accurate CIP gives s/s at the same centers.
        let mut mol = read_smiles("CC(=O)N[C@]1(C)C[C@H](N)C1", &Default::default()).unwrap();
        let path = [4, 6, 7, 9];
        let locants = BTreeMap::from([(4, 1), (6, 2), (7, 3), (9, 4)]);
        assert_eq!(
            assign_local_cip_with_locant_labels(&mol, &locants, &[4, 7]).unwrap(),
            BTreeMap::from([(4, "R".into()), (7, "S".into())])
        );
        let mut parts = AssemblyParts::new(4);
        parts.is_ring = true;
        parts.is_substituent = true;
        let get_loc = |idx| locants[&idx].to_string();
        assert_eq!(mol.atoms[&4].stereo.as_deref(), Some("s"));
        assert_eq!(mol.atoms[&7].stereo.as_deref(), Some("s"));
        assert!(scoped_small_ring_stereo_features(&mol, &parts, &path, get_loc).is_empty());
        mol.accurate_cip.clear();
        mol.atoms.get_mut(&4).unwrap().stereo = None;
        mol.atoms.get_mut(&7).unwrap().stereo = None;
        assert_eq!(
            scoped_small_ring_stereo_features(&mol, &parts, &path, get_loc),
            vec![("1".into(), "R".into()), ("3".into(), "s".into())]
        );
        parts.is_substituent = false;
        assert!(scoped_small_ring_stereo_features(&mol, &parts, &path, get_loc).is_empty());
    }

    #[test]
    fn monosubstituted_centers_use_full_cip_without_local_isotopes() {
        let mol = read_smiles("C[C@H]1C[C@@H](C)C1", &Default::default()).unwrap();
        let mut parts = AssemblyParts::new(4);
        parts.is_ring = true;
        parts.is_substituent = true;
        let path = [1, 2, 3, 5];
        assert_eq!(mol.atoms[&1].stereo.as_deref(), Some("s"));
        assert_eq!(mol.atoms[&3].stereo.as_deref(), Some("s"));
        assert!(
            scoped_small_ring_stereo_features(&mol, &parts, &path, |idx| (path
                .iter()
                .position(|&a| a == idx)
                .unwrap()
                + 1)
            .to_string())
            .is_empty()
        );
    }
}
