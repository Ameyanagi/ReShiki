use anyhow::Context;
use reshiki::{
    chemistry::{RDKIT_VERSION, reaction},
    document::Document,
};
use serde::Deserialize;
use serde_json::Value;
use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};

#[derive(Deserialize)]
struct Case {
    name: String,
    document: Value,
    selected: Option<Vec<u64>>,
    expected: Option<String>,
    failure: Option<String>,
}

#[test]
fn reaction_files_match_native_writer_and_rejections() -> anyhow::Result<()> {
    output_matches_reference("rxn")
}

#[test]
fn reaction_smiles_match_native_writer_and_rejections() -> anyhow::Result<()> {
    output_matches_reference("rsmi")
}

fn output_matches_reference(format: &str) -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/reaction_output_reference.py"))
        .args((format == "rsmi").then_some("--smiles"))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().context("Missing reaction oracle")?).lines();
    let version: Value = serde_json::from_str(&lines.next().context("Missing oracle version")??)?;
    anyhow::ensure!(version["rdkit_version"] == RDKIT_VERSION);
    let (mut accepted, mut rejected, mut mismatches) = (0, 0, 0);
    let mut failures = Vec::new();
    for line in lines {
        let line = line?;
        let case: Case = serde_json::from_str(&line)?;
        let actual = serde_json::from_value::<Document>(case.document.clone())
            .map_err(anyhow::Error::from)
            .and_then(|doc| {
                if format == "rsmi" {
                    reaction::write_smiles(&doc, case.selected.as_deref()).map_err(Into::into)
                } else {
                    reaction::write_rxn(&doc, case.selected.as_deref()).map_err(Into::into)
                }
            });
        let failure = match (&actual, &case.expected) {
            (Ok(actual), Some(expected)) if actual == expected => {
                accepted += 1;
                None
            }
            (Err(_), None) => {
                rejected += 1;
                None
            }
            (Err(error), Some(_)) => Some(format!("Unexpected rejection: {error}")),
            (Ok(_), None) => Some(format!("Accepted native rejection: {:?}", case.failure)),
            (Ok(actual), Some(expected)) => Some(format!(
                "{format} differs at line {:?}",
                actual
                    .lines()
                    .zip(expected.lines())
                    .position(|(a, e)| a != e)
            )),
        };
        if let Some(failure) = failure {
            mismatches += 1;
            if failures.len() < 20 {
                failures.push(format!("{}: {failure}", case.name));
            }
            if mismatches == 1 {
                std::fs::create_dir_all(root.join("artifacts"))?;
                std::fs::write(
                    root.join(format!("artifacts/reaction-{format}-mismatch.json")),
                    line,
                )?;
                if let Ok(actual) = actual {
                    std::fs::write(
                        root.join(format!("artifacts/reaction-actual.{format}")),
                        actual,
                    )?;
                }
            }
        }
    }
    anyhow::ensure!(child.wait()?.success(), "Reaction oracle failed");
    eprintln!("{format} output: {accepted} accepted, {rejected} rejected, {mismatches} mismatches");
    anyhow::ensure!(mismatches == 0, "{}", failures.join("\n"));
    anyhow::ensure!(
        accepted > 5000 && rejected > 100,
        "Insufficient reaction coverage"
    );
    Ok(())
}

#[tokio::test]
async fn reaction_export_runs_without_a_python_backend() -> anyhow::Result<()> {
    for format in ["rxn", "rsmi"] {
        offline_export(format).await?;
    }
    Ok(())
}

async fn offline_export(format: &str) -> anyhow::Result<()> {
    use reshiki::{
        document::{Arrow, Point},
        engine::{ChemistryEngine, LocalEngine, Request, Response},
        reactions::{Participant, Reaction},
    };
    struct Unavailable;
    impl ChemistryEngine for Unavailable {
        async fn execute(&self, _: Request) -> Result<Response, String> {
            Err("The backend must not be called for native reaction export".into())
        }
    }
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::default());
    let b = doc.add_atom("O", Point::new(420., 0.));
    doc.arrows.push(Arrow::new(
        3,
        Point::new(100., 0.),
        Point::new(300., 0.),
        Default::default(),
        Default::default(),
    ));
    let mut roles = Reaction::new(3);
    roles.reactants.push(Participant {
        atoms: vec![a],
        coefficient: 1,
    });
    roles.products.push(Participant {
        atoms: vec![b],
        coefficient: 1,
    });
    doc.reactions.push(roles);
    let mut request = Request::molecule("export", doc.clone());
    request.format = Some(format.into());
    let response = LocalEngine::with_backend(Unavailable)
        .execute(request)
        .await
        .map_err(anyhow::Error::msg)?;
    anyhow::ensure!(response.document.is_none() && response.analysis.is_none());
    anyhow::ensure!(response.engine_version == RDKIT_VERSION);
    let expected = if format == "rsmi" {
        reaction::write_smiles(&doc, None)?
    } else {
        reaction::write_rxn(&doc, None)?
    };
    anyhow::ensure!(response.output.as_deref() == Some(expected.as_str()));
    anyhow::ensure!(response.warnings.len() == 1);
    let mut invalid = doc;
    invalid.atoms.first_mut().context("Missing atom")?.map_num = 9;
    invalid
        .reactions
        .first_mut()
        .context("Missing reaction")?
        .reactants
        .first_mut()
        .context("Missing participant")?
        .coefficient = 2;
    let mut request = Request::molecule("export", invalid);
    request.format = Some(format.into());
    let error = LocalEngine::with_backend(Unavailable)
        .execute(request)
        .await
        .err()
        .context("Repeated mapped participants should fail")?;
    anyhow::ensure!(error.contains("Atom map numbers"), "{error}");
    Ok(())
}

