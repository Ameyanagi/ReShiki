use super::*;
use crate::graph::{Atom, Bond};

fn ring(size: usize) -> Graph {
    Graph {
        atoms: vec![
            Atom {
                atomic_number: 6,
                ..Default::default()
            };
            size
        ],
        bonds: (0..size)
            .map(|i| Bond {
                a: i,
                b: (i + 1) % size,
                order: if i % 2 == 0 { 1 } else { 2 },
                aromatic: false,
            })
            .collect(),
    }
}

#[test]
fn aromaticity_is_bounded_and_invalid_rings_leave_input_unchanged() -> Result<(), String> {
    let graph = ring(6);
    let before = serde_json::to_value(&graph).map_err(|e| e.to_string())?;
    for rings in [
        vec![vec![]],
        vec![vec![0, 1]],
        vec![vec![0, 1, 2, 2]],
        vec![vec![0, 1, 5]],
        vec![vec![0, 1, usize::MAX]],
    ] {
        assert!(perceive(&graph, &rings).is_err());
    }
    assert!(perceive_with_work(&graph, &[vec![0, 1, 2, 3, 4, 5]], &mut Work::units(0)).is_err());
    assert_eq!(
        serde_json::to_value(&graph).map_err(|e| e.to_string())?,
        before
    );
    Ok(())
}

#[test]
fn large_single_ring_uses_an_iterative_traversal() -> Result<(), String> {
    // 4n+2 electrons; the fused-system size cap must not exclude a single ring.
    let size = 20_002;
    let graph = ring(size);
    let result = perceive(&graph, &[(0..size).collect()])?;
    assert_eq!(result.aromatic_rings, 1);
    assert!(result.graph.atoms.iter().all(|a| a.aromatic));
    assert!(
        result
            .graph
            .bonds
            .iter()
            .all(|b| b.aromatic && b.order == 4)
    );
    Ok(())
}

#[test]
fn hydrogen_adjustment_rejects_invalid_cache_size_and_overflow() -> Result<(), String> {
    let graph = ring(6);
    assert!(adjust_hydrogens(&graph, &[]).is_err());
    let mut previous = graph.provisional_valences()?;
    previous[0].implicit_hydrogens = u32::MAX;
    assert!(adjust_hydrogens(&graph, &previous).is_err());
    assert!(graph.atoms.iter().all(|a| a.explicit_hydrogens == 0));
    Ok(())
}

fn exhaustive_connected(neighbors: &[Vec<usize>], size: usize) -> Vec<Vec<usize>> {
    let mut result = Vec::new();
    let mut choice: Vec<_> = (0..size).collect();
    let mut work = Work::units(usize::MAX);
    loop {
        if connected(&choice, neighbors, &mut work).unwrap() {
            result.push(choice.clone());
        }
        if !next_combination(&mut choice, neighbors.len()).unwrap() {
            break;
        }
    }
    result
}

