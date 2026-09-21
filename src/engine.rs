use crate::document::Document;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, process::Stdio, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::Mutex,
};

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
        // Exchange uses the renderer's measured text, avoiding a second font
        // engine in Python and keeping centered/right-aligned captions in place.
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

/// Shared contract for local Rust operations and the remaining chemistry backend.
pub trait ChemistryEngine: Send + Sync {
    fn execute(
        &self,
        request: Request,
    ) -> impl std::future::Future<Output = Result<Response, String>> + Send;
}

/// Local operations are migrated here one at a time; chemistry retains the
/// same checked request/response interface and can use a different backend.
#[derive(Clone)]
pub struct LocalEngine<B = PythonEngine> {
    chemistry: B,
}

impl Default for LocalEngine<PythonEngine> {
    fn default() -> Self {
        Self {
            chemistry: PythonEngine {
                local_properties: true,
                local_pictures: true,
                local_documents: true,
                ..PythonEngine::default()
            },
        }
    }
}

impl<B: ChemistryEngine> LocalEngine<B> {
    pub fn with_backend(chemistry: B) -> Self {
        Self { chemistry }
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

struct Worker {
    _child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    next_id: u64,
}
#[derive(Clone, Default)]
pub struct PythonEngine {
    worker: Arc<Mutex<Option<Worker>>>,
    // Default false keeps an independent reference backend for differential tests.
    local_properties: bool,
    local_pictures: bool,
    local_documents: bool,
}
impl PythonEngine {
    async fn spawn() -> Result<Worker, String> {
        let packaged = std::env::current_exe()
            .ok()
            .and_then(|exe| crate::python_runtime::packaged_project(&exe));
        let root = packaged.clone().unwrap_or_else(|| {
            crate::compatibility::environment("ROOT")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")))
        });
        let python = if let Some(path) = crate::compatibility::environment("PYTHON") {
            PathBuf::from(path)
        } else if let Some(project) = &packaged {
            crate::python_runtime::prepare(project).await?
        } else {
            root.join(if cfg!(windows) {
                ".venv/Scripts/python.exe"
            } else {
                ".venv/bin/python"
            })
        };
        let mut command = Command::new(python);
        command
            .arg("-u")
            .arg(root.join("engine/worker.py"))
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .env("PYTHONUTF8", "1");
        // A GUI launch on Windows must not open a console for the local worker.
        #[cfg(windows)]
        command.creation_flags(0x08000000);
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| {
                format!(
                    "Could not start chemistry worker: {e}. Run `uv sync` in the project directory."
                )
            })?;
        Ok(Worker {
            input: child.stdin.take().ok_or("Missing worker input")?,
            output: BufReader::new(child.stdout.take().ok_or("Missing worker output")?),
            _child: child,
            next_id: 1,
        })
    }
    pub async fn request(&self, request: Request) -> Result<Response, String> {
        if let Some(doc) = &request.document {
            doc.validate()?;
        }
        let prepared_molecule = if self.local_documents
            && (request.operation == "analyze"
                || request.operation == "export"
                    && matches!(request.format.as_deref(), Some("smiles" | "mol" | "inchi")))
            && let Some(document) = request.document.clone()
        {
            tokio::task::spawn_blocking(move || {
                use crate::chemistry::{document, rings::RingError, sanitize};
                match document::prepare(&document) {
                    Ok(molecule) => Ok(Some(molecule)),
                    // Preserve the existing native fallback only for the known
                    // platform-dependent ring tie; never hide a chemistry error.
                    Err(document::Error::Sanitization(sanitize::Error {
                        cause: sanitize::Cause::Rings(RingError::UnresolvedOrdering),
                        ..
                    })) => Ok(None),
                    Err(error) => Err(error),
                }
            })
            .await
            .map_err(|e| format!("Molecule preparation failed: {e}"))?
            .map_err(|e| e.to_string())?
        } else {
            None
        };
        let picture_exports = if self.local_pictures
            && request.operation == "export"
            && matches!(request.format.as_deref(), Some("cdxml" | "cdx"))
            && let Some(document) = request.document.clone()
        {
            Some(
                tokio::task::spawn_blocking(move || {
                    crate::pictures::exchange::prepare_exports(&document)
                })
                .await
                .map_err(|e| format!("Picture preparation failed: {e}"))??,
            )
        } else {
            None
        };
        let mut slot = self.worker.lock().await;
        let starting = slot.is_none();
        if starting {
            *slot = Some(Self::spawn().await?);
        }
        // A newly installed RDKit environment may need additional time for the
        // operating system to load and validate its native libraries once.
        let timeout_seconds = if starting { 120 } else { 30 };
        let result = tokio::time::timeout(Duration::from_secs(timeout_seconds), async {
            let worker = slot.as_mut().ok_or("Chemistry worker is unavailable")?;
            let id = worker.next_id;
            worker.next_id = worker
                .next_id
                .checked_add(1)
                .ok_or("Chemistry request counter exhausted; retry to restart the worker")?;
            let mut message = serde_json::to_value(request).map_err(|e| e.to_string())?;
            let envelope = message
                .as_object_mut()
                .ok_or("Invalid chemistry request envelope")?;
            envelope.insert("id".into(), id.into());
            if let Some(molecule) = prepared_molecule {
                envelope.insert(
                    "prepared_molecule".into(),
                    serde_json::to_value(molecule).map_err(|e| e.to_string())?,
                );
            }
            if self.local_properties {
                envelope.insert("local_properties".into(), true.into());
            }
            if self.local_pictures {
                envelope.insert("local_pictures".into(), true.into());
                if let Some(exports) = picture_exports {
                    envelope.insert(
                        "picture_exports".into(),
                        serde_json::to_value(exports).map_err(|e| e.to_string())?,
                    );
                }
            }
            let mut bytes = serde_json::to_vec(&message).map_err(|e| e.to_string())?;
            bytes.push(b'\n');
            worker
                .input
                .write_all(&bytes)
                .await
                .map_err(|e| e.to_string())?;
            worker.input.flush().await.map_err(|e| e.to_string())?;
            let mut line = String::new();
            if worker
                .output
                .read_line(&mut line)
                .await
                .map_err(|e| e.to_string())?
                == 0
            {
                return Err("Chemistry worker exited unexpectedly".into());
            }
            let value: serde_json::Value =
                serde_json::from_str(&line).map_err(|e| e.to_string())?;
            if value.get("id").and_then(serde_json::Value::as_u64) != Some(id) {
                return Err("Chemistry response ID mismatch".into());
            }
            if value.get("ok").and_then(serde_json::Value::as_bool) != Some(true) {
                return Err(value
                    .get("error")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("Chemistry error")
                    .to_string());
            }
            let mut result = value
                .get("result")
                .ok_or("Missing chemistry result")?
                .clone();
            if self.local_properties || self.local_pictures {
                let (properties, pictures) = (self.local_properties, self.local_pictures);
                result = tokio::task::spawn_blocking(move || {
                    if properties {
                        crate::chemistry::complete_analysis(&mut result)?;
                    }
                    if pictures {
                        crate::pictures::exchange::complete_imports(result)
                    } else {
                        Ok(result)
                    }
                })
                .await
                .map_err(|e| format!("Local chemistry completion failed: {e}"))??;
            }
            let response: Response = serde_json::from_value(result).map_err(|e| e.to_string())?;
            if let Some(doc) = &response.document {
                doc.validate()?;
            }
            Ok(response)
        })
        .await
        .unwrap_or_else(|_| {
            Err(format!(
                "Chemistry operation timed out after {timeout_seconds} seconds"
            ))
        });
        // Reset also on chemistry errors: the next request always has a clean stream.
        if result.is_err() {
            *slot = None;
        }
        result
    }
}
impl ChemistryEngine for PythonEngine {
    async fn execute(&self, request: Request) -> Result<Response, String> {
        self.request(request).await
    }
}
