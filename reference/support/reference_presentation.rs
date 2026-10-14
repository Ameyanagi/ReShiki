//! Explicit presentation changes that the original Python worker predates.
use anyhow::{Context, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

#[path = "reference_differences.rs"]
#[allow(dead_code)] // Diagnostics are used by root targets, not reshiki-io's bridge tests.
mod reference_differences;
#[allow(unused_imports)] // The bridge tests use only the presentation contract.
pub use reference_differences::response_differences;

/// Source-qualified bond XML IDs and donor/acceptor XML endpoints. The frozen
/// worker assigns atoms index+3 and bonds atom_count+index+3; Document bonds
/// have no persistent ID. Require unambiguous source identities independently
/// of either output. #300 / 8fbf166c removes only the dashed order-5 override.
#[allow(dead_code)] // Used by drawing_exchange, not the engine/IO consumers.
pub fn dative_export_bonds(
    source_document: Option<&Value>,
) -> anyhow::Result<BTreeMap<String, (String, String)>> {
    let mut result = BTreeMap::new();
    let Some(bonds) = source_document
        .and_then(|doc| doc.get("bonds"))
        .and_then(Value::as_array)
        .filter(|bonds| {
            bonds
                .iter()
                .any(|bond| bond["order"].as_u64() == Some(5) && bond["display"] == "dashed")
        })
    else {
        return Ok(result);
    };
    let atoms = source_document
        .and_then(|doc| doc.get("atoms"))
        .and_then(Value::as_array)
        .context("[dative-source] Missing source atoms")?;
    let identifier = |value: &Value| {
        value
            .as_u64()
            .filter(|id| *id != 0)
            .context("[dative-source] Missing or invalid source atom identity")
    };
    let mut atom_ids = HashMap::new();
    for (index, atom) in atoms.iter().enumerate() {
        ensure!(
            atom_ids
                .insert(identifier(&atom["id"])?, (index + 3).to_string())
                .is_none(),
            "[dative-source] Duplicate source atom identity"
        );
    }
    let mut pairs = HashSet::new();
    for (index, bond) in bonds.iter().enumerate() {
        let (begin, end) = (identifier(&bond["a"])?, identifier(&bond["b"])?);
        ensure!(
            begin != end && atom_ids.contains_key(&begin) && atom_ids.contains_key(&end),
            "[dative-source] Missing or self source bond endpoints"
        );
        ensure!(
            pairs.insert((begin.min(end), begin.max(end))),
            "[dative-source] Duplicate source bond identity"
        );
        if bond["order"].as_u64() == Some(5) && bond["display"] == "dashed" {
            result.insert(
                (atoms.len() + index + 3).to_string(),
                (atom_ids[&begin].clone(), atom_ids[&end].clone()),
            );
        }
    }
    Ok(result)
}

/// The frozen worker exports its independently computed response document
/// (worker.py's result_doc), so its old response is an independent source for
/// reshiki-io's historical callers. Never borrow current response chemistry.
#[allow(dead_code)] // Used by reshiki-io's reference tests, not root targets.
pub fn compare_export(actual: &Value, expected: &mut Value) -> anyhow::Result<()> {
    let source = expected
        .get("document")
        .filter(|doc| doc.is_object())
        .cloned();
    compare_export_with_source(actual, expected, source.as_ref())
}

/// Atom/bond semantics come from the independent request or frozen original drawing,
/// never the current output or response. All response fields compare separately.
pub fn compare_export_with_source(
    actual: &Value,
    expected: &mut Value,
    source_document: Option<&Value>,
) -> anyhow::Result<()> {
    let (Some(a), Some(e)) = (
        actual.get("output").and_then(Value::as_str),
        expected.get("output").and_then(Value::as_str),
    ) else {
        return Ok(());
    };
    if a == e {
        return Ok(());
    }
    // The original worker predates the absolute-stereochemistry MOL flag
    // added with Haworth interchange. Check that one explicit distinction;
    // the caller still compares every other byte and all chemical properties.
    if a.lines().nth(3).is_some_and(|line| line.ends_with("V2000"))
        && e.lines().nth(3).is_some_and(|line| line.ends_with("V2000"))
    {
        let absolute = actual["document"]["atoms"]
            .as_array()
            .is_some_and(|atoms| atoms.iter().any(|a| a["stereo"].is_object()));
        let actual_flag = a.lines().nth(3).and_then(|line| line.get(12..15));
        anyhow::ensure!(
            actual_flag == Some(if absolute { "  1" } else { "  0" }),
            "Incorrect absolute MOL flag"
        );
        let mut lines: Vec<_> = e.split('\n').map(str::to_owned).collect();
        let counts = lines.get_mut(3).context("Missing MOL counts")?;
        anyhow::ensure!(
            counts.get(12..15) == Some("  0"),
            "Unexpected reference MOL flag"
        );
        *counts = format!(
            "{}{}{}",
            counts.get(..12).context("Invalid counts prefix")?,
            actual_flag.context("Missing actual flag")?,
            counts.get(15..).context("Invalid counts suffix")?
        );
        expected["output"] = Value::String(lines.join("\n"));
        return Ok(());
    }
    let format = if e.starts_with("<?xml") {
        "cdxml"
    } else if STANDARD
        .decode(e)
        .is_ok_and(|bytes| bytes.starts_with(b"VjCD0100"))
    {
        "cdx"
    } else {
        return Ok(());
    };
    // The root crate's reference targets and reshiki-io (crates/io) both
    // include this file; resolve the checkout that holds the oracle.
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest
        .ancestors()
        .find(|dir| dir.join("reference/reference_presentation.py").is_file())
        .unwrap_or(manifest);
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("reference/reference_presentation.py"))
        .env("PYTHONUTF8", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut input = child.stdin.take().context("Missing reference input")?;
    input.write_all(&serde_json::to_vec(
        &json!({"actual": a, "expected": e, "format": format, "source_atoms": source_document.and_then(|doc| doc.get("atoms")), "source_document": source_document}),
    )?)?;
    drop(input);
    let output = child.wait_with_output()?;
    anyhow::ensure!(
        output.status.success(),
        "Independent presentation comparison failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    *expected
        .get_mut("output")
        .context("Missing reference output")? = Value::String(a.into());
    Ok(())
}

/// Keep uncapped failures as an artifact and in the test diagnostic.
#[allow(dead_code)] // Used by root targets, not reshiki-io's bridge tests.
pub fn report_mismatches(name: &str, failures: &[String]) -> anyhow::Result<()> {
    if failures.is_empty() {
        return Ok(());
    }
    let mut categories = std::collections::BTreeMap::<String, usize>::new();
    for failure in failures {
        let mut categorized = false;
        for line in failure.lines() {
            for (index, _) in line.match_indices('[') {
                if let Some((category, suffix)) = line[index + 1..].split_once(']')
                    && suffix.starts_with(' ')
                    && category
                        .chars()
                        .all(|c| c.is_ascii_alphabetic() || c == '-')
                {
                    *categories.entry(category.to_owned()).or_default() += 1;
                    categorized = true;
                    break;
                }
            }
        }
        if !categorized {
            *categories.entry("uncategorized".into()).or_default() += 1;
        }
    }
    std::fs::create_dir_all("artifacts")?;
    let diagnostic = format!(
        "{} failed comparisons; categories={categories:?}\n{}",
        failures.len(),
        failures.join("\n")
    );
    std::fs::write(format!("artifacts/{name}-failures.txt"), &diagnostic)?;
    anyhow::bail!("{name}: {diagnostic}");
}
