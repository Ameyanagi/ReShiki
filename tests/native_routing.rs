//! Real default dispatch in a subprocess whose Python worker records startup
//! and rejects every request. Expected chemistry comes from the original worker
//! in the parent; environment overrides never mutate the test runner process.
use anyhow::Context;
use reshiki::{
    arrows::ArrowStyle,
    document::{Annotation, Document, Point},
    engine::{ChemistryEngine, LocalEngine, PythonEngine, Request, Response},
};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

#[path = "support/reference_presentation.rs"]
mod reference_presentation;

#[derive(Clone, Serialize, Deserialize)]
struct Case {
    operation: String,
    format: Option<String>,
    document: Document,
    text: Option<String>,
    selected: Option<Vec<u64>>,
    expected: Result<Response, String>,
}
impl Case {
    fn request(&self) -> Request {
        let mut request = if self.operation == "import" {
            Request::import(
                self.format.as_deref().unwrap_or("smiles"),
                self.text.as_deref().unwrap_or_default(),
            )
        } else {
            Request::molecule(&self.operation, self.document.clone())
        };
        request.format = self.format.clone();
        request.text = self.text.clone();
        request.selected_ids = self.selected.clone();
        request
    }
}
fn helper(name: &str) -> anyhow::Result<Option<PathBuf>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
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
    eprintln!("Skipping optional routing test; build scripts/build_inchi_helper.py first");
    Ok(None)
}
async fn import(reference: &PythonEngine, text: &str) -> anyhow::Result<Document> {
    reference
        .execute(Request::import_smiles(text))
        .await
        .map_err(anyhow::Error::msg)?
        .document
        .context("Missing reference document")
}
async fn case(
    reference: &PythonEngine,
    operation: &str,
    format: Option<&str>,
    document: Document,
) -> Case {
    let mut result = Case {
        operation: operation.into(),
        format: format.map(str::to_owned),
        document,
        text: None,
        selected: None,
        expected: Err(String::new()),
    };
    result.expected = reference.execute(result.request()).await;
    result
}
async fn import_case(reference: &PythonEngine, format: &str, text: &str) -> Case {
    let mut result = Case {
        operation: "import".into(),
        format: Some(format.into()),
        document: Document::default(),
        text: Some(text.into()),
        selected: Some(vec![9_007_199_254_740_993]),
        expected: Err(String::new()),
    };
    result.expected = reference.execute(result.request()).await;
    result
}
async fn import_cases() -> anyhow::Result<Vec<Case>> {
    let reference = PythonEngine::default();
    let mut result = Vec::new();
    for smiles in ["N[C@@H](C)C(=O)O", "[13CH3:41][NH3+]", "COc1ccccc1"] {
        let mut document = import(&reference, smiles).await?;
        document.annotations.push(Annotation {
            id: document.next_id(),
            position: Point::new(0.1, -0.2),
            text: "試料 α".into(),
            format: Default::default(),
        });
        for format in ["mol", "cdxml", "cdx"] {
            let mut request = Request::molecule("export", document.clone());
            request.format = Some(format.into());
            let text = reference
                .execute(request)
                .await
                .map_err(anyhow::Error::msg)?
                .output
                .context("Missing reference export")?;
            let entry = import_case(&reference, format, &text).await;
            anyhow::ensure!(
                entry.expected.is_ok(),
                "Reference rejected {format}: {:?}",
                entry.expected
            );
            result.push(entry);
        }
    }
    for smiles in ["CCO.O>O>CC=O.O", "[CH3:1][OH:2]>>[CH2:1]=[O:2]"] {
        let document = reference
            .execute(Request::import("rsmi", smiles))
            .await
            .map_err(anyhow::Error::msg)?
            .document
            .context("Missing reaction")?;
        let mut request = Request::molecule("export", document);
        request.format = Some("rxn".into());
        let text = reference
            .execute(request)
            .await
            .map_err(anyhow::Error::msg)?
            .output
            .context("Missing reference reaction export")?;
        result.push(import_case(&reference, "rxn", &text).await);
    }
    for text in [
        "CO>O>N |(1,2,3;4,5,6;7,8,9;10,11,12)|",
        "[13CH3:2][OH:1]>>[13CH2:2]=[O:1] |(0,0,;1.5,0,;0,0,;1.5,0,)|",
        "F/C=C/F>O>F/C=C\\F |(0,0,;1,1,;2,1,;3,2,;0,3,;4,0,;5,1,;6,1,;7,2,)|",
    ] {
        result.push(import_case(&reference, "rsmi", text).await);
    }
    for entry in &result {
        anyhow::ensure!(
            entry.expected.is_ok(),
            "Reference rejected {:?}: {:?}",
            entry.format,
            entry.expected
        );
    }
    Ok(result)
}
async fn cases(lazy: bool) -> anyhow::Result<Vec<Case>> {
    let reference = PythonEngine::default();
    let mut documents = vec![Document::default()];
    let mut figure = Document::default();
    figure.annotations.push(Annotation {
        id: 1,
        position: Point::new(0.1, -0.2),
        text: "試料 α".into(),
        format: Default::default(),
    });
    documents.push(figure);
    for (order, element, other, display) in [
        (0, "H", "O", "dotted"),
        (5, "N", "Cu", "plain"),
        (6, "Mo", "Mo", "plain"),
        (7, "*", "*", "plain"),
    ] {
        let mut doc = Document::default();
        let a = doc.add_atom(element, Point::new(-42., 0.));
        let b = doc.add_atom(other, Point::new(0., 0.));
        doc.add_bond(a, b, order, display);
        if order == 0 {
            let n = doc.add_atom("N", Point::new(-84., 0.));
            doc.add_bond(n, a, 1, "plain");
        }
        documents.push(doc);
    }
    if !lazy {
        for text in [
            "C",
            "*",
            "N[C@@H](C)C(=O)O",
            "[13CH3:41][NH3+]",
            "[2H]O[3H]",
            "F/C=C/F",
            "COc1ccccc1",
            "C[S@](=O)CC",
        ] {
            let mut doc = import(&reference, text).await?;
            doc.atoms.reverse();
            doc.bonds.reverse();
            for atom in &mut doc.atoms {
                atom.position.x = -atom.position.x;
                atom.cip_label = Some("S".into());
            }
            for bond in &mut doc.bonds {
                bond.color = [17, 126, 108];
            }
            documents.push(doc);
        }
        let mut reaction = reference
            .execute(Request::import("rsmi", "CCO.O>O>CC=O.O"))
            .await
            .map_err(anyhow::Error::msg)?
            .document
            .context("Missing reaction")?;
        // Use a shared explicit style when comparing with the legacy Python
        // exporter, whose implicit arrowhead default predates the new app UI.
        for arrow in &mut reaction.arrows {
            arrow.style = Some(ArrowStyle {
                head_length_pt: 3.0,
                head_width_pt: 1.2,
                head_notch: 0.0,
                ..ArrowStyle::default()
            });
        }
        documents.push(reaction);
    }
    let mut result = Vec::new();
    for document in documents {
        for (operation, format) in [
            ("analyze", None),
            ("finish_abbreviation", None),
            ("export", Some("smiles")),
            ("export", Some("mol")),
            ("export", Some("inchi")),
            ("export", Some("cdxml")),
            ("export", Some("cdx")),
        ] {
            if lazy && document.atoms.is_empty() && operation == "finish_abbreviation" {
                continue;
            }
            result.push(case(&reference, operation, format, document.clone()).await);
        }
    }
    let mut native_empty = Document::default();
    let center = native_empty.add_atom("*", Point::default());
    for index in 0..21 {
        let neighbor = native_empty.add_atom("F", Point::new(index as f32 * 28., 42.));
        native_empty.add_bond(center, neighbor, 1, "plain");
    }
    result.push(case(&reference, "analyze", None, native_empty).await);
    if !lazy {
        let doc = import(&reference, "COCC").await?;
        for replace in [false, true] {
            let mut entry = case(
                &reference,
                "abbreviate",
                replace.then_some("replace"),
                doc.clone(),
            )
            .await;
            if replace {
                entry.text = Some("OMe".into());
                entry.selected = Some(vec![doc.atoms.first().context("Missing selected atom")?.id]);
            }
            entry.expected = reference.execute(entry.request()).await;
            result.push(entry);
        }
    }
    result.extend(aromatic_cases(&reference, lazy).await?);
    Ok(result)
}

