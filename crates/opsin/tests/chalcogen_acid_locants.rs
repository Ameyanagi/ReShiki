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
