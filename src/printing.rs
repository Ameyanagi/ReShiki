//! Snapshot-based printing. Native UI runs in a separate process; document data
//! remains owned by the editor and temporary PDFs live only for the print job.
use crate::{
    document::{Document, Point},
    pages::Layout,
    style::DEFAULT as STYLE,
};
use serde::{Deserialize, Serialize};
use std::{io::Write, path::PathBuf, process::Stdio, sync::Arc};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Document,
    Selection,
}
#[derive(Debug, Clone)]
pub struct Prepared {
    pub pdf: Arc<Vec<u8>>,
    pub title: String,
}
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct Outcome {
    pub completed: bool,
}
#[derive(Serialize)]
struct Request<'a> {
    path: &'a std::path::Path,
    title: &'a str,
}

pub fn available() -> bool {
    cfg!(target_os = "macos")
}

/// A non-mutating snapshot; an unpaged drawing uses a centered A4 sheet.
/// Selection printing uses one sheet with the document's paper and margins.
pub fn snapshot(doc: &Document, ids: &[u64], scope: Scope) -> Result<Document, String> {
    doc.validate()?;
    let selection = scope == Scope::Selection;
    if selection && (ids.is_empty() || ids.iter().any(|id| !doc.all_ids().contains(id))) {
        return Err("Select drawing objects to print.".into());
    }
    let mut result = if selection {
        crate::editing::selection(doc, ids)
    } else {
        doc.clone()
    };
    if result.all_ids().is_empty() {
        return Err("There is nothing to print.".into());
    }
    if selection || result.page_layout.is_none() {
        let mut layout = result
            .page_layout
            .clone()
            .unwrap_or_else(|| Layout::around(&result));
        layout.columns = 1;
        layout.rows = 1;
        let (lo, hi) = crate::scene::selection_bounds(&result, &result.all_ids())
            .ok_or("There is nothing to print.")?;
        layout.origin = Point::new(
            (lo.x + hi.x
                - STYLE.world(layout.width_pt + layout.margins.left - layout.margins.right))
                / 2.,
            (lo.y + hi.y
                - STYLE.world(layout.height_pt + layout.margins.top - layout.margins.bottom))
                / 2.,
        );
        if layout.overflow(&result) > 0 {
            return Err(if selection {
                "The selection exceeds one sheet. Choose a larger paper size, or print the document pages."
            } else {
                "The drawing exceeds one A4 sheet. Use Page setup to choose a larger paper size or a page grid."
            }.into());
        }
        result.page_layout = Some(layout);
    }
    result.version = 12;
    result.validate()?;
    Ok(result)
}
pub fn prepare(doc: Document, title: String) -> Result<Prepared, String> {
    let pdf = crate::export::pages_pdf(&doc)?;
    if pdf.len() > 128 * 1024 * 1024 {
        return Err("The print snapshot exceeds 128 MB. Print a smaller selection.".into());
    }
    Ok(Prepared {
        pdf: Arc::new(pdf),
        title: title
            .chars()
            .filter(|c| !c.is_control())
            .take(200)
            .collect(),
    })
}
fn helper() -> Result<PathBuf, String> {
    std::env::current_exe()
        .ok()
        .and_then(|exe| {
            let path = exe
                .parent()?
                .parent()?
                .join("Helpers/Moruno Print.app/Contents/MacOS/moruno-print");
            path.is_file().then_some(path)
        })
        .or_else(|| {
            option_env!("MORUNO_PRINT_HELPER")
                .map(PathBuf::from)
                .filter(|p| p.is_file())
        })
        .ok_or_else(|| "Native printing is unavailable in this build.".into())
}
async fn drain_errors(mut input: impl tokio::io::AsyncRead + Unpin) -> Result<Vec<u8>, String> {
    let mut result = Vec::new();
    let mut buffer = [0u8; 4096];
    loop {
        let size = input.read(&mut buffer).await.map_err(|e| e.to_string())?;
        if size == 0 {
            break;
        }
        let keep = size.min(8192usize.saturating_sub(result.len()));
        if let Some(bytes) = buffer.get(..keep) {
            result.extend_from_slice(bytes);
        }
    }
    Ok(result)
}
pub async fn show_dialog(job: Prepared) -> Result<Outcome, String> {
    if !available() {
        return Err("Native printing is currently available on macOS. Export a page PDF to print on this system.".into());
    }
    let helper = helper()?;
    let mut file = tempfile::Builder::new()
        .prefix("moruno-print-")
        .suffix(".pdf")
        .tempfile()
        .map_err(|e| e.to_string())?;
    file.write_all(&job.pdf).map_err(|e| e.to_string())?;
    file.as_file().sync_all().map_err(|e| e.to_string())?;
    let request = serde_json::to_vec(&Request {
        path: file.path(),
        title: &job.title,
    })
    .map_err(|e| e.to_string())?;
    let mut child = Command::new(helper)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("Could not open the print dialog: {e}"))?;
    let mut input = child.stdin.take().ok_or("Print input is unavailable")?;
    let output = child.stdout.take().ok_or("Print output is unavailable")?;
    let errors = child
        .stderr
        .take()
        .ok_or("Print error output is unavailable")?;
    let write = async {
        input.write_all(&request).await.map_err(|e| e.to_string())?;
        input.shutdown().await.map_err(|e| e.to_string())?;
        drop(input);
        Ok::<_, String>(())
    };
    let read = async {
        let mut bytes = Vec::new();
        output
            .take(65537)
            .read_to_end(&mut bytes)
            .await
            .map_err(|e| e.to_string())?;
        if bytes.len() > 65536 {
            return Err("Invalid print response".to_string());
        }
        Ok(bytes)
    };
    let wait = async { child.wait().await.map_err(|e| e.to_string()) };
    let (_, bytes, errors, status) = tokio::try_join!(write, read, drain_errors(errors), wait)?;
    if !status.success() {
        let message = String::from_utf8_lossy(&errors).trim().to_owned();
        return Err(if message.is_empty() {
            "The print dialog could not complete.".into()
        } else {
            message.chars().take(500).collect()
        });
    }
    serde_json::from_slice(&bytes).map_err(|_| "Invalid print response".into())
}
