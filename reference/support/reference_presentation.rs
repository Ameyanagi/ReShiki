//! Explicit presentation changes that the original Python worker predates.
use anyhow::Context;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

pub fn compare_export(actual: &Value, expected: &mut Value) -> anyhow::Result<()> {
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
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/reference_presentation.py"))
        .env("PYTHONUTF8", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut input = child.stdin.take().context("Missing reference input")?;
    input.write_all(&serde_json::to_vec(
        &json!({"actual": a, "expected": e, "format": format}),
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
