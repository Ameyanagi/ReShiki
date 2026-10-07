//! Schema and decoder parity for the agent's JSON contracts. Each contract's
//! cases file under tests/fixtures/agent-contract inventories the decoder's
//! rules with cited sources and pins how the hand-written JSON Schema and the
//! Rust decoder classify boundary values. See README.md there.
mod support;

use reshiki_agent::Proposal;
use serde_json::Value;
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
