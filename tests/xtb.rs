//! The real application executable, its supervised xTB worker, and the calc CLI.
#![cfg(not(target_arch = "wasm32"))]

use crate::headless;
use reshiki_xtb_worker::client::{Cancel, Error, Limits, Session};
use std::{path::Path, time::Duration};

#[cfg(unix)]
use reshiki_xtb_worker::protocol::{self, Opened, Outcome, Response};
#[cfg(feature = "xtb")]
use reshiki_xtb_worker::protocol::{
    ArrayName, ErrorKind, GfnFfOpen, HARTREE_TO_EV, HessianRequest, NEAR_DEGENERATE_GAP_EV,
    OrbitalOutput, SolvationState, Solvent, SolventModel,
};
#[cfg(any(feature = "xtb", unix))]
use reshiki_xtb_worker::protocol::{Compute, Conventions, Method, Open, Units};
#[cfg(feature = "xtb")]
use serde::Deserialize;
#[cfg(feature = "xtb")]
use std::collections::BTreeMap;
#[cfg(unix)]
use std::{thread, time::Instant};

fn executable() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_reshiki"))
}

fn fixtures() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/xtb"))
}

fn limits() -> Limits {
    Limits {
        hello: Duration::from_secs(30),
        open: Duration::from_secs(120),
        compute: Duration::from_secs(120),
        ..Limits::default()
    }
}

#[cfg(any(feature = "xtb", unix))]
fn molecule(name: &str) -> Open {
    let xyz = std::fs::read_to_string(fixtures().join(format!("{name}.xyz")))
        .expect("read xTB XYZ fixture");
    let mut lines = xyz.lines();
    let atoms: usize = lines
        .next()
        .expect("XYZ count")
        .parse()
        .expect("atom count");
    lines.next().expect("XYZ comment");
    let mut numbers = Vec::new();
    let mut positions = Vec::new();
    for line in lines.filter(|line| !line.trim().is_empty()) {
        let fields: Vec<_> = line.split_whitespace().collect();
        assert_eq!(fields.len(), 4, "{name}: XYZ row {line:?}");
        numbers.push(match fields[0] {
            "H" => 1,
            "C" => 6,
            "N" => 7,
            "O" => 8,
            "Fe" => 26,
            other => panic!("unexpected fixture element {other}"),
        });
        positions.push(std::array::from_fn(|axis| {
            fields[axis + 1].parse().expect("XYZ coordinate")
        }));
    }
    assert_eq!(numbers.len(), atoms, "{name}: XYZ atom count");
    Open {
        numbers,
        positions,
        units: Units::Angstrom,
        charge: 0,
        uhf: None,
        method: Method::Gfn2,
        conventions: Conventions::Axtb,
        solvent: None,
        scf: Default::default(),
        gfnff: None,
        threads: 1,
        heap_bytes: limits().heap_bytes,
    }
}

#[cfg(any(feature = "xtb", unix))]
fn compute(positions: Option<Vec<[f64; 3]>>, gradient: bool) -> Compute {
    Compute {
        positions,
        units: Units::Angstrom,
        gradient,
        ..Compute::default()
    }
}

#[cfg(feature = "xtb")]
#[derive(Deserialize)]
struct Reference {
    axtb: BTreeMap<String, Values>,
    #[serde(rename = "xtb-6.8")]
    xtb_68: BTreeMap<String, Values>,
}

#[cfg(feature = "xtb")]
#[derive(Deserialize)]
struct Values {
    energy: f64,
    gradient: Vec<[f64; 3]>,
    homo_lumo_gap: f64,
}

#[cfg(feature = "xtb")]
fn assert_reference(
    label: &str,
    energy: f64,
    gap_ev: f64,
    gradient: &[[f64; 3]],
    expected: &Values,
) {
    assert_eq!(gradient.len(), expected.gradient.len(), "{label}");
    let energy_difference = (energy - expected.energy).abs();
    let gap_difference = (gap_ev - expected.homo_lumo_gap * HARTREE_TO_EV).abs();
    let mut maximum_gradient_difference = 0.0_f64;
    for (actual, expected) in gradient.iter().zip(&expected.gradient) {
        for axis in 0..3 {
            assert!(actual[axis].is_finite(), "{label}: nonfinite gradient");
            maximum_gradient_difference =
                maximum_gradient_difference.max((actual[axis] - expected[axis]).abs());
        }
    }
    eprintln!(
        "xTB reference {label}: |ΔE|={energy_difference:.16e} Eh, |Δgap|={gap_difference:.16e} eV, max |Δgradient|={maximum_gradient_difference:.16e} Eh/bohr"
    );
    assert!(
        energy_difference <= 1e-10,
        "{label}: |ΔE|={energy_difference:.16e} Eh"
    );
    assert!(
        gap_difference <= 1e-9,
        "{label}: |Δgap|={gap_difference:.16e} eV"
    );
    // The fork's cross-platform validation measured differences of up to
    // 7e-8 Eh/bohr per gradient component and 4e-12 Eh in energy.
    assert!(
        maximum_gradient_difference <= 1e-7,
        "{label}: max |Δgradient|={maximum_gradient_difference:.16e} Eh/bohr"
    );
}

#[cfg(feature = "xtb")]
#[test]
fn worker_and_cli_match_reference_energies_gradients_and_gaps() {
    let reference: Reference = serde_json::from_str(include_str!("fixtures/xtb/reference.json"))
        .expect("xTB reference JSON");
    for (label, conventions, molecules) in [
        ("axtb", Conventions::Axtb, &reference.axtb),
        ("xtb-6.8", Conventions::Xtb68, &reference.xtb_68),
    ] {
        assert_eq!(molecules.len(), 3);
        for name in ["water", "caffeine", "ferrocene"] {
            let expected = &molecules[name];
            let (mut session, hello) = Session::start(executable(), limits(), &Cancel::default())
                .expect("start xTB worker");
            assert!(
                hello.engine.is_some(),
                "feature-enabled worker has an engine"
            );
            let mut open = molecule(name);
            open.conventions = conventions;
            let opened = session
                .open(&open, &Cancel::default())
                .expect("open fixture");
            assert_eq!(opened.method, Method::Gfn2);
            assert_eq!(opened.conventions, conventions);
            assert_eq!(opened.numerics, "gfn2:1");
            let result = session
                .compute(&compute(None, true), &Cancel::default())
                .expect("compute fixture");
            assert_eq!(result.method, opened.method);
            assert_eq!(result.conventions, conventions);
            assert_eq!(result.numerics, opened.numerics);
            assert!(!result.near_degenerate, "{label}/{name}");
            assert_reference(
                &format!("{label}/{name}/Session"),
                result.energy,
                result.homo_lumo_gap_ev.expect("HOMO-LUMO gap"),
                result.gradient.as_deref().expect("requested gradient"),
                expected,
            );
            session.close(&Cancel::default()).expect("close worker");

            let dir = tempfile::tempdir().expect("CLI working folder");
            let xyz = fixtures().join(format!("{name}.xyz"));
            let mut args = vec![
                "calc",
                "--energy",
                "--gradient",
                xyz.to_str().expect("fixture path"),
            ];
            if conventions == Conventions::Xtb68 {
                args.extend(["--conventions", label]);
            }
            let output = headless::run_cli(dir.path(), &args, b"", Duration::from_secs(120));
            assert!(
                output.status.success() && output.stderr.is_empty(),
                "{label}/{name}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let stdout = std::str::from_utf8(&output.stdout).expect("UTF-8 calc output");
            assert_eq!(stdout.lines().count(), 1, "one calc result line");
            let mut result: serde_json::Value =
                serde_json::from_str(stdout).expect("calc JSON output");
            assert_eq!(result["method"], "gfn2");
            assert_eq!(result["conventions"], label);
            assert_eq!(result["numerics"], "gfn2:1");
            assert_eq!(result["engine"]["name"], "axtb");
            assert_eq!(result["engine"]["rev"], reshiki_xtb_worker::AXTB_REV);
            assert_eq!(result["near_degenerate"], false, "{label}/{name}");
            let gradient: Vec<[f64; 3]> =
                serde_json::from_value(result["gradient"].take()).expect("calc gradient");
            assert_reference(
                &format!("{label}/{name}/CLI"),
                result["energy"].as_f64().expect("calc energy"),
                result["homo_lumo_gap_ev"].as_f64().expect("calc gap"),
                &gradient,
                expected,
            );
        }
    }
}

#[cfg(feature = "xtb")]
#[test]
fn calc_prints_the_near_degenerate_warning_and_preserves_success() {
    let dir = tempfile::tempdir().expect("CLI working folder");
    std::fs::write(
        dir.path().join("h2.xyz"),
        "2\nStretched H2\nH 0 0 0\nH 4 0 0\n",
    )
    .expect("write near-degenerate XYZ");
    let output = headless::run_cli(
        dir.path(),
        &["calc", "--energy", "h2.xyz"],
        b"",
        Duration::from_secs(120),
    );
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        output.stderr,
        b"reshiki: Near-degenerate electronic structure: results can differ between computers. Try a higher electronic temperature or more unpaired electrons.\n"
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).expect("calc JSON");
    let gap = result["homo_lumo_gap_ev"].as_f64().expect("numeric gap");
    assert!((0.0..NEAR_DEGENERATE_GAP_EV).contains(&gap));
    assert_eq!(result["near_degenerate"], true);
}

