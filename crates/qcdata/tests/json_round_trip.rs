use reshiki_qcdata::{
    AtomicCharges, BasisSet, Beta, BondOrder, CalcResult, ChargeScheme, Frame, OrbitalSet,
    Orbitals, Provenance, Settings, Shell, SolvationState, Solvent, SolventCorrection,
    SolventModel, Source, Status, Task, Thermo, Trajectory, Vibrations, XtbMethod, XtbSettings,
};

fn assert_compact(expected: &[f64], decoded: &mut [f64], quantized: bool) {
    assert_eq!(decoded.len(), expected.len());
    for (&x, &actual) in expected.iter().zip(decoded.iter()) {
        if quantized {
            assert!((x - actual).abs() <= 1.0 / 32767.0, "{x} -> {actual}");
        } else {
            assert_eq!(actual, f64::from(x as f32));
        }
    }
    // Restore the source precision so the whole remaining result can be compared exactly.
    decoded.copy_from_slice(expected);
}

fn imported() -> (Frame, CalcResult) {
    let frame = Frame {
        numbers: vec![8, 1, 1],
        positions_angstrom: vec![
            [0.0, 0.0, 0.0],
            [0.95725427, 0.0, 0.0],
            [-0.2399872, 0.9266272, 0.0],
        ],
        isotopes: vec![16, 2, 1],
        masses_amu: vec![15.994_914_619_57, 2.014_101_778_12, 1.007_825_032_23],
        charge: 0,
        multiplicity: 1,
        drawing_atoms: vec![Some(91), None, Some(92)],
    };
    let mut input_frame = frame.clone();
    input_frame.positions_angstrom[1][0] += 0.1;
    let trajectory = Trajectory {
        numbers: frame.numbers.clone(),
        isotopes: frame.isotopes.clone(),
        masses_amu: frame.masses_amu.clone(),
        charge: frame.charge,
        multiplicity: frame.multiplicity,
        positions_angstrom: input_frame
            .positions_angstrom
            .iter()
            .chain(&frame.positions_angstrom)
            .flatten()
            .copied()
            .collect(),
        energies_hartree: vec![-75.918_438_688, -76.023_452_819],
        labels: vec!["start".into(), "final".into()],
    };
    let basis = BasisSet {
        shells: [(0, 0, false), (1, 1, false), (0, 2, true), (2, 3, false)]
            .into_iter()
            .map(|(atom, l, pure)| Shell {
                atom,
                l,
                pure,
                exponents_per_bohr2: vec![0.527, 0.733],
                coefficients: vec![0.444_123, -0.223_456],
            })
            .collect(),
    };
    let n_ao = basis.shells.iter().map(Shell::n_functions).sum::<usize>();
    let set = |phase: f64| OrbitalSet {
        energies_hartree: vec![-0.739 * phase, -0.322 * phase, 0.891 * phase, 1.234 * phase],
        occupations: vec![0.97, 0.82, 0.21, 0.0],
        symmetries: vec!["A1".into(), "B2".into(), "A1".into(), "B2".into()],
        rows: vec![0, 1, 2, 3],
        coefficients: (0..4 * n_ao)
            .map(|i| (i as f64 * phase).sin() * 0.331)
            .collect(),
    };
    let displacements = [
        [0.21_f64, -0.13, 0.0, -0.31, 0.0, 0.09, 0.11, -0.8, 0.0345],
        [0.0, 0.82, 0.11, 0.0, 0.53, 0.0, 0.0, 0.25, -0.831],
        [0.63, 0.31, -0.13, -0.608, 0.0, 0.64, 0.0, -0.2, 0.0],
    ];
    let result = CalcResult {
        frame: frame.hash(),
        input_frame: Some(input_frame.hash()),
        task: Task::Optimize,
        settings: Settings::Imported {
            keywords: vec!["opt".into(), "freq".into()],
        },
        provenance: Provenance {
            program: "example".into(),
            program_version: Some("1.0.4".into()),
            method: Some("DFT".into()),
            basis: Some("custom".into()),
            source: Source::Imported {
                format: "log".into(),
                file_name: "water.log".into(),
                file_sha256: Some("f1".repeat(32)),
                job: 2,
            },
        },
        status: Status::NotConverged {
            reason: "iteration limit".into(),
        },
        energy_hartree: Some(-76.023_452_819),
        energy_terms: [
            ("electronic".into(), -84.923_45),
            ("nuclear".into(), 8.900_17),
        ]
        .into(),
        gradient_hartree_per_bohr: (0..9)
            .map(|i| (f64::from(i) - 4.0) * 0.004_152_73)
            .collect(),
        hessian_hartree_per_bohr2: (0..81)
            .map(|i| (f64::from(i) * 0.037_133_38).sin())
            .collect(),
        charges: vec![
            AtomicCharges {
                scheme: ChargeScheme::Mulliken,
                values: vec![-0.86, 0.42, 0.44],
            },
            AtomicCharges {
                scheme: ChargeScheme::Other("ESP".into()),
                values: vec![-0.738, 0.358, 0.38],
            },
        ],
        dipole_e_bohr: Some([0.435, 0.679, 0.0]),
        bond_orders: vec![
            BondOrder {
                a: 0,
                b: 1,
                order: 0.981_273_6,
            },
            BondOrder {
                a: 0,
                b: 2,
                order: 0.977_183_6,
            },
            BondOrder {
                a: 1,
                b: 2,
                order: 0.023_71,
            },
        ],
        orbitals: Some(Orbitals {
            basis: Some(basis),
            core_electrons: vec![2, 0, 0],
            alpha: set(1.0),
            beta: Beta::Unrestricted(set(0.93)),
        }),
        vibrations: Some(Vibrations {
            frequencies_cm1: vec![-12.44, 1642.928, 3699.1728],
            ir_intensities_km_mol: vec![0.01728, 43.162, 122.057],
            modes: displacements
                .into_iter()
                .flat_map(|mode| {
                    let norm = mode.iter().map(|x| x * x).sum::<f64>().sqrt();
                    mode.map(|x| x / norm)
                })
                .collect(),
            reduced_masses_amu: vec![1.033_142_71, 1.213_943, 1.481_283_1],
            masses_amu: frame.masses_amu.clone(),
        }),
        thermo: Some(Thermo {
            temperature_k: 298.15,
            pressure_pa: 101_325.0,
            zero_point_energy_hartree: 0.022_316,
            enthalpy_correction_hartree: 0.025_223,
            entropy_hartree_per_k: 0.000_214,
            gibbs_correction_hartree: -0.038_579,
            heat_capacity_hartree_per_k: Some(0.000_013),
            frequency_scale: 0.97,
            rotor_cutoff_cm1: 100.0,
            imaginary_cutoff_cm1: -25.0,
            symmetry_number: 1,
        }),
        trajectory: Some(trajectory),
        keep_full: true,
        omitted_bytes: Some(12_345),
    };
    (frame, result)
}

