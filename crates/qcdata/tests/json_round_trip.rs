use reshiki_qcdata::{
    AtomicCharges, BasisSet, Beta, BondOrder, CalcResult, ChargeScheme, Convergence, Endpoints,
    Error, Frame, MAX_RUN_EVALUATIONS, MAX_SAVED_CONFORMERS, MappedAtom, OrbitalSet, Orbitals,
    Provenance, RunSummary, ScfRetry, Settings, Shell, SolvationState, Solvent, SolventCorrection,
    SolventModel, Source, Spin, Status, Task, Thermo, Trajectory, TransitionStateContext,
    TransitionStateMapping, TransitionStateRun, Vibrations, XtbConventions, XtbMethod, XtbSettings,
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
        minimizer_energies_hartree: vec![],
        cluster_sizes: vec![],
        rmsd_to_best_angstrom: vec![],
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
        endpoints: None,
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
            point_group: None,
        }),
        trajectory: Some(trajectory),
        keep_full: true,
        omitted_bytes: Some(12_345),
        run: None,
    };
    (frame, result)
}

#[test]
fn full_imported_result_round_trips() {
    let (frame, original) = imported();
    original.validate(&frame).unwrap();
    let json = serde_json::to_vec_pretty(&original).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&json).unwrap();
    assert!(value.get("endpoints").is_none());
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
        conventions: XtbConventions::Xtb68,
        solvent: Some(Solvent {
            model: SolventModel::Gbsa,
            name: "water".into(),
            state: SolvationState::Bar1mol,
            correction: Some(SolventCorrection::Gfn2),
        }),
        electronic_temperature_k: Some(300.0),
        accuracy: Some(0.1),
        max_iterations: Some(256),
        scf_retries: vec![ScfRetry {
            electronic_temperature_k: Some(1000.0),
            max_iterations: None,
        }],
        gfnff: None,
        threads: Some(4),
        convergence: Some(Convergence::Tight),
        fixed_atoms: vec![0, 2],
    });
    calculated.provenance.program = "ReShiki".into();
    calculated.provenance.method = Some(XtbMethod::GXtb.label().into());
    calculated.provenance.basis = None;
    calculated.provenance.source = Source::Calculated {
        engine_revision: "v1.2.3".into(),
    };
    calculated.keep_full = false;
    calculated.omitted_bytes = None;
    calculated.run = Some(RunSummary {
        seconds: 12.5,
        scf_iterations: vec![7, 5, 4],
        finished: Some("2026-11-26T05:03:00Z".into()),
        transition_state: None,
    });
    calculated.validate(&frame).unwrap();
    let value = serde_json::to_value(&calculated).unwrap();
    let xtb = &value["settings"]["xtb"];
    assert_eq!(xtb["conventions"], "xtb-6.8");
    assert_eq!(xtb["threads"], 4);
    assert_eq!(xtb["convergence"], "tight");
    assert_eq!(xtb["fixed_atoms"], serde_json::json!([0, 2]));
    assert_eq!(xtb["solvent"]["model"], "gbsa");
    assert_eq!(
        xtb["scf_retries"],
        serde_json::json!([{ "electronic_temperature_k": 1000.0 }])
    );
    let mut decoded: CalcResult = serde_json::from_value(value.clone()).unwrap();
    decoded.validate(&frame).unwrap();
    let vibrations = calculated.vibrations.as_ref().unwrap();
    let decoded_vibrations = decoded.vibrations.as_mut().unwrap();
    assert_compact(
        &vibrations.reduced_masses_amu,
        &mut decoded_vibrations.reduced_masses_amu,
        false,
    );
    assert_compact(
        &vibrations.masses_amu,
        &mut decoded_vibrations.masses_amu,
        false,
    );
    assert_eq!(decoded, calculated);
    let light = calculated.for_file(0, &[]);
    light.validate(&frame).unwrap();
    assert_eq!(light.settings, calculated.settings);
    assert_eq!(light.run, calculated.run);
    let saved: CalcResult =
        serde_json::from_slice(&serde_json::to_vec_pretty(&light).unwrap()).unwrap();
    saved.validate(&frame).unwrap();
    assert_eq!(saved.settings, calculated.settings);
    assert_eq!(saved.run, calculated.run);
    for (parent, key) in [
        (&value, "gradient_hartree_per_bohr"),
        (&value, "hessian_hartree_per_bohr2"),
        (&value, "keep_full"),
        (&value, "omitted_bytes"),
        (&value["orbitals"], "basis"),
        (&value["orbitals"]["alpha"], "rows"),
        (&value["orbitals"]["alpha"], "coefficients"),
        (&value["vibrations"], "modes"),
        (&value["trajectory"], "positions_angstrom"),
    ] {
        assert!(parent.get(key).is_none(), "{key} should be omitted");
    }
}

