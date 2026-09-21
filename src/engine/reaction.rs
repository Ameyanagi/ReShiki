//! RXN import keeps parsing, chemical preparation and canvas assembly in Rust.
//! The temporary bridge only assigns full CIP labels and canonical identifiers.
use super::{PythonEngine, Response};
use crate::chemistry::{RDKIT_VERSION, document, molfile, reaction, rings::RingError, sanitize};
use serde::Deserialize;

impl PythonEngine {
    /// `None` retains the existing fallback for unresolved dense-ring ordering.
    pub(super) async fn import_rxn(&self, text: String) -> Result<Option<Response>, String> {
        let draft = tokio::task::spawn_blocking(move || match reaction::read_rxn(&text) {
            Ok(imported) => imported.drawing().map(Some).map_err(|e| e.to_string()),
            Err(molfile::ReadError::Sanitization(sanitize::Error {
                cause: sanitize::Cause::Rings(RingError::UnresolvedOrdering),
                ..
            })) => Ok(None),
            Err(error) => Err(error.to_string()),
        })
        .await
        .map_err(|e| format!("Reaction import failed: {e}"))??;
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
                "format": "rxn",
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
                "format": "rxn",
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
}
