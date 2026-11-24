//! The real application executable, its supervised xTB worker, and the calc CLI.
#![cfg(not(target_arch = "wasm32"))]

use crate::headless;
use reshiki_xtb_worker::client::{Limits, Session};
use std::{path::Path, time::Duration};

#[cfg(any(not(feature = "xtb"), unix))]
use reshiki_xtb_worker::client::Error;
#[cfg(feature = "xtb")]
use reshiki_xtb_worker::{
    client::Cancel,
    protocol::{Compute, Method, Open, Units},
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
        hello: Duration::from_secs(10),
        open: Duration::from_secs(120),
        compute: Duration::from_secs(120),
        ..Limits::default()
    }
}

#[cfg(feature = "xtb")]
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
        solvent: None,
        threads: 1,
        heap_bytes: limits().heap_bytes,
    }
}

#[cfg(feature = "xtb")]
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
    molecules: BTreeMap<String, Values>,
}

#[cfg(feature = "xtb")]
#[derive(Deserialize)]
struct Values {
    energy: f64,
    gradient: Vec<[f64; 3]>,
}

#[cfg(feature = "xtb")]
fn assert_reference(name: &str, path: &str, actual: &Values, expected: &Values) {
    assert_eq!(
        actual.gradient.len(),
        expected.gradient.len(),
        "{name}/{path}"
    );
    assert!(actual.energy.is_finite(), "{name}/{path}: nonfinite energy");
    let energy_difference = (actual.energy - expected.energy).abs();
    let mut maximum_gradient_difference = 0.0_f64;
    for (actual, expected) in actual.gradient.iter().zip(&expected.gradient) {
        for axis in 0..3 {
            assert!(
                actual[axis].is_finite(),
                "{name}/{path}: nonfinite gradient"
            );
            maximum_gradient_difference =
                maximum_gradient_difference.max((actual[axis] - expected[axis]).abs());
        }
    }
    eprintln!(
        "xTB reference {name}/{path}: |ΔE|={energy_difference:.16e} Eh, max |Δgradient|={maximum_gradient_difference:.16e} Eh/bohr"
    );
    assert!(
        energy_difference <= 1e-10,
        "{name}/{path}: |ΔE|={energy_difference:.16e} Eh"
    );
    assert!(
        maximum_gradient_difference <= 1e-9,
        "{name}/{path}: max |Δgradient|={maximum_gradient_difference:.16e} Eh/bohr"
    );
}

#[cfg(feature = "xtb")]
#[test]
fn worker_and_cli_match_reference_energy_and_gradients() {
    let reference: Reference = serde_json::from_str(include_str!("fixtures/xtb/reference.json"))
        .expect("xTB reference JSON");
    for (name, expected) in &reference.molecules {
        let (mut session, hello) =
            Session::start(executable(), limits()).expect("start xTB worker");
        assert!(
            hello.engine.is_some(),
            "feature-enabled worker has an engine"
        );
        session
            .open(&molecule(name), &Cancel::default())
            .expect("open fixture");
        let result = session
            .compute(&compute(None, true), &Cancel::default())
            .expect("compute fixture");
        assert_reference(
            name,
            "Session/angstrom",
            &Values {
                energy: result.energy,
                gradient: result.gradient.expect("requested gradient"),
            },
            expected,
        );
        session.close().expect("close worker");

        let dir = tempfile::tempdir().expect("CLI working folder");
        let xyz = fixtures().join(format!("{name}.xyz"));
        let output = headless::run_cli(
            dir.path(),
            &[
                "calc",
                "--energy",
                "--gradient",
                xyz.to_str().expect("fixture path"),
            ],
            b"",
            Duration::from_secs(120),
        );
        assert!(
            output.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = std::str::from_utf8(&output.stdout).expect("UTF-8 calc output");
        assert_eq!(stdout.lines().count(), 1, "one calc result line");
        let result: serde_json::Value = serde_json::from_str(stdout).expect("calc JSON output");
        assert_eq!(result["method"], "gfn2");
        assert_eq!(result["numerics"], "gfn2:1");
        assert_eq!(result["engine"]["name"], "axtb");
        assert_eq!(result["engine"]["rev"], reshiki_xtb_worker::AXTB_REV);
        let actual: Values = serde_json::from_value(result).expect("calc energy and gradient");
        assert_reference(name, "CLI", &actual, expected);
    }
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

// A 288-atom cluster gives cancellation and SIGKILL a stable window during
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

#[cfg(all(feature = "xtb", unix))]
#[test]
fn cancellation_during_compute_kills_and_reaps_the_worker() {
    let (mut session, _) = Session::start(executable(), limits()).expect("start worker");
    session
        .open(&long_running_molecule(), &Cancel::default())
        .expect("open caffeine cluster");
    let pid = session.pid();
    let watchdog = PidWatchdog::new(pid);
    let cancel = Cancel::default();
    let started = Instant::now();
    let result = thread::scope(|scope| {
        scope.spawn(|| {
            thread::sleep(Duration::from_millis(100));
            cancel.cancel();
        });
        session.compute(&compute(None, true), &cancel)
    });
    let elapsed = started.elapsed();
    watchdog.finish();
    eprintln!("xTB cancellation during compute: {elapsed:?}");
    assert!(matches!(result, Err(Error::Cancelled)), "{result:?}");
    assert!(
        elapsed < Duration::from_secs(2),
        "cancellation took {elapsed:?}"
    );
    assert_pid_gone(pid);
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
        Self::from_pid(move || Some(pid))
    }

    fn from_pid(pid: impl FnOnce() -> Option<u32> + Send + 'static) -> Self {
        let (stop, receiver) = std::sync::mpsc::channel();
        let thread = thread::spawn(move || {
            if receiver.recv_timeout(Duration::from_secs(2)).is_ok() {
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
#[test]
fn fake_workers_exit_garbage_and_partial_response_never_hang() {
    use std::os::unix::fs::PermissionsExt;
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
        let script = dir.path().join("worker.sh");
        let pid_file = dir.path().join("pid");
        let quoted_pid_file = pid_file.to_string_lossy().replace('\'', "'\\''");
        // Respond only after hello for the framing cases. The first request
        // is always the 16-byte header and the 21-byte hello JSON body.
        let receive_hello = if name == "exit" {
            ""
        } else {
            "dd bs=1 count=37 of=/dev/null 2>/dev/null\n"
        };
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$$\" > '{quoted_pid_file}'\n{receive_hello}{body}"
            ),
        )
        .expect("write fake worker");
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700))
            .expect("executable script");
        let watchdog_pid = pid_file.clone();
        let watchdog = PidWatchdog::from_pid(move || {
            std::fs::read_to_string(watchdog_pid)
                .ok()?
                .trim()
                .parse()
                .ok()
        });
        let started = Instant::now();
        let result = Session::start(
            &script,
            Limits {
                hello: Duration::from_millis(750),
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
            elapsed < Duration::from_secs(2),
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
