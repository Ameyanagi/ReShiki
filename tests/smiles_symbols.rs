use anyhow::Context;
use reshiki::chemistry::{
    RDKIT_VERSION,
    graph::{Graph, Valence},
    kekulize::Direction,
    ranking::Metadata,
    smiles::symbols::{self, AtomProperties, Options, Writer},
};
use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};

#[derive(Default, Deserialize, Serialize)]
struct Properties {
    custom_symbol: Option<String>,
    supplement: Option<String>,
    broken_chirality: bool,
}
#[derive(Deserialize, Serialize)]
struct Case {
    name: String,
    graph: Graph,
    metadata: Metadata,
    directions: Vec<Direction>,
    cache: Option<Vec<Valence>>,
    options: Options,
    properties: Vec<Properties>,
    operation: String,
    left: Option<usize>,
    expected: Option<Vec<String>>,
    failure: Option<String>,
}
impl Case {
    fn actual(&self) -> Result<Vec<String>, symbols::Error> {
        let writer = Writer::new(&self.graph, &self.metadata, self.cache.as_deref())?;
        if self.operation == "atoms" {
            self.properties
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    writer.atom(
                        i,
                        self.options,
                        AtomProperties {
                            custom_symbol: p.custom_symbol.as_deref(),
                            supplement: p.supplement.as_deref(),
                            broken_chirality: p.broken_chirality,
                        },
                    )
                })
                .collect()
        } else {
            self.graph
                .bonds
                .iter()
                .zip(&self.directions)
                .enumerate()
                .map(|(i, (b, &d))| {
                    writer
                        .bond(i, d, self.left.unwrap_or(b.a), self.options)
                        .map(str::to_owned)
                })
                .collect()
        }
    }
}

#[test]
fn atom_and_bond_symbols_match_native_writers() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/smiles_symbols_reference.py"))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().context("Missing symbols oracle")?).lines();
    let header: serde_json::Value =
        serde_json::from_str(&lines.next().context("Missing oracle version")??)?;
    assert_eq!(header["rdkit_version"], RDKIT_VERSION);
    let (mut accepted, mut rejected, mut mismatches) = (0, 0, 0);
    let mut failures = Vec::new();
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        let actual = case.actual();
        let failure = match (&actual, &case.expected) {
            (Ok(a), Some(e)) if a == e => {
                accepted += 1;
                None
            }
            (Err(_), None) => {
                rejected += 1;
                None
            }
            (Ok(a), Some(e)) => Some(format!("{a:?} != {e:?}")),
            (Err(e), Some(_)) => Some(format!("Rejected supported symbols: {e}")),
            (Ok(a), None) => Some(format!("Accepted rejected input {a:?}: {:?}", case.failure)),
        };
        if let Some(error) = failure {
            mismatches += 1;
            if mismatches == 1 {
                std::fs::create_dir_all(root.join("artifacts"))?;
                std::fs::write(
                    root.join("artifacts/smiles-symbols-mismatch.json"),
                    serde_json::to_vec_pretty(&case)?,
                )?;
            }
            if failures.len() < 24 {
                failures.push(format!("{}: {error}", case.name));
            }
        }
    }
    assert!(child.wait()?.success(), "Symbols reference failed");
    eprintln!("SMILES symbols: {accepted} accepted, {rejected} rejected, {mismatches} mismatches");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(
        accepted > 50_000 && rejected > 100,
        "Insufficient symbols coverage"
    );
    Ok(())
}

#[test]
fn symbol_inputs_are_bounded_and_never_edit_chemical_state() -> anyhow::Result<()> {
    let source = reshiki::chemistry::smiles::parse("[C@H](F)(Cl)Br")?;
    let before = serde_json::to_value(&source)?;
    let writer = Writer::new(&source.graph, &source.metadata, None)?;
    let options = Options::default();
    assert!(
        writer
            .atom(usize::MAX, options, AtomProperties::default())
            .is_err()
    );
    assert!(
        writer
            .bond(usize::MAX, Direction::None, 0, options)
            .is_err()
    );
    assert!(
        writer
            .bond(0, Direction::None, usize::MAX, options)
            .is_err()
    );
    let excessive = "X".repeat(1024 * 1024 + 1);
    assert!(matches!(
        writer.atom(
            0,
            options,
            AtomProperties {
                custom_symbol: Some(&excessive),
                ..AtomProperties::default()
            }
        ),
        Err(symbols::Error::Limit)
    ));
    assert!(matches!(
        writer.atom(
            0,
            options,
            AtomProperties {
                supplement: Some(&excessive),
                ..AtomProperties::default()
            }
        ),
        Err(symbols::Error::Limit)
    ));
    let half = "X".repeat(512 * 1024 + 1);
    assert!(matches!(
        writer.atom(
            0,
            options,
            AtomProperties {
                custom_symbol: Some(&half),
                supplement: Some(&half),
                ..AtomProperties::default()
            }
        ),
        Err(symbols::Error::Limit)
    ));
    assert!(Writer::new(&source.graph, &source.metadata, Some(&[])).is_err());
    let malformed = vec![
        Valence {
            explicit_valence: u32::MAX,
            implicit_hydrogens: u32::MAX
        };
        source.graph.atoms.len()
    ];
    assert!(Writer::new(&source.graph, &source.metadata, Some(&malformed)).is_err());
    let mut metadata = source.metadata.clone();
    metadata.atoms.clear();
    assert!(Writer::new(&source.graph, &metadata, None).is_err());
    let mut invalid = source.graph.clone();
    if let Some(bond) = invalid.bonds.first_mut() {
        bond.b = usize::MAX;
    }
    assert!(Writer::new(&invalid, &source.metadata, None).is_err());
    // A failed request cannot poison subsequent writing or invert the stored
    // stereochemistry; traversal-dependent inversions operate on a later copy.
    assert_eq!(
        writer.atom(0, options, AtomProperties::default())?,
        "[C@@H]"
    );
    assert_eq!(writer.bond(0, Direction::None, 0, options)?, "");
    assert_eq!(before, serde_json::to_value(&source)?);
    Ok(())
}
