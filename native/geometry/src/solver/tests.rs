use super::*;

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
fn retries_require_failure_evidence_and_preserve_successful_ensembles() {
    let mut parameters = EmbedParameters::etkdg_v3();
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