async fn aromatic_cases(reference: &PythonEngine, lazy: bool) -> anyhow::Result<Vec<Case>> {
    let mut result = Vec::new();
    for text in [
        "c1ccccc1",
        "c1cc[nH]c1",
        "c1ccc2ccccc2c1",
        "C[C@H](O)c1ccccc1",
        "c1ccccc1.CCO",
    ] {
        let document = import(reference, text).await?;
        for selected in [
            None,
            Some(vec![
                document.atoms.first().context("Missing ring atom")?.id,
            ]),
        ] {
            let mut entry = case(reference, "aromatic", None, document.clone()).await;
            entry.selected = selected;
            entry.expected = reference.execute(entry.request()).await;
            anyhow::ensure!(entry.expected.is_err());
            result.push(entry);
        }
        if !lazy {
            let mut entry = case(reference, "aromatic", None, document).await;
            entry.selected = Some(entry.document.all_ids());
            for _ in 0..2 {
                entry.expected = reference.execute(entry.request()).await;
                let next = entry
                    .expected
                    .as_ref()
                    .map_err(|e| anyhow::anyhow!(e.clone()))?
                    .document
                    .clone()
                    .context("Missing aromatic edit")?;
                result.push(entry.clone());
                entry.document = next;
            }
        }
    }
    Ok(result)
}
fn equal(actual: Response, expected: Response) -> anyhow::Result<()> {
    let mut actual = serde_json::to_value(actual)?;
    let mut expected = serde_json::to_value(expected)?;
    reference_presentation::compare_export(&actual, &mut expected)?;
    for field in ["mass", "exact_mass", "logp", "tpsa"] {
        if let (Some(a), Some(e)) = (
            actual["analysis"][field].as_f64(),
            expected["analysis"][field].as_f64(),
        ) {
            anyhow::ensure!(
                (a - e).abs() <= e.abs().max(1.) * 1e-12,
                "{field}: {a} != {e}"
            );
            actual["analysis"][field] = serde_json::Value::Null;
            expected["analysis"][field] = serde_json::Value::Null;
        }
    }
    anyhow::ensure!(
        actual == expected,
        "Response changed: {actual}\nExpected: {expected}"
    );
    Ok(())
}
fn outcome(
    actual: Result<Response, String>,
    expected: Result<Response, String>,
) -> anyhow::Result<()> {
    match (actual, expected) {
        (Ok(a), Ok(e)) => equal(a, e),
        (Err(a), Err(e)) => {
            anyhow::ensure!(a == e, "Error changed: {a} != {e}");
            Ok(())
        }
        (a, e) => anyhow::bail!("Outcome changed: {a:?} != {e:?}"),
    }
}
fn concurrent_indices(cases: &[Case]) -> Vec<usize> {
    cases
        .iter()
        .enumerate()
        .filter(|(_, c)| c.operation == "import" || !c.document.atoms.is_empty())
        .take(12)
        .chain(
            cases
                .iter()
                .enumerate()
                .filter(|(_, case)| case.operation == "aromatic")
                .take(4),
        )
        .map(|(index, _)| index)
        .collect()
}
async fn child(mode: &str, helper: &Path, cases: &[Case]) -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let temp = tempfile::tempdir()?;
    let guard = temp.path().join("guard");
    std::fs::create_dir_all(guard.join("engine"))?;
    std::fs::write(
        guard.join("engine/worker.py"),
        include_str!("native_routing_guard.py"),
    )?;
    let fixture = temp.path().join("cases.json");
    let responses = temp.path().join("responses.json");
    std::fs::write(&fixture, serde_json::to_vec(&serde_json::to_value(cases)?)?)?;
    let mut command = tokio::process::Command::new(std::env::current_exe()?);
    command
        .args(["--exact", "routing_child", "--ignored", "--nocapture"])
        .env("RESHIKI_NATIVE_ROUTING_MODE", mode)
        .env("RESHIKI_NATIVE_ROUTING_FIXTURE", fixture)
        .env("RESHIKI_NATIVE_ROUTING_RESPONSES", &responses)
        .env("RESHIKI_ROOT", &guard)
        .env(
            "RESHIKI_REFERENCE_PYTHON",
            root.join(if cfg!(windows) {
                ".venv/Scripts/python.exe"
            } else {
                ".venv/bin/python"
            }),
        )
        .env("RESHIKI_INCHI_HELPER", helper)
        .env("RESHIKI_DATA_DIR", temp.path().join("unexpected-cache"))
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(45), command.output()).await??;
    anyhow::ensure!(
        output.status.success(),
        "{mode}: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    anyhow::ensure!(
        !temp.path().join("unexpected-cache").exists(),
        "Created a Python runtime cache"
    );
    if matches!(mode, "native" | "lazy") {
        // Compare outside the guarded subprocess: the independent drawing
        // codec is test-only and must not weaken the Python-free runtime check.
        let actual: Vec<(usize, Result<Response, String>)> =
            serde_json::from_value(serde_json::from_slice(&std::fs::read(responses)?)?)?;
        let mut expected_indices: Vec<_> =
            (0..cases.len()).chain(concurrent_indices(cases)).collect();
        let mut actual_indices: Vec<_> = actual.iter().map(|(index, _)| *index).collect();
        expected_indices.sort_unstable();
        actual_indices.sort_unstable();
        anyhow::ensure!(
            actual_indices == expected_indices,
            "Missing routing responses"
        );
        for (index, response) in actual {
            let case = cases.get(index).context("Invalid routing response index")?;
            outcome(response, case.expected.clone())
                .with_context(|| format!("{mode}/{}/{:?}", case.operation, case.format))?;
        }
    }
    Ok(())
}

