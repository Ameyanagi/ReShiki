//! `reshiki --cli name`: import, generate locally, then verify reconstruction.
use super::{
    args::{Input, Name},
    host::{CliError, Session, labelled},
};
use crate::naming::{self, NameStatus};
use serde_json::{Map, json};
use std::io::Write;

pub(crate) async fn run(
    name: Name,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<(), CliError> {
    let mut session = Session::new();
    let Input { source, format } = name.input;
    let text = session.text(source, &format)?;
    // Import cleanup can drop stereo classes that naming rejects, such as
    // allene stereo, so the raw SMILES (before any title) is checked first.
    if format == "smiles" {
        let smiles = text.split_whitespace().next().unwrap_or_default();
        naming::canonical_smiles(smiles).map_err(CliError::Failed)?;
    }
    let handle = session.import_text(text, format, err).await?;
    let document = session.document(handle)?;
    let identity = naming::document_identity(&document).map_err(CliError::Failed)?;
    let record = naming::generate_name(&identity, naming::Cancel::default())
        .await
        .map_err(|error| CliError::Failed(error.to_string()))?;
    if name.json || name.locants {
        let status = record.name_status.ok_or_else(|| {
            CliError::Failed("internal error: generated name has no status".into())
        })?;
        let assessment = json!(status);
        let mut value = Map::from_iter([
            ("name".into(), json!(record.title)),
            (
                "preferred".into(),
                json!(matches!(status, NameStatus::Preferred)),
            ),
            ("status".into(), json!(assessment.get("status"))),
            ("reason".into(), json!(assessment.get("reason"))),
            ("detail".into(), json!(status.detail())),
            (
                "alternatives".into(),
                record
                    .alternatives
                    .iter()
                    .map(|alternative| {
                        let assessment = json!(alternative.status);
                        json!({
                            "name": alternative.name,
                            "status": assessment.get("status"),
                            "reason": assessment.get("reason"),
                        })
                    })
                    .collect(),
            ),
        ]);
        if name.locants {
            let locants = record
                .locants
                .as_ref()
                .map(|locants| locants.for_document(&document))
                .unwrap_or_default();
            value.insert(
                "locants".into(),
                json!(
                    document
                        .atoms
                        .iter()
                        .map(|atom| locants.get(&atom.id))
                        .collect::<Vec<_>>()
                ),
            );
        }
        let value = Map::from_iter([("value".into(), value.into())]);
        writeln!(out, "{}", labelled(value)).map_err(CliError::Stdout)
    } else {
        writeln!(out, "{}", record.title).map_err(CliError::Stdout)
    }
}
