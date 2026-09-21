use anyhow::Context;
use reshiki::chemistry::{
    RDKIT_VERSION,
    graph::Graph,
    kekulize::Direction,
    ranking::Metadata,
    stereo::{self, SpatialAnnotations, SpatialOptions, wedging::Conformer},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};

#[derive(Deserialize, Serialize)]
struct Input {
    graph: Graph,
    metadata: Metadata,
    directions: Vec<Direction>,
    annotations: SpatialAnnotations,
}
#[derive(Deserialize, Serialize)]
struct Case {
    name: String,
    before: Input,
    conformer: Option<Conformer>,
    options: SpatialOptions,
    expected: Option<Value>,
    failure: Option<String>,
}

#[test]
fn spatial_stereo_matches_native_atom_perception() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/spatial_stereo_reference.py"))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines =
        BufReader::new(child.stdout.take().context("Missing 3D reference output")?).lines();
    let version: Value =
        serde_json::from_str(&lines.next().context("Missing reference version")??)?;
    assert_eq!(version["rdkit_version"], RDKIT_VERSION);
    let (mut count, mut rejected, mut changed, mut mismatches) = (0, 0, 0, 0);
    let mut permutations = std::collections::BTreeSet::new();
    let mut failures = Vec::new();
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        let before = serde_json::to_value(&case)?;
        count += 1;
        let result = stereo::from_3d(
            &case.before.graph,
            &case.before.metadata,
            &case.before.directions,
            case.conformer.as_ref(),
            &case.before.annotations,
            case.options,
        );
        let mismatch = match (&result, &case.expected) {
            (Ok(actual), Some(expected)) => {
                changed += usize::from(
                    serde_json::to_value(&actual.metadata)?
                        != serde_json::to_value(&case.before.metadata)?,
                );
                for a in &actual.metadata.atoms {
                    if let Some(p) = a.chiral_permutation
                        && a.chiral_tag >= 6
                        && case.options.replace_existing
                        && case.options.allow_nontetrahedral
                    {
                        permutations.insert((a.chiral_tag, p));
                    }
                }
                serde_json::to_value(actual)? != *expected
            }
            (Err(_), None) => {
                rejected += 1;
                false
            }
            _ => true,
        };
        assert_eq!(serde_json::to_value(&case)?, before, "Changed caller data");
        if mismatch {
            mismatches += 1;
            if failures.len() < 15 {
                failures.push(format!(
                    "{}: {result:?} != {:?} ({:?})",
                    case.name, case.expected, case.failure
                ));
            }
            if mismatches == 1 {
                std::fs::create_dir_all(root.join("artifacts"))?;
                std::fs::write(
                    root.join("artifacts/spatial-stereo-mismatch.json"),
                    serde_json::to_vec_pretty(&case)?,
                )?;
                if let Ok(actual) = result {
                    std::fs::write(
                        root.join("artifacts/spatial-stereo-actual.json"),
                        serde_json::to_vec_pretty(&actual)?,
                    )?;
                }
            }
        }
    }
    assert!(child.wait()?.success(), "3D reference failed");
    eprintln!(
        "3D stereo: {count} cases, {changed} changed, {rejected} rejected, {} coordination permutations, {mismatches} mismatches",
        permutations.len()
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(
        count > 15000 && changed > 1000 && rejected > 10 && permutations.len() == 53,
        "Incomplete 3D stereo coverage"
    );
    Ok(())
}

#[test]
fn invalid_spatial_inputs_return_errors_without_changing_callers() -> anyhow::Result<()> {
    use reshiki::chemistry::{
        graph::{Atom, Bond},
        stereo::Point3,
    };
    let graph = Graph {
        atoms: vec![
            Atom {
                atomic_number: 6,
                ..Atom::default()
            };
            4
        ],
        bonds: (1..4)
            .map(|b| Bond {
                a: 0,
                b,
                order: 1,
                aromatic: false,
            })
            .collect(),
    };
    let metadata = Metadata::unspecified(&graph);
    let directions = vec![Direction::None; 3];
    let annotations = SpatialAnnotations {
        non_explicit: vec![None; 4],
        done: Some(true),
    };
    let conformer = Conformer {
        is_3d: true,
        positions: vec![Point3::default(); 4],
    };
    let original = serde_json::json!({"graph":graph,"metadata":metadata,"conformer":conformer,"annotations":annotations});
    for case in 0..7 {
        let mut g = graph.clone();
        let mut m = metadata.clone();
        let mut d = directions.clone();
        let mut c = conformer.clone();
        let mut a = annotations.clone();
        match case {
            0 => {
                c.positions.pop();
            }
            1 => c.positions.first_mut().context("Missing position")?.x = f64::NAN,
            2 => c.positions.first_mut().context("Missing position")?.y = 1e101,
            3 => {
                a.non_explicit.pop();
            }
            4 => {
                d.pop();
            }
            5 => g.bonds.first_mut().context("Missing bond")?.b = 90,
            _ => {
                m.atoms.pop();
            }
        }
        assert!(stereo::from_3d(&g, &m, &d, Some(&c), &a, SpatialOptions::default()).is_err());
    }
    // A large graph with one high-degree center must remain linear and bounded.
    let mut large = Graph {
        atoms: vec![
            Atom {
                atomic_number: 0,
                ..Atom::default()
            };
            30_000
        ],
        bonds: Vec::new(),
    };
    large.bonds.extend((1..30_000).map(|b| Bond {
        a: 0,
        b,
        order: 1,
        aromatic: false,
    }));
    let result = stereo::from_3d(
        &large,
        &Metadata::unspecified(&large),
        &vec![Direction::None; large.bonds.len()],
        Some(&Conformer {
            is_3d: true,
            positions: vec![Point3::default(); large.atoms.len()],
        }),
        &SpatialAnnotations {
            non_explicit: vec![None; large.atoms.len()],
            done: None,
        },
        SpatialOptions::default(),
    )?;
    assert!(result.metadata.atoms.iter().all(|a| a.chiral_tag == 0));
    assert_eq!(
        serde_json::json!({"graph":graph,"metadata":metadata,"conformer":conformer,"annotations":annotations}),
        original
    );
    Ok(())
}
