//! Local naming for the agent's operation tools: import's `name` format and
//! the `name` tool, served by the same bounded workers as the app's naming.
use super::{
    Cancel, NameStatus, Record, canonical_smiles, document_identity, generate_name, resolve_name,
    structure_smiles, verify_identity,
};
use crate::document::Document;
use reshiki_agent::ops::naming::{Alternative, Named, Naming, NamingFuture, Resolved};
use serde_json::json;

/// The naming hook `reshiki --mcp` and `reshiki --cli` give their host.
pub struct AgentNaming;

impl Naming for AgentNaming {
    fn resolve(&self, name: String) -> NamingFuture<Resolved> {
        Box::pin(async move {
            let record = resolve_name(&name, Cancel::default())
                .await
                .map_err(|error| error.to_string())?;
            // As the app's name tool chooses them (src/app/naming.rs).
            let identity = if record.prepared.is_some() {
                record.canonical_smiles
            } else {
                canonical_smiles(&record.smiles)?
            };
            Ok(Resolved {
                smiles: record.smiles,
                prepared: record.prepared,
                identity,
                warnings: record.warnings,
            })
        })
    }

    fn verify(&self, identity: &str, doc: &Document) -> Result<(), String> {
        verify_identity(identity, &structure_smiles(doc)?)
    }

    fn generate(&self, doc: Document) -> NamingFuture<(Named, Vec<String>)> {
        Box::pin(async move {
            let identity = document_identity(&doc)?;
            let record = generate_name(&identity, Cancel::default())
                .await
                .map_err(|error| error.to_string())?;
            // The drawing's warnings: the record's, without its last entry,
            // which credits the generator on every name.
            Ok((named(&record)?, identity.warnings))
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

#[cfg(test)]
mod tests {
    use super::AgentNaming;
    use reshiki_agent::ops::{
        budget::Budgets,
        headless::HeadlessHost,
        host::{Call, ToolHost},
        result::ToolResult,
        wire::{Principal, RequestId},
    };
    use serde_json::{Value, json};
    use std::sync::Arc;

    async fn ok(host: &HeadlessHost, tool: &str, arguments: Value) -> ToolResult {
        let call = Call {
            principal: Principal::local(),
            request: RequestId::Int(1),
            tool: tool.into(),
            arguments,
            progress: None,
        };
        let result = host.call(call).await.unwrap();
        assert!(!result.is_error, "{tool}: {:?}", result.value);
        result
    }

    /// Imports `input` and names the drawing: the name and its warnings.
    async fn named(host: &HeadlessHost, input: Value) -> (Value, Value) {
        let imported = ok(host, "import", input).await;
        let document = imported.value["value"]["document"].clone();
        let named = ok(host, "name", json!({"document": document})).await;
        (
            named.value["value"]["name"].clone(),
            named.value["warnings"].clone(),
        )
    }

    fn host() -> HeadlessHost {
        HeadlessHost::new("9.8.7", Budgets::default()).with_naming(Arc::new(AgentNaming))
    }

    /// A name's axial configuration lives in the prepared graph, not in
    /// ordinary SMILES: import and convert both keep it, so the M and P names
    /// stay two structures.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn name_import_and_convert_keep_the_axis_of_binol() {
        let host = host();
        for name in [
            "(1M)-[1,1'-binaphthalene]-2,2'-diol",
            "(1P)-[1,1'-binaphthalene]-2,2'-diol",
        ] {
            let input = json!({"format": "name", "text": name});
            assert_eq!(named(&host, input.clone()).await.0, name, "import");
            let converted = ok(
                &host,
                "convert",
                json!({
                    "input": input,
                    "to": "mol",
                    "pages": null,
                    "svg_text": null,
                    "max_width": null,
                    "max_height": null,
                }),
            )
            .await;
            let mol = String::from_utf8(converted.files[0].bytes.clone()).unwrap();
            let back = json!({"format": "mol", "text": mol});
            assert_eq!(named(&host, back).await.0, name, "convert");
        }
    }

    /// The parser's warnings and the named drawing's reach the result envelope.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn naming_warnings_reach_the_envelope() {
        let host = host();
        let rotation = json!([{"message": "Rotation sign (+) ignored; structure from (R)"}]);
        let input = json!({"format": "name", "text": "(R)-(+)-lactic acid"});
        let imported = ok(&host, "import", input.clone()).await;
        assert_eq!(imported.value["warnings"], rotation);
        let converted = ok(
            &host,
            "convert",
            json!({
                "input": input,
                "to": "inchi",
                "pages": null,
                "svg_text": null,
                "max_width": null,
                "max_height": null,
            }),
        )
        .await;
        assert_eq!(converted.value["warnings"], rotation);

        // A stereocentre drawn without a configuration, and with one.
        let lactic = |text: &str| json!({"format": "smiles", "text": text});
        assert_eq!(
            named(&host, lactic("CC(O)C(=O)O")).await,
            (
                json!("2-hydroxypropanoic acid"),
                json!([{"message": "C2 has no configuration; the name leaves it unspecified"}])
            )
        );
        assert_eq!(
            named(&host, lactic("C[C@H](O)C(=O)O")).await,
            (json!("(2S)-2-hydroxypropanoic acid"), json!([]))
        );
    }
}
