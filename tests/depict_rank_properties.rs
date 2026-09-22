//! Lazy native rank conversion, against the original public depiction API.
use anyhow::Context;
use reshiki::chemistry::{depict, smiles};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};

#[derive(Deserialize)]
struct Case {
    text: String,
    templates: bool,
    properties: Vec<BTreeMap<String, String>>,
    expected: Option<Vec<[f64; 3]>>,
    error: Option<String>,
}
#[test]
fn literal_rank_properties_follow_native_lazy_reads() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    if !python.is_file() {
        eprintln!("Skipping optional direct RDKit rank oracle");
        return Ok(());
    }
    let mut process = Command::new(python)
        .arg(root.join("tests/depict_rank_properties_reference.py"))
        .stdout(Stdio::piped())
        .spawn()?;
    let lines = BufReader::new(process.stdout.take().context("Missing oracle output")?).lines();
    let (mut accepted, mut rejected, mut scalars) = (0, 0, 0);
    for line in lines {
        let value: serde_json::Value = serde_json::from_str(&line?)?;
        if let Some(version) = value.get("rdkit_version") {
            assert_eq!(version, reshiki::chemistry::RDKIT_VERSION);
            continue;
        }
        let case: Case = serde_json::from_value(value)?;
        let source = smiles::read(&case.text)?.prepared.state;
        let before = serde_json::to_value(&source)?;
        let properties = case
            .properties
            .iter()
            .map(|properties| depict::ranks::AtomProperties {
                cip: properties
                    .get("_CIPRank")
                    .map_or(depict::ranks::Property::Absent, |s| {
                        depict::ranks::Property::from_bytes(s.as_bytes())
                    }),
                chiral: properties
                    .get("_chiralAtomRank")
                    .map_or(depict::ranks::Property::Absent, |s| {
                        depict::ranks::Property::from_bytes(s.as_bytes())
                    }),
            })
            .collect::<Vec<_>>();
        let actual = depict::compute_with_rank_properties(
            &source,
            &properties,
            None,
            depict::Options {
                use_ring_templates: case.templates,
                ..depict::Options::default()
            },
        );
        assert_eq!(
            serde_json::to_value(&source)?,
            before,
            "Source changed: {}",
            case.text
        );
        match (actual, case.expected) {
            (Ok(actual), Some(expected)) => {
                assert_eq!(actual.positions.len(), expected.len());
                for (point, expected) in actual.positions.iter().zip(expected) {
                    for (a, b) in [point.x, point.y, point.z].into_iter().zip(expected) {
                        assert_eq!(
                            a.to_bits(),
                            b.to_bits(),
                            "{} templates={}: {a:?} != {b:?}",
                            case.text,
                            case.templates
                        );
                        scalars += 1;
                    }
                }
                accepted += 1;
            }
            (Err(actual), None) => {
                // std::bad_any_cast::what differs between libc++, libstdc++, and MSVC.
                let native = case.error.context("Missing native error")?;
                anyhow::ensure!(
                    matches!(
                        native.as_str(),
                        "bad any cast" | "bad any_cast" | "Bad any_cast"
                    ),
                    "Unexpected native failure: {native}"
                );
                anyhow::ensure!(
                    matches!(
                        actual,
                        depict::Error::Initial(depict::expansion::Error::Attachment(
                            depict::attachment::Error::BadRank { .. }
                        )) | depict::Error::Initial(depict::expansion::Error::Seeds(
                            depict::seeds::Error::BadRank { .. }
                        )) | depict::Error::Initial(depict::expansion::Error::Templates(
                            depict::templates::Error::Seeds(depict::seeds::Error::BadRank { .. })
                        ))
                    ),
                    "Unexpected typed failure: {actual:?}"
                );
                rejected += 1;
            }
            (actual, expected) => anyhow::bail!(
                "{} templates={}: {actual:?} != {expected:?}; {:?}",
                case.text,
                case.templates,
                case.error
            ),
        }
    }
    anyhow::ensure!(process.wait()?.success(), "Native rank oracle failed");
    println!(
        "Literal ranks: {accepted} native successes, {rejected} matching lazy errors, {scalars} exact f64 scalars"
    );
    anyhow::ensure!(
        accepted > 500 && rejected > 100,
        "Insufficient lazy-rank cases"
    );
    Ok(())
}

#[test]
fn rank_dimensions_and_shared_work_fail_without_changing_source() -> anyhow::Result<()> {
    let source = smiles::prepare("CCCC")?.state;
    let before = serde_json::to_value(&source)?;
    let properties = vec![depict::ranks::AtomProperties::default(); 4];
    assert!(
        depict::compute_with_rank_properties(
            &source,
            &properties[..3],
            None,
            depict::Options::default()
        )
        .is_err()
    );
    for work_limit in [0, 1, 12, 40] {
        assert!(
            depict::compute_with_rank_properties(
                &source,
                &properties,
                None,
                depict::Options {
                    work_limit,
                    ..depict::Options::default()
                }
            )
            .is_err()
        );
        assert_eq!(serde_json::to_value(&source)?, before);
    }
    let ordinary = depict::compute(&source, &[None; 4], None, depict::Options::default())?;
    let retained = depict::compute_with_rank_properties(
        &source,
        &properties,
        None,
        depict::Options::default(),
    )?;
    assert_eq!(
        serde_json::to_value(ordinary)?,
        serde_json::to_value(retained)?
    );
    assert_eq!(serde_json::to_value(source)?, before);
    Ok(())
}
