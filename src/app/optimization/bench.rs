//! Opt-in comparison of the production live-relaxation schedule and fixed-20
//! requests, using the packaged geometry worker and identical seeded starts.
//!
//! RESHIKI_GEOMETRY_REFERENCE_APP=/absolute/path/to/reshiki cargo test --release
//! --bin reshiki manual_profile_live_relaxation_batches -- --ignored --nocapture

use super::*;
use reshiki::{
    engine::{LocalEngine, Request},
    geometry::{DEFAULT_SEED, Limits},
};
use serde_json::json;

#[tokio::test]
#[ignore = "manual packaged-worker live-relaxation benchmark; use --release and --nocapture"]
async fn manual_profile_live_relaxation_batches() -> anyhow::Result<()> {
    let executable = std::env::var_os("RESHIKI_GEOMETRY_REFERENCE_APP").ok_or_else(|| {
        anyhow::anyhow!("Set RESHIKI_GEOMETRY_REFERENCE_APP to the packaged executable")
    })?;
    let client = Client::new(executable.into(), Limits::default())?;
    for (name, smiles) in [
        ("aspirin", "CC(=O)Oc1ccccc1C(=O)O"),
        ("propranolol", "CC(C)NCC(O)COc1cccc2ccccc12"),
        (
            "imatinib",
            "Cc1ccc(NC(=O)c2ccc(CN3CCN(C)CC3)cc2)cc1Nc1nccc(-c2cccnc2)n1",
        ),
    ] {
        let doc = LocalEngine::in_process()
            .request(Request::import_smiles(smiles))
            .await
            .map_err(anyhow::Error::msg)?
            .document
            .ok_or_else(|| anyhow::anyhow!("No imported drawing"))?;
        let ids: Vec<_> = doc.atoms.iter().map(|atom| atom.id).collect();
        let prepared = Prepared::new(&doc, &ids)?.without_retained_depth();
        for field in [ForceField::Mmff94s, ForceField::Uff] {
            let generated = client
                .generate_seeded(&prepared, field, DEFAULT_SEED)
                .await?;
            anyhow::ensure!(generated.force_field == field, "Unexpected field fallback");
            let mut target = generated.conformer.positions[0];
            target.x += 0.45;
            target.y += 0.15;
            let pins = vec![Pin {
                atom: 0,
                position: target,
            }];
            for repeat in 0..3 {
                // Alternate order so one schedule does not always get the
                // warmer worker executable or the cooler machine.
                for adaptive in if repeat % 2 == 0 {
                    [false, true]
                } else {
                    [true, false]
                } {
                    let mut relaxation = Relaxation::default();
                    relaxation.update(field, pins.clone());
                    let mut conformer = generated.conformer.clone();
                    let mut schedule = Vec::new();
                    let started = iced::time::Instant::now();
                    let (energy, converged, pause) = loop {
                        let iterations = relaxation.begin_batch(false);
                        assert!(iterations > 0);
                        let batch_started = iced::time::Instant::now();
                        let result = client
                            .relax(&prepared, &conformer, field, &pins, iterations)
                            .await?;
                        let elapsed = batch_started.elapsed();
                        schedule.push(json!({"iterations": iterations, "wall_ms": elapsed.as_secs_f64() * 1000.}));
                        conformer = result.conformer.clone();
                        let pause = relaxation.completed(&result, iterations, elapsed);
                        if result.converged || pause.is_some() {
                            break (result.energy, result.converged, pause);
                        }
                        if !adaptive {
                            relaxation.next_iterations = BATCH_ITERATIONS;
                        }
                    };
                    let elapsed = started.elapsed();
                    // This is the same physical final evaluation used by Apply;
                    // its time is separate from the relaxation comparison.
                    let evaluated = client.evaluate(&prepared, &conformer, field, &pins).await?;
                    let actual = evaluated.conformer.positions[0];
                    assert_eq!(actual, target, "The displaced pin must remain exact");
                    assert!((evaluated.energy - energy).abs() <= 1e-6 + 1e-8 * energy.abs());
                    let gradient = evaluated
                        .gradient
                        .as_ref()
                        .expect("Apply analytic gradient");
                    let max_free_gradient = gradient
                        .iter()
                        .skip(1)
                        .flat_map(|point| [point.x.abs(), point.y.abs(), point.z.abs()])
                        .fold(0., f64::max);
                    println!(
                        "{}",
                        json!({
                            "case": name,
                            "field": field.to_string(),
                            "seed": DEFAULT_SEED,
                            "repeat": repeat,
                            "schedule": if adaptive { "adaptive" } else { "fixed20" },
                            "atoms": conformer.positions.len(),
                            "batches": schedule,
                            "requested_iterations": relaxation.iterations,
                            "wall_ms": elapsed.as_secs_f64() * 1000.,
                            "energy_kcal_mol": energy,
                            "max_free_gradient_kcal_mol_angstrom": max_free_gradient,
                            "converged": converged,
                            "pause": pause.map(RelaxationPause::phase),
                        })
                    );
                }
            }
        }
    }
    Ok(())
}
