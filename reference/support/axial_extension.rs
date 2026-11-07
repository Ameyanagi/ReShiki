//! #892 opens wedged atropisomer inputs that the original worker refused.
//! Such a case is accepted only when every axis RDKit perceives in the input
//! survives on the same bond with the same winding (bond stereo 6/7) and the
//! same M/P label, and no other axis appears. `reference/atropisomer_axes.py`
//! reads the input as the original worker does.
use reshiki::{
    chemistry::{
        document,
        stereo::{
            cip::{configuration::Target, digraph::Descriptor, label},
            perception::State,
        },
    },
    document::Document,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeSet, HashMap},
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::Mutex,
};

/// What the test checked: one molecular state, an RXN reader result with
/// `reactants`/`products`/`agents`, or a finished document.
#[allow(dead_code)]
pub enum Checked<'a> {
    State(&'a Value),
    Reaction(&'a Value),
    Document(&'a Document),
}

type Axis = (usize, usize, u64, Option<String>);

pub fn accepted(
    name: &str,
    failure: Option<&str>,
    format: &str,
    text: &str,
    checked: Checked,
) -> bool {
    // The original worker raises this error only for RDKit's atropisomer bond
    // stereo; its generic contract errors count only for the atropisomer
    // corpus and RDKit's wedged biaryl reaction fixture for GitHub #7669.
    let atropisomer = failure == Some("This bond stereochemistry is not supported yet.")
        || ((name.contains("atropisomer/") || name == "reaction source/github7669_1.rxn")
            && matches!(
                failure,
                Some(
                    "Outside the existing editable chemistry contract"
                        | "Outside the editable reaction contract"
                )
            ));
    if !atropisomer {
        return false;
    }
    let Some(expected) = rdkit(format, text) else {
        return false;
    };
    match checked {
        Checked::State(state) => {
            expected.len() == 1 && axes(state).is_some_and(|actual| actual == expected[0])
        }
        Checked::Reaction(imported) => {
            let states: Vec<_> = ["reactants", "products", "agents"]
                .iter()
                .flat_map(|role| imported[*role].as_array().into_iter().flatten())
                .map(|participant| &participant["molecule"]["state"])
                .collect();
            states.len() == expected.len()
                && states
                    .iter()
                    .zip(&expected)
                    .all(|(state, axes_)| axes(state).is_some_and(|actual| &actual == axes_))
        }
        Checked::Document(doc) => in_document(doc, &expected).unwrap_or(false),
    }
}

/// Map each RDKit molecule's atoms onto the prepared document state: a
/// reaction participant lists its atoms in document order, as `write_rxn` does.
fn in_document(doc: &Document, expected: &[BTreeSet<Axis>]) -> Option<bool> {
    let molecule = document::prepare(doc).ok()?;
    let state_index: HashMap<u64, usize> = molecule
        .ids
        .iter()
        .enumerate()
        .map(|(i, id)| (*id, i))
        .collect();
    let position: HashMap<u64, usize> = doc
        .atoms
        .iter()
        .enumerate()
        .map(|(i, a)| (a.id, i))
        .collect();
    let molecules: Vec<Vec<u64>> = match doc.reactions.as_slice() {
        [] => vec![doc.atoms.iter().map(|a| a.id).collect()],
        [reaction] => reaction
            .reactants
            .iter()
            .chain(&reaction.products)
            .chain(&reaction.agents)
            .map(|p| {
                let mut atoms = p.atoms.clone();
                atoms.sort_by_key(|id| position.get(id).copied());
                atoms
            })
            .collect(),
        _ => return None,
    };
    if molecules.len() != expected.len() {
        return Some(false);
    }
    let mut wanted = BTreeSet::new();
    for (atoms, axes_) in molecules.iter().zip(expected) {
        for (a, b, stereo, label) in axes_ {
            let a = *state_index.get(atoms.get(*a)?)?;
            let b = *state_index.get(atoms.get(*b)?)?;
            wanted.insert((a.min(b), a.max(b), *stereo, label.clone()));
        }
    }
    Some(axes(&serde_json::to_value(&molecule.state).ok()?)? == wanted)
}

/// Every axis of a state, labelled by the native CIP labeler.
fn axes(state: &Value) -> Option<BTreeSet<Axis>> {
    let parsed: State = serde_json::from_value(state.clone()).ok()?;
    let options = label::Options {
        atoms: None,
        bonds: None,
        max_iterations: 0,
    };
    let labels: HashMap<usize, &str> = label::assign(&parsed, &options)
        .ok()?
        .labels
        .iter()
        .filter_map(|l| match (l.target, l.descriptor) {
            (Target::Bond(i), Descriptor::M) => Some((i, "M")),
            (Target::Bond(i), Descriptor::P) => Some((i, "P")),
            _ => None,
        })
        .collect();
    Some(
        parsed
            .metadata
            .bonds
            .iter()
            .zip(&parsed.graph.bonds)
            .enumerate()
            .filter(|(_, (meta, _))| matches!(meta.stereo, 6 | 7))
            .map(|(i, (meta, bond))| {
                (
                    bond.a.min(bond.b),
                    bond.a.max(bond.b),
                    u64::from(meta.stereo),
                    labels.get(&i).map(|l| (*l).to_owned()),
                )
            })
            .collect(),
    )
}

/// RDKit's axes per molecule, from a persistent helper process.
fn rdkit(format: &str, text: &str) -> Option<Vec<BTreeSet<Axis>>> {
    static HELPER: Mutex<Option<(Child, ChildStdin, BufReader<ChildStdout>)>> = Mutex::new(None);
    let mut helper = HELPER.lock().ok()?;
    if helper.is_none() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut child = Command::new(root.join(if cfg!(windows) {
            ".venv/Scripts/python.exe"
        } else {
            ".venv/bin/python"
        }))
        .arg(root.join("reference/atropisomer_axes.py"))
        .env("PYTHONUTF8", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .ok()?;
        let stdin = child.stdin.take()?;
        let stdout = BufReader::new(child.stdout.take()?);
        *helper = Some((child, stdin, stdout));
    }
    let (_, stdin, stdout) = helper.as_mut()?;
    writeln!(stdin, "{}", json!({"format": format, "text": text})).ok()?;
    let mut line = String::new();
    stdout.read_line(&mut line).ok()?;
    let molecules: Vec<Vec<Value>> = serde_json::from_str(&line).ok()?;
    molecules
        .iter()
        .map(|axes_| {
            axes_
                .iter()
                .map(|axis| {
                    Some((
                        usize::try_from(axis["atoms"][0].as_u64()?).ok()?,
                        usize::try_from(axis["atoms"][1].as_u64()?).ok()?,
                        axis["stereo"].as_u64()?,
                        axis["label"].as_str().map(str::to_owned),
                    ))
                })
                .collect()
        })
        .collect()
}