#[tokio::test]
async fn supported_default_requests_are_python_free_and_match_original_worker() -> anyhow::Result<()>
{
    let Some(helper) = helper("reshiki-inchi-helper")? else {
        return Ok(());
    };
    let cases = cases(false).await?;
    child("native", &helper, &cases).await?;
    eprintln!(
        "Verified {} complete default responses with Python startup forbidden",
        cases.len()
    );
    Ok(())
}
#[tokio::test]
async fn coordinate_imports_are_python_free_and_match_original_worker() -> anyhow::Result<()> {
    let Some(helper) = helper("reshiki-inchi-helper")? else {
        return Ok(());
    };
    let cases = import_cases().await?;
    child("native", &helper, &cases).await?;
    eprintln!(
        "Verified {} complete default import responses with Python startup forbidden",
        cases.len()
    );
    Ok(())
}
#[tokio::test]
async fn invalid_imports_never_fall_back_to_python() -> anyhow::Result<()> {
    let reference = PythonEngine::default();
    let mut cases = Vec::new();
    for (format, text) in [
        ("mol", "invalid"),
        ("mol", " "),
        ("rxn", "$RXN\nmalformed"),
        ("rsmi", "[CH5]>>O |(0,0,;1,1,)|"),
        ("rsmi", "invalid"),
        ("cdxml", "<CDXML>"),
        (
            "cdxml",
            "<CDXML><page><fragment><n id='1' p='NaN 0'/></fragment></page></CDXML>",
        ),
        ("cdx", "invalid base64"),
        ("cdx", "aW52YWxpZA=="),
        ("unsupported", "C"),
    ] {
        let case = import_case(&reference, format, text).await;
        anyhow::ensure!(
            case.expected.is_err(),
            "Reference accepted invalid {format}: {text}"
        );
        cases.push(case);
    }
    child("invalid-imports", Path::new("relative-helper"), &cases).await
}
#[tokio::test]
async fn helper_discovery_is_lazy_for_empty_and_exotic_results() -> anyhow::Result<()> {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let reference = PythonEngine::default();
    let mut cases = cases(true).await?;
    for xml in [
        "<CDXML><page /></CDXML>",
        "<CDXML><page><t id='1' p='12 24'><s>Reaction conditions</s></t></page></CDXML>",
    ] {
        cases.push(import_case(&reference, "cdxml", xml).await);
        let binary = reshiki::exchange::to_cdx(xml).map_err(anyhow::Error::msg)?;
        cases.push(import_case(&reference, "cdx", &STANDARD.encode(binary)).await);
    }
    child(
        "lazy",
        Path::new("relative-helper-must-not-be-resolved"),
        &cases,
    )
    .await
}
#[tokio::test]
async fn helper_errors_do_not_fall_back_to_python() -> anyhow::Result<()> {
    let Some(stub) = helper("inchi-helper-stub")? else {
        return Ok(());
    };
    let reference = PythonEngine::default();
    let mut cases = vec![case(&reference, "analyze", None, import(&reference, "C").await?).await];
    let imports = import_cases().await?;
    let aromatic = aromatic_cases(&reference, false)
        .await?
        .into_iter()
        .find(|case| case.expected.is_ok())
        .context("Missing aromatic case")?;
    cases.push(aromatic.clone());
    for format in ["mol", "rxn", "cdxml", "cdx", "rsmi"] {
        cases.push(
            imports
                .iter()
                .find(|case| case.format.as_deref() == Some(format))
                .context("Missing import failure case")?
                .clone(),
        );
    }
    let temp = tempfile::tempdir()?;
    child("missing", &temp.path().join("missing-helper"), &cases).await?;
    child("relative", Path::new("relative-helper"), &cases).await?;
    for mode in ["exit", "protocol", "resource"] {
        let executable = temp.path().join(if cfg!(windows) {
            format!("{mode}.exe")
        } else {
            mode.into()
        });
        std::fs::copy(&stub, &executable)?;
        child(mode, &executable, &cases).await?;
    }
    #[cfg(unix)]
    {
        // Each cancellation check must observe the newly started child, not
        // the marker left by an earlier protocol/resource failure process.
        std::fs::remove_file(temp.path().join("pid"))?;
        let executable = temp.path().join("hang");
        std::fs::copy(stub, &executable)?;
        child("cancel", &executable, &cases).await?;
        std::fs::remove_file(temp.path().join("pid"))?;
        child("cancel", &executable, &imports).await?;
        std::fs::remove_file(temp.path().join("pid"))?;
        child("cancel", &executable, &[aromatic]).await?;
    }
    Ok(())
}

