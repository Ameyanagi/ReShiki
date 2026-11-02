//! Rigid cage descriptors must define the bridgeheads and the substituted atom.
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
