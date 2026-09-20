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

/// The editor's contract. A future Rust engine implements this same interface.
pub trait ChemistryEngine: Send + Sync {
    fn execute(
        &self,
        request: Request,
    ) -> impl std::future::Future<Output = Result<Response, String>> + Send;
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
}
fn bundled_worker(executable: &std::path::Path) -> Option<PathBuf> {
    let directory = executable.parent()?;
    let worker = if cfg!(target_os = "macos") {
        directory
            .parent()?
            .join("Resources/chemistry/moruno-engine")
    } else {
        directory.join("chemistry").join(if cfg!(windows) {
            "moruno-engine.exe"
        } else {
            "moruno-engine"
        })
    };
    worker.is_file().then_some(worker)
}

impl PythonEngine {
    async fn spawn() -> Result<Worker, String> {
        let bundled = std::env::current_exe()
            .ok()
            .and_then(|exe| bundled_worker(&exe));
        let root = std::env::var_os("MORUNO_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")));
        let default_python = if cfg!(windows) {
            root.join(".venv/Scripts/python.exe")
        } else {
            root.join(".venv/bin/python")
        };
        let python = std::env::var_os("MORUNO_PYTHON")
            .map(PathBuf::from)
            .unwrap_or(default_python);
        let mut command = if let Some(executable) =
            bundled.filter(|_| std::env::var_os("MORUNO_PYTHON").is_none())
        {
            Command::new(executable)
        } else {
            let mut command = Command::new(&python);
            command.arg("-u").arg(root.join("engine/worker.py"));
            command
        };
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
        let mut slot = self.worker.lock().await;
        if slot.is_none() {
            *slot = Some(Self::spawn().await?);
        }
        let result = tokio::time::timeout(Duration::from_secs(30), async {
            let worker = slot.as_mut().ok_or("Chemistry worker is unavailable")?;
            let id = worker.next_id;
            worker.next_id = worker
                .next_id
                .checked_add(1)
                .ok_or("Chemistry request counter exhausted; retry to restart the worker")?;
            let mut message = serde_json::to_value(request).map_err(|e| e.to_string())?;
            message
                .as_object_mut()
                .ok_or("Invalid chemistry request envelope")?
                .insert("id".into(), id.into());
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
            let response: Response = serde_json::from_value(
                value
                    .get("result")
                    .ok_or("Missing chemistry result")?
                    .clone(),
            )
            .map_err(|e| e.to_string())?;
            if let Some(doc) = &response.document {
                doc.validate()?;
            }
            Ok(response)
        })
        .await
        .unwrap_or_else(|_| Err("Chemistry operation timed out after 30 seconds".into()));
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

#[cfg(test)]
mod packaging_tests {
    use super::bundled_worker;

    #[test]
    fn discovers_the_bundled_worker_in_a_relocated_directory() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("Moruno with spaces");
        let (exe, worker) = if cfg!(target_os = "macos") {
            (
                root.join("Contents/MacOS/moruno"),
                root.join("Contents/Resources/chemistry/moruno-engine"),
            )
        } else if cfg!(windows) {
            (
                root.join("Moruno.exe"),
                root.join("chemistry/moruno-engine.exe"),
            )
        } else {
            (root.join("moruno"), root.join("chemistry/moruno-engine"))
        };
        assert_eq!(bundled_worker(&exe), None);
        std::fs::create_dir_all(worker.parent().unwrap()).unwrap();
        std::fs::write(&worker, b"fixture").unwrap();
        assert_eq!(bundled_worker(&exe), Some(worker));
    }
}
