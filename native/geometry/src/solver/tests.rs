use super::*;

#[test]
fn ensemble_threads_reserve_half_the_heap_for_hydrogen_expanded_matrices() {
    let atoms: usize = 500;
    let bytes_per_thread = 2 * (4 * atoms).pow(2) * std::mem::size_of::<f64>();
    let heap_bytes = 256 * 1024 * 1024;
    assert_eq!(bytes_per_thread, 64_000_000);
    assert_eq!(ensemble_threads(8, atoms, heap_bytes), 2);
    assert_eq!(ensemble_threads(8, atoms, 6 * bytes_per_thread), 3);
    assert_eq!(ensemble_threads(8, atoms, 6 * bytes_per_thread - 1), 2);
    assert_eq!(ensemble_threads(1, atoms, heap_bytes), 1);
    // Hydrogen expansion from 200 original atoms to 500 calculation atoms
    // reduces concurrency under the same budget.
    assert_eq!(ensemble_threads(8, 200, heap_bytes), 8);
    assert_eq!(ensemble_threads(8, atoms, 0), 1);
    assert_eq!(ensemble_threads(0, atoms, heap_bytes), 1);
    assert_eq!(ensemble_threads(8, usize::MAX, usize::MAX), 1);
}

#[test]
fn failed_embedding_ids_are_skipped_without_losing_valid_conformers() {
    assert_eq!(
        valid_conformer_ids(&[0, -1, 4, -7]).collect::<Vec<_>>(),
        [0, 4]
    );
    assert_eq!(valid_conformer_ids(&[-1]).count(), 0);
    assert_eq!(valid_conformer_ids(&[]).count(), 0);
}

#[test]
fn pruning_never_searches_molecule_symmetry() {
    // The automorphism search has no deadline (C96H24 outlasted the worker).
    let request: Request = serde_json::from_value(serde_json::json!({
        "atoms": [], "bonds": [], "field": "MMFF94s", "operation": "Generate",
        "coordinates": [], "fixed_atoms": [], "conformers": 8, "seed": 7,
        "max_iterations": 500
    }))
    .unwrap();
    let parameters = embedding_parameters(&request, 1);
    assert!(parameters.prune_rms_thresh > 0.);
    assert!(!parameters.use_symmetry_for_pruning);
}

#[test]
fn retries_require_failure_evidence_and_preserve_successful_ensembles() {
    let mut parameters = EmbedParams::etkdg_v3();
    parameters.failures = vec![0; EmbedFailureCause::EndOfEnum as usize];
    parameters.failures[EmbedFailureCause::ExceededTimeout as usize] = 1;
    assert_eq!(
        embedding_retry(&parameters, &[-1], 8),
        Some(EmbeddingRetry::SingleConformer)
    );
    assert_eq!(embedding_retry(&parameters, &[-1], 1), None);
    assert_eq!(embedding_retry(&parameters, &[0, -1], 8), None);
    parameters.failures[EmbedFailureCause::InitialCoords as usize] = 13;
    parameters.failures[EmbedFailureCause::EtkMinimization as usize] = 87;
    assert_eq!(
        embedding_retry(&parameters, &[], 8),
        Some(EmbeddingRetry::WithoutBasicKnowledge)
    );
    assert_eq!(
        embedding_retry(&parameters, &[], 1),
        Some(EmbeddingRetry::WithoutBasicKnowledge)
    );
    assert_eq!(embedding_retry(&parameters, &[0], 8), None);
    parameters.use_basic_knowledge = false;
    assert_eq!(
        embedding_retry(&parameters, &[-1], 8),
        Some(EmbeddingRetry::SingleConformer)
    );
    assert_eq!(embedding_retry(&parameters, &[], 1), None);
    parameters.use_basic_knowledge = true;
    parameters.failures.fill(0);
    parameters.failures[EmbedFailureCause::InitialCoords as usize] = 50;
    parameters.failures[EmbedFailureCause::EtkMinimization as usize] = 50;
    assert_eq!(embedding_retry(&parameters, &[], 8), None);
    parameters.failures[EmbedFailureCause::EtkMinimization as usize] = 51;
    assert_eq!(
        embedding_retry(&parameters, &[], 8),
        Some(EmbeddingRetry::WithoutBasicKnowledge)
    );
    // Eigenvector starts that fail outright switch to random coordinates once.
    parameters.failures.fill(0);
    parameters.failures[EmbedFailureCause::InitialCoords as usize] = 800;
    assert_eq!(
        embedding_retry(&parameters, &[], 8),
        Some(EmbeddingRetry::RandomCoordinates)
    );
    parameters.use_random_coords = true;
    assert_eq!(embedding_retry(&parameters, &[], 8), None);
    parameters.failures[EmbedFailureCause::ExceededTimeout as usize] = 1;
    assert_eq!(
        embedding_retry(&parameters, &[-1], 8),
        Some(EmbeddingRetry::SingleConformer)
    );
    parameters.use_random_coords = false;
    parameters.failures.clear();
    assert_eq!(embedding_retry(&parameters, &[], 8), None);
}

#[test]
fn seed_dimensionality_is_intrinsic_and_requires_physical_thickness() {
    let spatial = [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
    assert!(has_three_dimensional_extent(&spatial));
    let rotated = spatial.map(|[x, y, z]| {
        let u = (x + z) / 2_f64.sqrt();
        let v = (z - x) / 2_f64.sqrt();
        [
            u + 100.,
            (y + v) / 2_f64.sqrt() - 20.,
            (v - y) / 2_f64.sqrt() + 8.,
        ]
    });
    assert!(has_three_dimensional_extent(&rotated));
    let tilted_plane = [[0., 0., 0.], [1., 0., 1.], [0., 1., 1.], [1., 1., 2.]];
    assert!(!has_three_dimensional_extent(&tilted_plane));
    let almost_flat = [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [1., 1., 1e-5]];
    assert!(!has_three_dimensional_extent(&almost_flat));
    assert!(!has_three_dimensional_extent(&[[0.; 3]; 4]));
    assert!(!has_three_dimensional_extent(&spatial[..3]));
    let dimensionless = spatial.map(|p| p.map(|v| v * 1e-3));
    assert!(!has_three_dimensional_extent(&dimensionless));
    let nonfinite = [[f64::NAN, 0., 0.]; 4];
    assert!(!has_three_dimensional_extent(&nonfinite));
}
