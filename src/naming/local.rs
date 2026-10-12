//! Native Rust OPSIN adapter, isolated in the bounded same-executable worker.
use super::{Provenance, Record, canonical_smiles, client, worker};
use client::{CancelOnDrop, POLL};
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
const OUTPUT_LIMIT: usize = worker::OUTPUT_LIMIT;
static SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);
#[derive(Debug, Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);
impl Cancel {
    pub fn stop(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub(super) fn stopped(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
    pub(super) async fn cancelled(&self) {
        while !self.stopped() {
            tokio::time::sleep(POLL).await;
        }
    }
}

/// One deterministic local parse. Dropping the future cancels the supervised
/// worker, which kills and waits for its child before releasing its directory.
pub async fn resolve_name(name: &str, cancel: Cancel) -> Result<Record, String> {
    resolve_with(name, cancel, &SLOTS, run).await
}

async fn resolve_with(
    name: &str,
    cancel: Cancel,
    slots: &'static tokio::sync::Semaphore,
    run: impl FnOnce(&str, &Cancel) -> Result<Record, String> + Send + 'static,
) -> Result<Record, String> {
    if name.len() > worker::MAX_NAME_BYTES || name.chars().any(char::is_control) {
        return Err(
            "Enter a chemical name of at most 2048 bytes without control characters".into(),
        );
    }
    let name = name.trim();
    if name.is_empty() {
        return Err(
            "Enter a chemical name of at most 2048 bytes without control characters".into(),
        );
    }
    let stop = CancelOnDrop(cancel.clone());
    let result = client::on_slot(
        name,
        &cancel,
        slots,
        "Local parser is unavailable",
        "Local name parsing was cancelled",
        "Local parser worker failed",
        run,
    )
    .await;
    drop(stop);
    result
}

fn run(name: &str, cancel: &Cancel) -> Result<Record, String> {
    run_with_executable(name, cancel, &worker::executable()?)
}
fn run_with_executable(name: &str, cancel: &Cancel, executable: &Path) -> Result<Record, String> {
    if cancel.stopped() {
        return Err("Local name parsing was cancelled".into());
    }
    let interpreted_name = retained_parser_name(name);
    let request = serde_json::to_vec(&worker::Request::Parse {
        protocol: worker::PROTOCOL,
        name: interpreted_name.into(),
    })
    .map_err(|error| error.to_string())?;
    let output = client::run_worker("reshiki-native-name-", &request, cancel, || Ok(executable))?;
    certify_retained_name(name, parse_reply(&output, interpreted_name)?)
}

// A curated retained-name equivalence, not a rule from optical rotation to R/S.
// CHEBI:422 explicitly lists (+)-lactic acid and its (2S) graph:
// https://www.ebi.ac.uk/chebi/CHEBI:422 (entry modified 2024-04-18).
// Only this complete name is admitted; salts, derivatives, conflicting stereo
// and all other optical-rotation names still use the strict parser unchanged.
const PLUS_LACTIC_NAME: &str = "(+)-lactic acid";
const PLUS_LACTIC_PARSER_NAME: &str = "(2S)-2-hydroxypropanoic acid";
const PLUS_LACTIC_GRAPH: &str = "C[C@H](O)C(=O)O";

fn retained_parser_name(name: &str) -> &str {
    if name.eq_ignore_ascii_case(PLUS_LACTIC_NAME) {
        PLUS_LACTIC_PARSER_NAME
    } else {
        name
    }
}

fn certify_retained_name(name: &str, mut record: Record) -> Result<Record, String> {
    if !name.eq_ignore_ascii_case(PLUS_LACTIC_NAME) {
        return Ok(record);
    }
    // The strict adapter already checked warnings, semantic layers, provenance,
    // bounds and the interpreted input. Independently require the specific S
    // graph before exposing the user name to preview, insertion or captions.
    super::verify_identity(PLUS_LACTIC_GRAPH, &record.smiles).map_err(|error| {
        format!("The retained (+)-lactic acid name did not resolve to its verified (S) structure: {error}")
    })?;
    record.title = name.into();
    record.provenance = Provenance::OpsinRustRetainedName {
        source: "ChEBI:422",
    };
    Ok(record)
}

fn parse_reply(bytes: &[u8], name: &str) -> Result<Record, String> {
    if bytes.len() > OUTPUT_LIMIT {
        return Err("Local parser response exceeds the supported limit".into());
    }
    let response: worker::Response = serde_json::from_slice(bytes)
        .map_err(|_| "Local parser returned an invalid structured response")?;
    let worker::Response::Parse(reply) = response else {
        return Err("Local parser returned a result for a different operation".into());
    };
    if reply.protocol != worker::PROTOCOL
        || reply.backend != worker::BACKEND
        || reply.port_version != worker::PORT_VERSION
        || reply.upstream_version != worker::UPSTREAM_VERSION
        || reply.upstream_commit != worker::UPSTREAM_COMMIT
        || reply.resource_fingerprint != worker::RESOURCE_FINGERPRINT
        || reply.options != worker::OPTIONS
        || reply.name != name
    {
        return Err(
            "Local parser result has inconsistent version, options or input identity".into(),
        );
    }
    if !matches!(reply.status, worker::ParserStatus::Success) || !reply.warnings.is_empty() {
        let details = reply
            .warnings
            .iter()
            .take(16)
            .map(|w| format!("{}: {}", w.kind, w.message))
            .chain(std::iter::once(reply.message))
            .collect::<Vec<_>>()
            .join(" · ");
        return Err(format!(
            "The OPSIN Rust port could not interpret this name unambiguously with complete stereochemistry: {}",
            details.chars().take(2048).collect::<String>()
        ));
    }
    let smiles = reply
        .cxsmiles
        .ok_or("The OPSIN Rust port returned no molecular structure")?;
    if smiles.contains('|') {
        return Err("This name requires relative/racemic stereo, polymer or atom-label semantics unsupported by editable naming previews. No simplified structure was accepted".into());
    }
    if smiles.contains('.') {
        return Err("Local naming supports one connected molecule; disconnected salts and mixtures are unsupported".into());
    }
    let canonical_smiles = canonical_smiles(&smiles)?;
    let synonyms = super::aliases_for_smiles(&canonical_smiles)?;
    Ok(Record {
        title: name.into(),
        systematic_name: None,
        smiles,
        canonical_smiles,
        synonyms,
        warnings: vec![],
        provenance: Provenance::OpsinRust,
    })
}

#[cfg(test)]
mod tests;
