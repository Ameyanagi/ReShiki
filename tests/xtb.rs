//! The real application executable, its supervised xTB worker, and the calc CLI.
#![cfg(not(target_arch = "wasm32"))]

use crate::headless;
use reshiki_xtb_worker::client::{Error, Limits, Session};
use std::{path::Path, time::Duration};

#[cfg(feature = "xtb")]
use reshiki_xtb_worker::protocol::{
    ErrorKind, HARTREE_TO_EV, NEAR_DEGENERATE_GAP_EV, Solvent, SolventModel,
};
#[cfg(any(feature = "xtb", unix))]
use reshiki_xtb_worker::{
    client::Cancel,
    protocol::{Compute, Conventions, Method, Open, Units},
};
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
            let (mut session, hello) =
                Session::start(executable(), limits()).expect("start xTB worker");
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
            session.close().expect("close worker");

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
#[test]
fn reused_session_matches_fresh_sessions_for_three_displaced_geometries() {
    let (mut reused, _) = Session::start(executable(), limits()).expect("start reusable worker");
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
        let (mut fresh, _) = Session::start(executable(), limits()).expect("start fresh worker");
        fresh
            .open(&geometry, &Cancel::default())
            .expect("open displaced geometry");
        let independent = fresh
            .compute(&compute(None, false), &Cancel::default())
            .expect("fresh compute");
        fresh.close().expect("close fresh worker");
        let difference = (cached.energy - independent.energy).abs();
        maximum_difference = maximum_difference.max(difference);
        eprintln!(
            "xTB reused/fresh geometry {step}: |ΔE|={difference:.16e} Eh, SCF iterations {}/{}",
            cached.scf_iterations, independent.scf_iterations
        );
    }
    reused.close().expect("close reused worker");
    assert!(
        maximum_difference <= 1e-10,
        "maximum reused/fresh |ΔE|={maximum_difference:.16e} Eh"
    );
}

#[cfg(feature = "xtb")]
#[test]
fn invalid_client_input_is_rejected_without_losing_the_session() {
    let (mut session, _) = Session::start(executable(), limits()).expect("start worker");
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
        assert!(matches!(result, Err(Error::Input(_))), "{name}: {result:?}");
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
        let result = session.compute(&compute(Some(positions), false), &Cancel::default());
        assert!(matches!(result, Err(Error::Input(_))), "{result:?}");
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
    session.close().expect("close worker after invalid input");
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
            }),
        ),
    ] {
        let (mut session, _) = Session::start(executable(), limits()).expect("start worker");
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
        session.close().expect("close worker after rejected open");
    }
}

