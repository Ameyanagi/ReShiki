//! Complete original-worker cleanup responses with the actual Rust solver.
//! Captured layouts are observations only and never become implementation input.
use anyhow::Context;
use reshiki::{
    chemistry::{cleanup, inchi::generator},
    cleanup::{Options, Scope},
    document::{Document, History, Point},
    engine::{
        ChemistryEngine, LocalEngine, PythonEngine, Request, Response, native_cleanup,
        native_response,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
    time::Duration,
};

#[derive(Clone, Deserialize, Serialize)]
struct Case {
    name: String,
    document: Document,
    options: Options,
    selected: Vec<u64>,
    layouts: Vec<Layout>,
    expected: Option<Value>,
    error: Option<String>,
}
#[derive(Clone, Deserialize, Serialize)]
struct Layout {
    ids: Vec<u64>,
    chiral_ranks: Vec<Option<u32>>,
}
impl Case {
    fn request(&self) -> Request {
        let mut request = Request::molecule("clean", self.document.clone());
        request.cleanup = Some(self.options);
        request.selected_ids = Some(self.selected.clone());
        request
    }
}
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
fn helper(name: &str) -> anyhow::Result<Option<PathBuf>> {
    let path = root()
        .join("artifacts/inchi-helper")
        .join(if cfg!(windows) {
            format!("{name}.exe")
        } else {
            name.into()
        });
    if path.is_file() {
        return Ok(Some(path));
    }
    anyhow::ensure!(
        std::env::var_os("RESHIKI_REQUIRE_INCHI_HELPER").is_none(),
        "Build the pinned native helper first"
    );
    eprintln!("Skipping optional cleanup response test; build the pinned helper first");
    Ok(None)
}
fn python() -> PathBuf {
    root().join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    })
}
fn original_cases() -> anyhow::Result<Vec<Case>> {
    let mut child = Command::new(python())
        .arg(root().join("tests/cleanup_reference.py"))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let stdout = child.stdout.take().context("Missing reference output")?;
    let cases = BufReader::new(stdout)
        .lines()
        .map(|line| {
            // Exactly JSON -> Value(f64) -> Document(f32), like PythonEngine.
            Ok(serde_json::from_value(serde_json::from_str(&line?)?)?)
        })
        .collect::<anyhow::Result<Vec<Case>>>()?;
    anyhow::ensure!(child.wait()?.success(), "Original cleanup reference failed");
    for case in &cases {
        for layout in &case.layouts {
            anyhow::ensure!(
                layout.ids.len() == layout.chiral_ranks.len()
                    && layout.chiral_ranks.iter().all(Option::is_none),
                "{} native chiral-rank property changed",
                case.name
            );
        }
    }
    Ok(cases)
}
fn compare(actual: Response, mut expected: Response, name: &str) -> anyhow::Result<()> {
    // Native assertion-only diagnostics cannot truthfully be recreated in Rust.
    // These two precise replacements keep all chemistry and document fields exact.
    if name.starts_with("partial-warning/element/") {
        anyhow::ensure!(expected.analysis.is_none() && expected.warnings.len() == 1);
        anyhow::ensure!(expected.warnings.first().is_some_and(|warning| {
            warning.contains("Post-condition Violation\n\tElement 'Xx' not found")
        }));
        let warning = "Selected geometry cleaned. Another part of the drawing needs checking: Unknown element Xx";
        anyhow::ensure!(actual.warnings == [warning]);
        expected.warnings = vec![warning.into()];
    }
    if name.starts_with("partial-warning/stereo-references/") {
        anyhow::ensure!(expected.analysis.is_none() && expected.warnings.len() == 1);
        anyhow::ensure!(
            expected.warnings.first().is_some_and(|warning| warning
                .contains("Pre-condition Violation\n\tbgnIdx not connected to begin atom of bond"))
        );
        let warning = "Selected geometry cleaned. Another part of the drawing needs checking: Bond stereo references must be attached to the corresponding endpoints";
        anyhow::ensure!(actual.warnings == [warning]);
        expected.warnings = vec![warning.into()];
    }
    if let (Some(a), Some(e)) = (&actual.document, &expected.document) {
        for (a, e) in a.atoms.iter().zip(&e.atoms) {
            anyhow::ensure!(
                a.position.x.to_bits() == e.position.x.to_bits()
                    && a.position.y.to_bits() == e.position.y.to_bits(),
                "{name} coordinate bits changed for atom {}: {:?} != {:?}",
                a.id,
                a.position,
                e.position
            );
        }
    }
    let mut actual = serde_json::to_value(actual)?;
    let mut expected = serde_json::to_value(expected)?;
    for field in ["mass", "exact_mass", "logp", "tpsa"] {
        if let (Some(a), Some(e)) = (
            actual["analysis"][field].as_f64(),
            expected["analysis"][field].as_f64(),
        ) {
            anyhow::ensure!(
                (a - e).abs() <= e.abs().max(1.) * 1e-12,
                "{field}: {a} != {e}"
            );
            actual["analysis"][field] = Value::Null;
            expected["analysis"][field] = Value::Null;
        }
    }
    anyhow::ensure!(actual == expected, "{name}: {actual}\nExpected: {expected}");
    Ok(())
}
async fn isolated(mode: &str, helper: &Path, cases: &[Case]) -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let guard = directory.path().join("guard");
    std::fs::create_dir_all(guard.join("engine"))?;
    std::fs::write(
        guard.join("engine/worker.py"),
        include_str!("native_routing_guard.py"),
    )?;
    let fixture = directory.path().join("cases.json");
    std::fs::write(&fixture, serde_json::to_vec(&serde_json::to_value(cases)?)?)?;
    let mut command = tokio::process::Command::new(std::env::current_exe()?);
    command
        .args(["--exact", "cleanup_child", "--ignored", "--nocapture"])
        .env("RESHIKI_CLEANUP_MODE", mode)
        .env("RESHIKI_CLEANUP_FIXTURE", fixture)
        .env("RESHIKI_ROOT", &guard)
        .env("RESHIKI_PYTHON", python())
        .env("RESHIKI_REFERENCE_PYTHON", python())
        .env("RESHIKI_INCHI_HELPER", helper)
        .env(
            "RESHIKI_DATA_DIR",
            directory.path().join("unexpected-cache"),
        )
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(120), command.output()).await??;
    anyhow::ensure!(
        output.status.success(),
        "{mode}: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    anyhow::ensure!(!directory.path().join("unexpected-cache").exists());
    eprint!("{}", String::from_utf8_lossy(&output.stdout));
    Ok(())
}

