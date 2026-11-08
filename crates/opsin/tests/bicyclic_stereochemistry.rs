// SPDX-License-Identifier: MIT
// Origin: original nazuchem/ReShiki code; source history and separately attributed data/API context follow.
//   ReShiki, crates/opsin/tests/bicyclic_stereochemistry.rs @ d9859a4d1dbdae0e25ebf9a5201f963677c73b19.

//! Rigid cage descriptors must define the bridgeheads and the substituted atom.
use std::collections::{BTreeMap, BTreeSet};

use nazuchem_kernel::smiles;
use opsin::graph::StereoGroupType;
use opsin::{ParseOptions, Parser, Status, Structure, WarningKind};

fn parse(name: &str) -> Structure {
    let result = Parser::new().unwrap().parse(name, &ParseOptions::strict());
    assert_eq!(result.status, Status::Success, "{name}: {}", result.message);
    result.structure.unwrap()
}
const CAGE_GROUP_SCOPE_ERROR: &str =
    "rel-/rac- outside an endo/exo cage's own scope is not supported";

fn assert_group_scope_refused(name: &str) {
    let result = Parser::new().unwrap().parse(name, &ParseOptions::strict());
    assert_eq!(result.status, Status::Failure, "{name}: {}", result.message);
    assert!(result.structure.is_none(), "{name}");
    assert!(
        result.message.contains(CAGE_GROUP_SCOPE_ERROR),
        "{name}: {}",
        result.message
    );
}

fn canonical(text: &str) -> String {
    let imported = smiles::read(text, &Default::default()).unwrap();
    smiles::write::write(&imported.prepared.state, Default::default())
        .unwrap()
        .text
}
fn mirror(text: &str) -> String {
    text.replace("@@", "!").replace('@', "@@").replace('!', "@")
}
fn stereo_group_signature(structure: &Structure) -> String {
    let mut groups = BTreeMap::new();
    for atom in structure.graph.atoms.iter().filter(|atom| atom.active) {
        if let Some(parity) = &atom.parity {
            groups
                .entry(parity.stereo_group)
                .or_insert_with(Vec::new)
                .push(atom.locants.first().unwrap().as_str());
        }
    }
    let mut signatures = groups
        .into_iter()
        .map(|(group, mut locants)| {
            locants.sort();
            format!("{:?}:{}", group.kind, locants.join(","))
        })
        .collect::<Vec<_>>();
    signatures.sort();
    signatures.join(";")
}
fn assert_fixture(name: &str, fixture: &str, bridgeheads: [&str; 2], targets: &[&str]) -> String {
    let structure = parse(name);
    for locant in bridgeheads.into_iter().chain(targets.iter().copied()) {
        let atom = structure
            .graph
            .atom_by_locant(structure.fragment, locant)
            .unwrap();
        assert!(
            structure.graph.atom(atom).parity.is_some(),
            "{name}: missing parity at {locant}"
        );
    }
    let written = structure.semantic_cxsmiles().unwrap();
    let actual = canonical(&written);
    assert!(
        actual == canonical(fixture) || actual == canonical(&mirror(fixture)),
        "{name}: {written} => {actual}, expected {fixture}"
    );
    actual
}

#[test]
fn norbornane_exo_endo_match_complete_cage_fixtures() {
    let exo = assert_fixture(
        "(2-exo)-bicyclo[2.2.1]heptan-2-ol",
        "O[C@H]1C[C@@H]2CC[C@H]1C2",
        ["1", "4"],
        &["2"],
    );
    let endo = assert_fixture(
        "(2-endo)-bicyclo[2.2.1]heptan-2-ol",
        "O[C@@H]1C[C@@H]2CC[C@H]1C2",
        ["1", "4"],
        &["2"],
    );
    assert_ne!(exo, endo);
    for (descriptor, expected) in [("exo", exo), ("endo", endo)] {
        let suffix = parse(&format!("bicyclo[2.2.1]heptan-2-{descriptor}-ol"));
        assert_eq!(canonical(&suffix.semantic_cxsmiles().unwrap()), expected);
    }
}

#[test]
fn skeletal_replacement_preserves_endo_exo_cage_geometry() {
    let endo = assert_fixture(
        "(3-endo)-8-methyl-8-azabicyclo[3.2.1]octan-3-ol",
        "CN1[C@@H]2CC[C@H]1C[C@@H](O)C2",
        ["1", "5"],
        &["3"],
    );
    let exo = assert_fixture(
        "(3-exo)-8-methyl-8-azabicyclo[3.2.1]octan-3-ol",
        "CN1[C@@H]2CC[C@H]1C[C@H](O)C2",
        ["1", "5"],
        &["3"],
    );
    assert_ne!(exo, endo);
    assert_fixture(
        "8-azabicyclo[3.2.1]octan-3-exo-ol",
        "N1[C@@H]2CC[C@H]1C[C@H](O)C2",
        ["1", "5"],
        &["3"],
    );
    for (descriptor, expected) in [("exo", exo), ("endo", endo)] {
        let suffix = parse(&format!(
            "8-methyl-8-azabicyclo[3.2.1]octan-3-{descriptor}-ol"
        ));
        assert_eq!(canonical(&suffix.semantic_cxsmiles().unwrap()), expected);
    }
}

