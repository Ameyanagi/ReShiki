use super::*;
use reshiki::qcdata::{self, Provenance, Source, Trajectory, XtbSettings};

const START: [[f64; 3]; 3] = [[0., 0., 0.], [0.96, 0., 0.], [-0.24, 0.93, 0.]];
const MIDDLE: [[f64; 3]; 3] = [[0., 0., 0.], [0.958, 0., 0.], [-0.24, 0.928, 0.]];
const END: [[f64; 3]; 3] = [[0., 0., 0.], [0.9572, 0., 0.], [-0.24, 0.9266, 0.]];

fn frame(positions: &[[f64; 3]]) -> Frame {
    Frame {
        numbers: vec![8, 1, 1],
        positions_angstrom: positions.to_vec(),
        isotopes: vec![],
        masses_amu: vec![],
        charge: 0,
        multiplicity: 1,
        drawing_atoms: vec![],
    }
}

/// A water optimization over three steps; `full` keeps every step's geometry.
fn optimized(full: bool) -> CalcResult {
    let input = frame(&START);
    let output = frame(&END);
    let positions = if full {
        [START, MIDDLE, END].concat().as_flattened().to_vec()
    } else {
        vec![]
    };
    CalcResult {
        frame: output.hash(),
        input_frame: Some(input.hash()),
        task: Task::Optimize,
        endpoints: None,
        settings: Settings::Xtb(XtbSettings {
            method: qcdata::XtbMethod::Gfn2Xtb,
            conventions: Default::default(),
            scf_retries: vec![],
            solvent: Some(qcdata::Solvent {
                model: qcdata::SolventModel::Alpb,
                name: "water".into(),
                state: Default::default(),
                correction: None,
            }),
            electronic_temperature_k: None,
            accuracy: None,
            max_iterations: None,
            gfnff: None,
            threads: Some(4),
            convergence: Some(Convergence::Tight),
            fixed_atoms: vec![0],
        }),
        provenance: Provenance {
            program: "axtb".into(),
            program_version: Some("0.2.0".into()),
            method: Some("GFN2-xTB".into()),
            basis: None,
            source: Source::Calculated {
                engine_revision: "9d0388c".into(),
            },
        },
        status: Status::Converged,
        energy_hartree: Some(-5.070176),
        energy_terms: Default::default(),
        gradient_hartree_per_bohr: vec![1e-4; 9],
        hessian_hartree_per_bohr2: vec![],
        charges: vec![],
        dipole_e_bohr: None,
        bond_orders: vec![],
        orbitals: None,
        vibrations: None,
        thermo: None,
        trajectory: Some(Trajectory {
            numbers: vec![8, 1, 1],
            isotopes: vec![],
            masses_amu: vec![],
            charge: 0,
            multiplicity: 1,
            positions_angstrom: positions,
            energies_hartree: vec![-5.06, -5.0695, -5.070176],
            labels: vec![],
            minimizer_energies_hartree: vec![],
            cluster_sizes: vec![],
            rmsd_to_best_angstrom: vec![],
        }),
        keep_full: false,
        omitted_bytes: None,
        run: Some(qcdata::RunSummary {
            seconds: 1.25,
            scf_iterations: vec![9, 5],
            finished: Some("2026-11-25T14:03:00Z".into()),
        }),
    }
}

fn info(gap: f64) -> RunInfo {
    RunInfo {
        gap_ev: Some(gap),
        near_degenerate: false,
        evaluations: vec![
            Evaluation {
                energy: -5.06,
                scf_iterations: 9,
                seconds: 0.004,
            },
            Evaluation {
                energy: -5.070176,
                scf_iterations: 5,
                seconds: 0.002,
            },
        ],
        numerics: "f64".into(),
    }
}

