// SPDX-License-Identifier: MIT OR Apache-2.0
// Origin: original
// Source: ReShiki | src/naming/openclatura/spirobi_tests.rs | 5f559fd482e237307a589dfcb7ea333cee0a6e13
// Copyright (c) 2026 Ameyanagi and ReShiki contributors

//! P-24.3 spirobi forms, numbering, certification, and reconstruction.
use super::{component_namer, graph_io, molecule::Molecule, name_smiles};
use crate::naming::{NameReason, NameStatus};
use std::collections::BTreeMap;

type Case = (&'static str, &'static str, &'static str, NameStatus);
const UNVERIFIED: NameStatus = NameStatus::Unverified(NameReason::UnauditedParent);

const MATRIX: &[Case] = &[
    (
        "D1",
        "C1=CC2(C3=CC=CC=C13)C=CC1=CC=CC=C12",
        "1,1'-spirobi[indene]",
        NameStatus::Preferred,
    ),
    (
        "D2",
        "CC1=CC2(C3=CC=CC=C13)C=CC1=CC=CC=C12",
        "3-methyl-1,1'-spirobi[indene]",
        NameStatus::Preferred,
    ),
    (
        "D3",
        "C12(CC3CCC1C3)C1CCC2CC1",
        "2,7'-spirobi[bicyclo[2.2.1]heptane]",
        NameStatus::Preferred,
    ),
    (
        "D4",
        "C12(CC3CCC1C3)CC1CCC2O1",
        "7-oxa-2,2'-spirobi[bicyclo[2.2.1]heptane]",
        NameStatus::Preferred,
    ),
    (
        "D5",
        "C12(CC3CCC1C3)CC1CCC2C1",
        "2,2'-spirobi[bicyclo[2.2.1]heptane]",
        NameStatus::Preferred,
    ),
    (
        "D6",
        "C12(C3CCC1CC3)C1CCC2CC1",
        "7,7'-spirobi[bicyclo[2.2.1]heptane]",
        NameStatus::Preferred,
    ),
    (
        "D7",
        "C1CCC2C(C1)C23C4CCCCC34",
        "7,7'-spirobi[bicyclo[4.1.0]heptane]",
        NameStatus::Preferred,
    ),
    (
        "D8",
        "C12(CC3CCC1C3)CC1CCC2S1",
        "7-thia-2,2'-spirobi[bicyclo[2.2.1]heptane]",
        NameStatus::Preferred,
    ),
    (
        "D9",
        "C12(CC3CCC1O3)CC1CCC2O1",
        "7,7'-dioxa-2,2'-spirobi[bicyclo[2.2.1]heptane]",
        NameStatus::Preferred,
    ),
    (
        "D10",
        "C1=CC2(Oc3ccccc31)C=COc1ccccc12",
        "2,4'-spirobi[[1]benzopyran]",
        NameStatus::Preferred,
    ),
    (
        "D11",
        "c1ccc2c(c1)CC1(S2)Cc2ccccc2S1",
        "3H,3'H-2,2'-spirobi[[1]benzothiophene]",
        NameStatus::Preferred,
    ),
    (
        "D12",
        "C1Cc2ccccc2C12CCc1ccccc12",
        "2,2',3,3'-tetrahydro-1,1'-spirobi[indene]",
        UNVERIFIED,
    ),
    (
        "D17-unsaturation",
        "C12(CC3C=CC1C3)CC1CCC2C1",
        "2,2'-spirobi[bicyclo[2.2.1]heptan]-5-ene",
        NameStatus::Preferred,
    ),
    (
        "D18-dithia",
        "C12(CC3SCC1CC3)CC3CSC2CC3",
        "5,6'-dithia-2,2'-spirobi[bicyclo[2.2.2]octane]",
        NameStatus::Preferred,
    ),
    (
        "D19-mixed",
        "C12(CC3COC1C3)CC3CSC2C3",
        "6-oxa-6'-thia-2,2'-spirobi[bicyclo[2.2.1]heptane]",
        NameStatus::Preferred,
    ),
    (
        "D20-asym-benzothiophene",
        "c1ccc2c(c1)CC1(S2)CSc2ccccc21",
        "2'H,3H-2,3'-spirobi[[1]benzothiophene]",
        NameStatus::Preferred,
    ),
    (
        "unsaturation-compound-count",
        // P-31.1.4.2(1): 1-ene / 5-fluoro beats 1(6)-ene / 3-fluoro.
        "C12(C3=CCC1C(F)C3)C1CCC2CC1",
        "5-fluoro-7,7'-spirobi[bicyclo[2.2.1]heptan]-1-ene",
        NameStatus::Preferred,
    ),
    (
        "unsaturation-parenthesized-locants",
        // P-31.1.4.2(3): [1,5] beats [1,6] after compound count and primary ties.
        "C12(CC3C=C1C(F)3)CC1CC2C1",
        "6-fluoro-2,2'-spirobi[bicyclo[2.1.1]hexan]-1(5)-ene",
        NameStatus::Preferred,
    ),
    (
        "unsaturation-multiple-before-double",
        // P-31.1.4.3(2): multiple-bond set [2,5] beats [3,6], despite ene 5 > 3.
        "C12(C3C#CCC=CCC1CC3)C1CCCCCCC2CC1",
        "11,11'-spirobi[bicyclo[6.2.1]undecan]-5-en-2-yne",
        NameStatus::Preferred,
    ),
    (
        "unsaturation-double-before-substituent",
        // P-31.1.4.3(3): [2,5] ties; ene 2 beats ene 5, despite fluoro 7 > 1.
        "C12(C3C=CCC#CC1(F)CC3)C1CCCCCC2CC1",
        "7-fluoro-10,10'-spirobi[bicyclo[5.2.1]decan]-2-en-5-yne",
        NameStatus::Preferred,
    ),
    (
        "fusion-letter-hydro",
        "C1=CC2(C3=CC=CCC13)C=CC1=CC=CCC12",
        "3a,4,7',7'a-tetrahydro-1,1'-spirobi[indene]",
        UNVERIFIED,
    ),
];

const CONTROLS: &[Case] = &[
    (
        "D13",
        "C1CCC2(C1)CCCC2",
        "spiro[4.4]nonane",
        NameStatus::Preferred,
    ),
    (
        "D14",
        "C1CC2(CCN1)CCNCC2",
        "3,9-diazaspiro[5.5]undecane",
        NameStatus::Preferred,
    ),
    (
        "D15",
        "C1=CC2(c3ccccc13)N=Cc1ccccc12",
        "spiro[indene-1,1'-isoindole]",
        // Different skeletons now have a completed-parent P-24.5 proof.
        NameStatus::Preferred,
    ),
    (
        "D16",
        "c1ccc2c(c1)OC1(CCCC1)O2",
        "spiro[[1,3]benzodioxole-2,1'-cyclopentane]",
        NameStatus::Preferred,
    ),
];

const SUFFIX_BOUNDARIES: &[Case] = &[
    (
        "ketone",
        "C12(C(=O)C3CCC1C3)CC1CCC2C1",
        "2,2'-spirobi[bicyclo[2.2.1]heptan]-3-one",
        UNVERIFIED,
    ),
    (
        "alcohol",
        "C12(C(O)C3CCC1C3)CC1CCC2C1",
        "2,2'-spirobi[bicyclo[2.2.1]heptan]-3-ol",
        UNVERIFIED,
    ),
    (
        "diol",
        "C12(C(O)C3CCC1C3)C(O)C1CCC2C1",
        "2,2'-spirobi[bicyclo[2.2.1]heptane]-3,3'-diol",
        UNVERIFIED,
    ),
];

const UNCONVERTED_BOUNDARIES: &[Case] = &[
    (
        "different seven-atom von Baeyer parents",
        "C12(CC3CCC1C3)CCC1CC2C1",
        "spiro[bicyclo[2.2.1]heptane-2,2'-bicyclo[3.1.1]heptane]",
        UNVERIFIED,
    ),
    (
        "isotope",
        "C12([13CH2]C3CCC1C3)CC1CCC2C1",
        "(3-13C)spiro[bicyclo[2.2.1]heptane-2,2'-bicyclo[2.2.1]heptane]",
        UNVERIFIED,
    ),
    (
        "stereochemistry",
        "C12([C@H](O)C3CCC1C3)CC1CCC2C1",
        "(2'R)-spiro[bicyclo[2.2.1]heptane-2,3'-bicyclo[2.2.1]heptane]-2'-ol",
        UNVERIFIED,
    ),
];

#[test]
fn spirobi_matrix_names_and_statuses() {
    for &(row, smiles, expected, status) in MATRIX {
        let actual = name_smiles(smiles, &Default::default())
            .unwrap_or_else(|error| panic!("{row}: {error}"));
        assert_eq!(actual.text, expected, "{row}: {smiles}");
        assert_eq!(actual.status, status, "{row}: {}", actual.text);
    }
}

#[test]
fn spirobi_controls_keep_names_and_statuses() {
    for &(row, smiles, expected, status) in CONTROLS {
        let actual = name_smiles(smiles, &Default::default())
            .unwrap_or_else(|error| panic!("{row}: {error}"));
        assert_eq!(actual.text, expected, "{row}: {smiles}");
        assert_eq!(actual.status, status, "{row}: {}", actual.text);
    }
}

#[test]
fn spirobi_suffixes_are_elided_but_remain_unverified() {
    // One -ol elides the parent terminal e; multiplied -diol retains it.
    // Correct text alone does not certify principal-suffix spirobi naming.
    for &(row, smiles, expected, status) in SUFFIX_BOUNDARIES {
        let actual = name_smiles(smiles, &Default::default())
            .unwrap_or_else(|error| panic!("{row}: {error}"));
        assert_eq!(actual.text, expected, "{row}: {smiles}");
        assert_eq!(actual.status, status, "{row}: {}", actual.text);
    }
}

#[test]
fn spirobi_scope_boundaries_keep_generic_spiro_names() {
    for &(row, smiles, expected, status) in UNCONVERTED_BOUNDARIES {
        let actual = name_smiles(smiles, &Default::default())
            .unwrap_or_else(|error| panic!("{row}: {error}"));
        assert_eq!(actual.text, expected, "{row}: {smiles}");
        assert_eq!(actual.status, status, "{row}: {}", actual.text);
    }
}

#[test]
fn spirobi_target_names_reconstruct_matrix_graphs() {
    let parser = opsin::Parser::new().unwrap();
    for &(row, smiles, expected, _) in MATRIX
        .iter()
        .chain(CONTROLS)
        .chain(SUFFIX_BOUNDARIES)
        .chain(UNCONVERTED_BOUNDARIES)
    {
        let parsed = parser.parse(expected, &opsin::ParseOptions::strict());
        assert_eq!(
            parsed.status,
            opsin::Status::Success,
            "{row}: {}",
            parsed.message
        );
        assert!(parsed.warnings.is_empty(), "{row}: {:?}", parsed.warnings);
        let reconstructed = parsed.structure.unwrap().semantic_cxsmiles().unwrap();
        assert_eq!(
            crate::naming::canonical_smiles(&reconstructed).unwrap(),
            crate::naming::canonical_smiles(smiles).unwrap(),
            "{row}: {expected} -> {reconstructed}"
        );
    }
}

// Rebuild the same graph through the ordinary Molecule construction helpers,
// changing IDs and bond insertion order instead of reusing a cached SMILES.
fn relabel(source: &Molecule, order: &[usize], reverse_bonds: bool) -> Molecule {
    let ids: BTreeMap<_, _> = order
        .iter()
        .enumerate()
        .map(|(index, &id)| (id, index * 3 + 7))
        .collect();
    let mut target = Molecule::new();
    for atom in source.atoms.values() {
        let mut atom = atom.clone();
        atom.idx = ids[&atom.idx];
        target.add_atom(atom).unwrap();
    }
    let mut bonds: Vec<_> = source.bonds.values().collect();
    if reverse_bonds {
        bonds.reverse();
    }
    let mut bond_ids = BTreeMap::new();
    for (index, bond) in bonds.into_iter().enumerate() {
        let mut bond = bond.clone();
        let old = bond.idx;
        bond.idx = index * 5 + 11;
        bond.u = ids[&bond.u];
        bond.v = ids[&bond.v];
        bond_ids.insert(old, bond.idx);
        target.add_bond(bond).unwrap();
    }
    target.accurate_cip = source
        .accurate_cip
        .iter()
        .map(|(id, cip)| (ids[id], cip.clone()))
        .collect();
    target.legacy_cip = source
        .legacy_cip
        .iter()
        .map(|(id, cip)| (ids[id], cip.clone()))
        .collect();
    target.substituted_symbols = source
        .substituted_symbols
        .iter()
        .map(|id| ids[id])
        .collect();
    target.stereogenic_atoms = source.stereogenic_atoms.iter().map(|id| ids[id]).collect();
    target.stereogenic_bonds = source
        .stereogenic_bonds
        .iter()
        .map(|id| bond_ids[id])
        .collect();
    target.projected_substitution_centers = source
        .projected_substitution_centers
        .iter()
        .map(|id| ids[id])
        .collect();
    target
}

#[test]
fn spirobi_atom_ids_do_not_choose_component_priming() {
    for &(row, smiles, expected, status) in MATRIX.iter().filter(|case| {
        ["D1", "D3", "D4", "D10"].contains(&case.0) || case.0.starts_with("unsaturation-")
    }) {
        let source = graph_io::read_smiles(smiles, &Default::default()).unwrap();
        let ids: Vec<_> = source.atoms.keys().copied().collect();
        let mut reversed = ids.clone();
        reversed.reverse();
        let mut rotated = ids.clone();
        rotated.rotate_left(ids.len() / 2);
        let interleaved: Vec<_> = ids
            .iter()
            .step_by(2)
            .chain(ids.iter().skip(1).step_by(2))
            .copied()
            .collect();
        for (variant, order) in [reversed, rotated, interleaved].iter().enumerate() {
            let molecule = relabel(&source, order, variant % 2 == 0);
            super::validate_parent(&molecule).unwrap();
            let atoms = molecule.atoms.keys().copied().collect();
            let actual =
                component_namer::name_component_with_citation(&molecule, &atoms, None, None)
                    .unwrap_or_else(|error| panic!("{row}, order {variant}: {error}"));
            super::assess_derivation(&molecule, &actual.derivation);
            assert_eq!(
                super::nest_enclosing_marks(&actual.text),
                expected,
                "{row}, order {variant}"
            );
            assert_eq!(molecule.name_status(), status, "{row}, order {variant}");
        }
    }
}

#[test]
fn spirobi_mixed_unsaturation_elides_in_citation_order() {
    let parser = opsin::Parser::new().unwrap();
    for expected in [
        "2,2'-spirobi[bicyclo[5.2.1]decane]-5,5'-dien-3-yne",
        "2,2'-spirobi[bicyclo[5.2.1]decan]-3-en-5,5'-diyne",
    ] {
        let parsed = parser.parse(expected, &opsin::ParseOptions::strict());
        assert_eq!(parsed.status, opsin::Status::Success, "{}", parsed.message);
        let smiles = parsed.structure.unwrap().semantic_cxsmiles().unwrap();
        let source = graph_io::read_smiles(&smiles, &Default::default()).unwrap();
        let ids: Vec<_> = source.atoms.keys().copied().collect();
        for reverse in [false, true] {
            let molecule = relabel(&source, &ids, reverse);
            let atoms = molecule.atoms.keys().copied().collect();
            let actual =
                component_namer::name_component_with_citation(&molecule, &atoms, None, None)
                    .unwrap();
            super::assess_derivation(&molecule, &actual.derivation);
            assert_eq!(actual.text, expected, "reverse bonds: {reverse}");
            assert_eq!(molecule.name_status(), NameStatus::Preferred);
        }
    }
}
