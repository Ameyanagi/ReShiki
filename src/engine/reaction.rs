//! Reaction parsing, chemical preparation and canvas assembly stay in Rust.
//! The temporary bridge supplies missing layouts, full CIP labels and identifiers.
use super::{PythonEngine, Response};
use crate::chemistry::{RDKIT_VERSION, document, molfile, reaction, rings::RingError, sanitize};
use serde::Deserialize;

impl PythonEngine {
    /// `None` retains the existing fallback for unresolved dense-ring ordering.
    pub(super) async fn import_reaction(
        &self,
        text: String,
        format: &str,
    ) -> Result<Option<Response>, String> {
        let draft = if format == "rsmi" {
            self.prepare_reaction_smiles(text).await?
        } else {
            tokio::task::spawn_blocking(move || match reaction::read_rxn(&text) {
                Ok(imported) => imported.drawing().map(Some).map_err(|e| e.to_string()),
                Err(molfile::ReadError::Sanitization(sanitize::Error {
                    cause: sanitize::Cause::Rings(RingError::UnresolvedOrdering),
                    ..
                })) => Ok(None),
                Err(error) => Err(error.to_string()),
            })
            .await
            .map_err(|e| format!("Reaction import failed: {e}"))??
        };
        let Some(draft) = draft else {
            return Ok(None);
        };
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Labels {
            rdkit_version: String,
            labels: Vec<document::Labels>,
        }
        let parts: Vec<_> = draft
            .participants()
            .map(|(molecule, file)| serde_json::json!({ "molecule": molecule, "file": file }))
            .collect();
        let labeled: Labels = serde_json::from_value(
            self.exchange(serde_json::json!({
                "protocol": 1,
                "operation": "label_reaction",
                "format": format,
                "prepared_parts": parts,
            }))
            .await?,
        )
        .map_err(|e| format!("Invalid reaction labels: {e}"))?;
        if labeled.rdkit_version != RDKIT_VERSION {
            return Err("Reaction labeling version changed".into());
        }
        // Full CIP labeling can change double-bond control atoms. Prepare the
        // combined analysis graph only after those labels finish the drawing.
        let drawing = tokio::task::spawn_blocking(move || {
            let doc = draft.finish(labeled.labels).map_err(|e| e.to_string())?;
            match document::prepare(&doc) {
                Ok(molecule) => Ok(Some((doc, molecule))),
                Err(document::Error::Sanitization(sanitize::Error {
                    cause: sanitize::Cause::Rings(RingError::UnresolvedOrdering),
                    ..
                })) => Ok(None),
                Err(error) => Err(error.to_string()),
            }
        })
        .await
        .map_err(|e| format!("Reaction drawing preparation failed: {e}"))??;
        let Some((doc, molecule)) = drawing else {
            return Ok(None);
        };
        let mut result = self
            .exchange(serde_json::json!({
                "protocol": 1,
                "operation": "import",
                "format": format,
                "prepared_reaction": true,
                "document": doc,
                "prepared_molecule": molecule,
                "local_properties": self.local_properties,
            }))
            .await?;
        let properties = self.local_properties;
        tokio::task::spawn_blocking(move || {
            if properties {
                crate::chemistry::complete_analysis(&mut result)?;
            }
            let mut response: Response =
                serde_json::from_value(result).map_err(|e| e.to_string())?;
            if response.engine_version != RDKIT_VERSION {
                return Err("Reaction analysis version changed".into());
            }
            response.document = Some(doc);
            Ok(Some(response))
        })
        .await
        .map_err(|e| format!("Reaction completion failed: {e}"))?
    }

    async fn prepare_reaction_smiles(
        &self,
        text: String,
    ) -> Result<Option<reaction::Drawing>, String> {
        let layout = tokio::task::spawn_blocking(move || match reaction::read_smiles(&text) {
            Ok(source) => source.layout().map(Some).map_err(|e| e.to_string()),
            Err(reaction::SmilesError::Sanitization(sanitize::Error {
                cause: sanitize::Cause::Rings(RingError::UnresolvedOrdering),
                ..
            })) => Ok(None),
            Err(error) => Err(error.to_string()),
        })
        .await
        .map_err(|e| format!("Reaction SMILES import failed: {e}"))??;
        let Some(layout) = layout else {
            return Ok(None);
        };
        let parts: Vec<_> = layout.requests().collect();
        let positions = if parts.is_empty() {
            Vec::new()
        } else {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Positions {
                rdkit_version: String,
                positions: Vec<Vec<crate::chemistry::stereo::Point3>>,
            }
            let reply: Positions = serde_json::from_value(
                self.exchange(serde_json::json!({
                    "protocol": 1,
                    "operation": "layout_reaction",
                    "format": "rsmi",
                    "prepared_parts": parts,
                }))
                .await?,
            )
            .map_err(|e| format!("Invalid reaction layout: {e}"))?;
            if reply.rdkit_version != RDKIT_VERSION {
                return Err("Reaction layout version changed".into());
            }
            reply.positions
        };
        tokio::task::spawn_blocking(move || {
            layout
                .finish(positions)
                .map(Some)
                .map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| format!("Reaction drawing failed: {e}"))?
    }
}
