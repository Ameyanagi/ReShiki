use super::*;
use serde_json::json;

fn documented_app_version(version: &str) -> &str {
    version
        .split_once("-nightly.")
        .map_or(version, |(base, _)| base)
}

#[test]
fn documented_app_version_strips_only_the_nightly_suffix() {
    assert_eq!(
        documented_app_version("0.14.0-nightly.20261108.42529939246.1"),
        "0.14.0"
    );
    assert_eq!(documented_app_version("0.14.0"), "0.14.0");
    assert_eq!(documented_app_version("0.15.0-rc.1"), "0.15.0-rc.1");
}

#[test]
fn the_agent_reference_is_current() {
    assert_eq!(
        include_str!("../../../docs/agent-reference.md"),
        canonical_agent_reference(),
        "run cargo run --example agent_reference -- docs/agent-reference.md"
    );
}

#[test]
fn documented_and_compiled_commands_match_the_build() {
    let documented: Vec<_> = DOCUMENTED_COMMANDS
        .iter()
        .filter(|spec| cfg!(feature = "xtb") || !matches!(spec.name, "calc" | "calculate" | "ts"))
        .map(|spec| spec.name)
        .collect();
    let compiled: Vec<_> = COMMANDS.iter().map(|spec| spec.name).collect();
    assert_eq!(compiled, documented);
}

#[test]
fn the_agent_api_versions_are_current() {
    let table = include_str!("../../../docs/agent-api.md")
        .split_once("## Versioning\n")
        .unwrap()
        .1;
    for (field, version) in [
        (
            "app",
            documented_app_version(env!("CARGO_PKG_VERSION")).to_string(),
        ),
        ("document", crate::document::VERSION.to_string()),
    ] {
        let row = table
            .lines()
            .find(|line| line.starts_with(&format!("| `{field}` ")))
            .unwrap();
        assert_eq!(row.split('|').nth(2).unwrap().trim(), version, "{field}");
    }
}

#[test]
fn help_is_verbatim_and_commands_and_tools_keep_catalog_order() {
    let reference = agent_reference();
    let headings: Vec<_> = reference
        .lines()
        .filter_map(|line| line.strip_prefix("### reshiki --cli "))
        .collect();
    assert_eq!(
        headings,
        COMMANDS.iter().map(|spec| spec.name).collect::<Vec<_>>()
    );
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
            "fixed": {"type": ["array", "null"], "items": {"type": "integer", "minimum": 1}},
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
    assert!(
        text.contains("| `fixed` | `array<integer> or null` | false | true | items.minimum: `1` |")
    );
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
    assert!(reference.contains("Result `value`: `{document, closed}`. The closed"));
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

/// The CLI guide keeps every usage and points to the worker guide for calc options.
#[test]
fn the_cli_guide_states_every_usage_and_option() {
    let guide = include_str!("../../../docs/agent-cli.md");
    let (_, worker_cli) = include_str!("../../../docs/xtb-worker.md")
        .split_once("## Command line\n")
        .unwrap();
    let folded = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
    let guide_folded = folded(guide);
    let mut missing = Vec::new();
    for spec in DOCUMENTED_COMMANDS {
        if !guide_folded.contains(&folded(spec.usage)) {
            missing.push(format!("usage: {}", folded(spec.usage)));
        }
        if spec.name == "calc" {
            assert!(guide.contains("(xtb-worker.md#command-line)"));
            for option in spec.options {
                assert!(
                    worker_cli.contains(option.flag),
                    "missing calc option {}",
                    option.flag
                );
            }
            continue;
        }
        for option in spec.options {
            let cell = match option.value {
                Some(value) => format!("| `{} {value}` ", option.flag),
                None => format!("| `{}` ", option.flag),
            };
            let listed = guide.lines().any(|line| {
                line.starts_with(&cell)
                    && line.split('|').nth(2).is_some_and(|commands| {
                        commands.split(',').any(|name| name.trim() == spec.name)
                    })
            });
            if !listed {
                missing.push(format!("option row: {cell}| … {} … |", spec.name));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "docs/agent-cli.md lacks:\n{}",
        missing.join("\n")
    );
}