#[test]
fn thermochemistry_point_group_is_optional_and_preserves_the_engine_label() {
    let (_, mut result) = imported();
    let value = serde_json::to_value(&result).unwrap();
    assert!(value["thermo"].get("point_group").is_none());
    let decoded: CalcResult = serde_json::from_value(value).unwrap();
    assert_eq!(decoded.thermo.unwrap().point_group, None);

    result.thermo.as_mut().unwrap().point_group = Some("Cinfv".into());
    let value = serde_json::to_value(&result).unwrap();
    assert_eq!(value["thermo"]["point_group"], "Cinfv");
    let decoded: CalcResult = serde_json::from_value(value).unwrap();
    assert_eq!(decoded.thermo, result.thermo);
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
    let Settings::Xtb(settings) = &result.settings else {
        panic!("expected xTB settings");
    };
    assert_eq!(settings.threads, None);
    assert_eq!(settings.convergence, None);
    assert!(settings.fixed_atoms.is_empty());
    assert_eq!(result.run, None);
    assert_eq!(
        serde_json::to_value(result).unwrap(),
        serde_json::from_str::<serde_json::Value>(json).unwrap()
    );
}

#[test]
fn calculated_result_rejects_invalid_run_settings_and_summary() {
    let (frame, mut result) = imported();
    result.settings = Settings::Xtb(XtbSettings {
        method: XtbMethod::Gfn2Xtb,
        conventions: Default::default(),
        solvent: None,
        electronic_temperature_k: None,
        accuracy: None,
        max_iterations: None,
        scf_retries: vec![],
        gfnff: None,
        threads: Some(4),
        convergence: Some(Convergence::Tight),
        fixed_atoms: vec![0, 2],
    });
    result.provenance.source = Source::Calculated {
        engine_revision: "fixture".into(),
    };
    result.validate(&frame).unwrap();
    for fixed_atoms in [vec![2, 1], vec![0, 0], vec![frame.numbers.len() as u32]] {
        let Settings::Xtb(settings) = &mut result.settings else {
            unreachable!();
        };
        settings.fixed_atoms = fixed_atoms;
        assert!(result.validate(&frame).is_err());
    }
    let Settings::Xtb(settings) = &mut result.settings else {
        unreachable!();
    };
    settings.fixed_atoms.clear();
    settings.threads = Some(0);
    assert!(result.validate(&frame).is_err());
    let Settings::Xtb(settings) = &mut result.settings else {
        unreachable!();
    };
    settings.threads = Some(4);
    for seconds in [f64::NAN, f64::INFINITY, -1.0] {
        result.run = Some(RunSummary {
            seconds,
            scf_iterations: vec![],
            finished: None,
            transition_state: None,
        });
        assert!(result.validate(&frame).is_err());
    }
    result.run = Some(RunSummary {
        seconds: 0.0,
        scf_iterations: vec![0; MAX_RUN_EVALUATIONS + 1],
        finished: None,
        transition_state: None,
    });
    assert!(matches!(
        result.validate(&frame),
        Err(Error::TooLarge { .. })
    ));
}