#[test]
fn short_bridge_fixtures_distinguish_syn_from_anti() {
    let anti = assert_fixture(
        "2-endo-bromo-7-anti-fluorobicyclo[2.2.1]heptane",
        "Br[C@@H]1C[C@@H]2CC[C@H]1[C@H]2F",
        ["1", "4"],
        &["2", "7"],
    );
    let syn = assert_fixture(
        "2-endo-bromo-7-syn-fluorobicyclo[2.2.1]heptane",
        "Br[C@@H]1C[C@@H]2CC[C@H]1[C@@H]2F",
        ["1", "4"],
        &["2", "7"],
    );
    assert_ne!(anti, syn);
    let syn = assert_fixture(
        "8-syn-methylbicyclo[3.2.1]octane",
        "C[C@@H]1[C@@H]2CCC[C@H]1CC2",
        ["1", "5"],
        &["8"],
    );
    let anti = assert_fixture(
        "8-anti-methylbicyclo[3.2.1]octane",
        "C[C@H]1[C@@H]2CCC[C@H]1CC2",
        ["1", "5"],
        &["8"],
    );
    assert_ne!(anti, syn);
}

#[test]
fn unrelated_ring_systems_remain_unsupported() {
    for name in [
        "(2-exo)-bicyclo[2.2.2]octan-2-ol",
        "(2-endo)-cyclohexan-2-ol",
    ] {
        let result = Parser::new().unwrap().parse(name, &ParseOptions::strict());
        assert_eq!(result.status, Status::Failure, "{name}");
    }
}

#[test]
fn absolute_bridgehead_parity_selects_and_preserves_the_cage_mirror() {
    let r = parse("(1R,2-exo)-bicyclo[2.2.1]heptan-2-ol")
        .semantic_cxsmiles()
        .unwrap();
    let s = parse("(2-exo,1S)-bicyclo[2.2.1]heptan-2-ol")
        .semantic_cxsmiles()
        .unwrap();
    assert_ne!(canonical(&r), canonical(&s));
    assert_eq!(canonical(&r), canonical(&mirror(&s)));
    let contradictory = Parser::new().unwrap().parse(
        "(2-exo,2-endo)-bicyclo[2.2.1]heptan-2-ol",
        &ParseOptions::strict(),
    );
    assert_eq!(contradictory.status, Status::Failure);
    assert!(
        contradictory.message.contains("Contradictory cage"),
        "{}",
        contradictory.message
    );
}

#[test]
fn pseudo_cip_descriptor_cannot_override_the_requested_cage_face() {
    let agreeing = parse("(1R,3r,5S)-8-methyl-8-azabicyclo[3.2.1]octan-3-endo-ol");
    assert_eq!(
        canonical(&agreeing.semantic_cxsmiles().unwrap()),
        canonical("CN1[C@@H]2CC[C@H]1C[C@@H](O)C2")
    );
    let result = Parser::new().unwrap().parse(
        "(1R,3s,5S)-8-methyl-8-azabicyclo[3.2.1]octan-3-endo-ol",
        &ParseOptions::strict(),
    );
    assert_eq!(result.status, Status::Failure);
}

#[test]
fn grouped_descriptors_and_suffixes_keep_their_attachment_locants() {
    for (generated, prefix) in [
        (
            "bicyclo[3.2.1]octane-3-endo,8-syn-diol",
            "(3-endo,8-syn)-bicyclo[3.2.1]octane-3,8-diol",
        ),
        (
            "bicyclo[3.2.1]octane-3-endo,8-diol",
            "(3-endo)-bicyclo[3.2.1]octane-3,8-diol",
        ),
        (
            "3-endo,8-syn-dimethylbicyclo[3.2.1]octane",
            "(3-endo,8-syn)-3,8-dimethylbicyclo[3.2.1]octane",
        ),
        (
            "8-methyl-8-azabicyclo[3.2.1]octan-3-endo-yl acetate",
            "(3-endo)-8-methyl-8-azabicyclo[3.2.1]octan-3-yl acetate",
        ),
        (
            "bicyclo[2.2.1]hept-5-en-2-exo-ol",
            "(2-exo)-bicyclo[2.2.1]hept-5-en-2-ol",
        ),
    ] {
        let generated = parse(generated);
        let prefix = parse(prefix);
        assert_eq!(
            canonical(&generated.semantic_cxsmiles().unwrap()),
            canonical(&prefix.semantic_cxsmiles().unwrap())
        );
    }
}

#[test]
fn suffix_descriptors_define_each_bracketed_cage_independently() {
    let mut isomers = Vec::new();
    for (first, second) in [
        ("exo", "exo"),
        ("exo", "endo"),
        ("endo", "exo"),
        ("endo", "endo"),
    ] {
        let name = format!(
            "2-(bicyclo[2.2.1]heptan-2-{first}-yl)-3-(bicyclo[2.2.1]heptan-2-{second}-yl)propan-1-ol"
        );
        let structure = parse(&name);
        let cage_atoms = structure
            .graph
            .atoms
            .iter()
            .filter(|atom| {
                atom.active
                    && atom.in_cycle
                    && atom
                        .locants
                        .iter()
                        .any(|locant| ["1", "2", "4"].contains(&locant.as_str()))
            })
            .collect::<Vec<_>>();
        assert_eq!(cage_atoms.len(), 6, "{name}");
        for atom in cage_atoms {
            assert!(
                atom.parity.is_some(),
                "{name}: missing cage parity on {atom:?}"
            );
        }
        let prefix = parse(&format!(
            "2-((2-{first})-bicyclo[2.2.1]heptan-2-yl)-3-((2-{second})-bicyclo[2.2.1]heptan-2-yl)propan-1-ol",
        ));
        let actual = canonical(&structure.semantic_cxsmiles().unwrap());
        assert_eq!(actual, canonical(&prefix.semantic_cxsmiles().unwrap()));
        assert!(
            !isomers.contains(&actual),
            "{name}: descriptors did not distinguish the cages"
        );
        isomers.push(actual);
    }
}

