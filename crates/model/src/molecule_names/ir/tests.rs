// SPDX-License-Identifier: MIT OR Apache-2.0
// Origin: original ReShiki code

use super::*;
use crate::molecule_names::{canonical_identity, unknown_atom_units};
use nazuchem_ir::{Atom as IrAtom, Bond as IrBond, StereoGroup as IrGroup};
use nazuchem_n2s::{ParseOptions, Parser, Structure};
use std::sync::OnceLock;

fn structure(name: &str) -> Structure {
    static PARSER: OnceLock<Parser> = OnceLock::new();
    let parsed = PARSER
        .get_or_init(|| Parser::new().unwrap())
        .parse(name, &ParseOptions::strict());
    parsed
        .structure
        .unwrap_or_else(|| panic!("{name}: {}", parsed.message))
}

fn identity(molecule: &Molecule) -> String {
    let prepared = from_ir(molecule).unwrap();
    let (projected, _) = name_import_projection(&prepared).unwrap();
    canonical_identity(&projected.state).unwrap()
}

fn cip(molecule: &Molecule) -> Vec<String> {
    let state = stereo::cip::label::assign(&from_ir(molecule).unwrap().state, &Default::default())
        .unwrap()
        .state;
    state
        .properties
        .atoms
        .iter()
        .filter_map(|atom| atom.cip_code.clone())
        .chain(state.properties.bond_codes.into_iter().flatten())
        .collect()
}

#[test]
fn direct_named_graphs_match_legacy_import_identity() {
    for name in [
        "ethanol",
        "pyrrole",
        "naphthalene",
        "sodium acetate",
        "[13C]methane",
        "(2R)-butan-2-ol",
        "(2S)-butan-2-ol",
        "(R)-bromochlorofluoroiodomethane",
        "(E)-but-2-ene",
        "(Z)-but-2-ene",
        "(2E,4Z)-hexa-2,4-diene",
    ] {
        let source = structure(name);
        let molecule = source.to_ir().unwrap();
        let native = from_ir(&molecule).unwrap();
        let (projected, survivors) = name_import_projection(&native).unwrap();
        let legacy = chemistry::smiles::read(&source.semantic_cxsmiles().unwrap())
            .unwrap()
            .prepared
            .state;
        assert_eq!(
            canonical_identity(&projected.state),
            canonical_identity(&legacy),
            "{name}"
        );
        assert_eq!(
            native.state.graph.atoms.len(),
            molecule.atoms.len(),
            "{name}"
        );
        assert_eq!(survivors.len(), projected.state.graph.atoms.len(), "{name}");
        for (bond, original) in native.state.graph.bonds.iter().zip(&molecule.bonds) {
            assert_eq!([bond.a as u32, bond.b as u32], original.atoms, "{name}");
        }
    }
}

#[test]
fn aromatic_anion_salts_keep_vinyl_configurations() {
    for descriptor in ["E", "Z"] {
        let name = format!("sodium 3-[(1{descriptor})-2-(4-hydroxyphenyl)ethenyl]-1H-pyrrol-1-ide");
        let source = structure(&name);
        let molecule = source.to_ir().unwrap();
        let native =
            from_ir(&molecule).unwrap_or_else(|error| panic!("{name}: IR preparation: {error}"));
        let (projected, _) = name_import_projection(&native)
            .unwrap_or_else(|error| panic!("{name}: import projection: {error}"));
        assert_eq!(
            native.state.graph.atoms.len(),
            projected.state.graph.atoms.len()
        );
        assert!(
            projected
                .state
                .metadata
                .bonds
                .iter()
                .any(|meta| matches!(meta.stereo, 4 | 5))
        );
        let legacy = chemistry::smiles::read(&source.semantic_cxsmiles().unwrap())
            .unwrap()
            .prepared
            .state;
        assert_eq!(
            canonical_identity(&projected.state),
            canonical_identity(&legacy),
            "{name}"
        );
    }
}

#[test]
fn stereogenic_hydrogen_removal_keeps_salt_alkenes_structural() {
    for descriptor in ["E", "Z"] {
        let name = format!("sodium (2R,4{descriptor})-2-hydroxyhex-4-enoate");
        let source = structure(&name);
        let molecule = source.to_ir().unwrap();
        let native = from_ir(&molecule).unwrap();
        let (projected, _) = name_import_projection(&native).unwrap();
        assert!(
            native.state.graph.atoms.len() > projected.state.graph.atoms.len(),
            "{name}"
        );
        assert!(
            projected
                .state
                .metadata
                .bonds
                .iter()
                .any(|meta| matches!(meta.stereo, 4 | 5)),
            "{name}"
        );
        assert!(
            projected
                .state
                .directions
                .iter()
                .all(|direction| *direction == Direction::None)
        );
        let legacy = chemistry::smiles::read(&source.semantic_cxsmiles().unwrap())
            .unwrap()
            .prepared
            .state;
        assert_eq!(
            canonical_identity(&projected.state),
            canonical_identity(&legacy),
            "{name}"
        );
    }
}