#[test]
fn trajectory_matches_result_frame_metadata_and_last_step() {
    let (mut frame, mut result) = imported();
    frame.isotopes.clear();
    result.frame = frame.hash();
    let path = result.trajectory.as_mut().unwrap();
    path.isotopes.fill(0);
    let last = path.positions_angstrom.len() - 1;
    result.validate(&frame).unwrap();

    let path = result.trajectory.as_ref().unwrap();
    let mut wrong_masses = path.clone();
    wrong_masses.masses_amu[0] += 0.01;
    let mut wrong_geometry = path.clone();
    wrong_geometry.positions_angstrom[last] += 1e-4;
    for invalid in [
        Trajectory {
            isotopes: vec![16, 0, 0],
            ..path.clone()
        },
        wrong_masses,
        wrong_geometry,
    ] {
        let mut result = result.clone();
        result.trajectory = Some(invalid);
        assert!(result.validate(&frame).is_err());
    }
}

#[test]
fn full_optimization_validates_after_f32_reload() {
    let (mut frame, mut result) = imported();
    frame.positions_angstrom[2][2] = 65_536.003;
    result.frame = frame.hash();
    let path = result.trajectory.as_mut().unwrap();
    let last = path.positions_angstrom.len() - 1;
    path.positions_angstrom[last] = frame.positions_angstrom[2][2];
    result.validate(&frame).unwrap();

    let saved = result.for_file(0, &[]);
    let json = serde_json::to_vec_pretty(&saved).unwrap();
    let decoded: CalcResult = serde_json::from_slice(&json).unwrap();
    assert_eq!(
        decoded.trajectory.as_ref().unwrap().positions_angstrom[last],
        65_536.0
    );
    decoded.validate(&frame).unwrap();
}

#[test]
fn transition_state_endpoints_are_required_and_exclusive() {
    let (frame, mut result) = imported();
    result.task = Task::TransitionState;
    assert!(result.validate(&frame).is_err());

    result.endpoints = Some(Endpoints {
        reactant: result.input_frame.unwrap(),
        product: result.frame,
    });
    result.validate(&frame).unwrap();
    result.task = Task::Optimize;
    assert!(result.validate(&frame).is_err());
}

#[test]
fn transition_state_path_round_trips_and_light_drops_positions() {
    let (frame, mut result) = imported();
    result.task = Task::TransitionState;
    result.input_frame = None;
    let path = result.trajectory.as_mut().unwrap();
    *path.positions_angstrom.last_mut().unwrap() += 0.2;
    let endpoints = Endpoints {
        reactant: path.frame(0).unwrap().hash(),
        product: path.frame(1).unwrap().hash(),
    };
    result.endpoints = Some(endpoints);
    result.validate(&frame).unwrap();

    let json = serde_json::to_vec_pretty(&result).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&json).unwrap();
    assert_eq!(value["task"], "transition_state");
    assert_eq!(
        value["endpoints"]["reactant"],
        endpoints.reactant.to_string()
    );
    assert_eq!(value["endpoints"]["product"], endpoints.product.to_string());
    let decoded: CalcResult = serde_json::from_slice(&json).unwrap();
    decoded.validate(&frame).unwrap();
    assert_eq!(decoded.endpoints, Some(endpoints));
    assert_eq!(serde_json::to_vec_pretty(&decoded).unwrap(), json);

    let light = result.light(&[]);
    light.validate(&frame).unwrap();
    assert_eq!(light.endpoints, Some(endpoints));
    let full_path = result.trajectory.as_ref().unwrap();
    let light_path = light.trajectory.unwrap();
    assert!(light_path.positions_angstrom.is_empty());
    assert_eq!(light_path.energies_hartree, full_path.energies_hartree);
}