#[test]
fn suffix_descriptor_prefixes_parse_inside_square_and_nested_groups() {
    for (name, prefix) in [
        (
            "2-[bicyclo[2.2.1]heptan-2-exo-yl]ethan-1-ol",
            "2-[(2-exo)-bicyclo[2.2.1]heptan-2-yl]ethan-1-ol",
        ),
        (
            "3-[2-(bicyclo[2.2.1]heptan-2-endo-yl)ethyl]pentan-1-ol",
            "3-[2-((2-endo)-bicyclo[2.2.1]heptan-2-yl)ethyl]pentan-1-ol",
        ),
    ] {
        let structure = parse(name);
        let explicit = parse(prefix);
        assert_eq!(
            canonical(&structure.semantic_cxsmiles().unwrap()),
            canonical(&explicit.semantic_cxsmiles().unwrap())
        );
        assert_eq!(
            structure
                .graph
                .atoms
                .iter()
                .filter(|atom| atom.active && atom.parity.is_some())
                .count(),
            3,
            "{name}: the cage must have defined bridgeheads and substituent parity"
        );
    }
}

#[test]
fn cage_descriptor_on_unsupported_bracketed_cage_does_not_reach_another_cage() {
    let name = "2-(bicyclo[2.2.2]octan-2-exo-yl)-3-(bicyclo[2.2.1]heptan-2-yl)propan-1-ol";
    let result = Parser::new().unwrap().parse(name, &ParseOptions::strict());
    assert_eq!(result.status, Status::Failure, "{name}");
}

#[test]
fn explicit_cage_groups_cover_both_bridgeheads_and_the_described_atom() {
    const A: &str = "[C@@H]12[C@@H](C[C@@H](CC1)C2)O";
    for (name, kind, extension) in [
        (
            "rac-(2-exo)-bicyclo[2.2.1]heptan-2-ol",
            StereoGroupType::Racemic,
            "r",
        ),
        (
            "rac-bicyclo[2.2.1]heptan-2-exo-ol",
            StereoGroupType::Racemic,
            "r",
        ),
        (
            "rel-(2-exo)-bicyclo[2.2.1]heptan-2-ol",
            StereoGroupType::Relative,
            "o1:0,1,3",
        ),
        (
            "rel-bicyclo[2.2.1]heptan-2-exo-ol",
            StereoGroupType::Relative,
            "o1:0,1,3",
        ),
        (
            "rac-(1R,2-exo)-bicyclo[2.2.1]heptan-2-ol",
            StereoGroupType::Racemic,
            "r",
        ),
        (
            "rel-(1R,2-exo)-bicyclo[2.2.1]heptan-2-ol",
            StereoGroupType::Relative,
            "o1:0,1,3",
        ),
        (
            "rac-(1R,2R,4S)-bicyclo[2.2.1]heptan-2-ol",
            StereoGroupType::Racemic,
            "r",
        ),
        (
            "bicyclo[2.2.1]heptan-2-exo-ol",
            StereoGroupType::Relative,
            "o1:0,1,3",
        ),
    ] {
        let structure = parse(name);
        let groups: Vec<_> = ["1", "2", "4"]
            .map(|locant| {
                let atom = structure
                    .graph
                    .atom_by_locant(structure.fragment, locant)
                    .unwrap();
                structure
                    .graph
                    .atom(atom)
                    .parity
                    .as_ref()
                    .unwrap()
                    .stereo_group
            })
            .into();
        assert!(
            groups
                .iter()
                .all(|group| group.kind == kind && *group == groups[0]),
            "{name}: {groups:?}"
        );
        let written = structure.semantic_cxsmiles().unwrap();
        assert_eq!(written, format!("{A} |{extension}|"), "{name}");
        if kind == StereoGroupType::Relative {
            let imported = smiles::read(&written, &Default::default()).unwrap();
            let groups = &imported.prepared.state.metadata.groups;
            assert_eq!(groups.len(), 1, "{name}: {written}");
            assert_eq!(groups[0].kind, 1, "{name}");
            assert_eq!(groups[0].atoms.len(), 3, "{name}: {written}");
        }
    }
}

#[test]
fn achiral_cages_and_absolute_cip_anchored_cages_stay_ungrouped() {
    for name in [
        "8-methyl-8-azabicyclo[3.2.1]octan-3-endo-ol",
        "8-methyl-8-azabicyclo[3.2.1]octan-3-exo-ol",
        "8-azabicyclo[3.2.1]octan-3-endo-ol",
        "8-syn-methylbicyclo[3.2.1]octane",
        "8-anti-methylbicyclo[3.2.1]octane",
        "(1R)-bicyclo[2.2.1]heptan-2-exo-ol",
        "(1R,2-exo)-bicyclo[2.2.1]heptan-2-ol",
    ] {
        let structure = parse(name);
        assert!(
            structure
                .graph
                .atoms
                .iter()
                .filter_map(|atom| atom.parity.as_ref())
                .all(|parity| parity.stereo_group.kind == StereoGroupType::Absolute),
            "{name}"
        );
        assert!(
            !structure.semantic_cxsmiles().unwrap().contains('|'),
            "{name}"
        );
    }
}

#[test]
fn cage_groups_include_all_descriptors_in_one_group() {
    for (name, kind, centres) in [
        (
            "(1R*)-bicyclo[2.2.1]heptan-2-exo-ol",
            StereoGroupType::Relative,
            3,
        ),
        (
            "rac-(2-exo,7-syn)-bicyclo[2.2.1]heptane-2,7-diol",
            StereoGroupType::Racemic,
            4,
        ),
        (
            "bicyclo[2.2.1]heptane-2-exo,7-syn-diol",
            StereoGroupType::Relative,
            4,
        ),
    ] {
        let structure = parse(name);
        let groups: Vec<_> = structure
            .graph
            .atoms
            .iter()
            .filter_map(|atom| atom.parity.as_ref().map(|parity| parity.stereo_group))
            .collect();
        assert_eq!(groups.len(), centres, "{name}");
        assert!(
            groups
                .iter()
                .all(|group| group.kind == kind && *group == groups[0]),
            "{name}: {groups:?}"
        );
    }
}