#[tokio::test]
async fn complete_responses_use_actual_solver_without_python() -> anyhow::Result<()> {
    let Some(helper) = helper("reshiki-inchi-helper")? else {
        return Ok(());
    };
    let cases = tokio::task::spawn_blocking(original_cases).await??;
    anyhow::ensure!(cases.len() >= 330);
    isolated("reference", &helper, &cases).await?;
    let partial = cases
        .iter()
        .filter(|case| {
            case.expected
                .as_ref()
                .is_some_and(|expected| expected.get("analysis").is_some_and(Value::is_null))
        })
        .cloned()
        .collect::<Vec<_>>();
    anyhow::ensure!(partial.len() == 14);
    // Optional analysis warnings must not discover or start an unnecessary helper.
    isolated("reference", Path::new("relative-unused-helper"), &partial).await
}

fn carbon(scope: Scope) -> Case {
    let mut document = Document::default();
    let id = document.add_atom("C", Point::new(0.1, -0.2));
    Case {
        name: format!("helper/{scope}"),
        document,
        options: Options {
            scope,
            keep_orientation: true,
        },
        selected: vec![id],
        layouts: Vec::new(),
        expected: None,
        error: None,
    }
}
#[tokio::test]
async fn helper_failures_and_cancellation_remain_errors_in_every_scope() -> anyhow::Result<()> {
    let Some(stub) = helper("inchi-helper-stub")? else {
        return Ok(());
    };
    let cases = [
        Scope::Drawing,
        Scope::SelectedAtoms,
        Scope::SelectedMolecules,
    ]
    .map(carbon);
    let directory = tempfile::tempdir()?;
    isolated("missing", &directory.path().join("missing-helper"), &cases).await?;
    isolated("relative", Path::new("relative-helper"), &cases).await?;
    for mode in ["exit", "protocol", "resource", "timeout"] {
        let name = if mode == "timeout" { "hang" } else { mode };
        let executable = directory.path().join(if cfg!(windows) {
            format!("{name}.exe")
        } else {
            name.into()
        });
        std::fs::copy(&stub, &executable)?;
        isolated(mode, &executable, &cases).await?;
    }
    #[cfg(unix)]
    {
        std::fs::remove_file(directory.path().join("pid"))?;
        isolated("cancel", &directory.path().join("hang"), &cases).await?;
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "Invoked by isolated cleanup subprocess tests"]
async fn cleanup_child() -> anyhow::Result<()> {
    let mode = std::env::var("RESHIKI_CLEANUP_MODE")?;
    let bytes =
        std::fs::read(std::env::var_os("RESHIKI_CLEANUP_FIXTURE").context("Missing fixture")?)?;
    let cases: Vec<Case> = serde_json::from_value(serde_json::from_slice(&bytes)?)?;
    let marker = PathBuf::from(std::env::var_os("RESHIKI_ROOT").context("Missing guard root")?)
        .join("python-started");
    let mut completed = 0;
    let mut rejected = 0;
    for case in &cases {
        let request = Arc::new(case.request());
        let before = serde_json::to_value(request.as_ref())?;
        if mode == "cancel" {
            #[cfg(unix)]
            cancel(request.clone()).await?;
            anyhow::ensure!(serde_json::to_value(request.as_ref())? == before);
            continue;
        }
        let config = if mode == "timeout" {
            let mut config = native_response::Config::new(PathBuf::from(
                std::env::var_os("RESHIKI_INCHI_HELPER").context("Missing helper")?,
            ));
            config.limits.timeout = Duration::from_millis(50);
            Some(config)
        } else {
            None
        };
        let result = native_cleanup::execute(request.clone(), config).await;
        if mode != "timeout" {
            let routed = LocalEngine::default()
                .execute(request.as_ref().clone())
                .await;
            match (&result, routed) {
                (Ok(direct), Ok(routed)) => anyhow::ensure!(
                    serde_json::to_value(direct)? == serde_json::to_value(routed)?,
                    "{} default cleanup changed the complete native response",
                    case.name
                ),
                (Err(direct), Err(routed)) => anyhow::ensure!(
                    direct.to_string() == routed,
                    "{} default cleanup changed the native failure",
                    case.name
                ),
                (direct, routed) => anyhow::bail!(
                    "{} default cleanup outcome differs: {direct:?} != {routed:?}",
                    case.name
                ),
            }
        }
        anyhow::ensure!(
            serde_json::to_value(request.as_ref())? == before,
            "Source snapshot changed"
        );
        if mode == "reference" {
            match (result, case.expected.clone()) {
                (Ok(actual), Some(expected)) => {
                    let mut changed = actual
                        .document
                        .clone()
                        .context("Missing cleanup document")?;
                    compare(actual, serde_json::from_value(expected)?, &case.name)?;
                    let mut history = History::default();
                    history.commit(case.document.clone(), &changed);
                    history.undo(&mut changed);
                    anyhow::ensure!(changed == case.document, "Undo changed source");
                    completed += 1;
                }
                (Err(actual), None) => {
                    let expected = case
                        .error
                        .as_deref()
                        .context("Missing original rejection")?;
                    if case.name.starts_with("whole-error/element/") {
                        anyhow::ensure!(
                            expected.contains("Post-condition Violation\n\tElement 'Xx' not found")
                        );
                        anyhow::ensure!(actual.to_string() == "Unknown element Xx");
                    } else {
                        anyhow::ensure!(
                            actual.to_string() == expected,
                            "{}: {actual} != {expected}",
                            case.name
                        );
                    }
                    anyhow::ensure!(
                        matches!(
                            actual,
                            native_cleanup::Error::Cleanup(_)
                                | native_cleanup::Error::Empty
                                | native_cleanup::Error::Layout(_)
                        ),
                        "Unexpected failure: {actual}"
                    );
                    rejected += 1;
                }
                (actual, expected) => anyhow::bail!(
                    "{} outcome changed: {actual:?}, expected {expected:?} / {:?}",
                    case.name,
                    case.error
                ),
            }
        } else {
            let error = result
                .err()
                .context("Helper failure became successful partial cleanup")?;
            let matched = matches!(
                (&*mode, &error),
                (
                    "missing" | "relative",
                    native_cleanup::Error::Analysis(native_response::Error::Discovery(_)),
                ) | (
                    "exit",
                    native_cleanup::Error::Analysis(native_response::Error::Helper(
                        generator::Error::Exit { .. },
                    )),
                ) | (
                    "protocol",
                    native_cleanup::Error::Analysis(native_response::Error::Helper(
                        generator::Error::Protocol(_),
                    )),
                ) | (
                    "resource",
                    native_cleanup::Error::Analysis(native_response::Error::Helper(
                        generator::Error::ResourceLimit {
                            resource: generator::Resource::KernelHeap,
                            ..
                        },
                    )),
                ) | (
                    "timeout",
                    native_cleanup::Error::Analysis(native_response::Error::Helper(
                        generator::Error::Timeout,
                    )),
                )
            );
            anyhow::ensure!(matched, "{mode}: {error:?}");
        }
    }
    anyhow::ensure!(!marker.exists(), "Native cleanup started Python");
    let mut invalid = Request::molecule("clean", Document::default());
    anyhow::ensure!(matches!(
        native_cleanup::execute(invalid.clone(), None).await,
        Err(native_cleanup::Error::Empty)
    ));
    invalid.protocol = 0;
    anyhow::ensure!(matches!(
        native_cleanup::execute(invalid, None).await,
        Err(native_cleanup::Error::Protocol)
    ));
    // Positive control proves this guard would detect any reference worker startup.
    let error = PythonEngine::default()
        .execute(carbon(Scope::Drawing).request())
        .await
        .err();
    anyhow::ensure!(error.as_deref() == Some("Python routing guard"));
    anyhow::ensure!(std::fs::read_to_string(marker)?.lines().count() == 1);
    println!(
        "Native cleanup: {completed} exact complete responses, {rejected} matching rejections"
    );
    Ok(())
}

#[cfg(unix)]
async fn cancel(request: Arc<Request>) -> anyhow::Result<()> {
    let helper = PathBuf::from(std::env::var_os("RESHIKI_INCHI_HELPER").context("Missing helper")?);
    let marker = helper
        .parent()
        .context("Missing helper parent")?
        .join("pid");
    if marker.exists() {
        std::fs::remove_file(&marker)?;
    }
    let task = tokio::spawn(native_cleanup::execute(request, None));
    let pid = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(text) = std::fs::read_to_string(&marker)
                && let Ok(pid) = text.parse::<u32>()
            {
                break pid;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    anyhow::ensure!(pid > 0);
    task.abort();
    anyhow::ensure!(task.await.is_err());
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if !Command::new("kill")
                .args(["-0", &pid.to_string()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()?
                .success()
            {
                return Ok::<_, std::io::Error>(());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await??;
    Ok(())
}

#[test]
fn component_callback_order_and_failures_are_atomic() -> anyhow::Result<()> {
    #[derive(Debug, thiserror::Error)]
    enum Failure {
        #[error(transparent)]
        Cleanup(#[from] cleanup::Error),
        #[error("Later solver must not run")]
        Solver,
    }
    let mut document = Document::default();
    let a = document.add_atom("C", Point::new(0., 0.));
    let b = document.add_atom("C", Point::new(42., 0.));
    let c = document.add_atom("O", Point::new(84., 0.));
    document.add_bond(a, b, 1, "plain");
    document.add_bond(b, c, 1, "plain");
    let d = document.add_atom("N", Point::new(0., 84.));
    let options = Options {
        scope: Scope::SelectedAtoms,
        keep_orientation: true,
    };
    let before = document.clone();
    let prepared = cleanup::prepare(&document, options, &[b, d])?;
    let mut calls = 0;
    let result = prepared.finish_with(|request| {
        calls += 1;
        if calls > 1 {
            return Err(Failure::Solver);
        }
        let mut positions = request.molecule.positions.clone();
        positions.first_mut().ok_or(Failure::Solver)?.x += 1.;
        Ok(positions)
    });
    anyhow::ensure!(matches!(result, Err(Failure::Cleanup(cleanup::Error::Fixed))) && calls == 1);
    let prepared = cleanup::prepare(&document, options, &[b, d])?;
    let result = prepared.finish_with::<Failure>(|_| Err(Failure::Solver));
    anyhow::ensure!(matches!(result, Err(Failure::Solver)) && document == before);
    Ok(())
}

#[tokio::test]
async fn rejected_requests_preserve_the_source_and_skip_analysis() -> anyhow::Result<()> {
    let mut request = carbon(Scope::Drawing).request();
    request.protocol = 0;
    anyhow::ensure!(matches!(
        native_cleanup::execute(request.clone(), None).await,
        Err(native_cleanup::Error::Protocol)
    ));
    request.protocol = 1;
    request.operation = "analyze".into();
    anyhow::ensure!(matches!(
        native_cleanup::execute(request.clone(), None).await,
        Err(native_cleanup::Error::Operation)
    ));
    request.operation = "clean".into();
    request.document = None;
    anyhow::ensure!(matches!(
        native_cleanup::execute(request, None).await,
        Err(native_cleanup::Error::MissingDocument)
    ));
    for scope in [Scope::SelectedAtoms, Scope::SelectedMolecules] {
        let mut request = carbon(scope).request();
        request.selected_ids = None;
        let snapshot = Arc::new(request);
        let before = serde_json::to_value(snapshot.as_ref())?;
        anyhow::ensure!(matches!(
            native_cleanup::execute(snapshot.clone(), None).await,
            Err(native_cleanup::Error::Cleanup(cleanup::Error::Select))
        ));
        anyhow::ensure!(serde_json::to_value(snapshot.as_ref())? == before);
    }
    let mut request = carbon(Scope::Drawing).request();
    request.selected_ids = Some(vec![u64::MAX]);
    let snapshot = Arc::new(request);
    let before = serde_json::to_value(snapshot.as_ref())?;
    anyhow::ensure!(matches!(
        native_cleanup::execute(snapshot.clone(), None).await,
        Err(native_cleanup::Error::Cleanup(cleanup::Error::Selection))
    ));
    anyhow::ensure!(serde_json::to_value(snapshot.as_ref())? == before);
    Ok(())
}
