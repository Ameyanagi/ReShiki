//! Exercise actual asynchronous requests with native labeling disabled in the child.
use super::*;
use crate::document::{History, Point};
use anyhow::Context;

async fn guarded(record: &std::path::Path, inject: bool) -> anyhow::Result<LocalEngine> {
    guarded_script(
        record,
        "cip_engine_guard.py",
        inject.then_some("inject-labels"),
    )
    .await
}

async fn guarded_script(
    record: &std::path::Path,
    script: &str,
    inject: Option<&str>,
) -> anyhow::Result<LocalEngine> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut command = Command::new(root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    }));
    command
        .arg("-u")
        .arg(root.join("tests").join(script))
        .arg(record)
        .env("PYTHONUTF8", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true);
    if let Some(inject) = inject {
        command.arg(inject);
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

#[tokio::test]
async fn native_inchikey_is_never_called_for_any_analysis_path() -> anyhow::Result<()> {
    let temporary = tempfile::tempdir()?;
    let record = temporary.path().join("requests.jsonl");
    let local = guarded_script(&record, "inchi_engine_guard.py", None).await?;
    let reference = PythonEngine::default();
    let mut responses = 0;
    let mut concurrent = tokio::task::JoinSet::new();
    for text in [
        "N[C@@H](C)C(=O)O",
        "F/C=C/F",
        "[13CH3:4][NH3+]",
        "COc1ccccc1",
        "[Na+].[O-]C(=O)C",
        "[CH3]",
    ] {
        let imported = compare(&local, &reference, Request::import_smiles(text)).await?;
        let doc = imported.document.context("Missing imported drawing")?;
        responses += 1;
        for format in ["mol", "smiles", "inchi", "cdxml", "cdx"] {
            let mut request = Request::molecule("export", doc.clone());
            request.format = Some(format.into());
            let exported = compare(&local, &reference, request).await?;
            responses += 1;
            if let Some(output) = exported.output.filter(|s| !s.is_empty()) {
                compare(&local, &reference, Request::import(format, &output)).await?;
                responses += 1;
            }
        }
        for operation in ["analyze", "abbreviate", "clean"] {
            let mut request = Request::molecule(operation, doc.clone());
            request.selected_ids = Some(doc.all_ids());
            compare(&local, &reference, request).await?;
            responses += 1;
        }
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
    // An unspecified atom has no InChI, but remains a valid drawing. Its MOL
    // export is a query and is outside this engine's molecular import contract.
    let doc = compare(&local, &reference, Request::import_smiles("*C"))
        .await?
        .document
        .context("Missing unspecified-atom drawing")?;
    compare(&local, &reference, Request::molecule("analyze", doc)).await?;
    responses += 2;
    // Exotic bonds deliberately have no InChI; Rust must retain empty keys.
    for order in [0, 5, 6, 7] {
        let mut doc = Document::default();
        let a = doc.add_atom(if order == 0 { "H" } else { "C" }, Point::new(0., 0.));
        let b = doc.add_atom(if order == 0 { "O" } else { "C" }, Point::new(42., 0.));
        if order == 0 {
            let donor = doc.add_atom("O", Point::new(-42., 0.));
            doc.add_bond(donor, a, 1, "plain");
        }
        doc.add_bond(a, b, order, if order == 0 { "dotted" } else { "plain" });
        compare(&local, &reference, Request::molecule("analyze", doc)).await?;
        responses += 1;
    }
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(0., 0.));
    let b = doc.add_atom("C", Point::new(42., 0.));
    doc.add_bond(a, b, 1, "plain");
    let mut replacement = Request::molecule("abbreviate", doc);
    replacement.format = Some("replace".into());
    replacement.text = Some("OMe".into());
    replacement.selected_ids = Some(vec![a]);
    compare(&local, &reference, replacement).await?;
    responses += 1;
    let doc = compare(&local, &reference, Request::import_smiles("c1ccccc1"))
        .await?
        .document
        .context("Missing aromatic drawing")?;
    let mut aromatic = Request::molecule("aromatic", doc.clone());
    aromatic.selected_ids = Some(doc.all_ids());
    compare(&local, &reference, aromatic).await?;
    responses += 2;
    for text in ["F/C=C/F>O>F/C=C\\F", "N[C@@H](C)C(=O)O>>N[C@H](C)C(=O)O"] {
        let imported = compare(&local, &reference, Request::import("rsmi", text)).await?;
        let mut request =
            Request::molecule("export", imported.document.context("Missing reaction")?);
        request.format = Some("rxn".into());
        let exported = compare(&local, &reference, request).await?;
        compare(
            &local,
            &reference,
            Request::import("rxn", exported.output.as_deref().context("Missing RXN")?),
        )
        .await?;
        responses += 3;
    }
    for format in ["cdxml", "cdx"] {
        let mut request = Request::molecule("export", Document::default());
        request.format = Some(format.into());
        compare(&local, &reference, request).await?;
        responses += 1;
    }
    let requests: Vec<serde_json::Value> = std::fs::read_to_string(record)?
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    anyhow::ensure!(requests.iter().filter(|r| r["analysis"] == true).count() >= 95);
    anyhow::ensure!(
        requests
            .iter()
            .all(|r| r["inchi_calls"].as_u64().is_some_and(|n| n <= 1))
    );
    // CDX passes through the Rust binary codec before reaching this worker.
    for format in ["inchi", "cdxml", "mol", "rxn", "rsmi"] {
        anyhow::ensure!(
            requests
                .iter()
                .any(|r| r["operation"] == "import" && r["format"] == format),
            "Missing guarded import: {format}"
        );
    }
    eprintln!(
        "Verified {responses} complete responses with native InChIKey forbidden, one InChI generation per request, and atomic undo"
    );
    Ok(())
}

#[tokio::test]
async fn invalid_native_identifier_responses_cannot_publish_edits() -> anyhow::Result<()> {
    for (fault, message) in [
        ("native-key", "Unexpected native InChIKey"),
        ("missing-inchi", "Missing molecular InChI input"),
        ("invalid-inchi", "Invalid molecular InChI:"),
    ] {
        let temporary = tempfile::tempdir()?;
        let local = guarded_script(
            &temporary.path().join("requests.jsonl"),
            "inchi_engine_guard.py",
            Some(fault),
        )
        .await?;
        let mut doc = Document::default();
        doc.add_atom("C", Point::default());
        let before = doc.clone();
        let error = local
            .execute(Request::molecule("analyze", doc.clone()))
            .await
            .err()
            .context("Malformed identifier response was accepted")?;
        anyhow::ensure!(error.starts_with(message), "Unexpected error: {error}");
        anyhow::ensure!(doc == before);
    }
    Ok(())
}
