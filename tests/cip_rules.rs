#[path = "support/cip_view.rs"]
mod cip_view;
use anyhow::Context;
use cip_view::View;
use reshiki::chemistry::{
    RDKIT_VERSION,
    stereo::{
        cip::{
            Error, Molecule,
            digraph::{Descriptor, Digraph},
            rules::{self, Iterations, Rule, Rules},
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
    root: usize,
    atrop: bool,
    seed: usize,
    scheme: usize,
    aux: usize,
    reroot: bool,
    deep: bool,
    budget: u32,
    operation: usize,
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
const RULES: [Rule; 8] = [
    Rule::AtomicNumber,
    Rule::RingDuplicate,
    Rule::Isotope,
    Rule::DoubleBondStereo,
    Rule::DescriptorType,
    Rule::PseudoDescriptor,
    Rule::LegacyDescriptor,
    Rule::ReferenceAtom,
];
fn run(case: &Case) -> anyhow::Result<Value> {
    let mut mol = Molecule::new(&case.before)?;
    let mut graph = Digraph::new(&mol, case.root, case.atrop)?;
    let atoms = case.before.graph.atoms.len();
    graph.edges(&mut mol, 0)?;
    if case.aux != 0 || case.reroot {
        graph.nodes_for_atom(&mut mol, atoms.checked_sub(1).context("Empty input")?)?;
        let view = View::new(&graph)?;
        if case.aux != 0 {
            let count = if case.aux == 1 { 15 } else { 18 };
            for (i, &node) in view.nodes.iter().enumerate() {
                graph.set_node_aux(node, AUX[(i + case.seed) % count])?;
            }
            for (i, &edge) in view.edges.iter().enumerate() {
                graph.set_edge_aux(edge, AUX[(i * 3 + case.seed + 1) % count])?;
            }
            graph.set_rule6_reference(Some((case.root + 1) % atoms))?;
        }
        if case.reroot {
            let node = *view
                .nodes
                .get(case.seed % view.nodes.len())
                .context("Missing root")?;
            graph.change_root(&mut mol, node)?;
        }
    }
    let node = graph.current_root();
    let mut edges = graph.edges(&mut mol, node)?.to_vec();
    if case.seed % 2 == 1 {
        edges.reverse();
    }
    let size = edges.len();
    if size != 0 {
        edges.rotate_left(case.seed % size);
    }
    let rules = if case.scheme == 8 {
        Rules::constitutional()
    } else if case.scheme == 9 {
        Rules::new(&RULES)?
    } else {
        Rules::single(*RULES.get(case.scheme).context("Invalid rule")?)
    };
    let mut iterations = Iterations::new(case.budget);
    let result = {
        let mut ctx = rules::Context::new(&mut mol, &mut graph, &mut iterations)?;
        match case.operation {
            0 => match (edges.first(), edges.last()) {
                (Some(&a), Some(&b)) => json!(rules.compare(&mut ctx, a, b, case.deep)?),
                _ => Value::Null,
            },
            1 => {
                let sorted = rules.sort(&mut ctx, node, &edges, case.deep)?;
                let view = View::new(&graph)?;
                json!({"edges":view.edge_ids(&sorted.edges)?,"unique":sorted.unique,"pseudo":sorted.pseudo})
            }
            2 => {
                let groups = rules.groups(&mut ctx, &edges)?;
                let view = View::new(&graph)?;
                json!(
                    groups
                        .iter()
                        .map(|g| view.edge_ids(g))
                        .collect::<anyhow::Result<Vec<_>>>()?
                )
            }
            _ => anyhow::bail!("Invalid operation"),
        }
    };
    Ok(json!({"result":result,"graph":View::new(&graph)?.snapshot(&graph,atoms)?}))
}

#[test]
fn comparisons_priorities_and_groups_match_native_rdkit() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/cip_rules_reference.py"))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().context("Missing oracle output")?).lines();
    let header: Value = serde_json::from_str(&lines.next().context("Missing version")??)?;
    assert_eq!(header["rdkit_version"], RDKIT_VERSION);
    let (mut accepted, mut rejected, mut mismatches, mut iteration_limits) = (0, 0, 0, 0);
    let mut failures = Vec::new();
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        let before = serde_json::to_value(&case.before)?;
        let result = run(&case);
        let message = result.as_ref().err().map(ToString::to_string);
        let actual = match result {
            Ok(value) => value,
            Err(error) => json!({"error":match error.downcast_ref::<Error>() {
                Some(Error::Nodes)=>"nodes",Some(Error::Limit)=>"limit",Some(Error::Iterations)=>"iterations",_=>"invalid",
            }}),
        };
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
                    root.join("artifacts/cip-rules-first-mismatch.json"),
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
            iteration_limits += usize::from(actual["error"] == "iterations");
        } else {
            accepted += 1;
        }
    }
    assert!(child.wait()?.success());
    eprintln!(
        "CIP rules: {accepted} accepted, {rejected} rejected ({iteration_limits} iteration limits), {mismatches} mismatches"
    );
    assert_eq!(mismatches, 0, "{}", failures.join("\n"));
    assert!(accepted > 5_000);
    assert!(iteration_limits > 100);
    Ok(())
}
