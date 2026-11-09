// SPDX-License-Identifier: MIT OR Apache-2.0
// Origin: original
// Source: ReShiki | src/naming/openclatura/endo_exo_stereo.rs | 327d32fed63a5b3df15b7b8dc5ab84a7c36e0865
// Copyright (c) 2026 Ameyanagi and ReShiki contributors

//! Optional cage-relative names, derived from winding rather than CIP labels.
use super::{
    assembly_parts::AssemblyParts,
    molecule::Molecule,
    polycycle_topology::{audit_ring_numbering, edges_within_atoms},
};
use std::collections::BTreeMap;

type Point = [i64; 3];

/// Keep the absolute name as the primary result. A second naming pass cites
/// each substituent's face beside its constitutional locant instead.
pub(super) fn add_alternative(
    mol: &Molecule,
    parts: &mut AssemblyParts,
    path: &[usize],
    locants: &BTreeMap<usize, String>,
) {
    let Some(descriptors) = cage_descriptors(mol, parts, path, locants) else {
        return;
    };
    if mol.use_endo_exo_names() {
        parts.stereo_features.clear();
        parts.endo_exo_descriptors = descriptors;
    } else {
        mol.mark_endo_exo_alternative(&parts.parent_atom_ids);
    }
}

fn cage_descriptors(
    mol: &Molecule,
    parts: &AssemblyParts,
    path: &[usize],
    locants: &BTreeMap<usize, String>,
) -> Option<BTreeMap<String, &'static str>> {
    let (x, y, z) = parts.bicycle_xyz;
    if !parts.is_bicycle
        || !matches!((x, y, z), (2, 2, 1) | (3, 2, 1))
        || !audit_ring_numbering(
            "bicyclo",
            &[x, y, z],
            path,
            &edges_within_atoms(mol, &parts.parent_atom_ids),
        )
    {
        return None;
    }
    // The bridgeheads lie on the x axis. The longest bridge arches toward +y,
    // the next toward -y, and the one-atom shortest bridge toward +z. Curved
    // bridges give a nondegenerate tetrahedron even at the middle of [3.2.1].
    let mut points = BTreeMap::new();
    points.insert(path[0], [-2, 0, 0]);
    points.insert(path[x + 1], [2, 0, 0]);
    let long: &[Point] = if x == 2 {
        &[[-1, 2, 0], [1, 2, 0]]
    } else {
        &[[-1, 2, 0], [0, 3, 0], [1, 2, 0]]
    };
    for (&atom, &point) in path[1..=x].iter().zip(long) {
        points.insert(atom, point);
    }
    points.insert(path[x + 2], [1, -2, 0]);
    points.insert(path[x + 3], [-1, -2, 0]);
    points.insert(path[x + y + 2], [0, 0, 2]);

    // Both bridgeheads must agree on a realizable orientation of this cage.
    // Comparing raw winding with neighbour-ordered reference determinants
    // cancels the arbitrary absolute sign and any whole-cage reflection.
    let orientation = reference_parity(mol, &points, path[0], [-2, 0, 0], [-2, 0, 0])?;
    if reference_parity(mol, &points, path[x + 1], [2, 0, 0], [2, 0, 0])? != orientation {
        return None;
    }
    let mut descriptors = BTreeMap::new();
    for &atom in path {
        if atom == path[0] || atom == path[x + 1] || !mol.atoms[&atom].has_stereo() {
            continue;
        }
        let outside: Vec<_> = mol
            .get_neighbors(atom)
            .iter()
            .copied()
            .filter(|other| !parts.parent_atom_ids.contains(other))
            .collect();
        if mol.degree(atom) != 3
            || mol.atoms[&atom].total_h_count != 1
            || outside.len() != 1
            || mol.atoms[&outside[0]].symbol == "H"
        {
            return None;
        }
        let shortest_bridge = atom == path[x + y + 2];
        // At x/y-bridge atoms exo points toward the shortest bridge. At its
        // sole atom syn points toward the lowest numbered (longest) bridge.
        let toward = if shortest_bridge {
            [0, 2, 0]
        } else {
            [0, 0, 2]
        };
        let away = toward.map(|coordinate| -coordinate);
        let same = reference_parity(mol, &points, atom, toward, away)? == orientation;
        descriptors.insert(
            locants.get(&atom)?.clone(),
            match (shortest_bridge, same) {
                (false, true) => "exo",
                (false, false) => "endo",
                (true, true) => "syn",
                (true, false) => "anti",
            },
        );
    }
    (!descriptors.is_empty()).then_some(descriptors)
}