#[test]
fn the_summary_shows_what_the_run_returned_and_hides_the_rest() {
    let result = optimized(true);
    let rows = summary_rows(&result, &frame(&START), &frame(&END), Some(&info(14.01)));
    let labels: Vec<_> = rows.iter().map(|(label, _)| label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "Total energy",
            "HOMO–LUMO gap",
            "Optimization",
            "RMS gradient",
            "Molecule",
            "Run time"
        ]
    );
    let [
        (_, energy),
        (_, gap),
        (_, convergence),
        _,
        (_, molecule),
        (_, run_time),
    ] = rows.as_slice()
    else {
        panic!("unexpected summary rows: {rows:?}");
    };
    assert_eq!(energy, "−5.070176 Eh");
    assert_eq!(gap, "14.01 eV");
    assert!(
        convergence.starts_with("converged in 2 steps · RMSD from start 0.0"),
        "{convergence}"
    );
    assert_eq!(molecule, "H2O · 3 atoms");
    assert_eq!(run_time, "1.2 s · 2 SCF calculations");

    // The saved run summary survives reopening, and missing properties are omitted.
    let rows = summary_rows(&result, &frame(&START), &frame(&END), None);
    assert_eq!(
        rows.last().unwrap(),
        &("Run time".into(), "1.2 s · 2 SCF calculations".into())
    );
    assert!(
        rows.iter()
            .all(|(label, value)| !value.is_empty() && *label != "HOMO–LUMO gap")
    );

    let mut energy = optimized(false);
    energy.task = Task::Energy;
    energy.trajectory = None;
    energy.run = None;
    energy.gradient_hartree_per_bohr.clear();
    let rows = summary_rows(&energy, &frame(&START), &frame(&START), None);
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert_eq!(View::available(&energy), [View::Summary, View::Log]);
    assert_eq!(
        View::available(&result),
        [View::Summary, View::Steps, View::Log]
    );
}

#[test]
fn near_degenerate_gaps_warn() {
    let result = optimized(false);
    assert_eq!(warning(&result, Some(&info(0.05))), Some(NEAR_DEGENERATE));
    assert_eq!(warning(&result, Some(&info(3.4))), None);
    let flagged = RunInfo {
        near_degenerate: true,
        ..info(3.4)
    };
    assert_eq!(warning(&result, Some(&flagged)), Some(NEAR_DEGENERATE));
    assert_eq!(warning(&result, None), None);
    // Stored levels decide after reopening, without run information.
    let mut close = with_properties(optimized(false));
    *close
        .orbitals
        .as_mut()
        .unwrap()
        .alpha
        .energies_hartree
        .get_mut(4)
        .unwrap() = -0.406;
    assert_eq!(warning(&close, None), Some(NEAR_DEGENERATE));
}

/// Water's GFN2-xTB properties (axtb test geometry: charges and gap; the
/// lower levels and bond orders are example values).
fn with_properties(mut result: CalcResult) -> CalcResult {
    result.charges = vec![qcdata::AtomicCharges {
        scheme: qcdata::ChargeScheme::Mulliken,
        values: vec![-0.561, 0.2805, 0.2805],
    }];
    result.dipole_e_bohr = Some([0., 0., 0.85]);
    result.bond_orders = vec![
        qcdata::BondOrder {
            a: 0,
            b: 1,
            order: 0.92,
        },
        qcdata::BondOrder {
            a: 0,
            b: 2,
            order: 0.92,
        },
    ];
    // 8 valence electrons: four doubly occupied levels, then the LUMO.
    let energies = vec![-0.95, -0.62, -0.53, -0.4091, 0.1058, 0.2];
    result.orbitals = Some(qcdata::Orbitals {
        basis: None,
        core_electrons: vec![],
        alpha: qcdata::OrbitalSet {
            occupations: vec![2., 2., 2., 2., 0., 0.],
            energies_hartree: energies,
            symmetries: vec![],
            rows: vec![],
            coefficients: vec![],
        },
        beta: qcdata::Beta::Restricted,
    });
    result
}

#[test]
fn stored_properties_show_without_run_information() {
    let result = with_properties(optimized(false));
    let (input, output) = (frame(&START), frame(&END));
    let rows = summary_rows(&result, &input, &output, None);
    let row = |name: &str| {
        rows.iter()
            .find(|(label, _)| *label == name)
            .map(|(_, value)| value.as_str())
    };
    // (0.1058 + 0.4091) Eh in eV.
    assert_eq!(row("HOMO–LUMO gap"), Some("14.01 eV"));
    assert_eq!(row("Dipole"), Some("2.16 D"));
    assert_eq!(
        row("Charges"),
        Some("O −0.561 · H +0.281 · H +0.281 (mulliken)")
    );
    assert_eq!(
        bond_orders(&result, &output),
        [
            ("O1–H2".to_owned(), "0.92".to_owned()),
            ("O1–H3".to_owned(), "0.92".to_owned())
        ]
    );
    let names: Vec<_> = levels(&result).into_iter().map(|(name, _)| name).collect();
    assert_eq!(names, ["LUMO+1", "LUMO", "HOMO", "HOMO−1", "HOMO−2"]);
    let homo = levels(&result).get(2).unwrap().1;
    assert!((homo - -0.4091 * 27.211386245981).abs() < 1e-9);

    // An open shell sharing orbitals: alpha fills one more level.
    let mut radical = result.clone();
    let orbitals = radical.orbitals.as_mut().unwrap();
    orbitals.alpha.occupations = vec![1., 1., 1., 1., 1., 0.];
    orbitals.beta = qcdata::Beta::SameOrbitals {
        occupations: vec![1., 1., 1., 1., 0., 0.],
    };
    let gap = gap_ev(&radical, None).unwrap();
    assert!(
        (gap - (0.2 - 0.1058) * 27.211386245981).abs() < 1e-9,
        "{gap}"
    );
    // Open shells list the alpha levels, from the alpha electron count.
    let names: Vec<_> = levels(&radical).into_iter().map(|(name, _)| name).collect();
    assert_eq!(names, ["α LUMO", "α HOMO", "α HOMO−1", "α HOMO−2"]);
}

