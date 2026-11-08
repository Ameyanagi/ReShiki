// SPDX-License-Identifier: MIT OR Apache-2.0
// Origin: original nazuchem/ReShiki code.
//   ReShiki, src/naming/openclatura/anion_spelling_tests.rs @ 5f559fd482e237307a589dfcb7ea333cee0a6e13.
// Copyright (c) 2026 Ameyanagi and ReShiki contributors

use super::name_smiles;
use crate::naming::{NameReason, NameStatus};

fn name(smiles: &str) -> Result<super::Name, String> {
    // Match the CLI's canonical input boundary without changing numbering.
    name_smiles(
        &super::super::canonical_smiles(smiles)?,
        &Default::default(),
    )
}

fn assert_name_and_round_trip(
    parser: &opsin::Parser,
    smiles: &str,
    expected: &str,
    status: NameStatus,
) {
    let actual = name(smiles).unwrap();
    assert_eq!(
        (actual.text.as_str(), actual.status),
        (expected, status),
        "{smiles}"
    );
    let parsed = parser.parse(&actual.text, &opsin::ParseOptions::strict());
    assert_eq!(
        parsed.status,
        opsin::Status::Success,
        "{smiles}: {}: {}",
        actual.text,
        parsed.message
    );
    let round_trip = parsed.structure.unwrap().semantic_cxsmiles().unwrap();
    crate::naming::verify_identity(smiles, &round_trip).unwrap();
}

