use std::collections::BTreeSet;

use opsin::graph::StereoGroupType;
use opsin::{ParseOptions, Parser, Status, Structure};
use reshiki_chemistry::smiles::stereoisomers;

// These independent representatives were checked with RDKit CIP assignments
// and conformers before being used as expectations.
const BICYCLO_A: &str = "O[C@@H]1[C@H](C)[C@H]2CC[C@@H]1C2";
const BICYCLO_B: &str = "O[C@@H]1[C@@H](C)[C@H]2CC[C@@H]1C2";
const BICYCLO_C: &str = "[C@@H]12[C@@H](C[C@@H](CC1)C2)O";
const BICYCLO_D: &str = "C[C@@H]1[C@H]2CC[C@H](C2)[C@@H]1[C@@H](C)CO";
const BICYCLO_E: &str = "C[C@H](F)[C@@H]1[C@H]2CC[C@H](C2)[C@@H]1O";

fn check_relative_mirror(name: &str, actual: &BTreeSet<String>) -> Result<(), String> {
    // Leading rel-/rac- applies to the whole compound:
    // https://iupac.qmul.ac.uk/BlueBook/P9.html#930102
    // https://iupac.qmul.ac.uk/BlueBook/P9.html#930103
    let relative = name.split_once('-').is_some_and(|(prefix, _)| {
        prefix.eq_ignore_ascii_case("rel") || prefix.eq_ignore_ascii_case("rac")
    });
    if !relative {
        return Ok(());
    }
    let first = actual.first().unwrap();
    let mirror_pair = stereoisomers(&format!("{first} |r|")).unwrap();
    if actual != &mirror_pair {
        return Err(format!(
            "leading rel-/rac- requires one mirror pair: actual {actual:?}, mirror pair {mirror_pair:?}"
        ));
    }
    Ok(())
}

/// A mirror-set comparison also requires the expected group kind and every
/// configured centre to belong to that one group.
fn check(name: &str, intended: &str, expected_kind: StereoGroupType) -> Result<Structure, String> {
    let result = Parser::new().unwrap().parse(name, &ParseOptions::strict());
    assert_eq!(result.status, Status::Success, "{name}: {}", result.message);
    let structure = result.structure.unwrap();
    assert_complete_group(name, &structure, intended, expected_kind);
    let actual = structure.semantic_cxsmiles().unwrap();
    let actual_set = stereoisomers(&actual).unwrap();
    let intended_set = stereoisomers(intended).unwrap();
    if actual_set != intended_set {
        return Err(format!(
            "{actual}: actual {actual_set:?}, intended {intended_set:?}"
        ));
    }
    check_relative_mirror(name, &actual_set)?;
    Ok(structure)
}

fn assert_complete_group(
    name: &str,
    structure: &Structure,
    representative: &str,
    expected_kind: StereoGroupType,
) {
    let parities: Vec<_> = structure
        .graph
        .atoms
        .iter()
        .filter(|atom| atom.active)
        .filter_map(|atom| atom.parity.as_ref())
        .collect();
    let expected_count = representative
        .split('|')
        .next()
        .unwrap()
        .split('[')
        .skip(1)
        .filter(|atom| atom.split(']').next().unwrap().contains('@'))
        .count();
    assert_eq!(parities.len(), expected_count, "{name}: configured centres");
    let group = parities.first().unwrap().stereo_group;
    assert_eq!(group.kind, expected_kind, "{name}");
    assert!(
        parities.iter().all(|parity| parity.stereo_group == group),
        "{name}: every configured centre must belong to the same group"
    );
}

