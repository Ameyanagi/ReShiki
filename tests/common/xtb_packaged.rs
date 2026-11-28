//! Shared black-box calculation assertions for a supplied application executable.
#[path = "process_output.rs"]
mod process_output;

use reshiki_xtb_worker::{
    AXTB_REV,
    client::{Cancel, Limits, Session},
    protocol::{Compute, Conventions, Method, Open, Units},
};
use std::{
    path::Path,
    process::{Command, Stdio},
    time::Duration,
};

pub fn check_relocated_app(source: &Path) {
    let folder = tempfile::Builder::new()
        .prefix("ReShiki unsigned xTB ")
        .tempdir()
        .unwrap();
    let app = folder.path().join(if cfg!(windows) {
        "Only application.exe"
    } else {
        "Only application"
    });
    std::fs::copy(source, &app).unwrap();
    let xyz = folder.path().join("water input.xyz");
    let water = include_str!("../fixtures/xtb/water.xyz");
    std::fs::write(&xyz, water).unwrap();
    // Parse only this fixed fixture so the artifact harness has no app dependency.
    let (numbers, positions): (Vec<u8>, Vec<[f64; 3]>) = water
        .lines()
        .skip(2)
        .map(|line| {
            let mut fields = line.split_whitespace();
            let number = match fields.next().unwrap() {
                "O" => 8,
                "H" => 1,
                other => panic!("unexpected water-fixture element {other}"),
            };
            let position = std::array::from_fn(|_| fields.next().unwrap().parse().unwrap());
            (number, position)
        })
        .unzip();
    let reference: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/xtb/reference.json")).unwrap();
    let expected = reference["axtb"]["water"]["energy"].as_f64().unwrap();

    for (method, name) in [
        (Method::Gfn2, "gfn2"),
        (Method::Gxtb, "gxtb"),
        (Method::Gfnff, "gfnff"),
    ] {
        let cancel = Cancel::default();
        let (mut session, hello) = Session::start(&app, Limits::default(), &cancel).unwrap();
        assert_eq!(hello.engine.unwrap().rev, AXTB_REV);
        assert!(hello.methods.contains(&method));
        session
            .open(
                &Open {
                    numbers: numbers.clone(),
                    positions: positions.clone(),
                    units: Units::Angstrom,
                    charge: 0,
                    uhf: Some(0),
                    method,
                    conventions: Conventions::Axtb,
                    solvent: None,
                    scf: Default::default(),
                    gfnff: None,
                    threads: 1,
                    heap_bytes: Limits::default().heap_bytes,
                },
                &cancel,
            )
            .unwrap();
        let computed = session
            .compute(
                &Compute {
                    gradient: true,
                    ..Compute::default()
                },
                &cancel,
            )
            .unwrap();
        assert!(computed.energy.is_finite());
        assert_eq!(computed.gradient.as_ref().unwrap().len(), 3);
        session.close(&cancel).unwrap();

        let mut command = Command::new(&app);
        command
            .args(["--cli", "calculate", "--method", name, "--threads", "1"])
            .arg(&xyz)
            .current_dir(folder.path())
            .env("AXTB_LAPACK", "not-a-shipped-lapack-library")
            .env("AXTB_LAPACK_BLAS", "not-a-shipped-blas-library")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let output =
            process_output::wait_with_watchdog(command.spawn().unwrap(), Duration::from_secs(120));
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let value = &json["value"];
        assert_eq!(value["method"], name);
        assert_eq!(value["engine"]["rev"], AXTB_REV);
        assert!((value["energy_hartree"].as_f64().unwrap() - computed.energy).abs() < 1e-9);
        if method == Method::Gfn2 {
            assert!((computed.energy - expected).abs() < 1e-9);
        }
    }

    let child = Command::new(&app)
        .args(["--cli", "calculate", "--task", "opt+freq", "--threads", "1"])
        .arg(&xyz)
        .current_dir(folder.path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let output = process_output::wait_with_watchdog(child, Duration::from_secs(120));
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["value"]["optimization"]["converged"], true);
    assert_eq!(
        json["value"]["frequencies"]["frequencies_cm1"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
}
