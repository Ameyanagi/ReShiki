#[path = "support/cip_view.rs"]
mod cip_view;
use anyhow::Context;
use cip_view::View;
use reshiki::chemistry::{
    RDKIT_VERSION,
    stereo::{
        cip::{
            Error, Molecule,
            configuration::{Configuration, Target, parity4},
            digraph::{Descriptor, Digraph},
            rules::{Iterations, Rules},
        },
        perception::State,
    },
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};

#[derive(Deserialize)]
struct Case {
    name: String,
    before: State,
    kind: u8,
    target: usize,
    cfg: u8,
    origin: i32,
    node_atom: i32,
    seed: usize,
    full: bool,
    aux: usize,
    budget: u32,
    repeat: bool,
    reverse: bool,
    write: i32,
    expected: Value,
}
const AUX: [Descriptor; 18] = [
    Descriptor::None,
    Descriptor::Unknown,
    Descriptor::Other,
    Descriptor::R,
    Descriptor::S,
    Descriptor::PseudoR,
    Descriptor::PseudoS,
    Descriptor::SeqTrans,
    Descriptor::SeqCis,
    Descriptor::E,
    Descriptor::Z,
    Descriptor::M,
    Descriptor::P,
    Descriptor::PseudoM,
    Descriptor::PseudoP,
    Descriptor::SquarePlanar,
    Descriptor::TrigonalBipyramidal,
    Descriptor::Octahedral,
];
fn ids(values: &[Option<usize>]) -> Vec<i64> {
    values.iter().map(|a| a.map_or(-1, |a| a as i64)).collect()
}
fn run(case: &Case) -> anyhow::Result<Value> {
    let mut mol = Molecule::new(&case.before)?;
    let mut config = if case.kind == 0 {
        Configuration::tetrahedral(&mol, case.target)?
    } else {
        let bond = case
            .before
            .graph
            .bonds
            .get(case.target)
            .context("Missing bond")?;
        let mut foci = [bond.a, bond.b];
        if case.reverse {
            foci.swap(0, 1);
        }
        if case.kind == 1 {
            Configuration::sp2(&mol, case.target, foci, case.cfg)?
        } else {
            Configuration::atropisomer(&mol, case.target, foci, case.cfg)?
        }
    };
    let mut graph = if case.origin < 0 {
        config.make_digraph(&mol)?
    } else {
        Digraph::new(&mol, case.origin as usize, case.kind == 2)?
    };
    let atoms = case.before.graph.atoms.len();
    let mut node = graph.original_root();
    if case.aux != 0 || case.node_atom >= 0 || case.repeat {
        graph.nodes_for_atom(&mut mol, atoms.checked_sub(1).context("Empty molecule")?)?;
        let view = View::new(&graph)?;
        if case.aux != 0 {
            for (i, &node) in view.nodes.iter().enumerate() {
                let desc = if case.aux == 3 {
                    match graph
                        .node(node)?
                        .atom
                        .and_then(|a| case.before.metadata.atoms.get(a))
                        .map(|a| a.chiral_tag)
                    {
                        Some(1) => Descriptor::R,
                        Some(2) => Descriptor::S,
                        _ => Descriptor::None,
                    }
                } else if case.aux == 1 {
                    AUX[(i + case.seed) % 15]
                } else {
                    AUX[3 + (i + case.seed) % 4]
                };
                graph.set_node_aux(node, desc)?;
            }
            for (i, &edge) in view.edges.iter().enumerate() {
                graph.set_edge_aux(
                    edge,
                    if case.aux == 3 {
                        Descriptor::None
                    } else {
                        AUX[(i * 3 + case.seed + 1) % 15]
                    },
                )?;
            }
            graph.set_rule6_reference(Some((case.target + 1) % atoms))?;
        }
        if case.node_atom >= 0 {
            let nodes = graph.nodes_for_atom(&mut mol, case.node_atom as usize)?;
            let mut real = Vec::new();
            for id in nodes {
                if !graph.node(id)?.is_duplicate_or_h() {
                    real.push(id);
                }
            }
            anyhow::ensure!(!real.is_empty(), "Missing auxiliary node");
            node = *real
                .get(case.seed % real.len())
                .context("Missing auxiliary node")?;
        }
        if case.repeat {
            graph.change_root(
                &mut mol,
                *view
                    .nodes
                    .get(case.seed % view.nodes.len())
                    .context("Missing root")?,
            )?;
        }
    }
    let rules = if case.full {
        Rules::full()
    } else {
        Rules::constitutional()
    };
    let mut iterations = Iterations::new(case.budget);
    let mut results = Vec::new();
    let mut has = match config.target() {
        Target::Atom(a) => case
            .before
            .properties
            .atoms
            .get(a)
            .context("Missing props")?
            .cip_code
            .is_some(),
        Target::Bond(b) => case
            .before
            .properties
            .bond_codes
            .get(b)
            .context("Missing code")?
            .is_some(),
    };
    for _ in 0..if case.repeat { 2 } else { 1 } {
        let descriptor = if case.origin >= 0 || case.node_atom >= 0 {
            config.label_at(&mut mol, &mut graph, node, &rules, &mut iterations)?
        } else {
            config.label(&mut mol, &mut graph, &rules, &mut iterations)?
        };
        let chosen = if case.write >= 0 {
            *AUX.get(case.write as usize).context("Invalid write")?
        } else {
            match case.kind {
                0 => Descriptor::R,
                1 => Descriptor::E,
                _ => Descriptor::M,
            }
        };
        let primary = config.primary_label(chosen)?;
        let code = match chosen {
            Descriptor::R => "R",
            Descriptor::S => "S",
            Descriptor::PseudoR => "r",
            Descriptor::PseudoS => "s",
            Descriptor::E => "E",
            Descriptor::Z => "Z",
            Descriptor::SeqTrans => "e",
            Descriptor::SeqCis => "z",
            Descriptor::M => "M",
            Descriptor::P => "P",
            Descriptor::PseudoM => "m",
            Descriptor::PseudoP => "p",
            _ => "invalid",
        };
        let stereo = if let Some((stereo, carriers)) = primary.bond_stereo {
            json!([stereo, carriers])
        } else if case.kind != 0 {
            let b = case
                .before
                .metadata
                .bonds
                .get(case.target)
                .context("Missing stereo")?;
            json!([b.stereo, b.stereo_atoms])
        } else {
            Value::Null
        };
        results.push(json!({"descriptor":descriptor as u8,"code":code,"neighbor_order":ids(&primary.neighbor_order),"has":[has,true,false],"stereo":stereo,"graph":View::new(&graph)?.snapshot(&graph,atoms)?}));
        has = false;
    }
    Ok(json!({"foci":config.foci(),"carriers":ids(config.carriers()),"results":results}))
}

