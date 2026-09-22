//! Exercise actual asynchronous requests with native labeling disabled in the child.
use super::*;
use crate::document::{History, Point};
use anyhow::Context;

async fn guarded(record: &std::path::Path, inject: bool) -> anyhow::Result<LocalEngine> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut command = Command::new(root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    }));
    command
        .arg("-u")
        .arg(root.join("tests/cip_engine_guard.py"))
        .arg(record)
        .env("PYTHONUTF8", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true);
    if inject {
        command.arg("inject-labels");
    }
    let mut child = command.spawn()?;
    let worker = Worker {
        input: child.stdin.take().context("Missing guarded worker input")?,
        output: BufReader::new(
            child
                .stdout
                .take()
                .context("Missing guarded worker output")?,
        ),
        _child: child,
        next_id: 1,
    };
    let engine = LocalEngine::default();
    *engine.chemistry.worker.lock().await = Some(worker);
    Ok(engine)
}

fn matches(actual: &Response, expected: &Response) -> anyhow::Result<()> {
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
) -> anyhow::Result<Response> {
    let before = request.document.clone();
    let expected = reference
        .execute(request.clone())
        .await
        .map_err(anyhow::Error::msg)?;
    let actual = local.execute(request).await.map_err(anyhow::Error::msg)?;
    matches(&actual, &expected)?;
    if let (Some(before), Some(after)) = (before, &actual.document) {
        let mut history = History::default();
        let mut doc = after.clone();
        history.commit(before.clone(), &doc);
        history.undo(&mut doc);
        anyhow::ensure!(doc == before, "Undo changed the original drawing");
    }
    Ok(actual)
}

#[tokio::test]
async fn native_cip_is_never_called_for_migrated_app_operations() -> anyhow::Result<()> {
    let temporary = tempfile::tempdir()?;
    let record = temporary.path().join("requests.jsonl");
    let local = guarded(&record, false).await?;
    let reference = PythonEngine::default();
    let mut responses = 0;
    let mut concurrent = tokio::task::JoinSet::new();
    for text in [
        "N[C@@H](C)C(=O)O",
        "F/C=C/F",
        "F/C=C\\F",
        "C[S@](=O)CC",
        "C[C@H]1CC[C@@H](C)CC1",
        "[13CH3:4][NH3+]",
        "COc1ccccc1",
    ] {
        let imported = compare(&local, &reference, Request::import_smiles(text)).await?;
        responses += 1;
        let mut doc = imported.document.context("Missing guarded import")?;
        doc.atoms.reverse();
        doc.bonds.reverse();
        for atom in &mut doc.atoms {
            atom.cip_label = Some("S".into());
            atom.position.x = -atom.position.x;
        }
        for bond in &mut doc.bonds {
            bond.cip_label = Some("E".into());
            bond.color = [17, 126, 108];
        }
        for format in [
            None,
            Some("mol"),
            Some("smiles"),
            Some("inchi"),
            Some("cdxml"),
            Some("cdx"),
        ] {
            let mut request = Request::molecule(
                if format.is_some() {
                    "export"
                } else {
                    "analyze"
                },
                doc.clone(),
            );
            request.format = format.map(String::from);
            let response = compare(&local, &reference, request).await?;
            responses += 1;
            if format == Some("mol") {
                compare(
                    &local,
                    &reference,
                    Request::import("mol", response.output.as_deref().context("Missing MOL")?),
                )
                .await?;
                responses += 1;
            }
        }
        compare(
            &local,
            &reference,
            Request::molecule("abbreviate", doc.clone()),
        )
        .await?;
        responses += 1;
        let request = Request::molecule("analyze", doc);
        let expected = reference
            .execute(request.clone())
            .await
            .map_err(anyhow::Error::msg)?;
        let engine = local.clone();
        concurrent.spawn(async move { (engine.execute(request).await, expected) });
    }
    while let Some(result) = concurrent.join_next().await {
        let (actual, expected) = result?;
        matches(&actual.map_err(anyhow::Error::msg)?, &expected)?;
        responses += 1;
    }
    let mut doc = Document::default();
    let atom = doc.add_atom("C", Point::new(0.1, -0.2));
    let outside = doc.add_atom("C", Point::new(25.3, 33.4));
    doc.add_bond(atom, outside, 1, "plain");
    let mut request = Request::molecule("abbreviate", doc);
    request.format = Some("replace".into());
    request.text = Some("OMe".into());
    request.selected_ids = Some(vec![atom]);
    compare(&local, &reference, request).await?;
    responses += 1;
    for text in ["F/C=C/F>O>F/C=C\\F", "N[C@@H](C)C(=O)O>>N[C@H](C)C(=O)O"] {
        let response = compare(&local, &reference, Request::import("rsmi", text)).await?;
        let mut export =
            Request::molecule("export", response.document.context("Missing reaction")?);
        export.format = Some("rxn".into());
        let response = compare(&local, &reference, export).await?;
        compare(
            &local,
            &reference,
            Request::import("rxn", response.output.as_deref().context("Missing RXN")?),
        )
        .await?;
        responses += 3;
    }
    let requests: Vec<serde_json::Value> = std::fs::read_to_string(record)?
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    anyhow::ensure!(requests.iter().all(|r| r["operation"] != "label_reaction"));
    anyhow::ensure!(requests.iter().filter(|r| r["local_cip"] == true).count() >= 60);
    anyhow::ensure!(
        requests
            .iter()
            .filter(|r| r["prepared_reaction"] == true)
            .count()
            == 4
    );
    eprintln!("Verified {responses} complete responses and undo with native CIP forbidden");
    Ok(())
}

#[tokio::test]
async fn unexpected_native_labels_cannot_publish_a_drawing() -> anyhow::Result<()> {
    let temporary = tempfile::tempdir()?;
    let local = guarded(&temporary.path().join("requests.jsonl"), true).await?;
    let mut doc = Document::default();
    doc.add_atom("C", Point::default());
    let before = doc.clone();
    let result = local
        .execute(Request::molecule("analyze", doc.clone()))
        .await;
    anyhow::ensure!(result.err().as_deref() == Some("Unexpected native stereochemical labels"));
    anyhow::ensure!(doc == before);
    Ok(())
}
