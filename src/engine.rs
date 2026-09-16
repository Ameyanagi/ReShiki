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
    pub protocol: u32,
    pub operation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document: Option<Document>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
}
impl Request {
    pub fn import_smiles(text: &str) -> Self {
        Self::import("smiles", text)
    }
    pub fn import(format: &str, text: &str) -> Self {
        Self {
            protocol: 1,
            operation: "import".into(),
            document: None,
            text: Some(text.into()),
            format: Some(format.into()),
        }
    }
    pub fn molecule(operation: &str, document: Document) -> Self {
        Self {
            protocol: 1,
            operation: operation.into(),
            document: Some(document),
            text: None,
            format: None,
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
impl PythonEngine {
    async fn spawn() -> Result<Worker, String> {
        let bundled = std::env::current_exe().ok().and_then(|exe| {
            let contents = exe.parent()?.parent()?;
            let worker = contents.join("Resources/chemistry/moruno-engine");
            worker.is_file().then_some(worker)
        });
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
            let worker = slot.as_mut().unwrap();
            let id = worker.next_id;
            worker.next_id += 1;
            let mut message = serde_json::to_value(request).map_err(|e| e.to_string())?;
            message["id"] = id.into();
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
            if value["id"].as_u64() != Some(id) {
                return Err("Chemistry response ID mismatch".into());
            }
            if value["ok"] != true {
                return Err(value["error"]
                    .as_str()
                    .unwrap_or("Chemistry error")
                    .to_string());
            }
            let response: Response =
                serde_json::from_value(value["result"].clone()).map_err(|e| e.to_string())?;
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
