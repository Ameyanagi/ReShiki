//! Frozen worker wire responses for the naming-engine move.
use super::{FIXTURES, RULES_CASES, corpus};
use crate::naming::{IsotopeStyle, local, reverse, worker};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, path::Path, time::Instant};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
enum Response {
    Bytes(String),
    Error { error: String },
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    request: worker::Request,
    response: Response,
}

fn respond(request: &[u8]) -> Response {
    match worker::respond(request) {
        // Keep the original bytes; parsing/re-serializing would hide wire changes.
        Ok(bytes) => Response::Bytes(String::from_utf8(bytes).expect("UTF-8 worker response")),
        Err(error) => Response::Error { error },
    }
}

/// The corpora never populate these response fields, so a field reorder in
/// them would pass unnoticed; these inputs from the unit tests fill them.
const EXTRA_SMILES: [&str; 3] = [
    // Radical cation named through its recovered neutral parent (`verification`).
    "[CH2+][CH2]",
    // Unknown-configuration marks (`omitted_stereo`, `omitted_units`).
    "C[CH:900001](O)[C@H](Cl)C",
    "F[CH:900001](Cl)c1ccc([CH:900001](F)Br)cc1",
];
/// Names parsed directly; the corpora generate no atropisomer names (`axes`).
const EXTRA_NAMES: [&str; 1] = ["(1M)-[1,1'-binaphthalene]-2,2'-diol"];

fn inputs() -> Vec<String> {
    let rules = RULES_CASES.lines().skip(1).map(|line| {
        let [smiles, _, _] = line.split('\t').collect::<Vec<_>>()[..] else {
            panic!("Malformed rules case: {line}");
        };
        smiles.to_owned()
    });
    let mut seen = HashSet::new();
    // Preserve first occurrence in bakeoff, openclatura, then rules file order.
    corpus(false)
        .into_iter()
        .map(|case| case.smiles)
        .chain(rules)
        .chain(EXTRA_SMILES.map(str::to_owned))
        .filter(|smiles| seen.insert(smiles.clone()))
        .collect()
}

/// In replay mode `visit` returns the frozen response, so changed generation
/// results cannot remove or replace the Verify requests being checked.
fn visit_requests(mut visit: impl FnMut(Vec<u8>) -> Response) {
    for smiles in inputs() {
        let isotope = smiles
            .as_bytes()
            .windows(2)
            .any(|pair| pair[0] == b'[' && pair[1].is_ascii_digit());
        for style in [IsotopeStyle::Iupac, IsotopeStyle::Boughton] {
            if style == IsotopeStyle::Boughton && !isotope {
                continue;
            }
            let response = visit(
                reverse::generate_request_with_style(&smiles, style)
                    .expect("build app Generate request"),
            );
            let Response::Bytes(bytes) = response else {
                continue;
            };
            let worker::Response::Generate(generated) =
                serde_json::from_str(&bytes).expect("structured Generate response")
            else {
                panic!("Generate returned a different operation");
            };
            if let Ok(name) = generated.result {
                visit(local::verification_request(&name.text).expect("build app Verify request"));
                // Strict import of the same name covers the Parse path too.
                visit(local::parse_request(&name.text).expect("build app Parse request"));
                // The app verifies every alternative it offers (reverse.rs).
                for alternative in &name.alternatives {
                    visit(
                        local::verification_request(&alternative.name)
                            .expect("build app alternative Verify request"),
                    );
                }
                if let Some(verification) = name.verification {
                    visit(
                        local::verification_request(&verification.name)
                            .expect("build app neutral-parent Verify request"),
                    );
                }
            }
        }
    }
}

fn visit_all(mut visit: impl FnMut(Vec<u8>) -> Response) {
    visit_requests(&mut visit);
    for name in EXTRA_NAMES {
        visit(local::verification_request(name).expect("build app Verify request"));
        visit(local::parse_request(name).expect("build app Parse request"));
    }
}

/// Keep every optional response shape populated somewhere in the fixture.
fn assert_shapes(text: &str) {
    for (shape, needle) in [
        ("neutral-parent verification", r#"\"verification\":{"#),
        ("unknown stereo units", r#"\"omitted_stereo\":[\""#),
        ("alternative names", r#"\"alternatives\":[{"#),
        (
            "IR molecular structures",
            r#"\"molecule\":{\"nazuchem_ir\":1"#,
        ),
        ("atropisomer axes", r#"\"kind\":\"atropisomer\""#),
    ] {
        assert!(text.contains(needle), "worker snapshot lacks {shape}");
    }
}

fn update(path: &Path) {
    let mut bytes = Vec::new();
    let mut count = 0;
    visit_all(|request| {
        let response = respond(&request);
        let snapshot = Snapshot {
            request: serde_json::from_slice(&request).expect("structured app request"),
            response: response.clone(),
        };
        serde_json::to_writer(&mut bytes, &snapshot).expect("serialize worker snapshot");
        bytes.push(b'\n');
        count += 1;
        response
    });
    std::fs::write(path, &bytes).expect("write worker_snapshot.jsonl");
    assert_shapes(std::str::from_utf8(&bytes).expect("UTF-8 snapshot"));
    println!("updated {count} worker requests ({} bytes)", bytes.len());
}

fn replay(path: &Path) {
    let text = std::fs::read_to_string(path)
        .expect("read worker_snapshot.jsonl; regenerate with RESHIKI_UPDATE_S2N_SNAPSHOT=1");
    assert_shapes(&text);
    let snapshots: Vec<Snapshot> = text
        .lines()
        .enumerate()
        .map(|(index, line)| {
            serde_json::from_str(line)
                .unwrap_or_else(|error| panic!("worker snapshot line {}: {error}", index + 1))
        })
        .collect();
    let mut index = 0;
    let mut checked = 0;
    let mut differences = 0;
    let mut first = Vec::new();
    visit_all(|request| {
        let snapshot = snapshots
            .get(index)
            .unwrap_or_else(|| panic!("worker snapshot is missing request at line {}", index + 1));
        // Also guard corpus coverage, request order and the current app protocol.
        assert_eq!(
            serde_json::to_vec(&snapshot.request).expect("serialize frozen request"),
            request,
            "worker snapshot request differs from app/corpus at line {}",
            index + 1
        );
        checked += 1;
        let actual = respond(&request);
        if actual != snapshot.response {
            differences += 1;
            if first.len() < 10 {
                first.push(format!(
                    "line {} request: {}\nexpected: {:?}\nactual:   {actual:?}",
                    index + 1,
                    String::from_utf8(request).expect("UTF-8 app request"),
                    snapshot.response,
                ));
            }
        }
        index += 1;
        snapshot.response.clone()
    });
    assert_eq!(index, snapshots.len(), "extra requests in worker snapshot");
    println!(
        "replayed {checked} of {} frozen worker requests",
        snapshots.len()
    );
    assert_eq!(
        differences,
        0,
        "{differences} worker responses differ; first 10 differences:\n{}",
        first.join("\n\n")
    );
}

// RESHIKI_UPDATE_S2N_SNAPSHOT=1 rebuilds the full fixture; review its diff.
#[test]
fn s2n_worker_snapshot() {
    let start = Instant::now();
    let path = Path::new(FIXTURES).join("worker_snapshot.jsonl");
    if std::env::var("RESHIKI_UPDATE_S2N_SNAPSHOT").as_deref() == Ok("1") {
        update(&path);
    } else {
        replay(&path);
    }
    println!("worker snapshot took {:.2?}", start.elapsed());
}
