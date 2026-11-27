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
            search: None,
            transition_state: None,
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

fn with_frequencies(mut result: CalcResult) -> CalcResult {
    result.vibrations = Some(qcdata::Vibrations {
        frequencies_cm1: vec![1590.13, 3468.58, 3480.11],
        ir_intensities_km_mol: vec![],
        modes: vec![],
        reduced_masses_amu: vec![],
        masses_amu: vec![],
    });
    result.thermo = Some(qcdata::Thermo {
        temperature_k: 298.15,
        pressure_pa: 101325.,
        zero_point_energy_hartree: 0.019453,
        enthalpy_correction_hartree: 0.023230,
        entropy_hartree_per_k: 0.00007198,
        gibbs_correction_hartree: 0.001769,
        heat_capacity_hartree_per_k: None,
        frequency_scale: 1.,
        rotor_cutoff_cm1: 50.,
        imaginary_cutoff_cm1: 20.,
        symmetry_number: 2,
        point_group: None,
    });
    result
}

#[test]
fn the_headline_names_the_outcome_medium_and_electronic_state() {
    let mut result = optimized(false);
    let mut output = frame(&END);
    assert_eq!(
        headline(&result, &output),
        "Converged in 2 steps · GFN2-xTB · in water (ALPB) · neutral singlet"
    );
    result.trajectory.as_mut().unwrap().energies_hartree.pop();
    assert!(headline(&result, &output).starts_with("Converged in 1 step ·"));
    result.status = Status::NotConverged {
        reason: "step limit".into(),
    };
    assert!(headline(&result, &output).starts_with("Not converged after 1 step ·"));
    result.status = Status::Converged;
    result = with_frequencies(result);
    assert!(headline(&result, &output).starts_with("Converged in 1 step + frequencies ·"));
    result.task = Task::Hessian;
    assert!(headline(&result, &output).starts_with("Frequencies ·"));
    result.task = Task::Conformers;
    assert!(headline(&result, &output).starts_with("2 conformers kept ·"));
    result.task = Task::Energy;
    let Settings::Xtb(settings) = &mut result.settings else {
        unreachable!()
    };
    settings.solvent = None;
    for (charge, multiplicity, state) in [
        (1, 2, "cation doublet"),
        (-1, 3, "anion triplet"),
        (2, 4, "dication quartet"),
        (-2, 5, "dianion quintet"),
        (3, 6, "charge +3 multiplicity 6"),
        (-3, 1, "charge −3 singlet"),
    ] {
        output.charge = charge;
        output.multiplicity = multiplicity;
        assert_eq!(
            headline(&result, &output),
            format!("Single-point energy · GFN2-xTB · gas phase · {state}")
        );
    }
}

#[test]
fn unverified_gxtb_candidates_are_not_named_found_transition_states() {
    let mut result = optimized(false);
    result.task = Task::TransitionState;
    let Settings::Xtb(settings) = &mut result.settings else {
        unreachable!()
    };
    settings.method = qcdata::XtbMethod::GXtb;
    result.provenance.method = Some("GFN2-xTB".into());
    assert!(
        headline(&result, &frame(&END))
            .starts_with("Transition-state search · no reliable saddle · g-xTB")
    );
    assert!(method_line(&result, &frame(&END)).starts_with("g-xTB ·"));
    assert!(previous_result(&result).ends_with("g-xTB transition-state candidate"));
}

#[test]
fn imported_scan_headline_uses_a_generic_energy_label() {
    let file = reshiki_io::qcread::read(
        "so2_scan.log",
        include_bytes!("../../../../tests/fixtures/qc/pymatgen/so2_scan.log"),
    )
    .unwrap();
    let job = &file.jobs[0];
    assert_eq!(job.kind, reshiki_io::qcread::JobKind::Scan);
    let result = job.result.as_ref().unwrap();
    assert_eq!(result.task, Task::Energy);
    let output = &job.result_step().unwrap().frame;
    let text = headline(result, output);
    assert!(text.starts_with("Energy · Gaussian 09 ·"), "{text}");
}

