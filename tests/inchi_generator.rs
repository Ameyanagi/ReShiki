use anyhow::Context;
use reshiki::chemistry::{
    inchi::{
        generator::{self, Error},
        input,
    },
    stereo::{Point3, perception::State},
};
use serde::Deserialize;
use std::{
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

fn helper(name: &str) -> anyhow::Result<Option<PathBuf>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = root.join("artifacts/inchi-helper").join(if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    });
    if path.is_file() {
        return Ok(Some(path));
    }
    anyhow::ensure!(
        std::env::var_os("RESHIKI_REQUIRE_INCHI_HELPER").is_none(),
        "Build the pinned native development helper first"
    );
    eprintln!("Skipping optional InChI helper check; build scripts/build_inchi_helper.py first");
    Ok(None)
}

#[derive(Deserialize)]
struct Expected {
    inchi: String,
    status: i32,
    message: String,
    log: String,
    auxiliary: String,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    state: State,
    positions: Option<Vec<Point3>>,
    expected: Option<Expected>,
}
fn log_content(value: &str) -> String {
    // Empty-input help includes the native compiler/platform build stamp.
    // Preserve every chemical diagnostic and the version line.
    value
        .lines()
        .filter(|line| !line.contains(" Build ("))
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test]
async fn standalone_kernel_matches_original_native_generation() -> anyhow::Result<()> {
    let Some(path) = helper("reshiki-inchi-helper")? else {
        return Ok(());
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut process = Command::new(python)
        .arg(root.join("tests/inchi_generator_reference.py"))
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines = BufReader::new(
        process
            .stdout
            .take()
            .context("Missing native generation output")?,
    )
    .lines();
    let header: serde_json::Value =
        serde_json::from_str(&lines.next().context("Missing native generation header")??)?;
    assert_eq!(header["rdkit_version"], reshiki::chemistry::RDKIT_VERSION);
    let (mut accepted, mut warnings, mut empty, mut preparation_errors, mut adapter_empty) =
        (0, 0, 0, 0, 0);
    let mut failures = Vec::new();
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        let prepared = input::prepare(&case.state, case.positions.as_deref());
        match (prepared, case.expected) {
            (Err(_), None) => preparation_errors += 1,
            (Err(input::Error::NativeEmpty(input::NativeEmpty::TooManyNeighbors { .. })), Some(expected))
                // The original adapter returns before writing returnCode;
                // its Python wrapper exposes an uninitialized integer here.
                if expected.inchi.is_empty() && expected.message.is_empty() && expected.log.is_empty() && expected.auxiliary.is_empty() => adapter_empty += 1,
            (Ok(input), Some(expected)) => {
                let actual = generator::generate(&path, &input, Duration::from_secs(30))
                    .await
                    .with_context(|| case.name.clone())?;
                let agrees = actual.inchi == expected.inchi
                    && i32::from(actual.status.code()) == expected.status
                    && actual.message == expected.message
                    && actual.auxiliary == expected.auxiliary
                    && log_content(&actual.log) == log_content(&expected.log);
                if !agrees && failures.len() < 20 {
                    failures.push(format!(
                        "{}: actual={actual:?}, expected=({}, {:?}, {:?}, {:?}, {:?})",
                        case.name,
                        expected.status,
                        expected.inchi,
                        expected.message,
                        expected.log,
                        expected.auxiliary
                    ));
                }
                if actual.inchi.is_empty() {
                    empty += 1;
                } else {
                    accepted += 1;
                }
                if actual.status == generator::Status::Warning {
                    warnings += 1;
                }
            }
            (actual, expected) => failures.push(format!(
                "{}: preparation={actual:?}, native accepted={}",
                case.name,
                expected.is_some()
            )),
        }
    }
    assert!(process.wait()?.success());
    println!(
        "Standalone InChI: {accepted} identifiers, {warnings} warnings, {empty} empty results, {preparation_errors} preparation failures, {adapter_empty} native early-empty adapter results"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(accepted > 5000 && warnings > 100 && empty > 10 && preparation_errors > 20);
    assert_eq!(adapter_empty, 2);
    Ok(())
}

fn methane() -> input::Input {
    input::Input {
        has_coordinates: false,
        atoms: vec![input::Atom {
            position: [0.0; 3],
            element: "C".into(),
            isotopic_mass: 0,
            charge: 0,
            hydrogens: [-1, 0, 0, 0],
            radical: 0,
            bonds: vec![],
        }],
        stereo: vec![],
    }
}

#[tokio::test]
async fn helper_failures_are_bounded_and_typed() -> anyhow::Result<()> {
    let Some(stub) = helper("inchi-helper-stub")? else {
        return Ok(());
    };
    for mode in [
        "ok",
        "protocol",
        "version",
        "truncated",
        "rejected",
        "status",
        "string-length",
        "nonstandard",
        "trailing",
        "utf8",
        "exit",
        "oversized",
        "stderr",
        "hang",
    ] {
        let dir = tempfile::tempdir()?;
        let executable = dir.path().join(if cfg!(windows) {
            format!("{mode}.exe")
        } else {
            mode.into()
        });
        std::fs::copy(&stub, &executable)?;
        let result = generator::generate(
            &executable,
            &methane(),
            if mode == "hang" {
                Duration::from_millis(100)
            } else {
                Duration::from_secs(5)
            },
        )
        .await;
        let matches = match (mode, &result) {
            ("ok", Ok(output)) => output.inchi == "InChI=1S/CH4/h1H4",
            ("version", Err(Error::Version(_)))
            | ("rejected", Err(Error::Rejected(_)))
            | ("oversized" | "stderr" | "string-length", Err(Error::Limit(_)))
            | ("hang", Err(Error::Timeout)) => true,
            (
                "protocol" | "truncated" | "status" | "nonstandard" | "trailing" | "utf8",
                Err(Error::Protocol(_)),
            ) => true,
            ("exit", Err(Error::Exit { code: Some(17), .. })) => true,
            _ => false,
        };
        assert!(matches, "{mode}: {result:?}");
        #[cfg(unix)]
        if mode == "hang" {
            assert_stopped_if_started(dir.path()).await?;
        } else if matches!(mode, "oversized" | "stderr") {
            let pid: u32 = std::fs::read_to_string(dir.path().join("pid"))?.parse()?;
            assert_stopped(pid).await?;
        }
    }
    Ok(())
}

#[cfg(unix)]
async fn assert_stopped(pid: u32) -> anyhow::Result<()> {
    anyhow::ensure!(pid > 0, "Invalid helper process ID");
    for _ in 0..100 {
        let live = Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?
            .success();
        if !live {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    anyhow::bail!("Cancelled helper {pid} is still alive")
}

#[cfg(unix)]
async fn assert_stopped_if_started(directory: &Path) -> anyhow::Result<()> {
    // A short deadline can expire before a cold executable writes its marker.
    // The explicit startup handshake below separately proves cancellation of
    // a running child; timeout tests also accept cancellation before startup.
    match std::fs::read_to_string(directory.join("pid")) {
        Ok(value) if value.is_empty() => Ok(()),
        Ok(value) => assert_stopped(value.parse()?).await,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(unix)]
#[tokio::test]
async fn dropping_generation_kills_the_child_and_bad_input_never_spawns() -> anyhow::Result<()> {
    let Some(stub) = helper("inchi-helper-stub")? else {
        return Ok(());
    };
    let dir = tempfile::tempdir()?;
    let executable = dir.path().join("hang");
    std::fs::copy(&stub, &executable)?;
    let mut invalid = methane();
    invalid.atoms.first_mut().context("Missing atom")?.position[0] = f64::NAN;
    assert!(matches!(
        generator::generate(&executable, &invalid, Duration::from_secs(1)).await,
        Err(Error::Input(_))
    ));
    assert!(!dir.path().join("pid").exists());
    let task = tokio::spawn(async move {
        generator::generate(&executable, &methane(), generator::MAX_TIMEOUT).await
    });
    let started = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if let Ok(value) = std::fs::read_to_string(dir.path().join("pid"))
                && let Ok(pid) = value.parse::<u32>()
            {
                break Ok::<_, anyhow::Error>(pid);
            }
            anyhow::ensure!(
                !task.is_finished(),
                "Helper exited before its startup marker"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    // Always cancel and join, even if startup fails, so a failed test cannot
    // leave a detached task or helper running.
    task.abort();
    let completion = task.await;
    let pid = started.context("Helper did not publish its startup marker")??;
    assert!(matches!(completion, Err(error) if error.is_cancelled()));
    assert_stopped(pid).await?;
    // A helper which never drains stdin must not bypass the deadline.
    std::fs::remove_file(dir.path().join("pid"))?;
    let executable = dir.path().join("no-read");
    std::fs::copy(&stub, &executable)?;
    let mut large = methane();
    let atom = large.atoms.first().context("Missing atom")?.clone();
    large.atoms.resize(32767, atom);
    assert!(matches!(
        generator::generate(&executable, &large, Duration::from_millis(100)).await,
        Err(Error::Timeout)
    ));
    assert_stopped_if_started(dir.path()).await?;
    Ok(())
}
