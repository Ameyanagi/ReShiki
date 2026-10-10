//! Frozen Parser + SortParses output from the pinned upstream implementation.
//! Test execution, decompression and parsing are entirely native Rust.
use flate2::read::GzDecoder;
use opsin::{ParseOptions, Parser, parse_tree::sort_parses};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader};

#[test]
fn complete_parser_and_parse_ranking_match_pinned_opsin() {
    let parser = Parser::new().unwrap();
    let fixtures = GzDecoder::new(include_bytes!("fixtures/parse-tree-golden.jsonl.gz").as_slice());
    let mut failures = Vec::new();
    let mut failure_count = 0;
    let mut cases = 0;
    for line in BufReader::new(fixtures).lines() {
        let expected: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let input = expected["input"].as_str().unwrap();
        let options = ParseOptions {
            allow_radicals: expected["allow_radicals"].as_bool().unwrap(),
            detailed_failure_analysis: expected["detailed_failure_analysis"].as_bool().unwrap(),
            ..Default::default()
        };
        let mut actual = expected.clone();
        actual["normalized"] = json!(opsin::preprocess::preprocess(input).unwrap());
        match parser.parse_trees(input, &options) {
            Ok(mut trees) => {
                actual["error"] = Value::Null;
                actual["raw_trees"] =
                    json!(trees.iter().map(|tree| tree.to_xml()).collect::<Vec<_>>());
                sort_parses(&mut trees);
                actual["ranked_trees"] =
                    json!(trees.iter().map(|tree| tree.to_xml()).collect::<Vec<_>>());
            }
            Err(error) => {
                actual["error"] = json!(error.to_string());
                actual.as_object_mut().unwrap().remove("raw_trees");
                actual.as_object_mut().unwrap().remove("ranked_trees");
            }
        }
        if actual != expected {
            failure_count += 1;
            if failures.len() < 8 {
                let mut difference = format!("{input:?} ({options:?})");
                for field in ["normalized", "error", "raw_trees", "ranked_trees"] {
                    if actual[field] == expected[field] {
                        continue;
                    }
                    if let (Some(actual), Some(expected)) =
                        (actual[field].as_array(), expected[field].as_array())
                    {
                        difference.push_str(&format!(
                            "\n{field}: actual {} trees, expected {}",
                            actual.len(),
                            expected.len()
                        ));
                        if let Some((index, (actual, expected))) = actual
                            .iter()
                            .zip(expected)
                            .enumerate()
                            .find(|(_, (a, b))| a != b)
                        {
                            let actual = actual.as_str().unwrap();
                            let expected = expected.as_str().unwrap();
                            let actual_lines: Vec<_> = actual.lines().collect();
                            let expected_lines: Vec<_> = expected.lines().collect();
                            let first = actual_lines
                                .iter()
                                .zip(&expected_lines)
                                .position(|(a, b)| a != b)
                                .unwrap_or(actual_lines.len().min(expected_lines.len()));
                            difference.push_str(&format!("\ntree {index}, first differing line {first}:\nexpected {:?}\nactual {:?}", expected_lines.get(first), actual_lines.get(first)));
                        }
                    } else {
                        let describe = |value: &Value| {
                            value.as_array().map_or_else(
                                || value.to_string(),
                                |array| format!("{} trees", array.len()),
                            )
                        };
                        difference.push_str(&format!(
                            "\n{field}: expected {}, actual {}",
                            describe(&expected[field]),
                            describe(&actual[field])
                        ));
                    }
                }
                failures.push(difference);
            }
        }
        cases += 1;
    }
    assert_eq!(cases, 4_320);
    assert_eq!(
        failure_count,
        0,
        "Native Parser differences ({failure_count}/{cases}):\n{}",
        failures.join("\n\n")
    );
}
