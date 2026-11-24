//! Check the conformer pipeline against geometries with known distances and symmetries.

use super::*;

fn pair_topology() -> Topology {
    Topology::new(vec![6, 8], vec![[0, 1]]).unwrap()
}

// Distinct elements fix correspondence; the optimal RMSD is the half-length difference.
fn pair(half_length: f64) -> Vec<[f64; 3]> {
    vec![[-half_length, 0.0, 0.0], [half_length, 0.0, 0.0]]
}

fn start(half_length: f64, kcal: f64) -> Start {
    Start {
        positions: pair(half_length),
        energy_hartree: kcal / crate::KCAL_PER_HARTREE,
    }
}

fn settings() -> Settings {
    Settings {
        diversity: Diversity::Normal,
        per_cluster: PerCluster::Three,
        window_kcal: 100.0,
        max_representatives: 60,
        ..Settings::default()
    }
}

fn near(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "expected {expected}, observed {actual}"
    );
}

#[test]
fn methyl_rotor_hydrogens_do_not_split_ethane() {
    let topology = Topology::new(
        vec![6, 6, 1, 1, 1, 1, 1, 1],
        vec![[0, 1], [0, 2], [0, 3], [0, 4], [1, 5], [1, 6], [1, 7]],
    )
    .unwrap();
    let a = vec![
        [-0.77, 0.0, 0.0],
        [0.77, 0.0, 0.0],
        [-1.13, 1.02, 0.0],
        [-1.13, -0.51, 0.883_345_911_9],
        [-1.13, -0.51, -0.883_345_911_9],
        [1.13, 1.02, 0.0],
        [1.13, -0.51, 0.883_345_911_9],
        [1.13, -0.51, -0.883_345_911_9],
    ];
    let mut b = a.clone();
    let (sin, cos) = (std::f64::consts::PI / 3.0).sin_cos();
    for position in &mut b[5..] {
        let [x, y, z] = *position;
        *position = [x, cos * y - sin * z, sin * y + cos * z];
    }
    near(
        rmsd(&topology, &topology.automorphisms(128), &a, &b).unwrap(),
        0.0,
        1e-10,
    );
    let result = cluster(
        &topology,
        &[
            Start {
                positions: a,
                energy_hartree: -1.0,
            },
            Start {
                positions: b,
                energy_hartree: -1.0,
            },
        ],
        &settings(),
    )
    .unwrap();
    assert_eq!(result.kept, vec![0]);
    assert_eq!(result.clusters, vec![vec![0]]);
    assert_eq!(result.representatives, vec![0]);
}

#[test]
fn anchored_phenyl_flip_requires_a_ring_automorphism() {
    let topology = Topology::new(
        vec![6, 6, 6, 6, 6, 6, 8, 7, 9],
        vec![
            [0, 1],
            [1, 2],
            [2, 3],
            [3, 4],
            [4, 5],
            [5, 0],
            [0, 6],
            [6, 7],
            [7, 8],
        ],
    )
    .unwrap();
    let mut a: Vec<_> = (0..6)
        .map(|i| {
            let angle = i as f64 * std::f64::consts::PI / 3.0;
            [1.4 * angle.cos(), 1.4 * angle.sin(), 0.0]
        })
        .collect();
    // Three non-collinear substituent atoms prevent a rigid rotation from undoing the flip.
    a.extend([[3.0, 1.0, 2.0], [4.0, 3.0, -2.0], [5.0, -2.0, 3.0]]);
    let flip = [0, 5, 4, 3, 2, 1, 6, 7, 8];
    let b: Vec<_> = flip.iter().map(|&i| a[i]).collect();
    let identity = topology.automorphisms(1);
    assert!(rmsd(&topology, &identity, &a, &b).unwrap() > 0.5);
    let autos = topology.automorphisms(16);
    assert!(!autos.truncated());
    near(rmsd(&topology, &autos, &a, &b).unwrap(), 0.0, 1e-9);
}

#[test]
fn truncated_automorphisms_give_symmetric_rmsd() {
    let topology = Topology::new(vec![7, 6, 6, 6], vec![[0, 1], [0, 2], [0, 3]]).unwrap();
    let autos = topology.automorphisms(4);
    assert!(autos.truncated());
    let a = vec![
        [0.0, 0.0, 0.0],
        [1.5, 0.0, 0.0],
        [0.0, 2.0, 0.5],
        [0.25, -0.5, 3.0],
    ];
    // The retained prefix contains this three-cycle and the cap omits its inverse.
    let permutation = [0, 2, 3, 1];
    let mut b = a.clone();
    for (source, &target) in permutation.iter().enumerate() {
        b[target] = a[source];
    }
    let forward = rmsd(&topology, &autos, &a, &b).unwrap();
    let reverse = rmsd(&topology, &autos, &b, &a).unwrap();
    near(forward, 0.0, 1e-9);
    near(reverse, forward, 1e-9);
}

