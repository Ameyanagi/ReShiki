use reshiki_qcdata::{
    ANGSTROM_PER_BOHR, CalcResult, Endpoints, Frame, IrcEndpoint, IrcPath, IrcRelaxedEnd,
    Provenance, RunSummary, Settings, Source, Status, Task, Trajectory, TransitionStateMapping,
    TransitionStateRun, XtbMethod, XtbSettings,
};

fn evidence() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/claisen-irc.json")).unwrap()
}

fn fixture() -> (Frame, CalcResult) {
    let value = evidence();
    let mut trajectory: Trajectory = serde_json::from_value(value["irc"].clone()).unwrap();
    let neb: Trajectory = serde_json::from_value(value["neb"].clone()).unwrap();
    let ts_index = value["ts_index"].as_u64().unwrap() as usize;
    let climbing = value["climbing_index"].as_u64().unwrap() as usize;
    let frame = trajectory.frame(ts_index).unwrap();
    let endpoints = Endpoints {
        reactant: neb.frame(0).unwrap().hash(),
        product: neb.frame(neb.steps() - 1).unwrap().hash(),
    };
    let last = trajectory.steps() - 1;
    let width = trajectory.numbers.len() * 3;
    let relaxed = [0, last].map(|i| IrcRelaxedEnd {
        energy_hartree: trajectory.energies_hartree[i],
        positions_angstrom: trajectory.positions_angstrom[i * width..(i + 1) * width].to_vec(),
    });
    trajectory.energies_hartree = trajectory.energies_hartree[1..last].to_vec();
    trajectory.positions_angstrom = trajectory.positions_angstrom[width..last * width].to_vec();
    let mut irc = IrcPath::new(
        trajectory,
        ts_index - 1,
        [IrcEndpoint::Reactant, IrcEndpoint::Product],
        serde_json::from_value(value["completed"].clone()).unwrap(),
        relaxed,
    )
    .unwrap();
    irc.project_neb(&neb, climbing).unwrap();
    let endpoint_energies = [
        neb.energies_hartree[0],
        *neb.energies_hartree.last().unwrap(),
    ];
    let result = CalcResult {
        frame: frame.hash(),
        input_frame: None,
        task: Task::TransitionState,
        endpoints: Some(endpoints),
        settings: Settings::Xtb(XtbSettings {
            method: XtbMethod::Gfn2Xtb,
            conventions: Default::default(),
            solvent: None,
            electronic_temperature_k: None,
            accuracy: None,
            max_iterations: None,
            scf_retries: vec![],
            gfnff: None,
            threads: Some(1),
            convergence: None,
            fixed_atoms: vec![],
        }),
        provenance: Provenance {
            program: "Claisen verification fixture".into(),
            program_version: None,
            method: Some("GFN2-xTB".into()),
            basis: None,
            source: Source::Calculated {
                engine_revision: "9d0388c9e9d81c43c0e7bba01c0ccf706a88fc91".into(),
            },
        },
        status: Status::Converged,
        energy_hartree: Some(irc.trajectory.energies_hartree[irc.ts_index]),
        energy_terms: Default::default(),
        gradient_hartree_per_bohr: vec![],
        hessian_hartree_per_bohr2: vec![],
        charges: vec![],
        dipole_e_bohr: None,
        bond_orders: vec![],
        orbitals: None,
        vibrations: None,
        thermo: None,
        trajectory: Some(neb),
        keep_full: true,
        omitted_bytes: None,
        run: Some(RunSummary {
            seconds: 0.,
            scf_iterations: vec![],
            finished: None,
            search: None,
            transition_state: Some(Box::new(TransitionStateRun {
                method: Default::default(),
                refined_from: None,
                irc: Some(irc),
                arrow: None,
                context: None,
                free_energy: false,
                endpoint_energies_hartree: Some(endpoint_energies),
                endpoint_thermo: None,
                reached_positions_angstrom: vec![],
                forming_bonds: vec![],
                breaking_bonds: vec![],
                saddle_converged: true,
                imaginary: 1,
                imaginary_cm1: Some(-386.55),
                climbing_image: climbing,
                mapping: TransitionStateMapping {
                    formed: vec![],
                    broken: vec![],
                    hydrogen_moves: vec![],
                    automatic: false,
                },
                diagnostics: vec![],
            })),
        }),
    };
    result.validate(&frame).unwrap();
    (frame, result)
}

