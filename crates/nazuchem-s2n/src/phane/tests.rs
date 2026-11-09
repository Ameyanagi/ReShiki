// SPDX-License-Identifier: MIT AND (MIT OR Apache-2.0)
// Origin: mixed
// Source: OPSIN | opsin-inchi/src/test/resources/uk/ac/cam/ch/wwmm/opsin/phanes.txt | 42e87ac6efb1bd9f578986f5ea7ee1faa8e93c91
// Source: ReShiki | src/naming/openclatura/phane/tests.rs | 5f559fd482e237307a589dfcb7ea333cee0a6e13
// Copyright (c) 2026 lamalab-org
// Source: openclatura | tests/unit/test_analysis.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Copyright 2017 Daniel Lowe
// Copyright (c) 2026 Ameyanagi and ReShiki contributors
// Changes: Original Rust phane test harness with explicit OPSIN phane-oracle fixture selectors and assertions. Adapted openclatura control cases are marked below.
// Local contributions: MIT OR Apache-2.0; see NOTICE and PROVENANCE-nazuchem.toml.

use super::*;
use crate::{graph_io, test_support::verify_identity};
use nazuchem_ir::{NameReason, NameStatus};

const PARA: &str = "C1Cc2ccc(cc2)CCc2ccc1cc2";
const META: &str = "C1Cc2cccc(c2)CCc2cccc1c2";

// Provenance: fn round_trip | original
// Source: ReShiki | src/naming/openclatura/phane/tests.rs::round_trip
fn round_trip(smiles: &str, name: &str) -> Result<(), String> {
    let parsed = nazuchem_n2s::Parser::new()
        .map_err(|e| e.to_string())?
        .parse(name, &nazuchem_n2s::ParseOptions::strict());
    assert_eq!(
        parsed.status,
        nazuchem_n2s::Status::Success,
        "{name}: {}",
        parsed.message
    );
    assert!(parsed.warnings.is_empty(), "{name}: {:?}", parsed.warnings);
    let actual = parsed
        .structure
        .ok_or("Missing phane graph")?
        .semantic_cxsmiles()
        .map_err(|e| e.to_string())?;
    verify_identity(smiles, &actual)
}

#[test]
// Provenance: fn cyclic_phane_names_reconstruct_exactly | original
// Source: ReShiki | src/naming/openclatura/phane/tests.rs::cyclic_phane_names_reconstruct_exactly
fn cyclic_phane_names_reconstruct_exactly() -> Result<(), String> {
    for (smiles, expected) in [
        (PARA, "1,4(1,4)-dibenzenacyclohexaphane"),
        (
            "C1CC2=CC=C(C=C2)CCC2=CC=C1C=C2",
            "1,4(1,4)-dibenzenacyclohexaphane",
        ),
        (META, "1,4(1,3)-dibenzenacyclohexaphane"),
        (
            "C1CCc2cccc(c2)CCc2ccc1cc2",
            "1(1,3),4(1,4)-dibenzenacycloheptaphane",
        ),
        (
            "C1Cc2ncc(cc2)CCc2ccc1cc2",
            "1(2,5)-pyridina-4(1,4)-benzenacyclohexaphane",
        ),
        (
            "C1Cc2ccc(o2)CCc2ccc1cc2",
            "1(2,5)-furana-4(1,4)-benzenacyclohexaphane",
        ),
        (
            "C1Cc2ccc(s2)CCc2ccc1cc2",
            "1(2,5)-thiophena-4(1,4)-benzenacyclohexaphane",
        ),
        (
            "C1OC2=CC=C(C=C2)CCC=2C=CC1=NC2",
            "3-oxa-1(2,5)-pyridina-4(1,4)-benzenacyclohexaphane",
        ),
        (
            "C1Nc2ccc(cc2)CNc2ccc1cc2",
            "2,5-diaza-1,4(1,4)-dibenzenacyclohexaphane",
        ),
    ] {
        assert_eq!(
            name(&graph_io::read_smiles(smiles, &Default::default())?).as_deref(),
            Some(expected),
            "{smiles}"
        );
        round_trip(smiles, expected)?;
        // Input order and aromatic/Kekule spelling cannot change the result.
        let canonical = crate::test_support::canonical_smiles(smiles)?;
        assert_eq!(
            name(&graph_io::read_smiles(&canonical, &Default::default())?).as_deref(),
            Some(expected)
        );
    }
    Ok(())
}

