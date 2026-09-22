//! Compare the migrated application path with the unmodified abbreviation worker.
use anyhow::Context;
use reshiki::{
    chemistry::abbreviations,
    document::{Document, History, Point},
    engine::{ChemistryEngine, LocalEngine, PythonEngine, Request, Response},
};
use std::sync::{Arc, Mutex};

fn native_geometry_diagnostic() -> anyhow::Result<String> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let output = std::process::Command::new(python)
        .arg(root.join("tests/abbreviation_replacement_reference.py"))
        .arg("--geometry")
        .env("PYTHONUTF8", "1")
        .output()?;
    anyhow::ensure!(output.status.success(), "Native geometry probe failed");
    Ok(String::from_utf8(output.stdout)?)
}

fn matches(actual: Response, expected: Response) -> anyhow::Result<()> {
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
            actual["analysis"][field] = serde_json::Value::Null;
            expected["analysis"][field] = serde_json::Value::Null;
        }
    }
    anyhow::ensure!(
        actual == expected,
        "Response mismatch: {actual}\nExpected: {expected}"
    );
    Ok(())
}

async fn compare(
    local: &LocalEngine,
    reference: &PythonEngine,
    request: Request,
) -> anyhow::Result<Option<Document>> {
    let original = request.document.clone();
    let expected = reference.execute(request.clone()).await;
    let actual = local.execute(request.clone()).await;
    anyhow::ensure!(request.document == original, "Caller document changed");
    match (actual, expected) {
        (Ok(actual), Ok(expected)) => {
            let result = actual.document.clone();
            matches(actual, expected)?;
            if let (Some(before), Some(after)) = (original, &result) {
                let mut history = History::default();
                let mut document = after.clone();
                history.commit(before.clone(), &document);
                history.undo(&mut document);
                anyhow::ensure!(document == before, "Undo did not restore the drawing");
            }
            Ok(result)
        }
        (Err(actual), Err(expected)) => {
            anyhow::ensure!(actual == expected, "Error mismatch: {actual} != {expected}");
            Ok(None)
        }
        (actual, expected) => anyhow::bail!("Outcome mismatch: {actual:?} != {expected:?}"),
    }
}

#[tokio::test]
async fn all_presets_preserve_complete_app_responses_selection_and_undo() -> anyhow::Result<()> {
    let local = LocalEngine::default();
    let reference = PythonEngine::default();
    let mut cases = 0;
    for preset in abbreviations::presets()? {
        let tail = preset
            .smiles
            .strip_prefix('*')
            .context("Missing preset attachment")?;
        let doc = reference
            .execute(Request::import_smiles(&format!("N{tail}")))
            .await
            .map_err(anyhow::Error::msg)?
            .document
            .context("Missing imported document")?;
        for selection in [
            None,
            Some(doc.all_ids()),
            Some(vec![]),
            Some(vec![1]),
            Some(vec![999]),
        ] {
            for label in [None, Some(preset.label.clone()), Some("invalid".into())] {
                let mut request = Request::molecule("abbreviate", doc.clone());
                request.text = label;
                request.selected_ids = selection.clone();
                let result = compare(&local, &reference, request)
                    .await
                    .with_context(|| format!("{} / {selection:?}", preset.label))?;
                cases += 1;
                if let Some(collapsed) = result.filter(|doc| !doc.abbreviations.is_empty()) {
                    // Repeated requests must preserve existing groups and never relabel hidden atoms.
                    compare(
                        &local,
                        &reference,
                        Request::molecule("abbreviate", collapsed),
                    )
                    .await?;
                    cases += 1;
                }
            }
        }
    }
    for smiles in [
        "COc1ccc(NC(=O)OC(C)(C)C)cc1",
        "N[C@@H](C)C(=O)O",
        "[13CH3]OC",
        "CO[CH3:4]",
        "F/C=C/COC",
        "[Na+].[O-]C(=O)C",
        "C1CCCCC1",
        "O",
    ] {
        let imported = reference
            .execute(Request::import_smiles(smiles))
            .await
            .map_err(anyhow::Error::msg)?;
        let doc = imported.document.context("Missing imported document")?;
        let mut styled = serde_json::to_value(&doc)?;
        styled["atoms"][0]["text_style"] = serde_json::to_value(reshiki::typography::TextStyle {
            color: [17, 126, 108],
            size_pt: 12.,
            ..Default::default()
        })?;
        // A stale cached label cannot influence chemical identity or matching.
        styled["atoms"][0]["cip_label"] = "stale".into();
        let styled: Document = serde_json::from_value(styled)?;
        compare(&local, &reference, Request::molecule("abbreviate", styled)).await?;
        cases += 1;
    }
    for label in [None, Some("OMe".into()), Some("invalid".into())] {
        let mut request = Request::molecule("abbreviate", Document::default());
        request.text = label;
        compare(&local, &reference, request).await?;
        cases += 1;
    }
    eprintln!("Verified {cases} complete abbreviation responses, with atomic undo");
    Ok(())
}

