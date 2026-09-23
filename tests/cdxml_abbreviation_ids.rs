use anyhow::Context;
use reshiki::chemistry::{
    RDKIT_VERSION, cdxml,
    graph::{Atom, Graph},
    ranking::Metadata,
    stereo::Point3,
};
use serde::Deserialize;
use serde_json::Value;
use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};

#[derive(Deserialize)]
struct Record {
    label: String,
    reverse_label: String,
    anchor: Option<String>,
    members: Vec<Option<String>>,
}
impl From<Record> for cdxml::Abbreviation {
    fn from(r: Record) -> Self {
        Self {
            label: r.label,
            reverse_label: r.reverse_label,
            anchor: r.anchor,
            members: r.members,
        }
    }
}
#[derive(Deserialize)]
struct Case {
    name: String,
    text: String,
    records: Vec<Record>,
    atoms: Vec<u8>,
    positions: Vec<Point3>,
    scale: f64,
    sizes: Vec<usize>,
    expected: Option<Value>,
    failure: Option<String>,
    failure_type: Option<String>,
    restriction: Option<String>,
}

fn fragments(atoms: &[u8], sizes: &[usize]) -> anyhow::Result<Vec<cdxml::Fragment>> {
    let mut start = 0usize;
    let mut result = Vec::new();
    for (i, &size) in sizes.iter().enumerate() {
        let end = start.checked_add(size).context("Fragment size overflow")?;
        let graph = Graph {
            atoms: atoms
                .get(start..end)
                .context("Invalid reference fragment size")?
                .iter()
                .map(|&atomic_number| Atom {
                    atomic_number,
                    ..Atom::default()
                })
                .collect(),
            bonds: Vec::new(),
        };
        let metadata = Metadata::unspecified(&graph);
        result.push(cdxml::Fragment {
            id: i32::try_from(i)?,
            atom_ids: (0..size)
                .map(u32::try_from)
                .collect::<Result<Vec<_>, _>>()?,
            bond_ids: Vec::new(),
            fuse_labels: vec![None; size],
            graph,
            metadata,
            directions: Vec::new(),
            positions: vec![Point3::default(); size],
            is_3d: false,
            bond_cfg: Vec::new(),
            non_explicit_3d_chirality: vec![None; size],
            bond_cip: Vec::new(),
            atom_cip_ranks: vec![None; size],
        });
        start = end;
    }
    anyhow::ensure!(start == atoms.len(), "Unassigned reference atoms");
    Ok(result)
}

#[test]
fn associated_records_match_original_read_exactly() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/cdxml_abbreviation_ids_reference.py"))
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines =
        BufReader::new(child.stdout.take().context("Missing reference output")?).lines();
    let version: Value =
        serde_json::from_str(&lines.next().context("Missing reference version")??)?;
    assert_eq!(version["rdkit_version"], RDKIT_VERSION);
    let (mut accepted, mut rejected, mut restrictions) = (0, 0, 0);
    let mut failures = Vec::new();
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        let fragments = fragments(&case.atoms, &case.sizes)?;
        let records = case.records.into_iter().map(Into::into).collect::<Vec<_>>();
        let before = serde_json::to_value((&records, &fragments, &case.positions, &case.text))?;
        let result = cdxml::read_abbreviations(
            &records,
            &case.text,
            &fragments,
            &case.positions,
            case.scale,
        );
        assert_eq!(
            before,
            serde_json::to_value((&records, &fragments, &case.positions, &case.text))?
        );
        let failure = if case.restriction.is_some() {
            restrictions += 1;
            if result.is_ok() || case.expected.is_none() {
                Some("Incorrect explicit XML restriction".to_owned())
            } else {
                None
            }
        } else {
            match (&result, &case.expected) {
                (Ok(actual), Some(expected)) => {
                    accepted += 1;
                    let mut actual = serde_json::to_value(actual)?;
                    // The original reader discarded label justification. Its
                    // atom-ID contract stays exact; alignment has separate
                    // ChemDraw fixture and native/CDXML/CDX round-trip checks.
                    if let Some(groups) = actual.as_array_mut() {
                        for group in groups {
                            if let Some(group) = group.as_object_mut() {
                                group.remove("alignment");
                            }
                        }
                    }
                    (actual != *expected).then(|| format!("{actual} != {expected}"))
                }
                (Err(error), None) => {
                    rejected += 1;
                    if case.failure.as_deref()
                        == Some("Could not safely associate abbreviation atoms")
                        && error.to_string()
                            != "Invalid molecular CDXML: Could not safely associate abbreviation atoms"
                    {
                        Some(format!("Changed association failure: {error}"))
                    } else {
                        None
                    }
                }
                (Err(error), Some(_)) => Some(format!("Rejected original records: {error}")),
                (Ok(_), None) => Some(format!(
                    "Accepted original {:?}: {:?}",
                    case.failure_type, case.failure
                )),
            }
        };
        if let Some(failure) = failure {
            failures.push(format!("{}: {failure}", case.name));
        }
    }
    assert!(child.wait()?.success());
    eprintln!(
        "CDXML abbreviation IDs: {accepted} exact record successes, {rejected} matching failures, {restrictions} separate XML restrictions"
    );
    anyhow::ensure!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures
            .iter()
            .take(20)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(accepted > 800 && rejected > 700);
    Ok(())
}

