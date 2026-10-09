//! Exact pinned configuration behavior across all five flags and their interactions.
use flate2::read::GzDecoder;
use opsin::{ParseOptions, Parser, Status};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader};
#[path = "support/structure_snapshots.rs"]
mod snapshots;
use snapshots::{first_difference, graph_value};

#[test]
fn all_configuration_flags_match_pinned_complete_results() {
    let parser = Parser::new().unwrap();
    let reader = BufReader::new(GzDecoder::new(
        include_bytes!("fixtures/configuration-golden.jsonl.gz").as_slice(),
    ));
    let selected_name = std::env::var("OPSIN_CONFIG_NAME").ok();
    let selected_mask = std::env::var("OPSIN_CONFIG_MASK")
        .ok()
        .map(|value| value.parse::<u32>().unwrap());
    let mut differences = Vec::new();
    let mut count = 0;
    let mut passed = 0;
    for line in reader.lines() {
        let expected: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let name = expected["input"].as_str().unwrap();
        let mask = expected["mask"].as_u64().unwrap() as u32;
        if selected_name
            .as_deref()
            .is_some_and(|selected| selected != name)
        {
            continue;
        }
        if selected_mask.is_some_and(|selected| selected != mask) {
            continue;
        }
        let options = ParseOptions {
            allow_radicals: expected["options"]["allow_radicals"].as_bool().unwrap(),
            output_radicals_as_wildcard_atoms:
                expected["options"]["output_radicals_as_wildcard_atoms"]
                    .as_bool()
                    .unwrap(),
            detailed_failure_analysis: expected["options"]["detailed_failure_analysis"]
                .as_bool()
                .unwrap(),
            interpret_acids_without_the_word_acid:
                expected["options"]["interpret_acids_without_the_word_acid"]
                    .as_bool()
                    .unwrap(),
            warn_rather_than_fail_on_uninterpretable_stereochemistry:
                expected["options"]["warn_rather_than_fail_on_uninterpretable_stereochemistry"]
                    .as_bool()
                    .unwrap(),
        };
        let result = parser.parse(name, &options);
        assert_eq!(result.input, name);
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
        let actual = json!({"status":match result.status{Status::Success=>"SUCCESS",Status::Warning=>"WARNING",Status::Failure=>"FAILURE"},"message":result.message,"warnings":warnings,"cxsmiles":cxsmiles,"graph":graph});
        let subset = json!({"status":expected["status"],"message":expected["message"],"warnings":expected["warnings"],"cxsmiles":expected["cxsmiles"],"graph":expected["graph"]});
        count += 1;
        if let Some(difference) = first_difference(&subset, &actual, "result") {
            if differences.len() < 50 {
                differences.push(format!("{name} mask={mask}: {difference}"));
            }
        } else {
            passed += 1;
        }
    }
    assert!(count > 0, "configuration filter selected no records");
    assert_eq!(
        passed,
        count,
        "configuration complete results {passed}/{count}:\n{}",
        differences.join("\n")
    );
    if selected_name.is_none() && selected_mask.is_none() {
        assert_eq!(count, 640);
    }
}
