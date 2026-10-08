//! Schema and decoder parity for the agent's JSON contracts. Each contract's
//! cases file under tests/fixtures/agent-contract inventories the decoder's
//! rules with cited sources and pins how the hand-written JSON Schema and the
//! Rust decoder classify boundary values. See README.md there.
mod support;

use reshiki_agent::{
    Proposal, canvas_tools,
    ops::catalog::{SPECS, catalog_json, decode_check},
    tool_spec::valid_name,
};
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashSet};
use support::{
    corpus::{self, Contract},
    json_schema,
};

/// The Proposal decoder exactly as `canvas_preview` (canvas_tools.rs:101-103)
/// and the Codex Proposal turn (src/assistant/codex.rs:476-477) run it.
fn decode_proposal(value: Value) -> Result<(), String> {
    serde_json::from_value::<Proposal>(value)
        .map_err(|e| e.to_string())
        .and_then(|proposal| proposal.validate())
}

const PROPOSAL: Contract = Contract {
    name: "proposal",
    schema: reshiki_agent::schema,
    decode: decode_proposal,
    cases: "proposal-cases.json",
};

#[test]
fn proposal_schema_and_decoder_parity() {
    match corpus::check(&PROPOSAL) {
        Ok(report) => eprintln!("{report}"),
        Err(failures) => panic!("{failures}"),
    }
}

/// One schema for several operation tools. A case value is
/// `{"tool": name, "arguments": {...}}`; the branch for `name` checks the
/// arguments against that tool's inputSchema.
fn tools_schema(names: &[&str]) -> Value {
    let branches: Vec<Value> = names
        .iter()
        .map(|name| {
            let spec = SPECS
                .iter()
                .find(|spec| spec.name == *name)
                .unwrap_or_else(|| panic!("no operation tool {name}"));
            json!({
                "type": "object",
                "properties": {"tool": {"const": name}, "arguments": (spec.input_schema)()},
                "required": ["tool", "arguments"],
                "additionalProperties": false,
            })
        })
        .collect();
    json!({"anyOf": branches})
}

/// The operation decoder, `catalog::decode_check`, on a
/// `{"tool": name, "arguments": {...}}` case value.
fn decode_tool_call(value: Value) -> Result<(), String> {
    let malformed = || "A case value is {\"tool\": name, \"arguments\": {...}}".to_string();
    let Value::Object(mut call) = value else {
        return Err(malformed());
    };
    match (call.remove("tool"), call.remove("arguments")) {
        (Some(Value::String(tool)), Some(arguments)) if call.is_empty() => {
            decode_check(&tool, arguments)
        }
        _ => Err(malformed()),
    }
}

const DOCUMENTS: Contract = Contract {
    name: "ops-documents",
    schema: || tools_schema(&["info", "document_new", "document_list", "document_close"]),
    decode: decode_tool_call,
    cases: "ops-documents-cases.json",
};

#[test]
fn ops_documents_schema_and_decoder_parity() {
    match corpus::check(&DOCUMENTS) {
        Ok(report) => eprintln!("{report}"),
        Err(failures) => panic!("{failures}"),
    }
}

const IMPORT: Contract = Contract {
    name: "ops-import",
    schema: || tools_schema(&["import"]),
    decode: decode_tool_call,
    cases: "ops-import-cases.json",
};

#[test]
fn ops_import_schema_and_decoder_parity() {
    match corpus::check(&IMPORT) {
        Ok(report) => eprintln!("{report}"),
        Err(failures) => panic!("{failures}"),
    }
}

const INSPECT: Contract = Contract {
    name: "ops-inspect",
    schema: || tools_schema(&["inspect"]),
    decode: decode_tool_call,
    cases: "ops-inspect-cases.json",
};

#[test]
fn ops_inspect_schema_and_decoder_parity() {
    match corpus::check(&INSPECT) {
        Ok(report) => eprintln!("{report}"),
        Err(failures) => panic!("{failures}"),
    }
}