#[test]
fn hydrogen_origin_and_annotated_hydrogen_vertices_remain_exact() {
    for stated in [false, true] {
        let molecule = Molecule::new(
            vec![IrAtom {
                element: 6,
                hydrogens: 4,
                hydrogens_stated: stated,
                ..Default::default()
            }],
            vec![],
        )
        .unwrap();
        let state = from_ir(&molecule).unwrap().state;
        assert_eq!(
            state.graph.atoms[0].explicit_hydrogens,
            if stated { 4 } else { 0 }
        );
        assert_eq!(state.graph.atoms[0].no_implicit, stated);
    }
    let mut pyrrole = structure("pyrrole").to_ir().unwrap();
    let nitrogen = pyrrole
        .atoms
        .iter()
        .position(|atom| atom.element == 7)
        .unwrap();
    pyrrole.atoms[nitrogen].hydrogens_stated = false;
    let state = from_ir(&pyrrole).unwrap().state;
    assert_eq!(state.graph.atoms[nitrogen].explicit_hydrogens, 1);
    assert!(state.graph.atoms[nitrogen].no_implicit);

    let mut molecule = Molecule::new(
        vec![
            IrAtom {
                element: 1,
                isotope: Some(2),
                ..Default::default()
            },
            IrAtom {
                element: 8,
                ..Default::default()
            },
            IrAtom {
                element: 6,
                isotope: Some(13),
                hydrogens: 3,
                atom_map: Some(0),
                ..Default::default()
            },
            IrAtom {
                element: 1,
                locants: vec!["H".into()],
                ..Default::default()
            },
            IrAtom {
                element: 6,
                hydrogens: 3,
                ..Default::default()
            },
        ],
        vec![
            IrBond {
                atoms: [0, 1],
                order: BondOrder::Single,
            },
            IrBond {
                atoms: [1, 2],
                order: BondOrder::Single,
            },
            IrBond {
                atoms: [3, 4],
                order: BondOrder::Single,
            },
        ],
    )
    .unwrap();
    molecule.parent = vec![3];
    let state = from_ir(&molecule).unwrap().state;
    assert_eq!(state.graph.atoms.len(), 5);
    assert_eq!(state.graph.atoms[0].isotope, 2);
    assert_eq!(state.graph.atoms[2].isotope, 13);
    assert!(state.metadata.atoms[2].map_present);
    assert_eq!(state.metadata.atoms[2].map_number, 0);
    assert_eq!(state.graph.atoms[3].atomic_number, 1);
    let prepared = from_ir(&molecule).unwrap();
    let (projected, survivors) = name_import_projection(&prepared).unwrap();
    assert_eq!(survivors, [0, 1, 2, 4]);
    assert_eq!(projected.state.graph.atoms[0].isotope, 2);
    assert_eq!(prepared.state.graph.atoms.len(), 5);
    assert_eq!(projected.state.graph.atoms.len(), 4);
}

#[test]
fn wildcard_radical_charge_and_map_are_independent() {
    let molecule = Molecule::new(
        vec![
            IrAtom::default(),
            IrAtom {
                element: 6,
                hydrogens: 3,
                radical_electrons: 1,
                ..Default::default()
            },
            IrAtom {
                element: 7,
                hydrogens: 4,
                charge: 1,
                atom_map: Some(900_004),
                ..Default::default()
            },
            IrAtom {
                element: 17,
                charge: -1,
                ..Default::default()
            },
        ],
        vec![],
    )
    .unwrap();
    let state = from_ir(&molecule).unwrap().state;
    assert_eq!(state.graph.atoms[0].atomic_number, 0);
    assert_eq!(state.graph.atoms[0].radical_electrons, 0);
    assert_eq!(state.graph.atoms[1].radical_electrons, 1);
    assert_eq!(state.graph.atoms[2].charge, 1);
    assert_eq!(state.graph.atoms[3].charge, -1);
    assert!(state.metadata.atoms.iter().all(|atom| atom.chiral_tag == 0));
    assert!(state.metadata.bonds.iter().all(|bond| bond.stereo == 0));
    let mut invalid = molecule;
    invalid.atoms[1].radical_electrons = 0;
    assert!(
        from_ir(&invalid)
            .unwrap_err()
            .contains("radical electron count")
    );
}

