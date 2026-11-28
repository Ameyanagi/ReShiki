use super::*;
use serde_json::json;
use std::path::Path;

#[test]
fn optimization_stops_at_the_first_step_when_the_deadline_has_passed() {
    struct Quadratic;

    impl Potential for Quadratic {
        fn evaluate(
            &mut self,
            positions: &[f64],
            gradient: &mut [f64],
            _accuracy: Accuracy,
        ) -> Result<f64, EvalError> {
            gradient.copy_from_slice(positions);
            Ok(positions.iter().map(|x| x * x).sum::<f64>() / 2.0)
        }
    }

    let deadline = Instant::now() - std::time::Duration::from_secs(1);
    let mut steps = Vec::new();
    let outcome = optimize(
        &mut Quadratic,
        &[1, 1],
        &[0.0, 0.0, 0.0, 1.4, 0.0, 0.0],
        &Options::default(),
        |step| {
            steps.push(step.index);
            assert_eq!(
                optimization_step(step, None, None),
                ControlFlow::Continue(())
            );
            optimization_step(step, Some(deadline), None)
        },
    );
    assert_eq!(steps, [0]);
    assert_eq!(outcome.status, Status::Cancelled);
    assert_eq!(outcome.evaluations, 1);
}

#[test]
fn optimized_xyz_has_the_method_comment_and_eight_decimals() {
    let text = xyz(
        &[8, 1],
        &[[0.0, 1.234567891, -2.0], [-0.125, 0.0, 3.0]],
        XtbMethod::Gfn2Xtb,
    )
    .unwrap();
    assert_eq!(
        text,
        format!(
            "2\nOptimized with GFN2-xTB (ReShiki {CURRENT_VERSION})\n\
             O 0.00000000 1.23456789 -2.00000000\nH -0.12500000 0.00000000 3.00000000\n"
        )
    );
}

#[test]
fn calculated_value_rounds_properties_and_adds_thermochemical_corrections() {
    let frame = crate::qc_input::parse_xyz("2\nHydrogen\nH 0 0 0\nH 0 0 0.74\n").unwrap();
    let computed = serde_json::from_value(json!({
        "method": "gfn2",
        "conventions": "axtb",
        "engine_conventions": "axtb",
        "numerics": "f64",
        "settings": {"charge": 0, "uhf": 0, "solvent": null, "scf": {}, "threads": 1},
        "energy": -1.25,
        "energy_terms": {},
        "homo_lumo_gap_ev": 0.05,
        "near_degenerate": true,
        "dipole_e_bohr": [3.0, 4.0, 0.0],
        "charges": {"scheme": "mulliken", "values": [0.123456, -0.123456]},
        "vibrations": {
            "conventions": "axtb",
            "frequencies_cm1": [-12.3456],
            "ir_intensities_km_mol": [0.15678],
            "reduced_masses_amu": [1.0],
            "force_constants_mdyn_per_angstrom": [0.1],
            "masses_amu": [1.0, 1.0],
            "linear": true
        },
        "thermo": {
            "temperature_k": 298.15,
            "pressure_pa": 101_325.0,
            "zero_point_energy_hartree": 0.015625,
            "enthalpy_correction_hartree": 0.03125,
            "entropy_hartree_per_k": 0.0,
            "gibbs_correction_hartree": -0.0625,
            "frequency_scale": 1.0,
            "rotor_cutoff_cm1": 0.0,
            "imaginary_cutoff_cm1": 0.0,
            "symmetry_number": 2
        },
        "arrays": [],
        "seconds": 0.003
    }))
    .unwrap();
    let engine = AgentCalculations.engine();
    let result = build_value(
        CalcTask::OptFreq,
        &frame,
        &computed,
        protocol::Engine {
            name: engine.name,
            version: engine.version,
            license: engine.license,
            source: engine.source,
            rev: engine.rev,
        },
        Some((
            Optimization {
                converged: false,
                steps: 500,
                rmsd_angstrom: 0.0,
                xyz: String::new(),
            },
            None,
        )),
        0.25,
    );
    assert_eq!(result.value.method, "gfn2");
    assert_eq!(result.value.engine.rev, AXTB_REV);
    assert_eq!(result.value.atoms, 2);
    assert_eq!(result.value.energy_hartree, -1.25);
    assert_eq!(result.value.homo_lumo_gap_ev, Some(0.05));
    assert_eq!(result.value.dipole_debye, Some(5.0 * DEBYE_PER_E_BOHR));
    assert_eq!(
        result.value.charges.as_deref(),
        Some([0.1235, -0.1235].as_slice())
    );
    assert_eq!(result.value.seconds, 0.25);
    let frequencies = result.value.frequencies.unwrap();
    assert_eq!(frequencies.frequencies_cm1, [-12.35]);
    assert_eq!(frequencies.ir_intensities_km_mol, [0.16]);
    assert_eq!(frequencies.imaginary, 1);
    assert_eq!(frequencies.temperature_k, Some(298.15));
    assert_eq!(frequencies.zero_point_energy_hartree, Some(0.015625));
    assert_eq!(frequencies.enthalpy_hartree, Some(-1.21875));
    assert_eq!(frequencies.gibbs_hartree, Some(-1.3125));
    assert_eq!(
        result.warnings,
        [
            "The optimization did not converge in 500 steps.",
            XTB_NEAR_DEGENERATE,
            "1 imaginary frequencies: the geometry is not a minimum.",
        ]
    );
}

#[test]
fn converged_cli_optimization_warns_when_metal_hydride_bridging_changes() {
    let frame = crate::qc_input::parse_xyz(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/xtb/diruthenium_tetrahydride_seed.xyz"
    )))
    .unwrap();
    let output = crate::qc_input::parse_xyz(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/xtb/diruthenium_tetrahydride_gfn2.xyz"
    )))
    .unwrap();
    let outcome = Outcome {
        status: Status::Converged,
        positions: output
            .positions_angstrom
            .as_flattened()
            .iter()
            .map(|coordinate| coordinate / ANGSTROM_PER_BOHR)
            .collect(),
        energy: -10.,
        gradient: vec![0.; frame.numbers.len() * 3],
        steps: 25,
        evaluations: 26,
        hessian_evaluations: 0,
        method: Default::default(),
        methods: vec![],
        energies: vec![-9., -10.],
    };
    let optimized = optimization(&frame, XtbMethod::Gfn2Xtb, &outcome).unwrap();
    let computed = reshiki_xtb_jobs::run::example_computed(-10., 2.);
    let calculated = build_value(
        CalcTask::Optimize,
        &frame,
        &computed,
        reshiki_xtb_jobs::run::example_engine(),
        Some(optimized),
        0.25,
    );
    assert!(calculated.value.optimization.unwrap().converged);
    assert_eq!(calculated.value.energy_hartree, -10.);
    assert_eq!(calculated.warnings.len(), 1);
    assert!(calculated.warnings[0].contains("2 hydrogens lost a bridging metal contact"));
}

#[test]
fn dropping_the_future_signals_worker_cancellation() {
    let cancel = Cancel::default();
    let (_tx, rx) = oneshot::channel();
    drop(CancelOnDrop(cancel.clone()).receive(rx));

    // Cancellation is checked before path validation. A relative path can never spawn a worker.
    assert!(matches!(
        Session::start(Path::new("worker"), Limits::default(), &cancel),
        Err(Error::Cancelled)
    ));
}