#[test]
fn steps_keep_every_geometry_in_memory_and_the_ends_in_a_file() {
    let (input, output) = (frame(&START), frame(&END));
    let full = optimized(true);
    assert_eq!(
        step_positions(&full, 1, &input, &output),
        Some(MIDDLE.to_vec())
    );
    assert_eq!(step_positions(&full, 3, &input, &output), None);

    let light = full.light(&[]);
    assert_eq!(energies(&light).len(), 3, "energies stay in the file");
    assert_eq!(
        step_positions(&light, 0, &input, &output),
        Some(START.to_vec())
    );
    assert_eq!(step_positions(&light, 1, &input, &output), None);
    assert_eq!(
        step_positions(&light, 2, &input, &output),
        Some(END.to_vec())
    );
}

#[test]
fn the_method_line_names_engine_solvent_state_and_time() {
    let mut result = optimized(true);
    let local = chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2026, 11, 25, 14, 3, 0)
        .unwrap()
        .with_timezone(&chrono::Local);
    assert_eq!(
        method_line(&result, &frame(&END)),
        format!(
            "GFN2-xTB · axtb 0.2.0 · water (ALPB) · charge 0 · multiplicity 1 · {}",
            local.format("%-d %b %Y %H:%M")
        )
    );
    result.run.as_mut().unwrap().finished = Some("unknown".into());
    assert_eq!(
        method_line(&result, &frame(&END)),
        "GFN2-xTB · axtb 0.2.0 · water (ALPB) · charge 0 · multiplicity 1"
    );
    // g-xTB in water at 1000 K with xtb 6.8 conventions.
    result.run = None;
    if let Settings::Xtb(settings) = &mut result.settings {
        settings.method = qcdata::XtbMethod::GXtb;
        settings.conventions = qcdata::XtbConventions::Xtb68;
        settings.electronic_temperature_k = Some(1000.);
        settings.solvent.as_mut().unwrap().correction = Some(qcdata::SolventCorrection::Gfn2);
    }
    result.provenance.method = Some("g-xTB".into());
    assert_eq!(
        method_line(&result, &frame(&END)),
        "g-xTB · axtb 0.2.0 · water (ALPB, GFN2-xTB correction) · xtb 6.8 compatible · 1000 K · charge 0 · multiplicity 1"
    );
}

#[test]
fn the_table_and_log_are_plain_text() {
    let rows = vec![("Total energy".into(), "−5.070176 Eh".into())];
    assert_eq!(
        tsv("GFN2-xTB", &rows),
        "# GFN2-xTB\nQuantity\tValue\nTotal energy\t−5.070176 Eh\n"
    );
    let mut result = optimized(true);
    let text = log(&result, &frame(&END), Some(&info(14.01)), "GFN2-xTB");
    assert!(text.contains("engine revision 9d0388c"), "{text}");
    assert!(
        text.contains("task optimize · 4 threads · tight convergence · 1 atoms held fixed"),
        "{text}"
    );
    assert!(
        text.contains("    2        -5.07017600                 5"),
        "{text}"
    );
    assert!(text.contains("optimization converged in 2 steps"), "{text}");
    result.run = None;
    let reopened = log(&result, &frame(&END), None, "GFN2-xTB");
    assert!(
        reopened.contains("SCF details were not saved with this result; recalculate to see them.")
    );
}

#[test]
fn light_frequency_results_keep_the_spectrum_and_recalculate_the_same_task() {
    let output = frame(&END);
    let report = super::super::tests::optfreq(&START, &END);
    for (task, card_task) in [
        (Task::Optimize, CardTask::OptFreq),
        (Task::Hessian, CardTask::Frequencies),
    ] {
        let result = report
            .computed
            .to_calc_result(&output, task, &report.engine, None)
            .unwrap();
        assert_eq!(config(&result, &output, &[], 1).unwrap().task, card_task);
        let light = result.light(&[]);
        assert_eq!(config(&light, &output, &[], 1).unwrap().task, card_task);
        assert_eq!(light.thermo, result.thermo);
        let full = result.vibrations.unwrap();
        let light = light.vibrations.unwrap();
        assert_eq!(light.reduced_masses_amu, full.reduced_masses_amu);
        assert_eq!(light.masses_amu, full.masses_amu);
        assert!(!full.modes.is_empty());
        assert!(light.modes.is_empty());
    }
}