#[tokio::test]
async fn concurrent_detection_keeps_each_selection_and_label_with_its_drawing() -> anyhow::Result<()>
{
    let local = LocalEngine::default();
    let reference = PythonEngine::default();
    let mut tasks = tokio::task::JoinSet::new();
    for smiles in ["COCC", "CC(=O)Oc1ccccc1", "CC(C)(C)OC(=O)NC", "F/C=C/COC"] {
        let doc = reference
            .execute(Request::import_smiles(smiles))
            .await
            .map_err(anyhow::Error::msg)?
            .document
            .context("Missing drawing")?;
        for label in [None, Some("OMe".into()), Some("invalid".into())] {
            let mut request = Request::molecule("abbreviate", doc.clone());
            request.text = label;
            let expected = reference.execute(request.clone()).await;
            let local = local.clone();
            tasks.spawn(async move { (local.execute(request).await, expected) });
        }
    }
    while let Some(result) = tasks.join_next().await {
        match result? {
            (Ok(actual), Ok(expected)) => matches(actual, expected)?,
            (Err(actual), Err(expected)) => anyhow::ensure!(actual == expected),
            (actual, expected) => {
                anyhow::bail!("Concurrent outcome mismatch: {actual:?} != {expected:?}")
            }
        }
    }
    Ok(())
}

fn replacement(document: Document, selection: Vec<u64>, label: &str) -> Request {
    let mut request = Request::molecule("abbreviate", document);
    request.format = Some("replace".into());
    request.selected_ids = Some(selection);
    request.text = Some(label.into());
    request
}