#[test]
fn imported_optimizations_without_step_energies_omit_the_step_count() {
    let mut result = optimized(true);
    let output = frame(&END);
    result.settings = Settings::Imported { keywords: vec![] };
    result.trajectory.as_mut().unwrap().energies_hartree.clear();
    assert!(headline(&result, &output).starts_with("Converged ·"));
    result.trajectory = None;
    assert!(headline(&result, &output).starts_with("Converged ·"));

    result.status = Status::NotConverged {
        reason: "step limit".into(),
    };
    assert!(headline(&result, &output).starts_with("Optimization ·"));
}

#[test]
fn methods_text_describes_a_gas_phase_optimization() {
    let mut result = optimized(false);
    result.provenance.method = Some("GFN2-xTB · optimizer TRIC".into());
    let Settings::Xtb(settings) = &mut result.settings else {
        unreachable!()
    };
    settings.solvent = None;
    settings.convergence = Some(Convergence::Normal);
    assert_eq!(
        methods_text(&result, &frame(&END)),
        concat!(
            "Geometries were optimized with GFN2-xTB [Bannwarth2019GFN2] as implemented in ",
            "ReShiki [ReShiki] (axtb 0.2.0 [Axtb]) in the gas phase to the normal convergence criteria using the TRIC optimizer ",
            "[Wang2016TRIC; Baker1996DLC; Lindh1995Hessian; Banerjee1985PRFO] with 1 atom held fixed.",
        )
    );
}

#[test]
fn methods_text_credits_the_recorded_axtb_version_and_counts_held_atoms() {
    let mut result = optimized(false);
    result.provenance.program_version = Some("0.1.3".into());
    let Settings::Xtb(settings) = &mut result.settings else {
        unreachable!()
    };
    settings.fixed_atoms = vec![0, 2];
    let text = methods_text(&result, &frame(&END));
    assert!(
        text.contains("ReShiki [ReShiki] (axtb 0.1.3 [Axtb])"),
        "{text}"
    );
    assert!(text.ends_with(" with 2 atoms held fixed."), "{text}");
}

#[test]
fn methods_text_describes_solvation_and_frequencies() {
    let mut result = with_frequencies(optimized(false));
    let text = methods_text(&result, &frame(&END));
    assert!(
        text.contains("in water using ALPB [Ehlert2021ALPB] to the tight convergence criteria")
    );
    assert!(text.contains("Harmonic frequencies and quasi-RRHO thermochemistry at 298.15 K [Grimme2012QRRHO; FernandezRamos2007Symmetry]"));
    result.task = Task::Hessian;
    result.thermo.as_mut().unwrap().temperature_k = 310.;
    let text = methods_text(&result, &frame(&END));
    assert!(text.starts_with("Harmonic frequencies and quasi-RRHO thermochemistry at 310.00 K"));
    assert!(text.contains("were calculated with GFN2-xTB"));
    assert!(!text.contains("Wang2016TRIC"));
}

#[test]
fn methods_text_describes_gxtb_conventions_and_electronic_temperature() {
    let mut result = optimized(false);
    result.task = Task::Energy;
    let Settings::Xtb(settings) = &mut result.settings else {
        unreachable!()
    };
    settings.method = qcdata::XtbMethod::GXtb;
    settings.conventions = XtbConventions::Xtb68;
    settings.electronic_temperature_k = Some(550.);
    settings.solvent = None;
    let text = methods_text(&result, &frame(&END));
    assert!(text.starts_with("Energies were computed with g-xTB [Froitzheim2025gxTB]"));
    assert!(text.ends_with("using xtb 6.8 compatible conventions [Bannwarth2021Xtb], at an electronic temperature of 550 K."));
}

#[test]
fn methods_text_cites_embedding_minimization_and_clustering_for_conformers() {
    let mut result = optimized(false);
    result.task = Task::Conformers;
    result.provenance.method =
        Some("MMFF94s ETKDG 100 starts · Butina 0.75 Å · GFN2-xTB re-rank · optimizer TRIC".into());
    let text = methods_text(&result, &frame(&END));
    for phrase in [
        "Conformers were generated with ETKDG [Riniker2015ETKDG; Wang2020ETKDGv3]",
        "MMFF94s [Halgren1999MMFF94s]",
        "clustered by heavy-atom RMSD [Butina1999Clustering]",
        "Cluster representatives were re-optimized with GFN2-xTB [Bannwarth2019GFN2]",
        "to the tight convergence criteria using the TRIC optimizer",
        "Wang2016TRIC; Baker1996DLC; Lindh1995Hessian; Banerjee1985PRFO",
    ] {
        assert!(text.contains(phrase), "{text}");
    }
}

