//! Schema and decoder parity for the agent's JSON contracts. Each contract's
//! cases file under tests/fixtures/agent-contract inventories the decoder's
//! rules with cited sources and pins how the hand-written JSON Schema and the
//! Rust decoder classify boundary values. See README.md there.
mod support;

use reshiki_agent::{
    Proposal,
    ops::catalog::{SPECS, decode_check},
};
use serde_json::{Value, json};
use support::corpus::{self, Contract};

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
