//! Finish coordinate-bearing imports without starting the Python worker.
//!
//! Molecular files and drawing scenes retain their original chemical state for
//! analysis. Reactions deliberately prepare their finished combined drawing,
//! matching the original reaction import path. Missing layouts are explicit;
//! parse, chemistry and helper errors never request a fallback.
use super::{Request, Response, native_response};
use crate::{
    chemistry::{self, cdxml, document as molecular, molfile, reaction},
    document::Document,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Deferred {
    OtherOperation,
    MolecularLayout,
    DrawingLayout,
    ReactionLayout,
}

#[derive(Debug)]
pub enum Outcome {
    Complete(Box<Response>),
    Deferred(Deferred),
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Unsupported protocol version")]
    Protocol,
    #[error("Enter a structure first")]
    Empty,
    #[error("Unsupported import format")]
    Format,
    #[error("Drawing exceeds the 16 MB structure limit")]
    BinaryLimit,
    #[error("Invalid binary drawing base64: {0}")]
    Base64(#[from] base64::DecodeError),
    #[error("Drawing conversion failed: {0}")]
    Binary(String),
    #[error(transparent)]
    Molfile(#[from] molfile::ReadError),
    #[error(transparent)]
    Molecular(#[from] molecular::Error),
    // Keep the typed preparation stage for callers, while retaining the
    // original importer's public message at the application response boundary.
    #[error("{}", .0.cause)]
    CdxmlPreparation(#[from] cdxml::PreparationError),
    #[error(transparent)]
    CdxmlScene(#[from] cdxml::SceneError),
    #[error(transparent)]
    Reaction(#[from] reaction::Error),
    #[error(transparent)]
    ReactionSmiles(#[from] reaction::SmilesError),
    #[error(transparent)]
    Analysis(#[from] native_response::Error),
    #[error("Invalid imported drawing: {0}")]
    Document(String),
    #[error("Native import task failed: {0}")]
    Task(#[from] tokio::task::JoinError),
}

struct Prepared {
    molecule: molecular::Molecule,
    document: Document,
}
enum Preparation {
    Complete(Box<Prepared>),
    Deferred(Deferred),
}

/// Complete an import using the supplied helper, or discover the installed
/// helper lazily when `config` is absent. Empty figures need no helper. Dropping
/// this future cancels an active helper and cannot publish a partial drawing.
pub async fn execute(
    request: impl Into<Arc<Request>> + Send,
    config: Option<native_response::Config>,
) -> Result<Outcome, Error> {
    let request = request.into();
    // Retain an immutable source snapshot for a possible layout deferral. Any
    // large text/document clone belongs on the blocking executor, with parsing.
    let prepared = tokio::task::spawn_blocking(move || prepare((*request).clone())).await??;
    let prepared = match prepared {
        Preparation::Complete(prepared) => prepared,
        Preparation::Deferred(reason) => return Ok(Outcome::Deferred(reason)),
    };
    let Prepared { molecule, document } = *prepared;
    let analysis = if molecule.state.graph.atoms.is_empty() {
        None
    } else {
        Some(native_response::analyze_prepared(Arc::new(molecule), config).await?)
    };
    Ok(Outcome::Complete(Box::new(Response {
        document: Some(document),
        analysis,
        output: None,
        engine_version: chemistry::RDKIT_VERSION.into(),
        warnings: Vec::new(),
    })))
}

fn prepare(request: Request) -> Result<Preparation, Error> {
    if request.protocol != 1 {
        return Err(Error::Protocol);
    }
    if let Some(document) = &request.document {
        document.validate().map_err(Error::Document)?;
    }
    if request.operation != "import" {
        return Ok(Preparation::Deferred(Deferred::OtherOperation));
    }
    let format = request.format.as_deref().unwrap_or("smiles");
    if matches!(format, "smiles" | "inchi") {
        return Ok(Preparation::Deferred(Deferred::MolecularLayout));
    }
    let text = request.text.unwrap_or_default();
    if text.trim().is_empty() {
        return Err(Error::Empty);
    }
    let prepared = match format {
        "mol" => {
            let imported = molfile::read(&text)?;
            let drawing = imported.drawing()?;
            let labels = drawing.labels()?;
            let document = drawing.finish(labels)?;
            Prepared {
                molecule: imported.molecule,
                document,
            }
        }
        "cdxml" | "cdx" => {
            let xml = if format == "cdx" {
                if text.len() > crate::exchange::LIMIT.div_ceil(3) * 4 {
                    return Err(Error::BinaryLimit);
                }
                crate::exchange::from_cdx(&STANDARD.decode(text)?).map_err(Error::Binary)?
            } else {
                text
            };
            let scene = cdxml::assemble_cdxml(&cdxml::prepare_cdxml(&xml)?)?;
            if !scene.molecule.state.graph.atoms.is_empty() && scene.conformer_3d.is_none() {
                return Ok(Preparation::Deferred(Deferred::DrawingLayout));
            }
            let imported = scene.into_document()?;
            Prepared {
                molecule: imported.molecule,
                document: imported.document,
            }
        }
        "rxn" => finish_reaction(reaction::read_rxn(&text)?.drawing()?)?,
        "rsmi" => {
            let layout = reaction::read_smiles(&text)?.layout()?;
            if layout.requests().next().is_some() {
                return Ok(Preparation::Deferred(Deferred::ReactionLayout));
            }
            finish_reaction(layout.finish(Vec::new())?)?
        }
        _ => return Err(Error::Format),
    };
    prepared.document.validate().map_err(Error::Document)?;
    Ok(Preparation::Complete(Box::new(prepared)))
}

fn finish_reaction(drawing: reaction::Drawing) -> Result<Prepared, Error> {
    let labels = drawing.labels()?;
    let document = drawing.finish(labels)?;
    let molecule = molecular::prepare(&document)?;
    Ok(Prepared { molecule, document })
}