#[derive(Clone, Default)]
struct RecordingBackend(Arc<Mutex<Vec<serde_json::Value>>>);
impl ChemistryEngine for RecordingBackend {
    async fn execute(&self, request: Request) -> Result<Response, String> {
        self.0
            .lock()
            .map_err(|_| "Poisoned test lock")?
            .push(serde_json::to_value(request).map_err(|e| e.to_string())?);
        Ok(Response {
            document: None,
            analysis: None,
            output: Some("<CDXML><page id=\"1\" /></CDXML>".into()),
            engine_version: "custom".into(),
            warnings: vec!["custom backend".into()],
        })
    }
}
#[tokio::test]
async fn custom_backends_keep_existing_dispatch_and_binary_conversion() -> anyhow::Result<()> {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let recorder = RecordingBackend::default();
    let local = LocalEngine::with_backend(recorder.clone());
    let mut doc = Document::default();
    doc.add_atom("C", Point::default());
    for (operation, format) in [
        ("analyze", None),
        ("aromatic", None),
        ("finish_abbreviation", None),
        ("export", Some("smiles")),
        ("export", Some("mol")),
        ("export", Some("inchi")),
        ("export", Some("cdxml")),
        ("export", Some("cdx")),
    ] {
        let mut request = Request::molecule(operation, doc.clone());
        request.format = format.map(str::to_owned);
        let mut expected = request.clone();
        if format == Some("cdx") {
            expected.format = Some("cdxml".into());
        }
        let result = local.execute(request).await.map_err(anyhow::Error::msg)?;
        anyhow::ensure!(result.engine_version == "custom" && result.warnings == ["custom backend"]);
        anyhow::ensure!(
            recorder
                .0
                .lock()
                .map_err(|_| anyhow::anyhow!("Poisoned test lock"))?
                .last()
                == Some(&serde_json::to_value(expected)?)
        );
    }
    for format in ["smiles", "inchi", "mol", "rxn", "rsmi", "cdxml", "cdx"] {
        let text = if format == "cdx" {
            STANDARD.encode(
                reshiki::exchange::to_cdx("<CDXML><page /></CDXML>").map_err(anyhow::Error::msg)?,
            )
        } else {
            "arbitrary custom-backend input\r\n".into()
        };
        let request = Request::import(format, &text);
        let mut expected = request.clone();
        if format == "cdx" {
            expected.format = Some("cdxml".into());
            expected.text = Some(
                reshiki::exchange::from_cdx(&STANDARD.decode(&text)?)
                    .map_err(anyhow::Error::msg)?,
            );
        }
        let result = local.execute(request).await.map_err(anyhow::Error::msg)?;
        anyhow::ensure!(result.engine_version == "custom" && result.warnings == ["custom backend"]);
        anyhow::ensure!(
            recorder
                .0
                .lock()
                .map_err(|_| anyhow::anyhow!("Poisoned test lock"))?
                .last()
                == Some(&serde_json::to_value(expected)?)
        );
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "Invoked by isolated routing subprocess tests"]
async fn routing_child() -> anyhow::Result<()> {
    let mode = std::env::var("RESHIKI_NATIVE_ROUTING_MODE")?;
    let fixture = std::fs::read(
        std::env::var_os("RESHIKI_NATIVE_ROUTING_FIXTURE").context("Missing fixture")?,
    )?;
    // Decode through Value, exactly like the actual engine response boundary.
    let cases: Vec<Case> = serde_json::from_value(serde_json::from_slice(&fixture)?)?;
    let marker = PathBuf::from(std::env::var_os("RESHIKI_ROOT").context("Missing guard root")?)
        .join("python-started");
    let local = LocalEngine::default();
    if matches!(mode.as_str(), "native" | "lazy") {
        let mut responses = Vec::new();
        for (index, case) in cases.iter().enumerate() {
            let request = case.request();
            let before = serde_json::to_value(&request)?;
            responses.push((index, local.execute(request.clone()).await));
            assert_eq!(before, serde_json::to_value(request)?);
        }
        let mut tasks = tokio::task::JoinSet::new();
        for index in concurrent_indices(&cases) {
            let case = cases.get(index).context("Missing concurrent case")?.clone();
            let local = local.clone();
            tasks.spawn(async move { (index, local.execute(case.request()).await) });
        }
        while let Some(result) = tasks.join_next().await {
            responses.push(result?);
        }
        std::fs::write(
            std::env::var_os("RESHIKI_NATIVE_ROUTING_RESPONSES")
                .context("Missing response path")?,
            serde_json::to_vec(&responses)?,
        )?;
    } else if mode == "invalid-imports" {
        for case in &cases {
            let request = case.request();
            let before = serde_json::to_value(&request)?;
            let error = local
                .execute(request.clone())
                .await
                .err()
                .context("Invalid import was accepted")?;
            anyhow::ensure!(
                case.expected.is_err() && error != "Python routing guard",
                "Invalid {:?} reached Python: {error}",
                case.format
            );
            anyhow::ensure!(
                !error.contains("RESHIKI_INCHI_HELPER"),
                "Invalid {:?} reached the identifier helper: {error}",
                case.format
            );
            assert_eq!(before, serde_json::to_value(request)?);
        }
    } else if mode == "cancel" {
        #[cfg(unix)]
        cancel(&local, cases.first().context("Missing case")?).await?;
    } else {
        let fragment = match mode.as_str() {
            "missing" => "InChI helper is missing",
            "relative" => "RESHIKI_INCHI_HELPER must be an absolute path",
            "exit" => "exited with code Some(17)",
            "protocol" => "InChI helper protocol:",
            "resource" => "KernelHeap exhausted",
            _ => anyhow::bail!("Unknown failure mode"),
        };
        anyhow::ensure!(!cases.is_empty(), "Missing helper failure cases");
        for case in &cases {
            let request = case.request();
            let original = serde_json::to_value(&request)?;
            let error = local
                .execute(request.clone())
                .await
                .err()
                .context("Helper failure was hidden")?;
            anyhow::ensure!(
                error.contains(fragment),
                "{mode}/{:?}: {error}",
                case.format
            );
            assert_eq!(serde_json::to_value(request)?, original);
        }
        // A process failure cannot poison subsequent helper-free requests.
        let mut empty = Request::molecule("export", Document::default());
        empty.format = Some("cdxml".into());
        anyhow::ensure!(local.execute(empty).await.is_ok());
    }
    anyhow::ensure!(!marker.exists(), "Supported operation launched Python");
    // Positive controls: the independent oracle and explicitly selected custom
    // backend both reach the guard. Default dispatch above never starts it.
    let mut carbon = Document::default();
    carbon.add_atom("C", Point::default());
    assert_eq!(
        PythonEngine::default()
            .execute(Request::molecule("analyze", carbon))
            .await
            .err()
            .as_deref(),
        Some("Python routing guard")
    );
    for (format, text) in [
        ("smiles", "C"),
        ("inchi", "InChI=1S/CH4/h1H4"),
        ("rsmi", "C>>O"),
    ] {
        assert_eq!(
            LocalEngine::with_backend(PythonEngine::default())
                .execute(Request::import(format, text))
                .await
                .err()
                .as_deref(),
            Some("Python routing guard"),
            "{format} custom backend failed to reach the guard"
        );
    }
    anyhow::ensure!(
        std::fs::read_to_string(marker)?.lines().count() == 4,
        "Unexpected worker startup count"
    );
    Ok(())
}

#[cfg(unix)]
async fn cancel(local: &LocalEngine, case: &Case) -> anyhow::Result<()> {
    use std::process::{Command, Stdio};
    let path =
        PathBuf::from(std::env::var_os("RESHIKI_INCHI_HELPER").context("Missing helper path")?);
    let marker = path.parent().context("Missing helper parent")?.join("pid");
    let request = case.request();
    let before = serde_json::to_value(&request)?;
    let engine = local.clone();
    let snapshot = request.clone();
    let task = tokio::spawn(async move { engine.execute(snapshot).await });
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
    anyhow::ensure!(pid > 0, "Invalid child PID");
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
    assert_eq!(serde_json::to_value(request)?, before);
    let mut empty = Request::molecule("export", Document::default());
    empty.format = Some("cdxml".into());
    anyhow::ensure!(local.execute(empty).await.is_ok());
    Ok(())
}
