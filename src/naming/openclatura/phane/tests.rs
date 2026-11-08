use super::*;
use crate::naming::{NameReason, NameStatus, openclatura::graph_io, verify_identity};

const PARA: &str = "C1Cc2ccc(cc2)CCc2ccc1cc2";
const META: &str = "C1Cc2cccc(c2)CCc2cccc1c2";

fn round_trip(smiles: &str, name: &str) -> Result<(), String> {
    let parsed = opsin::Parser::with_budget(crate::naming::kernel_budget())
        .map_err(|e| e.to_string())?
        .parse(name, &opsin::ParseOptions::strict());
    assert_eq!(
        parsed.status,
        opsin::Status::Success,
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

fn fixture_smiles(name: &str) -> Result<String, String> {
    let inchi = include_str!("../../../../crates/opsin/tests/fixtures/upstream-inchi/phanes.txt")
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .find(|(fixture, _)| *fixture == name)
        .ok_or("Missing upstream phane fixture")?
        .1;
    crate::naming::kernel_bridge::smiles_from_inchi(inchi)
}

#[test]
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
            name(&graph_io::read_smiles(
                smiles,
                &crate::naming::kernel_budget()
            )?)
            .as_deref(),
            Some(expected),
            "{smiles}"
        );
        round_trip(smiles, expected)?;
        // Input order and aromatic/Kekule spelling cannot change the result.
        let canonical = crate::naming::canonical_smiles(smiles)?;
        assert_eq!(
            name(&graph_io::read_smiles(
                &canonical,
                &crate::naming::kernel_budget()
            )?)
            .as_deref(),
            Some(expected)
        );
    }
    Ok(())
}

#[test]
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
            name(&graph_io::read_smiles(
                smiles,
                &crate::naming::kernel_budget()
            )?)
            .is_none(),
            "{smiles}"
        );
    }
    Ok(())
}

#[test]
fn mixed_attachment_sets_follow_amplificant_seniority() -> Result<(), String> {
    // Pyridine/benzene/pyridine/pyridine/benzene/pyridine, each separated by CH2.
    // PhI-3.2.2 compares the pyridine attachment sets before the benzene sets.
    let smiles = "C1c2cc(ccn2)Cc2cccc(c2)Cc2ccc(cn2)Cc2cc(ccn2)Cc2cccc(c2)Cc2cc1ccn2";
    let expected = "1(2,4),3,7(4,2),9(5,2)-tetrapyridina-5,11(1,3)-dibenzenacyclododecaphane";
    assert_eq!(
        name(&graph_io::read_smiles(
            smiles,
            &crate::naming::kernel_budget()
        )?)
        .as_deref(),
        Some(expected)
    );
    round_trip(smiles, expected)
}

#[test]
fn failed_ordinary_name_falls_back_to_unverified_phane() -> Result<(), String> {
    let named = super::super::name_smiles(PARA, &crate::naming::kernel_budget())?;
    assert_eq!(named.text, "1,4(1,4)-dibenzenacyclohexaphane");
    assert_eq!(named.status, NameStatus::Unverified(NameReason::Phane));
    assert!(named.alternatives.is_empty());
    round_trip(PARA, &named.text)
}

#[test]
fn names_independent_upstream_inchi_graphs() -> Result<(), String> {
    let seven = "1(1,3),4(1,4)-dibenzenacycloheptaphane";
    verify_identity("C1CCc2cccc(c2)CCc2ccc1cc2", &fixture_smiles(seven)?)?;
    for expected in [
        seven,
        "1,3,5,7(1,3)-tetrabenzenacyclooctaphane",
        "5-oxa-2-thia-1,7(1,3)-dibenzenacyclododecaphane",
        "2,5-diaza-1,4(1,4)-dibenzenacyclohexaphane",
    ] {
        let smiles = fixture_smiles(expected)?;
        assert_eq!(
            name(&graph_io::read_smiles(
                &smiles,
                &crate::naming::kernel_budget()
            )?)
            .as_deref(),
            Some(expected)
        );
        round_trip(&smiles, expected)?;
    }
    Ok(())
}

#[test]
fn ordinary_names_with_phane_alternatives_are_never_preferred() -> Result<(), String> {
    for smiles in [META] {
        let named = super::super::name_smiles(smiles, &crate::naming::kernel_budget())?;
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