#[test]
fn names_match_intended_stereoisomer_sets() {
    let a_pair = format!("{BICYCLO_A} |r|");
    let b_pair = format!("{BICYCLO_B} |r|");
    let c_pair = format!("{BICYCLO_C} |r|");
    let d_pair = format!("{BICYCLO_D} |r|");
    let e_pair = format!("{BICYCLO_E} |r|");

    // Cyclohexane representatives were independently checked with
    // reshiki --cli name --smiles '<plain representative>' --json.
    for (name, intended, expected_kind) in [
        (
            "(1R,2S,3R,4S)-3-methylbicyclo[2.2.1]heptan-2-ol",
            BICYCLO_A,
            StereoGroupType::Absolute,
        ),
        (
            "rel-(1R,2S,3R,4S)-3-methylbicyclo[2.2.1]heptan-2-ol",
            a_pair.as_str(),
            StereoGroupType::Relative,
        ),
        (
            "rac-(1R,2S,3R,4S)-3-methylbicyclo[2.2.1]heptan-2-ol",
            a_pair.as_str(),
            StereoGroupType::Racemic,
        ),
        (
            "rel-(1R,2S,3S,4S)-3-methylbicyclo[2.2.1]heptan-2-ol",
            b_pair.as_str(),
            StereoGroupType::Relative,
        ),
        (
            "rac-(1R,2S,3S,4S)-3-methylbicyclo[2.2.1]heptan-2-ol",
            b_pair.as_str(),
            StereoGroupType::Racemic,
        ),
        (
            "rac-(1R,2R,4S)-bicyclo[2.2.1]heptan-2-ol",
            c_pair.as_str(),
            StereoGroupType::Racemic,
        ),
        (
            "rel-(1R,2S)-2-methylcyclohexan-1-ol",
            // CLI: (1R,2S).
            "O[C@H]1[C@@H](C)CCCC1 |r|",
            StereoGroupType::Relative,
        ),
        (
            "rel-1-[(1R,2S)-2-methylcyclohexyl]ethan-1-one",
            // CLI: ring (1R,2S).
            "CC(=O)[C@H]1[C@@H](C)CCCC1 |r|",
            StereoGroupType::Relative,
        ),
        (
            "(1R*,2S*)-2-methylcyclohexan-1-ol",
            "O[C@H]1[C@@H](C)CCCC1 |r|",
            StereoGroupType::Relative,
        ),
        (
            "(1RS,2SR)-2-methylcyclohexan-1-ol",
            "O[C@H]1[C@@H](C)CCCC1 |r|",
            StereoGroupType::Racemic,
        ),
        // A ring cis/trans descriptor keeps the rel/rac group of the centre
        // an earlier R*/RS descriptor configured. CLI: cis is (1R,2S),
        // trans (1R,2R), cis acid (1S,2R).
        (
            "(1R*)-cis-2-methylcyclohexan-1-ol",
            "O[C@H]1[C@@H](C)CCCC1 |r|",
            StereoGroupType::Relative,
        ),
        (
            "rel-(1R)-cis-2-methylcyclohexan-1-ol",
            "O[C@H]1[C@@H](C)CCCC1 |r|",
            StereoGroupType::Relative,
        ),
        (
            "rac-(1R)-cis-2-methylcyclohexan-1-ol",
            "O[C@H]1[C@@H](C)CCCC1 |r|",
            StereoGroupType::Racemic,
        ),
        (
            "(1RS)-cis-2-methylcyclohexan-1-ol",
            "O[C@H]1[C@@H](C)CCCC1 |r|",
            StereoGroupType::Racemic,
        ),
        (
            "(1R*)-trans-2-methylcyclohexan-1-ol",
            "O[C@H]1[C@H](C)CCCC1 |r|",
            StereoGroupType::Relative,
        ),
        (
            "rac-(1S)-cis-2-phenylcyclopropane-1-carboxylic acid",
            "OC(=O)[C@H]1C[C@H]1c1ccccc1 |r|",
            StereoGroupType::Racemic,
        ),
        (
            "(1R)-cis-2-methylcyclohexan-1-ol",
            "O[C@H]1[C@@H](C)CCCC1",
            StereoGroupType::Absolute,
        ),
        // An agreeing 5alpha keeps C5 in the rel group.
        (
            "rel-(5R,8R,9S,10S,13R,14S,17R,20R)-5alpha-cholestane",
            "CC(C)CCC[C@@H](C)[C@H]1CC[C@H]2[C@@H]3CC[C@H]4CCCC[C@]4(C)[C@H]3CC[C@]12C |r|",
            StereoGroupType::Relative,
        ),
        // The anomer joins the DL group: alpha-D plus alpha-L, not beta-L.
        (
            "alpha-DL-glucopyranose",
            "OC[C@H]1O[C@H](O)[C@H](O)[C@@H](O)[C@@H]1O |r|",
            StereoGroupType::Racemic,
        ),
        (
            "beta-DL-glucopyranose",
            "OC[C@H]1O[C@@H](O)[C@H](O)[C@@H](O)[C@@H]1O |r|",
            StereoGroupType::Racemic,
        ),
        // Leading qualifiers include every nested descriptor set (#934).
        (
            "rel-(2R)-2-[(1R,2S,3R,4S)-3-methylbicyclo[2.2.1]heptan-2-yl]propan-1-ol",
            d_pair.as_str(),
            StereoGroupType::Relative,
        ),
        (
            "rac-(2R)-2-[(1R,2S,3R,4S)-3-methylbicyclo[2.2.1]heptan-2-yl]propan-1-ol",
            d_pair.as_str(),
            StereoGroupType::Racemic,
        ),
        (
            "rel-(1R,2S,3R,4S)-3-[(1S)-1-fluoroethyl]bicyclo[2.2.1]heptan-2-ol",
            e_pair.as_str(),
            StereoGroupType::Relative,
        ),
        (
            "rac-(1R,2S,3R,4S)-3-[(1S)-1-fluoroethyl]bicyclo[2.2.1]heptan-2-ol",
            e_pair.as_str(),
            StereoGroupType::Racemic,
        ),
        (
            "rel-(1R)-1-[(1R,2S)-2-methylcyclohexyl]ethan-1-ol",
            // CLI: side chain (1R), ring (1R,2S).
            "C[C@@H](O)[C@H]1[C@@H](C)CCCC1 |r|",
            StereoGroupType::Relative,
        ),
        (
            "rac-(1R)-1-[(1R,2S)-2-methylcyclohexyl]ethan-1-ol",
            "C[C@@H](O)[C@H]1[C@@H](C)CCCC1 |r|",
            StereoGroupType::Racemic,
        ),
        (
            "rac-(1R,2S)-2-[(1S)-1-fluoroethyl]cyclohexan-1-ol",
            // CLI: ring (1R,2S), fluoroethyl (1S).
            "O[C@H]1[C@@H]([C@@H](F)C)CCCC1 |r|",
            StereoGroupType::Racemic,
        ),
    ] {
        check(name, intended, expected_kind).unwrap_or_else(|error| panic!("{name}: {error}"));
    }
}