#[tokio::test]
async fn replacement_preserves_full_responses_geometry_selection_and_undo() -> anyhow::Result<()> {
    let local = LocalEngine::default();
    let reference = PythonEngine::default();
    let mut cases = 0;
    for preset in abbreviations::presets()? {
        for (origin, outside) in [
            ((0.1, -0.2), (25.3, 33.4)),
            ((21.25, -17.5), (-3.95, 16.1)),
            ((21.25, -17.5), (46.45, -51.1)),
            ((0.0, 0.0), (25.2, 33.6)),
            ((-234_567.13, 456_789.25), (-231_207.13, 454_269.25)),
            ((0.0, 0.0), (0.15, 0.0)),
        ] {
            let mut doc = Document::default();
            let selected = doc.add_atom("C", Point::new(origin.0, origin.1));
            let neighbor = doc.add_atom("C", Point::new(outside.0, outside.1));
            doc.add_bond(selected, neighbor, 1, "plain");
            let request = replacement(doc, vec![selected], &preset.label);
            if let Err(error) = compare(&local, &reference, request).await {
                // Native template math may depend on the physical CPU even
                // when the Python executable ABI matches the stored fixture.
                // Preserve strict comparisons and capture fresh source data.
                let diagnostic = native_geometry_diagnostic()
                    .unwrap_or_else(|error| format!("Geometry diagnostic failed: {error}"));
                return Err(error).with_context(|| {
                    format!(
                        "{} / {origin:?}/{outside:?}\nNative template geometry: {diagnostic}",
                        preset.label,
                    )
                });
            }
            cases += 1;
        }
        let mut doc = Document::default();
        let selected = doc.add_atom("C", Point::new(20.25, -34.125));
        let request = replacement(doc, vec![selected], &preset.label);
        let result = compare(&local, &reference, request)
            .await?
            .context("No replacement")?;
        let members = result
            .abbreviations
            .first()
            .context("No group")?
            .members
            .clone();
        // Replacing a whole collapsed group retains its attachment and removes
        // the old members without disturbing the remaining document.
        compare(&local, &reference, replacement(result, members, "OMe")).await?;
        cases += 2;
    }
    for text in [
        "C",
        "CCC",
        "C=O",
        "N[C@@H](C)C(=O)O",
        "[13CH3:4]OC",
        "F/C=C/CC",
    ] {
        let doc = reference
            .execute(Request::import_smiles(text))
            .await
            .map_err(anyhow::Error::msg)?
            .document
            .context("No drawing")?;
        let mut value = serde_json::to_value(&doc)?;
        value["atoms"][0]["text_style"] = serde_json::to_value(reshiki::typography::TextStyle {
            color: [17, 126, 108],
            size_pt: 12.,
            ..Default::default()
        })?;
        value["atoms"][0]["cip_label"] = "stale".into();
        let doc: Document = serde_json::from_value(value)?;
        for selection in [vec![], vec![1], vec![2], vec![999], doc.all_ids()] {
            for label in ["OMe", "Ph", "invalid"] {
                compare(
                    &local,
                    &reference,
                    replacement(doc.clone(), selection.clone(), label),
                )
                .await
                .with_context(|| format!("{text}/{selection:?}/{label}"))?;
                cases += 1;
            }
        }
    }
    for (x, label) in [(0., "OMe"), (0.149, "OMe"), (42., "invalid")] {
        let mut doc = Document::default();
        let selected = doc.add_atom("C", Point::new(0., 0.));
        let neighbor = doc.add_atom("C", Point::new(x, 0.));
        doc.add_bond(selected, neighbor, 1, "plain");
        compare(&local, &reference, replacement(doc, vec![selected], label)).await?;
        cases += 1;
    }
    for label in ["OMe", "invalid"] {
        compare(
            &local,
            &reference,
            replacement(Document::default(), vec![], label),
        )
        .await?;
        cases += 1;
    }
    eprintln!("Verified {cases} complete replacement responses with atomic undo");
    Ok(())
}

#[tokio::test]
async fn concurrent_replacement_keeps_geometry_and_selection_separate() -> anyhow::Result<()> {
    let local = LocalEngine::default();
    let reference = PythonEngine::default();
    let mut tasks = tokio::task::JoinSet::new();
    for (i, label) in ["OMe", "Boc", "Ph", "TBS", "invalid"].iter().enumerate() {
        let mut doc = Document::default();
        let selected = doc.add_atom("C", Point::new(i as f32 * 30.1, 0.1));
        let neighbor = doc.add_atom("C", Point::new(i as f32 * 30.1 + 25.2, 33.7));
        doc.add_bond(selected, neighbor, 1, "plain");
        for selection in [vec![selected], vec![neighbor], vec![]] {
            let request = replacement(doc.clone(), selection, label);
            let expected = reference.execute(request.clone()).await;
            let local = local.clone();
            tasks.spawn(async move { (local.execute(request).await, expected) });
        }
    }
    while let Some(result) = tasks.join_next().await {
        match result? {
            (Ok(actual), Ok(expected)) => matches(actual, expected)?,
            (Err(actual), Err(expected)) => anyhow::ensure!(actual == expected),
            (actual, expected) => {
                anyhow::bail!("Concurrent outcome mismatch: {actual:?} != {expected:?}")
            }
        }
    }
    Ok(())
}

