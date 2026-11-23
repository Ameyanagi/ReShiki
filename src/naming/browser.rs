//! The browser sends the desktop worker's requests, as the `naming` job, to
//! the editor's Web Worker (`packages/editor/src/worker.js`). It has its own
//! WebAssembly memory, bounded like the desktop worker process, so a runaway
//! parse cannot take the editor down. A request past its time limit,
//! cancelled or dropped terminates the worker, and the next request starts
//! another. Other jobs can share this worker once they move off the page.
use super::{Error, Identity, Record, local, reverse};
pub use crate::browser_worker::Cancel;
use crate::browser_worker::{self, Failure};
use web_time::Duration;

const JOB: &str = "naming";
const DEADLINE: Duration = Duration::from_secs(15);
const PARSE_CANCELLED: &str = "Local name parsing was canceled";
const GENERATE_CANCELLED: &str = "Local name generation was canceled";

pub fn set_worker(factory: js_sys::Function) {
    browser_worker::set_worker(factory);
}

pub fn worker_loading() -> bool {
    browser_worker::loading(JOB)
}

pub async fn resolve_name(name: &str, cancel: Cancel) -> Result<Record, Error> {
    let name = local::checked_name(name)?;
    let output = exchange(&local::parse_request(name)?, &cancel, PARSE_CANCELLED).await?;
    local::parse_result(name, &output)
}

/// Parse a generated name in the generation-verification mode, which also
/// admits salts and radicals.
pub(super) async fn resolve_generated(name: &str, cancel: Cancel) -> Result<Record, Error> {
    let output = exchange(
        &local::verification_request(name)?,
        &cancel,
        GENERATE_CANCELLED,
    )
    .await?;
    local::verification_result(name, &output)
}

/// As on the desktop, a name is returned only after the parser exactly
/// reconstructs the structure from it.
pub async fn generate_name(identity: &Identity, cancel: Cancel) -> Result<Record, Error> {
    generate_name_with_options(identity, super::IsotopeStyle::Iupac, cancel).await
}

pub async fn generate_name_with_options(
    identity: &Identity,
    style: super::IsotopeStyle,
    cancel: Cancel,
) -> Result<Record, Error> {
    let expected = super::canonical_smiles(&identity.smiles).map_err(Error::Rejected)?;
    let input = identity.generation_smiles(&expected);
    let ordinary = identity.naming_smiles.as_deref().unwrap_or(&expected);
    // As on the desktop, Ξ/ξ for undefined units falls back to the plain name.
    match generate_from(identity, input, &expected, style, &cancel).await {
        Err(error) if input != ordinary && !matches!(error, Error::Cancelled(_)) => {
            generate_from(identity, ordinary, &expected, style, &cancel).await
        }
        result => result,
    }
}

async fn generate_from(
    identity: &Identity,
    input: &str,
    expected: &str,
    style: super::IsotopeStyle,
    cancel: &Cancel,
) -> Result<Record, Error> {
    let request = reverse::generate_request_with_style(input, style)?;
    let output = exchange(&request, cancel, GENERATE_CANCELLED).await?;
    let candidate = reverse::parse_response(&output, input)?;
    reverse::reconstruct(identity, expected.to_owned(), candidate, cancel).await
}

async fn exchange(request: &[u8], cancel: &Cancel, cancelled: &str) -> Result<Vec<u8>, Error> {
    browser_worker::exchange(JOB, request, cancel, DEADLINE)
        .await
        .map_err(|error| match error {
            Failure::Cancelled => Error::Cancelled(cancelled.into()),
            Failure::Unavailable(message) | Failure::Failed(message) => Error::Worker(message),
        })
}
