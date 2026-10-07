//! Byte pins for the in-app Codex contract: the dynamicTools array, the
//! Proposal schema (canvas_preview inputSchema and the turn outputSchema) and
//! the critique schema. See tests/fixtures/agent-contract/README.md.
use reshiki::assistant;
use serde_json::json;

fn golden(name: &str) -> &'static str {
    let text = match name {
        "codex-dynamic-tools.json" => {
            include_str!("fixtures/agent-contract/codex-dynamic-tools.json")
        }
        "proposal-schema.json" => include_str!("fixtures/agent-contract/proposal-schema.json"),
        "critique-schema.json" => include_str!("fixtures/agent-contract/critique-schema.json"),
        _ => panic!("unknown golden {name}"),
    };
    text.strip_suffix('\n')
        .unwrap_or_else(|| panic!("{name} must end in one LF"))
}

#[test]
fn codex_dynamic_tools_bytes_are_pinned() {
    assert_eq!(
        assistant::canvas_tools::definitions().to_string(),
        golden("codex-dynamic-tools.json")
    );
}

#[test]
fn proposal_schema_bytes_are_pinned() {
    assert_eq!(
        assistant::schema().to_string(),
        golden("proposal-schema.json")
    );
}

#[test]
fn critique_schema_bytes_are_pinned() {
    assert_eq!(
        assistant::review::schema().to_string(),
        golden("critique-schema.json")
    );
}

#[test]
fn canvas_tools_keep_plan_inspect_preview_order() {
    let tools = assistant::canvas_tools::definitions();
    let names: Vec<&str> = tools
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["canvas_plan", "canvas_inspect", "canvas_preview"]);
}

#[test]
fn serde_json_object_keys_stay_sorted() {
    // The Codex bytes rely on serde_json without preserve_order (Cargo.lock
    // serde_json 1.0.151 has no indexmap). A dependency enabling
    // serde_json/preserve_order (for example through schemars features pulled
    // by rmcp) would reorder every json! object.
    assert_eq!(json!({"b":1,"a":2}).to_string(), r#"{"a":2,"b":1}"#);
}
