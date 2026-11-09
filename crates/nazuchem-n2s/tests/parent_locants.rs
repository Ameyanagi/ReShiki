// SPDX-License-Identifier: MIT
// Origin: original
// Source: ReShiki | crates/opsin/tests/parent_locants.rs | 89f30ab6d32736a8beccf3bab9c355c68f178f00

use std::collections::BTreeSet;

use nazuchem_n2s::{
    ParseOptions, Parser, Status, Structure,
    graph::{AtomId, Element, Graph},
    smiles,
};

fn parse(name: &str) -> Structure {
    let result = Parser::new().unwrap().parse(name, &ParseOptions::strict());
    assert_eq!(result.status, Status::Success, "{name}: {}", result.message);
    result.structure.unwrap()
}

fn written_structure(structure: &Structure) -> (Graph, Vec<AtomId>, Vec<Option<String>>) {
    let (smiles, locants) = structure.semantic_cxsmiles_with_locants().unwrap();
    assert_eq!(smiles, structure.semantic_cxsmiles().unwrap());
    let mut graph = Graph::default();
    let fragment =
        smiles::build_fragment(&mut graph, smiles.split(" |").next().unwrap(), "", "none").unwrap();
    let atoms = graph.fragment(fragment).atoms.clone();
    assert_eq!(atoms.len(), locants.len());
    (graph, atoms, locants)
}

fn labels(locants: &[Option<String>]) -> BTreeSet<&str> {
    locants.iter().filter_map(Option::as_deref).collect()
}

#[test]
fn indole_locants_follow_written_atoms_and_exclude_chloro_substituent() {
    let structure = parse("5-chloro-1H-indole");
    assert_eq!(structure.principal_parent_atoms.len(), 9);
    let (graph, atoms, locants) = written_structure(&structure);
    assert_eq!(
        labels(&locants),
        BTreeSet::from(["1", "2", "3", "3a", "4", "5", "6", "7", "7a"])
    );
    let chlorine = atoms
        .iter()
        .position(|&atom| graph.atom(atom).element == Element::Cl)
        .unwrap();
    assert_eq!(locants[chlorine], None);
    let attached = graph.neighbours(atoms[chlorine]);
    assert_eq!(attached.len(), 1);
    let carbon = atoms.iter().position(|atom| *atom == attached[0]).unwrap();
    assert_eq!(graph.atom(atoms[carbon]).element, Element::C);
    assert_eq!(locants[carbon].as_deref(), Some("5"));
    let nitrogen = atoms
        .iter()
        .position(|&atom| graph.atom(atom).element == Element::N)
        .unwrap();
    assert_eq!(locants[nitrogen].as_deref(), Some("1"));
}

#[test]
fn naphthalene_fusion_atoms_keep_letter_suffixes() {
    let structure = parse("naphthalene");
    assert_eq!(structure.principal_parent_atoms.len(), 10);
    let (graph, atoms, locants) = written_structure(&structure);
    assert_eq!(
        labels(&locants),
        BTreeSet::from(["1", "2", "3", "4", "4a", "5", "6", "7", "8", "8a"])
    );
    let fusion_labels: BTreeSet<_> = atoms
        .iter()
        .zip(&locants)
        .filter(|(atom, _)| graph.neighbours(**atom).len() == 3)
        .map(|(_, locant)| locant.as_deref().unwrap())
        .collect();
    assert_eq!(fusion_labels, BTreeSet::from(["4a", "8a"]));
}

#[test]
fn ethanol_numbers_parent_carbons_without_suffix_oxygen_or_folded_hydrogens() {
    let structure = parse("ethanol");
    assert_eq!(structure.principal_parent_atoms.len(), 2);
    assert_eq!(structure.graph.fragment(structure.fragment).atoms.len(), 9);
    let (graph, atoms, locants) = written_structure(&structure);
    assert_eq!(locants, [Some("1".into()), Some("2".into()), None]);
    assert_eq!(
        atoms
            .iter()
            .map(|&atom| graph.atom(atom).element)
            .collect::<Vec<_>>(),
        [Element::C, Element::C, Element::O]
    );
}

#[test]
fn anthracene_retains_nine_and_ten_on_the_middle_ring() {
    let structure = parse("anthracene");
    let (graph, atoms, locants) = written_structure(&structure);
    assert_eq!(
        labels(&locants),
        BTreeSet::from([
            "1", "2", "3", "4", "4a", "5", "6", "7", "8", "8a", "9", "9a", "10", "10a"
        ])
    );
    for (locant, expected_neighbours) in [("9", ["8a", "9a"]), ("10", ["4a", "10a"])] {
        let index = locants
            .iter()
            .position(|label| label.as_deref() == Some(locant))
            .unwrap();
        let neighbours: BTreeSet<_> = graph
            .neighbours(atoms[index])
            .into_iter()
            .map(|atom| {
                let index = atoms.iter().position(|&written| written == atom).unwrap();
                locants[index].as_deref().unwrap()
            })
            .collect();
        assert_eq!(neighbours, BTreeSet::from(expected_neighbours));
    }
}

#[test]
fn generated_fused_parent_is_complete_before_the_snapshot() {
    let structure = parse("benzo[b]furan");
    assert_eq!(structure.principal_parent_atoms.len(), 9);
    let (_, _, locants) = written_structure(&structure);
    assert_eq!(
        labels(&locants),
        BTreeSet::from(["1", "2", "3", "3a", "4", "5", "6", "7", "7a"])
    );
}

#[test]
fn substituent_numbering_does_not_leak_into_the_parent() {
    let structure = parse("2-methylpyridine");
    let (graph, atoms, locants) = written_structure(&structure);
    assert_eq!(structure.principal_parent_atoms.len(), 6);
    assert_eq!(
        labels(&locants),
        BTreeSet::from(["1", "2", "3", "4", "5", "6"])
    );
    let methyl = atoms
        .iter()
        .position(|&atom| {
            graph.atom(atom).element == Element::C && graph.neighbours(atom).len() == 1
        })
        .unwrap();
    assert_eq!(locants[methyl], None);
}
