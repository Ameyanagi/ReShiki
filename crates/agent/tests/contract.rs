//! Schema and decoder parity for the agent's JSON contracts. Each contract's
//! cases file under tests/fixtures/agent-contract inventories the decoder's
//! rules with cited sources and pins how the hand-written JSON Schema and the
//! Rust decoder classify boundary values. See README.md there.
mod support;

use reshiki_agent::{
    Proposal,
    access::Grants,
    canvas_tools,
    ops::{
        budget::{Budgets, RenderBudget},
        calculate::{
            CalcFuture, CalcRequest, CalcTask, CalcValue, Calculated, Calculations, Engine,
            EngineIdentity,
        },
        catalog::{BASE_SPECS, RESULTS, SPECS, catalog_json, decode_check},
        compose::SKETCH_PLACEHOLDER,
        error::ErrorKind,
        headless::HeadlessHost,
        host::{Call, ToolHost},
        inspect::MAX_TEXT_CHARS,
        naming::{self, Naming, NamingFuture},
        progress::{Monotonic, Progress},
        wire::{Principal, RequestId},
    },
    tool_spec::{ToolSpec, valid_name},
};
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
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

/// `schema` as a client resolves it: each `{"$ref": "#/$defs/name"}` replaced
/// by its definition, and `$defs` removed. The contract harness's keyword
/// allowlist has neither keyword, so every check below runs on this form.
fn inlined(mut schema: Value) -> Value {
    fn resolve(schema: &mut Value, defs: &Value) {
        if let Some(reference) = schema.get("$ref") {
            let name = reference
                .as_str()
                .and_then(|reference| reference.strip_prefix("#/$defs/"))
                .unwrap_or_else(|| panic!("unsupported $ref {reference}"));
            assert_eq!(schema.as_object().map(Map::len), Some(1), "$ref {name}");
            *schema = defs
                .get(name)
                .unwrap_or_else(|| panic!("no $defs entry {name}"))
                .clone();
        }
        match schema {
            Value::Object(members) => members.values_mut().for_each(|m| resolve(m, defs)),
            Value::Array(items) => items.iter_mut().for_each(|item| resolve(item, defs)),
            _ => {}
        }
    }
    let defs = schema
        .as_object_mut()
        .and_then(|schema| schema.remove("$defs"))
        .unwrap_or_default();
    resolve(&mut schema, &defs);
    schema
}

fn spec(name: &str) -> &'static ToolSpec {
    SPECS
        .iter()
        .find(|spec| spec.name == name)
        .unwrap_or_else(|| panic!("no operation tool {name}"))
}

