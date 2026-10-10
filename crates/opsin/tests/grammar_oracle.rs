//! Frozen development oracle output, not a runtime Java dependency.
use opsin::{Parser, parse_rules::ParseTokens};
use serde_json::{Value, json};

fn alternatives(tokens: &[ParseTokens]) -> Value {
    json!(
        tokens
            .iter()
            .map(|alternative| json!({
                "tokens": alternative.tokens.iter().map(|t| &t.text).collect::<Vec<_>>(),
                "annotations": alternative.tokens.iter().map(|t| t.annotation).collect::<Vec<_>>(),
            }))
            .collect::<Vec<_>>()
    )
}

#[test]
fn native_annotation_and_tokenization_match_pinned_opsin() {
    let parser = Parser::new().expect("pinned OPSIN resources initialize");
    let mut failures = Vec::new();
    let mut cases = 0;
    for line in include_str!("fixtures/frontend-golden.jsonl").lines() {
        let expected: Value = serde_json::from_str(line).expect("valid frozen oracle row");
        let input = expected["input"].as_str().unwrap();
        let mode = expected["mode"].as_str().unwrap();
        let mut actual = json!({ "input": input, "mode": mode });
        let normalized = opsin::preprocess::preprocess(input).expect("corpus names preprocess");
        actual["normalized"] = json!(normalized);
        if mode.starts_with("parse") {
            let result = if mode == "parse" {
                parser.parse_word(&normalized)
            } else {
                parser.parse_word_reverse(&normalized)
            };
            match result {
                Ok(result) => {
                    actual["alternatives"] = alternatives(&result.parses);
                    actual["uninterpretable"] = json!(result.uninterpretable_name);
                    actual["unparseable"] = json!(result.unparseable_name);
                    actual["error"] = Value::Null;
                }
                Err(error) => actual["error"] = json!(error.to_string()),
            }
        } else {
            let remove_space = !mode.ends_with("NoSpace");
            let result = if mode.starts_with("tokenizeReverse") {
                parser.tokenize_reverse(&normalized, remove_space)
            } else {
                parser.tokenize(&normalized, remove_space)
            };
            match result {
                Ok(result) => {
                    actual["unparsed"] = json!(result.unparsed_name);
                    actual["uninterpretable"] = json!(result.uninterpretable_name);
                    actual["unparseable"] = json!(result.unparseable_name);
                    actual["words"] =
                        json!(result.words.iter().map(|word| json!({
                        "word": word.word, "alternatives": alternatives(&word.alternatives),
                    })).collect::<Vec<_>>());
                    actual["error"] = Value::Null;
                }
                Err(error) => actual["error"] = json!(error.to_string()),
            }
        }
        if actual != expected && failures.len() < 12 {
            failures.push(format!(
                "{input:?} ({mode})\nexpected: {expected}\nactual:   {actual}"
            ));
        }
        cases += 1;
    }
    assert_eq!(cases, 6_438, "all 1,073 names in all six oracle modes");
    assert!(
        failures.is_empty(),
        "Native frontend differences:\n{}",
        failures.join("\n\n")
    );
}

#[test]
fn resource_token_metadata_and_case_sensitivity_are_preserved() {
    let parser = Parser::new().unwrap();
    let result = parser.parse_word("ethane").unwrap();
    let eth = &result.parses[0].tokens[0];
    assert_eq!(eth.tag_name, "group");
    assert_eq!(eth.attributes["value"], "CC");
    assert_eq!(eth.attributes["labels"], "numeric");
    assert_eq!(eth.attributes["usableAsAJoiner"], "yes");
    assert_eq!(eth.attribute_order[0], ("type".into(), "chain".into()));
    assert_eq!(
        eth.attribute_order[1],
        ("subType".into(), "alkaneStem".into())
    );
    assert_eq!(eth.attribute_order[2], ("value".into(), "CC".into()));
    let result = parser.parse_word("N,N-dimethylacetamide").unwrap();
    assert!(result.parses.iter().all(|p| p.tokens[0].text == "N,N-"));
}
