use super::*;
use crate::chemistry::graph::{Atom, Bond};
type TestResult = Result<(), Box<dyn std::error::Error>>;

fn ring(n: usize) -> Graph {
    Graph {
        atoms: vec![
            Atom {
                atomic_number: 6,
                aromatic: true,
                ..Atom::default()
            };
            n
        ],
        bonds: (0..n)
            .map(|a| Bond {
                a,
                b: (a + 1) % n,
                order: 4,
                aromatic: true,
            })
            .collect(),
    }
}

#[test]
fn large_macrocycle_cleanup_does_not_recurse() -> TestResult {
    let graph = ring(30_000);
    let mut metadata = Metadata::unspecified(&graph);
    for bond in &mut metadata.bonds {
        bond.stereo = 6;
    }
    let result = atropisomers(
        &graph,
        &metadata,
        &vec![Hybridization::Sp2; graph.atoms.len()],
        &[(0..graph.atoms.len()).collect()],
    )?;
    assert!(result.bonds.iter().all(|b| b.stereo == 6));
    Ok(())
}

#[test]
fn malformed_metadata_rings_and_work_exhaustion_return_errors() -> TestResult {
    let graph = ring(3);
    let metadata = Metadata::unspecified(&graph);
    let hybs = [Hybridization::Sp2; 3];
    for kind in 0..10 {
        let mut bad = metadata.clone();
        match kind {
            0 => bad.atoms.clear(),
            1 => bad.atoms[0].chiral_tag = 9,
            2 => bad.bonds[0].stereo = 8,
            3 => bad.bonds[0].stereo_atoms = vec![99],
            4 => bad.groups.push(StereoGroup {
                atoms: vec![99],
                ..StereoGroup::default()
            }),
            5 => bad.groups.push(StereoGroup {
                bonds: vec![99],
                ..StereoGroup::default()
            }),
            6 => bad.groups.push(StereoGroup {
                atoms: vec![0, 0],
                ..StereoGroup::default()
            }),
            7 => bad.groups.push(StereoGroup {
                bonds: vec![0, 0],
                ..StereoGroup::default()
            }),
            8 => {
                bad.groups = vec![
                    StereoGroup {
                        atoms: vec![0],
                        ..StereoGroup::default()
                    };
                    2
                ]
            }
            _ => bad.groups.push(StereoGroup {
                kind: 3,
                atoms: vec![0],
                ..StereoGroup::default()
            }),
        }
        let before = serde_json::to_value(&bad)?;
        assert!(chirality(&graph, &bad, &hybs).is_err());
        assert!(atropisomers(&graph, &bad, &hybs, &[]).is_err());
        assert_eq!(serde_json::to_value(&bad)?, before);
    }
    for rings in [vec![vec![0, 1]], vec![vec![0, 1, 99]], vec![vec![0, 1, 1]]] {
        assert!(atropisomers(&graph, &metadata, &hybs, &rings).is_err());
    }
    assert!(chirality(&graph, &metadata, &[]).is_err());
    assert!(atropisomers(&graph, &metadata, &[], &[]).is_err());
    assert!(
        atropisomers_with_work(&graph, &metadata, &hybs, &[vec![0, 1, 2]], &mut Work(5)).is_err()
    );
    assert!(
        atropisomers_with_work(&graph, &metadata, &hybs, &[vec![0, 1, 2]], &mut Work(6)).is_ok()
    );
    Ok(())
}

#[test]
fn expanding_incident_bonds_cannot_exceed_group_storage() -> TestResult {
    let mut graph = ring(1002);
    graph.bonds = (1..1002)
        .map(|b| Bond {
            a: 0,
            b,
            order: 1,
            aromatic: false,
        })
        .collect();
    let mut metadata = Metadata::unspecified(&graph);
    for bond in &mut metadata.bonds {
        bond.stereo = 6;
    }
    metadata.groups = vec![
        StereoGroup {
            kind: 1,
            atoms: vec![0],
            ..StereoGroup::default()
        };
        2000
    ];
    let mut hybs = vec![Hybridization::Sp2; graph.atoms.len()];
    hybs[1001] = Hybridization::Sp3; // Trigger cleanup while 1,000 incident bonds remain.
    let before = serde_json::to_value(&metadata)?;
    let error = atropisomers(&graph, &metadata, &hybs, &[])
        .err()
        .ok_or("Expected storage error")?;
    assert!(error.contains("group storage exceeded"), "{error}");
    assert_eq!(serde_json::to_value(&metadata)?, before);
    Ok(())
}
