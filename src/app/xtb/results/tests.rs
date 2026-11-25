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
        }),
        keep_full: false,
        omitted_bytes: None,
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
        tight: true,
        threads: 4,
        fixed: vec![7],
        finished: "25 Nov 2026 14:03".into(),
        seconds: 1.25,
    }
}

#[test]
fn the_summary_shows_what_the_run_returned_and_hides_the_rest() {
    let result = optimized(true);
    let rows = summary_rows(&result, &frame(&START), &frame(&END), Some(&info(14.01)));
    let labels: Vec<_> = rows.iter().map(|(label, _)| *label).collect();
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
    assert_eq!(rows[0].1, "−5.070176 Eh");
    assert_eq!(rows[1].1, "14.01 eV");
    assert!(
        rows[2]
            .1
            .starts_with("converged in 2 steps · RMSD from start 0.0"),
        "{}",
        rows[2].1
    );
    assert_eq!(rows[4].1, "H2O · 3 atoms");
    assert_eq!(rows[5].1, "1.2 s · 2 SCF calculations");

    // A reopened file has no run information: no gap or time rows, no empty ones.
    let rows = summary_rows(&result, &frame(&START), &frame(&END), None);
    assert!(rows.iter().all(|(label, value)| {
        !value.is_empty() && *label != "HOMO–LUMO gap" && *label != "Run time"
    }));

    let mut energy = optimized(false);
    energy.task = Task::Energy;
    energy.trajectory = None;
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
    close.orbitals.as_mut().unwrap().alpha.energies_hartree[4] = -0.406;
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
    assert!((levels(&result)[2].1 - -0.4091 * 27.211386245981).abs() < 1e-9);

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
    assert!(levels(&radical).is_empty(), "open shells show only the gap");
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
    let result = optimized(true);
    assert_eq!(
        method_line(&result, &frame(&END), Some(&info(14.01))),
        "GFN2-xTB · axtb 0.2.0 · water (ALPB) · charge 0 · multiplicity 1 · 25 Nov 2026 14:03"
    );
    assert_eq!(
        method_line(&result, &frame(&END), None),
        "GFN2-xTB · axtb 0.2.0 · water (ALPB) · charge 0 · multiplicity 1"
    );
    // g-xTB in water at 1000 K with xtb 6.8 conventions.
    let mut result = result;
    if let Settings::Xtb(settings) = &mut result.settings {
        settings.method = qcdata::XtbMethod::GXtb;
        settings.conventions = qcdata::XtbConventions::Xtb68;
        settings.electronic_temperature_k = Some(1000.);
        settings.solvent.as_mut().unwrap().correction = Some(qcdata::SolventCorrection::Gfn2);
    }
    result.provenance.method = Some("g-xTB".into());
    assert_eq!(
        method_line(&result, &frame(&END), None),
        "g-xTB · axtb 0.2.0 · water (ALPB, GFN2-xTB correction) · xtb 6.8 compatible · 1000 K · charge 0 · multiplicity 1"
    );
}

#[test]
fn the_table_and_log_are_plain_text() {
    let rows = vec![("Total energy", "−5.070176 Eh".to_owned())];
    assert_eq!(
        tsv("GFN2-xTB", &rows),
        "# GFN2-xTB\nQuantity\tValue\nTotal energy\t−5.070176 Eh\n"
    );
    let result = optimized(true);
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
    let reopened = log(&result, &frame(&END), None, "GFN2-xTB");
    assert!(reopened.contains("SCF details are kept only while the app is open"));
}

#[test]
fn rmsd_ignores_rotation_and_translation() {
    let moved: Vec<[f64; 3]> = START
        .iter()
        .map(|[x, y, z]| [-y + 3., x - 1., z + 2.])
        .collect();
    assert!(rmsd(&START, &moved).is_some_and(|r| r < 1e-9));
    assert!(rmsd(&START, &END).is_some_and(|r| r > 0. && r < 0.01));
    assert_eq!(rmsd(&START, &END[..2]), None);
}

#[test]
fn sizes_read_like_a_file_browser() {
    assert_eq!(size(0), "1 KB");
    assert_eq!(size(18_200), "19 KB");
    assert_eq!(size(1_400_000), "1.4 MB");
}