#[test]
fn full_imported_result_round_trips() {
    let (frame, original) = imported();
    original.validate(&frame).unwrap();
    let json = serde_json::to_vec_pretty(&original).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&json).unwrap();
    assert!(
        value["hessian_hartree_per_bohr2"]
            .as_str()
            .unwrap()
            .starts_with("f32:")
    );
    assert!(
        value["vibrations"]["modes"]
            .as_str()
            .unwrap()
            .starts_with("q16:")
    );
    assert!(value["orbitals"]["basis"]["shells"][0]["coefficients"].is_array());
    let mut decoded: CalcResult = serde_json::from_slice(&json).unwrap();
    decoded.validate(&frame).unwrap();

    assert_compact(
        &original.gradient_hartree_per_bohr,
        &mut decoded.gradient_hartree_per_bohr,
        false,
    );
    assert_compact(
        &original.hessian_hartree_per_bohr2,
        &mut decoded.hessian_hartree_per_bohr2,
        false,
    );
    assert_compact(
        &original.trajectory.as_ref().unwrap().positions_angstrom,
        &mut decoded.trajectory.as_mut().unwrap().positions_angstrom,
        false,
    );
    let original_modes = original.vibrations.as_ref().unwrap();
    let decoded_modes = decoded.vibrations.as_mut().unwrap();
    assert_compact(&original_modes.modes, &mut decoded_modes.modes, true);
    assert_compact(
        &original_modes.masses_amu,
        &mut decoded_modes.masses_amu,
        false,
    );
    assert_compact(
        &original_modes.reduced_masses_amu,
        &mut decoded_modes.reduced_masses_amu,
        false,
    );
    let original_orbitals = original.orbitals.as_ref().unwrap();
    let decoded_orbitals = decoded.orbitals.as_mut().unwrap();
    assert_compact(
        &original_orbitals.alpha.coefficients,
        &mut decoded_orbitals.alpha.coefficients,
        false,
    );
    let (Beta::Unrestricted(expected), Beta::Unrestricted(actual)) =
        (&original_orbitals.beta, &mut decoded_orbitals.beta)
    else {
        panic!("unrestricted orbitals required")
    };
    assert_compact(&expected.coefficients, &mut actual.coefficients, false);
    assert_eq!(decoded, original);

    let frame_json = serde_json::to_vec_pretty(&frame).unwrap();
    let frame_copy: Frame = serde_json::from_slice(&frame_json).unwrap();
    assert_eq!(frame, frame_copy);
    assert_eq!(frame.hash(), frame_copy.hash());
}

