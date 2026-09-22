//! Native import records captured independently from the original InchiToMol.
use anyhow::Context;
use reshiki::chemistry::inchi::{
    generator::{self, Error, Limits, Resource},
    output,
};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

fn helper(name: &str) -> anyhow::Result<Option<PathBuf>> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("artifacts/inchi-helper")
        .join(if cfg!(windows) {
            format!("{name}.exe")
        } else {
            name.to_owned()
        });
    if path.is_file() {
        return Ok(Some(path));
    }
    anyhow::ensure!(
        std::env::var_os("RESHIKI_REQUIRE_INCHI_HELPER").is_none(),
        "Build the native helper first"
    );
    eprintln!("Skipping optional native InChI reader check; build the helper first");
    Ok(None)
}

#[derive(Deserialize)]
struct Case {
    name: String,
    inchi: String,
    raw: output::Output,
    #[serde(default)]
    reconstruct: bool,
    #[serde(default)]
    sanitize: bool,
    #[serde(default)]
    remove: bool,
    #[serde(default)]
    limit: bool,
    expected: Option<Value>,
    error: Option<String>,
}

async fn compare_original_captures(boundary: bool) -> anyhow::Result<()> {
    let Some(helper) = helper("reshiki-inchi-helper")? else {
        return Ok(());
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    };
    let mut command = Command::new(root.join(python));
    command.arg(root.join("tests/inchi_reader_reference.py"));
    if boundary {
        command.arg("--boundaries");
    }
    let mut process = command.stdout(Stdio::piped()).spawn()?;
    let mut lines =
        BufReader::new(process.stdout.take().context("Missing import fixtures")?).lines();
    let header: Value = serde_json::from_str(&lines.next().context("Missing fixture version")??)?;
    assert_eq!(header["rdkit_version"], reshiki::chemistry::RDKIT_VERSION);
    assert_eq!(
        header["inchi_version"],
        reshiki::chemistry::inchi::INCHI_VERSION
    );
    if boundary {
        let capture = include_str!("inchi_output_reference.cpp").replace(
            "in.get();std::getline(in,text);",
            "in.get();std::getline(in,text);text=observe::unhex(text);",
        );
        assert_eq!(
            header["transport_capture_sha256"],
            format!("{:x}", Sha256::digest(capture.as_bytes()))
        );
    }
    assert_eq!(
        header["capture_sha256"],
        format!(
            "{:x}",
            Sha256::digest(include_bytes!("inchi_output_reference.cpp"))
        )
    );
    let (mut records, mut warnings, mut failed, mut reconstructed, mut boundaries) =
        (0, 0, 0, 0, 0);
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        let before = case.inchi.clone();
        let result = generator::read(&helper, &case.inchi, Duration::from_secs(30)).await;
        assert_eq!(case.inchi, before, "{} changed input", case.name);
        if case.limit {
            assert!(
                matches!(result, Err(Error::Limit("InChI text"))),
                "{}: {result:?}",
                case.name
            );
            boundaries += 1;
            continue;
        }
        let actual = result.with_context(|| case.name.clone())?;
        assert_eq!(actual, case.raw, "{} native output", case.name);
        records += 1;
        warnings += usize::from(actual.status == 1);
        failed += usize::from(!matches!(actual.status, 0 | 1));
        if case.reconstruct {
            let input_before = actual.clone();
            let assembled = output::reconstruct(
                &actual,
                output::Options {
                    sanitize: case.sanitize,
                    remove_hydrogens: case.remove,
                },
            );
            assert_eq!(actual, input_before, "{} changed output records", case.name);
            match (assembled, case.expected, case.error) {
                (Ok(value), expected, None) => assert_eq!(
                    serde_json::to_value(value.state)?,
                    expected.unwrap_or(Value::Null),
                    "{} reconstruction",
                    case.name
                ),
                (Err(_), None, Some(_)) => (),
                (actual, expected, error) => anyhow::bail!(
                    "{} reconstruction mismatch: {actual:?}, expected={expected:?}, native error={error:?}",
                    case.name
                ),
            }
            reconstructed += 1;
        }
    }
    assert!(process.wait()?.success());
    if boundary {
        assert_eq!(records, 24);
        assert_eq!(reconstructed, 0);
        assert_eq!(boundaries, 1);
    } else {
        assert_eq!(records, 1848);
        assert_eq!(reconstructed, 1848);
        assert_eq!(boundaries, 0);
        assert_eq!(warnings, 4);
        assert_eq!(failed, 16);
    }
    println!(
        "Native InChI import: {records} exact raw records, {reconstructed} reconstruction outcomes, {warnings} warnings, {failed} native failures, {boundaries} explicit text limit"
    );
    Ok(())
}

#[tokio::test]
async fn native_import_and_reconstruction_match_original_captures() -> anyhow::Result<()> {
    compare_original_captures(false).await
}

#[tokio::test]
async fn native_import_text_boundaries_match_original_captures() -> anyhow::Result<()> {
    compare_original_captures(true).await
}

fn prepare_stub(source: &Path, destination: &Path) -> anyhow::Result<()> {
    // Reuse the completed native executable's inode on Unix. Copying and then
    // immediately executing can race filesystem writeback with ETXTBSY.
    #[cfg(unix)]
    std::fs::hard_link(source, destination)?;
    #[cfg(not(unix))]
    std::fs::copy(source, destination)?;
    Ok(())
}

