//! P-24.5 monospiro names from different components (issue #980).
use super::{component_namer, graph_io, molecule::Molecule, name_smiles};
use crate::naming::{NameReason, NameStatus};
use std::collections::BTreeMap;

type Case = (&'static str, &'static str, &'static str, NameStatus);
const UNVERIFIED: NameStatus = NameStatus::Unverified(NameReason::UnauditedParent);

// Design B rows 1–16 and 18; row 17 awaits parser support. Hydrogen belongs to the
// completed spiro parent; neither isolated component's indicated H survives.
const MATRIX: &[Case] = &[
    (
        "B1",
        "O=C1CCCC2(C1)CCc1ccccc12",
        "2',3'-dihydrospiro[cyclohexane-1,1'-inden]-3-one",
        NameStatus::Preferred,
    ),
    (
        "B2",
        "O=C1Cc2ccccc2C12CCCC2",
        "spiro[cyclopentane-1,1'-inden]-2'(3'H)-one",
        NameStatus::Preferred,
    ),
    (
        "B3",
        "O=C1CC2(CCN(C)CC2)Oc2ccccc12",
        "1'-methylspiro[[1]benzopyran-2,4'-piperidin]-4(3H)-one",
        NameStatus::Preferred,
    ),
    (
        "B4",
        "O=C1OC2(CCCC2)c2ccccc12",
        "3H-spiro[[2]benzofuran-1,1'-cyclopentan]-3-one",
        NameStatus::Preferred,
    ),
    (
        "B5",
        "C1OC2(CCCC2)c2ccccc21",
        "3H-spiro[[2]benzofuran-1,1'-cyclopentane]",
        NameStatus::Preferred,
    ),
    (
        "B6",
        "c1ccc2c(c1)COC21CCNCC1",
        "3H-spiro[[2]benzofuran-1,4'-piperidine]",
        NameStatus::Preferred,
    ),
    (
        "B7",
        "C1Oc2ccccc2C12CCCC2",
        "2H-spiro[[1]benzofuran-3,1'-cyclopentane]",
        NameStatus::Preferred,
    ),
    (
        "B8",
        "C1CCC2(CC1)CCc1ccccc12",
        "2',3'-dihydrospiro[cyclohexane-1,1'-indene]",
        NameStatus::Preferred,
    ),
    (
        "B9",
        "O=C1CCC2(CC1)CCc1ccccc12",
        "2',3'-dihydrospiro[cyclohexane-1,1'-inden]-4-one",
        NameStatus::Preferred,
    ),
    (
        "B10",
        "O=C1CCCCC12CCc1ccccc12",
        "2',3'-dihydrospiro[cyclohexane-1,1'-inden]-2-one",
        NameStatus::Preferred,
    ),
    (
        "B11",
        "O=C1Nc2ccccc2C12CCNCC2",
        "spiro[indole-3,4'-piperidin]-2(1H)-one",
        NameStatus::Preferred,
    ),
    (
        "B12",
        "O=C1Nc2ccccc2C12CCCC2",
        "spiro[cyclopentane-1,3'-indol]-2'(1'H)-one",
        NameStatus::Preferred,
    ),
    (
        "B13",
        "C1CC2(C1)Cc1ccccc1C2",
        "1',3'-dihydrospiro[cyclobutane-1,2'-indene]",
        NameStatus::Preferred,
    ),
    (
        "B14",
        "O=C1c2ccccc2C(=O)C12CCCC2",
        "spiro[cyclopentane-1,2'-indene]-1',3'-dione",
        NameStatus::Preferred,
    ),
    (
        "B15",
        "CC1(C)CC2(CCCC2)c2ccccc2O1",
        "2,2-dimethyl-2,3-dihydrospiro[[1]benzopyran-4,1'-cyclopentane]",
        NameStatus::Preferred,
    ),
    (
        "B16",
        "C1CC2(CCN1)c1ccccc1C(=O)N2",
        "spiro[isoindole-1,4'-piperidin]-3(2H)-one",
        NameStatus::Preferred,
    ),
    (
        "B18",
        "C1NCNC12C=NC3CCCCC3N2",
        "4'a,5',6',7',8',8'a-hexahydro-1'H-spiro[imidazolidine-4,2'-quinoxaline]",
        NameStatus::Preferred,
    ),
];

const CONTROLS: &[Case] = &[
    (
        "mancude indene",
        "C1CCC2(CC1)C=Cc1ccccc12",
        "spiro[cyclohexane-1,1'-indene]",
        NameStatus::Preferred,
    ),
    (
        "hydrogenated benzopyran",
        "C1CCC2(CC1)CCc1ccccc1O2",
        "3,4-dihydrospiro[[1]benzopyran-2,1'-cyclohexane]",
        NameStatus::Preferred,
    ),
    (
        "fluorene",
        "C1CC2(CC1)c1ccccc1-c1ccccc21",
        "spiro[cyclopentane-1,9'-fluorene]",
        NameStatus::Preferred,
    ),
    (
        "von Baeyer",
        "C1CCC2(C1)CCCC2",
        "spiro[4.4]nonane",
        NameStatus::Preferred,
    ),
    (
        "von Baeyer adjacent ketone",
        "O=C1CCCC12CCCC2",
        "spiro[4.4]nonan-1-one",
        NameStatus::Preferred,
    ),
    (
        "von Baeyer remote ketone",
        "O=C1CCCC2(C1)CCCCC2",
        "spiro[5.5]undecan-2-one",
        NameStatus::Preferred,
    ),
    (
        "replacement",
        "C1CCC2(CC1)OCCO2",
        "1,4-dioxaspiro[4.5]decane",
        NameStatus::Preferred,
    ),
    (
        "replacement ketone",
        "O=C1CCC2(CC1)OCCO2",
        "1,4-dioxaspiro[4.5]decan-8-one",
        NameStatus::Preferred,
    ),
    (
        "von Baeyer unsaturated ketone",
        "O=C1C=CC2(CCCCC2)C=C1",
        "spiro[5.5]undeca-1,4-dien-3-one",
        NameStatus::Preferred,
    ),
    (
        "corrected benzimidazole",
        "c1ccc2c(c1)NC1(CCC1)N2",
        "1,3-dihydrospiro[[1,3]benzimidazole-2,1'-cyclobutane]",
        NameStatus::Preferred,
    ),
    (
        "mancude spirobi",
        "C1=CC2(C3=CC=CC=C13)C=CC1=CC=CC=C12",
        "1,1'-spirobi[indene]",
        NameStatus::Preferred,
    ),
    (
        "hydrogenated spirobi stays unverified",
        "C1Cc2ccccc2C12CCc1ccccc12",
        "2,2',3,3'-tetrahydro-1,1'-spirobi[indene]",
        UNVERIFIED,
    ),
];

fn assert_names_and_statuses(cases: &[Case]) {
    let mut failures = Vec::new();
    for &(row, smiles, expected, status) in cases {
        match name_smiles(smiles, &Default::default()) {
            Ok(actual) if (actual.text.as_str(), actual.status) == (expected, status) => {}
            Ok(actual) => failures.push(format!(
                "{row}: {smiles}: expected {expected} ({status:?}), got {} ({:?})",
                actual.text, actual.status
            )),
            Err(error) => failures.push(format!("{row}: {smiles}: {error}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn heterospiro_matrix_names_and_statuses() {
    assert_eq!(MATRIX.len(), 17);
    assert_names_and_statuses(MATRIX);
}

#[test]
fn heterospiro_controls_keep_names_and_statuses() {
    // PR #919's benzimidazole control is intentionally corrected. The new
    // heterogeneous composition must not certify hydrogenated spirobi.
    assert_names_and_statuses(CONTROLS);
}

// These neutral, achiral cases need no stereo metadata. Rebuild their graph
// with sparse atom IDs and independently changed bond IDs and insertion order.
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
    for (index, bond) in bonds.into_iter().enumerate() {
        let mut bond = bond.clone();
        bond.idx = index * 5 + 11;
        bond.u = ids[&bond.u];
        bond.v = ids[&bond.v];
        target.add_bond(bond).unwrap();
    }
    target
}

#[test]
fn heterospiro_atom_ids_do_not_choose_junction_hydrogen_or_prime_scope() {
    // Cover junction versus suffix numbering, carbonyl-consumed indicated H,
    // primed N added H, paired carbonyls and fusion-letter primes.
    let cases = MATRIX
        .iter()
        .filter(|case| ["B1", "B4", "B12", "B14", "B18"].contains(&case.0))
        .chain(
            CONTROLS
                .iter()
                .filter(|case| case.0 == "corrected benzimidazole"),
        );
    for &(row, smiles, expected, status) in cases {
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
fn heterospiro_aromatic_and_kekule_inputs_have_the_same_name_and_status() {
    for (row, kekule) in [
        ("B1", "O=C1CCCC2(C1)CCC1=CC=CC=C12"),
        ("B5", "C1OC2(CCCC2)C2=CC=CC=C21"),
        ("B12", "O=C1NC2=CC=CC=C2C12CCCC2"),
        ("B15", "CC1(C)CC2(CCCC2)C2=CC=CC=C2O1"),
        ("corrected benzimidazole", "C1=CC=C2C(=C1)NC1(CCC1)N2"),
    ] {
        let &(_, aromatic, expected, status) = MATRIX
            .iter()
            .chain(CONTROLS)
            .find(|case| case.0 == row)
            .unwrap();
        assert_eq!(
            crate::naming::canonical_smiles(kekule).unwrap(),
            crate::naming::canonical_smiles(aromatic).unwrap(),
            "{row}: equivalent test inputs"
        );
        let actual = name_smiles(kekule, &Default::default())
            .unwrap_or_else(|error| panic!("{row}: {error}"));
        assert_eq!(actual.text, expected, "{row}: {kekule}");
        assert_eq!(actual.status, status, "{row}: {}", actual.text);
    }
}

#[test]
fn heterospiro_fallbacks_require_completed_hydrogen_proof() {
    assert_names_and_statuses(&[
        (
            "unsupported amine keeps fallback numbering",
            "Cc1ccc(F)c2c1C1(CN2)CC1N",
            "7'-fluoro-4'-methylspiro[cyclopropane-2,3'-(2,3-dihydro-1H-indole)]-1-amine",
            NameStatus::Unverified(NameReason::SpiroHydrogen),
        ),
        (
            "prefix-only mancude ketone under an acid suffix",
            "O=C(O)C1CCC2(CC1)C(=O)Cc1ccccc12",
            "2'-oxospiro[cyclohexane-4,1'-(2,3-dihydro-1H-indene)]-1-carboxylic acid",
            NameStatus::Unverified(NameReason::SpiroHydrogen),
        ),
        (
            "B17 awaits outer unsaturation parser support",
            "O1C2(C=CCCC2)CC3=C1C=CCC3",
            "2,3,4,5-tetrahydrospiro[[1]benzofuran-2,1'-(cyclohex-2-ene)]",
            NameStatus::Unverified(NameReason::SpiroHydrogen),
        ),
        (
            "monocycle unsaturation without fused hydrogen operations",
            "O=C1C=CC2(CC1)c1ccccc1-c1ccccc12",
            "spiro[cyclohex-2-ene-4,9'-fluorene]-1-one",
            NameStatus::Unverified(NameReason::SpiroHydrogen),
        ),
        (
            "isotopic component hydrogen stays unverified",
            "C1CCC2(CC1)[13CH2]Cc1ccccc12",
            "(2-13C)spiro[cyclohexane-1,1'-2,3-dihydro-1H-indene]",
            NameStatus::Unverified(NameReason::SpiroHydrogen),
        ),
    ]);
}

#[test]
fn heterospiro_hydrogen_and_suffixes_on_both_components() {
    assert_names_and_statuses(&[
        (
            "hydrogen on different fused components",
            "C1Cc2ccccc2C12COc1ccccc12",
            "2',3'-dihydro-2H-spiro[[1]benzofuran-3,1'-indene]",
            NameStatus::Preferred,
        ),
        (
            "carbonyls across the junction cannot be paired",
            "O=C1Cc2ccccc2C12CCCC2=O",
            "spiro[cyclopentane-1,1'-indene]-2,2'(3'H)-dione",
            NameStatus::Preferred,
        ),
    ]);
}

fn assert_names_statuses_and_structures(cases: &[Case]) {
    assert_names_and_statuses(cases);
    let parser = opsin::Parser::new().unwrap();
    for &(row, smiles, expected, _) in cases {
        let parsed = parser.parse(expected, &opsin::ParseOptions::strict());
        assert_eq!(
            parsed.status,
            opsin::Status::Success,
            "{row}: {}",
            parsed.message
        );
        let reconstructed = parsed.structure.unwrap().semantic_cxsmiles().unwrap();
        crate::naming::verify_identity(smiles, &reconstructed)
            .unwrap_or_else(|error| panic!("{row}: {error}"));
    }
}

#[test]
fn heterospiro_cross_component_hydrogen_allocation_stays_unverified() {
    assert_names_statuses_and_structures(&[(
        "two fused suffix contexts need joint P-58.2.3.1 allocation",
        "O=C1Cc2ccccc2C12CCC(=O)c1ccccc12",
        "2-oxospiro[2,3-dihydro-1H-indene-1,1'-1,2,3,4-tetrahydronaphthalene]-4'-one",
        NameStatus::Unverified(NameReason::SpiroHydrogen),
    )]);
}

#[test]
fn heterospiro_parent_descriptors_keep_fallback_names() {
    assert_names_statuses_and_structures(&[
        (
            "sulfone",
            "O=S1(=O)CCC2(C1)c1ccccc1-c1ccccc12",
            "spiro[1lambda^6-thiolane-3,9'-fluorene]-1,1-dione",
            NameStatus::Unverified(NameReason::SpiroHydrogen),
        ),
        (
            "sulfoxide",
            "O=S1CCC2(C1)c1ccccc1-c1ccccc12",
            "spiro[1lambda^4-thiolane-3,9'-fluorene]-1-one",
            NameStatus::Unverified(NameReason::SpiroHydrogen),
        ),
        (
            "phospholane oxide",
            "O=P1CCC2(C1)c1ccccc1-c1ccccc12",
            "spiro[1lambda^5-phospholane-3,9'-fluorene]-1-one",
            NameStatus::Unverified(NameReason::SpiroHydrogen),
        ),
    ]);
}

#[test]
fn heterospiro_substituents_keep_spelling_and_downgrade_hydrogen() {
    assert_names_statuses_and_structures(&[
        (
            "spiro substituent on furan",
            "c1ccoc1C1CCC2(CC1)CCc1ccccc12",
            "2-[spiro[cyclohexane-4,1'-(2,3-dihydro-1H-indene)]-1-yl]furan",
            NameStatus::Unverified(NameReason::SpiroHydrogen),
        ),
        (
            "spiro substituent on ethanone",
            "CC(=O)C1CCC2(CC1)CCc1ccccc12",
            "1-[spiro[cyclohexane-4,1'-(2,3-dihydro-1H-indene)]-1-yl]ethan-1-one",
            NameStatus::Unverified(NameReason::SpiroHydrogen),
        ),
        (
            "saturated spiro substituent on furan",
            "c1ccoc1C1CCC2(CC1)CCCC2",
            "2-(spiro[4.5]decan-8-yl)furan",
            NameStatus::Preferred,
        ),
        (
            "saturated spiro substituent on ethanone",
            "CC(=O)C1CCC2(CC1)CCCC2",
            "1-(spiro[4.5]decan-8-yl)ethan-1-one",
            NameStatus::Preferred,
        ),
        (
            "mixed alcohol and ketone remains outside composition",
            "OC1CCC2(CC1)CC(=O)c1ccccc12",
            "spiro[cyclohexane-1,1'-1,2-dihydro-3H-indene]-4-ol-3'-one",
            NameStatus::Unverified(NameReason::SpiroHydrogen),
        ),
    ]);
}