#[test]
fn the_summary_and_log_show_imaginary_modes_and_stored_thermochemistry() {
    let output = frame(&END);
    let mut report = super::super::tests::optfreq(&START, &END);
    let vibrations = report.computed.vibrations.as_mut().unwrap();
    vibrations.frequencies_cm1 = vec![-212., 23., 1590.13];
    vibrations.point_group = Some("Dinfh".into());
    let thermo = report.computed.thermo.as_mut().unwrap();
    thermo.temperature_k = 350.;
    thermo.pressure_pa = 100_000.;
    thermo.rotor_cutoff_cm1 = 75.;
    thermo.imaginary_cutoff_cm1 = -10.;
    thermo.gibbs_correction_hartree = 0.023230 - 350.0 * 0.00007198;
    let result = report
        .computed
        .to_calc_result(&output, Task::Optimize, &report.engine, None)
        .unwrap();
    let rows = summary_rows(&result, &frame(&START), &output, None);
    let row = |name: &str| {
        rows.iter()
            .find(|(label, _)| label == name)
            .map(|(_, value)| value.as_str())
    };
    assert_eq!(
        row("Frequencies"),
        Some("1 imaginary (−212i cm⁻¹) · lowest real 23 cm⁻¹")
    );
    assert_eq!(row("H (350.00 K)"), Some("−5.046946 Eh"));
    assert_eq!(row("Point group"), Some("D∞h"));
    let status =
        super::super::state::summary(report.energy, None, None, result.vibrations.as_ref());
    assert!(status.ends_with(" · 1 imaginary frequency"));
    let text = log(&result, &output, None, "GFN2-xTB");
    for expected in [
        "task opt+freq",
        "frequencies (cm⁻¹)\n -212.00    23.00  1590.13\nimaginary modes 1",
        "ZPVE  0.019453 Eh\nH(T)  -5.046946 Eh\nS  0.00007198 Eh/K\nG(T)  -5.072139 Eh",
        "point group D∞h",
        "thermochemistry: quasi-RRHO (Grimme 2012), free-rotor interpolation below 75 cm⁻¹, imaginary modes above −10 cm⁻¹ inverted, T 350.00 K, p 100000 Pa",
    ] {
        assert!(text.contains(expected), "Expected {expected:?} in {text}");
    }
    let hessian = CalcResult {
        task: Task::Hessian,
        ..result
    };
    assert!(log(&hessian, &output, None, "GFN2-xTB").contains("task frequencies"));
}

#[test]
fn rmsd_ignores_rotation_and_translation() {
    let moved: Vec<[f64; 3]> = START
        .iter()
        .map(|[x, y, z]| [-y + 3., x - 1., z + 2.])
        .collect();
    assert!(rmsd(&START, &moved).is_some_and(|r| r < 1e-9));
    assert!(rmsd(&START, &END).is_some_and(|r| r > 0. && r < 0.01));
    assert_eq!(rmsd(&START, END.get(..2).unwrap()), None);
}

#[test]
fn sizes_read_like_a_file_browser() {
    assert_eq!(size(0), "1 KB");
    assert_eq!(size(18_200), "19 KB");
    assert_eq!(size(1_400_000), "1.4 MB");
}