const ANALYZE: Contract = Contract {
    name: "ops-analyze",
    schema: || tools_schema(&["analyze"]),
    decode: decode_tool_call,
    cases: "ops-analyze-cases.json",
};

#[test]
fn ops_analyze_schema_and_decoder_parity() {
    match corpus::check(&ANALYZE) {
        Ok(report) => eprintln!("{report}"),
        Err(failures) => panic!("{failures}"),
    }
}

const RENDER: Contract = Contract {
    name: "ops-render",
    schema: || tools_schema(&["render"]),
    decode: decode_tool_call,
    cases: "ops-render-cases.json",
};

#[test]
fn ops_render_schema_and_decoder_parity() {
    match corpus::check(&RENDER) {
        Ok(report) => eprintln!("{report}"),
        Err(failures) => panic!("{failures}"),
    }
}

const EXPORT: Contract = Contract {
    name: "ops-export",
    schema: || tools_schema(&["export"]),
    decode: decode_tool_call,
    cases: "ops-export-cases.json",
};

#[test]
fn ops_export_schema_and_decoder_parity() {
    match corpus::check(&EXPORT) {
        Ok(report) => eprintln!("{report}"),
        Err(failures) => panic!("{failures}"),
    }
}

const FILES: Contract = Contract {
    name: "ops-files",
    schema: || tools_schema(&["file_open", "file_save"]),
    decode: decode_tool_call,
    cases: "ops-files-cases.json",
};

#[test]
fn ops_files_schema_and_decoder_parity() {
    match corpus::check(&FILES) {
        Ok(report) => eprintln!("{report}"),
        Err(failures) => panic!("{failures}"),
    }
}

const COMPOSE: Contract = Contract {
    name: "ops-compose",
    schema: || tools_schema(&["compose"]),
    decode: decode_tool_call,
    cases: "ops-compose-cases.json",
};

#[test]
fn ops_compose_schema_and_decoder_parity() {
    match corpus::check(&COMPOSE) {
        Ok(report) => eprintln!("{report}"),
        Err(failures) => panic!("{failures}"),
    }
}

const APPLY: Contract = Contract {
    name: "ops-apply",
    schema: || tools_schema(&["apply"]),
    decode: decode_tool_call,
    cases: "ops-apply-cases.json",
};

#[test]
fn ops_apply_schema_and_decoder_parity() {
    match corpus::check(&APPLY) {
        Ok(report) => eprintln!("{report}"),
        Err(failures) => panic!("{failures}"),
    }
}

/// compose nests the Proposal schema verbatim: the bytes the assistant's
/// `canvas_preview` publishes, pinned by the ops-1 golden.
#[test]
fn compose_nests_the_pinned_proposal_schema() {
    let golden = include_str!("../../../tests/fixtures/agent-contract/proposal-schema.json")
        .strip_suffix('\n')
        .expect("proposal-schema.json ends in one LF");
    let spec = SPECS
        .iter()
        .find(|spec| spec.name == "compose")
        .expect("compose is in the catalog");
    assert_eq!(
        (spec.input_schema)()["properties"]["proposal"].to_string(),
        golden
    );
}

/// The operation catalog as hosts list it. See
/// tests/fixtures/agent-contract/README.md for when the golden may change.
#[test]
fn the_operation_catalog_bytes_are_pinned() {
    let golden = include_str!("../../../tests/fixtures/agent-contract/ops-catalog.json")
        .strip_suffix('\n')
        .expect("ops-catalog.json ends in one LF");
    assert_eq!(catalog_json().to_string(), golden);
}

