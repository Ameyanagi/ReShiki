use super::name_smiles;
use crate::naming::{NameReason, NameStatus};

#[test]
fn ion_spelling_matrix() {
    // Only unsubstituted cationic N prefixes use azaniumyl/azaniumylidene.
    // Substituted N keeps the accepted ammonio/iminio alternative and CationPrefix.
    let parser = opsin::Parser::new().unwrap();
    let mut failures = Vec::new();
    for (row, smiles, expected, status) in [
        (
            "A1",
            "[NH3+]CC(=O)[O-]",
            "azaniumylacetate",
            NameStatus::Preferred,
        ),
        (
            "A2",
            "C[N+](C)(C)CC(=O)[O-]",
            "(trimethylammonio)acetate",
            NameStatus::Unverified(NameReason::CationPrefix),
        ),
        (
            "A3",
            "[NH3+]CCc1ccccc1C(=O)[O-]",
            "2-(2-azaniumylethyl)benzoate",
            NameStatus::Preferred,
        ),
        (
            "A4",
            "[NH3+]CCc1ccccc1[O-]",
            "2-(2-azaniumylethyl)benzen-1-olate",
            NameStatus::Unverified(NameReason::AnionParent),
        ),
        (
            "A5",
            "[NH3+]c1ccccc1C(=O)[O-]",
            "2-azaniumylbenzoate",
            NameStatus::Preferred,
        ),
        (
            "A6",
            "C[N+](C)(C)CCC(=O)[O-]",
            "3-(trimethylammonio)propanoate",
            NameStatus::Unverified(NameReason::CationPrefix),
        ),
        (
            "A7",
            "C[N+](C)(C)CC[O-]",
            "2-(trimethylammonio)ethan-1-olate",
            NameStatus::Unverified(NameReason::CationPrefix),
        ),
        (
            "A8",
            "[NH3+]CC(=O)NCC(=O)[O-]",
            "[(2-azaniumylacetyl)amino]acetate",
            NameStatus::NotPreferred(NameReason::AminoPrefix),
        ),
        (
            "A9",
            "[NH2+]1CC[NH2+]CC1",
            "piperazine-1,4-diium",
            NameStatus::Preferred,
        ),
        (
            "A10",
            "[OH+]1CC[OH+]CC1",
            "1,4-dioxane-1,4-diium",
            NameStatus::Preferred,
        ),
        (
            "A11",
            "[CH-]1C=Cc2ccccc21",
            "1H-inden-1-ide",
            NameStatus::Preferred,
        ),
        (
            "A12",
            "C[C@H](O)[C-]1C=Cc2ccccc21",
            "1-[(1S)-1-hydroxyethyl]-1H-inden-1-ide",
            NameStatus::Preferred,
        ),
        (
            "A13",
            "C[C@H](O)[C-]1c2ccccc2-c2ccccc21",
            "9-[(1S)-1-hydroxyethyl]-9H-fluoren-9-ide",
            NameStatus::Preferred,
        ),
        (
            "A14",
            "[CH-]1c2ccccc2-c2ccccc21",
            "9H-fluoren-9-ide",
            NameStatus::Preferred,
        ),
        (
            "A15",
            "C[N+](C)(C)c1ccccc1.[Cl-]",
            "N,N,N-trimethylanilinium chloride",
            NameStatus::Unverified(NameReason::UnauditedPath),
        ),
        (
            "A16",
            "[NH3+]c1ccccc1.[Cl-]",
            "anilinium chloride",
            NameStatus::Unverified(NameReason::UnauditedPath),
        ),
        (
            "A17",
            "[NH3+]c1ccc(Oc2ccccc2)cc1.O=C([O-])c1ccc(Oc2ccccc2)cc1",
            "4-phenoxyanilinium 4-phenoxybenzoate",
            NameStatus::Preferred,
        ),
        ("A18", "[NH3+]c1ccccc1", "anilinium", NameStatus::Preferred),
        (
            "A19",
            "C[NH2+]c1ccccc1",
            "N-methylanilinium",
            NameStatus::Preferred,
        ),
        (
            "A20",
            "C[N+](C)(C)c1ccc(C)cc1",
            "N,N,N,4-tetramethylanilinium",
            NameStatus::Preferred,
        ),
        (
            "A21",
            "C[NH2+]CC(=O)[O-]",
            "(methylammonio)acetate",
            NameStatus::Unverified(NameReason::CationPrefix),
        ),
        (
            "A22",
            "C[NH+](C)CC(=O)[O-]",
            "(dimethylammonio)acetate",
            NameStatus::Unverified(NameReason::CationPrefix),
        ),
        (
            "control methanaminium",
            "C[NH3+]",
            "methanaminium",
            NameStatus::Preferred,
        ),
        (
            "control quaternary aminium",
            "C[N+](C)(C)C",
            "N,N,N-trimethylmethanaminium",
            NameStatus::Preferred,
        ),
        (
            "control cyclohexanaminium",
            "[NH3+]C1CCCCC1",
            "cyclohexanaminium",
            NameStatus::Preferred,
        ),
        (
            "control benzene bis(aminium)",
            "[NH3+]c1ccc([NH3+])cc1",
            "benzene-1,4-bis(aminium)",
            NameStatus::Preferred,
        ),
        (
            "control ethane bis(aminium)",
            "[NH3+]CC[NH3+]",
            "ethane-1,2-bis(aminium)",
            NameStatus::Preferred,
        ),
        (
            "control aniline",
            "Nc1ccccc1",
            "aniline",
            NameStatus::Preferred,
        ),
        (
            "control piperidine cation",
            "C1CC[NH2+]CC1",
            "piperidin-1-ium",
            NameStatus::Preferred,
        ),
        (
            "control morpholine cation",
            "[NH2+]1CCOCC1",
            "morpholin-4-ium",
            NameStatus::Preferred,
        ),
        (
            "control methylpyridine cation",
            "C[n+]1ccccc1",
            "1-methylpyridin-1-ium",
            NameStatus::Preferred,
        ),
        (
            "control indene",
            "C1=Cc2ccccc2C1",
            "1H-indene",
            NameStatus::Preferred,
        ),
        (
            "control fluorene",
            "c1ccc2c(c1)Cc1ccccc1-2",
            "9H-fluorene",
            NameStatus::Preferred,
        ),
        (
            "control pyrrolide",
            "[n-]1cccc1",
            "1H-pyrrol-1-ide",
            NameStatus::Preferred,
        ),
        (
            "control benzenide",
            "[c-]1ccccc1",
            "benzenide",
            NameStatus::Preferred,
        ),
        (
            "control trianion",
            "[O-]C(=O)c1cc(C(=O)[O-])ccc1Cc1cc[n-]c1",
            "4-[(1H-pyrrol-1-id-3-yl)methyl]benzene-1,3-dicarboxylate",
            NameStatus::Preferred,
        ),
        (
            "control substituted iminium prefix",
            "C[N+](C)=CC(=O)[O-]",
            "(dimethyliminio)acetate",
            NameStatus::Unverified(NameReason::CationPrefix),
        ),
        (
            "control phenylammonio prefix",
            "[O-]C(=O)C[NH2+]c1ccccc1",
            "(phenylammonio)acetate",
            NameStatus::Unverified(NameReason::CationPrefix),
        ),
        (
            "control nitrobenzene",
            "[O-][N+](=O)c1ccccc1",
            "nitrobenzene",
            NameStatus::Preferred,
        ),
    ] {
        let result = (|| {
            let canonical = crate::naming::canonical_smiles(smiles)?;
            let actual = name_smiles(&canonical)?;
            println!("{row}\t{}\t{:?}", actual.text, actual.status);
            if (actual.text.as_str(), actual.status) != (expected, status) {
                return Err(format!(
                    "expected {expected} ({status:?}), got {} ({:?})",
                    actual.text, actual.status
                ));
            }
            let parsed = parser.parse(&actual.text, &opsin::ParseOptions::strict());
            if parsed.status != opsin::Status::Success {
                return Err(format!("OPSIN: {}", parsed.message));
            }
            let reconstructed = parsed
                .structure
                .unwrap()
                .semantic_cxsmiles()
                .map_err(|e| e.to_string())?;
            crate::naming::verify_identity(smiles, &reconstructed)
        })();
        if let Err(error) = result {
            failures.push(format!("{row}: {smiles}: {error}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
