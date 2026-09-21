use anyhow::Context;
use reshiki::{
    chemistry::{
        RDKIT_VERSION,
        document::{Identity, aromatic_display},
    },
    document::{Document, History, Point},
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
    document: Document,
    selection: Vec<u64>,
    expected: Option<Document>,
    before: Option<Value>,
    after: Option<Value>,
    identity: Option<Identity>,
    failure: Option<String>,
}

#[test]
fn selected_aromatic_displays_match_reference_without_changing_identity() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/aromatic_display_reference.py"))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().context("Missing oracle output")?).lines();
    let version: Value = serde_json::from_str(&lines.next().context("Missing oracle version")??)?;
    assert_eq!(version["rdkit_version"], RDKIT_VERSION);
    let (mut total, mut changed, mut rejected, mut mismatches) = (0, 0, 0, 0);
    let mut failures = Vec::new();
    for line in lines {
        let line = line?;
        let case: Case = serde_json::from_str(&line)?;
        total += 1;
        let original = case.document.clone();
        let result = aromatic_display(&case.document, &case.selection);
        let failure = match (result, &case.expected) {
            (Ok(draft), Some(expected)) => {
                changed += 1;
                if serde_json::to_value(draft.before())? != serde_json::to_value(&case.before)? {
                    Some("Original chemical state differs".into())
                } else if serde_json::to_value(draft.after())? != serde_json::to_value(&case.after)?
                {
                    Some("Changed chemical state differs".into())
                } else {
                    let actual = draft.finish(case.identity.context("Missing identifiers")?)?;
                    if actual == *expected {
                        None
                    } else {
                        Some("Editable document differs".into())
                    }
                }
            }
            (Err(_), None) => {
                rejected += 1;
                None
            }
            (Err(error), Some(_)) => Some(format!("Unexpected error: {error}")),
            (Ok(_), None) => Some(format!("Accepted reference failure: {:?}", case.failure)),
        };
        assert_eq!(case.document, original);
        if let Some(error) = failure {
            mismatches += 1;
            if failures.len() < 12 {
                failures.push(format!("{}: {error}", case.name));
            }
            if mismatches == 1 {
                std::fs::create_dir_all(root.join("artifacts"))?;
                std::fs::write(root.join("artifacts/aromatic-first-mismatch.json"), line)?;
            }
        }
    }
    assert!(child.wait()?.success(), "Oracle failed");
    eprintln!(
        "Verified {total} ring displays: {changed} changes, {rejected} rejected, {mismatches} mismatches"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(changed > 5000 && rejected > 500, "Insufficient coverage");
    Ok(())
}

#[test]
fn identity_checks_are_required_and_display_is_one_undoable_edit() -> anyhow::Result<()> {
    let mut original = Document::default();
    reshiki::editing::ring(&mut original, Point::default(), 6, false, 42.);
    for (i, bond) in original.bonds.iter_mut().enumerate() {
        bond.order = if i % 2 == 0 { 2 } else { 1 };
    }
    let selection: Vec<_> = original.atoms.iter().map(|a| a.id).collect();
    for (version, before, after) in [
        ("wrong", "c1ccccc1", "c1ccccc1"),
        (RDKIT_VERSION, "", ""),
        (RDKIT_VERSION, "c1ccccc1", "C1CCCCC1"),
    ] {
        let draft = aromatic_display(&original, &selection)?;
        assert!(
            draft
                .finish(Identity {
                    rdkit_version: version.into(),
                    before: before.into(),
                    after: after.into(),
                })
                .is_err()
        );
    }
    let draft = aromatic_display(&original, &selection)?;
    let mut changed = draft.finish(Identity {
        rdkit_version: RDKIT_VERSION.into(),
        before: "c1ccccc1".into(),
        after: "c1ccccc1".into(),
    })?;
    assert!(changed.bonds.iter().all(|b| b.order == 4));
    let mut history = History::default();
    assert!(history.commit(original.clone(), &changed));
    assert!(history.undo(&mut changed));
    assert_eq!(changed, original);
    Ok(())
}
