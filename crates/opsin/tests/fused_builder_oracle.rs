//! Pinned FusedRingBuilder XML and graph snapshots, with Java-free execution.
use opsin::{
    ParseOptions, build_state::BuildState, fused_ring_builder::process_fused_rings,
    parse_tree::Arena,
};
use serde_json::{Value, json};

#[test]
fn exact_resolved_fragment_fusion_stage_matches_pinned_opsin() {
    let mut failures = Vec::new();
    let mut cases = 0;
    for line in include_str!("fixtures/fused-building-golden.jsonl").lines() {
        let expected: Value = serde_json::from_str(line).unwrap();
        let document = roxmltree::Document::parse(expected["input_xml"].as_str().unwrap()).unwrap();
        let input_root = document.root_element();
        let mut arena = Arena::default();
        let word = arena.grouping("word");
        let root = arena.grouping(input_root.tag_name().name());
        arena.add_child(word, root);
        for child in input_root.children().filter(roxmltree::Node::is_element) {
            let value: String = child.children().filter_map(|node| node.text()).collect();
            let token = arena.token(child.tag_name().name(), value);
            for attribute in child.attributes() {
                arena[token].add_attribute(attribute.name(), attribute.value());
            }
            arena.add_child(root, token);
        }
        let mut state = BuildState::new(ParseOptions::strict());
        let mut parent = None;
        for group in arena.children_named(root, "group") {
            let smiles = arena[group].attribute("value").unwrap().to_owned();
            let fragment = state
                .fragment_manager
                .build_token_smiles(&smiles, &mut arena, group, "numeric")
                .unwrap();
            arena[group].fragment = Some(fragment);
            state.xml_suffix_map.insert(group, Vec::new());
            parent = Some(group);
        }
        let before = arena.to_xml(root);
        let actual = match process_fused_rings(&mut state, &mut arena, root) {
            Ok(()) => {
                let fragment = arena[parent.unwrap()].fragment.unwrap();
                let graph = state.graph();
                let atoms = &graph.fragment(fragment).atoms;
                let atoms_json: Vec<_> = atoms.iter().map(|&id| { let atom = graph.atom(id); json!({"element": atom.element.symbol(), "locants": atom.locants, "spare_valency": atom.spare_valency}) }).collect();
                let bonds_json: Vec<_> = graph.fragment(fragment).bonds.iter().map(|&id| { let bond = graph.bond(id); json!({"from": atoms.iter().position(|&a| a == bond.from).unwrap(), "to": atoms.iter().position(|&a| a == bond.to).unwrap(), "order": bond.order}) }).collect();
                json!({"before": before, "after": arena.to_xml(root), "error": null, "graph": {"atoms": atoms_json, "bonds": bonds_json}})
            }
            Err(error) => {
                json!({"before": before, "after": null, "error": error.to_string(), "graph": null})
            }
        };
        for field in ["before", "after", "error", "graph"] {
            if actual[field] != expected[field] {
                failures.push(format!(
                    "{} {field}: expected {}, got {}",
                    expected["name"], expected[field], actual[field]
                ));
            }
        }
        cases += 1;
    }
    assert_eq!(cases, 11);
    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
