use reshiki::chemistry::{
    RDKIT_VERSION,
    graph::Graph,
    kekulize::{self, Direction, Options},
};
use serde::Deserialize;
use std::{
    error::Error,
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};

#[derive(Deserialize)]
struct Case {
    name: String,
    graph: Graph,
    rings: Vec<Vec<usize>>,
    directions: Vec<Direction>,
    ranks: Option<Vec<u32>>,
    clear: bool,
    expected: Option<serde_json::Value>,
}

#[test]
fn kekule_assignment_matches_independent_rdkit() -> Result<(), Box<dyn Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/kekulize_reference.py"))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().ok_or("Missing oracle output")?).lines();
    let header: serde_json::Value =
        serde_json::from_str(&lines.next().ok_or("Missing oracle version")??)?;
    assert_eq!(header["rdkit_version"], RDKIT_VERSION);
    let (mut count, mut rejected, mut canonical, mut changed) = (0, 0, 0, 0);
    let mut failures = Vec::new();
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        let before = serde_json::to_value(&case.graph)?;
        count += 1;
        rejected += usize::from(case.expected.is_none());
        canonical += usize::from(case.ranks.is_some());
        changed += usize::from(case.expected.as_ref().is_some_and(|e| e["graph"] != before));
        let result = kekulize::assign(
            &case.graph,
            &case.rings,
            &case.directions,
            Options {
                clear_aromaticity: case.clear,
                ranks: case.ranks.as_deref(),
                ..Options::default()
            },
        );
        let matches = match (&result, &case.expected) {
            (Ok(a), Some(e)) => serde_json::to_value(a)? == *e,
            (Err(_), None) => true,
            _ => false,
        };
        if !matches && failures.len() < 20 {
            failures.push(format!("{}: {result:?} != {:?}", case.name, case.expected));
        }
        assert_eq!(serde_json::to_value(&case.graph)?, before, "Mutated input");
    }
    assert!(
        child.wait()?.success(),
        "Oracle failed: {}",
        failures.join("\n")
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(
        count > 50_000 && rejected > 3_000 && canonical > 16_000 && changed > 15_000,
        "Missing coverage: {count}/{rejected}/{canonical}/{changed}"
    );
    eprintln!(
        "Verified {count} Kekulé cases: {rejected} rejected, {canonical} ranked, {changed} transformed"
    );
    Ok(())
}