#[cfg(feature = "xtb")]
#[test]
fn calc_reports_a_null_gap_without_a_lumo() {
    let dir = tempfile::tempdir().expect("CLI working folder");
    std::fs::write(dir.path().join("hydride.xyz"), "1\nHydride ion\nH 0 0 0\n")
        .expect("write hydride XYZ");
    let output = headless::run_cli(
        dir.path(),
        &["calc", "--energy", "--charge", "-1", "hydride.xyz"],
        b"",
        Duration::from_secs(120),
    );
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).expect("calc JSON");
    assert_eq!(
        result.get("homo_lumo_gap_ev"),
        Some(&serde_json::Value::Null)
    );
    assert_eq!(result["near_degenerate"], false);
}

#[cfg(feature = "xtb")]
fn frame(open: &Open) -> reshiki_qcdata::Frame {
    assert_eq!(open.units, Units::Angstrom);
    reshiki_qcdata::Frame {
        numbers: open.numbers.clone(),
        positions_angstrom: open.positions.clone(),
        isotopes: vec![],
        masses_amu: vec![],
        charge: open.charge,
        multiplicity: open.uhf.unwrap_or(0) + 1,
        drawing_atoms: vec![],
    }
}

#[cfg(feature = "xtb")]
#[test]
fn water_hessian_crosses_binary_chunks_and_converts_to_a_valid_calc_result() {
    let open = molecule("water");
    let frame = frame(&open);
    let (mut session, hello) =
        Session::start(executable(), limits(), &Cancel::default()).expect("start worker");
    session.open(&open, &Cancel::default()).expect("open water");
    let computed = session
        .compute(
            &Compute {
                hessian: Some(HessianRequest::default()),
                ..Compute::default()
            },
            &Cancel::default(),
        )
        .expect("water Hessian");
    assert_eq!(computed.hessian.len(), 9 * 9);
    assert_eq!(computed.dipole_derivatives.len(), 9 * 3);
    assert!(
        computed
            .hessian
            .iter()
            .chain(&computed.dipole_derivatives)
            .all(|value| value.is_finite())
    );
    let vibrations = computed.vibrations.as_ref().expect("vibrational analysis");
    assert_eq!(vibrations.frequencies_cm1.len(), 3);
    assert_eq!(computed.normal_modes.len(), 3 * 9);
    assert!(
        vibrations
            .frequencies_cm1
            .windows(2)
            .all(|pair| pair[0] <= pair[1])
    );
    assert!(computed.thermo.is_some());
    for row in 0..9 {
        for column in 0..9 {
            assert!(
                (computed.hessian[row * 9 + column] - computed.hessian[column * 9 + row]).abs()
                    < 1e-12
            );
        }
    }
    let result = computed
        .to_calc_result(
            &frame,
            reshiki_qcdata::Task::Hessian,
            hello.engine.as_ref().expect("engine"),
            None,
        )
        .expect("convert Hessian");
    result
        .validate(&frame)
        .expect("valid scientific Hessian result");
    assert_eq!(result.frame, frame.hash());
    assert_eq!(result.hessian_hartree_per_bohr2, computed.hessian);
    assert_eq!(
        result.vibrations.as_ref().expect("stored vibrations").modes,
        computed.normal_modes
    );
    assert_eq!(result.energy_hartree, Some(computed.energy));
    session.close(&Cancel::default()).expect("close worker");

    let dir = tempfile::tempdir().expect("CLI folder");
    let xyz = fixtures().join("water.xyz");
    let output = headless::run_cli(
        dir.path(),
        &["calc", "--task", "hessian", xyz.to_str().unwrap()],
        b"",
        Duration::from_secs(120),
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("Hessian CLI JSON");
    assert_eq!(json["hessian"]["shape"], serde_json::json!([9, 9]));
    assert_eq!(json["hessian"]["values"].as_array().unwrap().len(), 81);
    assert_eq!(
        json["dipole_derivatives"]["shape"],
        serde_json::json!([9, 3])
    );
    assert_eq!(json["normal_modes"]["shape"], serde_json::json!([3, 9]));
    assert_eq!(json["arrays"].as_array().unwrap().len(), 3);
}

#[cfg(feature = "xtb")]
#[test]
fn ferrocene_all_orbitals_have_canonical_basis_rows_and_cli_arrays() {
    let open = molecule("ferrocene");
    let frame = frame(&open);
    let (mut session, hello) =
        Session::start(executable(), limits(), &Cancel::default()).expect("start worker");
    let opened = session
        .open(&open, &Cancel::default())
        .expect("open ferrocene");
    let width = opened.basis_functions.expect("basis size") as usize;
    let computed = session
        .compute(
            &Compute {
                orbitals: OrbitalOutput::All,
                bond_orders: true,
                ..Compute::default()
            },
            &Cancel::default(),
        )
        .expect("ferrocene orbitals");
    let orbitals = computed.orbitals.as_ref().expect("orbitals");
    orbitals
        .validate(&frame)
        .expect("valid canonical ferrocene orbitals");
    assert_eq!(orbitals.alpha.rows.len(), width);
    assert_eq!(orbitals.alpha.rows, (0..width as u32).collect::<Vec<_>>());
    assert_eq!(
        computed.array_values(ArrayName::CoefficientsAlpha).len(),
        width * width
    );
    let basis = orbitals.basis.as_ref().expect("canonical basis");
    assert!(
        basis.shells.iter().any(|shell| shell.l == 2 && shell.pure),
        "Fe d shell"
    );
    let result = computed
        .to_calc_result(
            &frame,
            reshiki_qcdata::Task::Energy,
            hello.engine.as_ref().unwrap(),
            None,
        )
        .expect("convert orbitals");
    result
        .validate(&frame)
        .expect("valid result with all orbital rows");
    session.close(&Cancel::default()).expect("close worker");

    let dir = tempfile::tempdir().expect("CLI folder");
    let xyz = fixtures().join("ferrocene.xyz");
    let output = headless::run_cli(
        dir.path(),
        &["calc", "--task", "orbitals", "--wbo", xyz.to_str().unwrap()],
        b"",
        Duration::from_secs(120),
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("orbital CLI JSON");
    assert_eq!(
        json["coefficients_alpha"]["shape"],
        serde_json::json!([width, width])
    );
    assert_eq!(
        json["coefficients_alpha"]["values"]
            .as_array()
            .unwrap()
            .len(),
        width * width
    );
    assert_eq!(json["orbitals"]["alpha"]["coefficients"], "");
    assert!(!json["bond_orders"].as_array().unwrap().is_empty());
}

#[cfg(feature = "xtb")]
#[test]
fn gfnff_topology_and_fragment_charges_survive_a_restarted_session() {
    let mut open = molecule("water");
    open.method = Method::Gfnff;
    open.units = Units::Bohr;
    open.numbers = vec![8, 1, 1, 8, 1, 1];
    open.positions = vec![
        [0., 0., 0.],
        [1.81, 0., 0.],
        [-0.45, 1.75, 0.],
        [10., 0., 0.],
        [11.81, 0., 0.],
        [9.55, 1.75, 0.],
    ];
    open.gfnff = Some(GfnFfOpen {
        reference_positions: Some(open.positions.clone()),
        fragment_charges: Some(vec![1, -1]),
    });
    let (mut original, hello) =
        Session::start(executable(), limits(), &Cancel::default()).expect("start GFN-FF");
    original
        .open(&open, &Cancel::default())
        .expect("open reference topology");
    let mut positions = open.positions.clone();
    positions[1][0] += 0.2;
    positions[4][1] += 0.1;
    let request = Compute {
        positions: Some(positions.clone()),
        units: Units::Bohr,
        gradient: true,
        ..Compute::default()
    };
    let computed = original
        .compute(&request, &Cancel::default())
        .expect("displaced GFN-FF");
    let topology = computed
        .settings
        .gfnff
        .as_ref()
        .expect("persistent topology");
    assert_eq!(topology.reference_positions_bohr, open.positions);
    assert_eq!(topology.fragment_charges, [1, -1]);
    assert_eq!(
        computed.gfnff.as_ref().expect("fragments").fragments,
        [0, 0, 0, 1, 1, 1]
    );
    assert!(computed.scf.is_none());
    assert!(computed.homo_lumo_gap_ev.is_none());
    let scientific_frame = |positions: &[[f64; 3]]| reshiki_qcdata::Frame {
        numbers: open.numbers.clone(),
        positions_angstrom: positions
            .iter()
            .map(|row| row.map(|coordinate| coordinate * reshiki_qcdata::ANGSTROM_PER_BOHR))
            .collect(),
        isotopes: vec![],
        masses_amu: vec![],
        charge: 0,
        multiplicity: 1,
        drawing_atoms: vec![],
    };
    let reference = scientific_frame(&open.positions).hash();
    let evaluated = scientific_frame(&positions);
    assert!(matches!(
        computed.to_calc_result(
            &evaluated,
            reshiki_qcdata::Task::Gradient,
            hello.engine.as_ref().unwrap(),
            None,
        ),
        Err(reshiki_qcdata::Error::Invalid(_))
    ));
    let result = computed
        .to_calc_result(
            &evaluated,
            reshiki_qcdata::Task::Gradient,
            hello.engine.as_ref().unwrap(),
            Some(reference),
        )
        .expect("convert persistent GFN-FF topology");
    let reshiki_qcdata::Settings::Xtb(settings) = result.settings else {
        panic!("xTB settings");
    };
    let saved_topology = settings.gfnff.expect("saved topology");
    assert_eq!(saved_topology.reference, reference);
    assert_eq!(saved_topology.fragment_charges, [1, -1]);
    original
        .close(&Cancel::default())
        .expect("close original GFN-FF");
    open.positions = positions;
    open.gfnff = Some(GfnFfOpen {
        reference_positions: Some(topology.reference_positions_bohr.clone()),
        fragment_charges: Some(topology.fragment_charges.clone()),
    });
    let (mut restarted, _) =
        Session::start(executable(), limits(), &Cancel::default()).expect("restart GFN-FF");
    restarted
        .open(&open, &Cancel::default())
        .expect("restore topology");
    let repeated = restarted
        .compute(
            &Compute {
                units: Units::Bohr,
                gradient: true,
                ..Compute::default()
            },
            &Cancel::default(),
        )
        .expect("restarted GFN-FF energy");
    assert!((computed.energy - repeated.energy).abs() < 1e-12);
    assert_eq!(computed.gradient, repeated.gradient);
    restarted
        .close(&Cancel::default())
        .expect("close restarted GFN-FF");
}

#[cfg(feature = "xtb")]
#[test]
fn unsupported_gfnff_outputs_are_refused_by_client_and_cli() {
    let mut open = molecule("water");
    open.method = Method::Gfnff;
    let (mut session, _) =
        Session::start(executable(), limits(), &Cancel::default()).expect("start worker");
    session
        .open(&open, &Cancel::default())
        .expect("open GFN-FF");
    for (request, expected) in [
        (
            Compute {
                orbitals: OrbitalOutput::All,
                ..Compute::default()
            },
            "GFN-FF does not provide orbitals",
        ),
        (
            Compute {
                bond_orders: true,
                ..Compute::default()
            },
            "GFN-FF does not provide bond orders",
        ),
    ] {
        let error = session
            .compute(&request, &Cancel::default())
            .expect_err("unsupported output");
        assert!(
            matches!(error, Error::Unsupported(ref message) if message == expected),
            "{error:?}"
        );
        session
            .compute(&Compute::default(), &Cancel::default())
            .expect("session survives refusal");
    }
    session.close(&Cancel::default()).expect("close worker");
    let dir = tempfile::tempdir().expect("CLI folder");
    let xyz = fixtures().join("water.xyz");
    for (args, expected) in [
        (
            vec!["--task", "orbitals"],
            "GFN-FF does not provide orbitals",
        ),
        (vec!["--wbo"], "GFN-FF does not provide bond orders"),
    ] {
        let mut tokens = vec!["calc", "--method", "gfnff"];
        tokens.extend(args);
        tokens.push(xyz.to_str().unwrap());
        let output = headless::run_cli(dir.path(), &tokens, b"", Duration::from_secs(120));
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert_eq!(
            String::from_utf8(output.stderr).unwrap(),
            format!("reshiki: error: {expected}\n")
        );
    }
}

#[cfg(feature = "xtb")]
#[test]
fn reused_session_matches_fresh_sessions_for_three_displaced_geometries() {
    let (mut reused, _) =
        Session::start(executable(), limits(), &Cancel::default()).expect("start reusable worker");
    reused
        .open(&molecule("water"), &Cancel::default())
        .expect("open reusable molecule");
    let mut maximum_difference = 0.0_f64;
    for step in 1..=3 {
        let mut geometry = molecule("water");
        geometry.positions[1][0] += 0.002 * f64::from(step);
        geometry.positions[2][2] -= 0.001 * f64::from(step);
        let cached = reused
            .compute(
                &compute(Some(geometry.positions.clone()), false),
                &Cancel::default(),
            )
            .expect("reused compute");
        let (mut fresh, _) =
            Session::start(executable(), limits(), &Cancel::default()).expect("start fresh worker");
        fresh
            .open(&geometry, &Cancel::default())
            .expect("open displaced geometry");
        let independent = fresh
            .compute(&compute(None, false), &Cancel::default())
            .expect("fresh compute");
        fresh.close(&Cancel::default()).expect("close fresh worker");
        let difference = (cached.energy - independent.energy).abs();
        maximum_difference = maximum_difference.max(difference);
        eprintln!(
            "xTB reused/fresh geometry {step}: |ΔE|={difference:.16e} Eh, SCF iterations {}/{}",
            cached.scf.as_ref().expect("SCF metadata").iterations,
            independent.scf.as_ref().expect("SCF metadata").iterations
        );
    }
    reused
        .close(&Cancel::default())
        .expect("close reused worker");
    assert!(
        maximum_difference <= 1e-10,
        "maximum reused/fresh |ΔE|={maximum_difference:.16e} Eh"
    );
}

#[cfg(feature = "xtb")]
#[test]
fn invalid_client_input_is_rejected_without_losing_the_session() {
    let (mut session, _) =
        Session::start(executable(), limits(), &Cancel::default()).expect("start worker");
    let water = molecule("water");
    let mut invalid = Vec::new();
    let mut open = water.clone();
    open.numbers.clear();
    open.positions.clear();
    invalid.push(("no atoms", open));
    let mut open = water.clone();
    open.numbers = vec![1; reshiki_xtb_worker::protocol::MAX_ATOMS + 1];
    open.positions = vec![[0.0; 3]; open.numbers.len()];
    invalid.push(("too many atoms", open));
    let mut open = water.clone();
    open.positions.pop();
    invalid.push(("position count", open));
    for coordinate in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut open = water.clone();
        open.positions[0][0] = coordinate;
        invalid.push(("nonfinite coordinate", open));
    }
    for threads in [0, 65] {
        let mut open = water.clone();
        open.threads = threads;
        invalid.push(("thread count", open));
    }
    for (name, open) in invalid {
        let result = session.open(&open, &Cancel::default());
        if name == "too many atoms" {
            assert!(matches!(result, Err(Error::Limit(_))), "{name}: {result:?}");
        } else {
            assert!(matches!(result, Err(Error::Input(_))), "{name}: {result:?}");
        }
    }

    // A finite replacement cannot be compared to an atom count before open.
    // Keep the worker's state error, but reject nonfinite data before sending it.
    for positions in [None, Some(vec![[0.0; 3]])] {
        let result = session.compute(&compute(positions, false), &Cancel::default());
        assert!(
            matches!(
                result,
                Err(Error::Engine {
                    kind: ErrorKind::State,
                    ..
                })
            ),
            "{result:?}"
        );
    }
    for positions in [
        vec![],
        vec![[0.0; 3]; reshiki_xtb_worker::protocol::MAX_ATOMS + 1],
        vec![[f64::NAN; 3]],
        vec![[f64::INFINITY; 3]],
        vec![[f64::NEG_INFINITY; 3]],
    ] {
        let too_many = positions.len() > reshiki_xtb_worker::protocol::MAX_ATOMS;
        let result = session.compute(&compute(Some(positions), false), &Cancel::default());
        if too_many {
            assert!(matches!(result, Err(Error::Limit(_))), "{result:?}");
        } else {
            assert!(matches!(result, Err(Error::Input(_))), "{result:?}");
        }
    }

    session
        .open(&water, &Cancel::default())
        .expect("open water after invalid input");
    let initial = session
        .compute(&compute(None, false), &Cancel::default())
        .expect("initial compute");
    let result = session.compute(&compute(Some(vec![[0.0; 3]]), false), &Cancel::default());
    assert!(matches!(result, Err(Error::Input(_))), "{result:?}");
    let mut invalid_positions = water.positions.clone();
    invalid_positions[0][0] = f64::NAN;
    let result = session.compute(&compute(Some(invalid_positions), false), &Cancel::default());
    assert!(matches!(result, Err(Error::Input(_))), "{result:?}");
    let recovered = session
        .compute(&compute(None, false), &Cancel::default())
        .expect("compute after invalid replacement");
    assert!(
        (recovered.energy - initial.energy).abs() <= 1e-10,
        "invalid replacement changed the geometry: {} vs {}",
        recovered.energy,
        initial.energy
    );
    session
        .close(&Cancel::default())
        .expect("close worker after invalid input");
}

#[cfg(feature = "xtb")]
#[test]
fn failed_engine_compute_keeps_the_last_successful_geometry() {
    let water = molecule("water");
    let cancel = Cancel::default();
    let (mut session, _) = Session::start(executable(), limits(), &cancel).expect("start worker");
    let pid = session.pid();
    session.open(&water, &cancel).expect("open water");
    session
        .compute(&compute(None, false), &cancel)
        .expect("compute water before the failed replacement");

    // The shifted hydrogen has finite coordinates even in bohr, but its
    // squared distance overflows in axtb and causes an SCF numerical error.
    let mut positions = water.positions.clone();
    positions[2] = [1e200, 0.0, 0.0];
    let error = session
        .compute(&compute(Some(positions), false), &cancel)
        .expect_err("axtb should fail to compute the replacement geometry");
    assert!(
        matches!(
            error,
            Error::Engine {
                kind: ErrorKind::Numerical,
                ..
            }
        ),
        "unexpected replacement error: {error:?}"
    );

    let recovered = session
        .compute(&compute(None, false), &cancel)
        .expect("compute after the engine error");
    assert_eq!(session.pid(), pid, "engine error changed the worker PID");

    let (mut fresh, _) =
        Session::start(executable(), limits(), &cancel).expect("start fresh worker");
    fresh.open(&water, &cancel).expect("open original water");
    let original = fresh
        .compute(&compute(None, false), &cancel)
        .expect("compute original water in the fresh worker");
    let difference = (recovered.energy - original.energy).abs();
    eprintln!("xTB recovery after {error}: |ΔE|={difference:.16e} Eh");
    assert!(
        difference <= 1e-10,
        "recovered and fresh energies differ by {difference:.16e} Eh"
    );
    fresh.close(&cancel).expect("close fresh worker");
    session.close(&cancel).expect("close recovered worker");
}

#[cfg(feature = "xtb")]
#[test]
fn non_finite_thermochemistry_keeps_the_last_successful_geometry() {
    let water = molecule("water");
    let cancel = Cancel::default();
    let (mut session, _) = Session::start(executable(), limits(), &cancel).expect("start worker");
    session.open(&water, &cancel).expect("open water");
    let original = session.compute(&Compute::default(), &cancel).unwrap();
    let mut positions = water.positions;
    positions[2][0] += 0.1;
    let mut request = compute(Some(positions), false);
    let mut hessian = HessianRequest::default();
    hessian.thermo.pressure_pa = Some(5e-324);
    request.hessian = Some(hessian);
    assert!(matches!(
        session.compute(&request, &cancel),
        Err(Error::Engine {
            kind: ErrorKind::Numerical,
            ..
        })
    ));
    let recovered = session.compute(&Compute::default(), &cancel).unwrap();
    assert!((recovered.energy - original.energy).abs() <= 1e-10);
    session.close(&cancel).expect("close recovered worker");
}

#[cfg(feature = "xtb")]
#[test]
fn worker_rejects_invalid_electrons_spin_and_unknown_solvent_on_open() {
    for (name, charge, uhf, solvent) in [
        ("negative valence electron count", 9, None, None),
        ("UHF parity", 0, Some(1), None),
        ("UHF exceeds valence electron count", 0, Some(10), None),
        (
            "unknown solvent",
            0,
            None,
            Some(Solvent {
                model: SolventModel::Alpb,
                name: "reshiki-no-such-solvent".into(),
                state: SolvationState::Gsolv,
                correction: None,
            }),
        ),
    ] {
        let (mut session, _) =
            Session::start(executable(), limits(), &Cancel::default()).expect("start worker");
        let mut open = molecule("water");
        open.charge = charge;
        open.uhf = uhf;
        open.solvent = solvent;
        let result = session.open(&open, &Cancel::default());
        assert!(
            matches!(
                result,
                Err(Error::Engine {
                    kind: ErrorKind::Input,
                    ..
                })
            ),
            "{name}: {result:?}"
        );
        session
            .close(&Cancel::default())
            .expect("close worker after rejected open");
    }
}

#[cfg(feature = "xtb")]
#[test]
fn cef3_accepts_explicit_and_default_valence_spin() {
    let mut cef3 = Open {
        numbers: vec![58, 9, 9, 9],
        positions: vec![
            [0.0, 0.0, 0.0],
            [2.1, 0.0, 0.0],
            [-1.05, 1.82, 0.0],
            [-1.05, -1.82, 0.0],
        ],
        uhf: Some(0),
        ..molecule("water")
    };
    let cancel = Cancel::default();
    for uhf in [Some(0), None] {
        cef3.uhf = uhf;
        let (mut session, _) =
            Session::start(executable(), limits(), &cancel).expect("start worker");
        let opened = session.open(&cef3, &cancel).expect("open neutral CeF3");
        assert_eq!(opened.uhf, 0);
        session.close(&cancel).expect("close worker");
    }
}

#[cfg(all(feature = "xtb", unix))]
#[test]
fn directly_launched_worker_clears_lapack_override_and_matches_water_energy() {
    use reshiki_xtb_worker::protocol::{self, Outcome, Request, RequestOp, Response};
    use std::{
        io::Write,
        process::{Command, Stdio},
    };

    let mut child = Command::new(executable())
        .arg("--xtb-worker")
        .env("AXTB_LAPACK", "/no/such/reshiki-test-lapack")
        .env_remove("AXTB_LAPACK_BLAS")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("launch worker directly");
    let pid = child.id();
    let watchdog = PidWatchdog::from_pid(move || Some(pid), Duration::from_secs(30));
    let mut input = child.stdin.take().expect("worker stdin");
    let mut output = child.stdout.take().expect("worker stdout");
    let mut id = 0;
    let mut request = |op| {
        id += 1;
        let frame = protocol::encode_frame(&Request { id, op }).expect("encode request");
        input.write_all(&frame).expect("write request");
        let response: Response<serde_json::Value> = protocol::read_frame(&mut output)
            .expect("read response")
            .expect("worker response");
        assert_eq!(response.id, id);
        match response.result {
            Outcome::Ok { ok } => ok,
            Outcome::Error { error } => panic!("request {id}: {error:?}"),
        }
    };

    let hello = request(RequestOp::Hello {});
    assert_eq!(hello["engine"]["name"], "axtb");
    #[cfg(target_os = "linux")]
    {
        // axtb can fall back after a failed LAPACK load, so the energy alone
        // cannot distinguish the clean environment from that fallback.
        let environment =
            std::fs::read(format!("/proc/{pid}/environ")).expect("worker environment");
        assert!(environment.split(|&byte| byte == 0).all(|entry| {
            !entry.starts_with(b"AXTB_LAPACK=") && !entry.starts_with(b"AXTB_LAPACK_BLAS=")
        }));
    }
    let opened = request(RequestOp::Open(molecule("water")));
    assert_eq!(opened["atoms"], 3);
    let computed = request(RequestOp::Compute(compute(None, false)));
    let energy = computed["energy"].as_f64().expect("water energy");
    let reference: Reference = serde_json::from_str(include_str!("fixtures/xtb/reference.json"))
        .expect("xTB reference JSON");
    let expected = reference.axtb["water"].energy;
    assert!((energy - expected).abs() <= 1e-10, "{energy} vs {expected}");
    request(RequestOp::Close {});
    assert!(child.wait().expect("reap worker").success());
    watchdog.finish();
}

#[cfg(all(feature = "xtb", unix))]
#[test]
fn cancellation_precedes_local_validation_and_reaps_the_worker() {
    let cancel = Cancel::default();
    cancel.cancel();
    let water = molecule("water");

    let (mut session, _) =
        Session::start(executable(), limits(), &Cancel::default()).expect("start worker");
    let pid = session.pid();
    let invalid_open = Open {
        threads: 0,
        ..water.clone()
    };
    let result = session.open(&invalid_open, &cancel);
    assert!(matches!(result, Err(Error::Cancelled)), "{result:?}");
    assert_pid_gone(pid);

    let (mut session, _) =
        Session::start(executable(), limits(), &Cancel::default()).expect("start worker");
    session
        .open(&water, &Cancel::default())
        .expect("open water");
    let pid = session.pid();
    let invalid_compute = compute(Some(vec![]), false);
    let result = session.compute(&invalid_compute, &cancel);
    assert!(matches!(result, Err(Error::Cancelled)), "{result:?}");
    assert_pid_gone(pid);
}

// A 288-atom cluster gives SIGKILL a stable window during
// compute, including optimized builds where the small reference runs take ms.
#[cfg(all(feature = "xtb", unix))]
fn long_running_molecule() -> Open {
    let mut cluster = molecule("caffeine");
    let numbers = cluster.numbers.clone();
    let positions = cluster.positions.clone();
    for copy in 1..12 {
        cluster.numbers.extend_from_slice(&numbers);
        cluster.positions.extend(positions.iter().map(|position| {
            [
                position[0] + 15.0 * f64::from(copy),
                position[1],
                position[2],
            ]
        }));
    }
    cluster
}

#[cfg(unix)]
#[test]
fn cancellation_after_the_compute_frame_kills_and_reaps_the_worker() {
    let dir = tempfile::tempdir().expect("fake worker folder");
    let open = molecule("water");
    let opened = fake_opened(&open, Some(6));
    let mut session = open_scripted_worker(dir.path(), &open, &opened, &[]);
    let request = compute(None, false);
    let pid = session.pid();
    let watchdog = PidWatchdog::from_pid(move || Some(pid), Duration::from_secs(8));
    let cancel = Cancel::default();
    let helper = cancel_after_request(dir.path(), 2, &cancel, Duration::ZERO);
    let result = session.compute(&request, &cancel);
    let cancelled = helper.join();
    watchdog.finish();
    let cancelled = cancelled.expect("cancel helper");
    assert_pid_gone(pid);
    let elapsed = cancelled.elapsed();
    eprintln!("xTB cancellation after complete compute frame: {elapsed:?}");
    assert!(matches!(result, Err(Error::Cancelled)), "{result:?}");
    assert!(
        elapsed < Duration::from_secs(2),
        "cancellation took {elapsed:?}"
    );
}

#[cfg(all(feature = "xtb", unix))]
#[test]
fn compute_deadline_kills_and_reaps_the_worker() {
    let (mut session, _) = Session::start(
        executable(),
        Limits {
            compute: Duration::from_millis(2),
            ..limits()
        },
        &Cancel::default(),
    )
    .expect("start worker");
    session
        .open(&molecule("caffeine"), &Cancel::default())
        .expect("open caffeine");
    let pid = session.pid();
    let watchdog = PidWatchdog::new(pid);
    let started = Instant::now();
    let result = session.compute(&compute(None, true), &Cancel::default());
    let elapsed = started.elapsed();
    watchdog.finish();
    eprintln!("xTB 2 ms caffeine compute deadline: {elapsed:?}");
    assert!(matches!(result, Err(Error::Timeout)), "{result:?}");
    assert!(
        elapsed < Duration::from_secs(2),
        "deadline returned after {elapsed:?}"
    );
    assert_pid_gone(pid);
}

#[cfg(all(feature = "xtb", unix))]
#[test]
fn sigkill_during_compute_reports_exit_without_hanging() {
    let (mut session, _) =
        Session::start(executable(), limits(), &Cancel::default()).expect("start worker");
    session
        .open(&long_running_molecule(), &Cancel::default())
        .expect("open caffeine cluster");
    let pid = session.pid();
    let watchdog = PidWatchdog::new(pid);
    let started = Instant::now();
    let result = thread::scope(|scope| {
        scope.spawn(|| {
            thread::sleep(Duration::from_millis(100));
            assert!(kill(pid, "-KILL"), "SIGKILL worker {pid}");
        });
        session.compute(&compute(None, true), &Cancel::default())
    });
    let elapsed = started.elapsed();
    watchdog.finish();
    eprintln!("xTB SIGKILL during compute: {elapsed:?}");
    assert!(matches!(result, Err(Error::Exited { .. })), "{result:?}");
    assert!(
        elapsed < Duration::from_secs(2),
        "exit returned after {elapsed:?}"
    );
    assert_pid_gone(pid);
}

#[cfg(unix)]
fn kill(pid: u32, signal: &str) -> bool {
    std::process::Command::new("kill")
        .args([signal, &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("run kill")
        .success()
}

#[cfg(unix)]
fn assert_pid_gone(pid: u32) {
    assert!(
        !kill(pid, "-0"),
        "worker PID {pid} still exists after client returned"
    );
}

/// If the deadline/cancel implementation regresses, terminate the worker so
/// the test fails promptly instead of leaving the entire suite blocked.
#[cfg(unix)]
struct PidWatchdog {
    stop: std::sync::mpsc::Sender<()>,
    thread: thread::JoinHandle<bool>,
}

#[cfg(unix)]
impl PidWatchdog {
    #[cfg(feature = "xtb")]
    fn new(pid: u32) -> Self {
        Self::from_pid(move || Some(pid), Duration::from_secs(2))
    }

    fn from_pid(pid: impl FnOnce() -> Option<u32> + Send + 'static, timeout: Duration) -> Self {
        let (stop, receiver) = std::sync::mpsc::channel();
        let thread = thread::spawn(move || {
            if receiver.recv_timeout(timeout) != Err(std::sync::mpsc::RecvTimeoutError::Timeout) {
                return false;
            }
            if let Some(pid) = pid() {
                kill(pid, "-KILL");
            }
            true
        });
        Self { stop, thread }
    }

    fn finish(self) {
        let _ = self.stop.send(());
        assert!(
            !self.thread.join().expect("worker watchdog"),
            "worker operation exceeded watchdog"
        );
    }
}

#[cfg(unix)]
fn write_fake_worker(dir: &Path, body: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let script = dir.join("worker.sh");
    let pid_file = dir.join("pid");
    let quoted_pid_file = pid_file.to_string_lossy().replace('\'', "'\\''");
    std::fs::write(
        &script,
        format!("#!/bin/sh\nprintf '%s\\n' \"$$\" > '{quoted_pid_file}'\n{body}"),
    )
    .expect("write fake worker");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700))
        .expect("executable script");
    (script, pid_file)
}

#[cfg(unix)]
fn write_scripted_worker(
    dir: &Path,
    responses: &[&[u8]],
) -> (std::path::PathBuf, std::path::PathBuf) {
    use std::fmt::Write;

    let mut body = String::from(
        r#"set -e
cd "$(dirname "$0")"
read_request() {
    dd bs=1 count=16 of=header 2>/dev/null
    [ "$(wc -c < header)" -eq 16 ]
    set -- $(od -An -tu4 -j12 -N4 header)
    received=$(dd bs=1 count="$1" 2>/dev/null | wc -c)
    [ "$received" -eq "$1" ]
}
"#,
    );
    for (index, bytes) in responses.iter().enumerate() {
        std::fs::write(dir.join(format!("response-{index}")), bytes).expect("write response");
        writeln!(
            body,
            "read_request\n: > request-{index}\ncat response-{index}"
        )
        .expect("write script");
    }
    body.push_str("exec sleep 30\n");
    write_fake_worker(dir, &body)
}

#[cfg(unix)]
fn ok_frame(id: u64, ok: &impl serde::Serialize) -> Vec<u8> {
    protocol::encode_frame(&Response {
        id,
        result: Outcome::Ok { ok },
    })
    .expect("encode fake worker response")
}

#[cfg(unix)]
fn hello_frame() -> Vec<u8> {
    ok_frame(
        1,
        &protocol::Hello {
            protocol: protocol::VERSION,
            engine: Some(protocol::Engine {
                name: "axtb".into(),
                version: "0.2.0".into(),
                license: "LGPL-3.0-or-later".into(),
                source: "https://github.com/Ameyanagi/axtb".into(),
                rev: "test".into(),
            }),
            methods: Method::ALL.to_vec(),
            max_atoms: protocol::MAX_ATOMS,
            max_hessian_atoms: protocol::MAX_HESSIAN_ATOMS,
        },
    )
}

#[cfg(unix)]
fn fake_opened(open: &Open, basis_functions: Option<u32>) -> Opened {
    Opened {
        atoms: open.numbers.len(),
        method: open.method,
        uhf: open.uhf.unwrap_or(0),
        conventions: open.conventions,
        numerics: match open.method {
            Method::Gfn2 => "gfn2:1",
            Method::Gfnff => "gfnff:1",
            Method::Gxtb => "gxtb-v2.0.1:1",
        }
        .into(),
        engine_conventions: "axtb".into(),
        basis_functions,
    }
}

#[cfg(unix)]
fn open_scripted_worker(dir: &Path, open: &Open, opened: &Opened, next: &[u8]) -> Session {
    let (script, _) = write_scripted_worker(dir, &[&hello_frame(), &ok_frame(2, opened), next]);
    let cancel = Cancel::default();
    let (mut session, _) = Session::start(&script, limits(), &cancel).expect("fake worker hello");
    session.open(open, &cancel).expect("open fake worker");
    session
}

#[cfg(unix)]
fn fake_worker_pid(pid_file: &Path) -> Option<u32> {
    std::fs::read_to_string(pid_file).ok()?.trim().parse().ok()
}

#[cfg(unix)]
fn cancel_after_request(
    dir: &Path,
    index: usize,
    cancel: &Cancel,
    delay: Duration,
) -> thread::JoinHandle<Instant> {
    let received = dir.join(format!("request-{index}"));
    let cancel = cancel.clone();
    thread::spawn(move || {
        let waiting = Instant::now();
        while !received.exists() {
            assert!(
                waiting.elapsed() < Duration::from_secs(3),
                "request {index} was not received"
            );
            thread::sleep(Duration::from_millis(5));
        }
        thread::sleep(delay);
        let cancelled = Instant::now();
        cancel.cancel();
        cancelled
    })
}

#[cfg(unix)]
#[test]
fn cancellation_during_hello_kills_and_reaps_the_worker() {
    let dir = tempfile::tempdir().expect("fake worker folder");
    let (script, pid_file) = write_scripted_worker(dir.path(), &[&[]]);
    let watchdog_pid = pid_file.clone();
    let watchdog = PidWatchdog::from_pid(
        move || fake_worker_pid(&watchdog_pid),
        Duration::from_secs(5),
    );
    let cancel = Cancel::default();
    let helper = cancel_after_request(dir.path(), 0, &cancel, Duration::from_millis(200));
    let started = Instant::now();
    let error = Session::start(&script, limits(), &cancel).err();
    let elapsed = started.elapsed();
    watchdog.finish();
    helper.join().expect("cancel helper");
    assert!(matches!(error, Some(Error::Cancelled)), "{error:?}");
    assert!(elapsed < Duration::from_secs(2), "hello took {elapsed:?}");
    assert_pid_gone(fake_worker_pid(&pid_file).expect("fake worker PID"));
}

#[cfg(unix)]
#[test]
fn cancellation_during_close_kills_and_reaps_the_worker() {
    let open = molecule("water");
    let opened = fake_opened(&open, Some(6));
    for (case, response) in [
        ("no close reply", vec![]),
        ("no exit", ok_frame(3, &protocol::Closed {})),
    ] {
        let dir = tempfile::tempdir().expect("fake worker folder");
        let session = open_scripted_worker(dir.path(), &open, &opened, &response);
        let pid = session.pid();
        let watchdog = PidWatchdog::from_pid(move || Some(pid), Duration::from_secs(5));
        let cancel = Cancel::default();
        let helper = cancel_after_request(dir.path(), 2, &cancel, Duration::from_millis(200));
        let started = Instant::now();
        let result = session.close(&cancel);
        let elapsed = started.elapsed();
        watchdog.finish();
        helper.join().expect("cancel helper");
        assert!(
            matches!(result, Err(Error::Cancelled)),
            "{case}: {result:?}"
        );
        assert!(
            elapsed < Duration::from_secs(2),
            "{case}: close took {elapsed:?}"
        );
        assert_pid_gone(pid);
    }
}

#[cfg(unix)]
#[test]
fn cancellation_before_hello_skips_spawning_the_worker() {
    let dir = tempfile::tempdir().expect("fake worker folder");
    let (script, pid_file) = write_scripted_worker(dir.path(), &[&hello_frame()]);
    let cancel = Cancel::default();
    cancel.cancel();
    for executable in [script, dir.path().join("missing-worker")] {
        let started = Instant::now();
        let error = Session::start(&executable, limits(), &cancel).err();
        let elapsed = started.elapsed();
        assert!(matches!(error, Some(Error::Cancelled)), "{error:?}");
        assert!(elapsed < Duration::from_secs(2), "start took {elapsed:?}");
    }
    assert!(!pid_file.exists(), "cancelled start launched the script");
}

#[cfg(unix)]
#[test]
fn fake_workers_exit_garbage_and_partial_response_never_hang() {
    let hello_deadline = Duration::from_secs(5);
    let watchdog_deadline = hello_deadline + Duration::from_secs(2);
    let frame = ok_frame(1, &protocol::Closed {});
    let escaped: String = frame[..protocol::HEADER_LEN + 7]
        .iter()
        .map(|byte| format!("\\{byte:03o}"))
        .collect();
    let partial = format!("printf '{escaped}'\n");
    for (name, body) in [
        ("exit", "echo 'fake immediate exit' >&2\nexit 7\n".into()),
        (
            "garbage",
            "printf 'this is not an xTB protocol frame'\nexec sleep 30\n".into(),
        ),
        ("truncated_eof", partial.clone()),
        (
            "truncated_closed_stdout",
            format!("{partial}exec 1>&-\nexec sleep 30\n"),
        ),
        // A complete valid header announcing 16 body bytes, followed by only
        // seven bytes and a live process: this must reach the hello deadline.
        ("partial", format!("{partial}exec sleep 30\n")),
    ] {
        let dir = tempfile::tempdir().expect("fake worker folder");
        // Respond only after hello for the framing cases. The first request
        // is always the 16-byte header and the 21-byte hello JSON body.
        let receive_hello = if name == "exit" {
            ""
        } else {
            "dd bs=1 count=37 of=/dev/null 2>/dev/null\n"
        };
        let (script, pid_file) = write_fake_worker(dir.path(), &format!("{receive_hello}{body}"));
        let watchdog_pid = pid_file.clone();
        let watchdog =
            PidWatchdog::from_pid(move || fake_worker_pid(&watchdog_pid), watchdog_deadline);
        let started = Instant::now();
        let result = Session::start(
            &script,
            Limits {
                hello: hello_deadline,
                ..limits()
            },
            &Cancel::default(),
        );
        let elapsed = started.elapsed();
        watchdog.finish();
        let error = match result {
            Ok((session, _)) => {
                drop(session);
                panic!("{name}: fake worker unexpectedly completed hello");
            }
            Err(error) => error,
        };
        eprintln!("xTB fake worker {name}: {elapsed:?}, {error:?}");
        match name {
            "exit" => assert!(
                matches!(error, Error::Exited { status, ref stderr }
                    if status.code() == Some(7) && stderr.contains("fake immediate exit")),
                "{error:?}"
            ),
            "truncated_eof" | "truncated_closed_stdout" => {
                assert!(matches!(error, Error::Exited { .. }), "{error:?}")
            }
            "garbage" => assert!(matches!(error, Error::Protocol(_)), "{error:?}"),
            "partial" => assert!(matches!(error, Error::Timeout), "{error:?}"),
            _ => unreachable!(),
        }
        assert!(
            elapsed < watchdog_deadline,
            "{name}: returned after {elapsed:?}"
        );
        assert_pid_gone(fake_worker_pid(&pid_file).expect("fake worker PID"));
    }
}

#[cfg(unix)]
mod protocol_v2 {
    use super::*;
    use protocol::{
        ArrayHeader, ArrayName, Computed, GfnFfReport, GfnFfSettings, HessianRequest,
        OrbitalOutput, ScfReport, SessionSettings, VibrationsReport,
    };
    use reshiki_qcdata::{
        AtomicCharges, BasisSet, Beta, ChargeScheme, OrbitalSet, Orbitals, Shell, Thermo,
    };

    const PROMPT: Duration = Duration::from_secs(2);

    fn energy_result(open: &Open, opened: &Opened) -> Computed {
        let gfnff = open.method == Method::Gfnff;
        Computed {
            method: opened.method,
            conventions: opened.conventions,
            engine_conventions: opened.engine_conventions.clone(),
            numerics: opened.numerics.clone(),
            settings: SessionSettings {
                charge: open.charge,
                uhf: opened.uhf,
                solvent: open.solvent.clone(),
                scf: open.scf.clone(),
                gfnff: gfnff.then(|| GfnFfSettings {
                    reference_positions_bohr: open
                        .positions
                        .iter()
                        .map(|p| p.map(|x| x / reshiki_qcdata::ANGSTROM_PER_BOHR))
                        .collect(),
                    fragment_charges: vec![open.charge],
                }),
                threads: open.threads,
            },
            energy: -5.125,
            energy_terms: Default::default(),
            gradient: None,
            charges: Some(AtomicCharges {
                scheme: if gfnff {
                    ChargeScheme::Eeq
                } else {
                    ChargeScheme::Mulliken
                },
                values: vec![0.0; opened.atoms],
            }),
            dipole_e_bohr: Some([0.0, 0.0, 0.875]),
            quadrupole_e_bohr2: (!gfnff).then_some([0.0; 6]),
            bond_orders: None,
            orbitals: None,
            homo_lumo_gap_ev: (!gfnff).then_some(14.0),
            near_degenerate: false,
            scf: (!gfnff).then(|| ScfReport {
                iterations: 4,
                start: "eeq".into(),
                electronic_temperature_k: 300.0,
                retry: None,
            }),
            composite: None,
            gfnff: gfnff.then(|| GfnFfReport {
                fragments: vec![0; opened.atoms],
            }),
            hessian_method: None,
            vibrations: None,
            thermo: None,
            arrays: vec![],
            seconds: 0.01,
            hessian: vec![],
            dipole_derivatives: vec![],
            normal_modes: vec![],
        }
    }

    fn array_header(name: ArrayName, shape: [u32; 2]) -> ArrayHeader {
        let values = u64::from(shape[0]) * u64::from(shape[1]);
        ArrayHeader {
            name,
            shape,
            chunks: values.div_ceil(protocol::CHUNK_VALUES as u64) as u32,
        }
    }

    struct WaterHessian {
        open: Open,
        opened: Opened,
        request: Compute,
        result: Computed,
    }

    impl WaterHessian {
        fn new(orbitals: OrbitalOutput) -> Self {
            let open = molecule("water");
            let opened = fake_opened(&open, Some(6));
            let mut result = energy_result(&open, &opened);
            result.gradient = Some(vec![[0.0; 3]; 3]);
            result.hessian_method = Some("analytic".into());
            result.vibrations = Some(VibrationsReport {
                conventions: "axtb".into(),
                frequencies_cm1: vec![1600.0, 3600.0, 3700.0],
                ir_intensities_km_mol: Some(vec![1.25, 3.5, 4.75]),
                reduced_masses_amu: vec![1.0; 3],
                force_constants_mdyn_per_angstrom: vec![1.0, 3.0, 4.0],
                masses_amu: vec![16.0, 1.0, 1.0],
                linear: false,
                point_group: Some("c2v".into()),
            });
            result.thermo = Some(Thermo {
                temperature_k: 298.15,
                pressure_pa: 101_325.0,
                zero_point_energy_hartree: 0.02,
                enthalpy_correction_hartree: 0.025,
                entropy_hartree_per_k: 0.000_07,
                gibbs_correction_hartree: 0.025 - 298.15 * 0.000_07,
                heat_capacity_hartree_per_k: Some(0.000_04),
                frequency_scale: 1.0,
                rotor_cutoff_cm1: 100.0,
                imaginary_cutoff_cm1: 20.0,
                symmetry_number: 2,
                point_group: None,
            });
            result.hessian = (0..9)
                .flat_map(|row| (0..9).map(move |col| f64::from(row + col) / 13.0))
                .collect();
            result.hessian[0] = -0.0;
            result.dipole_derivatives = (0..27).map(|i| f64::from(i) / 17.0).collect();
            result.normal_modes = vec![-0.0; 27];
            for (index, row) in result
                .normal_modes
                .as_chunks_mut::<9>()
                .0
                .iter_mut()
                .enumerate()
            {
                row[index] = 0.6;
                row[index + 3] = 0.8;
            }
            result.arrays = vec![
                array_header(ArrayName::Hessian, [9, 9]),
                array_header(ArrayName::DipoleDerivatives, [9, 3]),
                array_header(ArrayName::NormalModes, [3, 9]),
            ];
            if orbitals == OrbitalOutput::All {
                result.orbitals = Some(Orbitals {
                    basis: Some(BasisSet {
                        shells: [(0, 0), (0, 1), (1, 0), (2, 0)]
                            .into_iter()
                            .map(|(atom, l)| Shell {
                                atom,
                                l,
                                pure: false,
                                exponents_per_bohr2: vec![1.0],
                                coefficients: vec![1.0],
                            })
                            .collect(),
                    }),
                    core_electrons: vec![2, 0, 0],
                    alpha: OrbitalSet {
                        energies_hartree: vec![-1.0, -0.8, -0.6, -0.4, 0.1, 0.2],
                        occupations: vec![2.0, 2.0, 2.0, 2.0, 0.0, 0.0],
                        symmetries: vec![],
                        rows: (0..6).collect(),
                        coefficients: (0..36).map(|i| f64::from(i) / 19.0).collect(),
                    },
                    beta: Beta::Restricted,
                });
                result
                    .arrays
                    .push(array_header(ArrayName::CoefficientsAlpha, [6, 6]));
            }
            let request = Compute {
                hessian: Some(HessianRequest::default()),
                orbitals,
                ..compute(None, false)
            };
            Self {
                open,
                opened,
                request,
                result,
            }
        }

        fn chunks(&self) -> Vec<Vec<u8>> {
            self.result
                .arrays
                .iter()
                .map(|array| protocol::encode_chunk(0, self.result.array_values(array.name)))
                .collect()
        }

        fn response(&self, id: u64, chunks: Vec<Vec<u8>>) -> Vec<u8> {
            let mut bytes = ok_frame(id, &self.result);
            bytes.extend(chunks.into_iter().flatten());
            bytes
        }

        fn exchange(
            &self,
            case: &str,
            response: &[u8],
            limit: Duration,
        ) -> Result<Computed, Error> {
            let dir = tempfile::tempdir().expect("fake worker folder");
            let mut session = open_scripted_worker(dir.path(), &self.open, &self.opened, response);
            let pid = session.pid();
            let result = timely_compute(&mut session, &self.request, case, limit);
            if result.is_ok() {
                assert!(kill(pid, "-0"), "{case}: worker exited after a valid reply");
                drop(session);
            }
            assert_pid_gone(pid);
            result
        }

        fn rejects(&self, case: &str, response: &[u8], limit: Duration) {
            let result = self.exchange(case, response, limit);
            assert!(
                matches!(result, Err(Error::Protocol(_))),
                "{case}: {result:?}"
            );
        }
    }

    fn timely_compute(
        session: &mut Session,
        request: &Compute,
        case: &str,
        limit: Duration,
    ) -> Result<Computed, Error> {
        let pid = session.pid();
        let watchdog = PidWatchdog::from_pid(move || Some(pid), Duration::from_secs(5));
        let started = Instant::now();
        let result = session.compute(request, &Cancel::default());
        let elapsed = started.elapsed();
        watchdog.finish();
        assert!(elapsed < limit, "{case}: compute took {elapsed:?}");
        result
    }

    #[test]
    fn valid_binary_arrays_round_trip_bit_for_bit() {
        for orbitals in [OrbitalOutput::None, OrbitalOutput::All] {
            let fixture = WaterHessian::new(orbitals);
            let response = fixture.response(3, fixture.chunks());
            let result = fixture
                .exchange("valid Hessian", &response, PROMPT)
                .expect("accept valid response and chunks");
            assert_eq!(result.arrays, fixture.result.arrays);
            assert_eq!(
                result
                    .orbitals
                    .as_ref()
                    .map(|orbitals| &orbitals.alpha.rows),
                fixture
                    .result
                    .orbitals
                    .as_ref()
                    .map(|orbitals| &orbitals.alpha.rows)
            );
            for array in &result.arrays {
                let actual = result.array_values(array.name);
                let expected = fixture.result.array_values(array.name);
                assert_eq!(actual.len(), expected.len(), "{:?}", array.name);
                for (actual, expected) in actual.iter().zip(expected) {
                    assert_eq!(actual.to_bits(), expected.to_bits(), "{:?}", array.name);
                }
            }
        }
    }

    #[test]
    fn late_open_reply_id_is_rejected() {
        let fixture = WaterHessian::new(OrbitalOutput::None);
        fixture.rejects("late reply", &fixture.response(2, fixture.chunks()), PROMPT);
    }

    #[test]
    fn extra_chunk_after_announced_chunks_is_rejected() {
        let fixture = WaterHessian::new(OrbitalOutput::None);
        let mut chunks = fixture.chunks();
        chunks.push(chunks[0].clone());
        fixture.rejects("extra chunk", &fixture.response(3, chunks), PROMPT);
    }

    #[test]
    fn second_json_reply_after_announced_chunks_is_rejected() {
        let fixture = WaterHessian::new(OrbitalOutput::None);
        let mut response = fixture.response(3, fixture.chunks());
        response.extend(ok_frame(3, &fixture.result));
        fixture.rejects("second JSON reply", &response, PROMPT);
    }

    #[test]
    fn mismatched_array_dimensions_are_rejected() {
        for (case, orbitals, index, shape) in [
            ("Hessian columns", OrbitalOutput::None, 0, [9, 8]),
            ("normal mode rows", OrbitalOutput::None, 2, [2, 9]),
            ("coefficient columns", OrbitalOutput::All, 3, [6, 5]),
        ] {
            let mut fixture = WaterHessian::new(orbitals);
            fixture.result.arrays[index].shape = shape;
            fixture.rejects(case, &fixture.response(3, fixture.chunks()), PROMPT);
        }

        let fixture = WaterHessian::new(OrbitalOutput::None);
        let mut chunks = fixture.chunks();
        chunks[0] = protocol::encode_chunk(0, &fixture.result.hessian[..80]);
        fixture.rejects("Hessian value count", &fixture.response(3, chunks), PROMPT);
    }

    #[test]
    fn oversized_array_announcements_are_rejected_without_waiting_for_chunks() {
        let coefficient_columns = (reshiki_qcdata::MAX_COEFFICIENTS / 6 + 1) as u32;
        for (case, orbitals, index, shape) in [
            (
                "oversized Hessian",
                OrbitalOutput::None,
                0,
                [600_000, 600_000],
            ),
            (
                "oversized coefficients",
                OrbitalOutput::All,
                3,
                [6, coefficient_columns],
            ),
        ] {
            let mut fixture = WaterHessian::new(orbitals);
            let name = fixture.result.arrays[index].name;
            fixture.result.arrays[index] = array_header(name, shape);
            fixture.rejects(case, &fixture.response(3, vec![]), Duration::from_secs(1));
        }
    }

    #[test]
    fn wrong_chunk_index_and_non_finite_values_are_rejected() {
        let fixture = WaterHessian::new(OrbitalOutput::None);
        let mut non_finite = fixture.result.hessian.clone();
        non_finite[40] = f64::NAN;
        for (case, chunk) in [
            (
                "wrong chunk index",
                protocol::encode_chunk(1, &fixture.result.hessian),
            ),
            ("non-finite value", protocol::encode_chunk(0, &non_finite)),
        ] {
            let mut chunks = fixture.chunks();
            chunks[0] = chunk;
            fixture.rejects(case, &fixture.response(3, chunks), PROMPT);
        }
    }

    #[test]
    fn unsupported_gfnff_requests_leave_the_next_response_for_energy() {
        let dir = tempfile::tempdir().expect("fake worker folder");
        let open = Open {
            method: Method::Gfnff,
            ..molecule("water")
        };
        let opened = fake_opened(&open, None);
        let energy = energy_result(&open, &opened);
        let response = ok_frame(3, &energy);
        let mut session = open_scripted_worker(dir.path(), &open, &opened, &response);
        let pid = session.pid();
        for (case, request) in [
            (
                "GFN-FF orbitals",
                Compute {
                    orbitals: OrbitalOutput::All,
                    ..compute(None, false)
                },
            ),
            (
                "GFN-FF bond orders",
                Compute {
                    bond_orders: true,
                    ..compute(None, false)
                },
            ),
        ] {
            let result = timely_compute(&mut session, &request, case, PROMPT);
            assert!(
                matches!(result, Err(Error::Unsupported(_))),
                "{case}: {result:?}"
            );
        }
        let result = timely_compute(&mut session, &compute(None, false), "GFN-FF energy", PROMPT)
            .expect("unsupported requests left the energy reply unread");
        assert_eq!(result.energy.to_bits(), energy.energy.to_bits());
        drop(session);
        assert_pid_gone(pid);
    }

    #[test]
    fn hessian_atom_limit_leaves_the_next_response_for_energy() {
        let dir = tempfile::tempdir().expect("fake worker folder");
        let open = Open {
            numbers: vec![2; 201],
            positions: (0..201).map(|i| [f64::from(i) * 3.0, 0.0, 0.0]).collect(),
            ..molecule("water")
        };
        let opened = fake_opened(&open, Some(201));
        let energy = energy_result(&open, &opened);
        let response = ok_frame(3, &energy);
        let mut session = open_scripted_worker(dir.path(), &open, &opened, &response);
        let pid = session.pid();
        let request = Compute {
            hessian: Some(HessianRequest::default()),
            ..compute(None, false)
        };
        let result = timely_compute(&mut session, &request, "201-atom Hessian", PROMPT);
        assert!(matches!(result, Err(Error::Limit(_))), "{result:?}");
        let result = timely_compute(
            &mut session,
            &compute(None, false),
            "201-atom energy",
            PROMPT,
        )
        .expect("Hessian rejection left the energy reply unread");
        assert_eq!(result.energy.to_bits(), energy.energy.to_bits());
        drop(session);
        assert_pid_gone(pid);
    }

    #[test]
    fn chunk_where_json_is_expected_is_rejected() {
        let fixture = WaterHessian::new(OrbitalOutput::None);
        let mut response = protocol::encode_chunk(0, &fixture.result.hessian);
        response.extend(fixture.response(3, fixture.chunks()));
        fixture.rejects("chunk before JSON", &response, PROMPT);
    }
}

#[cfg(not(feature = "xtb"))]
#[test]
fn build_without_xtb_has_null_engine_and_a_clear_calc_error() {
    let (session, hello) =
        Session::start(executable(), limits(), &Cancel::default()).expect("start native worker");
    assert!(
        hello.engine.is_none(),
        "build without xTB advertises no engine"
    );
    assert!(hello.methods.is_empty());
    assert!(matches!(
        session.close(&Cancel::default()),
        Err(Error::Unavailable)
    ));
    let dir = tempfile::tempdir().expect("CLI working folder");
    let water = fixtures().join("water.xyz");
    let output = headless::run_cli(
        dir.path(),
        &[
            "calc",
            "--energy",
            "--gradient",
            water.to_str().expect("fixture path"),
        ],
        b"",
        Duration::from_secs(30),
    );
    assert_eq!(
        output.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stdout.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(
        output.stderr,
        b"reshiki: xTB is not included in this build\n"
    );
}