#[test]
fn explicit_groups_on_multiple_cages_are_refused() {
    assert_group_scope_refused(
        "2-(bicyclo[2.2.1]heptan-2-exo-yl)-3-[rel-(2-exo)-bicyclo[2.2.1]heptan-2-yl]propan-1-ol",
    );
    for prefix in ["rel", "rac"] {
        let name = format!(
            "2-[{prefix}-(2-exo)-bicyclo[2.2.1]heptan-2-yl]-3-[{prefix}-(2-exo)-bicyclo[2.2.1]heptan-2-yl]propan-1-ol"
        );
        assert_group_scope_refused(&name);
    }
}

#[test]
fn single_cage_groups_allow_simple_ring_substituents() {
    for substituent in ["phenyl", "cyclohexyl"] {
        for (prefix, kind) in [("rel", "Relative"), ("rac", "Racemic")] {
            let name = format!("{prefix}-(1R,3R)-3-{substituent}bicyclo[2.2.1]heptan-2-exo-ol");
            let structure = parse(&name);
            assert_eq!(
                stereo_group_signature(&structure),
                format!("{kind}:1,2,3,4"),
                "{name}"
            );
            let members =
                smiles::stereoisomers(&structure.semantic_cxsmiles().unwrap(), &Default::default())
                    .unwrap();
            assert_eq!(members.len(), 2, "{name}");
        }
    }
}

#[test]
fn explicitly_grouped_cip_descriptors_on_multiple_cages_are_refused() {
    for prefix in ["rel", "rac"] {
        let suffix_name = format!(
            "2-[{prefix}-(1R)-bicyclo[2.2.1]heptan-2-exo-yl]-3-[{prefix}-(1R)-bicyclo[2.2.1]heptan-2-exo-yl]propan-1-ol"
        );
        let combined_name = format!(
            "2-[{prefix}-(1R,2-exo)-bicyclo[2.2.1]heptan-2-yl]-3-[{prefix}-(1R,2-exo)-bicyclo[2.2.1]heptan-2-yl]propan-1-ol"
        );
        assert_group_scope_refused(&suffix_name);
        assert_group_scope_refused(&combined_name);
    }
}

#[test]
fn explicit_groups_on_multiple_cages_and_the_parent_are_refused() {
    for prefix in ["rel", "rac"] {
        let name = format!(
            "{prefix}-(2R)-2-[{prefix}-(1R)-bicyclo[2.2.1]heptan-2-exo-yl]-3-[{prefix}-(1R)-bicyclo[2.2.1]heptan-2-exo-yl]propan-1-ol"
        );
        assert_group_scope_refused(&name);
    }
}

#[test]
fn mixed_absolute_and_starred_frame_descriptors_are_refused() {
    for name in [
        "(1R,4S*)-bicyclo[2.2.1]heptan-2-exo-ol",
        "(1R*,4S)-bicyclo[2.2.1]heptan-2-exo-ol",
    ] {
        assert_group_scope_refused(name);
    }
}

#[test]
fn absolute_cip_outside_the_frame_preserves_both_cage_diastereomers() {
    for (locant, descriptor, representative, inverted) in [
        (
            "3",
            "R",
            "C[C@H]1[C@H]([C@@H]2CC[C@H]1C2)O",
            "O[C@H]1[C@H](C)[C@@H]2CC[C@H]1C2",
        ),
        (
            "3",
            "S",
            "C[C@@H]1[C@H]([C@@H]2CC[C@H]1C2)O",
            "O[C@H]1[C@@H](C)[C@@H]2CC[C@H]1C2",
        ),
        (
            "7",
            "R",
            "C[C@H]1[C@@H]2[C@@H](C[C@H]1CC2)O",
            "C[C@H]1[C@H]2[C@H](C[C@@H]1CC2)O",
        ),
        (
            "7",
            "S",
            "C[C@@H]1[C@@H]2[C@@H](C[C@H]1CC2)O",
            "C[C@@H]1[C@H]2[C@H](C[C@@H]1CC2)O",
        ),
    ] {
        let name = format!("({locant}{descriptor})-{locant}-methylbicyclo[2.2.1]heptan-2-exo-ol");
        let mut structure = parse(&name);
        let outside = structure
            .graph
            .atom_by_locant(structure.fragment, locant)
            .unwrap();
        let outside_parity = structure.graph.atom(outside).parity.clone().unwrap();
        assert_eq!(outside_parity.stereo_group.kind, StereoGroupType::Absolute);
        let frame = ["1", "2", "4"].map(|locant| {
            structure
                .graph
                .atom_by_locant(structure.fragment, locant)
                .unwrap()
        });
        let group = structure
            .graph
            .atom(frame[0])
            .parity
            .as_ref()
            .unwrap()
            .stereo_group;
        assert_eq!(group.kind, StereoGroupType::Relative, "{name}");
        for &atom in &frame {
            assert_eq!(
                structure
                    .graph
                    .atom(atom)
                    .parity
                    .as_ref()
                    .unwrap()
                    .stereo_group,
                group,
                "{name}"
            );
        }
        let (written, locants) = structure.semantic_cxsmiles_with_locants().unwrap();
        assert_eq!(
            canonical(&written),
            canonical(representative),
            "{name}: {written}"
        );
        let imported = smiles::read(&written, &Default::default()).unwrap();
        let groups = &imported.prepared.state.metadata.groups;
        assert_eq!(groups.len(), 1, "{name}: {written}");
        assert_eq!(groups[0].kind, 1);
        let mut grouped_locants: Vec<_> = groups[0]
            .atoms
            .iter()
            .map(|&index| locants[index].as_deref().unwrap())
            .collect();
        grouped_locants.sort();
        assert_eq!(grouped_locants, ["1", "2", "4"], "{name}: {written}");
        for atom in frame {
            let parity = structure.graph.atom_mut(atom).parity.as_mut().unwrap();
            parity.parity *= -1;
            parity.stereo_group = Default::default();
        }
        assert_eq!(
            structure.graph.atom(outside).parity.as_ref().unwrap(),
            &outside_parity
        );
        let written = structure.semantic_cxsmiles().unwrap();
        assert_eq!(
            canonical(&written),
            canonical(inverted),
            "{name}: {written}"
        );
        assert_ne!(canonical(representative), canonical(inverted), "{name}");
    }
}

