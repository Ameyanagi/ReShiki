use crate::document::Document;
use serde::{Deserialize, Serialize};
pub mod native_aromatic;
pub mod native_cleanup;
pub mod native_import;
pub mod native_response;
#[cfg(feature = "rdkit-reference")]
mod reference;
#[cfg(feature = "rdkit-reference")]
pub use reference::PythonEngine;

#[derive(Debug, Clone, Serialize)]
pub struct Request {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cleanup: Option<crate::cleanup::Options>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_ids: Option<Vec<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub atom_indicators: Option<Vec<crate::atom_labels::Indicator>>,
    pub protocol: u32,
    pub operation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document: Option<Document>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_layout: Option<std::collections::HashMap<u64, TextMetrics>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graphic_paths: Option<std::collections::HashMap<u64, Vec<crate::graphics::PathCommand>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graphic_parts: Option<std::collections::HashMap<u64, Vec<crate::scientific::Part>>>,
}
#[derive(Debug, Clone, Serialize)]
pub struct TextMetrics {
    pub width: f32,
    pub height: f32,
    pub baseline: f32,
}
impl Request {
    pub fn import_smiles(text: &str) -> Self {
        Self::import("smiles", text)
    }
    pub fn import(format: &str, text: &str) -> Self {
        Self {
            cleanup: None,
            selected_ids: None,
            atom_indicators: None,
            protocol: 1,
            operation: "import".into(),
            document: None,
            text: Some(text.into()),
            format: Some(format.into()),
            text_layout: None,
            graphic_paths: None,
            graphic_parts: None,
        }
    }
    pub fn molecule(operation: &str, document: Document) -> Self {
        let graphic_parts = (operation == "export").then(|| {
            document
                .graphics
                .iter()
                .filter(|g| {
                    matches!(
                        g.kind,
                        crate::graphics::GraphicKind::Symbol(_)
                            | crate::graphics::GraphicKind::Orbital(_)
                    )
                })
                .map(|g| (g.id, g.parts()))
                .collect()
        });
        let graphic_paths = (operation == "export").then(|| {
            document
                .graphics
                .iter()
                .map(|g| (g.id, g.commands()))
                .collect()
        });
        // Exchange uses the renderer's measured text to keep centered and
        // right-aligned captions in place.
        let text_layout = (operation == "export").then(|| {
            document
                .annotations
                .iter()
                .map(|a| {
                    let layout = crate::typography::layout(&a.text, &a.format);
                    let scale = crate::style::DEFAULT.points_per_world();
                    let baseline = layout
                        .fragments
                        .first()
                        .map(|run| run.position.y + crate::style::text_ascent(&run.style))
                        .unwrap_or_else(|| crate::style::text_ascent(&a.format.style));
                    (
                        a.id,
                        TextMetrics {
                            width: layout.width * scale,
                            height: layout.height * scale,
                            baseline: baseline * scale,
                        },
                    )
                })
                .collect()
        });
        Self {
            cleanup: None,
            selected_ids: None,
            atom_indicators: (operation == "export")
                .then(|| crate::atom_labels::indicators(&document)),
            protocol: 1,
            operation: operation.into(),
            document: Some(document),
            text: None,
            format: None,
            text_layout,
            graphic_paths,
            graphic_parts,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Analysis {
    pub smiles: String,
    pub formula: String,
    pub mass: f64,
    pub exact_mass: f64,
    pub logp: f64,
    pub tpsa: f64,
    pub donors: u32,
    pub acceptors: u32,
    pub rings: u32,
    #[serde(default)]
    pub unpaired_electrons: u32,
    pub inchi: String,
    pub inchikey: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub document: Option<Document>,
    pub analysis: Option<Analysis>,
    pub output: Option<String>,
    pub engine_version: String,
    #[serde(default)]
    pub warnings: Vec<String>,
}

/// Shared contract for native operations and optional custom chemistry backends.
pub trait ChemistryEngine: Send + Sync {
    fn execute(
        &self,
        request: Request,
    ) -> impl std::future::Future<Output = Result<Response, String>> + Send;
}

/// Molecular response backend used after local drawing operations are dispatched.
#[derive(Clone, Copy, Debug, Default)]
pub struct NativeBackend;

impl ChemistryEngine for NativeBackend {
    async fn execute(&self, request: Request) -> Result<Response, String> {
        native_response::execute(request)
            .await
            .map_err(|error| error.to_string())
    }
}

/// Native drawing and chemistry operations behind a checked request interface.
/// `with_backend` retains local exchange conversions for custom backends.
#[derive(Clone)]
pub struct LocalEngine<B = NativeBackend> {
    chemistry: B,
    native_responses: bool,
}

impl Default for LocalEngine<NativeBackend> {
    fn default() -> Self {
        Self {
            native_responses: true,
            chemistry: NativeBackend,
        }
    }
}

impl<B: ChemistryEngine> LocalEngine<B> {
    pub fn with_backend(chemistry: B) -> Self {
        Self {
            chemistry,
            native_responses: false,
        }
    }

    pub async fn request(&self, request: Request) -> Result<Response, String> {
        self.execute(request).await
    }
}

impl<B: ChemistryEngine> ChemistryEngine for LocalEngine<B> {
    async fn execute(&self, mut request: Request) -> Result<Response, String> {
        use base64::{Engine, engine::general_purpose::STANDARD};
        if request.protocol != 1 {
            return Err("Unsupported protocol version".into());
        }
        if self.native_responses && request.operation == "clean" {
            return native_cleanup::execute(request, None)
                .await
                .map_err(|error| error.to_string());
        }
        if self.native_responses && request.operation == "aromatic" {
            return native_aromatic::execute(request, None)
                .await
                .map_err(|error| error.to_string());
        }
        if request.operation == "abbreviate" {
            let document = request
                .document
                .take()
                .ok_or("Missing abbreviation drawing")?;
            let selected = request.selected_ids.clone().unwrap_or_default();
            let label = request.text.clone();
            let replace = request.format.as_deref() == Some("replace");
            request.document = Some(
                tokio::task::spawn_blocking(move || {
                    use crate::chemistry::{abbreviations, document as chemistry};
                    document.validate()?;
                    let policy = abbreviations::AttachmentPolicy::SharedAnchor;
                    abbreviations::validate_with_policy(&document, policy)
                        .map_err(|e| e.to_string())?;
                    let molecule = chemistry::prepare(&document).map_err(|e| e.to_string())?;
                    if replace {
                        abbreviations::replace_with_policy(
                            &document,
                            &selected,
                            label.as_deref().unwrap_or(""),
                            policy,
                        )
                        .map_err(|e| e.to_string())
                    } else {
                        abbreviations::find_with_policy(
                            &document,
                            &molecule,
                            &selected,
                            label.as_deref(),
                            policy,
                        )
                        .map_err(|e| e.to_string())
                    }
                })
                .await
                .map_err(|e| format!("Abbreviation edit failed: {e}"))??,
            );
            // Reuse drawing reconstruction and analysis for the edited snapshot.
            // Unlike Analyze, the original abbreviation operation accepts an empty drawing.
            request.operation = "finish_abbreviation".into();
        }
        if self.native_responses
            && (matches!(
                request.operation.as_str(),
                "analyze" | "finish_abbreviation"
            ) || request.operation == "export"
                && matches!(
                    request.format.as_deref(),
                    Some("smiles" | "mol" | "inchi" | "cdxml" | "cdx")
                ))
        {
            return native_response::execute(request)
                .await
                .map_err(|e| e.to_string());
        }
        if request.operation == "export"
            && let Some(format @ ("rxn" | "rsmi")) = request.format.as_deref()
        {
            let reaction_smiles = format == "rsmi";
            use crate::chemistry::reaction;
            let document = request.document.clone().ok_or("Missing reaction drawing")?;
            let selected = request.selected_ids.clone();
            let output = tokio::task::spawn_blocking(move || {
                if reaction_smiles {
                    reaction::write_smiles(&document, selected.as_deref())
                } else {
                    reaction::write_rxn(&document, selected.as_deref())
                }
            })
            .await
            .map_err(|e| format!("Reaction export failed: {e}"))?;
            match output {
                Ok(output) => {
                    return Ok(Response {
                        document: None,
                        analysis: None,
                        output: Some(output),
                        engine_version: crate::chemistry::RDKIT_VERSION.into(),
                        warnings: vec![reaction::EXPORT_WARNING.into()],
                    });
                }
                Err(error) => return Err(error.to_string()),
            }
        }
        if self.native_responses && request.operation == "import" {
            return match native_import::execute(request, None)
                .await
                .map_err(|e| e.to_string())?
            {
                native_import::Outcome::Complete(response) => Ok(*response),
                native_import::Outcome::Deferred(_) => {
                    Err("Native import unexpectedly deferred an import operation".into())
                }
            };
        }
        let binary = request.format.as_deref() == Some("cdx");
        let export_binary = binary && request.operation == "export";
        if binary && request.operation == "import" {
            let text = request.text.take().unwrap_or_default();
            request.text = Some(
                tokio::task::spawn_blocking(move || {
                    if text.trim().is_empty() {
                        return Err("Enter a structure first".into());
                    }
                    if text.len() > crate::exchange::LIMIT.div_ceil(3) * 4 {
                        return Err("Drawing exceeds the 16 MB structure limit".into());
                    }
                    let bytes = STANDARD
                        .decode(text)
                        .map_err(|_| "Invalid binary drawing base64")?;
                    crate::exchange::from_cdx(&bytes)
                })
                .await
                .map_err(|e| format!("Drawing conversion failed: {e}"))??,
            );
            request.format = Some("cdxml".into());
        } else if export_binary {
            request.format = Some("cdxml".into());
        }
        let mut response = self.chemistry.execute(request).await?;
        if export_binary {
            let xml = response.output.take().ok_or("Missing exported drawing")?;
            response.output = Some(
                tokio::task::spawn_blocking(move || {
                    crate::exchange::to_cdx(&xml).map(|bytes| STANDARD.encode(bytes))
                })
                .await
                .map_err(|e| format!("Drawing conversion failed: {e}"))??,
            );
        }
        Ok(response)
    }
}

#[cfg(test)]
mod tests;