#[derive(Clone, Default)]
struct Recorder(Arc<Mutex<Vec<Request>>>);
impl ChemistryEngine for Recorder {
    async fn execute(&self, request: Request) -> Result<Response, String> {
        self.0
            .lock()
            .map_err(|_| "Poisoned recorder")?
            .push(request.clone());
        Ok(Response {
            document: request.document,
            analysis: None,
            output: None,
            engine_version: "test".into(),
            warnings: vec![],
        })
    }
}

#[tokio::test]
async fn replacement_runs_before_the_backend_and_errors_never_publish_edits() -> anyhow::Result<()>
{
    let recorder = Recorder::default();
    let local = LocalEngine::with_backend(recorder.clone());
    let mut doc = Document::default();
    let selected = doc.add_atom("C", Point::new(0.1, -0.2));
    let outside = doc.add_atom("C", Point::new(25.3, 33.4));
    doc.add_bond(selected, outside, 1, "plain");
    let request = replacement(doc.clone(), vec![selected], "Ph");
    let result = local
        .execute(request.clone())
        .await
        .map_err(anyhow::Error::msg)?
        .document
        .context("No drawing")?;
    anyhow::ensure!(result.atoms.len() == 7 && result.bonds.len() == 7);
    anyhow::ensure!(result.abbreviations.len() == 1);
    anyhow::ensure!(request.document.as_ref() == Some(&doc));
    {
        let recorded = recorder
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("Poisoned recorder"))?;
        let forwarded = recorded.first().context("No finalization request")?;
        anyhow::ensure!(forwarded.operation == "finish_abbreviation");
        anyhow::ensure!(forwarded.document.as_ref() == Some(&result));
    }
    for (selection, label, error) in [
        (vec![selected], "invalid", "Choose a defined abbreviation"),
        (
            vec![],
            "Ph",
            "Select one terminal atom or one abbreviation to replace",
        ),
    ] {
        let request = replacement(doc.clone(), selection, label);
        anyhow::ensure!(local.execute(request.clone()).await.err().as_deref() == Some(error));
        anyhow::ensure!(request.document.as_ref() == Some(&doc));
    }
    anyhow::ensure!(
        recorder
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("Poisoned recorder"))?
            .len()
            == 1
    );
    Ok(())
}

#[tokio::test]
async fn detection_runs_before_the_backend_and_invalid_requests_are_atomic() -> anyhow::Result<()> {
    let mut doc = Document::default();
    let n = doc.add_atom("N", Point::new(0., 0.));
    let o = doc.add_atom("O", Point::new(42., 0.));
    let c = doc.add_atom("C", Point::new(63., 36.373));
    doc.add_bond(n, o, 1, "plain");
    doc.add_bond(o, c, 1, "plain");
    let recorder = Recorder::default();
    let local = LocalEngine::with_backend(recorder.clone());
    let before = doc.clone();
    let mut request = Request::molecule("abbreviate", doc);
    request.text = Some("OMe".into());
    let result = local
        .execute(request.clone())
        .await
        .map_err(anyhow::Error::msg)?
        .document
        .context("No drawing")?;
    anyhow::ensure!(result.abbreviations.len() == 1);
    anyhow::ensure!(result.atoms == before.atoms && result.bonds == before.bonds);
    {
        let recorded = recorder
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("Poisoned recorder"))?;
        let forwarded = recorded.first().context("No finalization request")?;
        anyhow::ensure!(forwarded.operation == "finish_abbreviation");
        anyhow::ensure!(forwarded.document.as_ref() == Some(&result));
    }
    request.text = Some("invalid".into());
    anyhow::ensure!(
        local.execute(request.clone()).await.err().as_deref() == Some("Unknown abbreviation")
    );
    anyhow::ensure!(request.document.as_ref() == Some(&before));
    request.document.as_mut().context("No drawing")?.version = 999;
    anyhow::ensure!(local.execute(request.clone()).await.is_err());
    request.document = None;
    anyhow::ensure!(local.execute(request.clone()).await.is_err());
    request.protocol = 2;
    anyhow::ensure!(local.execute(request).await.is_err());
    anyhow::ensure!(
        recorder
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("Poisoned recorder"))?
            .len()
            == 1
    );
    Ok(())
}
