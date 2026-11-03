//! Rigid cage descriptors must define the bridgeheads and the substituted atom.
use opsin::graph::StereoGroupType;
use opsin::{ParseOptions, Parser, Status, Structure};
use reshiki_chemistry::smiles;

fn parse(name: &str) -> Structure {
    let result = Parser::new().unwrap().parse(name, &ParseOptions::strict());
    assert_eq!(result.status, Status::Success, "{name}: {}", result.message);
    result.structure.unwrap()
}
fn canonical(text: &str) -> String {
    let imported = smiles::read(text).unwrap();
    smiles::write::write(&imported.prepared.state, Default::default())
        .unwrap()
        .text
}
fn mirror(text: &str) -> String {
    text.replace("@@", "!").replace('@', "@@").replace('!', "@")
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
            let imported = smiles::read(&written).unwrap();
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
fn cage_groups_include_all_descriptors_and_remain_independent() {
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
    let structure = parse(
        "2-(bicyclo[2.2.1]heptan-2-exo-yl)-3-[rel-(2-exo)-bicyclo[2.2.1]heptan-2-yl]propan-1-ol",
    );
    let written = structure.semantic_cxsmiles().unwrap();
    let imported = smiles::read(&written).unwrap();
    let groups = &imported.prepared.state.metadata.groups;
    assert_eq!(groups.len(), 2, "{written}");
    assert!(
        groups
            .iter()
            .all(|group| group.kind == 1 && group.atoms.len() == 3),
        "{written}"
    );
}

#[test]
fn separately_scoped_cage_groups_are_independent() {
    for (prefix, kind, extensions) in [("rel", 1, ["o1:", "o2:"]), ("rac", 2, ["&1:", "&2:"])] {
        let name = format!(
            "2-[{prefix}-(2-exo)-bicyclo[2.2.1]heptan-2-yl]-3-[{prefix}-(2-exo)-bicyclo[2.2.1]heptan-2-yl]propan-1-ol"
        );
        let written = parse(&name).semantic_cxsmiles().unwrap();
        let imported = smiles::read(&written).unwrap();
        let groups = &imported.prepared.state.metadata.groups;
        assert_eq!(groups.len(), 2, "{name}: {written}");
        assert!(
            groups
                .iter()
                .all(|group| group.kind == kind && group.atoms.len() == 3),
            "{name}: {written}"
        );
        for extension in extensions {
            assert!(written.contains(extension), "{name}: {written}");
        }
    }
}

#[test]
fn separately_scoped_cip_descriptors_keep_cage_groups_independent() {
    for (prefix, kind, extension) in [
        ("rel", 1, "|o1:0,1,3,o2:11,12,15|"),
        ("rac", 2, "|&1:0,1,3,&2:11,12,15|"),
    ] {
        let suffix_name = format!(
            "2-[{prefix}-(1R)-bicyclo[2.2.1]heptan-2-exo-yl]-3-[{prefix}-(1R)-bicyclo[2.2.1]heptan-2-exo-yl]propan-1-ol"
        );
        let combined_name = format!(
            "2-[{prefix}-(1R,2-exo)-bicyclo[2.2.1]heptan-2-yl]-3-[{prefix}-(1R,2-exo)-bicyclo[2.2.1]heptan-2-yl]propan-1-ol"
        );
        let written = parse(&suffix_name).semantic_cxsmiles().unwrap();
        assert!(written.ends_with(extension), "{suffix_name}: {written}");
        assert_eq!(
            written,
            parse(&combined_name).semantic_cxsmiles().unwrap(),
            "{suffix_name} differs from {combined_name}"
        );
        let imported = smiles::read(&written).unwrap();
        let groups = &imported.prepared.state.metadata.groups;
        assert_eq!(groups.len(), 2, "{suffix_name}: {written}");
        assert!(
            groups
                .iter()
                .all(|group| group.kind == kind && group.atoms.len() == 3),
            "{suffix_name}: {written}"
        );
    }
}

#[test]
fn cage_groups_remain_independent_of_stereo_centres_outside_cages() {
    for (prefix, kind) in [("rel", 1), ("rac", 2)] {
        let name = format!(
            "{prefix}-(2R)-2-[{prefix}-(1R)-bicyclo[2.2.1]heptan-2-exo-yl]-3-[{prefix}-(1R)-bicyclo[2.2.1]heptan-2-exo-yl]propan-1-ol"
        );
        let written = parse(&name).semantic_cxsmiles().unwrap();
        let imported = smiles::read(&written).unwrap();
        let groups = &imported.prepared.state.metadata.groups;
        assert_eq!(groups.len(), 3, "{name}: {written}");
        assert!(
            groups.iter().all(|group| group.kind == kind),
            "{name}: {written}"
        );
        let mut group_sizes: Vec<_> = groups.iter().map(|group| group.atoms.len()).collect();
        group_sizes.sort();
        assert_eq!(group_sizes, [1, 3, 3], "{name}: {written}");
    }
}

#[test]
fn any_absolute_cip_centre_anchors_the_entire_cage() {
    for name in [
        "(1R,4S*)-bicyclo[2.2.1]heptan-2-exo-ol",
        "(1R*,4S)-bicyclo[2.2.1]heptan-2-exo-ol",
    ] {
        let structure = parse(name);
        let written = structure.semantic_cxsmiles().unwrap();
        assert!(
            structure
                .graph
                .atoms
                .iter()
                .filter_map(|atom| atom.parity.as_ref())
                .all(|parity| parity.stereo_group.kind == StereoGroupType::Absolute),
            "{name}: {written}"
        );
        assert!(!written.contains('|'), "{name}: {written}");
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
