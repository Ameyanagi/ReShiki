//! Opt-in numerical evidence using the production worker and discovery backend.

use super::*;
use reshiki_optimize::Potential;
use std::{path::PathBuf, sync::mpsc as std_mpsc, thread};

#[test]
#[ignore = "requires RESHIKI_XTB_EXECUTABLE pointing to an xTB-enabled ReShiki worker; about 35 seconds"]
fn manual_captured_claisen_retains_the_unexpected_conformer_and_searches_its_gap() {
    let executable = PathBuf::from(
        std::env::var_os("RESHIKI_XTB_EXECUTABLE")
            .expect("set RESHIKI_XTB_EXECUTABLE to an xTB-enabled ReShiki executable"),
    );
    let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/crates/qcdata/tests/fixtures/claisen-irc.json"
    )))
    .unwrap();
    let band: qcdata::Trajectory = serde_json::from_value(fixture["neb"].clone()).unwrap();
    band.validate().unwrap();
    let frame = band.frame(0).unwrap();
    let width = 3 * band.numbers.len();
    let mut path: Vec<_> = band
        .positions_angstrom
        .chunks_exact(width)
        .zip(&band.energies_hartree)
        .map(|(positions, &energy)| ts::Minimum {
            positions: positions.iter().map(|x| x / ANGSTROM_PER_BOHR).collect(),
            energy,
        })
        .collect();
    let cancel = Cancel::default();
    let watchdog_cancel = cancel.clone();
    let (stop_watchdog, stopped) = std_mpsc::channel::<()>();
    let watchdog = thread::spawn(move || {
        if matches!(
            stopped.recv_timeout(Duration::from_secs(180)),
            Err(std_mpsc::RecvTimeoutError::Timeout)
        ) {
            watchdog_cancel.cancel();
        }
    });
    let limits = Limits {
        compute: Duration::from_secs(30),
        ..Limits::default()
    };
    let (worker, engine, _) = Worker::start(
        executable,
        Open {
            numbers: band.numbers.clone(),
            positions: frame.positions_angstrom.clone(),
            units: Units::Angstrom,
            charge: band.charge,
            uhf: Some(band.multiplicity - 1),
            method: reshiki_xtb_worker::protocol::Method::Gfn2,
            conventions: Conventions::Axtb,
            solvent: None,
            scf: Default::default(),
            gfnff: None,
            threads: 1,
            heap_bytes: limits.heap_bytes,
        },
        limits,
        cancel.clone(),
    )
    .unwrap();
    let mut workers = vec![worker];
    let started = Instant::now();
    for point in &mut path {
        point.energy = workers[0]
            .evaluate(
                &point.positions,
                &mut vec![0.; width],
                reshiki_optimize::Accuracy::Normal,
            )
            .unwrap();
    }
    let options = ts::TsOptions {
        images: path.len(),
        optimize_endpoints: false,
        preserve_endpoint_identity: true,
        ..ts::TsOptions::default()
    };
    let search = ts::Search {
        numbers: &band.numbers,
        masses_amu: &band.masses_amu,
        reactant: &path[0].positions,
        product: &path[path.len() - 1].positions,
        options: &options,
    };
    let (tx, _rx) = mpsc::unbounded();
    let initial = ts::refine_from_band(&mut workers[0], &search, &path, true, |_| {
        control(&cancel, &tx)
    })
    .unwrap();
    let checked = workers[0]
        .checked_hessian(&initial.ts.positions, &band.masses_amu)
        .and_then(|computed| {
            checked_method(computed, qcdata::XtbMethod::Gfn2Xtb)
                .map_err(|error| reshiki_optimize::EvalError::Failed(error.to_string()))
        })
        .unwrap();
    assert!(initial.saddle_converged && initial.connects);
    assert_eq!(initial.imaginary, 1);
    assert_eq!(
        imaginary_count(&checked.vibrations.as_ref().unwrap().frequencies_cm1),
        1
    );
    let mut initial_residuals = Vec::new();
    for (label, minimum) in [
        ("requested reactant", &initial.reactant),
        ("requested product", &initial.product),
        ("IRC +mode end", &initial.irc.as_ref().unwrap()[0].end),
        ("IRC -mode end", &initial.irc.as_ref().unwrap()[1].end),
    ] {
        let spectrum = workers[0]
            .checked_hessian(&minimum.positions, &band.masses_amu)
            .unwrap();
        let mut gradient = vec![0.; width];
        workers[0]
            .evaluate(
                &minimum.positions,
                &mut gradient,
                reshiki_optimize::Accuracy::Normal,
            )
            .unwrap();
        let residual =
            discovery::minimum_residual_rmsd(&band.numbers, minimum, &gradient, &spectrum.hessian)
                .unwrap();
        let measurement = serde_json::json!({
            "label": label,
            "energy_hartree": minimum.energy,
            "gradient_norm": gradient.iter().map(|value| value * value).sum::<f64>().sqrt(),
            "newton_residual_rmsd_bohr": residual,
        });
        eprintln!("initial minimum residual: {measurement}");
        initial_residuals.push(measurement);
    }
    let analysis = Analysis {
        checked: Ok(Arc::new(checked)),
        trace: SearchTrace::default(),
        evaluations: Vec::new(),
        seconds: started.elapsed().as_secs_f64(),
    };
    let mut backend = Native {
        workers: &mut workers,
        frame: &frame,
        numbers: &band.numbers,
        masses: &band.masses_amu,
        method: qcdata::XtbMethod::Gfn2Xtb,
        images: path.len(),
        cancel: &cancel,
        tx: &tx,
        free: false,
        minimum_thermo: Vec::new(),
    };
    let found = discovery::discover(
        &band.numbers,
        (initial, analysis),
        &discovery::Options {
            max_searches: 2,
            ..discovery::Options::default()
        },
        &mut backend,
    )
    .unwrap();
    assert!(
        found.complete(),
        "{:?}; {:?}",
        found.gaps,
        found.diagnostics
    );
    assert!(!found.cancelled && found.error.is_none());
    assert_eq!(found.order, [0, 2, 1]);
    assert_eq!(found.minima.len(), 3);
    assert_eq!(found.edges.len(), 2);
    assert_eq!(found.attempts.len(), 2);

    let mut minima = Vec::new();
    for (id, minimum) in found.minima.iter().enumerate() {
        let checked = backend.workers[0]
            .checked_hessian(&minimum.positions, &band.masses_amu)
            .unwrap();
        let checked = checked_method(checked, qcdata::XtbMethod::Gfn2Xtb).unwrap();
        let frequencies = &checked.vibrations.as_ref().unwrap().frequencies_cm1;
        assert!(frequencies.iter().all(|&frequency| frequency > 0.));
        minima.push(serde_json::json!({
            "id": id,
            "energy_hartree": minimum.energy,
            "positions_bohr": minimum.positions,
            "frequencies_cm1": frequencies,
            "mapped_bonds": ts::bonds(&band.numbers, &minimum.positions).unwrap(),
        }));
    }
    let mut edges = Vec::new();
    for edge in &found.edges {
        let attempt = &found.attempts[edge.attempt];
        let report = &attempt.report;
        let checked = attempt.verification.checked.as_ref().unwrap();
        let frequencies = &checked.vibrations.as_ref().unwrap().frequencies_cm1;
        assert!(report.saddle_converged && report.connects);
        assert_eq!(report.imaginary, 1);
        assert_eq!(imaginary_count(frequencies), 1);
        assert!(report.irc.as_ref().unwrap().iter().all(|b| b.completed()));
        edges.push(serde_json::json!({
            "reactant": edge.reactant,
            "product": edge.product,
            "saddle_energy_hartree": report.ts.energy,
            "positions_bohr": report.ts.positions,
            "worker_frequencies_cm1": frequencies,
            "forward_barrier_kcal_mol": report.forward_barrier_kcal(),
            "reverse_barrier_kcal_mol": report.reverse_barrier_kcal(),
        }));
    }
    assert_eq!(minima[0]["mapped_bonds"], minima[2]["mapped_bonds"]);
    assert_ne!(minima[2]["mapped_bonds"], minima[1]["mapped_bonds"]);
    let evidence = serde_json::json!({
        "fixture": "crates/qcdata/tests/fixtures/claisen-irc.json",
        "method": "GFN2-xTB",
        "conventions": "axtb",
        "engine": engine,
        "numbers": band.numbers,
        "masses_amu": band.masses_amu,
        "route": found.order,
        "complete": found.complete(),
        "initial_minimum_residuals": initial_residuals,
        "minima": minima,
        "edges": edges,
        "elapsed_seconds": started.elapsed().as_secs_f64(),
    });
    let json = serde_json::to_string_pretty(&evidence).unwrap();
    println!("{json}");
    if let Some(destination) = std::env::var_os("RESHIKI_DISCOVERY_EVIDENCE") {
        std::fs::write(destination, json).unwrap();
    }
    drop(stop_watchdog);
    watchdog.join().unwrap();
}