#[test]
fn twenty_thousand_distinct_ids_use_spatial_lookup() -> anyhow::Result<()> {
    use std::fmt::Write;
    let atoms = vec![6; 20_000];
    let fragments = fragments(&atoms, &[10_000, 10_000])?;
    let mut source = String::from("<CDXML><page><fragment id='1'>");
    let mut members = Vec::new();
    let mut positions = Vec::new();
    for i in 0..atoms.len() {
        let x = f64::from(u32::try_from(i)?) * 2.;
        write!(source, "<n id='source-{i}' p='{x} 0'/>")?;
        positions.push(Point3 {
            x: x / 28.,
            y: 0.,
            z: 0.,
        });
        members.push(Some(format!("source-{i}")));
    }
    source.push_str("</fragment></page></CDXML>");
    let records = vec![cdxml::Abbreviation {
        label: "X".into(),
        reverse_label: "".into(),
        anchor: Some("source-0".into()),
        members,
    }];
    let actual = cdxml::read_abbreviations(&records, &source, &fragments, &positions, 1.)?;
    let group = actual.first().context("Missing associated group")?;
    assert_eq!(group.anchor, 1);
    assert_eq!(group.members, (1..=20_000).collect::<Vec<_>>());
    Ok(())
}

#[test]
fn invalid_dimensions_coordinates_and_storage_fail_atomically() -> anyhow::Result<()> {
    let source = "<CDXML><page><n id='a' p='0 0'/></page></CDXML>";
    let fragments = fragments(&[6], &[1])?;
    let records = vec![cdxml::Abbreviation {
        label: "Me".into(),
        reverse_label: "".into(),
        anchor: Some("a".into()),
        members: vec![Some("a".into())],
    }];
    assert!(cdxml::read_abbreviations(&records, source, &fragments, &[], 1.).is_err());
    for p in [
        Point3 {
            x: f64::NAN,
            ..Point3::default()
        },
        Point3 {
            z: f64::INFINITY,
            ..Point3::default()
        },
    ] {
        assert!(cdxml::read_abbreviations(&records, source, &fragments, &[p], 1.).is_err());
    }
    assert!(
        cdxml::read_abbreviations(
            &records,
            source,
            &fragments,
            &[Point3::default()],
            f64::INFINITY
        )
        .is_err()
    );
    let mut excessive = records.clone();
    excessive.first_mut().context("Missing test record")?.label = "x".repeat(16 * 1024 * 1024 + 1);
    assert!(matches!(
        cdxml::read_abbreviations(&excessive, source, &fragments, &[Point3::default()], 1.),
        Err(cdxml::Error::Limit)
    ));
    assert_eq!(
        records.first().context("Missing original record")?.label,
        "Me"
    );
    Ok(())
}
