use anyhow::Context;
use reshiki::chemistry::{
    RDKIT_VERSION,
    smiles::{
        traversal::Output,
        write::{self, Options},
    },
    stereo::perception::State,
};
use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};

#[derive(Deserialize, Serialize)]
struct Expected {
    text: String,
    atom_order: Vec<usize>,
    bond_order: Vec<usize>,
}
#[derive(Deserialize, Serialize)]
struct AtomProperties {
    custom_symbol: Option<String>,
    supplement: Option<String>,
    broken_chirality: bool,
}
#[derive(Deserialize, Serialize)]
struct Case {
    name: String,
    before: State,
    options: Options,
    properties: Vec<AtomProperties>,
    expected: Option<Expected>,
    failure: Option<String>,
}
impl Case {
    fn actual(&self) -> anyhow::Result<Output> {
        let props = self
            .properties
            .iter()
            .map(|p| reshiki::chemistry::smiles::symbols::AtomProperties {
                custom_symbol: p.custom_symbol.as_deref(),
                supplement: p.supplement.as_deref(),
                broken_chirality: p.broken_chirality,
            })
            .collect::<Vec<_>>();
        Ok(write::with_symbols(&self.before, &props, self.options)?)
    }
}

#[test]
fn full_writer_matches_native_molecular_output() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/smiles_write_reference.py"))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().context("Missing writer oracle")?).lines();
    let version: serde_json::Value =
        serde_json::from_str(&lines.next().context("Missing version")??)?;
    assert_eq!(version["rdkit_version"], RDKIT_VERSION);
    let (mut accepted, mut rejected, mut mismatches) = (0, 0, 0);
    let mut failures = Vec::new();
    let mut all_failures = Vec::new();
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        let actual = case.actual();
        let error = match (&actual, &case.expected) {
            (Ok(a), Some(e))
                if a.text == e.text
                    && a.atom_order == e.atom_order
                    && a.bond_order == e.bond_order =>
            {
                accepted += 1;
                None
            }
            (Err(_), None) => {
                rejected += 1;
                None
            }
            (Ok(a), Some(e)) => Some(format!("expected {}, actual {}", e.text, a.text)),
            (Err(e), Some(_)) => Some(e.to_string()),
            (Ok(_), None) => Some(format!("Accepted native rejection: {:?}", case.failure)),
        };
        if let Some(error) = error {
            mismatches += 1;
            all_failures.push(format!("{}: {error}", case.name));
            if mismatches == 1 {
                std::fs::create_dir_all(root.join("artifacts"))?;
                std::fs::write(
                    root.join("artifacts/smiles-write-mismatch.json"),
                    serde_json::to_vec_pretty(&case)?,
                )?;
            }
            if failures.len() < 20 {
                failures.push(format!("{}: {error}", case.name));
            }
        }
    }
    assert!(child.wait()?.success(), "Writer oracle failed");
    eprintln!("SMILES writer: {accepted} accepted, {rejected} rejected, {mismatches} mismatches");
    if !all_failures.is_empty() {
        std::fs::write(
            root.join("artifacts/smiles-write-failures.txt"),
            all_failures.join("\n"),
        )?;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(accepted > 25_000);
    Ok(())
}

#[test]
fn invalid_writer_states_fail_without_mutating_the_molecule() -> anyhow::Result<()> {
    use reshiki::chemistry::smiles::{prepare, symbols::AtomProperties};
    let state = prepare("F[C@H](Cl)/C=C/C.O")?.state;
    let original = serde_json::to_value(&state)?;
    let output = write::write(&state, Options::default())?;
    assert!(!output.text.is_empty());
    assert_eq!(original, serde_json::to_value(&state)?);
    assert!(
        write::write(
            &state,
            Options {
                root: Some(usize::MAX),
                ..Options::default()
            }
        )
        .is_err()
    );
    assert!(write::with_symbols(&state, &[], Options::default()).is_err());
    for variant in 0..9 {
        let mut bad = state.clone();
        match variant {
            0 => {
                bad.valences.pop();
            }
            1 => {
                bad.metadata.bonds.pop();
            }
            2 => {
                bad.properties.atoms.pop();
            }
            3 => {
                bad.directions.pop();
            }
            4 => {
                bad.graph.bonds.get_mut(0).context("Missing test bond")?.b = usize::MAX;
            }
            5 => {
                bad.rings.atoms = vec![vec![0, 1, 5]];
            }
            6 => {
                bad.hybridizations.pop();
            }
            7 => {
                bad.conjugated.pop();
            }
            _ => {
                bad.properties.bond_codes.pop();
            }
        }
        let before = serde_json::to_value(&bad)?;
        assert!(
            write::write(&bad, Options::default()).is_err(),
            "variant {variant}"
        );
        assert_eq!(before, serde_json::to_value(&bad)?);
    }
    let long_label = "x".repeat(1024 * 1024 + 1);
    let props = vec![
        AtomProperties {
            supplement: Some(&long_label),
            ..AtomProperties::default()
        };
        state.graph.atoms.len()
    ];
    assert!(write::with_symbols(&state, &props, Options::default()).is_err());
    Ok(())
}

#[test]
fn many_disconnected_atoms_and_combined_output_are_bounded() -> anyhow::Result<()> {
    use reshiki::chemistry::smiles::{prepare, symbols::AtomProperties};
    let mut state = prepare("[Na+]")?.state;
    let count = 20_000;
    state.graph.atoms = vec![
        state
            .graph
            .atoms
            .first()
            .context("Missing test atom")?
            .clone();
        count
    ];
    state.metadata.atoms = vec![
        state
            .metadata
            .atoms
            .first()
            .context("Missing test metadata")?
            .clone();
        count
    ];
    state.valences = vec![*state.valences.first().context("Missing test cache")?; count];
    state.hybridizations = vec![
        *state
            .hybridizations
            .first()
            .context("Missing hybridization")?;
        count
    ];
    state.properties.atoms = vec![
        state
            .properties
            .atoms
            .first()
            .context("Missing properties")?
            .clone();
        count
    ];
    let output = write::write(&state, Options::default())?;
    assert_eq!(output.text.split('.').count(), count);
    assert_eq!(output.atom_order, (0..count).collect::<Vec<_>>());
    // Individually valid labels must not evade the whole-molecule text limit.
    let state = prepare(&vec!["C"; 40].join("."))?.state;
    let label = "x".repeat(512 * 1024);
    let props = vec![
        AtomProperties {
            supplement: Some(&label),
            ..AtomProperties::default()
        };
        40
    ];
    assert!(matches!(
        write::with_symbols(&state, &props, Options::default()),
        Err(write::Error::Limit)
    ));
    Ok(())
}