#[test]
fn methods_text_uses_the_effective_uff_minimizer_and_xtb_optimization_stage() {
    let mut result = optimized(false);
    result.task = Task::Conformers;
    result.provenance.method =
        Some("UFF ETKDG 100 starts · Butina 0.75 Å · GFN2-xTB re-rank · optimizer ANC".into());
    let text = methods_text(&result, &frame(&END));
    assert!(text.contains("UFF [Rappe1992UFF]"), "{text}");
    assert!(!text.contains("Halgren"), "{text}");
    assert!(text.contains("Cluster representatives were re-optimized with GFN2-xTB"));
    assert!(text.contains("Grimme2017ANCopt"));
    assert!(!text.contains("Wang2016TRIC"));

    result.provenance.method =
        Some("GFN-FF ETKDG 100 starts · Butina 0.75 Å · optimizer TRIC".into());
    let Settings::Xtb(settings) = &mut result.settings else {
        unreachable!()
    };
    settings.method = qcdata::XtbMethod::GfnFf;
    let text = methods_text(&result, &frame(&END));
    assert!(text.contains("Conformer starts were optimized with GFN-FF [Spicher2020GFNFF]"));
    assert!(!text.contains("re-optimized"));
}

#[test]
fn methods_text_distinguishes_unreached_criteria_and_cites_optimizer_fallbacks() {
    for (convergence, label) in [
        (Convergence::Normal, "normal"),
        (Convergence::Tight, "tight"),
    ] {
        let mut result = optimized(false);
        result.status = Status::NotConverged {
            reason: "Stopped after 2 steps".into(),
        };
        result.provenance.method = Some("GFN2-xTB · optimizer TRIC, ANC, L-BFGS".into());
        let Settings::Xtb(settings) = &mut result.settings else {
            unreachable!()
        };
        settings.convergence = Some(convergence);
        let text = methods_text(&result, &frame(&END));
        assert!(
            text.contains(&format!(
                "for 2 steps without reaching the {label} convergence criteria"
            )),
            "{text}"
        );
        for citation in [
            "Wang2016TRIC",
            "Grimme2017ANCopt",
            "Liu1989LBFGS",
            "Armijo1966LineSearch",
        ] {
            assert!(text.contains(citation), "{text}");
        }
        assert_eq!(text.matches("Lindh1995Hessian").count(), 1);
        assert!(!text.contains("with tight convergence"));
    }
}

#[test]
fn methods_text_describes_zero_step_input_convergence_without_an_optimizer() {
    for (convergence, label, method) in [
        (Convergence::Normal, "normal", "GFN2-xTB"),
        (Convergence::Tight, "tight", "GFN2-xTB · optimizer TRIC"),
    ] {
        let mut result = optimized(false);
        result.frame = frame(&START).hash();
        result
            .trajectory
            .as_mut()
            .unwrap()
            .energies_hartree
            .truncate(1);
        result.provenance.method = Some(method.into());
        let Settings::Xtb(settings) = &mut result.settings else {
            unreachable!()
        };
        settings.convergence = Some(convergence);
        let text = methods_text(&result, &frame(&START));
        assert!(
            text.contains(&format!(
                "the start geometry already met the {label} convergence criteria"
            )),
            "{text}"
        );
        for phrase in [
            "were optimized",
            "optimizer",
            "Wang2016TRIC",
            "Lindh1995Hessian",
        ] {
            assert!(!text.contains(phrase), "{text}");
        }
    }
}

#[test]
fn methods_text_qualifies_missing_legacy_optimizer_and_convergence_provenance() {
    let mut result = optimized(false);
    let Settings::Xtb(settings) = &mut result.settings else {
        unreachable!()
    };
    settings.convergence = None;
    let text = methods_text(&result, &frame(&END));
    assert!(text.contains("convergence criteria were not recorded"));
    assert!(text.contains("optimizer algorithms were not recorded"));
    assert!(text.contains("ReShiki’s default optimizer uses TRIC [Wang2016TRIC]"));
}