#[test]
fn tetrahedral_reference_permutations_and_both_hands_match_cip() {
    for (name, expected) in [("(2R)-butan-2-ol", "R"), ("(2S)-butan-2-ol", "S")] {
        let mut molecule = structure(name).to_ir().unwrap();
        assert_eq!(cip(&molecule), [expected]);
        let original = identity(&molecule);
        for entry in &mut molecule.stereo {
            if let Stereo::Tetrahedral {
                neighbors, winding, ..
            } = entry
            {
                neighbors.swap(0, 1);
                *winding = if *winding == Winding::Clockwise {
                    Winding::Anticlockwise
                } else {
                    Winding::Clockwise
                };
            }
        }
        assert_eq!(cip(&molecule), [expected]);
        assert_eq!(identity(&molecule), original);
    }
}

fn substituted_alkene() -> Molecule {
    let mut molecule = Molecule::new(
        [6, 6, 9, 17, 35, 53]
            .into_iter()
            .map(|element| IrAtom {
                element,
                ..Default::default()
            })
            .collect(),
        vec![
            IrBond {
                atoms: [0, 1],
                order: BondOrder::Double,
            },
            IrBond {
                atoms: [0, 2],
                order: BondOrder::Single,
            },
            IrBond {
                atoms: [0, 3],
                order: BondOrder::Single,
            },
            IrBond {
                atoms: [1, 4],
                order: BondOrder::Single,
            },
            IrBond {
                atoms: [1, 5],
                order: BondOrder::Single,
            },
        ],
    )
    .unwrap();
    molecule.stereo.push(Stereo::DoubleBond {
        bond: 0,
        refs: [2, 4],
        relation: Relation::Trans,
    });
    molecule
}

#[test]
fn double_bond_references_are_structural_and_endpoint_order_independent() {
    let mut molecule = substituted_alkene();
    let original = identity(&molecule);
    assert_eq!(cip(&molecule), ["E"]);
    molecule.stereo[0] = Stereo::DoubleBond {
        bond: 0,
        refs: [3, 4],
        relation: Relation::Cis,
    };
    assert_eq!(identity(&molecule), original);
    molecule.bonds[0].atoms.reverse();
    molecule.stereo[0] = Stereo::DoubleBond {
        bond: 0,
        refs: [4, 3],
        relation: Relation::Cis,
    };
    assert_eq!(identity(&molecule), original);
}

#[test]
fn alkene_configuration_can_distinguish_tetrahedral_ligands() {
    // F-C(Cl)(CH=CH-CH3)(CH=CH-CH3), with opposite alkene relations.
    // The alkene configurations supply the center's only ligand distinction.
    let mut molecule = Molecule::new(
        [9, 6, 17, 6, 6, 6, 6, 6, 6]
            .into_iter()
            .enumerate()
            .map(|(index, element)| IrAtom {
                element,
                hydrogens: match index {
                    3 | 4 | 6 | 7 => 1,
                    5 | 8 => 3,
                    _ => 0,
                },
                ..Default::default()
            })
            .collect(),
        [
            (0, 1, false),
            (1, 2, false),
            (1, 3, false),
            (3, 4, true),
            (4, 5, false),
            (1, 6, false),
            (6, 7, true),
            (7, 8, false),
        ]
        .into_iter()
        .map(|(a, b, double)| IrBond {
            atoms: [a, b],
            order: if double {
                BondOrder::Double
            } else {
                BondOrder::Single
            },
        })
        .collect(),
    )
    .unwrap();
    molecule.stereo = vec![
        Stereo::Tetrahedral {
            center: 1,
            neighbors: [Ref::Atom(0), Ref::Atom(2), Ref::Atom(3), Ref::Atom(6)],
            winding: Winding::Clockwise,
        },
        Stereo::DoubleBond {
            bond: 3,
            refs: [1, 5],
            relation: Relation::Trans,
        },
        Stereo::DoubleBond {
            bond: 6,
            refs: [1, 8],
            relation: Relation::Cis,
        },
    ];
    let state = from_ir(&molecule).unwrap().state;
    assert_ne!(state.metadata.atoms[1].chiral_tag, 0);
    assert!(
        state
            .directions
            .iter()
            .all(|direction| *direction == Direction::None)
    );
    let clockwise = identity(&molecule);
    if let Stereo::Tetrahedral { winding, .. } = &mut molecule.stereo[0] {
        *winding = Winding::Anticlockwise;
    }
    assert_ne!(identity(&molecule), clockwise);
    if let Stereo::DoubleBond { relation, .. } = &mut molecule.stereo[2] {
        *relation = Relation::Trans;
    }
    assert_eq!(from_ir(&molecule).unwrap_err(), TETRAHEDRAL_LOSS);
}

