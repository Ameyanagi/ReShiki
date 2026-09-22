use anyhow::Context;
use reshiki::chemistry::{
    RDKIT_VERSION,
    graph::{Graph, Valence},
    kekulize::Direction,
    ranking::{self, Metadata},
    smiles::{
        symbols::{AtomProperties, Options, Writer},
        traversal::{self, Output},
    },
};
use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};

#[derive(Deserialize, Serialize)]
struct Properties {
    custom_symbol: Option<String>,
    supplement: Option<String>,
}
#[derive(Deserialize, Serialize)]
struct Expected {
    text: String,
    atom_order: Vec<usize>,
    bond_order: Vec<usize>,
}
#[derive(Deserialize, Serialize)]
struct Case {
    name: String,
    graph: Graph,
    metadata: Metadata,
    rings: Vec<Vec<usize>>,
    ring_bonds: Vec<bool>,
    ranks: Vec<u32>,
    start: usize,
    cache: Vec<Valence>,
    properties: Vec<Properties>,
    canonical: bool,
    options: Options,
    expected: Option<Expected>,
    failure: Option<String>,
}
impl Case {
    fn actual(&self) -> anyhow::Result<Output> {
        if self.canonical {
            let ranks = ranking::rank(
                &self.graph,
                &self.rings,
                &self.metadata,
                ranking::Options {
                    include_chirality: false,
                    include_isotopes: false,
                    include_stereo_groups: false,
                    ..ranking::Options::default()
                },
            )
            .map_err(anyhow::Error::msg)?;
            anyhow::ensure!(ranks == self.ranks, "Writer rank preparation differs");
        }
        let walk = traversal::build(&self.graph, &self.ring_bonds, &self.ranks, self.start)?;
        let writer = Writer::new(&self.graph, &self.metadata, Some(&self.cache))?;
        let properties = self
            .properties
            .iter()
            .map(|p| AtomProperties {
                custom_symbol: p.custom_symbol.as_deref(),
                supplement: p.supplement.as_deref(),
                broken_chirality: false,
            })
            .collect::<Vec<_>>();
        Ok(walk.render(
            &writer,
            &vec![Direction::None; self.graph.bonds.len()],
            &properties,
            self.options,
        )?)
    }
}

#[test]
fn ranked_walk_matches_complete_native_strings_and_output_order() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/smiles_traversal_reference.py"))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines =
        BufReader::new(child.stdout.take().context("Missing traversal oracle")?).lines();
    let version: serde_json::Value =
        serde_json::from_str(&lines.next().context("Missing oracle version")??)?;
    assert_eq!(version["rdkit_version"], RDKIT_VERSION);
    let (mut accepted, mut rejected, mut mismatches) = (0, 0, 0);
    let mut failures = Vec::new();
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        let actual = case.actual();
        let failure = match (&actual, &case.expected) {
            (Ok(a), Some(e))
                if a.text == e.text
                    && a.atom_order == e.atom_order
                    && a.bond_order == e.bond_order =>
            {
                accepted += 1;
                None
            }
            (Ok(a), Some(e)) => Some(format!(
                "text differs: {}; atom order differs: {}; bond order differs: {}",
                a.text != e.text,
                a.atom_order != e.atom_order,
                a.bond_order != e.bond_order
            )),
            (Err(_), None) => {
                rejected += 1;
                None
            }
            (Err(e), Some(_)) => Some(format!("Rejected supported traversal: {e}")),
            (Ok(_), None) => Some(format!("Accepted rejected traversal: {:?}", case.failure)),
        };
        if let Some(error) = failure {
            mismatches += 1;
            if mismatches == 1 {
                std::fs::create_dir_all(root.join("artifacts"))?;
                std::fs::write(
                    root.join("artifacts/smiles-traversal-mismatch.json"),
                    serde_json::to_vec_pretty(&case)?,
                )?;
                if let Ok(actual) = actual {
                    std::fs::write(
                        root.join("artifacts/smiles-traversal-actual.json"),
                        serde_json::to_vec_pretty(&actual)?,
                    )?;
                }
            }
            if failures.len() < 20 {
                failures.push(format!("{}: {error}", case.name));
            }
        }
    }
    assert!(child.wait()?.success(), "Traversal oracle failed");
    eprintln!(
        "SMILES traversal: {accepted} accepted, {rejected} rejected, {mismatches} mismatches"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(
        accepted > 25000 && rejected > 0,
        "Insufficient traversal coverage"
    );
    Ok(())
}

