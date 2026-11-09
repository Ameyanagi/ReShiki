// SPDX-License-Identifier: MIT AND (MIT OR Apache-2.0)
// Origin: mixed
// Source: ReShiki | src/naming/openclatura/salt_anion_spelling_tests.rs | 5f559fd482e237307a589dfcb7ea333cee0a6e13
// Copyright (c) 2026 lamalab-org
// Source: openclatura | tests/data/parser_xml_resources/simpleGroups.json | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/unit/test_fusion_wrappers.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/unit/test_analysis.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/unit/test_audit_reconstruction.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Copyright (c) 2026 Ameyanagi and ReShiki contributors

use nazuchem_ir::{NameReason, NameStatus};

#[test]
// Provenance: fn salt_anion_spelling_matrix | adapted
// Source: ReShiki | src/naming/openclatura/salt_anion_spelling_tests.rs::salt_anion_spelling_matrix
// Source: openclatura | tests/data/parser_xml_resources/simpleGroups.json::<fixture data>
// Source: openclatura | tests/unit/test_fusion_wrappers.py::test_bridge_wrapper_composes_with_shared_parent_derivative_operations
// Source: openclatura | tests/unit/test_analysis.py::test_public_api_golden_names
// Source: openclatura | tests/unit/test_audit_reconstruction.py::test_self_audit_confirms_modelled_names
// Test provenance: upstream inputs/expected names with local assertions and extensions;
// exact matches are conservatively adapted; independent ancestry is not established.
fn salt_anion_spelling_matrix() {
    // Retained ion spelling and salt ordering share the public naming pipeline
    // with the P-72.7 parent-choice rules.
    let parser = nazuchem_n2s::Parser::new().unwrap();
    let mut failures = Vec::new();
    for (row, smiles, expected, status) in [
        (
            "B1",
            "[Na+].[O-]c1ccccc1",
            "sodium phenoxide",
            NameStatus::Preferred,
        ),
        (
            "B2",
            "[O-]c1ccc(C)cc1",
            "4-methylphenoxide",
            NameStatus::Preferred,
        ),
        ("B3", "C[O-]", "methoxide", NameStatus::Preferred),
        ("B4", "CC[O-]", "ethoxide", NameStatus::Preferred),
        ("B5", "CCC[O-]", "propoxide", NameStatus::Preferred),
        ("B6", "CCCC[O-]", "butoxide", NameStatus::Preferred),
        ("B7", "CC(C)(C)[O-]", "tert-butoxide", NameStatus::Preferred),
        ("B8", "CC(C)[O-]", "propan-2-olate", NameStatus::Preferred),
        (
            "B9a",
            "ClCC[O-]",
            "2-chloroethan-1-olate",
            NameStatus::Unverified(NameReason::AnionParent),
        ),
        (
            "B9b",
            "CC(C)C[O-]",
            "2-methylpropan-1-olate",
            NameStatus::Unverified(NameReason::AnionParent),
        ),
        (
            "B9c",
            "CC(=O)C[O-]",
            "2-oxopropan-1-olate",
            NameStatus::Unverified(NameReason::AnionParent),
        ),
        (
            "B9d",
            "C[N+](C)(C)CC[O-]",
            "2-(trimethylammonio)ethan-1-olate",
            NameStatus::Unverified(NameReason::AnionParent),
        ),
        (
            "B10",
            "[O-]c1ccccc1[O-]",
            "benzene-1,2-bis(olate)",
            NameStatus::Preferred,
        ),
        (
            "B11",
            "[O-]c1ccc(C(=O)O)cc1",
            "4-carboxyphenoxide",
            NameStatus::Preferred,
        ),
        (
            "B12",
            "[NH3+]CCc1ccccc1[O-]",
            "2-(2-azaniumylethyl)phenoxide",
            NameStatus::Preferred,
        ),
        (
            "B13",
            "[Na+].[Na+].Cc1ccc(cc1)C(=O)[O-].CC(=O)[O-]",
            "disodium acetate 4-methylbenzoate",
            NameStatus::Preferred,
        ),
        (
            "B14",
            "[Na+].[Na+].[Na+].CC(=O)CC[O-].O=C([O-])c1ccc(cc1)Oc1ccc(cc1)C(=O)[O-]",
            "trisodium 4-(4-carboxylatophenoxy)benzoate 3-oxobutan-1-olate",
            NameStatus::NotPreferred(NameReason::Multiplicative),
        ),
        (
            "B17",
            "C[N+](C)(C)c1ccc2c(c1)C1CCC2O1.[Cl-]",
            "N,N,N-trimethyl-1,2,3,4-tetrahydro-1,4-epoxynaphthalen-6-aminium chloride",
            NameStatus::Unverified(NameReason::BridgedFusedParent),
        ),
        (
            "B18",
            "Nc1ccc2c(c1)C1CCC2O1",
            "1,2,3,4-tetrahydro-1,4-epoxynaphthalen-6-amine",
            NameStatus::Unverified(NameReason::BridgedFusedParent),
        ),
        (
            "B19",
            "CN(C)c1ccc2c(c1)C1CCC2O1",
            "N,N-dimethyl-1,2,3,4-tetrahydro-1,4-epoxynaphthalen-6-amine",
            NameStatus::Unverified(NameReason::BridgedFusedParent),
        ),
        (
            "B20",
            "Oc1ccc2c(c1)C1CCC2O1",
            "1,2,3,4-tetrahydro-1,4-epoxynaphthalen-6-ol",
            NameStatus::Unverified(NameReason::BridgedFusedParent),
        ),
        (
            "B21",
            "c1ccc2c(c1)C1CCC2O1",
            "1,2,3,4-tetrahydro-1,4-epoxynaphthalene",
            NameStatus::Unverified(NameReason::BridgedFusedParent),
        ),
        (
            "B22",
            "Cc1ccc2c(c1)C1CCC2O1",
            "6-methyl-1,2,3,4-tetrahydro-1,4-epoxynaphthalene",
            NameStatus::Unverified(NameReason::BridgedFusedParent),
        ),
        (
            "B23",
            "[O-]C(=O)c1ccccc1-c1[n-]cc(C(=O)[O-])c1",
            "5-(2-carboxylatophenyl)-1H-pyrrol-1-ide-3-carboxylate",
            NameStatus::Preferred,
        ),
        (
            "B24",
            "[O-]C(=O)c1ccccc1CC(=O)[O-]",
            "2-(carboxylatomethyl)benzoate",
            NameStatus::Preferred,
        ),
        (
            "B25",
            "O=C([O-])c1ccc(cc1)Oc1ccc(cc1)C(=O)[O-]",
            "4-(4-carboxylatophenoxy)benzoate",
            NameStatus::NotPreferred(NameReason::Multiplicative),
        ),
        (
            "B26",
            "[O-]C(=O)c1ccccc1CC(=O)O",
            "2-(carboxymethyl)benzoate",
            NameStatus::Preferred,
        ),
        (
            "B27",
            "OCCC[O-]",
            "3-hydroxypropan-1-olate",
            NameStatus::Preferred,
        ),
        (
            "B28",
            "[Na+].[O-]CC=C",
            "sodium prop-2-en-1-olate",
            NameStatus::Preferred,
        ),
        (
            "control phenol",
            "Oc1ccccc1",
            "phenol",
            NameStatus::Preferred,
        ),
        (
            "control oxidobenzoate",
            "[O-]c1ccc(cc1)C(=O)[O-]",
            "4-oxidobenzoate",
            NameStatus::Preferred,
        ),
        (
            "control acetate",
            "CC(=O)[O-]",
            "acetate",
            NameStatus::Preferred,
        ),
        (
            "control sodium acetate",
            "[Na+].CC(=O)[O-]",
            "sodium acetate",
            NameStatus::Preferred,
        ),
        (
            "control inorganic salt",
            "[K+].[Na+].[Cl-].[Cl-]",
            "potassium sodium dichloride",
            NameStatus::Unverified(NameReason::UnauditedPath),
        ),
        (
            "control ring dicarboxylate",
            "[Na+].[Na+].[O-]C(=O)c1ccccc1C(=O)[O-]",
            "disodium benzene-1,2-dicarboxylate",
            NameStatus::Preferred,
        ),
        (
            "control propanedioate",
            "[O-]C(=O)CC(=O)[O-]",
            "propanedioate",
            NameStatus::Preferred,
        ),
        (
            "control pyrrolide",
            "O=C([O-])c1cc[n-]c1",
            "1H-pyrrol-1-ide-3-carboxylate",
            NameStatus::Preferred,
        ),
        (
            "control substituted tert-butoxide",
            "ClCC(C)(C)[O-]",
            "1-chloro-2-methylpropan-2-olate",
            NameStatus::Unverified(NameReason::AnionParent),
        ),
        (
            "control secondary butoxide",
            "CCC(C)[O-]",
            "butan-2-olate",
            NameStatus::Preferred,
        ),
        (
            "control partial phenol salt",
            "[O-]c1ccccc1O",
            "2-hydroxybenzen-1-olate",
            NameStatus::Preferred,
        ),
        (
            "control thiolate multiplication",
            "[S-]CC[S-]",
            "ethane-1,2-bis(thiolate)",
            NameStatus::Preferred,
        ),
        (
            "control dihydro bridge",
            "C12CC=C(C3=CC=CC=C13)O2",
            "1,2-dihydro-1,4-epoxynaphthalene",
            NameStatus::Unverified(NameReason::BridgedFusedParent),
        ),
    ] {
        let result = crate::test_support::canonical_smiles(smiles)
            .and_then(|canonical| super::name_smiles(&canonical, &Default::default()));
        let actual = match result {
            Ok(actual) => actual,
            Err(error) => {
                failures.push(format!("{row}: {smiles}: {error}"));
                continue;
            }
        };
        println!("{row}\t{}\t{:?}", actual.text, actual.status);
        if (actual.text.as_str(), actual.status) != (expected, status) {
            failures.push(format!(
                "{row}: {smiles}\n  expected: {expected} ({status:?})\n  actual: {} ({:?})",
                actual.text, actual.status
            ));
        }
        let parsed = parser.parse(&actual.text, &nazuchem_n2s::ParseOptions::strict());
        if parsed.status != nazuchem_n2s::Status::Success {
            failures.push(format!("{row}: OPSIN: {}", parsed.message));
            continue;
        }
        let round_trip = parsed.structure.unwrap().semantic_cxsmiles().unwrap();
        if let Err(error) = crate::test_support::verify_identity(smiles, &round_trip) {
            failures.push(format!("{row}: {} -> {round_trip}: {error}", actual.text));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