#[test]
fn imported_method_line_omits_blank_provenance_fields() {
    let file = reshiki_io::qcread::read(
        "water.log",
        include_bytes!("../../../../tests/fixtures/qc/cclib/g16_water_mp2.log"),
    )
    .unwrap();
    let job = &file.jobs[0];
    let mut result = job.result.clone().unwrap();
    let output = &job.result_step().unwrap().frame;
    for (method, basis, version, expected) in [
        ("", " \t", " \t", "Gaussian · from water.log"),
        ("RMP2-FC", " ", "", "Gaussian · RMP2-FC · from water.log"),
        (
            " ",
            " STO-3G ",
            " 16 Revision A.03 ",
            "Gaussian 16 · STO-3G · from water.log",
        ),
    ] {
        result.provenance.method = Some(method.into());
        result.provenance.basis = Some(basis.into());
        result.provenance.program_version = Some(version.into());
        assert_eq!(
            method_line(&result, output),
            format!("{expected} · charge 0 · multiplicity 1")
        );
    }
}

#[test]
fn imported_methods_keep_the_orca6_method_basis_and_program_reference() {
    let file = reshiki_io::qcread::read(
        "water.out",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/qc/cclib/orca60_water_mp2.out"
        )),
    )
    .unwrap();
    let job = &file.jobs[0];
    let text = methods_text(job.result.as_ref().unwrap(), &job.steps[0].frame);
    assert!(text.contains("ORCA 6.0.1 [Neese2025ORCA6]"), "{text}");
    assert!(
        text.to_ascii_lowercase().contains("using mp2/sto-3g"),
        "{text}"
    );
}

#[test]
fn imported_methods_describe_an_unknown_molden_writer_as_a_file_source() {
    let file = reshiki_io::qcread::read(
        "water.molden",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/qc/cclib/molden5.7_water_mp2.molden"
        )),
    )
    .unwrap();
    let job = &file.jobs[0];
    let text = methods_text(job.result.as_ref().unwrap(), &job.steps[0].frame);
    assert!(
        text.contains("were read from water.molden (Molden format)"),
        "{text}"
    );
    assert!(text.contains("MoldenFormat; Schaftenaar2000Molden"));
    assert!(!text.contains("computed with Molden"));
    let mut result = job.result.as_ref().unwrap().clone();
    result.energy_hartree = Some(-75.);
    assert!(
        methods_text(&result, &job.steps[0].frame)
            .starts_with("Geometries and energies were read from water.molden")
    );
    result.energy_hartree = None;
    result.trajectory = None;
    assert!(
        methods_text(&result, &job.steps[0].frame)
            .starts_with("Geometries were read from water.molden")
    );
}

#[test]
fn imported_methods_credit_a_named_molden_writer_as_the_exporter() {
    let input = concat!(
        "# Generated by Multiwfn\n",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/qc/cclib/molden5.7_water_mp2.molden"
        ))
    );
    let file = reshiki_io::qcread::read("water.molden", input.as_bytes()).unwrap();
    let job = &file.jobs[0];
    let result = job.result.as_ref().unwrap();
    assert_eq!(result.provenance.program, "Multiwfn");
    let text = methods_text(result, &job.steps[0].frame);
    assert!(
        text.contains("read from water.molden (Molden format, written by Multiwfn)"),
        "{text}"
    );
    assert!(text.contains("imported into ReShiki [ReShiki; MoldenFormat; Schaftenaar2000Molden]"));
    assert!(!text.contains("computed with"));
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
    assert_eq!(
        key_numbers(&result, None),
        [
            ("Energy", "−5.070176 Eh".into()),
            ("Gap", "14.01 eV".into()),
            ("Dipole", "2.16 D".into()),
        ]
    );
    assert!(
        summary_grid_rows(&result, &input, &output)
            .iter()
            .all(|(label, _)| {
                !matches!(
                    label.as_str(),
                    "Total energy" | "HOMO–LUMO gap" | "Dipole" | "Run time"
                )
            })
    );
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
