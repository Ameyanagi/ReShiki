//! Windows/Linux desktop handoff and long-lived Office launcher proxies.
#[cfg(target_os = "linux")]
use reshiki_linux::desktop as native;
#[cfg(windows)]
use reshiki_windows::desktop as native;
mod admission;
mod client;
mod launch;
pub(crate) mod protocol;
mod server;

pub(crate) use admission::Admission;
pub(crate) use launch::{LaunchOutcome, launch};
pub(crate) use server::GuiHost;

use crate::session::Binding;
use std::{sync::Arc, time::Duration};

const HANDOFF: Duration = Duration::from_secs(2);
const UI_REPLY: Duration = Duration::from_millis(750);
// Ordering: client deadlines that wait for a UI answer (HANDOFF and
// PROXY_CALL_TIMEOUT) exceed UI_REPLY, the server's wait for that answer.
// PREPARE_EXPIRY exceeds HANDOFF, so an uncommitted prepare outlives the
// proxy's commit window. ACCEPT_POLL also bounds the quick probes (first open
// attempt, abandon, ack), which do not wait for the UI.
const PREPARE_EXPIRY: Duration = Duration::from_secs(5);
const ACCEPT_POLL: Duration = Duration::from_millis(100);
const BUSY_BACKOFF: Duration = Duration::from_millis(10);
const RETRY_INTERVAL: Duration = Duration::from_millis(40);
const PROXY_CALL_TIMEOUT: Duration = Duration::from_secs(1);
const WATCH_INTERVAL: Duration = Duration::from_millis(250);
const LEASE_LIMIT: usize = 32;
const REQUEST_LIMIT: usize = 64;

#[derive(Debug, Clone)]
pub(crate) enum Event {
    Open(Vec<std::path::PathBuf>, Arc<Admission>),
    Prepare(Binding, Arc<Admission>),
    Commit(Binding),
}

/// Save As and ordinary saves cannot overwrite a path owned by an Office tab
/// in this desktop session. Called and dropped inside the same blocking worker.
pub(crate) fn write_target(path: &std::path::Path) -> Result<native::Election, String> {
    native::Endpoint::current()
        .and_then(|endpoint| endpoint.write_claim(path))
        .map_err(|error| format!("Could not protect the save destination: {error}"))?
        .ok_or_else(|| {
            "This file belongs to an active Office edit session; choose another file".into()
        })
}

#[cfg(test)]
mod tests;
