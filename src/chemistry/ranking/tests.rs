use super::*;
use crate::chemistry::graph::{Atom, Bond};

fn benzene() -> Graph {
    Graph {
        atoms: vec![
            Atom {
                atomic_number: 6,
                aromatic: true,
                ..Default::default()
            };
            6
        ],
        bonds: (0..6)
            .map(|i| Bond {
                a: i,
                b: (i + 1) % 6,
                order: 4,
                aromatic: true,
            })
            .collect(),
    }
}

#[test]
fn symmetry_classes_and_unique_ranks_match_reference() -> Result<(), String> {
    let graph = benzene();
    let metadata = Metadata::unspecified(&graph);
    let rings = vec![vec![0, 1, 2, 3, 4, 5]];
    assert_eq!(
        rank(&graph, &rings, &metadata, Options::default())?,
        [3, 1, 0, 2, 4, 5]
    );
    assert_eq!(
        rank(
            &graph,
            &rings,
            &metadata,
            Options {
                break_ties: false,
                ..Options::default()
            }
        )?,
        [0; 6]
    );
    Ok(())
}

#[test]
fn invalid_metadata_and_work_exhaustion_are_atomic_errors() -> Result<(), String> {
    let graph = benzene();
    let metadata = Metadata::unspecified(&graph);
    let before = serde_json::to_value(&graph).map_err(|e| e.to_string())?;
    let rings = vec![vec![0, 1, 2, 3, 4, 5]];
    assert!(rank_with_work(&graph, &rings, &metadata, Options::default(), Work(0)).is_err());
    assert!(rank(&graph, &rings, &Metadata::default(), Options::default()).is_err());
    for rings in [
        vec![vec![]],
        vec![vec![0, 1, 5]],
        vec![vec![0, 1, usize::MAX]],
        vec![vec![0, 1, 2, 2]],
    ] {
        assert!(rank(&graph, &rings, &metadata, Options::default()).is_err());
    }
    let mut invalid = metadata.clone();
    invalid.atoms[0].chiral_tag = 9;
    assert!(rank(&graph, &rings, &invalid, Options::default()).is_err());
    let mut invalid = metadata.clone();
    invalid.bonds[0].stereo = 4;
    assert!(rank(&graph, &rings, &invalid, Options::default()).is_err());
    invalid.bonds[0].stereo_atoms = vec![usize::MAX, 2];
    assert!(rank(&graph, &rings, &invalid, Options::default()).is_err());
    invalid.bonds[0].stereo_atoms = vec![3, 2];
    assert!(rank(&graph, &rings, &invalid, Options::default()).is_err());
    let mut invalid = metadata.clone();
    invalid.groups.push(StereoGroup {
        kind: 0,
        atoms: vec![usize::MAX],
        ..StereoGroup::default()
    });
    assert!(rank(&graph, &rings, &invalid, Options::default()).is_err());
    assert_eq!(
        serde_json::to_value(&graph).map_err(|e| e.to_string())?,
        before
    );
    Ok(())
}

#[test]
fn large_equal_partition_is_sorted_without_recursion() -> Result<(), String> {
    let n = 20_000;
    let graph = Graph {
        atoms: vec![
            Atom {
                atomic_number: 6,
                ..Default::default()
            };
            n
        ],
        bonds: Vec::new(),
    };
    let ranks = rank(
        &graph,
        &[],
        &Metadata::unspecified(&graph),
        Options::default(),
    )?;
    assert_eq!(ranks, (0..n as u32).collect::<Vec<_>>());
    Ok(())
}