#[tokio::test]
async fn reader_limits_and_transport_failures_are_distinct_from_native_status() -> anyhow::Result<()>
{
    let Some(path) = helper("reshiki-inchi-helper")? else {
        return Ok(());
    };
    let text = "InChI=1S/CH4/h1H4";
    for budget in [1, 128, 1024] {
        assert!(matches!(
            generator::read_with_limits(
                &path,
                text,
                Limits {
                    timeout: Duration::from_secs(5),
                    kernel_heap_bytes: budget
                }
            )
            .await,
            Err(Error::ResourceLimit {
                resource: Resource::KernelHeap,
                ..
            })
        ));
    }
    let invalid = generator::read(&path, "invalid", Duration::from_secs(5)).await?;
    assert!(!matches!(invalid.status, 0 | 1));
    let actual = generator::read(&path, text, Duration::from_secs(5)).await?;
    assert_eq!(actual.atoms.len(), 1);
    assert_eq!(actual.atoms.first().context("Missing atom")?.element, "C");
    let Some(stub) = helper("inchi-helper-stub")? else {
        return Ok(());
    };
    for mode in [
        "protocol",
        "version",
        "truncated",
        "oversized",
        "hang",
        "resource",
        "resource-unavailable",
        "ok",
        "read-ok",
        "read-counts",
        "read-stereo",
        "read-element",
        "read-coordinate",
        "read-adjacency",
        "read-index",
        "read-status",
        "read-trailing",
        "read-truncated",
    ] {
        let dir = tempfile::tempdir_in(stub.parent().context("Missing stub directory")?)?;
        let executable = dir.path().join(if cfg!(windows) {
            format!("{mode}.exe")
        } else {
            mode.into()
        });
        prepare_stub(&stub, &executable)?;
        let result = generator::read(
            &executable,
            text,
            if mode == "hang" {
                Duration::from_millis(100)
            } else {
                Duration::from_secs(5)
            },
        )
        .await;
        let correct = match (mode, &result) {
            ("version", Err(Error::Version(_)))
            | ("oversized", Err(Error::Limit(_)))
            | ("hang", Err(Error::Timeout))
            | ("resource", Err(Error::ResourceLimit { .. }))
            | ("resource-unavailable", Err(Error::ResourceUnavailable { .. })) => true,
            ("read-ok", Ok(output)) => {
                assert_eq!(output.warning_flags, [[1 << 50, 7], [8, 9]]);
                let atom = output.atoms.first().context("Missing stub atom")?;
                assert_eq!(atom.radical, 2);
                assert_eq!(
                    atom.bonds
                        .iter()
                        .map(|b| (b.neighbor, b.kind, b.stereo))
                        .collect::<Vec<_>>(),
                    [(1, -1, -4), (1, -1, -4)]
                );
                assert_eq!(
                    output.stereo.first().context("Missing stub stereo")?.parity,
                    4
                );
                true
            }
            (_, Err(Error::Protocol(_)))
                if mode.starts_with("read-") || matches!(mode, "protocol" | "truncated" | "ok") =>
            {
                true
            }
            _ => false,
        };
        assert!(correct, "{mode}: {result:?}");
    }
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn cancelling_an_import_kills_the_running_child() -> anyhow::Result<()> {
    let Some(stub) = helper("inchi-helper-stub")? else {
        return Ok(());
    };
    let dir = tempfile::tempdir_in(stub.parent().context("Missing stub directory")?)?;
    let executable = dir.path().join("hang");
    prepare_stub(&stub, &executable)?;
    let task = tokio::spawn(async move {
        generator::read(&executable, "InChI=1S/CH4/h1H4", generator::MAX_TIMEOUT).await
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
                "Import helper exited before its marker"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    task.abort();
    let completion = task.await;
    let pid = started.context("Import helper startup timed out")??;
    anyhow::ensure!(pid > 0, "Invalid import helper PID");
    assert!(matches!(completion, Err(error) if error.is_cancelled()));
    let mut stopped = false;
    for _ in 0..100 {
        if !Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?
            .success()
        {
            stopped = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(stopped, "Import helper survived cancellation");
    let executable = dir.path().join("no-read");
    prepare_stub(&stub, &executable)?;
    let text = " ".repeat(generator::MAX_INCHI_BYTES);
    assert!(matches!(
        generator::read(&executable, &text, Duration::from_millis(100)).await,
        Err(Error::Timeout)
    ));
    Ok(())
}

#[tokio::test]
async fn invalid_import_limits_do_not_launch_a_process() -> anyhow::Result<()> {
    let missing = tempfile::tempdir()?.path().join("helper-does-not-exist");
    let text = "InChI=1S/CH4/h1H4";
    for limits in [
        Limits {
            timeout: Duration::ZERO,
            kernel_heap_bytes: 1,
        },
        Limits {
            timeout: generator::MAX_TIMEOUT + Duration::from_secs(1),
            kernel_heap_bytes: 1,
        },
        Limits {
            timeout: Duration::from_secs(1),
            kernel_heap_bytes: 0,
        },
        Limits {
            timeout: Duration::from_secs(1),
            kernel_heap_bytes: generator::MAX_KERNEL_HEAP_BYTES + 1,
        },
    ] {
        assert!(matches!(
            generator::read_with_limits(&missing, text, limits).await,
            Err(Error::Input(_))
        ));
    }
    let oversized = "x".repeat(generator::MAX_INCHI_BYTES + 1);
    assert!(matches!(
        generator::read(&missing, &oversized, Duration::from_secs(1)).await,
        Err(Error::Limit("InChI text"))
    ));
    Ok(())
}