#[test]
fn literal_cxsmiles_audit_rejects_split_or_lost_mirror_pairs() {
    let a_pair = format!("{BICYCLO_A} |r|");
    let c_pair = format!("{BICYCLO_C} |r|");
    let d_pair = format!("{BICYCLO_D} |r|");
    let e_pair = format!("{BICYCLO_E} |r|");
    for (name, literal, intended, size) in [
        // #898 round 3: one mirror pair was split into independent groups.
        (
            "rel-(1R,3R)-3-methylbicyclo[2.2.1]heptan-2-exo-ol",
            "C[C@H]1[C@H]([C@@H]2CC[C@H]1C2)O |o1:1,o2:2,3,6|",
            a_pair.as_str(),
            4,
        ),
        // #898 round 1: racemic bridgehead configurations were lost.
        (
            "rac-(2-exo)-bicyclo[2.2.1]heptan-2-ol",
            BICYCLO_C,
            c_pair.as_str(),
            1,
        ),
        (
            "rac-(1R,2-exo)-bicyclo[2.2.1]heptan-2-ol",
            "[C@@H]12[C@@H](C[C@@H](CC1)C2)O |&1:0|",
            c_pair.as_str(),
            2,
        ),
        // #898 rounds 4/5: nested descriptor sets remained absolute.
        (
            "rel-(2R)-2-[(1R,2S,3R,4S)-3-methylbicyclo[2.2.1]heptan-2-yl]propan-1-ol",
            "C[C@H]1[C@H]([C@@H]2CC[C@H]1C2)[C@H](CO)C |o1:8|",
            d_pair.as_str(),
            2,
        ),
        (
            "rel-(1R,2S,3R,4S)-3-[(1S)-1-fluoroethyl]bicyclo[2.2.1]heptan-2-ol",
            "F[C@@H](C)[C@H]1[C@H]([C@@H]2CC[C@H]1C2)O |o1:3,4,5,8|",
            e_pair.as_str(),
            2,
        ),
    ] {
        let actual = stereoisomers(literal).unwrap();
        assert_eq!(actual.len(), size, "{literal}");
        assert_ne!(actual, stereoisomers(intended).unwrap(), "{literal}");
        assert!(check_relative_mirror(name, &actual).is_err(), "{literal}");
    }
}

