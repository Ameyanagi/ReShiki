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