#[test]
fn duplicate_filter_keeps_lower_energy_and_requires_both_close_energy_and_geometry() {
    let topology = pair_topology();
    let duplicate = cluster(
        &topology,
        &[start(1.0, 0.05), start(1.0625, 0.0)],
        &settings(),
    )
    .unwrap();
    assert_eq!(duplicate.kept, vec![1]);
    assert_eq!(duplicate.clusters, vec![vec![1]]);
    assert_eq!(duplicate.representatives, vec![1]);

    for starts in [
        [start(1.0, 0.0), start(1.125, 0.0)],
        [start(1.0, 0.0), start(1.0, 0.1)],
        [start(1.0, 0.0), start(1.0, 0.125)],
    ] {
        let distinct = cluster(&topology, &starts, &settings()).unwrap();
        assert_eq!(distinct.kept.len(), 2);
        assert_eq!(distinct.clusters, vec![vec![0, 1]]);
    }
}

#[test]
fn all_diversity_thresholds_include_the_boundary() {
    let topology = pair_topology();
    for (diversity, threshold) in [
        (Diversity::Fine, 0.5),
        (Diversity::Normal, 0.75),
        (Diversity::Coarse, 1.25),
    ] {
        let config = Settings {
            diversity,
            ..settings()
        };
        let at_boundary = cluster(
            &topology,
            &[start(1.0, 0.0), start(1.0 + threshold, 0.0)],
            &config,
        )
        .unwrap();
        assert_eq!(at_boundary.clusters, vec![vec![0, 1]]);
        let outside = cluster(
            &topology,
            &[start(1.0, 0.0), start(1.0 + threshold + 1.0 / 1024.0, 0.0)],
            &config,
        )
        .unwrap();
        assert_eq!(outside.clusters, vec![vec![0], vec![1]]);
    }
}

#[test]
fn butina_recomputes_remaining_neighbor_counts() {
    let starts: Vec<_> = [1.0, 1.25, 2.0, 2.25, 3.0, 3.25]
        .into_iter()
        .map(|half_length| start(half_length, 0.0))
        .collect();
    let result = cluster(&pair_topology(), &starts, &settings()).unwrap();
    assert_eq!(result.kept, vec![0, 1, 2, 3, 4, 5]);
    assert_eq!(result.clusters, vec![vec![0, 1, 2], vec![3, 4, 5]]);
    assert_eq!(result.representatives, vec![0, 3, 1, 4, 2, 5]);
}

#[test]
fn representative_rounds_honor_per_cluster_global_window_and_cap() {
    let starts: Vec<_> = (0..9)
        .map(|i| {
            start(
                1.0 + 2.0 * (i / 3) as f64 + 0.25 * (i % 3) as f64,
                i as f64 * 0.25,
            )
        })
        .collect();
    let topology = pair_topology();
    let three = cluster(&topology, &starts, &settings()).unwrap();
    assert_eq!(
        three.clusters,
        vec![vec![0, 1, 2], vec![3, 4, 5], vec![6, 7, 8]]
    );
    assert_eq!(three.representatives, vec![0, 3, 6, 1, 4, 7, 2, 5, 8]);
    let one = cluster(
        &topology,
        &starts,
        &Settings {
            per_cluster: PerCluster::One,
            ..settings()
        },
    )
    .unwrap();
    assert_eq!(one.representatives, vec![0, 3, 6]);
    let limited = cluster(
        &topology,
        &starts,
        &Settings {
            max_representatives: 5,
            ..settings()
        },
    )
    .unwrap();
    assert_eq!(limited.representatives, vec![0, 3, 6, 1, 4]);
    let window = cluster(
        &topology,
        &starts,
        &Settings {
            window_kcal: 1.0,
            ..settings()
        },
    )
    .unwrap();
    assert_eq!(window.representatives, vec![0, 3, 1, 4, 2]);
}

