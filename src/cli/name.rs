//! `reshiki --cli name`: import, generate locally, then verify reconstruction.
use super::{
    args::{Input, Name},
    host::{CliError, Session, labelled},
};
use crate::naming::{self, NameStatus};
use serde_json::{Map, Value, json};
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
    if format == "smiles" || format == "auto" && crate::engine::text_format(&text) == "smiles" {
        let smiles = text.split_whitespace().next().unwrap_or_default();
        naming::canonical_smiles(smiles).map_err(CliError::Failed)?;
    }
    let mut imported = session
        .call("import", json!({"format": format, "text": text}), err)
        .await?
        .value;
    let handle = imported
        .get("value")
        .and_then(|value| value.get("document"))
        .cloned()
        .unwrap_or_default();
    let document = session.document(handle)?;
    let identity = naming::document_identity(&document).map_err(CliError::Failed)?;
    let record = naming::generate_name_with_options(
        &identity,
        name.isotope_style,
        naming::Cancel::default(),
    )
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
                "aliases".into(),
                json!(
                    record
                        .synonyms
                        .iter()
                        .map(|alias| &alias.name)
                        .collect::<Vec<_>>()
                ),
            ),
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
        // The import's warnings and versions, as analyze reports its own.
        let mut field = |name: &str| (name.to_owned(), imported.remove(name).unwrap_or_default());
        let result = Map::from_iter([
            ("value".into(), Value::Object(value)),
            field("warnings"),
            field("versions"),
        ]);
        writeln!(out, "{}", labelled(result)).map_err(CliError::Stdout)
    } else {
        writeln!(out, "{}", record.title).map_err(CliError::Stdout)
    }
}
