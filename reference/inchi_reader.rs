//! Native import records captured independently from the original InchiToMol.
#[path = "../tests/common/fixture.rs"]
mod fixture;

use anyhow::Context;
use reshiki::chemistry::inchi::{
    generator::{self, Error, Limits, Resource},
    output,
};
use serde::Deserialize;
use serde_json::Value;
#[cfg(unix)]
use std::process::{Command, Stdio};
use std::{
    io::BufRead,
    path::{Path, PathBuf},
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
    sanitize: bool,
    #[serde(default)]
    remove: bool,
    expected: Option<Value>,
    error: Option<String>,
}

#[tokio::test]
async fn import_options_match_independently_captured_molecules() -> anyhow::Result<()> {
    let Some(helper) = helper("reshiki-inchi-helper")? else {
        return Ok(());
    };
    let mut count = 0;
    for line in fixture::open("inchi-output.jsonl.gz")?.lines().skip(1) {
        let value: Value = serde_json::from_str(&line?)?;
        if value["operation"] != "import" {
            continue;
        }
        let case: Case = serde_json::from_value(value)?;
        let actual = generator::read_with_options(
            &helper,
            &case.inchi,
            Limits {
                timeout: Duration::from_secs(30),
                heap_bytes: generator::DEFAULT_HEAP_BYTES,
            },
            output::Options {
                sanitize: case.sanitize,
                remove_hydrogens: case.remove,
            },
        )
        .await;
        match (actual, case.expected, case.error) {
            (Ok(value), expected, None) => {
                assert_eq!(value.status, case.raw.status, "{} status", case.name);
                assert_eq!(value.message, case.raw.message, "{} diagnostics", case.name);
                assert_eq!(
                    serde_json::to_value(value.state)?,
                    expected.unwrap_or(Value::Null),
                    "{}",
                    case.name
                );
            }
            (Err(Error::Rejected(_)), None, Some(_)) => (),
            (actual, expected, error) => anyhow::bail!(
                "{}: {actual:?}, expected={expected:?}, error={error:?}",
                case.name
            ),
        }
        count += 1;
    }
    assert_eq!(count, 1848);
    Ok(())
}

#[tokio::test]
async fn import_text_boundaries_preserve_native_results() -> anyhow::Result<()> {
    let Some(helper) = helper("reshiki-inchi-helper")? else {
        return Ok(());
    };
    let options = output::Options {
        sanitize: false,
        remove_hydrogens: false,
    };
    let mut count = 0;
    for line in fixture::open("inchi-reader-text.json.gz")?.lines().skip(1) {
        let case: Value = serde_json::from_str(&line?)?;
        let text = case["inchi"].as_str().context("Missing InChI text")?;
        let actual = generator::read_with_options(
            &helper,
            text,
            Limits {
                timeout: Duration::from_secs(30),
                heap_bytes: generator::DEFAULT_HEAP_BYTES,
            },
            options,
        )
        .await;
        if case["limit"] == true {
            assert!(matches!(actual, Err(Error::Limit("InChI text"))));
        } else {
            let raw: output::Output = serde_json::from_value(case["raw"].clone())?;
            let actual = actual.with_context(|| case["name"].to_string())?;
            assert_eq!(actual.status, raw.status, "{} status", case["name"]);
            assert_eq!(actual.message, raw.message, "{} message", case["name"]);
            assert_eq!(actual.log, raw.log, "{} log", case["name"]);
            assert_eq!(
                serde_json::to_value(actual.state)?,
                serde_json::to_value(output::reconstruct(&raw, options)?.state)?,
                "{} molecule",
                case["name"]
            );
        }
        count += 1;
    }
    assert_eq!(count, 25);
    Ok(())
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
                    heap_bytes: budget
                }
            )
            .await,
            Err(Error::ResourceLimit {
                resource: Resource::Heap,
                ..
            })
        ));
    }
    let invalid = generator::read(&path, "invalid", Duration::from_secs(5)).await?;
    assert!(!matches!(invalid.status, 0 | 1));
    let actual = generator::read(&path, text, Duration::from_secs(5)).await?;
    let state = actual.state.context("Missing molecule")?;
    assert_eq!(state.graph.atoms.len(), 1);
    assert_eq!(
        state
            .graph
            .atoms
            .first()
            .context("Missing atom")?
            .atomic_number,
        6
    );
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
        "ok",
        "read-ok",
        "read-counts",
        "read-stereo",
        "read-element",
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
            | ("resource", Err(Error::ResourceLimit { .. })) => true,
            ("read-ok", Ok(output)) => output.state.is_some(),
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
            heap_bytes: 1,
        },
        Limits {
            timeout: generator::MAX_TIMEOUT + Duration::from_secs(1),
            heap_bytes: 1,
        },
        Limits {
            timeout: Duration::from_secs(1),
            heap_bytes: 0,
        },
        Limits {
            timeout: Duration::from_secs(1),
            heap_bytes: generator::MAX_HEAP_BYTES + 1,
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