/// The pointers below `at` of every object-level subschema of `schema` that
/// is not strict: `additionalProperties: false` with every property in
/// `required`. Subtrees equal to `skip` are not walked; their pointers go to
/// `skipped`.
fn loose_objects(
    schema: &Value,
    at: &str,
    skip: &Value,
    loose: &mut Vec<String>,
    skipped: &mut Vec<String>,
) {
    if schema == skip {
        skipped.push(at.to_owned());
        return;
    }
    let Some(schema) = schema.as_object() else {
        return;
    };
    let properties = schema.get("properties").and_then(Value::as_object);
    // `type` may be one name or, as the keyword allowlist permits, an array.
    let object_type = schema.get("type").is_some_and(|t| {
        t == "object"
            || t.as_array()
                .is_some_and(|types| types.iter().any(|t| t == "object"))
    });
    if object_type || properties.is_some() {
        let listed: BTreeSet<&str> = properties
            .into_iter()
            .flat_map(|properties| properties.keys().map(String::as_str))
            .collect();
        let required: BTreeSet<&str> = schema
            .get("required")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        if schema.get("additionalProperties") != Some(&Value::Bool(false)) || listed != required {
            loose.push(at.to_owned());
        }
    }
    for (name, sub) in properties.into_iter().flatten() {
        loose_objects(
            sub,
            &format!("{at}/properties/{name}"),
            skip,
            loose,
            skipped,
        );
    }
    if let Some(items) = schema.get("items") {
        loose_objects(items, &format!("{at}/items"), skip, loose, skipped);
    }
    let branches = schema.get("anyOf").and_then(Value::as_array);
    for (i, branch) in branches.into_iter().flatten().enumerate() {
        loose_objects(branch, &format!("{at}/anyOf/{i}"), skip, loose, skipped);
    }
}

/// `loose_objects` finds object levels by a `type` name, a `type` array or
/// `properties`, through properties, items and anyOf, and skips `skip`.
#[test]
fn loose_objects_finds_every_non_strict_object_level() {
    let strict =
        json!({"type": "object", "properties": {}, "required": [], "additionalProperties": false});
    let schema = json!({
        "type": "object",
        "properties": {
            "nullable": {"type": ["object", "null"]},
            "untyped": {"properties": {"a": {"type": "string"}}, "additionalProperties": false},
            "list": {"type": "array", "items": {"anyOf": [{"type": "null"}, {"type": "object"}]}},
            "strict": {"type": ["null", "object"], "properties": {"a": {"type": "string"}}, "required": ["a"], "additionalProperties": false},
            "verbatim": strict.clone(),
        },
        "required": ["nullable", "untyped", "list", "strict", "verbatim"],
        "additionalProperties": false,
    });
    let (mut loose, mut skipped) = (Vec::new(), Vec::new());
    loose_objects(&schema, "", &strict, &mut loose, &mut skipped);
    loose.sort();
    assert_eq!(
        loose,
        [
            "/properties/list/items/anyOf/1",
            "/properties/nullable",
            "/properties/untyped",
        ]
    );
    assert_eq!(skipped, ["/properties/verbatim"]);
}

/// The catalog against the tool rules of both supported MCP revisions,
/// <https://modelcontextprotocol.io/specification/2026-07-28/server/tools>
/// and <https://modelcontextprotocol.io/specification/2025-11-25/server/tools>,
/// which state the same rules:
///
/// - names are 1 to 128 characters of `[A-Za-z0-9_.-]` and unique; ReShiki
///   also keeps them apart from the in-app canvas tools;
/// - inputSchema is a JSON Schema object with root type "object", read as
///   2020-12 without `$schema`;
/// - every annotation hint is explicit, because the spec defaults
///   destructiveHint and openWorldHint to true.
///
/// ReShiki's own rules: every schema uses only the contract harness's
/// keywords and pattern table, and every object level is strict, except the
/// verbatim Proposal subtree, which proposal-schema.json pins.
#[test]
fn the_operation_catalog_follows_the_mcp_tool_rules() {
    let proposal = reshiki_agent::schema();
    let mut names = HashSet::new();
    for spec in SPECS {
        let name = spec.name;
        assert!(valid_name(name), "{name}");
        assert!(names.insert(name), "duplicate tool {name}");
        assert!(
            canvas_tools::SPECS.iter().all(|canvas| canvas.name != name),
            "{name} is also a canvas tool"
        );
        assert!(spec.hints.is_some(), "{name} has no hints");
        let schema = (spec.input_schema)();
        assert_eq!(schema["type"], "object", "{name}");
        assert!(schema.get("$schema").is_none(), "{name}");
        assert_eq!(json_schema::check_keywords(&schema), [], "{name}");
        let (mut loose, mut skipped) = (Vec::new(), Vec::new());
        loose_objects(&schema, "", &proposal, &mut loose, &mut skipped);
        assert_eq!(loose, Vec::<String>::new(), "{name}: not strict");
        let verbatim: &[&str] = if name == "compose" {
            &["/properties/proposal"]
        } else {
            &[]
        };
        assert_eq!(skipped, verbatim, "{name}");
    }
    assert_eq!(catalog_json().as_array().map(Vec::len), Some(SPECS.len()));
}

