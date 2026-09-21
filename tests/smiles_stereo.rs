use anyhow::Context;
use reshiki::chemistry::{
    RDKIT_VERSION,
    smiles::{
        stereo,
        symbols::{AtomProperties, Options, Writer},
        traversal::{self, Output},
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
struct Case {
    name: String,
    before: State,
    ranks: Vec<u32>,
    start: usize,
    broken: Vec<bool>,
    ring_bonds: Vec<bool>,
    explicit: bool,
    isomeric: bool,
    expected: Option<Expected>,
    failure: Option<String>,
}
impl Case {
    fn actual(&self) -> anyhow::Result<Output> {
        let walk = traversal::build(
            &self.before.graph,
            &self.ring_bonds,
            &self.ranks,
            self.start,
        )?;
        let stereo = stereo::canonicalize(&self.before, &walk, &self.broken, self.isomeric)?;
        let writer = Writer::new(
            &self.before.graph,
            &stereo.metadata,
            Some(&self.before.valences),
        )?;
        let props = self
            .broken
            .iter()
            .map(|&broken_chirality| AtomProperties {
                broken_chirality,
                ..AtomProperties::default()
            })
            .collect::<Vec<_>>();
        Ok(walk.render(
            &writer,
            &stereo.directions,
            &props,
            Options {
                isomeric: self.isomeric,
                all_hydrogens: self.explicit,
                all_bonds: self.explicit,
                ..Options::default()
            },
        )?)
    }
}

#[test]
fn stereo_traversal_matches_complete_native_output() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/smiles_stereo_reference.py"))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().context("Missing stereo oracle")?).lines();
    let version: serde_json::Value =
        serde_json::from_str(&lines.next().context("Missing version")??)?;
    assert_eq!(version["rdkit_version"], RDKIT_VERSION);
    let (mut accepted, mut rejected, mut mismatches) = (0, 0, 0);
    let mut failures = Vec::new();
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
            if mismatches == 1 {
                std::fs::create_dir_all(root.join("artifacts"))?;
                std::fs::write(
                    root.join("artifacts/smiles-stereo-mismatch.json"),
                    serde_json::to_vec_pretty(&case)?,
                )?;
            }
            if failures.len() < 20 {
                failures.push(format!("{}: {error}", case.name));
            }
        }
    }
    assert!(child.wait()?.success(), "Stereo oracle failed");
    eprintln!("SMILES stereo: {accepted} accepted, {rejected} rejected, {mismatches} mismatches");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(accepted > 25_000);
    Ok(())
}

#[test]
fn invalid_stereo_and_stale_traversals_leave_input_unchanged() -> anyhow::Result<()> {
    let state = reshiki::chemistry::smiles::prepare("F[C@H](Cl)/C=C/C")?.state;
    let n = state.graph.atoms.len();
    let ranks = (0..u32::try_from(n)?).collect::<Vec<_>>();
    let walk = traversal::build(
        &state.graph,
        &vec![false; state.graph.bonds.len()],
        &ranks,
        0,
    )?;
    let broken = vec![false; n];
    let original = serde_json::to_value(&state)?;
    stereo::canonicalize(&state, &walk, &broken, true)?;
    assert_eq!(original, serde_json::to_value(&state)?);
    assert!(stereo::canonicalize(&state, &walk, &[], true).is_err());
    for variant in 0..11 {
        let mut bad = state.clone();
        match variant {
            0 => {
                bad.directions.pop();
            }
            1 => {
                bad.valences.pop();
            }
            2 => {
                bad.metadata.atoms.pop();
            }
            3 => {
                bad.properties.atoms.pop();
            }
            4 => {
                bad.hybridizations.pop();
            }
            5 => {
                bad.conjugated.pop();
            }
            6 => {
                bad.properties.done = None;
            }
            7..=9 => {
                bad.properties
                    .atoms
                    .get_mut(0)
                    .context("Missing atom")?
                    .ring_members = Some(vec![match variant {
                    7 => 0,
                    8 => i32::MIN,
                    _ => i32::try_from(n + 1)?,
                }]);
            }
            _ => {
                bad.rings.atoms = vec![vec![0, 1, n]];
            }
        }
        let before = serde_json::to_value(&bad)?;
        assert!(
            stereo::canonicalize(&bad, &walk, &broken, true).is_err(),
            "accepted invalid variant {variant}"
        );
        assert_eq!(before, serde_json::to_value(&bad)?);
    }
    let other = reshiki::chemistry::smiles::prepare("CC(C)(C)CC")?.state;
    assert_eq!(other.graph.atoms.len(), n);
    assert!(stereo::canonicalize(&other, &walk, &broken, true).is_err());
    let mut excessive = state.clone();
    excessive
        .properties
        .atoms
        .get_mut(0)
        .context("Missing atom")?
        .ring_members = Some(vec![1; 2_000_001]);
    assert!(stereo::canonicalize(&excessive, &walk, &broken, true).is_err());
    Ok(())
}

#[test]
fn high_degree_cached_coordination_uses_bounded_storage() -> anyhow::Result<()> {
    use reshiki::chemistry::{
        electronic::Hybridization,
        graph::{Atom, Bond, Valence},
        kekulize::Direction,
        ranking::Metadata,
        stereo::perception::Properties,
    };
    let degree = 20_000;
    let mut state = reshiki::chemistry::smiles::prepare("*")?.state;
    state.graph.atoms = vec![
        Atom {
            atomic_number: 0,
            no_implicit: true,
            ..Atom::default()
        };
        degree + 1
    ];
    state.graph.bonds = (1..=degree)
        .map(|b| Bond {
            a: 0,
            b,
            order: 0,
            aromatic: false,
        })
        .collect();
    state.metadata = Metadata::unspecified(&state.graph);
    state
        .metadata
        .atoms
        .get_mut(0)
        .context("Missing center")?
        .chiral_tag = 6;
    state.valences = vec![
        Valence {
            explicit_valence: 0,
            implicit_hydrogens: 0
        };
        degree + 1
    ];
    state.directions = vec![Direction::None; degree];
    state.conjugated = vec![false; degree];
    state.hybridizations = vec![Hybridization::Unspecified; degree + 1];
    state.properties = Properties::unspecified(&state.graph);
    state.properties.done = Some(true);
    let ranks = (0..=u32::try_from(degree)?).rev().collect::<Vec<_>>();
    let walk = traversal::build(&state.graph, &vec![false; degree], &ranks, 0)?;
    let result = stereo::canonicalize(&state, &walk, &vec![false; degree + 1], true)?;
    assert_eq!(
        serde_json::to_value(&result.metadata)?,
        serde_json::to_value(&state.metadata)?
    );
    assert_eq!(result.directions, state.directions);
    Ok(())
}