fn reference_parity(
    mol: &Molecule,
    points: &BTreeMap<usize, Point>,
    atom: usize,
    outside_direction: Point,
    hydrogen_direction: Point,
) -> Option<bool> {
    let clockwise = match mol.atoms[&atom].raw_stereo.as_deref()? {
        "CW" => true,
        "CCW" => false,
        _ => return None,
    };
    let center = points.get(&atom)?;
    let translated = |direction: Point| std::array::from_fn(|axis| center[axis] + direction[axis]);
    let mut neighbours: Vec<_> = mol
        .get_neighbors(atom)
        .iter()
        .map(|other| {
            points
                .get(other)
                .copied()
                .unwrap_or_else(|| translated(outside_direction))
        })
        .collect();
    if mol.atoms[&atom].total_h_count == 1 {
        // SMILES preparation leaves bracket/implicit hydrogen last in the
        // retained tetrahedral neighbour order, just as for cis/trans rings.
        neighbours.push(translated(hydrogen_direction));
    }
    let [a, b, c, d] = neighbours.as_slice() else {
        return None;
    };
    let subtract = |left: &Point| std::array::from_fn::<_, 3, _>(|i| left[i] - d[i]);
    let (a, b, c) = (subtract(a), subtract(b), subtract(c));
    let determinant = a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
        + a[2] * (b[0] * c[1] - b[1] * c[0]);
    (determinant != 0).then_some(clockwise ^ (determinant > 0))
}

#[cfg(test)]
mod tests {
    use super::super::name_smiles;
    use crate::naming::canonical_smiles;
    use nazuchem_ir::{NameReason, NameStatus};
    use nazuchem_kernel::smiles;