#[test]
fn absolute_frame_anchors_preserve_the_specified_diastereomer() {
    for (name, expected) in [
        (
            "(1R,3R)-3-methylbicyclo[2.2.1]heptan-2-exo-ol",
            "C[C@H]1[C@H]([C@@H]2CC[C@H]1C2)O",
        ),
        (
            "(1S,3R)-3-methylbicyclo[2.2.1]heptan-2-exo-ol",
            "C[C@H]1[C@@H]([C@H]2CC[C@@H]1C2)O",
        ),
        (
            "(3R)-3-exo-methylbicyclo[2.2.1]heptan-2-exo-ol",
            "C[C@H]1[C@H]([C@@H]2CC[C@H]1C2)O",
        ),
        (
            "(1R,3R)-3-[(1S)-bicyclo[2.2.1]heptan-2-exo-yl]bicyclo[2.2.1]heptan-2-exo-ol",
            "[C@H]12[C@H](C[C@H](CC1)C2)[C@@H]2[C@H]([C@@H]1CC[C@H]2C1)O",
        ),
        (
            "(1R,3R)-3-(bicyclo[2.2.1]heptan-2-yl)bicyclo[2.2.1]heptan-2-exo-ol",
            "C12C(CC(CC1)C2)[C@@H]2[C@H]([C@@H]1CC[C@H]2C1)O",
        ),
        (
            "(1R,3R)-3-[(1S)-1-fluoroethyl]bicyclo[2.2.1]heptan-2-exo-ol",
            "F[C@@H](C)[C@H]1[C@H]([C@@H]2CC[C@H]1C2)O",
        ),
        (
            "(1S,3S)-3-[(1R)-1-fluoroethyl]bicyclo[2.2.1]heptan-2-exo-ol",
            "F[C@H](C)[C@@H]1[C@@H]([C@H]2CC[C@@H]1C2)O",
        ),
    ] {
        let structure = parse(name);
        assert_eq!(
            canonical(&structure.semantic_cxsmiles().unwrap()),
            canonical(expected),
            "{name}"
        );
        assert!(
            structure
                .graph
                .atoms
                .iter()
                .filter_map(|atom| atom.parity.as_ref())
                .all(|parity| parity.stereo_group.kind == StereoGroupType::Absolute),
            "{name}"
        );
    }
}

#[test]
fn all_cage_descriptors_are_collected_before_choosing_the_mirror() {
    for (names, expected) in [
        (
            [
                "(3R)-3-endo-methylbicyclo[2.2.1]heptan-2-exo-ol",
                "(3R,3-endo,2-exo)-3-methylbicyclo[2.2.1]heptan-2-ol",
                "(3R,2-exo,3-endo)-3-methylbicyclo[2.2.1]heptan-2-ol",
            ],
            "O[C@H]1[C@H](C)[C@@H]2CC[C@H]1C2",
        ),
        (
            [
                "(3R)-3-exo-methylbicyclo[2.2.1]heptan-2-endo-ol",
                "(3R,3-exo,2-endo)-3-methylbicyclo[2.2.1]heptan-2-ol",
                "(3R,2-endo,3-exo)-3-methylbicyclo[2.2.1]heptan-2-ol",
            ],
            "C[C@H]1[C@@H]([C@@H]2CC[C@H]1C2)O",
        ),
    ] {
        for name in names {
            let written = parse(name).semantic_cxsmiles().unwrap();
            assert!(!written.contains('|'), "{name}: {written}");
            assert_eq!(
                canonical(&written),
                canonical(expected),
                "{name}: {written}"
            );
        }
    }
    for name in [
        "(1R,3R)-3-endo-methylbicyclo[2.2.1]heptan-2-exo-ol",
        "(1S,3R)-3-exo-methylbicyclo[2.2.1]heptan-2-exo-ol",
    ] {
        let result = Parser::new().unwrap().parse(name, &ParseOptions::strict());
        assert_eq!(result.status, Status::Failure, "{name}");
        assert!(
            result.message.contains("Contradictory cage"),
            "{name}: {}",
            result.message
        );
    }
}

#[test]
fn nonabsolute_cip_outside_the_frame_is_refused() {
    for descriptor in ["3R*", "3S*", "3RS", "3SR"] {
        let name = format!("({descriptor})-3-methylbicyclo[2.2.1]heptan-2-exo-ol");
        assert_group_scope_refused(&name);
    }
}

