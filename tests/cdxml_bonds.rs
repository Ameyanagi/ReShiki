//! Appearance observations call the original Python helper with independent
//! native molecules. A second lane supplies the Rust parser's raw fragments.
use anyhow::Context;
use reshiki::{
    chemistry::{
        RDKIT_VERSION, cdxml,
        graph::{Atom, Graph},
        ranking::Metadata,
        stereo::Point3,
    },
    document::Bond,
};
use serde::Deserialize;
use serde_json::Value;
use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};

#[derive(Deserialize)]
struct Part {
    id: i32,
    numbers: Vec<u8>,
    positions: Vec<Point3>,
}
impl Part {
    fn fragment(self) -> anyhow::Result<cdxml::Fragment> {
        let graph = Graph {
            atoms: self
                .numbers
                .into_iter()
                .map(|atomic_number| Atom {
                    atomic_number,
                    ..Default::default()
                })
                .collect(),
            bonds: Vec::new(),
        };
        Ok(cdxml::Fragment {
            id: self.id,
            // Deliberately unrelated, high source IDs: the appearance reader
            // must use native positions rather than the parser's retained IDs.
            atom_ids: (0..graph.atoms.len())
                .map(|i| Ok(u32::MAX - u32::try_from(i)?))
                .collect::<anyhow::Result<_>>()?,
            bond_ids: Vec::new(),
            fuse_labels: vec![None; graph.atoms.len()],
            metadata: Metadata::unspecified(&graph),
            directions: Vec::new(),
            positions: self.positions,
            is_3d: false,
            bond_cfg: Vec::new(),
            non_explicit_3d_chirality: vec![None; graph.atoms.len()],
            bond_cip: Vec::new(),
            atom_cip_ranks: vec![None; graph.atoms.len()],
            graph,
        })
    }
}
#[derive(Deserialize)]
struct Case {
    name: String,
    text: String,
    parts: Vec<Part>,
    scale: f64,
    colors: Vec<[u8; 3]>,
    expected: Option<Value>,
    error: Option<String>,
    error_type: Option<String>,
    layer_bound: bool,
    parsed_xml: Option<String>,
}

fn matches(
    actual: Result<Vec<Bond>, cdxml::bonds::Error>,
    expected: Option<&[Bond]>,
    error: Option<&str>,
    error_type: Option<&str>,
) -> anyhow::Result<()> {
    match (actual, expected) {
        (Ok(actual), Some(expected)) => {
            anyhow::ensure!(actual == expected, "{actual:?} != {expected:?}")
        }
        (Err(actual), None) => {
            anyhow::ensure!(
                Some(actual.to_string().as_str()) == error,
                "{actual} != {error:?}"
            );
            let key_error = matches!(
                actual,
                cdxml::bonds::Error::Endpoint(_) | cdxml::bonds::Error::Display(_)
            );
            anyhow::ensure!(
                key_error == (error_type == Some("KeyError")),
                "Exception classification differs"
            );
        }
        (actual, expected) => {
            anyhow::bail!("Outcome differs: {actual:?} != {expected:?} ({error:?})")
        }
    }
    Ok(())
}