#[test]
fn anion_spelling_matrix() {
    // Check ionic spelling through the public pipeline, including the parent,
    // suffix and numbering corrections from P-72.7, P-41 and P-72.5.3.
    let mut failures = Vec::new();
    for (row, smiles, expected) in [
        (
            1,
            "[O-]C(=O)c1cc(C(=O)[O-])ccc1Cc1cc[n-]c1",
            "4-[(1H-pyrrol-1-id-3-yl)methyl]benzene-1,3-dicarboxylate",
        ),
        (
            2,
            "[n-]1ccc(c1)-c1cc[n-]c1",
            "3-(1H-pyrrol-1-id-3-yl)-1H-pyrrol-1-ide",
        ),
        (
            3,
            "[O-]C(=O)c1ccccc1Cc1cc[n-]c1",
            "3-[(2-carboxylatophenyl)methyl]-1H-pyrrol-1-ide",
        ),
        (
            4,
            "N#C[C-](C#N)c1ccccc1C(=O)[O-]",
            "(2-carboxylatophenyl)dicyanomethanide",
        ),
        (
            5,
            "O[C@H](C1=CC(F)=C[N-]1)[C-]1C=CC=N1",
            "4-fluoro-2-[(S)-(hydroxy)(2H-pyrrol-2-id-2-yl)methyl]-1H-pyrrol-1-ide",
        ),
        (
            6,
            "[NH3+]C1C(C#N)[N-]CC1",
            "2-cyanopyrrolidin-1-id-3-aminium",
        ),
        (7, "[NH3+]CC[CH-]C#N", "1-cyanopropan-1-id-3-aminium"),
        (
            8,
            "[NH3+][C-]1C=CC2=C1N=NO2",
            "cyclopenta[d][1,2,3]oxadiazol-4-id-4-aminium",
        ),
        (9, "CC[CH-]C=O", "1-oxobutan-2-ide"),
        (10, "N#CC1CCC[CH-]1", "2-cyanocyclopentan-1-ide"),
        (
            11,
            "OC1=CC(=O)C=C[CH-]1",
            "2-hydroxy-4-oxocyclohexa-2,5-dien-1-ide",
        ),
        (
            12,
            "[n-]1cccc1Cc1ccc[nH]1",
            "2-[(1H-pyrrol-2-yl)methyl]-1H-pyrrol-1-ide",
        ),
        (
            13,
            "[N-]1CCC(CC1)N1CCCCC1",
            "4-(piperidin-1-yl)piperidin-1-ide",
        ),
        (14, "[n-]1cccc1", "1H-pyrrol-1-ide"),
        (15, "[n-]1nnnc1", "1H-tetrazol-1-ide"),
        (16, "[n-]1cnnc1", "4H-1,2,4-triazol-4-ide"),
        (17, "c1ccc2c(c1)[n-]c1ccccc12", "9H-carbazol-9-ide"),
        (18, "[N-]1CCOCC1", "morpholin-4-ide"),
        (19, "[N-]1C(=O)NC(=O)C1", "2,4-dioxoimidazolidin-1-ide"),
        (
            20,
            "[n-]1c(=O)[nH]c(=O)cc1",
            "pyrimidin-1-ide-2,4(1H,3H)-dione",
        ),
        (21, "N#C[CH-]C#N", "dicyanomethanide"),
        (22, "C#C[CH-]C#N", "1-cyanoprop-2-yn-1-ide"),
        (23, "CC(=O)[CH-]C(C)=O", "2,4-dioxopentan-3-ide"),
        (24, "[O-]C(=O)C1CCC[N-]1", "pyrrolidin-1-ide-2-carboxylate"),
        (25, "[N-]1C(=O)CCC1", "2-oxopyrrolidin-1-ide"),
        (26, "Oc1ccccc1[CH2-]", "(2-hydroxyphenyl)methanide"),
        (
            27,
            "[O-]C(=O)c1ccccc1[CH2-]",
            "(2-carboxylatophenyl)methanide",
        ),
        (28, "[N-]1[NH+]=CC=C1", "1H-pyrazol-2-ium-1-ide"),
        (
            29,
            "[N-]1[NH+]=CC=C2N=CC=C12",
            "1H-pyrrolo[3,2-c]pyridazin-2-ium-1-ide",
        ),
        (30, "C[S+](C)[CH-]C", "1-(dimethylsulfaniumyl)ethan-1-ide"),
        (31, "[NH2+]1CC[NH2+]CC1", "piperazine-1,4-diium"),
        (33, "CC(C)[CH-]C(=O)C(C)C", "2,5-dimethyl-4-oxohexan-3-ide"),
        (34, "[NH2+]1CC(=O)[CH-]CC1", "3-oxopiperidin-1-ium-4-ide"),
        (35, "[NH2+]1CC[CH-]CC1C=O", "2-formylpiperidin-1-ium-4-ide"),
        (36, "N#CCC[CH-]CC#N", "1,4-dicyanobutan-2-ide"),
        (
            37,
            "[N-]1CCC(CC1)C1CC[NH2+]CC1",
            "4-(piperidin-1-ium-4-yl)piperidin-1-ide",
        ),
        (38, "[c-]1ccccc1", "benzenide"),
    ] {
        match name(smiles) {
            Ok(actual) if actual.text == expected => {}
            Ok(actual) => failures.push(format!(
                "design row {row}: {smiles}\n  expected: {expected}\n  actual: {}",
                actual.text
            )),
            Err(error) => failures.push(format!("design row {row}: {smiles}: {error}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn zwitterion_suffix_order_preserves_numbering_status_and_identity() {
    // P-74.1.1: cite cations first while anionic centres retain low locants
    // and the neutral parent's indicated hydrogen remains explicit.
    let parser = opsin::Parser::new().unwrap();
    for (smiles, expected, status) in [
        (
            "[N-]1C[NH2+]CC1",
            "imidazolidin-3-ium-1-ide",
            NameStatus::Preferred,
        ),
        (
            "[N-]1[NH+]=CC=C1",
            "1H-pyrazol-2-ium-1-ide",
            NameStatus::Preferred,
        ),
        (
            "[N-]1CC[NH2+]CC1",
            "piperazin-4-ium-1-ide",
            NameStatus::Preferred,
        ),
        (
            "C[N+]1=CC=C[N-]1",
            "2-methyl-1H-pyrazol-2-ium-1-ide",
            NameStatus::Preferred,
        ),
        (
            "[N-]1C=C[NH+]=C1",
            "1H-imidazol-3-ium-1-ide",
            NameStatus::Preferred,
        ),
        (
            "[N-]1[NH+]=CC=C2N=CC=C12",
            "1H-pyrrolo[3,2-c]pyridazin-2-ium-1-ide",
            NameStatus::Preferred,
        ),
        (
            "C1C[N-]C[NH2+]1",
            "imidazolidin-3-ium-1-ide",
            NameStatus::Preferred,
        ),
        (
            "C1=C[N-][NH+]=C1",
            "1H-pyrazol-2-ium-1-ide",
            NameStatus::Preferred,
        ),
        (
            "[NH+]1=CC=C2N=CC=C2[N-]1",
            "1H-pyrrolo[3,2-c]pyridazin-2-ium-1-ide",
            NameStatus::Preferred,
        ),
        (
            "C1C=C2N=CC=C2[N-][NH+]=1",
            "1H-pyrrolo[3,2-c]pyridazin-2-ium-1-ide",
            NameStatus::Preferred,
        ),
        (
            "[NH2+]1CC(=O)[CH-]CC1",
            "3-oxopiperidin-1-ium-4-ide",
            NameStatus::Preferred,
        ),
        (
            "[CH-]1CC[NH2+]CC1",
            "piperidin-1-ium-4-ide",
            NameStatus::Preferred,
        ),
        (
            "[O-]C(=O)C1CCC[N-]1",
            "pyrrolidin-1-ide-2-carboxylate",
            NameStatus::Preferred,
        ),
        (
            "[O-]C(=O)C[NH3+]",
            "azaniumylacetate",
            NameStatus::Preferred,
        ),
        (
            "[N-]1C(=O)NC(=O)C1",
            "2,4-dioxoimidazolidin-1-ide",
            NameStatus::Preferred,
        ),
        (
            "C[N+](C)(C)[N-]C",
            "N-(trimethylammonio)methanaminide",
            NameStatus::Unverified(NameReason::CationPrefix),
        ),
        (
            "[Na+].[Cl-]",
            "sodium chloride",
            NameStatus::Unverified(NameReason::UnauditedPath),
        ),
        (
            "[N-]1CC[N-]CC1",
            "piperazine-1,4-diide",
            NameStatus::Preferred,
        ),
        (
            "[NH2+]1CC[NH2+]CC1",
            "piperazine-1,4-diium",
            NameStatus::Preferred,
        ),
        (
            "[NH4+]",
            "ammonium",
            NameStatus::Unverified(NameReason::UnauditedPath),
        ),
        ("CC(=O)[O-]", "acetate", NameStatus::Preferred),
    ] {
        assert_name_and_round_trip(&parser, smiles, expected, status);
    }
    // Exercise traversal order inside the namer as well as the CLI boundary.
    for (smiles, expected) in [
        ("C1C[N-]C[NH2+]1", "imidazolidin-3-ium-1-ide"),
        ("C1=C[N-][NH+]=C1", "1H-pyrazol-2-ium-1-ide"),
        (
            "[NH+]1=CC=C2N=CC=C2[N-]1",
            "1H-pyrrolo[3,2-c]pyridazin-2-ium-1-ide",
        ),
        (
            "C1C=C2N=CC=C2[N-][NH+]=1",
            "1H-pyrrolo[3,2-c]pyridazin-2-ium-1-ide",
        ),
    ] {
        let actual = name_smiles(smiles, &Default::default()).unwrap();
        assert_eq!(
            (actual.text.as_str(), actual.status),
            (expected, NameStatus::Preferred),
            "{smiles}"
        );
    }
}

#[test]
fn anion_spelling_supports_charge_citations_on_chains_and_rings() {
    // The low-locant and neutral-suffix rules admit these former row-32
    // refusals without changing their molecular identity.
    let parser = opsin::Parser::new().unwrap();
    for (smiles, expected) in [
        ("[cH-]1cccc1", "cyclopenta-2,4-dien-1-ide"),
        ("O=C1CCC[CH-]1", "2-oxocyclopentan-1-ide"),
        ("C[CH2-]", "ethanide"),
        ("CC[CH-]C", "butan-2-ide"),
        ("O=CC1CCC[CH-]1", "2-formylcyclopentan-1-ide"),
        ("[CH2-]C=CC=O", "4-oxobut-2-en-1-ide"),
        ("[N-]1CC[N-]CC1", "piperazine-1,4-diide"),
    ] {
        assert_name_and_round_trip(&parser, smiles, expected, NameStatus::Preferred);
    }
}

#[test]
fn retained_anion_parents_compose_supported_charge_sites() {
    for smiles in [
        // Needs added hydrogen, `3,6-dihydropyridin-1(2H)-ide` (P-72.2.2.1.1).
        "[N-]1CC=CCC1",
        // Two cations: the old route cited a single `-N-ium` only.
        "[N-]1[NH+]=C[N-][NH+]=C1",
    ] {
        assert!(name(smiles).is_err(), "{smiles}");
    }
    let parser = opsin::Parser::new().unwrap();
    for (smiles, expected, status) in [
        (
            "[c-]1ccccc1C",
            "2-methylbenzen-1-ide",
            NameStatus::Preferred,
        ),
        (
            "[c-]1ccc(F)cc1",
            "4-fluorobenzen-1-ide",
            NameStatus::Preferred,
        ),
        (
            "[N-]1CCN(C)CC1",
            "4-methylpiperazin-1-ide",
            NameStatus::Preferred,
        ),
        (
            "[c-]1ccccc1O",
            "2-hydroxybenzen-1-ide",
            NameStatus::Preferred,
        ),
        (
            "Cc1ccc[n-]1",
            "2-methyl-1H-pyrrol-1-ide",
            NameStatus::Preferred,
        ),
        (
            "[N-]1CCC(C)CC1",
            "4-methylpiperidin-1-ide",
            NameStatus::Preferred,
        ),
        (
            "Oc1ccccc1C1C[N-]CCO1",
            "2-(2-hydroxyphenyl)morpholin-4-ide",
            NameStatus::Preferred,
        ),
        (
            "CC1CC[N-]C(=O)C1",
            "4-methyl-2-oxopiperidin-1-ide",
            NameStatus::Preferred,
        ),
        // A hydron does not change the ring-assembly parent hydride (P-28).
        (
            "[O-]C(=O)c1ccccc1-c1cccc[c-]1",
            "2-(2-carboxylatophenyl)benzen-1-ide",
            NameStatus::NotPreferred(NameReason::RingAssembly),
        ),
        (
            "[c-]1ccccc1-c1ccccc1",
            "2-phenylbenzen-1-ide",
            NameStatus::NotPreferred(NameReason::RingAssembly),
        ),
        (
            "[n-]1cccc1-c1ccc[nH]1",
            "2-(1H-pyrrol-2-yl)-1H-pyrrol-1-ide",
            NameStatus::NotPreferred(NameReason::RingAssembly),
        ),
        (
            "[N-]1CCC(CC1)C1CC[NH2+]CC1",
            "4-(piperidin-1-ium-4-yl)piperidin-1-ide",
            NameStatus::NotPreferred(NameReason::RingAssembly),
        ),
    ] {
        assert_name_and_round_trip(&parser, smiles, expected, status);
    }
}

#[test]
fn ketone_anions_needing_added_hydrogen_are_not_claimed_preferred() {
    // Oxo on a mancude ring anion needs added hydrogen (`pyridin-1(2H)-ide`,
    // P-72.2.2.1.1, P-58.2.2), which is not composed yet.
    for smiles in [
        "[N-]1CC=CC(=O)C1",
        "O=C1CC[N-]C=C1",
        "O=C1CCC=C[N-]1",
        "O=C1[N-]C=CC=C1C",
    ] {
        assert!(name(smiles).is_err(), "{smiles}");
    }
    for (smiles, expected) in [
        ("O=C1C=CC=C[N-]1", "pyridin-1-id-2(1H)-one"),
        ("O=C1C=C[N-]C=C1", "pyridin-1-id-4(1H)-one"),
        ("O=c1cc[n-]cn1", "pyrimidin-1-id-4(1H)-one"),
        ("O=c1ccc2ccccc2[n-]1", "quinolin-1-id-2(1H)-one"),
        ("O=c1cc[n-]c2ccccc12", "quinolin-1-id-4(1H)-one"),
    ] {
        let actual = name(smiles).unwrap();
        assert_eq!(actual.text, expected, "{smiles}");
        assert!(
            matches!(actual.status, NameStatus::Unverified(_)),
            "{smiles}: {:?}",
            actual.status
        );
    }
}