#[test]
fn explicit_groups_crossing_the_cage_frame_preserve_their_correlations() {
    let mirror_pair =
        |fixture: &str| BTreeSet::from([canonical(fixture), canonical(&mirror(fixture))]);
    let rr = mirror_pair("C[C@H]1[C@H]([C@@H]2CC[C@H]1C2)O");
    let rs = mirror_pair("C[C@@H]1[C@H]([C@@H]2CC[C@H]1C2)O");
    assert_eq!(rr.len(), 2);
    assert_eq!(rs.len(), 2);
    assert!(rr.is_disjoint(&rs));

    // Correlated single-cage cases from the anchor review, including the distinct
    // rel-/rac-(1R,3R) and (1R,3S) full-cage mirror pairs.
    for (name, expected_groups, expected_pair) in [
        (
            "(1R*,3R*)-3-methylbicyclo[2.2.1]heptan-2-exo-ol",
            "Relative:1,2,3,4",
            Some(&rr),
        ),
        (
            "(1R*,3S*)-3-methylbicyclo[2.2.1]heptan-2-exo-ol",
            "Relative:1,2,3,4",
            Some(&rs),
        ),
        (
            "(1RS,3RS)-3-methylbicyclo[2.2.1]heptan-2-exo-ol",
            "Racemic:1,2,3,4",
            Some(&rr),
        ),
        (
            "(1RS,3SR)-3-methylbicyclo[2.2.1]heptan-2-exo-ol",
            "Racemic:1,2,3,4",
            Some(&rs),
        ),
        (
            "(1SR,3RS)-3-methylbicyclo[2.2.1]heptan-2-exo-ol",
            "Racemic:1,2,3,4",
            Some(&rs),
        ),
        (
            "(1R*,3R*,5S*)-3,5-dimethylbicyclo[2.2.1]heptan-2-exo-ol",
            "Relative:1,2,3,4,5",
            None,
        ),
        (
            "(1R*,4R*)-4-methylbicyclo[3.2.1]octan-2-exo-ol",
            "Relative:1,2,4,5",
            None,
        ),
        (
            "(1RS,4RS)-4-methylbicyclo[3.2.1]octan-2-exo-ol",
            "Racemic:1,2,4,5",
            None,
        ),
        (
            "(2-exo,3R*,1R*)-3-methylbicyclo[2.2.1]heptan-2-ol",
            "Relative:1,2,3,4",
            Some(&rr),
        ),
        (
            "(1R*,3R*,2-exo)-3-methylbicyclo[2.2.1]heptan-2-ol",
            "Relative:1,2,3,4",
            Some(&rr),
        ),
        (
            "rel-(1R,3R)-3-methylbicyclo[2.2.1]heptan-2-exo-ol",
            "Relative:1,2,3,4",
            Some(&rr),
        ),
        (
            "rel-(1R,3S)-3-methylbicyclo[2.2.1]heptan-2-exo-ol",
            "Relative:1,2,3,4",
            Some(&rs),
        ),
        (
            "rac-(1R,3R)-3-methylbicyclo[2.2.1]heptan-2-exo-ol",
            "Racemic:1,2,3,4",
            Some(&rr),
        ),
        (
            "rac-(1R,3S)-3-methylbicyclo[2.2.1]heptan-2-exo-ol",
            "Racemic:1,2,3,4",
            Some(&rs),
        ),
    ] {
        let structure = parse(name);
        assert_eq!(
            stereo_group_signature(&structure),
            expected_groups,
            "{name}"
        );
        let members =
            smiles::stereoisomers(&structure.semantic_cxsmiles().unwrap(), &Default::default())
                .unwrap();
        let independent_groups = expected_groups
            .split(';')
            .filter(|group| !group.starts_with("Absolute:"))
            .count();
        assert_eq!(
            members.len(),
            1 << independent_groups,
            "{name}: {members:?}"
        );
        if let Some(expected_pair) = expected_pair {
            assert_eq!(&members, expected_pair, "{name}");
        }
    }
}

#[test]
fn correlated_groups_on_multiple_cages_and_the_parent_are_refused() {
    for prefix in ["rel", "rac"] {
        let name = format!(
            "{prefix}-(2R)-2-[{prefix}-(1R,3R)-3-methylbicyclo[2.2.1]heptan-2-exo-yl]-3-[{prefix}-(1R,3S)-3-methylbicyclo[2.2.1]heptan-2-endo-yl]propan-1-ol"
        );
        assert_group_scope_refused(&name);
    }
}

#[test]
fn mixed_groups_within_one_cage_and_starred_groups_on_multiple_cages_are_refused() {
    for name in [
        "(1R*,3RS)-3-methylbicyclo[2.2.1]heptan-2-exo-ol",
        "(1RS,3R*)-3-methylbicyclo[2.2.1]heptan-2-exo-ol",
        "(1R*,3R*,5S)-3,5-dimethylbicyclo[2.2.1]heptan-2-exo-ol",
        "(1R*,3R*,5R)-3,5-dimethylbicyclo[2.2.1]heptan-2-exo-ol",
        "2-[(1R*,3R*)-3-methylbicyclo[2.2.1]heptan-2-exo-yl]-3-(bicyclo[2.2.1]heptan-2-endo-yl)propan-1-ol",
    ] {
        assert_group_scope_refused(name);
    }
}

#[test]
fn grouped_stereo_outside_a_nested_cage_is_refused() {
    for descriptor in ["rel-(2R)", "rac-(2R)", "(2R*)", "(2S*)", "(2RS)", "(2SR)"] {
        let name =
            format!("{descriptor}-2-[(1R,3R)-3-methylbicyclo[2.2.1]heptan-2-exo-yl]propan-1-ol");
        assert_group_scope_refused(&name);
    }
}

