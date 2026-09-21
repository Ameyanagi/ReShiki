use super::*;
use crate::chemistry::graph::{Atom, Bond};
type TestResult = Result<(), Box<dyn std::error::Error>>;

fn ring(size: usize) -> Graph {
    Graph {
        atoms: vec![
            Atom {
                atomic_number: 6,
                aromatic: true,
                ..Atom::default()
            };
            size
        ],
        bonds: (0..size)
            .map(|a| Bond {
                a,
                b: (a + 1) % size,
                order: 4,
                aromatic: true,
            })
            .collect(),
    }
}

#[test]
fn failure_after_normalization_keeps_input_unchanged() -> TestResult {
    let mut graph = ring(5); // Cannot assign alternating Kekule bonds.
    // This neutral nitro group would be normalized before the later failure.
    graph.atoms.extend([7, 8, 8, 6].map(|atomic_number| Atom {
        atomic_number,
        ..Atom::default()
    }));
    graph
        .bonds
        .extend([(5, 6, 2), (5, 7, 2), (5, 8, 1)].map(|(a, b, order)| Bond {
            a,
            b,
            order,
            aromatic: false,
        }));
    let meta = Metadata::unspecified(&graph);
    let dirs = vec![Direction::None; graph.bonds.len()];
    let before = serde_json::to_value((&graph, &meta, &dirs))?;
    let error = sanitize(&graph, &meta, &dirs).expect_err("Odd aromatic carbon ring must fail");
    assert_eq!(error.stage, Stage::Kekulize);
    assert_eq!(serde_json::to_value((&graph, &meta, &dirs))?, before);
    Ok(())
}

#[test]
fn invalid_input_and_cache_fail_without_panics() -> TestResult {
    let graph = ring(6);
    let mut meta = Metadata::unspecified(&graph);
    let dirs = vec![Direction::None; graph.bonds.len()];
    assert_eq!(
        sanitize(&graph, &meta, &[])
            .expect_err("Missing directions")
            .stage,
        Stage::Input
    );
    meta.atoms[0].chiral_tag = 9;
    assert_eq!(
        sanitize(&graph, &meta, &dirs)
            .expect_err("Invalid chiral tag")
            .stage,
        Stage::Input
    );
    assert!(graph.cached_valences(Some(&[])).is_err());
    let mut cache = graph.valences()?;
    cache[0].explicit_valence = u32::MAX;
    assert!(graph.refresh_implicit(&cache).is_err());
    let mut invalid = graph.clone();
    invalid.bonds[0].a = usize::MAX;
    assert_eq!(
        sanitize(&invalid, &Metadata::unspecified(&invalid), &dirs)
            .expect_err("Invalid endpoint")
            .stage,
        Stage::Input
    );
    Ok(())
}

#[test]
fn long_chain_pipeline_is_iterative() -> TestResult {
    let mut graph = ring(30_000);
    graph.bonds.pop();
    for atom in &mut graph.atoms {
        atom.aromatic = false;
    }
    for bond in &mut graph.bonds {
        bond.order = 1;
        bond.aromatic = false;
    }
    let result = sanitize(
        &graph,
        &Metadata::unspecified(&graph),
        &vec![Direction::None; graph.bonds.len()],
    )?;
    assert_eq!(result.valences[0].implicit_hydrogens, 3);
    assert_eq!(result.valences[1].implicit_hydrogens, 2);
    assert!(result.rings.is_empty());
    assert!(
        result
            .hybridizations
            .iter()
            .all(|&h| h == Hybridization::Sp3)
    );
    Ok(())
}