    #[test]
    fn cage_relative_names_keep_cip_primary_names_in_multiple_atom_orders() {
        for (input, primary, alternative) in [
            (
                "O[C@H]1C[C@@H]2CC[C@H]1C2",
                "(1S,2S,4R)-bicyclo[2.2.1]heptan-2-ol",
                "bicyclo[2.2.1]heptan-2-exo-ol",
            ),
            (
                "O[C@@H]1C[C@@H]2CC[C@H]1C2",
                "(1S,2R,4R)-bicyclo[2.2.1]heptan-2-ol",
                "bicyclo[2.2.1]heptan-2-endo-ol",
            ),
            (
                "CN1[C@@H]2CC[C@H]1C[C@@H](O)C2",
                "(1R,3r,5S)-8-methyl-8-azabicyclo[3.2.1]octan-3-ol",
                "8-methyl-8-azabicyclo[3.2.1]octan-3-endo-ol",
            ),
            (
                "CN1[C@@H]2CC[C@H]1C[C@H](O)C2",
                "(1R,3s,5S)-8-methyl-8-azabicyclo[3.2.1]octan-3-ol",
                "8-methyl-8-azabicyclo[3.2.1]octan-3-exo-ol",
            ),
            (
                "O[C@H]1C[C@H]2CC[C@@H](C1)N2",
                "(1R,3r,5S)-8-azabicyclo[3.2.1]octan-3-ol",
                "8-azabicyclo[3.2.1]octan-3-endo-ol",
            ),
            (
                "Br[C@@H]1C[C@@H]2CC[C@H]1[C@@H]2F",
                "(1S,2R,4S,7R)-2-bromo-7-fluorobicyclo[2.2.1]heptane",
                "2-endo-bromo-7-syn-fluorobicyclo[2.2.1]heptane",
            ),
            (
                "Br[C@@H]1C[C@@H]2CC[C@H]1[C@H]2F",
                "(1S,2R,4S,7S)-2-bromo-7-fluorobicyclo[2.2.1]heptane",
                "2-endo-bromo-7-anti-fluorobicyclo[2.2.1]heptane",
            ),
            (
                "C[C@H]1[C@@H]2CCC[C@H]1CC2",
                "(1R,5S,8r)-8-methylbicyclo[3.2.1]octane",
                "8-anti-methylbicyclo[3.2.1]octane",
            ),
            (
                "C[C@@H]1[C@@H]2CCC[C@H]1CC2",
                "(1R,5S,8s)-8-methylbicyclo[3.2.1]octane",
                "8-syn-methylbicyclo[3.2.1]octane",
            ),
            (
                "O[C@H]1C[C@@H]2C=C[C@H]1C2",
                "(1R,2S,4R)-bicyclo[2.2.1]hept-5-en-2-ol",
                "bicyclo[2.2.1]hept-5-en-2-exo-ol",
            ),
            (
                "O[C@H]1[C@@H]2CC[C@H]1C[C@@H](O)C2",
                "(1R,3r,5S,8r)-bicyclo[3.2.1]octane-3,8-diol",
                "bicyclo[3.2.1]octane-3-endo,8-syn-diol",
            ),
            (
                "C[C@H]1[C@@H]2CC[C@H]1C[C@@H](C)C2",
                "(1R,3r,5S,8r)-3,8-dimethylbicyclo[3.2.1]octane",
                "3-endo,8-syn-dimethylbicyclo[3.2.1]octane",
            ),
            (
                "CN1[C@@H]2CC[C@H]1C[C@@H](OC(=O)C)C2",
                "(1R,3r,5S)-8-methyl-8-azabicyclo[3.2.1]octan-3-yl acetate",
                "8-methyl-8-azabicyclo[3.2.1]octan-3-endo-yl acetate",
            ),
        ] {
            let identity = canonical_smiles(input).unwrap();
            for order in atom_orders(input) {
                assert_eq!(canonical_smiles(&order).unwrap(), identity, "{order}");
                let name = name_smiles(&order, &Default::default()).unwrap();
                assert_eq!(name.text, primary, "{order}");
                assert_ne!(
                    name.status,
                    NameStatus::NotPreferred(NameReason::EndoExoStereo)
                );
                assert_eq!(
                    name.alternatives.len(),
                    1,
                    "{order}: {:?}",
                    name.alternatives
                );
                assert_eq!(name.alternatives[0].name, alternative, "{order}");
                assert_eq!(
                    name.alternatives[0].status,
                    NameStatus::NotPreferred(NameReason::EndoExoStereo)
                );
            }
            assert_round_trip(primary, &identity, false);
            assert_round_trip(alternative, &identity, true);
        }
    }

    fn atom_orders(input: &str) -> [String; 2] {
        let prepared = smiles::prepare(input, &Default::default()).unwrap();
        let reordered = smiles::write::write(
            &prepared.state,
            smiles::write::Options {
                canonical: false,
                root: Some(prepared.state.graph.atoms.len() - 1),
                ..Default::default()
            },
        )
        .unwrap()
        .text;
        assert_ne!(input, reordered);
        [input.to_owned(), reordered]
    }

    fn assert_round_trip(name: &str, expected: &str, relative: bool) {
        let request = crate::naming::local::verification_request(name).unwrap();
        let reply = crate::naming::worker::respond(&request).unwrap();
        let parsed = crate::naming::local::verification_result(name, &reply).unwrap();
        let result = if relative {
            crate::naming::reverse::verify_alternative_identity(
                expected,
                &parsed.smiles,
                NameStatus::NotPreferred(NameReason::EndoExoStereo),
            )
        } else {
            crate::naming::verify_identity(expected, &parsed.smiles)
        };
        result.unwrap_or_else(|error| panic!("{name}: {error}"));
    }

    #[test]
    fn cage_relative_names_place_each_descriptor_beside_its_locant() {
        let input = "O[C@H]1C[C@@H]2[C@H](C)C[C@H]1[C@H]2C";
        let expected = "5-endo,7-anti-dimethylbicyclo[2.2.1]heptan-2-exo-ol";
        for order in atom_orders(input) {
            let name = name_smiles(&order, &Default::default()).unwrap();
            assert_eq!(name.alternatives[0].name, expected, "{order}");
        }
        assert_round_trip(expected, &canonical_smiles(input).unwrap(), true);
    }

