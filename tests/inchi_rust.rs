#[path = "common/fixture.rs"]
mod fixture;

use anyhow::Context;
use reshiki::chemistry::inchi::{kernel, output};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::BufRead;

#[derive(Deserialize)]
struct Case {
    name: String,
    operation: String,
    #[serde(default)]
    inchi: String,
    #[serde(default)]
    sanitize: bool,
    #[serde(default)]
    remove: bool,
    expected: Option<Value>,
    error: Option<String>,
}

#[test]
fn rust_kernel_import_matches_independent_molecules() -> anyhow::Result<()> {
    let mut count = 0;
    let mut failures = Vec::new();
    for line in fixture::open("inchi-output.jsonl.gz")?.lines().skip(1) {
        let case: Case = serde_json::from_str(&line?)?;
        if case.operation != "import" {
            continue;
        }
        let actual = kernel::read(
            &case.inchi,
            output::Options {
                sanitize: case.sanitize,
                remove_hydrogens: case.remove,
            },
        );
        match (actual, case.expected, case.error) {
            (Ok(actual), expected, None)
                if serde_json::to_value(&actual.state)?
                    == expected.clone().unwrap_or(Value::Null) => {}
            (Err(_), None, Some(_)) => {}
            (actual, expected, expected_error) => {
                if failures.len() < 10 {
                    failures.push(format!(
                        "{}: actual={actual:?}\nexpected={expected:?}, error={expected_error:?}",
                        case.name
                    ));
                }
            }
        }
        count += 1;
    }
    assert_eq!(count, 1848);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}

#[test]
fn patched_generation_matches_the_official_1075_kernel() -> anyhow::Result<()> {
    let capture: Value = serde_json::from_reader(fixture::open("inchi-1075-generation.json.gz")?)?;
    assert_eq!(capture["checked"], 11_772);
    assert_eq!(capture["new_version"], kernel::VERSION);
    assert_eq!(capture["changes"], serde_json::json!({}));
    assert_eq!(
        capture["capture_sha256"],
        format!(
            "{:x}",
            Sha256::digest(include_bytes!("../reference/inchi_kernel_reference.py"))
        )
    );
    assert_eq!(
        capture["input_sha256"],
        format!(
            "{:x}",
            Sha256::digest(include_bytes!("fixtures/inchi-input-native.json.gz"))
        )
    );
    let rows = capture["regressions"]
        .as_object()
        .context("Missing native generation captures")?;
    assert_eq!(rows.len(), 692);
    for (name, row) in rows {
        let molecule = kernel::Molecule {
            state: serde_json::from_value(row["state"].clone())?,
            positions: serde_json::from_value(row["positions"].clone())?,
        };
        let actual = kernel::generate(&molecule).map_err(anyhow::Error::msg)?;
        assert!(
            actual.diagnostics.is_empty(),
            "{name}: {:?}",
            actual.diagnostics
        );
        let value = serde_json::to_value(actual)?;
        for (field, expected) in row["expected"]
            .as_object()
            .context("Missing native generation result")?
        {
            assert_eq!(value[field], *expected, "{name}: {field}");
        }
    }
    Ok(())
}
