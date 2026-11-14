//! Local naming for the agent's operation tools: import's `name` format and
//! the `name` tool, served by the same bounded workers as the app's naming.
use super::{Cancel, NameStatus, Record, document_identity, generate_name, resolve_name};
use crate::document::Document;
use reshiki_agent::ops::naming::{Alternative, Named, Naming, NamingFuture};
use serde_json::json;

/// The naming hook `reshiki --mcp` and `reshiki --cli` give their host.
pub struct AgentNaming;

impl Naming for AgentNaming {
    fn resolve(&self, name: String) -> NamingFuture<String> {
        Box::pin(async move {
            resolve_name(&name, Cancel::default())
                .await
                .map(|record| record.smiles)
                .map_err(|error| error.to_string())
        })
    }

    fn generate(&self, doc: Document) -> NamingFuture<Named> {
        Box::pin(async move {
            let identity = document_identity(&doc)?;
            let record = generate_name(&identity, Cancel::default())
                .await
                .map_err(|error| error.to_string())?;
            named(&record)
        })
    }
}

/// `status` as its wire name and the rule family behind it.
fn assessment(status: NameStatus) -> (String, Option<String>) {
    let value = json!(status);
    let text = |field: &str| value.get(field).and_then(|v| v.as_str()).map(str::to_owned);
    (text("status").unwrap_or_default(), text("reason"))
}

/// A generated name as the `name` tool and `reshiki --cli name --json`
/// report it.
pub fn named(record: &Record) -> Result<Named, String> {
    let status = record
        .name_status
        .ok_or("internal error: generated name has no status")?;
    let (label, reason) = assessment(status);
    Ok(Named {
        name: record.title.clone(),
        status: label,
        preferred: matches!(status, NameStatus::Preferred),
        reason,
        detail: status.detail().to_owned(),
        aliases: record
            .synonyms
            .iter()
            .map(|alias| alias.name.clone())
            .collect(),
        alternatives: record
            .alternatives
            .iter()
            .map(|alternative| {
                let (status, reason) = assessment(alternative.status);
                Alternative {
                    name: alternative.name.clone(),
                    status,
                    reason,
                }
            })
            .collect(),
    })
}
