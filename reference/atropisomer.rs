use anyhow::Context;
use reshiki::chemistry::{
    RDKIT_VERSION,
    graph::{Atom, Bond, Graph},
    kekulize::Direction,
    ranking::Metadata,
    stereo::{self, Point3, wedging::Conformer},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};

#[derive(Deserialize, Serialize)]
struct Case {
    name: String,
    graph: Graph,
    metadata: Metadata,
    directions: Vec<Direction>,
    conformer: Option<Conformer>,
    expected: Value,
}

#[test]
fn axial_stereo_matches_native_file_perception() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/atropisomer_reference.py"))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines = BufReader::new(
        child
            .stdout
            .take()
            .context("Missing axial reference output")?,
    )
    .lines();
    let version: Value =
        serde_json::from_str(&lines.next().context("Missing reference version")??)?;
    assert_eq!(version["rdkit_version"], RDKIT_VERSION);
    let (mut count, mut changed, mut mismatches) = (0, 0, 0);
    let mut labels = std::collections::BTreeSet::new();
    let mut failures = Vec::new();
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        count += 1;
        let before = serde_json::to_value(&case)?;
        let result = stereo::detect_atropisomers(
            &case.graph,
            &case.metadata,
            &case.directions,
            case.conformer.as_ref(),
        );
        let mismatch = match &result {
            Ok(actual) => {
                changed += usize::from(
                    serde_json::to_value(actual)? != serde_json::to_value(&case.metadata)?,
                );
                for b in &actual.bonds {
                    if b.stereo >= 6 {
                        labels.insert(b.stereo);
                    }
                }
                serde_json::to_value(actual)? != case.expected
            }
            Err(_) => true,
        };
        assert_eq!(before, serde_json::to_value(&case)?, "Changed caller data");
        if mismatch {
            mismatches += 1;
            if failures.len() < 15 {
                failures.push(format!("{}: {result:?} != {}", case.name, case.expected));
            }
            if mismatches == 1 {
                std::fs::create_dir_all(root.join("artifacts"))?;
                std::fs::write(
                    root.join("artifacts/atropisomer-mismatch.json"),
                    serde_json::to_vec_pretty(&case)?,
                )?;
                if let Ok(actual) = result {
                    std::fs::write(
                        root.join("artifacts/atropisomer-actual.json"),
                        serde_json::to_vec_pretty(&actual)?,
                    )?;
                }
            }
        }
    }
    assert!(child.wait()?.success(), "Native axial reference failed");
    eprintln!(
        "Atropisomer detection: {count} cases, {changed} changed, labels {labels:?}, {mismatches} mismatches"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(
        count > 10000 && changed > 100 && labels == [6, 7].into_iter().collect(),
        "Incomplete axial stereo coverage"
    );
    Ok(())
}

#[test]
fn invalid_axial_inputs_and_high_degree_graphs_are_bounded() -> anyhow::Result<()> {
    let mut graph = Graph {
        atoms: vec![
            Atom {
                atomic_number: 6,
                ..Atom::default()
            };
            3
        ],
        bonds: vec![
            Bond {
                a: 0,
                b: 1,
                order: 1,
                aromatic: false,
            },
            Bond {
                a: 0,
                b: 2,
                order: 1,
                aromatic: false,
            },
        ],
    };
    let metadata = Metadata::unspecified(&graph);
    let directions = vec![Direction::Wedge; 2];
    let before = serde_json::to_value((&graph, &metadata, &directions))?;
    assert!(stereo::detect_atropisomers(&graph, &metadata, &[], None).is_err());
    assert!(stereo::detect_atropisomers(&graph, &Metadata::default(), &directions, None).is_err());
    for x in [f64::NAN, f64::INFINITY, 1e101] {
        let conf = Conformer {
            is_3d: true,
            positions: vec![Point3 { x, y: 0., z: 0. }; 3],
        };
        assert!(stereo::detect_atropisomers(&graph, &metadata, &directions, Some(&conf)).is_err());
    }
    let conf = Conformer {
        is_3d: false,
        positions: vec![],
    };
    assert!(stereo::detect_atropisomers(&graph, &metadata, &directions, Some(&conf)).is_err());
    assert_eq!(
        before,
        serde_json::to_value((&graph, &metadata, &directions))?
    );
    graph.bonds.first_mut().context("Missing bond")?.b = 99;
    assert!(stereo::detect_atropisomers(&graph, &metadata, &directions, None).is_err());
    // Candidate collection must not scan all adjacent bonds for each wedge.
    let graph = Graph {
        atoms: vec![Atom::default(); 30_001],
        bonds: (1..=30_000)
            .map(|b| Bond {
                a: 0,
                b,
                order: 1,
                aromatic: false,
            })
            .collect(),
    };
    let metadata = Metadata::unspecified(&graph);
    let result =
        stereo::detect_atropisomers(&graph, &metadata, &vec![Direction::Wedge; 30_000], None)?;
    assert_eq!(
        serde_json::to_value(result)?,
        serde_json::to_value(metadata)?
    );
    Ok(())
}
