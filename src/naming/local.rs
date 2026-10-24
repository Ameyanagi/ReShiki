//! Native Rust OPSIN adapter, isolated in the bounded same-executable worker.
//! The browser sends the same request to its naming Web Worker instead.
#[cfg(not(target_arch = "wasm32"))]
use super::client::{self, CancelOnDrop, POLL};
use super::{Error, Provenance, Record, canonical_smiles, worker};
#[cfg(not(target_arch = "wasm32"))]
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
const OUTPUT_LIMIT: usize = worker::OUTPUT_LIMIT;
#[cfg(not(target_arch = "wasm32"))]
static SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);
#[cfg(not(target_arch = "wasm32"))]
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
#[cfg(not(target_arch = "wasm32"))]
pub async fn resolve_name(name: &str, cancel: Cancel) -> Result<Record, Error> {
    resolve_with(name, cancel, &SLOTS, run).await
}

#[cfg(not(target_arch = "wasm32"))]
async fn resolve_with(
    name: &str,
    cancel: Cancel,
    slots: &'static tokio::sync::Semaphore,
    run: impl FnOnce(&str, &Cancel) -> Result<Record, Error> + Send + 'static,
) -> Result<Record, Error> {
    let name = checked_name(name)?;
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

/// The trimmed name, if the parser accepts its length and characters.
pub(super) fn checked_name(name: &str) -> Result<&str, Error> {
    const MESSAGE: &str = "Enter a chemical name of at most 2048 bytes without control characters";
    if name.len() > worker::MAX_NAME_BYTES || name.chars().any(char::is_control) {
        return Err(Error::Rejected(MESSAGE.into()));
    }
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::Rejected(MESSAGE.into()));
    }
    Ok(name)
}

#[cfg(not(target_arch = "wasm32"))]
fn run(name: &str, cancel: &Cancel) -> Result<Record, Error> {
    run_with_executable(name, cancel, &worker::executable().map_err(Error::Worker)?)
}
#[cfg(not(target_arch = "wasm32"))]
fn run_with_executable(name: &str, cancel: &Cancel, executable: &Path) -> Result<Record, Error> {
    if cancel.stopped() {
        return Err(Error::Cancelled("Local name parsing was cancelled".into()));
    }
    let request = parse_request(name)?;
    let output = client::run_worker("reshiki-native-name-", &request, cancel, || Ok(executable))?;
    parse_result(name, &output)
}

/// The worker request that parses `name`.
pub(super) fn parse_request(name: &str) -> Result<Vec<u8>, Error> {
    serde_json::to_vec(&worker::Request::Parse {
        protocol: worker::PROTOCOL,
        name: retained_parser_name(name).into(),
    })
    .map_err(|error| Error::Worker(error.to_string()))
}

/// The record for the worker's reply to `parse_request(name)`.
pub(super) fn parse_result(name: &str, output: &[u8]) -> Result<Record, Error> {
    certify_retained_name(name, parse_reply(output, retained_parser_name(name))?)
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

fn certify_retained_name(name: &str, mut record: Record) -> Result<Record, Error> {
    if !name.eq_ignore_ascii_case(PLUS_LACTIC_NAME) {
        return Ok(record);
    }
    // The strict adapter already checked warnings, semantic layers, provenance,
    // bounds and the interpreted input. Independently require the specific S
    // graph before exposing the user name to preview, insertion or captions.
    const MESSAGE: &str =
        "The retained (+)-lactic acid name did not resolve to its verified (S) structure";
    super::verify_identity(PLUS_LACTIC_GRAPH, &record.smiles)
        .map_err(|error| Error::Rejected(format!("{MESSAGE}: {error}")))?;
    record.title = name.into();
    record.provenance = Provenance::OpsinRustRetainedName {
        source: "ChEBI:422",
    };
    Ok(record)
}

fn parse_reply(bytes: &[u8], name: &str) -> Result<Record, Error> {
    if bytes.len() > OUTPUT_LIMIT {
        return Err(Error::Worker(
            "Local parser response exceeds the supported limit".into(),
        ));
    }
    let response: worker::Response = serde_json::from_slice(bytes).map_err(|_| {
        Error::Worker("Local parser returned an invalid structured response".into())
    })?;
    let worker::Response::Parse(reply) = response else {
        return Err(Error::Worker(
            "Local parser returned a result for a different operation".into(),
        ));
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
        return Err(Error::Worker(
            "Local parser result has inconsistent version, options or input identity".into(),
        ));
    }
    if !matches!(reply.status, worker::ParserStatus::Success) || !reply.warnings.is_empty() {
        const MESSAGE: &str = "The OPSIN Rust port could not interpret this name unambiguously with complete stereochemistry";
        let details = reply
            .warnings
            .iter()
            .take(16)
            .map(|w| format!("{}: {}", w.kind, w.message))
            .chain(std::iter::once(reply.message))
            .collect::<Vec<_>>()
            .join(" · ");
        return Err(Error::Rejected(format!(
            "{MESSAGE}: {}",
            details.chars().take(2048).collect::<String>()
        )));
    }
    let smiles = reply.cxsmiles.ok_or_else(|| {
        Error::Rejected("The OPSIN Rust port returned no molecular structure".into())
    })?;
    if smiles.contains('|') {
        const MESSAGE: &str = "This name requires relative/racemic stereo, polymer or atom-label semantics unsupported by editable naming previews. No simplified structure was accepted";
        return Err(Error::Rejected(MESSAGE.into()));
    }
    if smiles.contains('.') {
        const MESSAGE: &str = "Local naming supports one connected molecule; disconnected salts and mixtures are unsupported";
        return Err(Error::Disconnected(MESSAGE.into()));
    }
    let canonical_smiles = canonical_smiles(&smiles).map_err(Error::Rejected)?;
    let synonyms = super::aliases_for_smiles(&canonical_smiles).map_err(Error::Worker)?;
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