#[test]
fn streaming_connected_subsets_match_exhaustive_order_for_all_small_graphs() -> Result<(), String> {
    let mut cases = 0;
    for count in 1..=5 {
        let pairs: Vec<_> = (0..count)
            .flat_map(|a| (a + 1..count).map(move |b| (a, b)))
            .collect();
        for mask in 0..(1usize << pairs.len()) {
            let mut neighbors = vec![vec![]; count];
            for (bit, &(a, b)) in pairs.iter().enumerate() {
                if mask & (1 << bit) != 0 {
                    neighbors[a].push(b);
                    neighbors[b].push(a);
                }
            }
            for size in 1..=count {
                let mut actual = Vec::new();
                connected_combinations(
                    &neighbors,
                    size,
                    &mut Work::units(50_000_000),
                    &mut |set, _| {
                        actual.push(set.to_vec());
                        Ok(())
                    },
                )?;
                assert_eq!(
                    actual,
                    exhaustive_connected(&neighbors, size),
                    "nodes={count}, mask={mask}, size={size}"
                );
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 5405);
    Ok(())
}

#[test]
fn large_path_enumeration_is_bounded_and_disconnected_prefixes_are_retained() -> Result<(), String>
{
    let neighbors = vec![vec![2], vec![2], vec![0, 1]];
    let mut result = Vec::new();
    connected_combinations(&neighbors, 3, &mut Work::units(100), &mut |set, _| {
        result.push(set.to_vec());
        Ok(())
    })?;
    assert_eq!(result, [vec![0, 1, 2]]);
    for count in [150usize, 300, 301] {
        let neighbors: Vec<Vec<usize>> = (0..count)
            .map(|id| {
                (0..count)
                    .filter(|&other| id.abs_diff(other) == 1)
                    .collect()
            })
            .collect();
        let mut work = Work::units(5_000_000);
        for size in 1..=if count > 300 { 2 } else { 6 } {
            let mut emitted = 0;
            connected_combinations(&neighbors, size, &mut work, &mut |_, _| {
                emitted += 1;
                Ok(())
            })?;
            assert_eq!(emitted, count - size + 1);
        }
    }
    Ok(())
}

#[test]
fn issue_248_graph_completes_with_the_original_budget_without_mutating_input() -> Result<(), String>
{
    for repeat in [1, 37] {
        let text = format!(
            "C12=CC=CC1{}=CC=C2",
            "=CC=1C2=CC=2C1C=C1C2C=C2C1".repeat(repeat)
        );
        let parsed = crate::smiles::parse(&text).map_err(|e| e.to_string())?;
        let rings = crate::rings::perceive(&parsed.graph, Default::default())
            .map_err(|e| e.to_string())?
            .atoms;
        let original = parsed.graph.clone();
        let mut work = Work::units(50_000_000);
        let result = perceive_with_work(&parsed.graph, &rings, &mut work)?;
        assert_eq!(parsed.graph, original);
        assert_eq!(result.graph.atoms.len(), 8 + repeat * 12);
        if repeat == 37 {
            assert_eq!(rings.len(), 150);
            assert!(50_000_000 - work.remaining < 5_000_000);
            eprintln!("issue248 native work units={}", 50_000_000 - work.remaining);
        }
    }
    Ok(())
}

#[test]
fn deadlines_cancellation_and_dense_work_exhaustion_do_not_publish_partial_chemistry() {
    let graph = ring(6);
    let before = graph.clone();
    let token = Cancellation::default();
    token.cancel();
    let budget = Capabilities {
        memory_headroom_bytes: Some(1024 * 1024 * 1024),
        lookup_operations_per_second: None,
    }
    .resolve();
    assert!(
        perceive_with_budget(&graph, &[vec![0, 1, 2, 3, 4, 5]], budget, token)
            .err()
            .unwrap()
            .contains("cancelled")
    );
    let mut work = Work::units(50_000_000);
    work.deadline = Some(Instant::now());
    assert!(work.check().unwrap_err().contains("time budget"));
    let neighbors: Vec<Vec<usize>> = (0..30)
        .map(|id| (0..30).filter(|&other| other != id).collect())
        .collect();
    assert!(
        connected_combinations(&neighbors, 6, &mut Work::units(10_000), &mut |_, _| Ok(()))
            .unwrap_err()
            .contains("work limit")
    );
    assert_eq!(graph, before);
}

#[test]
fn low_and_high_profiles_produce_the_same_complete_chemistry() -> Result<(), String> {
    let graph = ring(6);
    let rings = [(0..6).collect()];
    let mut outputs = Vec::new();
    for (memory, rate) in [
        (256 * 1024 * 1024, 100_000),
        (8 * 1024 * 1024 * 1024, 50_000_000),
    ] {
        let budget = Capabilities {
            memory_headroom_bytes: Some(memory),
            lookup_operations_per_second: Some(rate),
        }
        .resolve();
        // Other unit tests share the process reservation pool. Low-memory
        // admission may briefly be busy even though this fixture itself fits.
        let mut attempts = 0;
        let result = loop {
            match perceive_with_budget(&graph, &rings, budget, Cancellation::default()) {
                Err(error) if error.contains("resources are busy") && attempts < 100 => {
                    attempts += 1;
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                result => break result?,
            }
        };
        outputs.push((result.graph, result.aromatic_rings));
    }
    assert_eq!(outputs[0], outputs[1]);
    assert!(outputs[0].0.atoms.iter().all(|atom| atom.aromatic));
    assert!(graph.atoms.iter().all(|atom| !atom.aromatic));
    Ok(())
}

#[test]
fn observed_exhausted_memory_denies_perception_without_mutating_input() {
    let graph = ring(6);
    let original = graph.clone();
    let budget = Capabilities {
        memory_headroom_bytes: Some(0),
        lookup_operations_per_second: None,
    }
    .resolve();
    let error = perceive_with_budget(&graph, &[(0..6).collect()], budget, Cancellation::default())
        .err()
        .unwrap();
    assert!(error.contains("Insufficient available memory"));
    assert_eq!(graph, original);
}

#[test]
fn fused_stop_is_checked_only_after_a_complete_combination_size() -> Result<(), String> {
    // Kernel fixture: every cube face has six donated electrons. The first
    // five faces cover every edge, but the final face must still be counted.
    let mut graph = Graph {
        atoms: vec![
            Atom {
                atomic_number: 6,
                ..Default::default()
            };
            8
        ],
        bonds: vec![],
    };
    for a in 0..8 {
        for bit in [1, 2, 4] {
            let b = a ^ bit;
            if a < b {
                graph.bonds.push(Bond {
                    a,
                    b,
                    order: 1,
                    aromatic: false,
                });
            }
        }
    }
    let mut rings = Vec::new();
    for fixed in [1, 2, 4] {
        let varying: Vec<_> = [1, 2, 4].into_iter().filter(|&bit| bit != fixed).collect();
        for base in [0, fixed] {
            rings.push(vec![
                base,
                base ^ varying[0],
                base ^ varying[0] ^ varying[1],
                base ^ varying[1],
            ]);
        }
    }
    let ring_bonds = Topology::new(&graph, &rings, None)?.ring_bonds;
    let ring_refs: Vec<_> = rings.iter().map(Vec::as_slice).collect();
    let bond_refs: Vec<_> = ring_bonds.iter().map(Vec::as_slice).collect();
    let mut work = Work::units(10_000);
    let neighbors = ring_neighbors(&bond_refs, graph.bonds.len(), &mut work)?;
    let donors: Vec<_> = (0u32..8)
        .map(|id| {
            if id.count_ones() % 2 == 0 {
                Donor::Two
            } else {
                Donor::One
            }
        })
        .collect();
    let result = mark_fused(
        &mut graph,
        &ring_refs,
        &bond_refs,
        &neighbors,
        &[4, 1, 5, 2, 0, 3],
        &donors,
        &mut work,
    )?;
    assert_eq!(result, 6);
    assert!(graph.bonds.iter().all(|bond| bond.aromatic));
    Ok(())
}

#[test]
fn actual_fused_model_keeps_six_sizes_at_300_rings_and_two_at_301() -> Result<(), String> {
    let mut spent = Vec::new();
    for count in [300usize, 301] {
        // Ladder of square faces. Vacant donors keep every combination size
        // relevant, so an accidental model-cutoff change cannot hide behind
        // the normal all-bonds-marked early stop.
        let mut graph = Graph {
            atoms: vec![
                Atom {
                    atomic_number: 6,
                    ..Default::default()
                };
                2 * (count + 1)
            ],
            bonds: vec![],
        };
        for i in 0..=count {
            graph.bonds.push(Bond {
                a: 2 * i,
                b: 2 * i + 1,
                order: 1,
                aromatic: false,
            });
            if i < count {
                for row in [0, 1] {
                    graph.bonds.push(Bond {
                        a: 2 * i + row,
                        b: 2 * i + 2 + row,
                        order: 1,
                        aromatic: false,
                    });
                }
            }
        }
        let rings: Vec<_> = (0..count)
            .map(|i| vec![2 * i, 2 * i + 1, 2 * i + 3, 2 * i + 2])
            .collect();
        let ring_bonds = Topology::new(&graph, &rings, None)?.ring_bonds;
        let ring_refs: Vec<_> = rings.iter().map(Vec::as_slice).collect();
        let bond_refs: Vec<_> = ring_bonds.iter().map(Vec::as_slice).collect();
        let mut work = Work::units(10_000_000);
        let neighbors = ring_neighbors(&bond_refs, graph.bonds.len(), &mut work)?;
        let donors = vec![Donor::Vacant; graph.atoms.len()];
        assert_eq!(
            mark_fused(
                &mut graph,
                &ring_refs,
                &bond_refs,
                &neighbors,
                &(0..count).collect::<Vec<_>>(),
                &donors,
                &mut work
            )?,
            0
        );
        assert!(graph.bonds.iter().all(|bond| !bond.aromatic));
        spent.push(10_000_000 - work.remaining);
    }
    assert!(
        spent[0] > 1_000_000,
        "300-ring model skipped higher sizes: {spent:?}"
    );
    assert!(
        spent[1] < 50_000,
        "301-ring model exceeded its two-ring chemistry cutoff: {spent:?}"
    );
    Ok(())
}