fn details(result: &mut CalcResult) -> &mut TransitionStateRun {
    result
        .run
        .as_mut()
        .unwrap()
        .transition_state
        .as_mut()
        .unwrap()
}

#[test]
fn full_and_light_irc_round_trip_and_older_files_remain_readable() {
    let (frame, mut result) = fixture();
    let full_json = serde_json::to_vec_pretty(&result.for_file(0, &[])).unwrap();
    let full: CalcResult = serde_json::from_slice(&full_json).unwrap();
    full.validate(&frame).unwrap();
    let full_path = full
        .run
        .as_ref()
        .unwrap()
        .transition_state
        .as_ref()
        .unwrap()
        .irc
        .as_ref()
        .unwrap();
    assert!(full_path.has_frames());
    assert_eq!(full_path.steps(), 121);
    assert_eq!(full_path.ts_index, 60);
    assert_eq!(full_path.frame(60), Some(frame.clone()));
    assert_ne!(full_path.relaxed_frame(0), full_path.frame(0));
    assert_ne!(full_path.relaxed_frame(1), full_path.frame(120));
    let s = &full_path.arc_lengths_sqrt_amu_bohr;
    assert!((s[120] - s[119] - 0.07).abs() < 0.01);
    let terminal_neb_s = full_path.neb_arc_lengths_sqrt_amu_bohr.last();
    assert_eq!(terminal_neb_s, s.last());
    assert_eq!(
        full_path.reached,
        [IrcEndpoint::Reactant, IrcEndpoint::Product]
    );
    assert_eq!(full_path.completed, [false, false]);
    assert_eq!(serde_json::to_vec_pretty(&full).unwrap(), full_json);

    result.keep_full = false;
    let light_json = serde_json::to_vec_pretty(&result.for_file(0, &[])).unwrap();
    let mut light: CalcResult = serde_json::from_slice(&light_json).unwrap();
    light.validate(&frame).unwrap();
    let light_path = details(&mut light).irc.as_ref().unwrap();
    assert!(!light_path.has_frames());
    assert_eq!(light_path.frame(0), None);
    for branch in 0..2 {
        assert!(light_path.relaxed_frame(branch).is_none());
        assert_eq!(
            light_path.relaxed_ends.as_ref().unwrap()[branch].energy_hartree,
            full_path.relaxed_ends.as_ref().unwrap()[branch].energy_hartree
        );
    }
    assert_eq!(light_path.steps(), full_path.steps());
    assert_eq!(
        light_path.trajectory.energies_hartree,
        full_path.trajectory.energies_hartree
    );
    assert_eq!(
        light_path.arc_lengths_sqrt_amu_bohr,
        full_path.arc_lengths_sqrt_amu_bohr
    );
    assert_eq!(
        light_path.neb_arc_lengths_sqrt_amu_bohr,
        full_path.neb_arc_lengths_sqrt_amu_bohr
    );

    let mut older = serde_json::to_value(&light).unwrap();
    older["run"]["transition_state"]
        .as_object_mut()
        .unwrap()
        .remove("irc");
    let mut old_result: CalcResult = serde_json::from_value(older).unwrap();
    old_result.validate(&frame).unwrap();
    assert_eq!(details(&mut old_result).irc, None);
}

