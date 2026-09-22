//! Compare the migrated application path with the unmodified abbreviation worker.
use anyhow::Context;
use reshiki::{
    chemistry::abbreviations,
    document::{Document, History, Point},
    engine::{ChemistryEngine, LocalEngine, PythonEngine, Request, Response},
};
use std::sync::{Arc, Mutex};

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
