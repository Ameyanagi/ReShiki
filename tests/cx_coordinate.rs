use anyhow::Context;
use reshiki::chemistry::{RDKIT_VERSION, cx};
use serde::Deserialize;
use serde_json::Value;
use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};

#[derive(Deserialize)]
struct Case {
    text: String,
    expected: Option<String>,
}

#[test]
fn coordinate_bits_match_native_cx_reader() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/cx_coordinate_reference.py"))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines = BufReader::new(
        child
            .stdout
            .take()
            .context("Missing coordinate oracle output")?,
    )
    .lines();
    let version: Value =
        serde_json::from_str(&lines.next().context("Missing reference version")??)?;
    assert_eq!(version["rdkit_version"], RDKIT_VERSION);
    let (mut count, mut mismatch) = (0, Vec::new());
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        let value = cx::coordinate(&case.text).map(|v| {
            if v.is_nan() {
                "nan".to_owned()
            } else {
                v.to_bits().to_string()
            }
        });
        count += 1;
        if value != case.expected {
            mismatch.push(format!("{}: {:?} != {:?}", case.text, value, case.expected));
        }
    }
    assert!(child.wait()?.success());
    eprintln!(
        "CX coordinate values: {count} cases, {} mismatches",
        mismatch.len()
    );
    assert!(
        mismatch.is_empty(),
        "{}",
        mismatch
            .iter()
            .take(30)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(count > 8000);
    Ok(())
}

#[test]
fn malformed_topologies_and_expanding_annotations_are_bounded() {
    assert!(
        cx::read(
            "||",
            &cx::Topology {
                atoms: usize::MAX,
                bonds: Vec::new()
            }
        )
        .is_err()
    );
    for (a, b, index) in [(0, 1, 0), (usize::MAX, 0, 0), (0, 0, usize::MAX)] {
        let graph = cx::Topology {
            atoms: 1,
            bonds: vec![cx::ParseBond {
                a,
                b,
                index: Some(index),
            }],
        };
        assert!(cx::read("|C:0.0|", &graph).is_err());
    }
    let graph = cx::Topology {
        atoms: 10_001,
        bonds: (1..=10_000)
            .map(|b| cx::ParseBond {
                a: 0,
                b,
                index: Some(b - 1),
            })
            .collect(),
    };
    let text = format!("|{}|", "Sg:n:0,1,".repeat(500));
    assert!(matches!(cx::read(&text, &graph), Err(cx::Error::Limit)));
    for text in ["|", "|(&#", "|atomProp:0.", "|$&#9999999999999999999999;$|"] {
        assert!(cx::read(text, &graph).is_err());
    }
    assert!(cx::read("|^1:0|", &graph).is_ok());
}