#[test]
fn scaled_saved_arcs_are_recomputed_with_frames_and_preserved_without_them() {
    let (frame, result) = fixture();
    for full in [true, false] {
        let mut saved = if full {
            result.clone()
        } else {
            result.light(&[])
        };
        let path = details(&mut saved).irc.as_mut().unwrap();
        for s in path
            .arc_lengths_sqrt_amu_bohr
            .iter_mut()
            .chain(&mut path.neb_arc_lengths_sqrt_amu_bohr)
        {
            *s *= ANGSTROM_PER_BOHR;
        }
        saved.validate(&frame).unwrap();
        let mut loaded: CalcResult =
            serde_json::from_slice(&serde_json::to_vec(&saved).unwrap()).unwrap();
        loaded.validate(&frame).unwrap();
        let mut expected = if full { result.clone() } else { saved };
        assert_eq!(details(&mut loaded).irc, details(&mut expected).irc);
    }
}

#[test]
fn legacy_full_and_light_claisen_saves_separate_the_relaxed_ends() {
    let (frame, mut result) = fixture();
    let trajectory: Trajectory = serde_json::from_value(evidence()["irc"].clone()).unwrap();
    for full in [true, false] {
        let mut legacy = if full {
            result.clone()
        } else {
            result.light(&[])
        };
        let band = result.trajectory.as_ref().unwrap();
        let mut old = details(&mut legacy).irc.take().unwrap();
        old.trajectory = trajectory.clone();
        old.ts_index += 1;
        old.arc_lengths_sqrt_amu_bohr =
            IrcPath::geometric_arc_lengths(&old.trajectory, old.ts_index).unwrap();
        old.project_neb(
            band,
            evidence()["climbing_index"].as_u64().unwrap() as usize,
        )
        .unwrap();
        if !full {
            old.trajectory.positions_angstrom.clear();
        }
        legacy
            .trajectory
            .as_mut()
            .unwrap()
            .positions_angstrom
            .clear();
        details(&mut legacy).irc = Some(old);
        let mut wire = serde_json::to_value(legacy).unwrap();
        wire["run"]["transition_state"]["irc"]
            .as_object_mut()
            .unwrap()
            .remove("relaxed_ends");
        let mut loaded: CalcResult = serde_json::from_value(wire).unwrap();
        loaded.validate(&frame).unwrap();
        let path = details(&mut loaded).irc.as_ref().unwrap();
        let expected = details(&mut result).irc.as_ref().unwrap();
        assert_eq!(
            path.arc_lengths_sqrt_amu_bohr,
            expected.arc_lengths_sqrt_amu_bohr
        );
        for (actual, expected) in path
            .neb_arc_lengths_sqrt_amu_bohr
            .iter()
            .zip(&expected.neb_arc_lengths_sqrt_amu_bohr)
        {
            assert!((actual - expected).abs() < 1e-12);
        }
        for branch in 0..2 {
            assert_eq!(
                path.relaxed_frame(branch),
                if full {
                    expected.relaxed_frame(branch)
                } else {
                    None
                }
            );
        }
    }
}

#[test]
fn geometric_arc_uses_actual_steps_masses_and_bohr_units() {
    let trajectory = Trajectory {
        numbers: vec![1, 1],
        isotopes: vec![1, 2],
        masses_amu: vec![1., 4.],
        charge: 0,
        multiplicity: 1,
        positions_angstrom: [-2., -1.5, 0., 0.25, 1.]
            .into_iter()
            .flat_map(|x| [0., 0., 0., x * ANGSTROM_PER_BOHR, 0., 0.])
            .collect(),
        energies_hartree: vec![-2., -1., 0., -0.5, -1.5],
        labels: vec![],
        minimizer_energies_hartree: vec![],
        cluster_sizes: vec![],
        rmsd_to_best_angstrom: vec![],
    };
    let irc = IrcPath::new(
        trajectory.clone(),
        2,
        [IrcEndpoint::Reactant, IrcEndpoint::Product],
        [true, true],
        std::array::from_fn(|_| IrcRelaxedEnd {
            energy_hartree: -2.,
            positions_angstrom: vec![],
        }),
    )
    .unwrap();
    for (actual, expected) in irc
        .arc_lengths_sqrt_amu_bohr
        .iter()
        .zip([-4., -3., 0., 0.5, 2.])
    {
        assert!((actual - expected).abs() < 1e-12);
    }
    let mut no_masses = trajectory;
    no_masses.masses_amu.clear();
    assert!(IrcPath::geometric_arc_lengths(&no_masses, 2).is_err());
}

