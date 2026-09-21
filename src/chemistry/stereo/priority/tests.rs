use super::*;
use crate::chemistry::graph::{Atom, Bond};
type TestResult = Result<(), Box<dyn std::error::Error>>;

fn carbons(n: usize) -> Graph {
    Graph {
        atoms: vec![
            Atom {
                atomic_number: 6,
                no_implicit: true,
                ..Atom::default()
            };
            n
        ],
        bonds: Vec::new(),
    }
}

#[test]
fn empty_graph_and_explicit_zero_maps_are_distinct() -> TestResult {
    let empty = carbons(0);
    assert!(atom_priorities(&empty, &Metadata::unspecified(&empty))?.is_empty());
    let graph = carbons(2);
    let mut meta = Metadata::unspecified(&graph);
    assert_eq!(atom_priorities(&graph, &meta)?, [0, 0]);
    meta.atoms[0].map_present = true;
    assert_eq!(atom_priorities(&graph, &meta)?, [1, 0]);
    meta.atoms[0].map_number = 1023;
    assert_eq!(atom_priorities(&graph, &meta)?, [0, 0]);
    meta.atoms[0].map_number = -1;
    assert_eq!(atom_priorities(&graph, &meta)?, [0, 0]);
    meta.atoms[0].map_number = i32::MAX;
    assert_eq!(atom_priorities(&graph, &meta)?, [0, 0]);
    Ok(())
}

#[test]
fn high_degree_uses_bounded_dynamic_storage() -> TestResult {
    let mut graph = carbons(30_000);
    graph.bonds = (1..graph.atoms.len())
        .map(|b| Bond {
            a: 0,
            b,
            order: 1,
            aromatic: false,
        })
        .collect();
    let ranks = atom_priorities(&graph, &Metadata::unspecified(&graph))?;
    assert_eq!(ranks[0], 1);
    assert!(ranks.iter().skip(1).all(|&v| v == 0));
    Ok(())
}

#[test]
fn invalid_input_and_resource_limits_leave_inputs_unchanged() -> TestResult {
    let graph = carbons(6);
    let meta = Metadata::unspecified(&graph);
    let before = serde_json::to_value((&graph, &meta))?;
    for value in [i32::MIN, -2] {
        let mut bad = meta.clone();
        bad.atoms[0].map_number = value;
        assert!(atom_priorities(&graph, &bad).is_err());
    }
    assert!(priorities(&graph, &meta, None, &mut Work(0)).is_err());
    assert!(priorities(&graph, &meta, None, &mut Work(50)).is_err());
    assert!(atom_priorities_cached(&graph, &meta, Some(&[])).is_err());
    let mut bad = meta.clone();
    bad.atoms.pop();
    assert!(atom_priorities(&graph, &bad).is_err());
    assert_eq!(serde_json::to_value((&graph, &meta))?, before);
    let mut huge = carbons(20_000);
    for atom in &mut huge.atoms {
        atom.explicit_hydrogens = 255;
    }
    let cache = vec![
        Valence {
            explicit_valence: 255,
            implicit_hydrogens: 255
        };
        huge.atoms.len()
    ];
    let error = atom_priorities_cached(&huge, &Metadata::unspecified(&huge), Some(&cache));
    assert!(error.is_err_and(|e| e.contains("storage limit")));
    Ok(())
}

#[test]
fn supplied_hydrogen_cache_controls_tied_substituents() -> TestResult {
    let graph = carbons(2);
    let cache = [
        Valence {
            explicit_valence: 0,
            implicit_hydrogens: 1,
        },
        Valence {
            explicit_valence: 0,
            implicit_hydrogens: 2,
        },
    ];
    assert_eq!(
        atom_priorities_cached(&graph, &Metadata::unspecified(&graph), Some(&cache))?,
        [0, 1]
    );
    Ok(())
}