/// One schema for several operation tools. A case value is
/// `{"tool": name, "arguments": {...}}`; the branch for `name` checks the
/// arguments against that tool's inputSchema.
fn tools_schema(names: &[&str]) -> Value {
    let branches: Vec<Value> = names
        .iter()
        .map(|name| {
            let arguments = inlined((spec(name).input_schema)());
            json!({
                "type": "object",
                "properties": {"tool": {"const": name}, "arguments": arguments},
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

const PREDICT_NMR: Contract = Contract {
    name: "ops-predict-nmr",
    schema: || tools_schema(&["predict_nmr"]),
    decode: decode_tool_call,
    cases: "ops-predict-nmr-cases.json",
};

#[test]
fn ops_predict_nmr_schema_and_decoder_parity() {
    match corpus::check(&PREDICT_NMR) {
        Ok(report) => eprintln!("{report}"),
        Err(failures) => panic!("{failures}"),
    }
}

const LABEL_ATOMS: Contract = Contract {
    name: "ops-label-atoms",
    schema: || tools_schema(&["label_atoms"]),
    decode: decode_tool_call,
    cases: "ops-label-atoms-cases.json",
};

#[test]
fn ops_label_atoms_schema_and_decoder_parity() {
    match corpus::check(&LABEL_ATOMS) {
        Ok(report) => eprintln!("{report}"),
        Err(failures) => panic!("{failures}"),
    }
}

#[test]
fn quantum_chemistry_schema_and_decoder_parity() {
    for contract in [
        Contract {
            name: "ops-export-input",
            schema: || tools_schema(&["export_input"]),
            decode: decode_tool_call,
            cases: "ops-export-input-cases.json",
        },
        Contract {
            name: "ops-read-output",
            schema: || tools_schema(&["read_output"]),
            decode: decode_tool_call,
            cases: "ops-read-output-cases.json",
        },
        Contract {
            name: "ops-calculate",
            schema: || tools_schema(&["calculate"]),
            decode: decode_tool_call,
            cases: "ops-calculate-cases.json",
        },
    ] {
        match corpus::check(&contract) {
            Ok(report) => eprintln!("{report}"),
            Err(failures) => panic!("{failures}"),
        }
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

/// What compose lists for the sketch object.
fn sketch_placeholder() -> Value {
    json!({"type": "object", "description": SKETCH_PLACEHOLDER})
}

/// compose's inputSchema as a client that drew a sketch knows it: the
/// sketch placeholder replaced by the schema the `schema` tool returns.
fn compose_with_sketch() -> Value {
    let mut schema = inlined((spec("compose").input_schema)());
    let sketch = &mut schema["properties"]["proposal"]["properties"]["sketch"]["anyOf"][1];
    assert_eq!(*sketch, sketch_placeholder());
    *sketch = reshiki_agent::ops::schema::sketch();
    schema
}

const COMPOSE: Contract = Contract {
    name: "ops-compose",
    schema: || {
        json!({
            "type": "object",
            "properties": {"tool": {"const": "compose"}, "arguments": compose_with_sketch()},
            "required": ["tool", "arguments"],
            "additionalProperties": false,
        })
    },
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

/// compose publishes the assistant's Proposal schema in a smaller form:
/// without `explanation` and `replace_ids`, and with the sketch object as a
/// placeholder whose fields the `schema` tool returns. With the placeholder
/// swapped back and `$defs` resolved it is the pinned schema (the bytes
/// `canvas_preview` publishes) less those two properties.
#[test]
fn compose_derives_its_proposal_schema_from_the_pinned_one() {
    let golden = include_str!("../../../tests/fixtures/agent-contract/proposal-schema.json")
        .strip_suffix('\n')
        .expect("proposal-schema.json ends in one LF");
    assert_eq!(reshiki_agent::schema().to_string(), golden);

    let published = (spec("compose").input_schema)();
    let defs: Vec<&str> = published["$defs"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(defs, ["molecules"]);

    let mut expected = reshiki_agent::schema();
    for name in ["explanation", "replace_ids"] {
        assert!(
            expected["properties"]
                .as_object_mut()
                .unwrap()
                .remove(name)
                .is_some()
        );
    }
    expected["required"] = json!(["molecules", "reactions", "composition", "sketch"]);
    assert_eq!(
        reshiki_agent::ops::schema::sketch(),
        expected["properties"]["sketch"]["anyOf"][1]
    );
    assert_eq!(compose_with_sketch()["properties"]["proposal"], expected);
    assert!(SKETCH_PLACEHOLDER.contains("call schema with name sketch"));
    assert!(SKETCH_PLACEHOLDER.contains("Keep molecules and reactions empty when using sketch."));
}

/// The tool list every MCP client loads into its context stays small. It
/// was 27,316 characters with the whole Proposal schema, 24,412 once
/// compose shared its repeated subschemas. The sketch schema is now fetched
/// on demand.
#[test]
fn the_operation_catalog_stays_small() {
    let size = catalog_json().to_string().chars().count();
    eprintln!("the operation catalog is {size} characters");
    assert!(size <= 24_610, "the catalog is {size} characters");
    let sketch = reshiki_agent::ops::schema::sketch()
        .to_string()
        .chars()
        .count();
    eprintln!("the sketch schema, fetched on demand, is {sketch} characters");
    assert!(sketch > 9_000, "the sketch schema is {sketch} characters");
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

/// The leading `{...}` lists only the keys of the envelope's `value`; the
/// prose after it documents nested fields and their meanings.
fn result_keys(tool: &str) -> BTreeSet<&'static str> {
    let description = RESULTS
        .iter()
        .find(|(name, _)| *name == tool)
        .unwrap_or_else(|| panic!("no result documentation for {tool}"))
        .1;
    let (keys, prose) = description
        .strip_prefix('{')
        .and_then(|text| text.split_once('}'))
        .unwrap_or_else(|| panic!("{tool}: result must start with {{top-level keys}}"));
    assert!(!prose.trim().is_empty(), "{tool}: no result description");
    let listed: Vec<_> = keys.split(',').map(str::trim).collect();
    assert!(
        listed.iter().all(|key| valid_name(key)),
        "{tool}: invalid result keys {listed:?}"
    );
    let unique: BTreeSet<_> = listed.iter().copied().collect();
    assert_eq!(listed.len(), unique.len(), "{tool}: duplicate result key");
    unique
}

#[test]
fn every_operation_has_exactly_one_result_description() {
    let mut names = BTreeSet::new();
    for (name, _) in RESULTS {
        assert!(names.insert(*name), "duplicate result documentation {name}");
        let _ = result_keys(name);
    }
    assert_eq!(names, SPECS.iter().map(|spec| spec.name).collect());
}

/// Match real request IDs to successful structured results in every tools
/// transcript, including future fixtures in this directory.
// Test fixtures are read directly, outside the agent's filesystem grants.
#[allow(clippy::disallowed_methods)]
fn golden_results() -> Vec<(PathBuf, String, Value)> {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/mcp");
    let mut paths: Vec<_> = std::fs::read_dir(&directory)
        .expect("MCP fixture directory")
        .map(|entry| entry.expect("MCP fixture entry").path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("tools-") && name.ends_with(".jsonl"))
        })
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "no tools-*.jsonl fixtures");
    let mut successes = Vec::new();
    for path in paths {
        let mut requests = BTreeMap::new();
        for line in std::fs::read_to_string(&path)
            .expect("MCP tools fixture")
            .lines()
        {
            if let Some(request) = line.strip_prefix("> ") {
                let request: Value = serde_json::from_str(request).expect("request JSON");
                if request["method"] == "tools/call" {
                    let name = request["params"]["name"]
                        .as_str()
                        .expect("tool name")
                        .to_owned();
                    assert!(
                        SPECS.iter().any(|spec| spec.name == name),
                        "{}: unknown operation tool {name}",
                        path.display()
                    );
                    requests.insert(request["id"].to_string(), name);
                }
            } else if let Some(response) = line.strip_prefix("< ") {
                let response: Value = serde_json::from_str(response).expect("response JSON");
                if let Some(name) = requests.remove(&response["id"].to_string())
                    && response["result"]["isError"] == false
                {
                    successes.push((
                        path.clone(),
                        name,
                        response["result"]["structuredContent"].clone(),
                    ));
                }
            }
        }
    }
    successes
}

async fn successful_result(host: &HeadlessHost, tool: &str, arguments: Value) -> Value {
    let result = host
        .call(Call {
            principal: Principal::local(),
            request: RequestId::Int(1),
            tool: tool.to_owned(),
            arguments,
            progress: None,
        })
        .await
        .unwrap_or_else(|error| panic!("{tool}: {error}"));
    assert!(!result.is_error, "{tool}: {:?}", result.value);
    Value::Object(result.value)
}

/// Every successful result is the one envelope, and its `value` has exactly
/// the documented keys.
fn assert_result_keys(tool: &str, result: &Value, source: &str) {
    let keys = |value: &Value| -> BTreeSet<String> {
        value
            .as_object()
            .unwrap_or_else(|| panic!("{source}: {tool}: not an object"))
            .keys()
            .cloned()
            .collect()
    };
    assert_eq!(
        keys(result),
        ["validation", "value", "versions", "warnings"]
            .map(str::to_owned)
            .into(),
        "{source}: {tool}"
    );
    let documented: BTreeSet<String> = result_keys(tool).into_iter().map(str::to_owned).collect();
    assert_eq!(documented, keys(&result["value"]), "{source}: {tool}");
}

/// A naming hook for the name tool's result keys; the real one lives in the
/// application.
struct Named;

impl Naming for Named {
    fn resolve(&self, _: String) -> NamingFuture<naming::Resolved> {
        Box::pin(async {
            Ok(naming::Resolved {
                smiles: "C".into(),
                prepared: None,
                identity: "C".into(),
                warnings: Vec::new(),
            })
        })
    }

    fn verify(&self, _: &str, _: &reshiki_model::document::Document) -> Result<(), String> {
        Ok(())
    }

    fn generate(
        &self,
        _: reshiki_model::document::Document,
    ) -> NamingFuture<(naming::Named, Vec<String>)> {
        Box::pin(async {
            let named = naming::Named {
                name: "methane".into(),
                status: "preferred".into(),
                preferred: true,
                reason: None,
                detail: String::new(),
                aliases: Vec::new(),
                alternatives: Vec::new(),
            };
            Ok((named, Vec::new()))
        })
    }
}

const WATER_XYZ: &str = "3\nwater\nO 0 0 0\nH 0.96 0 0\nH -0.24 0.93 0\n";
const GAUSSIAN_OUTPUT: &str = include_str!("../../../tests/fixtures/qc/pymatgen/H2O_gau_vib.out");

fn export_input_args() -> Value {
    json!({
        "xyz": format!("{WATER_XYZ}1\nsecond frame\nCl 1 2 3\n"),
        "program": "gaussian", "job": "opt_freq", "method": null, "basis": null,
        "charge": null, "multiplicity": null, "title": null, "cores": null,
        "memory_gb": null, "extra": null,
    })
}

fn calculate_args() -> Value {
    json!({
        "xyz": WATER_XYZ, "task": "energy", "method": "gfn2", "charge": 0, "uhf": 0,
        "solvent": null, "conventions": "axtb", "electronic_temperature_k": null,
        "accuracy": null, "max_iterations": null, "threads": null, "convergence": null,
        "fixed_atoms": null,
    })
}

struct FakeCalculations {
    stall: Option<Arc<Stall>>,
}

#[derive(Default)]
struct Stall {
    started: tokio::sync::Notify,
    dropped: AtomicBool,
}

struct DropFlag(Arc<Stall>);

impl Drop for DropFlag {
    fn drop(&mut self) {
        self.0.dropped.store(true, Ordering::SeqCst);
    }
}

impl Calculations for FakeCalculations {
    fn engine(&self) -> Engine {
        Engine {
            name: "axtb".into(),
            version: "test".into(),
            license: "MIT".into(),
            source: "test".into(),
            rev: "abc123".into(),
        }
    }

    fn calculate(&self, request: CalcRequest, progress: Option<Arc<Monotonic>>) -> CalcFuture {
        assert_eq!(request.task, CalcTask::Energy);
        assert_eq!(request.frame.numbers, [8, 1, 1]);
        assert_eq!(
            request.settings.method,
            reshiki_io::qcdata::XtbMethod::Gfn2Xtb
        );
        if let Some(stall) = &self.stall {
            let flag = DropFlag(Arc::clone(stall));
            return Box::pin(async move {
                flag.0.started.notify_one();
                let _flag = flag;
                std::future::pending().await
            });
        }
        Box::pin(async move {
            if let Some(progress) = progress {
                progress.report(Progress {
                    completed: 1.0,
                    total: Some(2.0),
                    message: Some("SCF".into()),
                });
            }
            Ok(Calculated {
                value: CalcValue {
                    task: CalcTask::Energy,
                    method: "gfn2".into(),
                    engine: EngineIdentity {
                        name: "axtb".into(),
                        version: "test".into(),
                        rev: "abc123".into(),
                    },
                    atoms: 3,
                    charge: 0,
                    multiplicity: 1,
                    energy_hartree: -5.0,
                    homo_lumo_gap_ev: Some(1.0),
                    dipole_debye: Some(1.2),
                    charges: Some(vec![-0.6, 0.3, 0.3]),
                    optimization: None,
                    frequencies: None,
                    seconds: 0.25,
                },
                warnings: vec!["test warning".into()],
            })
        })
    }
}

#[tokio::test]
async fn calculate_is_hidden_without_a_hook_and_reports_progress_with_one() {
    let host = HeadlessHost::new(
        "contract-test",
        Budgets {
            op_deadline: Duration::from_nanos(1),
            ..Budgets::default()
        },
    );
    assert_eq!(host.catalog().len(), BASE_SPECS.len());
    let call = |progress| Call {
        principal: Principal::local(),
        request: RequestId::Int(1),
        tool: "calculate".into(),
        arguments: calculate_args(),
        progress,
    };
    let error = host.call(call(None)).await.unwrap_err();
    assert_eq!(error.kind, ErrorKind::UnknownTool);

    let host = host.with_calculations(Arc::new(FakeCalculations { stall: None }));
    assert_eq!(host.catalog().len(), SPECS.len());
    assert_eq!(host.catalog().last().unwrap().name, "calculate");
    let events = Arc::new(Mutex::new(Vec::new()));
    let result = host
        .call(call(Some(Arc::new({
            let events = Arc::clone(&events);
            move |progress| events.lock().unwrap().push(progress)
        }))))
        .await
        .unwrap();
    assert!(!result.is_error, "{:?}", result.value);
    assert_eq!(result.value["value"]["energy_hartree"], -5.0);
    assert_eq!(
        result.value["warnings"],
        json!([{"message":"test warning"}])
    );
    assert_eq!(events.lock().unwrap()[0].completed, 1.0);
}

#[tokio::test]
async fn calculate_cancellation_drops_the_hook_future() {
    let stall = Arc::new(Stall::default());
    let host = Arc::new(
        HeadlessHost::new("contract-test", Budgets::default()).with_calculations(Arc::new(
            FakeCalculations {
                stall: Some(Arc::clone(&stall)),
            },
        )),
    );
    let request = RequestId::Int(42);
    let principal = Principal::local();
    let run = tokio::spawn({
        let host = Arc::clone(&host);
        let request = request.clone();
        let principal = principal.clone();
        async move {
            host.call(Call {
                principal,
                request,
                tool: "calculate".into(),
                arguments: calculate_args(),
                progress: None,
            })
            .await
        }
    });
    let bound = Duration::from_secs(10);
    tokio::time::timeout(bound, stall.started.notified())
        .await
        .unwrap();
    host.cancel(&principal, &request);
    let error = tokio::time::timeout(bound, run)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::Cancelled);
    assert!(stall.dropped.load(Ordering::SeqCst));
}

#[tokio::test]
async fn gfnff_solvent_is_an_unsupported_calculation() {
    let host = HeadlessHost::new("contract-test", Budgets::default())
        .with_calculations(Arc::new(FakeCalculations { stall: None }));
    let mut arguments = calculate_args();
    arguments["method"] = json!("gfnff");
    arguments["solvent"] = json!("water");
    let result = host
        .call(Call {
            principal: Principal::local(),
            request: RequestId::Int(1),
            tool: "calculate".into(),
            arguments,
            progress: None,
        })
        .await
        .unwrap();
    assert!(result.is_error);
    assert_eq!(result.value["error"]["code"], "unsupported");
    assert_eq!(
        result.value["error"]["message"],
        "GFN-FF does not support solvent"
    );
}

#[tokio::test]
async fn export_input_checks_xyz_and_output_byte_limits() {
    let request = || Call {
        principal: Principal::local(),
        request: RequestId::Int(1),
        tool: "export_input".into(),
        arguments: export_input_args(),
        progress: None,
    };
    let host = HeadlessHost::new(
        "contract-test",
        Budgets {
            max_text_bytes: 40,
            ..Budgets::default()
        },
    );
    let rejected = host.call(request()).await.unwrap();
    assert_eq!(rejected.value["error"]["code"], "budget");
    assert_eq!(rejected.value["error"]["param"], "xyz");

    let host = HeadlessHost::new(
        "contract-test",
        Budgets {
            max_output_bytes: 40,
            ..Budgets::default()
        },
    );
    let rejected = host.call(request()).await.unwrap();
    assert_eq!(rejected.value["error"]["code"], "budget");
    assert!(
        rejected.value["error"]["message"]
            .as_str()
            .unwrap()
            .contains("max_output_bytes")
    );
}

#[tokio::test]
#[allow(clippy::disallowed_methods, clippy::disallowed_types)]
async fn read_output_accepts_a_granted_file_and_uses_its_detection_name() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("result");
    std::fs::write(&path, WATER_XYZ).unwrap();
    let args = json!({"text": null, "file_name": "result.xyz", "path": path, "job": 0});
    let host = HeadlessHost::new("contract-test", Budgets::default());
    let denied = host
        .call(Call {
            principal: Principal::local(),
            request: RequestId::Int(1),
            tool: "read_output".into(),
            arguments: args.clone(),
            progress: None,
        })
        .await
        .unwrap();
    assert_eq!(denied.value["error"]["code"], "access_denied");

    let grants = Grants::open(&[dir.path().to_path_buf()], &[]).unwrap();
    let host = host.with_grants(Arc::new(grants));
    let result = successful_result(&host, "read_output", args).await;
    assert_eq!(result["value"]["format"], "XYZ");
    assert_eq!(result["value"]["job"]["index"], 0);
}

/// Prefer successful real-binary goldens. Tools without one run against the
/// same operation API through HeadlessHost; native files need no engine.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[allow(clippy::disallowed_methods, clippy::disallowed_types)]
async fn documented_result_keys_match_successful_operations() {
    let mut covered = BTreeSet::new();
    for (path, tool, result) in golden_results() {
        assert_result_keys(&tool, &result, &path.display().to_string());
        covered.insert(tool);
    }
    let missing: Vec<_> = SPECS
        .iter()
        .filter(|spec| !covered.contains(spec.name))
        .map(|spec| spec.name)
        .collect();
    if missing.is_empty() {
        return;
    }
    let directory = tempfile::tempdir().expect("native file fixture directory");
    let input = directory.path().join("input.rsk");
    std::fs::write(
        &input,
        include_bytes!("../../../tests/fixtures/bond-join-regression.rsk"),
    )
    .expect("native input fixture");
    let grants = Grants::open(
        &[directory.path().to_path_buf()],
        &[directory.path().to_path_buf()],
    )
    .expect("fixture grants");
    let host = HeadlessHost::new("contract-test", Budgets::default())
        .with_grants(Arc::new(grants))
        .with_naming(Arc::new(Named))
        .with_calculations(Arc::new(FakeCalculations { stall: None }));
    for tool in missing {
        let arguments = match tool {
            "info" | "document_new" | "document_list" => json!({}),
            "document_close" | "file_save" => {
                let created = successful_result(&host, "document_new", json!({})).await;
                if tool == "document_close" {
                    json!({"document": created["value"]["document"]})
                } else {
                    json!({
                        "document": created["value"]["document"],
                        "path": directory.path().join("output.rsk"),
                        "format": null, "overwrite": false, "pages": null, "svg_text": null,
                    })
                }
            }
            "file_open" => json!({"path": input, "format": "auto"}),
            "schema" => json!({"name": "sketch"}),
            "export_input" => export_input_args(),
            "read_output" => json!({
                "text": GAUSSIAN_OUTPUT, "file_name": "H2O_gau_vib.out", "path": null, "job": null,
            }),
            "calculate" => calculate_args(),
            "convert" => json!({
                "input": {
                    "format": "rsk",
                    "text": include_str!("../../../tests/fixtures/bond-join-regression.rsk"),
                },
                "to": "preview", "pages": null, "svg_text": null,
                "max_width": null, "max_height": null,
            }),
            "name" => {
                let created = successful_result(&host, "document_new", json!({})).await;
                json!({"document": created["value"]["document"]})
            }
            _ => panic!("{tool}: add a successful golden or HeadlessHost fallback"),
        };
        let result = successful_result(&host, tool, arguments).await;
        assert_result_keys(tool, &result, "HeadlessHost");
        match tool {
            "info" => {
                let calculations = &result["value"]["calculations"];
                assert_eq!(calculations["engine"]["rev"], "abc123");
                assert_eq!(calculations["max_atoms"], 500);
                assert_eq!(calculations["max_frequency_atoms"], 200);
                assert_eq!(calculations["methods"][0]["solvent"], "native");
                assert_eq!(calculations["methods"][1]["solvent"], "gfn2_correction");
                assert!(calculations["methods"][2]["solvent"].is_null());
            }
            "export_input" => {
                assert_eq!(result["value"]["file_name"], "molecule.gjf");
                let text = result["value"]["text"].as_str().unwrap();
                assert!(text.contains("%nprocshared=8") && text.contains("%mem=16GB"));
                assert!(text.contains("Def2SVP") && text.contains("molecule"));
                assert!(!text.contains("Cl"));
            }
            "read_output" => {
                let job = &result["value"]["job"];
                assert_eq!(job["completion"], "normal");
                assert_eq!(job["frequencies_cm1"].as_array().unwrap().len(), 3);
                assert!(job["xyz"].as_str().unwrap().starts_with("3\n"));
            }
            _ => {}
        }
        covered.insert(tool.to_owned());
    }
    assert_eq!(
        covered,
        SPECS.iter().map(|spec| spec.name.to_owned()).collect()
    );
}

#[test]
fn tool_description_limits_match_defaults() {
    let description = |name| spec(name).description;
    let parameter = |name, parameter: &str| {
        (spec(name).input_schema)()["properties"][parameter]["description"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let number = |text: &str, marker: &str| -> u64 {
        text.split_once(marker)
            .unwrap()
            .1
            .trim_start()
            .chars()
            .take_while(|character| character.is_ascii_digit() || *character == ',')
            .filter(|character| *character != ',')
            .collect::<String>()
            .parse()
            .unwrap()
    };
    let budgets = Budgets::default();
    let new = description("document_new");
    assert_eq!(
        number(new, "without use (") * 60,
        budgets.idle_ttl.as_secs()
    );
    assert_eq!(number(new, "max_documents ("), budgets.max_documents as u64);
    assert_eq!(
        number(description("inspect"), "Annotation text over "),
        MAX_TEXT_CHARS as u64,
    );
    assert_eq!(
        number(&parameter("apply", "idempotency_key"), "At most "),
        budgets.max_idempotency_key_bytes as u64,
    );
    assert_eq!(
        number(&parameter("render", "format"), "scaled up to ") as f32,
        canvas_tools::MAX_UPSCALE
    );
    assert_eq!(
        number(&parameter("render", "max_width"), "null for "),
        u64::from(budgets.render.default_width)
    );
    assert_eq!(
        number(&parameter("render", "max_height"), "null for "),
        u64::from(budgets.render.default_height)
    );
    // The side limits are in the schema itself.
    let side = &(spec("render").input_schema)()["properties"]["max_width"]["anyOf"][1];
    assert_eq!(side["minimum"], budgets.render.min_side);
    assert_eq!(side["maximum"], budgets.render.max_side);
    assert_eq!(budgets.render, RenderBudget::DEFAULT);
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
/// ReShiki's own rules, checked with `$defs` resolved: every schema uses
/// only the contract harness's keywords and pattern table, and every object
/// level is strict.
#[test]
fn the_operation_catalog_follows_the_mcp_tool_rules() {
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
        let schema = inlined((spec.input_schema)());
        assert_eq!(schema["type"], "object", "{name}");
        assert!(schema.get("$schema").is_none(), "{name}");
        assert_eq!(json_schema::check_keywords(&schema), [], "{name}");
        // The one object level left open: compose's sketch, whose strict
        // schema the schema tool returns.
        let (mut loose, mut skipped) = (Vec::new(), Vec::new());
        loose_objects(&schema, "", &sketch_placeholder(), &mut loose, &mut skipped);
        assert_eq!(loose, Vec::<String>::new(), "{name}: not strict");
        let open: &[&str] = if name == "compose" {
            &["/properties/proposal/properties/sketch/anyOf/1"]
        } else {
            &[]
        };
        assert_eq!(skipped, open, "{name}");
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
const EFFECTS: [(&str, Effect); 21] = [
    ("info", Effect::ReadOnly),
    ("convert", Effect::ReadOnly),
    ("name", Effect::ReadOnly),
    ("schema", Effect::ReadOnly),
    ("document_list", Effect::ReadOnly),
    ("inspect", Effect::ReadOnly),
    ("analyze", Effect::ReadOnly),
    ("predict_nmr", Effect::ReadOnly),
    ("render", Effect::ReadOnly),
    ("export", Effect::ReadOnly),
    ("export_input", Effect::ReadOnly),
    ("read_output", Effect::ReadOnly),
    ("calculate", Effect::ReadOnly),
    ("document_new", Effect::Additive),
    ("import", Effect::Additive),
    ("compose", Effect::Additive),
    ("file_open", Effect::Additive),
    ("document_close", Effect::Destructive),
    ("apply", Effect::Destructive),
    ("label_atoms", Effect::Destructive),
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
