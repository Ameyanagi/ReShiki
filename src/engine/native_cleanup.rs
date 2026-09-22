//! Atomic cleanup using the detached Rust layout and drawing pipeline.
//!
//! Each component is laid out and checked before the next component starts.
//! The original-precision final molecule is analyzed independently from its f32
//! drawing; selected-scope chemistry warnings never hide helper failures.
use super::{Request, Response, native_response};
use crate::chemistry::{self, cleanup, depict};
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Unsupported protocol version")]
    Protocol,
    #[error("Unknown chemistry operation")]
    Operation,
    #[error("'document'")]
    MissingDocument,
    #[error("Draw or import a molecule first")]
    Empty,
    #[error("{0}")]
    Document(String),
    #[error("Cleanup optional analysis lost its typed chemistry failure")]
    AnalysisPolicy,
    #[error("{}", .0.diagnostic())]
    Cleanup(#[from] cleanup::Error),
    #[error(transparent)]
    Layout(#[from] depict::Error),
    #[error(transparent)]
    Analysis(#[from] native_response::Error),
    #[error("Invalid cleanup response: {0}")]
    Response(#[from] serde_json::Error),
    #[error("Native cleanup task failed: {0}")]
    Task(#[from] tokio::task::JoinError),
}

/// Complete a cleanup without starting Python or changing the source snapshot.
/// Chemistry runs on the blocking executor. Dropping this future cancels an
/// active identifier helper and cannot publish a partially cleaned drawing.
/// Helper discovery is lazy when no explicit configuration is supplied.
pub async fn execute(
    request: impl Into<Arc<Request>> + Send,
    config: Option<native_response::Config>,
) -> Result<Response, Error> {
    let request = request.into();
    let cleaned = tokio::task::spawn_blocking(move || prepare(&request)).await??;
    let analysis = match cleaned.molecule {
        Some(molecule) => {
            Some(native_response::analyze_prepared(Arc::new(molecule), config).await?)
        }
        None if cleaned.analysis_policy == cleanup::AnalysisPolicy::SelectedChemistry
            && cleaned.analysis_failure.is_some() =>
        {
            None
        }
        None => return Err(Error::AnalysisPolicy),
    };
    tokio::task::spawn_blocking(move || {
        let response = Response {
            document: Some(cleaned.document),
            analysis,
            output: None,
            engine_version: chemistry::RDKIT_VERSION.into(),
            warnings: cleaned.warnings,
        };
        // The app parses worker JSON through f64 Value before Document's f32.
        // Preserve that response boundary instead of direct decimal-to-f32.
        let response: Response = serde_json::from_value(serde_json::to_value(response)?)?;
        if let Some(document) = &response.document {
            document.validate().map_err(Error::Document)?;
        }
        Ok(response)
    })
    .await?
}

fn prepare(request: &Request) -> Result<cleanup::Cleaned, Error> {
    if request.protocol != 1 {
        return Err(Error::Protocol);
    }
    if request.operation != "clean" {
        return Err(Error::Operation);
    }
    let document = request.document.as_ref().ok_or(Error::MissingDocument)?;
    document.validate().map_err(Error::Document)?;
    if document.atoms.is_empty() {
        return Err(Error::Empty);
    }
    let prepared = cleanup::prepare(
        document,
        request.cleanup.unwrap_or_default(),
        request.selected_ids.as_deref().unwrap_or_default(),
    )?;
    prepared.finish_with(|request| {
        // from_document and cleanup's identity SMILES pass do not set the
        // native _chiralAtomRank property. The independent public-layout
        // observer checks its absence, including selected and stereo cases.
        let ranks = vec![None; request.molecule.state.graph.atoms.len()];
        Ok(depict::compute(
            &request.molecule.state,
            &ranks,
            Some(request.fixed),
            depict::Options {
                bond_length: request.bond_length,
                canonical_orientation: request.canonical_orientation,
                use_ring_templates: request.use_ring_templates,
                ..Default::default()
            },
        )?
        .positions)
    })
}