#[test]
fn invalid_irc_arrays_states_and_saddle_values_are_rejected() {
    let (frame, good) = fixture();
    for invalid in 0..14 {
        let mut result = good.clone();
        let path = details(&mut result).irc.as_mut().unwrap();
        match invalid {
            0 => path.arc_lengths_sqrt_amu_bohr.pop().map(|_| ()).unwrap(),
            1 => path.arc_lengths_sqrt_amu_bohr[0] = f64::NAN,
            2 => path.arc_lengths_sqrt_amu_bohr[path.ts_index] = 1.,
            3 => path.arc_lengths_sqrt_amu_bohr[0] = 1.,
            4 => path.ts_index = usize::MAX,
            5 => path.trajectory.energies_hartree.pop().map(|_| ()).unwrap(),
            6 => path.trajectory.energies_hartree[path.ts_index] += 0.1,
            7 => path.trajectory.masses_amu[0] += 1.,
            8 => path.trajectory.positions_angstrom[path.ts_index * frame.numbers.len() * 3] += 0.1,
            9 => path
                .neb_arc_lengths_sqrt_amu_bohr
                .pop()
                .map(|_| ())
                .unwrap(),
            10 => path.neb_arc_lengths_sqrt_amu_bohr[0] -= 1.,
            11 => path.neb_arc_lengths_sqrt_amu_bohr[6] = 0.1,
            12 => path.trajectory.charge += 2,
            _ => path.reached.swap(0, 1),
        }
        assert!(
            result.validate(&frame).is_err(),
            "IRC invalid case {invalid}"
        );
    }
}

#[test]
fn collapsed_neb_branch_preserves_the_irc_without_comparison_dots() {
    let (frame, mut result) = fixture();
    let path = details(&mut result).irc.as_mut().unwrap();
    let original = path.arc_lengths_sqrt_amu_bohr.clone();
    let mut band = path.trajectory.clone();
    let width = band.numbers.len() * 3;
    band.positions_angstrom.copy_within(width..2 * width, 0);
    path.project_neb(&band, 1).unwrap();
    assert!(path.neb_arc_lengths_sqrt_amu_bohr.is_empty());
    assert_eq!(path.arc_lengths_sqrt_amu_bohr, original);
    result.validate(&frame).unwrap();
}

#[test]
fn claisen_irc_reports_added_full_and_light_saved_sizes() {
    let (frame, mut result) = fixture();
    let full_bytes = result.saved_bytes(0, &[]);
    let path = details(&mut result).irc.take().unwrap();
    let baseline_full_bytes = result.saved_bytes(0, &[]);
    details(&mut result).irc = Some(path);
    result.keep_full = false;
    let light_bytes = result.saved_bytes(0, &[]);
    let mut baseline = result.light(&[]);
    details(&mut baseline).irc = None;
    let baseline_light_bytes = baseline.saved_bytes(0, &[]);
    result.validate(&frame).unwrap();
    let added_full = full_bytes - baseline_full_bytes;
    let added_light = light_bytes - baseline_light_bytes;
    println!(
        "14-atom Claisen, 121 IRC frames, 2 relaxed ends, 15 NEB images: full {baseline_full_bytes} → {full_bytes} bytes (+{added_full}); light {baseline_light_bytes} → {light_bytes} bytes (+{added_light}); full minus light {} bytes",
        full_bytes - light_bytes
    );
    assert!(added_full < 40_000);
    assert!(added_light < 12_000);
    assert!(added_full > added_light);
    assert_eq!(
        result.heavy_bytes(),
        serde_json::to_vec_pretty(&result).unwrap().len() - light_bytes
    );
}