#[test]
fn cdxml_bond_appearance_matches_original_helper() -> anyhow::Result<()> {
    // Error quoting pins the native Unicode 15 categories against this toolchain.
    assert_eq!(char::UNICODE_VERSION, (17, 0, 0));
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut child = Command::new(root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    }))
    .arg(root.join("tests/cdxml_bonds_reference.py"))
    .env("PYTHONUTF8", "1")
    .stdout(Stdio::piped())
    .stderr(Stdio::inherit())
    .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().context("Missing oracle output")?).lines();
    let header: Value = serde_json::from_str(&lines.next().context("Missing oracle header")??)?;
    assert_eq!(header["rdkit_version"], RDKIT_VERSION);
    let (mut accepted, mut rejected, mut layer_bounds, mut parsed_cases) = (0, 0, 0, 0);
    let mut failures = Vec::new();
    for line in lines {
        let case: Case = serde_json::from_value(serde_json::from_str(&line?)?)?;
        let parts = case
            .parts
            .into_iter()
            .map(Part::fragment)
            .collect::<anyhow::Result<Vec<_>>>()?;
        let before = serde_json::to_value(&parts)?;
        let actual = cdxml::bonds::read(&case.text, &parts, case.scale, &case.colors);
        assert_eq!(
            before,
            serde_json::to_value(&parts)?,
            "Mutated input: {}",
            case.name
        );
        if case.layer_bound {
            layer_bounds += 1;
            if !matches!(actual, Err(cdxml::bonds::Error::Layer)) {
                failures.push(format!(
                    "{}: Document layer boundary differs: {actual:?}",
                    case.name
                ));
            }
            continue;
        }
        let expected: Option<Vec<Bond>> = case.expected.map(serde_json::from_value).transpose()?;
        if expected.is_some() {
            accepted += 1;
        } else {
            rejected += 1;
        }
        if let Err(error) = matches(
            actual,
            expected.as_deref(),
            case.error.as_deref(),
            case.error_type.as_deref(),
        ) && failures.len() < 30
        {
            failures.push(format!("{}: {error}", case.name));
        }
        if let Some(xml) = &case.parsed_xml {
            match cdxml::read(xml) {
                Ok(parsed) => {
                    parsed_cases += 1;
                    if let Err(error) = matches(
                        cdxml::bonds::read(&case.text, &parsed.fragments, case.scale, &case.colors),
                        expected.as_deref(),
                        case.error.as_deref(),
                        case.error_type.as_deref(),
                    ) && failures.len() < 30
                    {
                        failures.push(format!("{} / Rust parser: {error}", case.name));
                    }
                }
                // The bounded parser rejects malformed partial reads before
                // presentation; the native-fragment lane above still checks
                // the original helper's exact partial-import error.
                Err(error) if expected.is_some() => {
                    failures.push(format!("{} / Rust parser: {error}", case.name))
                }
                Err(_) => (),
            }
        }
    }
    assert!(
        child.wait()?.success(),
        "Oracle failed: {}",
        failures.join("\n")
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(
        accepted > 5000 && rejected > 80 && layer_bounds >= 3 && parsed_cases > 50,
        "Incomplete cases: {accepted}/{rejected}/{layer_bounds}/{parsed_cases}"
    );
    eprintln!(
        "Verified {accepted} native successes, {rejected} exact errors, {layer_bounds} document layer bounds and {parsed_cases} Rust parser appearances"
    );
    Ok(())
}

fn points(count: usize, dense: bool) -> anyhow::Result<(String, cdxml::Fragment)> {
    let mut text = String::from("<CDXML><fragment id='30'>");
    let mut positions = Vec::with_capacity(count);
    for index in 0..count {
        let (x, y) = if dense {
            ((index % 40) as f64 * 0.03, (index / 40) as f64 * 0.03)
        } else {
            (0.0, index as f64 * 3.0)
        };
        text.push_str(&format!("<n id='{index}' p='{x} {y}'/>"));
        positions.push(Point3 {
            x: x / 28.0,
            y: -y / 28.0,
            z: 0.0,
        });
    }
    text.push_str("</fragment></CDXML>");
    let part = Part {
        id: 30,
        numbers: vec![6; count],
        positions,
    }
    .fragment()?;
    Ok((text, part))
}

#[test]
fn association_is_bounded_atomic_and_handles_long_vertical_fragments() -> anyhow::Result<()> {
    let entity = "<!DOCTYPE CDXML [<!-- <!ENTITY inert 'x'> --><!ENTITY actual 'x'>]><CDXML/>";
    assert!(matches!(
        cdxml::bonds::read(entity, &[], 1.0, &[]),
        Err(cdxml::bonds::Error::Invalid(_))
    ));
    let (text, part) = points(25_000, false)?;
    let before = serde_json::to_value(&part)?;
    assert!(cdxml::bonds::read(&text, std::slice::from_ref(&part), 1.0, &[]).is_ok());
    assert_eq!(before, serde_json::to_value(&part)?);
    let (text, part) = points(4000, true)?;
    assert!(matches!(
        cdxml::bonds::read(&text, &[part], 1.0, &[]),
        Err(cdxml::bonds::Error::Limit)
    ));
    let oversized = " ".repeat(16 * 1024 * 1024 + 1);
    assert!(matches!(
        cdxml::bonds::read(&oversized, &[], 1.0, &[]),
        Err(cdxml::bonds::Error::Limit)
    ));
    let deep = format!("{}{}", "<group>".repeat(70), "</group>".repeat(70));
    assert!(matches!(
        cdxml::bonds::read(&deep, &[], 1.0, &[]),
        Err(cdxml::bonds::Error::Limit)
    ));
    let (text, mut part) = points(2, false)?;
    part.positions.clear();
    assert!(matches!(
        cdxml::bonds::read(&text, &[part], 1.0, &[]),
        Err(cdxml::bonds::Error::Invalid(_))
    ));
    Ok(())
}