#[test]
fn rank_deduplicates_strictly_and_aggregates_onto_the_lower_energy_geometry() {
    let best = vec![[9.0, 2.0, 3.0], [11.0, 2.0, 3.0]];
    let candidates = vec![
        Candidate {
            positions: pair(1.25),
            energy_hartree: -1.0,
            start_energy_hartree: -0.5,
            cluster: 0,
            cluster_size: 3,
        },
        Candidate {
            positions: best.clone(),
            energy_hartree: -2.0,
            start_energy_hartree: -1.5,
            cluster: 1,
            cluster_size: 7,
        },
        Candidate {
            positions: vec![[20.0, -1.5, -4.0], [20.0, 1.5, -4.0]],
            energy_hartree: -1.5,
            start_energy_hartree: -0.75,
            cluster: 2,
            cluster_size: 11,
        },
    ];
    let ranked = rank(
        &pair_topology(),
        candidates,
        &Settings {
            final_dedupe_angstrom: 0.5,
            ..settings()
        },
    )
    .unwrap();
    assert_eq!(
        ranked.len(),
        2,
        "an RMSD exactly at the threshold must survive"
    );
    assert_eq!(ranked[0].positions, best);
    assert_eq!(ranked[0].energy_hartree, -2.0);
    assert_eq!(ranked[0].start_energy_hartree, -1.5);
    assert_eq!(ranked[0].cluster_size, 10);
    assert_eq!(ranked[1].cluster_size, 11);
    near(ranked[0].relative_kcal, 0.0, 1e-12);
    near(
        ranked[1].relative_kcal,
        0.5 * crate::KCAL_PER_HARTREE,
        1e-10,
    );
    near(ranked[0].rmsd_to_best, 0.0, 1e-10);
    near(ranked[1].rmsd_to_best, 0.5, 1e-10);
    for (actual, expected) in ranked[1]
        .positions
        .iter()
        .zip([[8.5, 2.0, 3.0], [11.5, 2.0, 3.0]])
    {
        for axis in 0..3 {
            near(actual[axis], expected[axis], 1e-9);
        }
    }
    near(ranked.iter().map(|item| item.boltzmann).sum(), 1.0, 1e-12);
}

#[test]
fn boltzmann_weights_normalize_rt_log_two_and_large_energy_spreads() {
    let temperature = 298.15;
    let rt_log_two = 0.001_987_204_258_640_83 * temperature * 2.0_f64.ln();
    let weights =
        boltzmann_weights(&[0.0, rt_log_two / crate::KCAL_PER_HARTREE], temperature).unwrap();
    near(weights[0], 2.0 / 3.0, 1e-12);
    near(weights[1], 1.0 / 3.0, 1e-12);
    let weights = boltzmann_weights(&[-1.0e6, 0.0, 1.0e6], temperature).unwrap();
    assert_eq!(weights, vec![1.0, 0.0, 0.0]);
    assert!(boltzmann_weights(&[], temperature).unwrap().is_empty());
}

#[test]
fn three_reoptimized_representatives_count_their_initial_cluster_once() {
    let topology = pair_topology();
    let starts = [start(1.0, 0.0), start(1.25, 0.25), start(1.5, 0.5)];
    let clustered = cluster(&topology, &starts, &settings()).unwrap();
    assert_eq!(clustered.clusters, vec![vec![0, 1, 2]]);
    assert_eq!(clustered.representatives, vec![0, 1, 2]);
    let candidates = clustered
        .representatives
        .iter()
        .map(|&index| Candidate {
            positions: pair(1.0),
            energy_hartree: -1.0,
            start_energy_hartree: starts[index].energy_hartree,
            cluster: 0,
            cluster_size: clustered.clusters[0].len() as u32,
        })
        .collect();
    let ranked = rank(&topology, candidates, &settings()).unwrap();
    assert_eq!(ranked.len(), 1);
    assert_eq!(ranked[0].cluster_size, 3);
}

#[test]
fn initial_cluster_sizes_are_counted_once_for_each_final_geometry() {
    let candidates = [
        (0, 3, 1.0),
        (0, 3, 1.125),
        (0, 3, 2.0),
        (1, 5, 1.0),
        (1, 5, 2.0),
    ]
    .into_iter()
    .map(|(cluster, cluster_size, half_length)| Candidate {
        positions: pair(half_length),
        energy_hartree: -1.0,
        start_energy_hartree: -0.5,
        cluster,
        cluster_size,
    })
    .collect();
    let ranked = rank(&pair_topology(), candidates, &settings()).unwrap();
    assert_eq!(ranked.len(), 2);
    assert_eq!(ranked[0].cluster_size, 8);
    assert_eq!(ranked[1].cluster_size, 8);
}

#[test]
fn boltzmann_weights_reject_invalid_energies_and_temperatures() {
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(boltzmann_weights(&[0.0, invalid], 298.15).is_err());
    }
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 0.0, -1.0] {
        assert!(boltzmann_weights(&[0.0], invalid).is_err());
        assert!(boltzmann_weights(&[], invalid).is_err());
    }
}