#[test]
fn transition_state_context_survives_light_save_and_old_metadata_loads() {
    fn details(result: &mut CalcResult) -> &mut TransitionStateRun {
        result
            .run
            .as_mut()
            .unwrap()
            .transition_state
            .as_mut()
            .unwrap()
    }

    let (frame, mut result) = imported();
    result.task = Task::TransitionState;
    result.endpoints = Some(Endpoints {
        reactant: result.input_frame.unwrap(),
        product: result.frame,
    });
    result.run = Some(RunSummary {
        seconds: 42.5,
        scf_iterations: vec![6, 8],
        finished: Some("2026-11-26T05:03:00Z".into()),
        transition_state: Some(Box::new(TransitionStateRun {
            arrow: Some(123),
            context: None,
            saddle_converged: true,
            imaginary: 1,
            imaginary_cm1: Some(-721.3),
            climbing_image: 1,
            mapping: TransitionStateMapping {
                formed: vec![[
                    MappedAtom {
                        element: "O".into(),
                        number: 1,
                    },
                    MappedAtom {
                        element: "H".into(),
                        number: 2,
                    },
                ]],
                broken: vec![],
                hydrogen_moves: vec![],
                automatic: false,
            },
            diagnostics: vec![("IRC".into(), "Connects both endpoints".into())],
        })),
    });
    result.validate(&frame).unwrap();
    let baseline_heap = result.heap_bytes();
    let context = TransitionStateContext {
        reactant_chemistry: "ab".repeat(32),
        product_chemistry: "cd".repeat(32),
        title: "Reactant → product".into(),
        reactant: "Reactant".into(),
    };
    let added_heap = context.reactant_chemistry.len()
        + context.product_chemistry.len()
        + context.title.len()
        + context.reactant.len();
    details(&mut result).context = Some(context.clone());
    result.validate(&frame).unwrap();
    assert_eq!(result.heap_bytes() - baseline_heap, added_heap);

    let light = result.light(&[]);
    let json = serde_json::to_vec_pretty(&light).unwrap();
    let decoded: CalcResult = serde_json::from_slice(&json).unwrap();
    decoded.validate(&frame).unwrap();
    assert_eq!(decoded.run, result.run);
    assert!(decoded.trajectory.unwrap().positions_angstrom.is_empty());

    let mut old: serde_json::Value = serde_json::from_slice(&json).unwrap();
    let search = old["run"]["transition_state"].as_object_mut().unwrap();
    search.remove("context");
    let decoded: CalcResult = serde_json::from_value(old.clone()).unwrap();
    decoded.validate(&frame).unwrap();
    let search = decoded.run.unwrap().transition_state.unwrap();
    assert_eq!(search.arrow, Some(123));
    assert_eq!(search.context, None);

    old["run"]["transition_state"]
        .as_object_mut()
        .unwrap()
        .remove("arrow");
    let mut unbound: CalcResult = serde_json::from_value(old).unwrap();
    unbound.validate(&frame).unwrap();
    assert!(details(&mut unbound).arrow.is_none());
    assert!(
        serde_json::to_value(&unbound).unwrap()["run"]["transition_state"]
            .get("arrow")
            .is_none()
    );
    details(&mut unbound).arrow = Some(0);
    assert!(unbound.validate(&frame).is_err());

    // Three maximum-size drawing captions fit in a reaction title.
    details(&mut result).context = Some(TransitionStateContext {
        title: "t".repeat(3 * 64 * 1024),
        reactant: "r".repeat(64 * 1024),
        ..context.clone()
    });
    result.validate(&frame).unwrap();

    for invalid in [
        TransitionStateContext {
            reactant_chemistry: "A".repeat(64),
            ..context.clone()
        },
        TransitionStateContext {
            product_chemistry: "f".repeat(63),
            ..context.clone()
        },
        TransitionStateContext {
            product_chemistry: "g".repeat(64),
            ..context.clone()
        },
        TransitionStateContext {
            title: "t".repeat(256 * 1024 + 1),
            ..context.clone()
        },
        TransitionStateContext {
            reactant: "r".repeat(256 * 1024 + 1),
            ..context
        },
    ] {
        details(&mut result).context = Some(invalid);
        assert!(result.validate(&frame).is_err());
    }
}

