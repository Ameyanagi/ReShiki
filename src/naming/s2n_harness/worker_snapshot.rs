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

fn update(path: &Path) {
    let mut bytes = Vec::new();
    let mut count = 0;
    visit_requests(|request| {
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
    println!("updated {count} worker requests ({} bytes)", bytes.len());
}

fn replay(path: &Path) {
    let text = std::fs::read_to_string(path)
        .expect("read worker_snapshot.jsonl; regenerate with RESHIKI_UPDATE_S2N_SNAPSHOT=1");
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
    visit_requests(|request| {
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