#[test]
fn equal_energy_candidates_keep_the_first_input_geometry_and_metadata() {
    let first = pair(1.0);
    let candidates = vec![
        Candidate {
            positions: first.clone(),
            energy_hartree: -1.0,
            start_energy_hartree: -0.5,
            cluster: 0,
            cluster_size: 2,
        },
        Candidate {
            positions: vec![[9.0, 0.0, 0.0], [11.0, 0.0, 0.0]],
            energy_hartree: -1.0,
            start_energy_hartree: -0.75,
            cluster: 1,
            cluster_size: 3,
        },
    ];
    let ranked = rank(&pair_topology(), candidates, &settings()).unwrap();
    assert_eq!(ranked.len(), 1);
    assert_eq!(ranked[0].positions, first);
    assert_eq!(ranked[0].start_energy_hartree, -0.5);
    assert_eq!(ranked[0].cluster_size, 5);
}

#[test]
fn trajectory_saves_the_first_32_conformers_and_reconstructs_valid_frames() {
    let candidates: Vec<_> = (0..40)
        .map(|i| Candidate {
            positions: pair(1.0 + i as f64),
            energy_hartree: -10.0 + i as f64 * 0.001,
            start_energy_hartree: -9.0 + i as f64 * 0.001,
            cluster: i as usize,
            cluster_size: i + 1,
        })
        .collect();
    let ranked = rank(&pair_topology(), candidates, &settings()).unwrap();
    assert_eq!(ranked.len(), 40);
    let saved = trajectory(&ranked, &[6, 8], 0, 1).unwrap();
    saved.validate().unwrap();
    assert_eq!(saved.steps(), 32);
    assert_eq!(saved.energies_hartree.len(), 32);
    assert_eq!(saved.minimizer_energies_hartree.len(), 32);
    assert_eq!(saved.cluster_sizes.len(), 32);
    assert_eq!(
        saved.labels,
        (1..=32).map(|rank| rank.to_string()).collect::<Vec<_>>()
    );
    for (i, conformer) in ranked.iter().take(32).enumerate() {
        let frame = saved.frame(i).unwrap();
        frame.validate().unwrap();
        assert_eq!(frame.positions_angstrom, conformer.positions);
        assert_eq!(frame.numbers, vec![6, 8]);
        assert_eq!(saved.energies_hartree[i], conformer.energy_hartree);
        assert_eq!(
            saved.minimizer_energies_hartree[i],
            conformer.start_energy_hartree
        );
        assert_eq!(saved.cluster_sizes[i], conformer.cluster_size);
    }
    assert!(saved.frame(32).is_none());
}

#[test]
fn threaded_clustering_is_deterministic() {
    let starts: Vec<_> = (0..96)
        .map(|i| start(1.0 + i as f64 * 0.25, i as f64 * 0.25))
        .collect();
    let first = cluster(&pair_topology(), &starts, &settings()).unwrap();
    let second = cluster(&pair_topology(), &starts, &settings()).unwrap();
    assert_eq!(first.kept.len(), 96);
    assert_eq!(first.kept, second.kept);
    assert_eq!(first.clusters, second.clusters);
    assert_eq!(first.representatives, second.representatives);
    assert_eq!(
        first.truncated_automorphisms,
        second.truncated_automorphisms
    );
}

#[test]
fn pipeline_rejects_invalid_settings_coordinates_and_energies() {
    let topology = pair_topology();
    for config in [
        Settings {
            window_kcal: -1.0,
            ..settings()
        },
        Settings {
            window_kcal: f64::NAN,
            ..settings()
        },
        Settings {
            temperature_k: 0.0,
            ..settings()
        },
        Settings {
            max_representatives: 0,
            ..settings()
        },
        Settings {
            max_representatives: 61,
            ..settings()
        },
        Settings {
            final_dedupe_angstrom: f64::NAN,
            ..settings()
        },
    ] {
        assert!(cluster(&topology, &[start(1.0, 0.0)], &config).is_err());
    }
    for invalid in [
        Start {
            positions: vec![[0.0; 3]],
            energy_hartree: 0.0,
        },
        Start {
            positions: vec![[f64::NAN, 0.0, 0.0], [1.0, 0.0, 0.0]],
            energy_hartree: 0.0,
        },
        Start {
            positions: pair(1.0),
            energy_hartree: f64::INFINITY,
        },
    ] {
        assert!(cluster(&topology, &[invalid], &settings()).is_err());
    }
    let invalid_start_energy = Candidate {
        positions: pair(1.0),
        energy_hartree: -1.0,
        start_energy_hartree: f64::NAN,
        cluster: 0,
        cluster_size: 1,
    };
    assert!(rank(&topology, vec![invalid_start_energy], &settings()).is_err());
}

#[test]
fn merged_cluster_sizes_cannot_overflow() {
    let candidates = [u32::MAX, 1]
        .into_iter()
        .enumerate()
        .map(|(cluster, cluster_size)| Candidate {
            positions: pair(1.0),
            energy_hartree: -1.0,
            start_energy_hartree: -0.5,
            cluster,
            cluster_size,
        })
        .collect();
    assert!(rank(&pair_topology(), candidates, &settings()).is_err());
}