#[test]
fn conformers_match_first_step_and_keep_trajectory_in_light_results() {
    let (frame, mut result) = imported();
    result.task = Task::Conformers;
    assert!(result.validate(&frame).is_err());

    let path = result.trajectory.as_mut().unwrap();
    path.positions_angstrom.rotate_left(frame.numbers.len() * 3);
    path.energies_hartree.reverse();
    path.labels.reverse();
    path.minimizer_energies_hartree = vec![-0.03, -0.02];
    path.cluster_sizes = vec![3, 2];
    result.validate(&frame).unwrap();

    let light = result.light(&[]);
    light.validate(&frame).unwrap();
    assert_eq!(light.trajectory, result.trajectory);
    assert_eq!(serde_json::to_value(&light).unwrap()["task"], "conformers");
}

#[test]
fn conformer_result_limits_saved_steps() {
    let (frame, mut result) = imported();
    result.task = Task::Conformers;
    let positions: Vec<_> = frame.positions_angstrom.iter().flatten().copied().collect();
    let path = result.trajectory.as_mut().unwrap();
    path.energies_hartree.clear();
    path.labels.clear();
    path.positions_angstrom = positions.repeat(MAX_SAVED_CONFORMERS);
    result.validate(&frame).unwrap();
    result
        .trajectory
        .as_mut()
        .unwrap()
        .positions_angstrom
        .extend(positions);

    assert!(matches!(
        result.validate(&frame),
        Err(Error::TooLarge {
            count,
            max: MAX_SAVED_CONFORMERS,
            ..
        }) if count == MAX_SAVED_CONFORMERS + 1
    ));
}

#[test]
fn orbital_validation_checks_frame_spin_electrons_and_nonempty_basis() {
    let (frame, mut result) = imported();
    result.trajectory = None;
    result.validate(&frame).unwrap();
    let valid = result.orbitals.take().unwrap();
    for (frame, orbitals) in [
        (
            Frame {
                multiplicity: 3,
                ..frame.clone()
            },
            Orbitals {
                beta: Beta::Restricted,
                ..valid.clone()
            },
        ),
        (
            Frame {
                charge: 8,
                ..frame.clone()
            },
            Orbitals {
                core_electrons: vec![2, 1, 0],
                ..valid.clone()
            },
        ),
        (
            frame,
            Orbitals {
                basis: Some(BasisSet { shells: vec![] }),
                alpha: OrbitalSet {
                    coefficients: vec![],
                    ..valid.alpha.clone()
                },
                beta: Beta::Restricted,
                ..valid
            },
        ),
    ] {
        result.frame = frame.hash();
        result.orbitals = Some(orbitals);
        assert!(result.validate(&frame).is_err());
    }
}

#[test]
fn xtb_settings_require_calculated_provenance_with_revision() {
    let (frame, mut result) = imported();
    result.settings = Settings::Xtb(XtbSettings {
        method: XtbMethod::Gfn2Xtb,
        conventions: XtbConventions::Axtb,
        solvent: None,
        electronic_temperature_k: None,
        accuracy: None,
        max_iterations: None,
        scf_retries: vec![],
        gfnff: None,
        threads: None,
        convergence: Default::default(),
        fixed_atoms: vec![],
    });
    assert!(result.validate(&frame).is_err());
    result.provenance.source = Source::Calculated {
        engine_revision: String::new(),
    };
    assert!(result.validate(&frame).is_err());
    result.provenance.source = Source::Calculated {
        engine_revision: "v1.2.3".into(),
    };
    result.validate(&frame).unwrap();
    let value = serde_json::to_value(&result).unwrap();
    assert!(value["settings"]["xtb"].get("conventions").is_none());
    assert!(value["settings"]["xtb"].get("scf_retries").is_none());
    let decoded: CalcResult = serde_json::from_value(value).unwrap();
    assert_eq!(decoded.settings, result.settings);
}

