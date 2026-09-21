use anyhow::Context;
use reshiki::chemistry::{
    RDKIT_VERSION,
    smarts::{self, Error},
};
use serde::Deserialize;
use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};

#[derive(Deserialize)]
struct Case {
    text: String,
    expected: Option<usize>,
}

#[test]
fn smarts_validation_matches_native_parser() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut child = Command::new(root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    }))
    .arg(root.join("tests/smarts_reference.py"))
    .env("PYTHONUTF8", "1")
    .stdout(Stdio::piped())
    .stderr(Stdio::inherit())
    .spawn()?;
    let mut lines = BufReader::new(
        child
            .stdout
            .take()
            .context("Missing SMARTS reference output")?,
    )
    .lines();
    let version: serde_json::Value =
        serde_json::from_str(&lines.next().context("Missing version")??)?;
    assert_eq!(version["rdkit_version"], RDKIT_VERSION);
    let (mut accepted, mut rejected) = (0, 0);
    let mut failures = Vec::new();
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        let actual = match smarts::validate(&case.text) {
            Ok(n) => {
                accepted += 1;
                Some(n)
            }
            Err(Error::Syntax(_)) => {
                rejected += 1;
                None
            }
            Err(error) => anyhow::bail!("Unexpected parser error for {:?}: {error}", case.text),
        };
        if actual != case.expected {
            failures.push(format!(
                "{:?}: {actual:?} != {:?}",
                case.text, case.expected
            ));
        }
    }
    assert!(child.wait()?.success());
    eprintln!(
        "SMARTS: {accepted} accepted, {rejected} rejected, {} mismatches",
        failures.len()
    );
    if !failures.is_empty() {
        std::fs::create_dir_all(root.join("artifacts"))?;
        std::fs::write(
            root.join("artifacts/smarts-failures.txt"),
            failures.join("\n"),
        )?;
    }
    assert!(
        failures.is_empty(),
        "{}",
        failures
            .iter()
            .take(30)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(accepted > 5000 && rejected > 1000);
    Ok(())
}

#[test]
fn parser_bounds_and_long_branches_are_safe() -> anyhow::Result<()> {
    assert!(matches!(
        smarts::validate(&"C".repeat(1024 * 1024 + 1)),
        Err(Error::Limit)
    ));
    let mut recursive = "C".to_owned();
    for _ in 0..66 {
        recursive = format!("[$({recursive})]");
    }
    assert!(matches!(smarts::validate(&recursive), Err(Error::Limit)));
    let branches = format!("{}C{}", "C(".repeat(30_000), ")".repeat(30_000));
    assert_eq!(smarts::validate(&branches)?, 30_001);
    assert_eq!(smarts::validate(&format!("[{}C]", "!".repeat(100_000)))?, 1);
    assert!(matches!(
        smarts::validate(&"C".repeat(100_001)),
        Err(Error::Limit)
    ));
    for symbol in ['酸', '🧪', '\0'] {
        for column in 0..16 {
            let text = format!(
                "[{}{}{}]",
                "C".repeat(column),
                symbol,
                "C".repeat(16 - column)
            );
            let _ = smarts::validate(&text);
        }
    }
    for section in [
        "$酸;🧪$",
        "atomProp:0.name.酸",
        "SgD:0:name:data::::",
        "wU:0.0",
        "(1,2,3)",
    ] {
        let text = format!("CC |{section}|");
        for (end, _) in text.char_indices() {
            let prefix = text.get(..end).context("Invalid test boundary")?;
            let _ = smarts::validate(prefix);
        }
    }
    let large = format!(
        "{} |u:{}|",
        "C".repeat(30_000),
        (0..30_000)
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(",")
    );
    assert_eq!(smarts::validate(&large)?, 30_000);
    Ok(())
}