#[test]
fn calculated_result_round_trips_with_solvent_settings() {
    let (frame, original) = imported();
    let mut calculated = original.light(&[]);
    calculated.settings = Settings::Xtb(XtbSettings {
        method: XtbMethod::GXtb,
        solvent: Some(Solvent {
            model: SolventModel::Alpb,
            name: "water".into(),
            state: SolvationState::Bar1mol,
            correction: Some(SolventCorrection::Gfn2),
        }),
        electronic_temperature_k: Some(300.0),
        accuracy: Some(0.1),
        max_iterations: Some(256),
        gfnff: None,
    });
    calculated.provenance.program = "ReShiki".into();
    calculated.provenance.method = Some(XtbMethod::GXtb.label().into());
    calculated.provenance.basis = None;
    calculated.provenance.source = Source::Calculated {
        engine_revision: "v1.2.3".into(),
    };
    calculated.keep_full = false;
    calculated.omitted_bytes = None;
    let value = serde_json::to_value(&calculated).unwrap();
    let decoded: CalcResult = serde_json::from_value(value.clone()).unwrap();
    decoded.validate(&frame).unwrap();
    assert_eq!(decoded, calculated);
    for (parent, key) in [
        (&value, "gradient_hartree_per_bohr"),
        (&value, "hessian_hartree_per_bohr2"),
        (&value, "keep_full"),
        (&value, "omitted_bytes"),
        (&value["orbitals"], "basis"),
        (&value["orbitals"]["alpha"], "rows"),
        (&value["orbitals"]["alpha"], "coefficients"),
        (&value["vibrations"], "modes"),
        (&value["vibrations"], "reduced_masses_amu"),
        (&value["vibrations"], "masses_amu"),
        (&value["trajectory"], "positions_angstrom"),
    ] {
        assert!(parent.get(key).is_none(), "{key} should be omitted");
    }
}

#[test]
fn minimal_result_deserializes_from_only_required_fields() {
    let json = r#"{
        "frame": "b2309becad0c41f063deb724f828b3eb45ecaaaf18da32f90aa0ca0aa2276725",
        "task": "energy",
        "settings": {"xtb": {"method": "gfn2_xtb"}},
        "provenance": {"program": "example", "source": {"calculated": {"engine_revision": "1"}}},
        "status": {"failed": {"reason": "SCF failed"}}
    }"#;
    let result: CalcResult = serde_json::from_str(json).unwrap();
    let frame: Frame = serde_json::from_str(
        r#"{
        "numbers": [8, 1, 1],
        "positions_angstrom": [[0, 0, 0], [0.9572, 0, 0], [-0.2399872, 0.9266272, 0]],
        "charge": 0,
        "multiplicity": 1
    }"#,
    )
    .unwrap();
    result.validate(&frame).unwrap();
    assert_eq!(
        serde_json::to_value(result).unwrap(),
        serde_json::from_str::<serde_json::Value>(json).unwrap()
    );
}
