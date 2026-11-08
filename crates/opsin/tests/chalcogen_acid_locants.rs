// SPDX-License-Identifier: MIT
// Origin: adapted from OPSIN; source paths and pins follow.
//   OPSIN 3.0-SNAPSHOT, opsin-core/src/main/java/uk/ac/cam/ch/wwmm/opsin/SuffixApplier.java @ 42e87ac6efb1bd9f578986f5ea7ee1faa8e93c91.
//   OPSIN 3.0-SNAPSHOT, opsin-core/src/main/java/uk/ac/cam/ch/wwmm/opsin/FunctionalReplacement.java @ 42e87ac6efb1bd9f578986f5ea7ee1faa8e93c91.
//   OPSIN 3.0-SNAPSHOT, opsin-core/src/test/java/uk/ac/cam/ch/wwmm/opsin/ChalcogenAcidLocantTest.java @ 42e87ac6efb1bd9f578986f5ea7ee1faa8e93c91.
//   OPSIN 3.0-SNAPSHOT, opsin-core/src/test/resources/uk/ac/cam/ch/wwmm/opsin/chalcogenAcidLocants.txt @ 42e87ac6efb1bd9f578986f5ea7ee1faa8e93c91.
//   ReShiki, crates/opsin/tests/chalcogen_acid_locants.rs @ 5c3d90c2852227ad644ac07ad91300f032c697f5.
// Copyright 2017 Daniel Lowe
// Changes: Rust graph/tree construction and regression harness around source-derived cases/assertions; local regressions retained.

//! Upstream ChalcogenAcidLocantTest: acidic hydrogens must stay on the specified elements.
use opsin::{ParseOptions, Parser, Status, graph::Element};

#[test]
fn terminal_chalcogens_match_acid_locants() {
    let parser = Parser::new().unwrap();
    let mut count = 0;
    for line in include_str!("fixtures/chalcogenAcidLocants.txt").lines() {
        let (name, expected) = line.split_once('\t').unwrap();
        let result = parser.parse(name, &ParseOptions::strict());
        assert_eq!(result.status, Status::Success, "{name}: {}", result.message);
        let structure = result.structure.unwrap();
        let graph = &structure.graph;
        let mut terminal = Vec::new();
        for &atom in &graph.fragment(structure.fragment).atoms {
            let element = graph.atom(atom).element;
            if !matches!(element, Element::O | Element::S | Element::Se | Element::Te) {
                continue;
            }
            let heavy_neighbours: Vec<_> = graph
                .neighbours(atom)
                .into_iter()
                .filter(|&neighbour| graph.atom(neighbour).element != Element::H)
                .collect();
            if heavy_neighbours.len() == 1 {
                let bond = graph.bond(graph.bond_between(atom, heavy_neighbours[0]).unwrap());
                terminal.push(if bond.order == 2 {
                    format!("={element}")
                } else {
                    format!("{element}H")
                });
            }
        }
        terminal.sort();
        assert_eq!(terminal.join(" "), expected, "{name}");
        count += 1;
    }
    assert_eq!(count, 26);
}