fn chain(size: usize) -> Graph {
    use reshiki::chemistry::graph::{Atom, Bond};
    Graph {
        atoms: vec![
            Atom {
                atomic_number: 6,
                ..Atom::default()
            };
            size
        ],
        bonds: (1..size)
            .map(|b| Bond {
                a: b - 1,
                b,
                order: 1,
                aromatic: false,
            })
            .collect(),
    }
}

#[test]
fn long_traversals_use_heap_storage_and_preserve_neighbor_order() -> anyhow::Result<()> {
    let graph = chain(100_000);
    let ranks: Vec<_> = (0..100_000).collect();
    let walk = traversal::build(&graph, &vec![false; graph.bonds.len()], &ranks, 0)?;
    assert_eq!(walk.tokens().len(), 199_999);
    assert!(walk.ring_closures().iter().all(Vec::is_empty));
    for (i, neighbors) in walk.atom_bond_order().iter().enumerate() {
        let expected = match i {
            0 => vec![0],
            99_999 => vec![99_998],
            _ => vec![i - 1, i],
        };
        assert_eq!(*neighbors, expected);
    }
    let metadata = Metadata::unspecified(&graph);
    let writer = Writer::new(&graph, &metadata, None)?;
    let output = walk.render(
        &writer,
        &vec![Direction::None; graph.bonds.len()],
        &vec![AtomProperties::default(); graph.atoms.len()],
        Options {
            isomeric: false,
            ..Options::default()
        },
    )?;
    assert_eq!(output.text, "C".repeat(100_000));
    assert_eq!(output.atom_order, (0..100_000).collect::<Vec<_>>());
    assert_eq!(output.bond_order, (0..99_999).collect::<Vec<_>>());
    Ok(())
}

#[test]
fn malformed_traversals_and_excessive_output_fail_without_editing_input() -> anyhow::Result<()> {
    let graph = chain(3);
    let before = serde_json::to_value(&graph)?;
    for (rings, ranks, start) in [
        (vec![], vec![0, 1, 2], 0),
        (vec![false; 2], vec![0, 1], 0),
        (vec![false; 2], vec![0, 0, 2], 0),
        (vec![false; 2], vec![0, 1, u32::MAX], 0),
        (vec![false; 2], vec![0, 1, 2], usize::MAX),
    ] {
        assert!(traversal::build(&graph, &rings, &ranks, start).is_err());
    }
    let disconnected = Graph {
        atoms: graph.atoms.clone(),
        bonds: Vec::new(),
    };
    assert!(traversal::build(&disconnected, &[], &[0, 1, 2], 0).is_err());
    let walk = traversal::build(&graph, &[false; 2], &[0, 1, 2], 0)?;
    let metadata = Metadata::unspecified(&graph);
    let writer = Writer::new(&graph, &metadata, None)?;
    assert!(
        walk.render(
            &writer,
            &[],
            &[AtomProperties::default(); 3],
            Options::default()
        )
        .is_err()
    );
    assert!(
        walk.render(&writer, &[Direction::None; 2], &[], Options::default())
            .is_err()
    );
    assert_eq!(before, serde_json::to_value(&graph)?);
    let graph = chain(18);
    let metadata = Metadata::unspecified(&graph);
    let writer = Writer::new(&graph, &metadata, None)?;
    let walk = traversal::build(&graph, &[false; 17], &(0..18).collect::<Vec<_>>(), 0)?;
    let label = "X".repeat(1024 * 1024);
    let properties = vec![
        AtomProperties {
            custom_symbol: Some(&label),
            ..AtomProperties::default()
        };
        18
    ];
    assert!(matches!(
        walk.render(
            &writer,
            &[Direction::None; 17],
            &properties,
            Options::default()
        ),
        Err(traversal::Error::Limit)
    ));
    Ok(())
}