#[test]
// Provenance: fn declines_out_of_scope_graphs | adapted
// Source: ReShiki | src/naming/openclatura/phane/tests.rs::declines_out_of_scope_graphs
// Source: openclatura | tests/unit/test_analysis.py::test_biphenyl_is_graph_selected_not_exact_name_rewritten
// Test provenance: upstream inputs/expected names with local assertions and extensions;
// exact matches are conservatively adapted; independent ancestry is not established.
fn declines_out_of_scope_graphs() -> Result<(), String> {
    for smiles in [
        "CC1Cc2ccc(cc2)CCc2ccc1cc2",
        "C1Cc2cc(C)cc(c2)CCc2cccc1c2",
        "C1=Cc2ccc(cc2)CCc2ccc1cc2",
        "C1CCCCCCCCCCC1",
        "C1CCCCCCc2ccc1cc2",
        "c1ccccc1-c1ccccc1",
        "[13CH2]1Cc2ccc(cc2)CCc2ccc1cc2",
        "C1[NH2+]c2ccc(cc2)CCc2ccc1cc2",
        "C[C@H]1Cc2ccc(cc2)CCc2ccc1cc2",
        "C1Cc2ccc3ccccc3c2CCc2ccc1cc2",
        "C1Cc2ccc(cc2)C2(CC2)Cc2ccc1cc2",
        "C1Cc2ccc(cc2)C2CC(C2)c2ccc1cc2",
        // Fewer than six nodes is not a cyclophane (P-52.2.5.1).
        "C1c2ccccc2Cc2ccccc21",
        "C1c2cccc(c2)Cc2cccc1c2",
        // Adjacent attachment atoms make a fused system (P-52.2.5.2.1).
        "C1Cc2ccccc2CCc2ccccc21",
        "C1Cc2occc2CCc2ccc1cc2",
    ] {
        assert!(
            name(&graph_io::read_smiles(smiles, &Default::default())?).is_none(),
            "{smiles}"
        );
    }
    Ok(())
}

#[test]
// Provenance: fn mixed_attachment_sets_follow_amplificant_seniority | original
// Source: ReShiki | src/naming/openclatura/phane/tests.rs::mixed_attachment_sets_follow_amplificant_seniority
fn mixed_attachment_sets_follow_amplificant_seniority() -> Result<(), String> {
    // Pyridine/benzene/pyridine/pyridine/benzene/pyridine, each separated by CH2.
    // PhI-3.2.2 compares the pyridine attachment sets before the benzene sets.
    let smiles = "C1c2cc(ccn2)Cc2cccc(c2)Cc2ccc(cn2)Cc2cc(ccn2)Cc2cccc(c2)Cc2cc1ccn2";
    let expected = "1(2,4),3,7(4,2),9(5,2)-tetrapyridina-5,11(1,3)-dibenzenacyclododecaphane";
    assert_eq!(
        name(&graph_io::read_smiles(smiles, &Default::default())?).as_deref(),
        Some(expected)
    );
    round_trip(smiles, expected)
}

#[test]
// Provenance: fn failed_ordinary_name_falls_back_to_unverified_phane | original
// Source: ReShiki | src/naming/openclatura/phane/tests.rs::failed_ordinary_name_falls_back_to_unverified_phane
fn failed_ordinary_name_falls_back_to_unverified_phane() -> Result<(), String> {
    let named = super::super::name_smiles(PARA, &Default::default())?;
    assert_eq!(named.text, "1,4(1,4)-dibenzenacyclohexaphane");
    assert_eq!(named.status, NameStatus::Unverified(NameReason::Phane));
    assert!(named.alternatives.is_empty());
    round_trip(PARA, &named.text)
}

#[test]
// Provenance: fn ordinary_names_with_phane_alternatives_are_never_preferred | original
// Source: ReShiki | src/naming/openclatura/phane/tests.rs::ordinary_names_with_phane_alternatives_are_never_preferred
fn ordinary_names_with_phane_alternatives_are_never_preferred() -> Result<(), String> {
    for smiles in [META] {
        let named = super::super::name_smiles(smiles, &Default::default())?;
        assert_eq!(named.status, NameStatus::NotPreferred(NameReason::Phane));
        assert_eq!(named.alternatives.len(), 1);
        assert!(!named.text.contains("phane"));
        round_trip(smiles, &named.text)?;
        for alternative in named.alternatives {
            assert_eq!(
                alternative.status,
                NameStatus::Unverified(NameReason::Phane)
            );
            round_trip(smiles, &alternative.name)?;
        }
    }
    Ok(())
}
