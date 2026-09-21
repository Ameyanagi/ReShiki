use reshiki::chemistry::{
    RDKIT_VERSION,
    graph::Graph,
    kekulize::{self, Direction},
    ranking::{self, Metadata, Options},
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
    metadata: Metadata,
    rings: Vec<Vec<usize>>,
    options: Options,
    expected: Vec<u32>,
    directions: Vec<Direction>,
    kekule: serde_json::Value,
}

#[test]
fn canonical_ranking_and_ranked_kekule_match_rdkit() -> Result<(), Box<dyn Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/ranking_reference.py"))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().ok_or("Missing oracle stdout")?).lines();
    let version: serde_json::Value =
        serde_json::from_str(&lines.next().ok_or("Missing oracle version")??)?;
    assert_eq!(version["rdkit_version"], RDKIT_VERSION);
    let (mut count, mut stereo, mut ranked, mut failure_count) = (0, 0, 0, 0);
    let (mut ring_stereo, mut groups, mut atrop, mut relative) = (0, 0, 0, 0);
    let mut failures = Vec::new();
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        count += 1;
        ring_stereo += usize::from(case.metadata.atoms.iter().any(|a| a.ring_stereo));
        groups += usize::from(!case.metadata.groups.is_empty());
        atrop += usize::from(
            case.metadata
                .bonds
                .iter()
                .any(|b| matches!(b.stereo, 6 | 7)),
        );
        relative += usize::from(
            case.metadata
                .bonds
                .iter()
                .any(|b| matches!(b.stereo, 4 | 5)),
        );
        stereo += usize::from(
            case.metadata.atoms.iter().any(|a| a.chiral_tag != 0)
                || case.metadata.bonds.iter().any(|b| b.stereo != 0),
        );
        let before = serde_json::to_value(&case.graph)?;
        let result = ranking::rank(&case.graph, &case.rings, &case.metadata, case.options);
        if result.as_ref().ok() != Some(&case.expected) {
            failure_count += 1;
            if failures.len() < 30 {
                failures.push(format!(
                    "{} {:?}: {result:?} != {:?}",
                    case.name, case.options, case.expected
                ));
            }
        } else if !case.kekule.is_null() {
            ranked += 1;
            let ranks = result?;
            let assignment = kekulize::assign(
                &case.graph,
                &case.rings,
                &case.directions,
                kekulize::Options {
                    ranks: Some(&ranks),
                    ..Default::default()
                },
            );
            let matches = match assignment {
                Ok(a) => serde_json::to_value(a)? == case.kekule,
                Err(_) => case.kekule == false,
            };
            if !matches {
                failure_count += 1;
                if failures.len() < 30 {
                    failures.push(format!("{}: ranked assignment mismatch", case.name));
                }
            }
        }
        assert_eq!(serde_json::to_value(&case.graph)?, before);
    }
    assert!(
        child.wait()?.success(),
        "Oracle failed: {}",
        failures.join("\n")
    );
    assert_eq!(failure_count, 0, "{}", failures.join("\n"));
    assert!(
        count > 45_000 && stereo > 1_000 && ranked > 10_000,
        "Missing coverage {count}/{stereo}/{ranked}"
    );
    assert!(
        ring_stereo >= 100 && groups >= 20 && atrop >= 20 && relative >= 10,
        "Missing stereo coverage {ring_stereo}/{groups}/{atrop}/{relative}"
    );
    eprintln!(
        "Verified {count} ranking cases, {stereo} with stereo and {ranked} ranked assignments"
    );
    Ok(())
}
