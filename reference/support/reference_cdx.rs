//! Canonical CDX fixtures from the independently corrected Python codec.
use anyhow::Context;
use serde::Deserialize;
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

#[derive(Deserialize)]
pub struct Roundtrip {
    pub binary: String,
    pub decoded: String,
}

fn run(mode: &str, input: &str) -> anyhow::Result<Vec<u8>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/cdx_reference.py"))
        .arg(mode)
        .env("PYTHONUTF8", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .context("Missing codec input")?
        .write_all(input.as_bytes())?;
    let output = child.wait_with_output()?;
    anyhow::ensure!(
        output.status.success(),
        "Independent CDX codec failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output.stdout)
}

pub fn roundtrip(xml: &str) -> anyhow::Result<Roundtrip> {
    Ok(serde_json::from_slice(&run("--roundtrip", xml)?)?)
}

pub fn decode(binary: &str) -> anyhow::Result<String> {
    Ok(serde_json::from_slice(&run("--decode", binary)?)?)
}
