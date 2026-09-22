use anyhow::Context;
use reshiki::chemistry::{
    RDKIT_VERSION,
    stereo::{
        cip::{
            Error, Molecule,
            digraph::{Descriptor, Digraph},
        },
        perception::State,
    },
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{HashMap, hash_map::Entry},
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
    expected: Value,
}

struct View {
    nodes: Vec<usize>,
    edges: Vec<usize>,
    ids: HashMap<usize, usize>,
    eids: HashMap<usize, usize>,
}
impl View {
    fn new(g: &Digraph<'_>) -> anyhow::Result<Self> {
        let mut v = Self {
            nodes: vec![g.original_root()],
            edges: Vec::new(),
            ids: HashMap::from([(g.original_root(), 0)]),
            eids: HashMap::new(),
        };
        let mut i = 0;
        while let Some(&node) = v.nodes.get(i) {
            i += 1;
            for &e in g.node(node)?.stored_edges() {
                if let Entry::Vacant(entry) = v.eids.entry(e) {
                    entry.insert(v.edges.len());
                    v.edges.push(e);
                }
                let other = g.edge(e)?.other(node)?;
                if let Entry::Vacant(entry) = v.ids.entry(other) {
                    entry.insert(v.nodes.len());
                    v.nodes.push(other);
                }
            }
        }
        anyhow::ensure!(
            v.nodes.len() == g.node_count() && v.edges.len() == g.edge_count(),
            "Disconnected CIP expansion"
        );
        Ok(v)
    }
    fn edge_ids(&self, edges: &[usize]) -> anyhow::Result<Vec<usize>> {
        edges
            .iter()
            .map(|e| self.eids.get(e).copied().context("Missing edge ID"))
            .collect()
    }
    fn node_id(&self, node: usize) -> anyhow::Result<usize> {
        self.ids.get(&node).copied().context("Missing node ID")
    }
    fn snapshot(&self, g: &Digraph<'_>, atoms: usize) -> anyhow::Result<Value> {
        let nodes = self
            .nodes
            .iter()
            .map(|&id| -> anyhow::Result<Value> {
                let n = g.node(id)?;
                let distance = if n.is_duplicate() {
                    i32::from(n.distance as u8)
                } else {
                    n.distance
                };
                Ok(json!([
                    n.atom.map_or(-1, |a| a as i64),
                    distance,
                    n.fraction.0,
                    n.fraction.1,
                    n.number,
                    n.isotope,
                    n.mass.to_bits(),
                    n.flags,
                    n.is_terminal(),
                    n.aux as u8,
                    self.edge_ids(n.stored_edges())?
                ]))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let edges = self
            .edges
            .iter()
            .map(|&id| -> anyhow::Result<Value> {
                let e = g.edge(id)?;
                Ok(json!([
                    self.node_id(e.begin)?,
                    self.node_id(e.end)?,
                    e.bond.map_or(-1, |b| b as i64),
                    e.aux as u8
                ]))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let seen = (0..atoms)
            .map(|a| g.seen_atom(a))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(
            json!({"root":self.node_id(g.current_root())?, "nodes":nodes, "edges":edges, "seen":seen, "rule6":g.rule6_reference().map_or(-1, |a| a as i64)}),
        )
    }
}
fn run(case: &Case) -> anyhow::Result<Value> {
    let mut mol = Molecule::new(&case.before)?;
    let mut graph = Digraph::new(&mol, case.root, case.atrop)?;
    let n = case.before.graph.atoms.len();
    let mut states = vec![View::new(&graph)?.snapshot(&graph, n)?];
    graph.edges(&mut mol, 0)?;
    states.push(View::new(&graph)?.snapshot(&graph, n)?);
    if let Some(&e) = graph.node(0)?.stored_edges().first() {
        graph.change_root(&mut mol, graph.edge(e)?.end)?;
    }
    states.push(View::new(&graph)?.snapshot(&graph, n)?);
    let target = n.checked_sub(1).context("Empty native input")?;
    graph.nodes_for_atom(&mut mol, target)?;
    let full = View::new(&graph)?;
    states.push(full.snapshot(&graph, n)?);
    let root = *full
        .nodes
        .get(case.seed % full.nodes.len())
        .context("Missing reroot node")?;
    graph.change_root(&mut mol, root)?;
    graph.set_rule6_reference(Some((case.root + 1) % n))?;
    let aux = [
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
    for (i, &node) in full.nodes.iter().enumerate() {
        graph.set_node_aux(node, aux[i % aux.len()])?;
    }
    for (i, &edge) in full.edges.iter().enumerate() {
        graph.set_edge_aux(edge, aux[(i + 3) % aux.len()])?;
    }
    states.push(full.snapshot(&graph, n)?);
    let find = graph
        .nodes_for_atom(&mut mol, target)?
        .into_iter()
        .map(|id| full.node_id(id))
        .collect::<anyhow::Result<Vec<_>>>()?;
    let mut out = Vec::new();
    let mut to = Vec::new();
    for &id in &full.nodes {
        out.push(full.edge_ids(&graph.nonterminal_out_edges(&mut mol, id)?)?);
        to.push(full.edge_ids(&graph.edges_to_atom(&mut mol, id, target)?)?);
    }
    Ok(json!({"states":states, "find":find, "out":out, "to":to}))
}

#[test]
fn expansion_and_rerooting_match_native_rdkit() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/cip_digraph_reference.py"))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().context("Missing oracle output")?).lines();
    let header: Value = serde_json::from_str(&lines.next().context("Missing version")??)?;
    assert_eq!(header["rdkit_version"], RDKIT_VERSION);
    let (mut accepted, mut rejected, mut mismatches) = (0, 0, 0);
    let mut failures = Vec::new();
    let mut benchmark_seen = false;
    let mut node_limits = 0;
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        let before = serde_json::to_value(&case.before)?;
        let result = run(&case);
        let message = result.as_ref().err().map(ToString::to_string);
        let actual = match result {
            Ok(value) => value,
            Err(error) => json!({"error":match error.downcast_ref::<Error>() {
                Some(Error::Nodes)=>"nodes", Some(Error::Limit)=>"limit", _=>"invalid",
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
                    root.join("artifacts/cip-digraph-first-mismatch.json"),
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
            node_limits += usize::from(actual["error"] == "nodes");
        } else {
            accepted += 1;
        }
        if case.name == "special/0/1/False" {
            benchmark_seen = true;
            assert_eq!(
                actual["states"][3]["nodes"].as_array().map(Vec::len),
                Some(3819)
            );
        }
    }
    assert!(child.wait()?.success());
    eprintln!("CIP digraph: {accepted} accepted, {rejected} rejected, {mismatches} mismatches");
    assert_eq!(mismatches, 0, "{}", failures.join("\n"));
    assert!(accepted > 1_000);
    assert!(benchmark_seen);
    assert_eq!(node_limits, 4);
    Ok(())
}