#[test]
fn explicit_cage_groups_with_other_molecular_stereo_are_refused() {
    // Exact scope-regression names from the S855 follow-up report. An explicit
    // cage qualifier requires every parity to belong to its single cage group.
    for name in [
        // S01
        "rel-(2R)-2-[(1R,3R)-3-methylbicyclo[2.2.1]heptan-2-exo-yl]propan-1-ol",
        // S02
        "rac-(2R)-2-[(1R,3R)-3-methylbicyclo[2.2.1]heptan-2-exo-yl]propan-1-ol",
        // S03
        "rel-(1R,3R)-3-[(1R)-bicyclo[2.2.1]heptan-2-exo-yl]bicyclo[2.2.1]heptan-2-exo-ol",
        // S04
        "rac-(1R,3R)-3-[(1R)-bicyclo[2.2.1]heptan-2-exo-yl]bicyclo[2.2.1]heptan-2-exo-ol",
        // S05
        "rel-(1S,3S)-3-[(1S)-bicyclo[2.2.1]heptan-2-exo-yl]bicyclo[2.2.1]heptan-2-exo-ol",
        // S06
        "rac-(1S,3S)-3-[(1S)-bicyclo[2.2.1]heptan-2-exo-yl]bicyclo[2.2.1]heptan-2-exo-ol",
        // S07
        "rel-(1R,3R)-3-[(1S)-bicyclo[2.2.1]heptan-2-exo-yl]bicyclo[2.2.1]heptan-2-exo-ol",
        // S08
        "rac-(1R,3R)-3-[(1S)-bicyclo[2.2.1]heptan-2-exo-yl]bicyclo[2.2.1]heptan-2-exo-ol",
        // S09
        "rel-(1R,3R)-3-[(1S)-1-fluoroethyl]bicyclo[2.2.1]heptan-2-exo-ol",
        // S10
        "rac-(1R,3R)-3-[(1S)-1-fluoroethyl]bicyclo[2.2.1]heptan-2-exo-ol",
        // S11
        "rel-(1S,3S)-3-[(1R)-1-fluoroethyl]bicyclo[2.2.1]heptan-2-exo-ol",
        // S12
        "rac-(1S,3S)-3-[(1R)-1-fluoroethyl]bicyclo[2.2.1]heptan-2-exo-ol",
        // S13
        "rel-(1R)-bicyclo[2.2.1]heptan-2-exo-yl (2S)-2-hydroxypropanoate",
        // S14
        "rac-(1R)-bicyclo[2.2.1]heptan-2-exo-yl (2S)-2-hydroxypropanoate",
        // S15
        "(1R*,3R*)-3-[(1S)-1-fluoroethyl]bicyclo[2.2.1]heptan-2-exo-ol",
        // S16
        "(1RS,3RS)-3-[(1S)-1-fluoroethyl]bicyclo[2.2.1]heptan-2-exo-ol",
        // S17
        "(2S)-2-[rel-(1R,3R)-3-methylbicyclo[2.2.1]heptan-2-exo-yl]propan-1-ol",
        // S18
        "(2S)-2-[rac-(1R,3R)-3-methylbicyclo[2.2.1]heptan-2-exo-yl]propan-1-ol",
        // S19
        "(1R,3R)-3-[(1R*,2S*)-1,2-dihydroxypropyl]bicyclo[2.2.1]heptan-2-exo-ol",
        // S20
        "(1R,3R)-3-[(1RS,2SR)-1,2-dihydroxypropyl]bicyclo[2.2.1]heptan-2-exo-ol",
        // S21
        "(1R*,3R*)-3-[(1RS,2SR)-1,2-dihydroxypropyl]bicyclo[2.2.1]heptan-2-exo-ol",
        // S22
        "(1RS,3RS)-3-[(1R*,2S*)-1,2-dihydroxypropyl]bicyclo[2.2.1]heptan-2-exo-ol",
        // S24
        "(1R*,3R*)-3-[(1S)-bicyclo[2.2.1]heptan-2-exo-yl]bicyclo[2.2.1]heptan-2-exo-ol",
        // S25
        "(1RS,3RS)-3-[(1S)-bicyclo[2.2.1]heptan-2-exo-yl]bicyclo[2.2.1]heptan-2-exo-ol",
        // S28
        "(1R,3R)-3-[(1R*,2S*)-1,2-dihydroxypropyl]bicyclo[2.2.1]heptan-2-endo-ol",
    ] {
        assert_group_scope_refused(name);
    }
}

#[test]
fn explicit_cage_groups_with_stereogenic_axes_are_refused() {
    for name in [
        "rel-(1R)-bicyclo[2.2.1]heptan-2-exo-yl (1P)-2',5'-dimethoxy-6-nitro[1,1'-biphenyl]-2-carboxylate",
        "rac-(1R)-bicyclo[2.2.1]heptan-2-exo-yl (1P)-2',5'-dimethoxy-6-nitro[1,1'-biphenyl]-2-carboxylate",
        "rel-(1R)-bicyclo[2.2.1]heptan-2-exo-yl (1M)-2',5'-dimethoxy-6-nitro[1,1'-biphenyl]-2-carboxylate",
        "rac-(1R)-bicyclo[2.2.1]heptan-2-exo-yl (1M)-2',5'-dimethoxy-6-nitro[1,1'-biphenyl]-2-carboxylate",
        "rel-(1R)-8-ethyl-8-azabicyclo[3.2.1]octan-3-endo-yl (1P)-2',5'-dimethoxy-6-nitro[1,1'-biphenyl]-2-carboxylate",
        "rac-(1R)-8-ethyl-8-azabicyclo[3.2.1]octan-3-endo-yl (1P)-2',5'-dimethoxy-6-nitro[1,1'-biphenyl]-2-carboxylate",
    ] {
        assert_group_scope_refused(name);
    }
}

#[test]
fn stereogenic_axes_without_cage_descriptors_or_qualifiers_are_unchanged() {
    parse(
        "(1R,2R,4S)-bicyclo[2.2.1]heptan-2-yl (1P)-2',5'-dimethoxy-6-nitro[1,1'-biphenyl]-2-carboxylate",
    );
}

#[test]
fn explicit_cage_groups_with_e_z_double_bonds_are_allowed() {
    for prefix in ["rel", "rac"] {
        for configuration in ["E", "Z"] {
            let name = format!(
                "{prefix}-(1R)-bicyclo[2.2.1]heptan-2-exo-yl (2{configuration})-but-2-enoate"
            );
            let structure = parse(&name);
            assert!(
                structure
                    .graph
                    .bonds
                    .iter()
                    .any(|bond| bond.active && bond.stereo.is_some()),
                "{name}"
            );
        }
    }
}

