//! Frozen complete-result differential gate; no live Java or network oracle.
use flate2::read::GzDecoder;
use opsin::{ParseOptions, Parser, Status};
use serde_json::{Value, json};
use std::io::Write;
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader},
};

#[path = "support/structure_snapshots.rs"]
mod snapshots;
use snapshots::{first_difference, graph_value};

#[test]
fn complete_upstream_structures_status_warnings_semantic_cxsmiles_and_graph() {
    let parser = Parser::new().expect("embedded OPSIN resources");
    let reader = BufReader::new(GzDecoder::new(
        include_bytes!("fixtures/structure-golden.jsonl.gz").as_slice(),
    ));
    let selected_family = std::env::var("OPSIN_ORACLE_FAMILY").ok();
    let selected_name = std::env::var("OPSIN_ORACLE_NAME").ok();
    let mut report = std::env::var_os("OPSIN_ORACLE_REPORT")
        .map(|path| std::fs::File::create(path).expect("create optional oracle report"));
    let mut families: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut differences = Vec::new();
    let mut count = 0;
    for line in reader.lines() {
        let expected: Value =
            serde_json::from_str(&line.expect("fixture decompression")).expect("fixture JSON");
        let family = expected["family"].as_str().unwrap();
        let name = expected["input"].as_str().unwrap();
        if selected_family
            .as_deref()
            .is_some_and(|selected| selected != family)
        {
            continue;
        }
        if selected_name
            .as_deref()
            .is_some_and(|selected| selected != name)
        {
            continue;
        }
        let options = ParseOptions {
            allow_radicals: expected["allow_radicals"].as_bool().unwrap(),
            ..ParseOptions::strict()
        };
        let result = parser.parse(name, &options);
        assert_eq!(result.input, name, "input identity");
        let warnings: Vec<Value> = result
            .warnings
            .iter()
            .map(|warning| json!({"kind":warning.kind.as_str(),"message":warning.message}))
            .collect();
        let (cxsmiles, graph) = match &result.structure {
            Some(structure) => (
                structure
                    .semantic_cxsmiles()
                    .map(Value::String)
                    .unwrap_or_else(|error| json!({"serialization_error":error.to_string()})),
                graph_value(structure),
            ),
            None => (Value::Null, Value::Null),
        };
        let actual = json!({
            "status":match result.status { Status::Success=>"SUCCESS",Status::Warning=>"WARNING",Status::Failure=>"FAILURE" },
            "message":result.message, "warnings":warnings, "cxsmiles":cxsmiles, "graph":graph,
        });
        let subset = json!({
            "status":expected["status"], "message":expected["message"], "warnings":expected["warnings"],
            "cxsmiles":expected["cxsmiles"], "graph":expected["graph"],
        });
        let family_counts = families.entry(family.to_owned()).or_default();
        family_counts.1 += 1;
        count += 1;
        let difference = first_difference(&subset, &actual, "result");
        if let Some(report) = report.as_mut() {
            let row = json!({
                "family": family, "input": name, "allow_radicals": options.allow_radicals,
                "expected_status": expected["status"], "actual_status": actual["status"],
                "expected_cxsmiles": expected["cxsmiles"], "actual_cxsmiles": actual["cxsmiles"],
                "expected_message": expected["message"], "actual_message": actual["message"],
                "expected_warnings": expected["warnings"], "actual_warnings": actual["warnings"],
                "difference": difference,
            });
            writeln!(report, "{row}").expect("write optional oracle report");
        }
        if let Some(diff) = difference {
            if differences.len() < 60 {
                differences.push(format!(
                    "{family}: {name} (radicals={}): {diff}",
                    options.allow_radicals
                ));
            }
        } else {
            family_counts.0 += 1;
        }
    }
    assert!(count > 0, "oracle filter selected no fixture records");
    let passed: usize = families.values().map(|counts| counts.0).sum();
    for (family, (passed, total)) in &families {
        eprintln!("{family}: {passed}/{total} exact complete structures");
    }
    assert_eq!(
        passed,
        count,
        "complete structures {passed}/{count}; first mismatches:\n{}",
        differences.join("\n")
    );
    if selected_family.is_none() && selected_name.is_none() {
        assert_eq!(count, 2096);
    }
}