#[tokio::test]
async fn complete_reaction_export_responses_match_the_original_engine() -> anyhow::Result<()> {
    use reshiki::engine::{ChemistryEngine, LocalEngine, PythonEngine, Request};
    let reference = PythonEngine::default();
    let local = LocalEngine::default();
    for text in [
        "[CH3:1][OH:2]>O>[CH2:1]=[O:2]",
        "C[C@H](O)Cl>O>C[C@@H](O)Cl",
        "F/C=C/Cl>>F/C=C\\Cl",
        "c1ccccc1.O>[Na+]>Oc1ccccc1",
    ] {
        let doc = reference
            .execute(Request::import("rsmi", text))
            .await
            .map_err(anyhow::Error::msg)?
            .document
            .context("Missing reaction")?;
        for format in ["rxn", "rsmi"] {
            let mut request = Request::molecule("export", doc.clone());
            request.format = Some(format.into());
            let expected = reference
                .execute(request.clone())
                .await
                .map_err(anyhow::Error::msg)?;
            let actual = local.execute(request).await.map_err(anyhow::Error::msg)?;
            anyhow::ensure!(
                serde_json::to_value(actual)? == serde_json::to_value(expected)?,
                "{text}"
            );
        }
    }
    Ok(())
}

#[tokio::test]
async fn dense_ring_reaction_export_never_calls_the_backend() -> anyhow::Result<()> {
    use reshiki::{
        chemistry::graph::Graph,
        document::{Arrow, Point},
        engine::{ChemistryEngine, LocalEngine, PythonEngine, Request, Response},
        reactions::{Participant, Reaction},
    };
    let graph: Graph = serde_json::from_str(include_str!("fixtures/ring-order-dependent.json"))?;
    let mut doc = Document::default();
    let ids: Vec<_> = graph
        .atoms
        .iter()
        .enumerate()
        .map(|(i, _)| doc.add_atom("*", Point::new(i as f32 * 42., 0.)))
        .collect();
    for bond in graph.bonds {
        doc.add_bond(
            *ids.get(bond.a).context("Missing endpoint")?,
            *ids.get(bond.b).context("Missing endpoint")?,
            bond.order,
            "plain",
        );
    }
    let product = doc.add_atom("O", Point::new(0., 100.));
    let arrow = product.checked_add(1).context("ID limit")?;
    doc.arrows.push(Arrow::new(
        arrow,
        Point::new(0., 50.),
        Point::new(100., 50.),
        Default::default(),
        Default::default(),
    ));
    let mut roles = Reaction::new(arrow);
    roles.reactants.push(Participant {
        atoms: ids,
        coefficient: 1,
    });
    roles.products.push(Participant {
        atoms: vec![product],
        coefficient: 1,
    });
    doc.reactions.push(roles);
    struct Unavailable;
    impl ChemistryEngine for Unavailable {
        async fn execute(&self, _: Request) -> Result<Response, String> {
            Err("Dense-ring export called the backend".into())
        }
    }
    let reference = PythonEngine::default();
    for format in ["rxn", "rsmi"] {
        let mut request = Request::molecule("export", doc.clone());
        request.format = Some(format.into());
        let expected = reference
            .execute(request.clone())
            .await
            .map_err(anyhow::Error::msg)?;
        let result = LocalEngine::with_backend(Unavailable)
            .execute(request)
            .await
            .map_err(anyhow::Error::msg)?;
        anyhow::ensure!(
            result.output.as_ref().is_some_and(|s| !s.is_empty()) && result.document.is_none()
        );
        assert_eq!(
            serde_json::to_value(result)?,
            serde_json::to_value(expected)?
        );
    }
    Ok(())
}