/// What a tool does to state, read against the MCP ToolAnnotations
/// (<https://raw.githubusercontent.com/modelcontextprotocol/modelcontextprotocol/main/schema/2026-07-28/schema.ts>):
/// the defaults are readOnlyHint false, destructiveHint true,
/// idempotentHint false and openWorldHint true, and destructiveHint false
/// means the tool performs only additive updates.
#[derive(Clone, Copy, Debug)]
enum Effect {
    /// Changes nothing.
    ReadOnly,
    /// Only adds: a new document.
    Additive,
    /// Changes, replaces or removes something that exists.
    Destructive,
}

/// Every operation tool's effect class.
const EFFECTS: [(&str, Effect); 13] = [
    ("info", Effect::ReadOnly),
    ("document_list", Effect::ReadOnly),
    ("inspect", Effect::ReadOnly),
    ("analyze", Effect::ReadOnly),
    ("render", Effect::ReadOnly),
    ("export", Effect::ReadOnly),
    ("document_new", Effect::Additive),
    ("import", Effect::Additive),
    ("compose", Effect::Additive),
    ("file_open", Effect::Additive),
    ("document_close", Effect::Destructive),
    ("apply", Effect::Destructive),
    ("file_save", Effect::Destructive),
];

/// The ToolSpec hints, which every MCP tools/list renders, tell the truth
/// about each tool's effect; no tool reaches beyond the local machine. A
/// tool missing from [`EFFECTS`] fails.
#[test]
fn hints_follow_effects() {
    for spec in SPECS {
        let name = spec.name;
        let (_, effect) = EFFECTS
            .iter()
            .find(|(tool, _)| *tool == name)
            .unwrap_or_else(|| panic!("{name} has no effect class"));
        let hints = spec.hints.unwrap_or_else(|| panic!("{name} has no hints"));
        match effect {
            Effect::ReadOnly => assert!(hints.read_only, "{name}"),
            Effect::Additive => assert!(!hints.read_only && !hints.destructive, "{name}"),
            Effect::Destructive => assert!(!hints.read_only && hints.destructive, "{name}"),
        }
        assert!(!hints.open_world, "{name}");
    }
    for (tool, _) in EFFECTS {
        assert!(SPECS.iter().any(|spec| spec.name == tool), "{tool}");
    }
    // Every hint is explicit, so no MCP default applies.
    let catalog = catalog_json();
    for tool in catalog.as_array().into_iter().flatten() {
        let annotations = tool["annotations"].as_object();
        let keys: BTreeSet<&str> = annotations
            .into_iter()
            .flat_map(|annotations| annotations.keys().map(String::as_str))
            .collect();
        assert_eq!(
            keys,
            BTreeSet::from([
                "destructiveHint",
                "idempotentHint",
                "openWorldHint",
                "readOnlyHint"
            ]),
            "{}",
            tool["name"]
        );
        assert!(
            annotations
                .into_iter()
                .flatten()
                .all(|(_, hint)| hint.is_boolean()),
            "{}",
            tool["name"]
        );
    }
}