#[test]
fn both_axial_hands_and_permuted_references_retain_cip_and_unknown() {
    for hand in ["M", "P"] {
        let name = format!("(1{hand})-[1,1'-binaphthalene]-2,2'-diol");
        let mut molecule = structure(&name).to_ir().unwrap();
        assert_eq!(cip(&molecule), [hand]);
        let original = identity(&molecule);
        for entry in &mut molecule.stereo {
            if let Stereo::Atropisomer { refs, winding, .. } = entry {
                refs.swap(0, 1);
                *winding = if *winding == AxialWinding::Clockwise {
                    AxialWinding::Anticlockwise
                } else {
                    AxialWinding::Clockwise
                };
            }
        }
        assert_eq!(cip(&molecule), [hand]);
        assert_eq!(identity(&molecule), original);
        for entry in &mut molecule.stereo {
            if let Stereo::Atropisomer { winding, .. } = entry {
                *winding = AxialWinding::Unknown;
            }
        }
        let state = from_ir(&molecule).unwrap().state;
        assert_eq!(
            state
                .metadata
                .bonds
                .iter()
                .filter(|bond| bond.stereo == 1 && bond.unknown_stereo)
                .count(),
            1
        );
        assert!(cip(&molecule).is_empty());
    }
}

#[test]
fn unknown_tetrahedral_and_double_bond_annotations_are_retained() {
    let mut molecule = structure("(2R)-butan-2-ol").to_ir().unwrap();
    for entry in &mut molecule.stereo {
        if let Stereo::Tetrahedral { winding, .. } = entry {
            *winding = Winding::Unknown;
        }
    }
    let state = from_ir(&molecule).unwrap().state;
    assert_eq!(
        unknown_atom_units(&state)
            .iter()
            .filter(|&&unknown| unknown)
            .count(),
        1
    );
    assert!(state.metadata.atoms.iter().all(|atom| atom.chiral_tag == 0));
    let mut molecule = substituted_alkene();
    molecule.stereo[0] = Stereo::DoubleBond {
        bond: 0,
        refs: [2, 4],
        relation: Relation::Unknown,
    };
    let state = from_ir(&molecule).unwrap().state;
    assert_eq!(state.metadata.bonds[0].stereo, 1);
    assert!(state.metadata.bonds[0].unknown_stereo);
    assert_eq!(state.metadata.bonds[0].stereo_atoms, [2, 4]);
}

#[test]
fn enhanced_groups_remain_typed_and_unsupported_metadata_is_rejected() {
    let mut molecule = structure("(2R)-butan-2-ol").to_ir().unwrap();
    for (kind, expected) in [(StereoGroupKind::And(7), 2), (StereoGroupKind::Or(9), 1)] {
        molecule.stereo_groups = vec![IrGroup {
            kind,
            stereo: vec![0],
        }];
        let state = from_ir(&molecule).unwrap().state;
        assert_eq!(state.metadata.groups.len(), 1);
        assert_eq!(state.metadata.groups[0].kind, expected);
        assert_eq!(state.metadata.groups[0].atoms.len(), 1);
        let (projected, _) = name_import_projection(&from_ir(&molecule).unwrap()).unwrap();
        assert_eq!(projected.state.metadata.groups[0].kind, expected);
        assert_eq!(projected.state.metadata.groups[0].atoms.len(), 1);
    }
    molecule.stereo_groups.clear();
    molecule.atoms[0].label = Some("R".into());
    assert_eq!(from_ir(&molecule).unwrap_err(), METADATA);
    molecule.atoms[0].label = None;
    molecule.atoms[0].attachment_point = Some(1);
    assert_eq!(from_ir(&molecule).unwrap_err(), METADATA);
    molecule.atoms[0].attachment_point = None;
    molecule.bonds[0].atoms = [u32::MAX, 0];
    assert!(from_ir(&molecule).is_err());
}

#[test]
fn retained_hydrogen_vertices_do_not_consume_the_legacy_atom_token_limit() {
    let mut molecule = Molecule::default();
    for index in 0..256 {
        let carbon = molecule.atoms.len() as u32;
        molecule.atoms.push(IrAtom {
            element: 6,
            hydrogens: 3,
            ..Default::default()
        });
        molecule.atoms.push(IrAtom {
            element: 1,
            locants: vec![format!("H{index}")],
            ..Default::default()
        });
        molecule.bonds.push(IrBond {
            atoms: [carbon, carbon + 1],
            order: BondOrder::Single,
        });
    }
    molecule.atoms.push(IrAtom {
        element: 6,
        hydrogens: 4,
        ..Default::default()
    });
    assert_eq!(molecule.atoms.len(), MAX_NAMING_ATOMS + 1);
    let prepared = from_ir(&molecule).unwrap();
    let (projected, survivors) = name_import_projection(&prepared).unwrap();
    assert_eq!(prepared.state.graph.atoms.len(), 513);
    assert_eq!(projected.state.graph.atoms.len(), 257);
    assert_eq!(survivors, (0..513).step_by(2).collect::<Vec<_>>());
    molecule.atoms.resize(MAX_IR_ATOMS + 1, IrAtom::default());
    assert!(from_ir(&molecule).unwrap_err().contains("import size"));
}
