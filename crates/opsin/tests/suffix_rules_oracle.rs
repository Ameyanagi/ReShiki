//! Complete suffix applicability selection and ordered metadata from pinned OPSIN.
use opsin::suffix_rules::SuffixRules;
use serde_json::{Value, json};

#[test]
fn all_pinned_suffix_rule_queries_match_ordered_metadata_and_errors() {
    let rules = SuffixRules::new().unwrap();
    let mut count = 0;
    for line in include_str!("fixtures/suffix-rule-golden.jsonl").lines() {
        let expected: Value = serde_json::from_str(line).unwrap();
        let group_type = expected["group_type"].as_str().unwrap();
        let suffix = expected["suffix"].as_str().unwrap();
        let subtype = expected["sub_type"].as_str();
        let (selected, error) = match rules.rule_tags(group_type, suffix, subtype) {
            Ok(selected) => (
                json!(
                    selected
                        .iter()
                        .map(|rule| json!({"kind":rule.kind.as_str(),"attributes":rule.attributes}))
                        .collect::<Vec<_>>()
                ),
                Value::Null,
            ),
            Err(error) => (Value::Null, json!(error.to_string())),
        };
        assert_eq!(
            rules.is_group_type_with_specific_suffix_rules(group_type),
            expected["specific_group_type"].as_bool().unwrap(),
            "{group_type}"
        );
        assert_eq!(
            selected, expected["rules"],
            "{group_type} {suffix} {subtype:?}"
        );
        assert_eq!(
            error, expected["error"],
            "{group_type} {suffix} {subtype:?}"
        );
        count += 1;
    }
    assert_eq!(count, 997);
}