#[test]
fn unrestricted_gaps_follow_the_conventions_and_list_both_spins() {
    let set = |energies: Vec<f64>, occupations: Vec<f64>| qcdata::OrbitalSet {
        energies_hartree: energies,
        occupations,
        symmetries: vec![],
        rows: vec![],
        coefficients: vec![],
    };
    let mut result = with_properties(optimized(false));
    // Alpha gap 0.50 Eh, beta gap 0.60 Eh, across spins 0.40 Eh.
    result.orbitals = Some(qcdata::Orbitals {
        basis: None,
        core_electrons: vec![],
        alpha: set(vec![-0.6, -0.5, 0.0, 0.3], vec![1., 1., 0., 0.]),
        beta: qcdata::Beta::Unrestricted(set(vec![-0.7, -0.1, 0.2, 0.4], vec![1., 0., 0., 0.])),
    });
    let ev = |hartree: f64| hartree * 27.211386245981;
    let gap = gap_ev(&result, None).unwrap();
    // Native conventions: lowest empty (beta −0.1) above highest filled (alpha −0.5).
    assert!((gap - ev(0.4)).abs() < 1e-9, "{gap}");
    if let Settings::Xtb(settings) = &mut result.settings {
        settings.conventions = qcdata::XtbConventions::Xtb68;
    }
    let gap = gap_ev(&result, None).unwrap();
    assert!(
        (gap - ev(0.5)).abs() < 1e-9,
        "xtb 6.8: the alpha frontier, {gap}"
    );
    let names: Vec<_> = levels(&result).into_iter().map(|(name, _)| name).collect();
    assert_eq!(
        names,
        [
            "α LUMO+1",
            "α LUMO",
            "α HOMO",
            "α HOMO−1",
            "β LUMO+2",
            "β LUMO+1",
            "β LUMO",
            "β HOMO"
        ]
    );

    // An H atom: alpha full, beta empty, so no spin has both a HOMO and a
    // LUMO and the engine reports no gap.
    result.settings = optimized(false).settings;
    result.orbitals = Some(qcdata::Orbitals {
        basis: None,
        core_electrons: vec![],
        alpha: set(vec![-0.5], vec![1.]),
        beta: qcdata::Beta::Unrestricted(set(vec![-0.4], vec![0.])),
    });
    assert_eq!(gap_ev(&result, None), None);
}

#[test]
fn recalculate_reads_the_settings_the_result_stored() {
    let frame = qcdata::Frame {
        numbers: vec![8, 1, 1],
        positions_angstrom: vec![[0.; 3], [0.96, 0., 0.], [-0.24, 0.93, 0.]],
        isotopes: vec![],
        masses_amu: vec![],
        charge: 0,
        multiplicity: 1,
        drawing_atoms: vec![],
    };
    let result: qcdata::CalcResult = serde_json::from_value(serde_json::json!({
        "frame": frame.hash(),
        "task": "energy",
        "settings": { "xtb": {
            "method": "g_xtb",
            "conventions": "xtb-6.8",
            "solvent": { "model": "alpb", "name": "water", "correction": "gfn2" },
            "electronic_temperature_k": 1000.0,
            "accuracy": 0.5,
            "max_iterations": 400
        } },
        "provenance": { "program": "axtb", "source": { "calculated": { "engine_revision": "9d0388c" } } },
        "status": "converged"
    }))
    .expect("a stored result");
    assert_eq!(
        config(&result, &frame, &[], 3),
        Some(Config {
            task: CardTask::Energy,
            method: qcdata::XtbMethod::GXtb,
            charge: 0,
            unpaired: 0,
            solvent: Some("water"),
            xtb68: true,
            accuracy: Some(0.5),
            temperature_k: Some(1000.),
            max_iterations: Some(400),
            threads: 3,
            tight: false,
            fixed: vec![],
        })
    );
    // GBSA, a solvent the card does not list, or an imported job: no rerun.
    let mut gbsa = result.clone();
    if let qcdata::Settings::Xtb(settings) = &mut gbsa.settings {
        settings.solvent.as_mut().unwrap().model = qcdata::SolventModel::Gbsa;
    }
    assert_eq!(config(&gbsa, &frame, &[], 3), None);
    let mut benzene = result.clone();
    if let qcdata::Settings::Xtb(settings) = &mut benzene.settings {
        settings.solvent.as_mut().unwrap().name = "benzene".into();
    }
    assert_eq!(config(&benzene, &frame, &[], 3), None);
    // GFN-FF saves its resolved default fragment charges: still repeatable.
    let mut force_field = result.clone();
    if let qcdata::Settings::Xtb(settings) = &mut force_field.settings {
        settings.method = qcdata::XtbMethod::GfnFf;
        settings.solvent = None;
        settings.gfnff = Some(qcdata::GfnFfTopology {
            reference: frame.hash(),
            fragment_charges: vec![0],
        });
    }
    assert!(config(&force_field, &frame, &[], 3).is_some());
    if let qcdata::Settings::Xtb(settings) = &mut force_field.settings {
        settings.gfnff.as_mut().unwrap().fragment_charges = vec![1, -1];
    }
    assert_eq!(config(&force_field, &frame, &[], 3), None);
    let mut hessian = result.clone();
    hessian.task = Task::Hessian;
    assert_eq!(
        config(&hessian, &frame, &[], 3).unwrap().task,
        CardTask::Frequencies
    );
    let mut conformers = result;
    conformers.task = Task::Conformers;
    assert_eq!(
        config(&conformers, &frame, &[], 3).unwrap().task,
        CardTask::Conformers
    );
}
