//! Scoped small-ring descriptors, including upstream's local isotope tie break.
use super::{
    assembly_parts::AssemblyParts,
    graph_io::assign_local_cip_with_locant_labels,
    molecule::{AtomSet, Molecule},
};
use std::collections::BTreeMap;

pub fn scoped_small_ring_stereo_features(
    mol: &Molecule,
    parts: &AssemblyParts,
    numbered_path: &[usize],
    get_loc: impl Fn(usize) -> String,
) -> Vec<(String, String)> {
    if !parts.is_substituent
        || !parts.is_ring
        || parts.is_bicycle
        || parts.is_spiro
        || parts.is_polycycle
        || !(3..=7).contains(&numbered_path.len())
    {
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
mod tests {
    use super::*;
    use crate::naming::openclatura::graph_io::read_smiles;

    #[test]
    fn local_isotope_cip_and_modern_precedence_match_upstream() {
        // Upstream _assign_local_cip_with_locant_labels independently gives
        // R/S at atoms 4/7; modern accurate CIP gives s/s at the same centers.
        let mut mol = read_smiles("CC(=O)N[C@]1(C)C[C@H](N)C1").unwrap();
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
        assert_eq!(
            scoped_small_ring_stereo_features(&mol, &parts, &path, get_loc),
            vec![("1".into(), "s".into()), ("3".into(), "s".into())]
        );
        mol.accurate_cip.clear();
        assert_eq!(
            scoped_small_ring_stereo_features(&mol, &parts, &path, get_loc),
            vec![("1".into(), "R".into()), ("3".into(), "s".into())]
        );
        parts.is_substituent = false;
        assert!(scoped_small_ring_stereo_features(&mol, &parts, &path, get_loc).is_empty());
    }

    #[test]
    fn monosubstituted_centers_keep_relative_fallback() {
        let mol = read_smiles("C[C@H]1C[C@@H](C)C1").unwrap();
        let mut parts = AssemblyParts::new(4);
        parts.is_ring = true;
        parts.is_substituent = true;
        let path = [1, 2, 3, 5];
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