    #[test]
    fn cage_relative_names_round_trip_with_substituent_scopes_and_alkene_geometry() {
        for (input, alternative) in [
            (
                "OCC[C@H]1C[C@@H]2CC[C@H]1C2",
                "2-(bicyclo[2.2.1]heptan-2-exo-yl)ethan-1-ol",
            ),
            (
                "F/C=C/[C@H]1C[C@@H]2CC[C@H]1C2",
                "2-exo-[(1E)-2-fluoroethenyl]bicyclo[2.2.1]heptane",
            ),
        ] {
            let identity = canonical_smiles(input).unwrap();
            for order in atom_orders(input) {
                let name = name_smiles(&order, &Default::default()).unwrap();
                assert_eq!(name.alternatives.len(), 1, "{order}");
                assert_eq!(name.alternatives[0].name, alternative, "{order}");
                assert_eq!(
                    name.alternatives[0].status,
                    NameStatus::NotPreferred(NameReason::EndoExoStereo)
                );
                assert_round_trip(&name.text, &identity, false);
            }
            assert_round_trip(alternative, &identity, true);
        }
    }

    #[test]
    fn cage_relative_names_keep_descriptors_when_spirobi_is_ineligible() {
        for (input, primary, alternative) in [
            (
                "C12([C@@H](Br)[C@@H]3CC[C@H]1C3)CC1CCC2C1",
                "(1S,3S,4R)-3-bromospiro[bicyclo[2.2.1]heptane-2,2'-bicyclo[2.2.1]heptane]",
                "3-exo-bromospiro[bicyclo[2.2.1]heptane-2,2'-bicyclo[2.2.1]heptane]",
            ),
            (
                "C12([C@H](Br)[C@@H]3CC[C@H]1C3)CC1CCC2C1",
                "(1S,3R,4R)-3-bromospiro[bicyclo[2.2.1]heptane-2,2'-bicyclo[2.2.1]heptane]",
                "3-endo-bromospiro[bicyclo[2.2.1]heptane-2,2'-bicyclo[2.2.1]heptane]",
            ),
        ] {
            let identity = canonical_smiles(input).unwrap();
            for order in atom_orders(input) {
                assert_eq!(canonical_smiles(&order).unwrap(), identity, "{order}");
                let name = name_smiles(&order, &Default::default()).unwrap();
                assert_eq!(name.text, primary, "{order}");
                assert_eq!(
                    name.status,
                    NameStatus::Unverified(NameReason::UnauditedParent),
                    "{order}"
                );
                assert_eq!(
                    name.alternatives.len(),
                    1,
                    "{order}: {:?}",
                    name.alternatives
                );
                assert_eq!(name.alternatives[0].name, alternative, "{order}");
                assert_eq!(
                    name.alternatives[0].status,
                    NameStatus::NotPreferred(NameReason::EndoExoStereo)
                );
            }
            assert_round_trip(primary, &identity, false);
            assert_round_trip(alternative, &identity, true);
        }
    }

    #[test]
    fn cage_relative_names_decline_other_stereo_and_unsupported_centres() {
        for input in [
            // A side-chain stereocentre cannot be represented by cage faces.
            "CN1[C@@H]2CC[C@H]1C[C@@H](OC(=O)[C@H](CO)c1ccccc1)C2",
            // Acid-side fragment naming must also see alcohol-side stereo.
            "CC[C@H](C)OC(=O)[C@H]1C[C@@H]2CC[C@H]1C2",
            // A capped acid fragment in an N-acyl prefix shares the budget.
            "OC(=O)[C@H](C)NC(=O)[C@H]1C[C@@H]2CC[C@H]1C2",
            // One bridgehead is unspecified.
            "O[C@H]1CC2CC[C@H]1C2",
            // The defined nonbridgehead has two substituents and no H.
            "C[C@]1(O)C[C@@H]2CC[C@H]1C2",
            // A different bridge-length system.
            "O[C@H]1C[C@@H]2CC[C@H]1CC2",
        ] {
            let name = name_smiles(input, &Default::default()).unwrap();
            assert!(
                name.alternatives
                    .iter()
                    .all(|alternative| alternative.status
                        != NameStatus::NotPreferred(NameReason::EndoExoStereo)),
                "{input}: {:?}",
                name.alternatives
            );
        }
    }
}
