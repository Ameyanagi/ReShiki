use super::*;
use serde_json::json;

#[test]
fn the_agent_reference_is_current() {
    assert_eq!(
        include_str!("../../../docs/agent-reference.md"),
        agent_reference(),
        "run cargo run --example agent_reference -- docs/agent-reference.md"
    );
}

#[test]
fn help_is_verbatim_and_commands_and_tools_keep_catalog_order() {
    let reference = agent_reference();
    assert!(reference.contains(&format!("```text\n{}```", help(None))));
    assert!(reference.contains(&format!("```text\n{}```", *MCP_USAGE)));
    let mut remaining = reference.as_str();
    for spec in COMMANDS {
        let (_, rest) = remaining
            .split_once(&format!("### reshiki --cli {}\n\n", spec.name))
            .expect("command in help order");
        let body = help(Some(spec));
        if body == help(None) {
            assert!(rest.starts_with("Prints the [overview](#overview).\n\n"));
        } else {
            assert!(rest.starts_with(&format!("```text\n{body}```")));
        }
        remaining = rest;
    }
    let catalog = catalog_json();
    for tool in catalog.as_array().unwrap() {
        let (_, rest) = remaining
            .split_once(&format!("### {}\n\n", string(tool, "name")))
            .expect("tool in catalog order");
        remaining = rest;
    }
}

#[test]
fn parameters_render_nullable_types_and_constraints_from_schema() {
    let schema = json!({
        "properties": {
            "choice": {"anyOf": [{"type": "null"}, {
                "type": "string", "enum": ["a", "b"], "maxLength": 4,
                "pattern": "^(a|b)$"
            }]},
            "ids": {"type": "array", "maxItems": 2,
                "items": {"type": "integer", "minimum": 1, "maximum": 9}},
            "optional": {"type": "boolean", "description": "One | two\nlines"}
        },
        "required": ["choice", "ids"]
    });
    let mut text = String::new();
    parameters(&mut text, &schema);
    let choice = "| `choice` | `null or string` | true | true | enum: `[\"a\",\"b\"]`; ";
    assert!(text.contains(choice));
    assert!(text.contains("maxLength: `4`; pattern: `\"^(a\\|b)$\"`"));
    let ids = "| `ids` | `array<integer>` | true | false | maxItems: `2`; ";
    assert!(text.contains(ids));
    assert!(text.contains("items.minimum: `1`; items.maximum: `9`"));
    assert!(text.contains("| `optional` | `boolean` | false | false |  | One \\| two lines |"));
}

#[test]
fn parameters_without_descriptions_omit_the_column_and_link_the_proposal() {
    let schema = json!({
        "properties": {"proposal": {"type": "object"}},
        "required": ["proposal"]
    });
    let mut text = String::new();
    parameters(&mut text, &schema);
    assert!(text.starts_with("| Parameter | Type | Required | Nullable | Constraints |\n"));
    assert!(text.contains("| `proposal` | `object` ([Proposal](#proposal)) | true | false |  |\n"));
}

#[test]
fn results_start_with_code_formatted_top_level_keys() {
    let reference = agent_reference();
    assert!(reference.contains(&format!("## Tools\n\n{RESULT_CONVENTIONS}\n\n")));
    assert!(reference.contains("Result: `{document, closed, versions}`. The closed"));
}

#[test]
fn budget_units_cover_the_serialized_limits() {
    let mut text = String::new();
    budget_rows(&mut text, "", &budgets_json(&Budgets::default()));
    assert!(text.contains("| `max_text_bytes` | 16777216 | bytes |"));
    assert!(text.contains("| `max_cdx_base64` | 22369624 | bytes |"));
    assert!(text.contains("| `idle_ttl_seconds` | 3600 | seconds |"));
    assert!(text.contains("| `render.default_width` | 1600 | pixels |"));
    assert!(text.contains("| `export_png_pixels` | 16000000 | pixels |"));
    assert!(text.contains("| `max_documents` | 16 | count |"));
    assert!(!agent_reference().contains("| `app` |"));
}
