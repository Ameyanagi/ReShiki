//! The browser has no worker processes to bound the parsers in yet, so name
//! and structure conversion report that they are unavailable there.
use super::{Error, Identity, Record};

const UNAVAILABLE: &str = "Naming is not available in the browser yet";

#[derive(Debug, Clone, Default)]
pub struct Cancel {}
impl Cancel {
    pub fn stop(&self) {}
}

pub async fn resolve_name(_name: &str, _cancel: Cancel) -> Result<Record, Error> {
    Err(Error::Worker(UNAVAILABLE.into()))
}

pub async fn generate_name(_identity: &Identity, _cancel: Cancel) -> Result<Record, Error> {
    Err(Error::Worker(UNAVAILABLE.into()))
}
