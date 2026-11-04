use super::name_smiles;

fn name(smiles: &str) -> Result<super::Name, String> {
    // Match the CLI's canonical input boundary without changing numbering.
    name_smiles(&super::super::canonical_smiles(smiles)?)
}

#[test]
fn anion_spelling_matrix() {
    // Keep the design's parent, numbering and suffix-order decisions fixed while
    // checking the spelling of each complete name through the public pipeline.
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
            "2-[(1H-pyrrol-1-id-3-yl)methyl]benzoate",
        ),
        (
            4,
            "N#C[C-](C#N)c1ccccc1C(=O)[O-]",
            "2-(1,3-dinitrilopropan-2-id-2-yl)benzoate",
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
        (7, "[NH3+]CC[CH-]C#N", "3-cyanopropan-3-id-1-aminium"),
        (
            8,
            "[NH3+][C-]1C=CC2=C1N=NO2",
            "cyclopenta[d][1,2,3]oxadiazol-4-id-4-aminium",
        ),
        (9, "CC[CH-]C=O", "butan-2-id-1-al"),
        (10, "N#CC1CCC[CH-]1", "cyclopentan-2-ide-1-carbonitrile"),
        (
            11,
            "OC1=CC(=O)C=C[CH-]1",
            "3-hydroxy-1-oxocyclohexa-2,5-dien-4-ide",
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
        (19, "[N-]1C(=O)NC(=O)C1", "imidazolidin-1-ide-2,4-dione"),
        (
            20,
            "[n-]1c(=O)[nH]c(=O)cc1",
            "pyrimidin-1-ide-2,4(1H,3H)-dione",
        ),
        (21, "N#C[CH-]C#N", "propan-2-ide-1,3-dinitrile"),
        (22, "C#C[CH-]C#N", "but-3-yn-2-ide-1-nitrile"),
        (23, "CC(=O)[CH-]C(C)=O", "pentan-3-ide-2,4-dione"),
        (24, "[O-]C(=O)C1CCC[N-]1", "pyrrolidin-1-ide-2-carboxylate"),
        (25, "[N-]1C(=O)CCC1", "2-oxopyrrolidin-1-ide"),
        (26, "Oc1ccccc1[CH2-]", "(2-hydroxyphenyl)methanide"),
        (27, "[O-]C(=O)c1ccccc1[CH2-]", "2-methanidylbenzoate"),
        (28, "[N-]1[NH+]=CC=C1", "1H-pyrazol-1-ide-2-ium"),
        (
            29,
            "[N-]1[NH+]=CC=C2N=CC=C12",
            "pyrrolo[3,2-c]pyridazin-1-ide-2-ium",
        ),
        (30, "C[S+](C)[CH-]C", "1-(dimethylsulfaniumyl)ethan-1-ide"),
        (31, "[NH2+]1CC[NH2+]CC1", "piperazin-1,4-diium"),
        (33, "CC(C)[CH-]C(=O)C(C)C", "2,5-dimethyl-3-oxohexan-4-ide"),
        (34, "[NH2+]1CC(=O)[CH-]CC1", "3-oxopiperidin-1-ium-4-ide"),
        (
            35,
            "[NH2+]1CC[CH-]CC1C=O",
            "piperidin-1-ium-4-ide-2-carbaldehyde",
        ),
        (36, "N#CCC[CH-]CC#N", "hexan-3-ide-1,6-dinitrile"),
        (
            37,
            "[N-]1CCC(CC1)C1CC[NH2+]CC1",
            "4-(piperidinium-4-yl)piperidin-1-ide",
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
fn anion_spelling_keeps_unsupported_charge_citations_unsupported() {
    // Design row 32 freezes the existing coverage boundary.
    let mut failures = Vec::new();
    for smiles in [
        "[cH-]1cccc1",
        "O=C1CCC[CH-]1",
        "CC[CH-]C",
        "O=CC1CCC[CH-]1",
        "[CH2-]C=CC=O",
        "[N-]1CC[N-]CC1",
    ] {
        if let Ok(actual) = name(smiles) {
            failures.push(format!("design row 32: {smiles}: {}", actual.text));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn retained_anion_parents_are_cited_only_where_numbering_fixes_the_centre() {
    use crate::naming::{NameReason, NameStatus};
    // Numbering does not yet give `ide` the lowest locant, and functional
    // parents (phenol) or contracted stems (phenyl) have no anion spelling.
    for smiles in [
        "[c-]1ccccc1C",
        "[c-]1ccc(F)cc1",
        "[N-]1CCN(C)CC1",
        "[c-]1ccccc1O",
        "[O-]C(=O)c1ccccc1-c1cccc[c-]1",
        // Needs added hydrogen, `3,6-dihydropyridin-1(2H)-ide` (P-72.2.2.1.1).
        "[N-]1CC=CCC1",
        // Two cations: the old route cited a single `-N-ium` only.
        "[N-]1[NH+]=C[N-][NH+]=C1",
    ] {
        assert!(name(smiles).is_err(), "{smiles}");
    }
    for (smiles, expected, status) in [
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
            "[n-]1cccc1-c1ccc[nH]1",
            "2-(1H-pyrrol-2-yl)-1H-pyrrol-1-ide",
            NameStatus::NotPreferred(NameReason::RingAssembly),
        ),
        (
            "[N-]1CCC(CC1)C1CC[NH2+]CC1",
            "4-(piperidinium-4-yl)piperidin-1-ide",
            NameStatus::NotPreferred(NameReason::RingAssembly),
        ),
    ] {
        let actual = name(smiles).unwrap();
        assert_eq!(
            (actual.text.as_str(), actual.status),
            (expected, status),
            "{smiles}"
        );
    }
}
