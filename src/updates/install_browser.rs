//! The browser build has no installer; the host page serves new releases.
use super::Release;
use std::{path::PathBuf, sync::Arc};

#[derive(Debug)]
pub struct Prepared;
#[derive(Debug, Clone)]
pub struct Progress(pub String);

const UNAVAILABLE: &str = "The browser editor updates with the page that hosts it.";

pub async fn prepare(
    _release: Release,
    _progress: tokio::sync::mpsc::Sender<Progress>,
) -> Result<Arc<Prepared>, String> {
    Err(UNAVAILABLE.into())
}

pub async fn handoff(_prepared: Arc<Prepared>, _drawings: Vec<PathBuf>) -> Result<(), String> {
    Err(UNAVAILABLE.into())
}