#[cfg(feature = "xtb")]
#[test]
fn worker_checks_cef3_spin_against_gfn2_valence_electrons_on_open() {
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
    let (mut session, _) = Session::start(executable(), limits()).expect("start worker");
    session
        .open(&cef3, &Cancel::default())
        .expect("open neutral CeF3 with UHF 0");
    session.close().expect("close worker");

    let (mut session, _) = Session::start(executable(), limits()).expect("start worker");
    cef3.uhf = Some(1);
    let result = session.open(&cef3, &Cancel::default());
    assert!(
        matches!(
            result,
            Err(Error::Engine {
                kind: ErrorKind::Input,
                ..
            })
        ),
        "neutral CeF3 with UHF 1: {result:?}"
    );
    session.close().expect("close worker after rejected open");
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

    let (mut session, _) = Session::start(executable(), limits()).expect("start worker");
    let pid = session.pid();
    let invalid_open = Open {
        threads: 0,
        ..water.clone()
    };
    let result = session.open(&invalid_open, &cancel);
    assert!(matches!(result, Err(Error::Cancelled)), "{result:?}");
    assert_pid_gone(pid);

    let (mut session, _) = Session::start(executable(), limits()).expect("start worker");
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
    use reshiki_xtb_worker::protocol::{Request, RequestOp};

    let dir = tempfile::tempdir().expect("fake worker folder");
    let received = dir.path().join("compute-received");
    let quoted_received = received.to_string_lossy().replace('\'', "'\\''");
    let open = molecule("water");
    let request = compute(None, false);
    let receive = |id, op| {
        let length = reshiki_xtb_worker::protocol::encode_frame(&Request { id, op })
            .expect("encode fake worker request")
            .len();
        format!(
            "received=$(dd bs=1 count={length} 2>/dev/null | wc -c)\n[ \"$received\" -eq {length} ] || exit 8\n"
        )
    };
    let response = |value| {
        let frame = reshiki_xtb_worker::protocol::encode_frame(&value)
            .expect("encode fake worker response");
        let escaped: String = frame.iter().map(|byte| format!("\\{byte:03o}")).collect();
        format!("printf '{escaped}'\n")
    };
    let body = format!(
        "{}{}{}{}{}: > '{quoted_received}'\nexec sleep 30\n",
        receive(1, RequestOp::Hello {}),
        response(serde_json::json!({
            "id": 1,
            "ok": {
                "protocol": 1,
                "engine": {
                    "name": "fake", "version": "1", "license": "MIT",
                    "source": "test", "rev": "test"
                },
                "methods": ["gfn2"],
                "operations": ["energy", "gradient"],
                "max_atoms": reshiki_xtb_worker::protocol::MAX_ATOMS,
            }
        })),
        receive(2, RequestOp::Open(open.clone())),
        response(serde_json::json!({
            "id": 2,
            "ok": {
                "atoms": open.numbers.len(),
                "method": "gfn2",
                "conventions": "axtb",
                "numerics": "gfn2:1"
            }
        })),
        receive(3, RequestOp::Compute(request.clone())),
    );
    let (script, _) = write_fake_worker(dir.path(), &body);
    let (mut session, _) = Session::start(&script, limits()).expect("start fake worker");
    session
        .open(&open, &Cancel::default())
        .expect("open fake worker");
    let pid = session.pid();
    let watchdog = PidWatchdog::from_pid(move || Some(pid), Duration::from_secs(8));
    let cancel = Cancel::default();
    let cancel_helper = cancel.clone();
    let helper = thread::spawn(move || {
        let waiting = Instant::now();
        while !received.exists() {
            assert!(
                waiting.elapsed() < Duration::from_secs(5),
                "compute frame was not received"
            );
            thread::sleep(Duration::from_millis(5));
        }
        let started = Instant::now();
        cancel_helper.cancel();
        started
    });
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
    let (mut session, _) = Session::start(executable(), limits()).expect("start worker");
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
#[test]
fn fake_workers_exit_garbage_and_partial_response_never_hang() {
    let hello_deadline = Duration::from_secs(5);
    let watchdog_deadline = hello_deadline + Duration::from_secs(2);
    for (name, body) in [
        ("exit", "echo 'fake immediate exit' >&2\nexit 7\n"),
        (
            "garbage",
            "printf 'this is not an xTB protocol frame'\nexec sleep 30\n",
        ),
        (
            "truncated_eof",
            "printf 'RSHXTBWK\\001\\000\\000\\000\\020\\000\\000\\000{\"id\":1'\n",
        ),
        (
            "truncated_closed_stdout",
            "printf 'RSHXTBWK\\001\\000\\000\\000\\020\\000\\000\\000{\"id\":1'\nexec 1>&-\nexec sleep 30\n",
        ),
        // A complete valid header announcing 16 body bytes, followed by only
        // seven bytes and a live process: this must reach the hello deadline.
        (
            "partial",
            "printf 'RSHXTBWK\\001\\000\\000\\000\\020\\000\\000\\000{\"id\":1'\nexec sleep 30\n",
        ),
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
        let watchdog = PidWatchdog::from_pid(
            move || {
                std::fs::read_to_string(watchdog_pid)
                    .ok()?
                    .trim()
                    .parse()
                    .ok()
            },
            watchdog_deadline,
        );
        let started = Instant::now();
        let result = Session::start(
            &script,
            Limits {
                hello: hello_deadline,
                ..limits()
            },
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
        let pid = std::fs::read_to_string(pid_file)
            .expect("fake worker PID")
            .trim()
            .parse()
            .expect("PID number");
        assert_pid_gone(pid);
    }
}

#[cfg(not(feature = "xtb"))]
#[test]
fn build_without_xtb_has_null_engine_and_a_clear_calc_error() {
    let (session, hello) = Session::start(executable(), limits()).expect("start native worker");
    assert!(
        hello.engine.is_none(),
        "build without xTB advertises no engine"
    );
    assert!(hello.methods.is_empty());
    assert!(hello.operations.is_empty());
    assert!(matches!(session.close(), Err(Error::Unavailable)));
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