#[test]
fn racemic_qualifiers_on_e_z_descriptors_cannot_become_relative_cage_groups() {
    for name in [
        "rac-(1E)-1-(bicyclo[2.2.1]heptan-2-exo-yl)-2-fluoroethene",
        "rac-(1Z)-1-(bicyclo[2.2.1]heptan-2-exo-yl)-2-fluoroethene",
        "rac-(E)-1-(bicyclo[2.2.1]heptan-2-exo-yl)-2-fluoroethene",
        "rac-(Z)-1-(bicyclo[2.2.1]heptan-2-exo-yl)-2-fluoroethene",
        "rac-(2E)-4-(bicyclo[2.2.1]heptan-2-exo-yl)but-2-en-1-ol",
        "rac-(2Z)-4-(bicyclo[2.2.1]heptan-2-exo-yl)but-2-en-1-ol",
    ] {
        assert_group_scope_refused(name);
    }
}

#[test]
fn supported_racemic_cage_groups_with_e_z_double_bonds_preserve_their_kind() {
    for name in [
        "(rac)-(E)-1-(bicyclo[2.2.1]heptan-2-exo-yl)-2-fluoroethene",
        "rac-(1R)-2-exo-[(1E)-2-fluoroethenyl]bicyclo[2.2.1]heptane",
    ] {
        let structure = parse(name);
        assert_eq!(
            stereo_group_signature(&structure),
            "Racemic:1,2,4",
            "{name}"
        );
        assert!(
            structure
                .graph
                .bonds
                .iter()
                .any(|bond| bond.active && bond.stereo.is_some()),
            "{name}"
        );
    }
}

#[test]
fn qualifiers_on_non_cip_descriptors_are_retained_for_cage_scope_checks() {
    for unqualified in [
        "(1E)-1-[(1R)-bicyclo[2.2.1]heptan-2-exo-yl]-2-fluoroethene",
        "(3alpha,5alpha)-cholestan-3-yl (1R)-bicyclo[2.2.1]heptane-2-exo-carboxylate",
    ] {
        let structure = parse(unqualified);
        assert!(
            structure
                .graph
                .atoms
                .iter()
                .filter_map(|atom| atom.parity.as_ref())
                .all(|parity| parity.stereo_group.kind == StereoGroupType::Absolute),
            "{unqualified}"
        );
        for prefix in ["rel", "rac"] {
            assert_group_scope_refused(&format!("{prefix}-{unqualified}"));
        }
    }
}

#[test]
fn grouped_stereo_without_cage_descriptors_is_unchanged() {
    for prefix in ["rel", "rac"] {
        parse(&format!(
            "{prefix}-(2R)-2-[(1R,2S,3R,4S)-3-methylbicyclo[2.2.1]heptan-2-yl]propan-1-ol"
        ));
    }
}

#[test]
fn grouped_stereo_outside_a_nested_cage_uses_warning_mode() {
    let mut options = ParseOptions::strict();
    options.warn_rather_than_fail_on_uninterpretable_stereochemistry = true;
    for name in [
        "rel-(2R)-2-[(1R,3R)-3-methylbicyclo[2.2.1]heptan-2-exo-yl]propan-1-ol",
        "rel-(1R,3R)-3-[(1R)-bicyclo[2.2.1]heptan-2-exo-yl]bicyclo[2.2.1]heptan-2-exo-ol",
        "rac-(1R,3R)-3-[(1S)-1-fluoroethyl]bicyclo[2.2.1]heptan-2-exo-ol",
        "(1R,4S*)-bicyclo[2.2.1]heptan-2-exo-ol",
    ] {
        assert_group_scope_refused(name);
        let result = Parser::new().unwrap().parse(name, &options);
        assert_eq!(result.status, Status::Warning, "{name}: {}", result.message);
        assert!(result.structure.is_some(), "{name}");
        assert!(
            result.warnings.iter().any(|warning| {
                warning.kind == WarningKind::StereochemistryIgnored
                    && warning.message.contains(CAGE_GROUP_SCOPE_ERROR)
            }),
            "{name}: {:?}",
            result.warnings
        );
    }
}

#[test]
fn cage_descriptors_follow_their_own_group_before_methanol() {
    let names = [
        "exo-bicyclo[2.2.1]heptan-2-ylmethanol",
        "(2-exo)-bicyclo[2.2.1]heptan-2-ylmethanol",
        "bicyclo[2.2.1]heptan-2-exo-ylmethanol",
    ];
    let expected = canonical(&parse(names[0]).semantic_cxsmiles().unwrap());
    for name in names {
        let written = parse(name).semantic_cxsmiles().unwrap();
        assert_eq!(canonical(&written), expected, "{name}: {written}");
        assert!(written.contains("|o1:"), "{name}: {written}");
    }
}

#[test]
fn locanted_cage_descriptors_cannot_escape_their_parent_scope() {
    for name in [
        "(2-exo)-2-(bicyclo[2.2.1]heptan-2-yl)cyclohexan-1-ol",
        "(7-syn)-2-(bicyclo[2.2.1]heptan-7-yl)cyclohexan-1-ol",
        "(2-exo)-cyclohexan-2-yl bicyclo[2.2.1]heptane-2-carboxylate",
    ] {
        let result = Parser::new().unwrap().parse(name, &ParseOptions::strict());
        assert_eq!(result.status, Status::Failure, "{name}");
        assert!(
            result.message.contains("Endo/exo/syn/anti"),
            "{name}: {}",
            result.message
        );
    }
    for name in [
        "2-(bicyclo[2.2.1]heptan-2-exo-yl)ethan-1-ol",
        "2-[(2-exo)-bicyclo[2.2.1]heptan-2-yl]ethan-1-ol",
    ] {
        let written = parse(name).semantic_cxsmiles().unwrap();
        assert!(written.contains("|o1:"), "{name}: {written}");
    }
}