#[test]
fn configuration_evaluation_matches_independent_native_apis() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/cip_configuration_reference.py"))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().context("Missing output")?).lines();
    let header: Value = serde_json::from_str(&lines.next().context("Missing version")??)?;
    assert_eq!(header["rdkit_version"], RDKIT_VERSION);
    let (mut accepted, mut rejected, mut mismatches, mut limits, mut parities) = (0, 0, 0, 0, 0);
    let mut failures = Vec::new();
    let mut native_crashes = 0;
    let mut observed = std::collections::HashSet::new();
    for line in lines {
        let value: Value = serde_json::from_str(&line?)?;
        if let Some(values) = value.get("parity").and_then(Value::as_array) {
            for a in 0..256usize {
                for b in 0..256usize {
                    let first: Vec<_> = (0..4).map(|i| (a >> (i * 2)) % 4).collect();
                    let second: Vec<_> = (0..4).map(|i| (b >> (i * 2)) % 4).collect();
                    assert_eq!(
                        json!(parity4(&first, &second)?),
                        *values.get(a * 256 + b).context("Missing parity")?,
                        "{a}/{b}"
                    );
                    parities += 1;
                }
            }
            continue;
        }
        let case: Case = serde_json::from_value(value)?;
        let before = serde_json::to_value(&case.before)?;
        let result = run(&case);
        let message = result.as_ref().err().map(ToString::to_string);
        let checked_invalid = result
            .as_ref()
            .is_err_and(|error| matches!(error.downcast_ref::<Error>(), Some(Error::Invalid(_))));
        let mut actual = match result {
            Ok(value) => value,
            Err(error) => json!({"error":match error.downcast_ref::<Error>() {
                Some(Error::Iterations)=>"iterations",Some(Error::Nodes)=>"nodes",Some(Error::Limit)=>"limit",_=>"invalid",
            }}),
        };
        if case.expected["error"] == "native_crash" && checked_invalid {
            actual = json!({"error":"native_crash"});
            native_crashes += 1;
        }
        assert_eq!(
            before,
            serde_json::to_value(&case.before)?,
            "Input changed: {}",
            case.name
        );
        if actual != case.expected {
            mismatches += 1;
            if mismatches == 1 {
                std::fs::create_dir_all(root.join("artifacts"))?;
                std::fs::write(
                    root.join("artifacts/cip-configuration-first-mismatch.json"),
                    serde_json::to_vec_pretty(
                        &json!({"name":case.name,"actual":actual,"expected":case.expected,"message":message}),
                    )?,
                )?;
            }
            if failures.len() < 20 {
                failures.push(format!("{}: {:?}", case.name, message));
            }
        } else if actual.get("error").is_some() {
            rejected += 1;
            limits += usize::from(actual["error"] == "iterations");
        } else {
            accepted += 1;
            for result in actual["results"].as_array().context("Missing results")? {
                observed.insert(
                    result["descriptor"]
                        .as_u64()
                        .context("Missing descriptor")?,
                );
            }
        }
    }
    assert!(child.wait()?.success());
    eprintln!(
        "CIP configurations: {accepted} accepted, {rejected} rejected ({limits} iteration limits), {native_crashes} checked native-crash inputs, {parities} parities, {mismatches} mismatches"
    );
    assert_eq!(mismatches, 0, "{}", failures.join("\n"));
    assert!(accepted > 1000 && limits > 100);
    assert_eq!(parities, 65536);
    assert!(native_crashes > 0);
    assert!((1..=14).all(|descriptor| observed.contains(&descriptor)));
    assert!(parity4::<u8>(&[], &[]).is_err());
    Ok(())
}
