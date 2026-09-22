//! Finish a selected aromatic display edit without changing the source snapshot.
use super::{Request, Response, native_response};
use crate::chemistry::{self, document as molecular};
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Unsupported protocol version")]
    Protocol,
    #[error("Unknown chemistry operation")]
    Operation,
    #[error("'document'")]
    MissingDocument,
    #[error("{0}")]
    Drawing(String),
    #[error("{}", preparation_message(.0))]
    Preparation(#[from] molecular::Error),
    #[error(transparent)]
    Analysis(#[from] native_response::Error),
    #[error("Aromatic display preparation failed: {0}")]
    Task(#[from] tokio::task::JoinError),
}

fn preparation_message(error: &molecular::Error) -> String {
    match error {
        // Preserve the original operation's selection and drawing diagnostics.
        molecular::Error::Drawing(message) => message.clone(),
        error => error.to_string(),
    }
}

/// Return one complete undoable edit, using the original prepared molecular
/// state for analysis. No helper is discovered before display/identity checks
/// succeed. Dropping the future kills an active helper; detached blocking work
/// may finish but cannot publish a drawing or mutate the caller's snapshot.
pub async fn execute(
    request: impl Into<Arc<Request>> + Send,
    config: Option<native_response::Config>,
) -> Result<Response, Error> {
    let request = request.into();
    let edit = tokio::task::spawn_blocking(move || {
        if request.protocol != 1 {
            return Err(Error::Protocol);
        }
        if request.operation != "aromatic" {
            return Err(Error::Operation);
        }
        let document = request.document.as_ref().ok_or(Error::MissingDocument)?;
        document.validate().map_err(Error::Drawing)?;
        let selected = request.selected_ids.as_deref().unwrap_or_default();
        Ok::<_, Error>(molecular::aromatic_display(document, selected)?.finish()?)
    })
    .await??;
    let analysis = native_response::analyze_prepared(Arc::new(edit.molecule), config).await?;
    Ok(Response {
        document: Some(edit.document),
        analysis: Some(analysis),
        output: None,
        engine_version: chemistry::RDKIT_VERSION.into(),
        warnings: Vec::new(),
    })
}
