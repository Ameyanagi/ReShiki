//! The experimental labels and tool annotations that `reshiki --mcp` and
//! `reshiki --cli` show clients and users, through the real binary.
//!
//! Every run has an empty data folder and a watchdog, and inherits
//! RESHIKI_INCHI_HELPER as CI sets it (.github/workflows/checks.yml).
#[path = "common/headless.rs"]
mod headless;

use headless::{McpSession, StderrMode};
use reshiki_agent::{
    ops::catalog::SPECS,
    stability::{DATA_NOT_INSTRUCTIONS, GUIDE_URL, NOTICE, STABILITY},
};
use serde_json::{Value, json};
use std::{process::Output, time::Duration};

const LINE_TIMEOUT: Duration = Duration::from_secs(10);
/// Long enough for a debug build's first chemistry engine call on CI.
const CLI: Duration = Duration::from_secs(120);
const TITLE: &str = "ReShiki (experimental)";

fn cli(args: &[&str]) -> Output {
    let dir = tempfile::tempdir().expect("working folder");
    let output = headless::run_cli(dir.path(), args, b"", CLI);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "{args:?}: {stderr}");
    output
}

/// The one JSON line a CLI command printed.
fn json_line(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("one JSON line")
}

fn assert_instructions(result: &Value) {
    let instructions = result["instructions"].as_str().expect("instructions");
    let prefix = format!("{NOTICE} {DATA_NOT_INSTRUCTIONS} ");
    assert!(instructions.starts_with(&prefix), "{instructions}");
}

fn finish(mut session: McpSession) {
    session.close_stdin();
    assert_eq!(session.wait_exit(LINE_TIMEOUT).code(), Some(0));
    assert_eq!(session.rest(LINE_TIMEOUT), Vec::<Value>::new());
}

#[test]
fn discover_lists_and_info_carry_the_labels_and_hints() {
    let mut session = McpSession::start(&[], StderrMode::Captured);
    session.send(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "server/discover",
        "params": {"_meta": headless::modern_meta()},
    }));
    let discovered = session.recv_for(Some(&json!(1)), LINE_TIMEOUT);
    let result = &discovered["result"];
    let server = &result["_meta"]["io.modelcontextprotocol/serverInfo"];
    assert_eq!(server["name"], "reshiki", "{discovered}");
    assert_eq!(server["title"], TITLE, "{discovered}");
    assert_eq!(server["description"], NOTICE, "{discovered}");
    assert_eq!(server["websiteUrl"], GUIDE_URL, "{discovered}");
    assert_instructions(result);

    session.send(headless::modern_list(2));
    let listed = session.recv_for(Some(&json!(2)), LINE_TIMEOUT);
    let tools = listed["result"]["tools"].as_array().expect("tools");
    let served: Vec<(&str, Value)> = tools
        .iter()
        .map(|tool| {
            (
                tool["name"].as_str().expect("name"),
                tool["annotations"].clone(),
            )
        })
        .collect();
    let hints: Vec<(&str, Value)> = SPECS
        .iter()
        .map(|spec| {
            let hints = spec.hints.expect("every operation tool has hints");
            let annotations = json!({
                "readOnlyHint": hints.read_only,
                "destructiveHint": hints.destructive,
                "idempotentHint": hints.idempotent,
                "openWorldHint": hints.open_world,
            });
            (spec.name, annotations)
        })
        .collect();
    assert_eq!(served, hints);

    session.send(headless::modern_call(3, "info", json!({})));
    let info = session.recv_for(Some(&json!(3)), LINE_TIMEOUT);
    assert_eq!(
        info["result"]["structuredContent"]["stability"], STABILITY,
        "{info}"
    );
    finish(session);
}

#[test]
fn initialize_names_the_server_experimental() {
    let mut session = McpSession::start(&[], StderrMode::Captured);
    session.send(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": {"name": "agent_api_labels", "version": "1.0.0"},
        },
    }));
    let initialized = session.recv_for(Some(&json!(1)), LINE_TIMEOUT);
    let result = &initialized["result"];
    assert_eq!(result["protocolVersion"], "2025-11-25", "{initialized}");
    assert_eq!(result["serverInfo"]["title"], TITLE, "{initialized}");
    assert_instructions(result);
    finish(session);
}

#[test]
fn cli_help_opens_with_the_notice() {
    let output = cli(&["help"]);
    let help = String::from_utf8(output.stdout).expect("UTF-8 help");
    assert_eq!(help.lines().next(), Some(NOTICE), "{help}");
}

#[test]
fn cli_json_is_marked_experimental() {
    let info = json_line(&cli(&["info"]));
    assert_eq!(info["experimental"], true, "{info}");
    assert_eq!(info["stability"], STABILITY, "{info}");
    let analyzed = json_line(&cli(&["analyze", "--smiles", "CCO"]));
    assert_eq!(analyzed["experimental"], true, "{analyzed}");
    assert_eq!(
        analyzed["value"]["analysis"]["formula"], "C2H6O",
        "{analyzed}"
    );
    let receipt = json_line(&cli(&[
        "convert",
        "--smiles",
        "CCO",
        "-o",
        "ethanol.mol",
        "--receipt",
    ]));
    assert_eq!(receipt["experimental"], true, "{receipt}");
}
