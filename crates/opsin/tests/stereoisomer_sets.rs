use std::collections::BTreeSet;

use opsin::{ParseOptions, Parser, Status};
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

/// Parsing must succeed; only a stereoisomer-set mismatch is an `Err`, so a
/// known failure cannot hide a name that stopped parsing.
fn check(name: &str, intended: &str) -> Result<(), String> {
    let result = Parser::new().unwrap().parse(name, &ParseOptions::strict());
    assert_eq!(result.status, Status::Success, "{name}: {}", result.message);
    let actual = result.structure.unwrap().semantic_cxsmiles().unwrap();
    let actual_set = stereoisomers(&actual).unwrap();
    let intended_set = stereoisomers(intended).unwrap();
    if actual_set != intended_set {
        return Err(format!(
            "{actual}: actual {actual_set:?}, intended {intended_set:?}"
        ));
    }
    check_relative_mirror(name, &actual_set)
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
    for (name, intended, known_failure) in [
        (
            "(1R,2S,3R,4S)-3-methylbicyclo[2.2.1]heptan-2-ol",
            BICYCLO_A,
            None,
        ),
        (
            "rel-(1R,2S,3R,4S)-3-methylbicyclo[2.2.1]heptan-2-ol",
            a_pair.as_str(),
            None,
        ),
        (
            "rac-(1R,2S,3R,4S)-3-methylbicyclo[2.2.1]heptan-2-ol",
            a_pair.as_str(),
            None,
        ),
        (
            "rel-(1R,2S,3S,4S)-3-methylbicyclo[2.2.1]heptan-2-ol",
            b_pair.as_str(),
            None,
        ),
        (
            "rac-(1R,2S,3S,4S)-3-methylbicyclo[2.2.1]heptan-2-ol",
            b_pair.as_str(),
            None,
        ),
        (
            "rac-(1R,2R,4S)-bicyclo[2.2.1]heptan-2-ol",
            c_pair.as_str(),
            None,
        ),
        (
            "rel-(1R,2S)-2-methylcyclohexan-1-ol",
            // CLI: (1R,2S).
            "O[C@H]1[C@@H](C)CCCC1 |r|",
            None,
        ),
        (
            "rel-1-[(1R,2S)-2-methylcyclohexyl]ethan-1-one",
            // CLI: ring (1R,2S).
            "CC(=O)[C@H]1[C@@H](C)CCCC1 |r|",
            None,
        ),
        (
            "(1R*,2S*)-2-methylcyclohexan-1-ol",
            "O[C@H]1[C@@H](C)CCCC1 |r|",
            None,
        ),
        (
            "(1RS,2SR)-2-methylcyclohexan-1-ol",
            "O[C@H]1[C@@H](C)CCCC1 |r|",
            None,
        ),
        // #934: leading rel-/rac- does not reach nested descriptor sets.
        (
            "rel-(2R)-2-[(1R,2S,3R,4S)-3-methylbicyclo[2.2.1]heptan-2-yl]propan-1-ol",
            d_pair.as_str(),
            Some(934_u32),
        ),
        (
            "rac-(2R)-2-[(1R,2S,3R,4S)-3-methylbicyclo[2.2.1]heptan-2-yl]propan-1-ol",
            d_pair.as_str(),
            Some(934),
        ),
        (
            "rel-(1R,2S,3R,4S)-3-[(1S)-1-fluoroethyl]bicyclo[2.2.1]heptan-2-ol",
            e_pair.as_str(),
            Some(934),
        ),
        (
            "rac-(1R,2S,3R,4S)-3-[(1S)-1-fluoroethyl]bicyclo[2.2.1]heptan-2-ol",
            e_pair.as_str(),
            Some(934),
        ),
        (
            "rel-(1R)-1-[(1R,2S)-2-methylcyclohexyl]ethan-1-ol",
            // CLI: side chain (1R), ring (1R,2S).
            "C[C@@H](O)[C@H]1[C@@H](C)CCCC1 |r|",
            Some(934),
        ),
        (
            "rac-(1R)-1-[(1R,2S)-2-methylcyclohexyl]ethan-1-ol",
            "C[C@@H](O)[C@H]1[C@@H](C)CCCC1 |r|",
            Some(934),
        ),
        (
            "rac-(1R,2S)-2-[(1S)-1-fluoroethyl]cyclohexan-1-ol",
            // CLI: ring (1R,2S), fluoroethyl (1S).
            "O[C@H]1[C@@H]([C@@H](F)C)CCCC1 |r|",
            Some(934),
        ),
    ] {
        match (known_failure, check(name, intended)) {
            (None, Ok(())) => eprintln!("PASS {name}"),
            (Some(issue), Err(error)) => eprintln!("KNOWN #{issue} {name}: {error}"),
            (None, Err(error)) => panic!("{name}: {error}"),
            (Some(issue), Ok(())) => {
                panic!("{name} now passes: move this row to the passing cases and close #{issue}")
            }
        }
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
