//! Complete aromatic responses compared with the retained original worker.
use anyhow::Context;
use reshiki::{
    document::{Document, History, Point},
    engine::{
        ChemistryEngine, PythonEngine, Request, Response, native_aromatic, native_response::Config,
    },
};
use serde::Deserialize;
use std::{
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
};

fn helper() -> anyhow::Result<Option<PathBuf>> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("artifacts/inchi-helper")
        .join(if cfg!(windows) {
            "reshiki-inchi-helper.exe"
        } else {
            "reshiki-inchi-helper"
        });
    if path.is_file() {
        return Ok(Some(path));
    }
    anyhow::ensure!(
        std::env::var_os("RESHIKI_REQUIRE_INCHI_HELPER").is_none(),
        "Build the pinned native helper first"
    );
    eprintln!("Skipping optional native aromatic responses; build the helper first");
    Ok(None)
}

fn request(document: Document, selected: Vec<u64>) -> Request {
    let mut request = Request::molecule("aromatic", document);
    request.selected_ids = Some(selected);
    request
}

fn equal(actual: Response, expected: Response) -> anyhow::Result<()> {
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
        "Response changed: {actual}\nExpected: {expected}"
    );
    Ok(())
}

#[derive(Deserialize)]
struct Case {
    name: String,
    document: Document,
    selection: Vec<u64>,
}

#[tokio::test]
async fn complete_aromatic_responses_match_original_worker() -> anyhow::Result<()> {
    let Some(helper) = helper()? else {
        return Ok(());
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    // This existing independent corpus includes full/partial selections,
    // individual fused rings, both toggle directions and abbreviation failures.
    let mut child = Command::new(python)
        .arg(root.join("tests/aromatic_display_reference.py"))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().context("Missing corpus")?).lines();
    let header: serde_json::Value =
        serde_json::from_str(&lines.next().context("Missing corpus version")??)?;
    assert_eq!(header["rdkit_version"], reshiki::chemistry::RDKIT_VERSION);
    let reference = PythonEngine::default();
    let config = Config::new(helper);
    let (mut accepted, mut rejected) = (0, 0);
    let mut failures = Vec::new();
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        let request = Arc::new(request(case.document, case.selection));
        let before = serde_json::to_value(&*request)?;
        let expected = reference.execute((*request).clone()).await;
        let actual = native_aromatic::execute(Arc::clone(&request), Some(config.clone())).await;
        assert_eq!(
            before,
            serde_json::to_value(&*request)?,
            "{} changed input",
            case.name
        );
        let result = match (actual, expected) {
            (Ok(actual), Ok(expected)) => {
                accepted += 1;
                equal(actual, expected)
            }
            (Err(actual), Err(expected)) => {
                rejected += 1;
                if actual.to_string() == expected {
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("Error changed: {actual} != {expected}"))
                }
            }
            (actual, expected) => Err(anyhow::anyhow!(
                "Outcome changed: {actual:?} != {expected:?}"
            )),
        };
        if let Err(error) = result {
            failures.push(format!("{}: {error}", case.name));
            if failures.len() == 12 {
                child.kill()?;
                break;
            }
        }
    }
    let status = child.wait()?;
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(status.success(), "Independent corpus failed");
    assert!(accepted > 5000 && rejected > 500, "Insufficient coverage");
    eprintln!(
        "Native aromatic responses: {accepted} exact responses, {rejected} matching rejections"
    );
    Ok(())
}

fn benzene() -> Document {
    let mut document = Document::default();
    reshiki::editing::ring(&mut document, Point::default(), 6, false, 42.);
    for (index, bond) in document.bonds.iter_mut().enumerate() {
        bond.order = if index % 2 == 0 { 2 } else { 1 };
        bond.color = [17, 126, 108];
    }
    document
}

#[tokio::test]
async fn display_response_is_one_undoable_edit_and_failures_publish_nothing() -> anyhow::Result<()>
{
    let original = benzene();
    let selected = original.atoms.iter().map(|a| a.id).collect();
    let request = Arc::new(request(original.clone(), selected));
    let before = serde_json::to_value(&*request)?;
    let missing = Config::new(PathBuf::from("missing-helper"));
    let error = native_aromatic::execute(Arc::clone(&request), Some(missing)).await;
    assert!(matches!(error, Err(native_aromatic::Error::Analysis(_))));
    assert_eq!(before, serde_json::to_value(&*request)?);
    let Some(helper) = helper()? else {
        return Ok(());
    };
    let response = native_aromatic::execute(request, Some(Config::new(helper))).await?;
    let mut document = response.document.context("Missing display edit")?;
    assert!(document.bonds.iter().all(|b| b.order == 4));
    let changed = document.clone();
    let mut history = History::default();
    assert!(history.commit(original.clone(), &document));
    assert!(history.undo(&mut document));
    assert_eq!(document, original);
    assert!(!history.can_undo());
    assert!(history.redo(&mut document));
    assert_eq!(document, changed);
    Ok(())
}

#[tokio::test]
async fn validation_does_not_resolve_a_helper_and_matches_original_errors() -> anyhow::Result<()> {
    let reference = PythonEngine::default();
    let document = benzene();
    let selected: Vec<_> = document.atoms.iter().map(|a| a.id).collect();
    let first = *selected.first().context("Missing benzene atom")?;
    let mut cases = vec![
        request(document.clone(), vec![]),
        request(document.clone(), vec![first, u64::MAX]),
        request(Document::default(), vec![1]),
    ];
    let mut invalid = document.clone();
    let duplicate = invalid.atoms.get(1).context("Missing second atom")?.id;
    invalid.atoms.first_mut().context("Missing first atom")?.id = duplicate;
    cases.push(request(invalid, selected.clone()));
    let mut missing = request(document.clone(), selected.clone());
    missing.document = None;
    cases.push(missing);
    let mut protocol = request(document, selected);
    protocol.protocol = 2;
    cases.push(protocol);
    for request in cases {
        let expected = reference
            .execute(request.clone())
            .await
            .err()
            .context("Expected rejection")?;
        let error = native_aromatic::execute(
            request,
            Some(Config::new(PathBuf::from("helper-must-not-be-discovered"))),
        )
        .await
        .err()
        .context("Native validation accepted request")?;
        assert_eq!(error.to_string(), expected);
    }
    Ok(())
}