#[test]
fn heavy_bytes_measures_pretty_json_difference() {
    let (frame, original) = imported();
    let light = original.light(&[(Spin::Alpha, 0)]);
    light.validate(&frame).unwrap();
    let with_basis = serde_json::to_vec_pretty(&light).unwrap().len();
    let without_basis = serde_json::to_vec_pretty(&light.light(&[])).unwrap().len();
    assert!(with_basis > without_basis);
    assert_eq!(light.heavy_bytes(), with_basis - without_basis);
}

#[test]
fn imported_result_with_only_a_pinned_row_leaves_omitted_bytes_unset() {
    let (frame, original) = imported();
    let pins = [(Spin::Alpha, 0)];
    let mut result = original.light(&pins);
    result.keep_full = false;
    result.omitted_bytes = None;
    result.validate(&frame).unwrap();
    assert!(result.heavy_bytes() > 0);

    let saved = result.for_file(0, &pins);
    assert_eq!(*saved, result);
    let json = serde_json::to_value(&saved).unwrap();
    assert_eq!(json["orbitals"]["alpha"]["rows"], serde_json::json!([0]));
    assert!(json["orbitals"]["basis"].is_object());
    assert!(json.get("omitted_bytes").is_none());
}

#[test]
fn imported_pinned_row_keeps_omitted_bytes_across_saves() {
    let (frame, mut result) = imported();
    result.keep_full = false;
    result.validate(&frame).unwrap();
    let pins = [(Spin::Alpha, 0)];
    let saved = result.for_file(0, &pins);
    assert_eq!(saved.orbitals.as_ref().unwrap().alpha.rows, [0]);
    let json = serde_json::to_vec_pretty(&saved).unwrap();
    let loaded: CalcResult = serde_json::from_slice(&json).unwrap();
    loaded.validate(&frame).unwrap();
    assert_eq!(loaded.for_file(0, &pins).omitted_bytes, saved.omitted_bytes);
    assert_eq!(
        saved.omitted_bytes,
        Some(result.omitted_bytes.unwrap() + result.heavy_bytes() - saved.heavy_bytes())
    );
}

#[test]
fn thermo_checks_gibbs_correction_temperature_and_pressure() {
    let (frame, mut result) = imported();
    result.validate(&frame).unwrap();
    let valid = result.thermo.take().unwrap();
    for invalid in [
        Thermo {
            gibbs_correction_hartree: valid.gibbs_correction_hartree + 2e-5,
            ..valid.clone()
        },
        Thermo {
            temperature_k: 0.0,
            gibbs_correction_hartree: valid.enthalpy_correction_hartree,
            ..valid.clone()
        },
        Thermo {
            pressure_pa: 0.0,
            ..valid
        },
    ] {
        result.thermo = Some(invalid);
        assert!(result.validate(&frame).is_err());
    }
}

#[test]
fn only_path_tasks_carry_a_trajectory() {
    let (frame, mut result) = imported();
    result.validate(&frame).unwrap();
    for task in [Task::Energy, Task::Gradient, Task::Hessian] {
        result.task = task;
        assert!(result.validate(&frame).is_err());
    }
}

#[test]
fn imported_save_never_drops_data_without_shrinking_the_file() {
    let (frame, original) = imported();
    let rows = original.orbitals.as_ref().unwrap().alpha.rows.clone();
    let both = [(Spin::Alpha, rows[0]), (Spin::Alpha, rows[1])];
    let mut result = original.light(&both);
    result.keep_full = false;
    result.omitted_bytes = None;
    // Zero rows compress to almost nothing, so dropping one saves less
    // than the omission counter would add.
    result
        .orbitals
        .as_mut()
        .unwrap()
        .alpha
        .coefficients
        .fill(0.0);
    result.validate(&frame).unwrap();
    let saved = result.for_file(0, &[(Spin::Alpha, rows[0])]);
    assert_eq!(*saved, result);
}