#[test]
fn leading_qualifiers_include_implied_stereoparent_configuration() {
    // Representatives frozen in the independent noncage stereo audit. Each
    // chiral reference covers both qualifiers; tropanols remain achiral.
    for (unqualified, representative, grouped_atoms, chiral) in [
        (
            "(5alpha)-androstane",
            "C[C@@]12CCC[C@H]1[C@@H]1CC[C@H]3CCCC[C@]3(C)[C@H]1CC2",
            &[1, 5, 6, 9, 14, 16][..],
            true,
        ),
        (
            "(3beta,5alpha)-androstan-3-ol",
            "C[C@@]12CCC[C@H]1[C@@H]1CC[C@H]3C[C@H](CC[C@]3(C)[C@H]1CC2)O",
            &[1, 5, 6, 9, 11, 14, 16][..],
            true,
        ),
        (
            "(3alpha,5beta)-androstan-3-ol",
            "C[C@@]12CCC[C@H]1[C@@H]1CC[C@@H]3C[C@@H](CC[C@]3(C)[C@H]1CC2)O",
            &[1, 5, 6, 9, 11, 14, 16][..],
            true,
        ),
        (
            "(17beta)-androst-4-en-17-ol",
            "C[C@@]12[C@H](CC[C@H]1[C@@H]1CCC3=CCCC[C@]3(C)[C@H]1CC2)O",
            &[1, 2, 5, 6, 14, 16][..],
            true,
        ),
        (
            "(8alpha,9beta,17beta)-17-hydroxyandrost-4-en-3-one",
            "O[C@@H]1[C@]2(C)[C@@H](CC1)[C@H]1CCC3=CC(CC[C@]3(C)[C@@H]1CC2)=O",
            &[1, 2, 4, 7, 15, 17][..],
            true,
        ),
        (
            "(5alpha,23E)-cholest-23-ene",
            "CC(C)\\C=C\\C[C@@H](C)[C@H]1CC[C@H]2[C@@H]3CC[C@H]4CCCC[C@]4(C)[C@H]3CC[C@]12C",
            &[6, 8, 11, 12, 15, 20, 22, 25][..],
            true,
        ),
        (
            "(3beta)-tropan-3-ol",
            "[C@H]12C[C@H](C[C@H](CC1)N2C)O",
            &[0, 2, 4][..],
            false,
        ),
        (
            "(3alpha)-tropan-3-ol",
            "[C@H]12C[C@@H](C[C@H](CC1)N2C)O",
            &[0, 2, 4][..],
            false,
        ),
        (
            "(6beta)-6-aminopenam",
            "N[C@H]1[C@@H]2N(CCS2)C1=O",
            &[1, 2][..],
            true,
        ),
        (
            "(7beta)-7-aminocepham",
            "N[C@H]1[C@@H]2N(CCCS2)C1=O",
            &[1, 2][..],
            true,
        ),
        (
            "(6alpha)-morphinan-6-ol",
            "C1=CC=CC=2[C@@]34C[C@H](CC[C@H]3[C@@H](CC12)NCC4)O",
            &[5, 7, 10, 11][..],
            true,
        ),
        (
            "(8beta)-ergolin-8-ol",
            "[C@@H]1(CN[C@@H]2CC3=CNC4=CC=CC([C@H]2C1)=C34)O",
            &[0, 3, 13][..],
            true,
        ),
        (
            "(8alpha)-ergolin-8-ol",
            "[C@H]1(CN[C@@H]2CC3=CNC4=CC=CC([C@H]2C1)=C34)O",
            &[0, 3, 13][..],
            true,
        ),
    ] {
        check(unqualified, representative, StereoGroupType::Absolute).unwrap();
        for (prefix, kind) in [
            ("rel", StereoGroupType::Relative),
            ("rac", StereoGroupType::Racemic),
        ] {
            let name = format!("{prefix}-{unqualified}");
            let intended = format!("{representative} |r|");
            let structure =
                check(&name, &intended, kind).unwrap_or_else(|error| panic!("{name}: {error}"));
            let written = structure.semantic_cxsmiles().unwrap();
            assert_eq!(
                written.split(" |").next().unwrap(),
                representative,
                "{name}"
            );
            let expected_layer = if prefix == "rel" {
                format!(
                    "|o1:{}|",
                    grouped_atoms
                        .iter()
                        .map(usize::to_string)
                        .collect::<Vec<_>>()
                        .join(",")
                )
            } else {
                "|r|".into()
            };
            assert!(written.ends_with(&expected_layer), "{name}: {written}");
            assert_eq!(
                stereoisomers(&written).unwrap().len(),
                if chiral { 2 } else { 1 },
                "{name}"
            );
        }
    }
}

