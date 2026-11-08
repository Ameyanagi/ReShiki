use crate::naming::{NameReason, NameStatus};

#[test]
fn anion_parent_choice_matrix() {
    // P-72.7 chooses the parent before neutral groups become prefixes (P-41),
    // and P-72.5.3 assigns the skeletal anion the lowest locant. Each row checks
    // the public canonical input boundary and the complete name's identity.
    let parser = opsin::Parser::with_budget(crate::naming::kernel_budget()).unwrap();
    let mut failures = Vec::new();
    for (row, smiles, expected, status) in [
        (
            "C1",
            "[O-]C(=O)c1ccccc1Cc1cc[n-]c1",
            "3-[(2-carboxylatophenyl)methyl]-1H-pyrrol-1-ide",
            NameStatus::Preferred,
        ),
        (
            "C2",
            "[O-]C(=O)c1ccccc1[CH2-]",
            "(2-carboxylatophenyl)methanide",
            NameStatus::Preferred,
        ),
        (
            "C3",
            "[O-]c1ccccc1Cc1cc[n-]c1",
            "3-[(2-oxidophenyl)methyl]-1H-pyrrol-1-ide",
            NameStatus::Preferred,
        ),
        (
            "C4",
            "[Na+].[Na+].[O-]c1ccccc1Cc1cc[n-]c1",
            "disodium 3-[(2-oxidophenyl)methyl]-1H-pyrrol-1-ide",
            NameStatus::Preferred,
        ),
        (
            "C5",
            "[O-]C(=O)c1ccccc1[C-](C#N)C#N",
            "(2-carboxylatophenyl)dicyanomethanide",
            NameStatus::Preferred,
        ),
        (
            "C6",
            "[O-]C(=O)c1ccccc1CC(=O)[O-]",
            "2-(carboxylatomethyl)benzoate",
            NameStatus::Preferred,
        ),
        (
            "C7",
            "O=C(O)c1cc[n-]c1",
            "3-carboxy-1H-pyrrol-1-ide",
            NameStatus::Preferred,
        ),
        (
            "C8",
            "[Na+].O=C(O)c1cc[n-]c1",
            "sodium 3-carboxy-1H-pyrrol-1-ide",
            NameStatus::Preferred,
        ),
        (
            "C9",
            "[Na+].OC(=O)c1nc[n-]c1",
            "sodium 4-carboxy-1H-imidazol-1-ide",
            NameStatus::Preferred,
        ),
        (
            "C10",
            "[Na+].c1ccc2[n-]cc(C(=O)O)c2c1",
            "sodium 3-carboxy-1H-indol-1-ide",
            NameStatus::Preferred,
        ),
        (
            "C11",
            "[Na+].[n-]1nnnc1C(=O)O",
            "sodium 5-carboxy-1H-tetrazol-1-ide",
            NameStatus::Preferred,
        ),
        (
            "C12",
            "[Na+].[n-]1nnnc1N",
            "sodium 5-amino-1H-tetrazol-1-ide",
            NameStatus::Preferred,
        ),
        (
            "C13",
            "[K+].Nc1nc[n-]c1",
            "potassium 4-amino-1H-imidazol-1-ide",
            NameStatus::Preferred,
        ),
        (
            "C14",
            "N#C[CH-]C#N",
            "dicyanomethanide",
            NameStatus::Preferred,
        ),
        (
            "C15",
            "[N-]1C(=O)NC(=O)C1",
            "2,4-dioxoimidazolidin-1-ide",
            NameStatus::Preferred,
        ),
        (
            "C16",
            "[NH3+]CC1[N-]C(=O)NC1=O",
            "5-(azaniumylmethyl)-2,4-dioxoimidazolidin-1-ide",
            NameStatus::Preferred,
        ),
        (
            "C17",
            "OC1=CC(=O)C=C[CH-]1",
            "2-hydroxy-4-oxocyclohexa-2,5-dien-1-ide",
            NameStatus::Preferred,
        ),
        (
            "C18",
            "CC(C)[CH-]C(=O)C(C)C",
            "2,5-dimethyl-4-oxohexan-3-ide",
            NameStatus::Preferred,
        ),
        (
            "C19",
            "O=C1C=CC=C[CH-]1",
            "6-oxocyclohexa-2,4-dien-1-ide",
            NameStatus::Preferred,
        ),
        // The skeletal anion gets locant 1; the order of skeletal -ide and
        // characteristic -aminium remains unsettled (P-70.3.3.2.2/P-74.1.2).
        (
            "C20",
            "[NH3+]CC[CH-]C#N",
            "1-cyanopropan-1-id-3-aminium",
            NameStatus::Unverified(NameReason::IonicSuffixOrder),
        ),
        (
            "C21",
            "N#CC1CCC[CH-]1",
            "2-cyanocyclopentan-1-ide",
            NameStatus::Preferred,
        ),
        (
            "C22",
            "CC[CH-]C=O",
            "1-oxobutan-2-ide",
            NameStatus::Preferred,
        ),
        (
            "C23",
            "[N-]1CCC(O)CC1",
            "4-hydroxypiperidin-1-ide",
            NameStatus::Preferred,
        ),
        (
            "C24",
            "N#CCC[CH-]CC#N",
            "1,4-dicyanobutan-2-ide",
            NameStatus::Preferred,
        ),
        (
            "C25",
            "[C-]#CCCC(=O)[O-]",
            "pent-1-yn-1-id-5-oate",
            NameStatus::Preferred,
        ),
        (
            "C26",
            "[CH-]1CCC(CC1)S(=O)(=O)[O-]",
            "cyclohexan-1-ide-4-sulfonate",
            NameStatus::Preferred,
        ),
        (
            "retained pyrrolide numbering",
            "CCc1c[c-]c[nH]1",
            "5-ethyl-1H-pyrrol-3-ide",
            NameStatus::Preferred,
        ),
        (
            "retained zwitterionic pyrrolide numbering",
            "[NH3+]CCc1c[c-]c[nH]1",
            "5-(2-azaniumylethyl)-1H-pyrrol-3-ide",
            NameStatus::Preferred,
        ),
        (
            "sole suffix on ethane",
            "C[CH2-]",
            "ethanide",
            NameStatus::Preferred,
        ),
        (
            "sole suffix on ethene",
            "C=[CH-]",
            "ethenide",
            NameStatus::Preferred,
        ),
        (
            "sole suffix on ethyne",
            "C#[C-]",
            "ethynide",
            NameStatus::Preferred,
        ),
        (
            "sole suffix on cyclopentane",
            "[CH-]1CCCC1",
            "cyclopentanide",
            NameStatus::Preferred,
        ),
        (
            "sole suffix on cyclohexane",
            "[CH-]1CCCCC1",
            "cyclohexanide",
            NameStatus::Preferred,
        ),
        (
            "control prefix requires suffix locant",
            "C[C-]=O",
            "1-oxoethan-1-ide",
            NameStatus::Preferred,
        ),
        (
            "control diene requires suffix locant",
            "[CH-]1C=CC=C1",
            "cyclopenta-2,4-dien-1-ide",
            NameStatus::Preferred,
        ),
        (
            "control substituted benzene suffix locant",
            "Cc1cc[c-]cc1",
            "4-methylbenzen-1-ide",
            NameStatus::Preferred,
        ),
        (
            "control propane suffix locant",
            "C[CH-]C",
            "propan-2-ide",
            NameStatus::Preferred,
        ),
        (
            "control butane suffix locant",
            "CC[CH-]C",
            "butan-2-ide",
            NameStatus::Preferred,
        ),
        (
            "control pyrrolide-carboxylate",
            "O=C([O-])c1cc[n-]c1",
            "1H-pyrrol-1-ide-3-carboxylate",
            NameStatus::Preferred,
        ),
        (
            "control hydroxyphenylmethanide",
            "Oc1ccccc1[CH2-]",
            "(2-hydroxyphenyl)methanide",
            NameStatus::Preferred,
        ),
        (
            "control characteristic suffix seniority",
            "[O-]c1ccccc1Cc1ccccc1C(=O)[O-]",
            "2-[(2-oxidophenyl)methyl]benzoate",
            NameStatus::Preferred,
        ),
        (
            "control carboxyphenylpyrrolide",
            "OC(=O)c1ccccc1-c1[n-]ccc1",
            "2-(2-carboxyphenyl)-1H-pyrrol-1-ide",
            NameStatus::Preferred,
        ),
        (
            "control retained parent numbering",
            "[Na+].Oc1ccccc1-c1[n-]ccc1C",
            "sodium 2-(2-hydroxyphenyl)-3-methyl-1H-pyrrol-1-ide",
            NameStatus::Preferred,
        ),
        (
            "control two cited characteristic anions",
            "[O-]C(=O)c1cc(C(=O)[O-])ccc1Cc1cc[n-]c1",
            "4-[(1H-pyrrol-1-id-3-yl)methyl]benzene-1,3-dicarboxylate",
            NameStatus::Preferred,
        ),
        (
            "substituent free valence precedes retained skeletal anion",
            "[O-]C(=O)c1cc(C(=O)[O-])ccc1Cc1c[c-]c[nH]1",
            "4-[(1H-pyrrol-4-id-2-yl)methyl]benzene-1,3-dicarboxylate",
            NameStatus::Preferred,
        ),
        (
            "control two cited anions (B23)",
            "[O-]C(=O)c1ccccc1-c1[n-]cc(C(=O)[O-])c1",
            "5-(2-carboxylatophenyl)-1H-pyrrol-1-ide-3-carboxylate",
            NameStatus::Preferred,
        ),
        // Added hydrogen remains outside this change: this compatibility name
        // is not the PIN (2-oxopyridin-1(2H)-ide) and stays unverified.
        (
            "control pyridone anion",
            "O=C1C=CC=C[N-]1",
            "pyridin-1-id-2(1H)-one",
            NameStatus::Unverified(NameReason::AnionParent),
        ),
    ] {
        let result = crate::naming::canonical_smiles(smiles)
            .and_then(|canonical| super::name_smiles(&canonical, &crate::naming::kernel_budget()));
        let actual = match result {
            Ok(actual) => actual,
            Err(error) => {
                failures.push(format!("{row}: {smiles}: {error}"));
                continue;
            }
        };
        if (actual.text.as_str(), actual.status) != (expected, status) {
            failures.push(format!(
                "{row}: {smiles}\n  expected: {expected} ({status:?})\n  actual: {} ({:?})",
                actual.text, actual.status
            ));
        }
        let parsed = parser.parse(&actual.text, &opsin::ParseOptions::strict());
        if parsed.status != opsin::Status::Success {
            failures.push(format!("{row}: OPSIN: {}", parsed.message));
            continue;
        }
        let round_trip = match parsed.structure.unwrap().semantic_cxsmiles() {
            Ok(round_trip) => round_trip,
            Err(error) => {
                failures.push(format!("{row}: OPSIN structure: {error}"));
                continue;
            }
        };
        if let Err(error) = crate::naming::verify_identity(smiles, &round_trip) {
            failures.push(format!("{row}: {} -> {round_trip}: {error}", actual.text));
        }
    }
    // Nonroot chain carbanions remain unsupported: decline before assembly
    // can miscite the charged atom as the substituent's free valence.
    let smiles = "[O-]C(=O)c1cc(C(=O)[O-])ccc1CC[CH2-]";
    let result = crate::naming::canonical_smiles(smiles)
        .and_then(|canonical| super::name_smiles(&canonical, &crate::naming::kernel_budget()));
    match result {
        Err(error)
            if error.starts_with("openclatura: anionic C parent atom ")
                && error.ends_with(" has no supported charge citation") => {}
        result => failures.push(format!("nonroot chain carbanion: {smiles}: {result:?}")),
    }
    // Several skeletal anions with a skeletal cation remain outside the
    // supported parent-anion scope after the P-74.1.1 spelling correction.
    let smiles = "[N-]1C[NH2+]C[N-]C1";
    if let Ok(name) = super::name_smiles(smiles, &crate::naming::kernel_budget()) {
        failures.push(format!("mixed-charge ring: {smiles}: {}", name.text));
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