#[test]
fn whole_molecule_qualifiers_cover_nested_and_ester_centres() {
    for (unqualified, representative) in [
        (
            "(1R,2S)-2-[(1S)-1-fluoroethyl]cyclohexan-1-ol",
            "O[C@H]1[C@@H]([C@@H](F)C)CCCC1",
        ),
        (
            "(1R)-1-fluoroethyl (2S)-2-hydroxypropanoate",
            "O[C@H](C(=O)O[C@@H](C)F)C",
        ),
        (
            "(1R)-1-[(1R,2S)-2-methylcyclohexyl]ethan-1-ol",
            "C[C@@H](O)[C@H]1[C@@H](C)CCCC1",
        ),
        (
            "(5beta)-androstane",
            "C[C@@]12CCC[C@H]1[C@@H]1CC[C@@H]3CCCC[C@]3(C)[C@H]1CC2",
        ),
    ] {
        let absolute = check(unqualified, representative, StereoGroupType::Absolute)
            .unwrap()
            .semantic_cxsmiles()
            .unwrap();
        for (prefix, kind) in [
            ("rel", StereoGroupType::Relative),
            ("rac", StereoGroupType::Racemic),
        ] {
            let name = format!("{prefix}-{unqualified}");
            let structure = check(&name, &format!("{representative} |r|"), kind)
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            let written = structure.semantic_cxsmiles().unwrap();
            assert_eq!(written.split(" |").next().unwrap(), absolute, "{name}");
        }
    }
    let androstane = "C[C@@]12CCC[C@H]1[C@@H]1CC[C@H]3CCCC[C@]3(C)[C@H]1CC2";
    for (prefix, kind) in [
        ("rel", StereoGroupType::Relative),
        ("rac", StereoGroupType::Racemic),
    ] {
        check(
            &format!("{prefix}-5alpha-androstane"),
            &format!("{androstane} |r|"),
            kind,
        )
        .unwrap();
    }
}
