//! Complete immutable, already prepared Analyze and molecular export requests.
//!
//! No worker, Python environment or chemistry cache is created here. The caller
//! supplies the pinned standalone InChI helper and prepares both molecular and
//! drawing states from the same request snapshot. All chemical work and drawing
//! serialization run on blocking tasks; only the bounded helper exchange is async.
use super::{Analysis, Request, Response};
use crate::{
    chemistry::{
        self, document as molecular,
        inchi::{generator, input, key},
        molfile, rings, smiles,
    },
    document::Document,
    exchange,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use std::{path::PathBuf, sync::Arc, time::Duration};

#[derive(Clone, Debug)]
pub struct Config {
    pub helper: PathBuf,
    pub limits: generator::Limits,
}
impl Config {
    pub fn new(helper: PathBuf) -> Self {
        Self {
            helper,
            limits: generator::Limits {
                timeout: Duration::from_secs(30),
                kernel_heap_bytes: generator::DEFAULT_KERNEL_HEAP_BYTES,
            },
        }
    }
}

/// Both states must have been prepared from the supplied request document.
/// The chemical state supplies analysis/identifiers; the drawing state supplies
/// display labels and appearance. Neither is changed by response construction.
#[derive(Debug)]
pub struct Prepared {
    pub molecule: molecular::Molecule,
    pub drawing: molecular::Drawing,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Unsupported protocol version")]
    Protocol,
    #[error("Unknown chemistry operation")]
    Operation,
    #[error("'document'")]
    MissingDocument,
    #[error("Missing prepared molecule and drawing")]
    MissingPrepared,
    #[error("Prepared molecule and drawing identities do not match the request")]
    Identity,
    #[error("Draw or import a molecule first")]
    Empty,
    #[error("Unsupported export format")]
    Export,
    #[error("'format'")]
    MissingFormat,
    #[error(
        "This export cannot preserve the hydrogen, partial, dative or quadruple bonds in this drawing; use native or CDXML"
    )]
    Exotic,
    #[error(transparent)]
    Drawing(#[from] molecular::Error),
    #[error(transparent)]
    Smiles(#[from] smiles::write::Error),
    #[error(transparent)]
    InchiInput(#[from] input::Error),
    #[error(transparent)]
    Helper(#[from] generator::Error),
    #[error("Invalid molecular InChI: {0}")]
    Key(#[from] key::Error),
    #[error(transparent)]
    Rings(#[from] rings::RingError),
    #[error(transparent)]
    Molfile(#[from] molfile::Error),
    #[error(transparent)]
    DrawingExport(#[from] exchange::drawing::Error),
    #[error("{0}")]
    Chemistry(String),
    #[error("Invalid chemistry response: {0}")]
    Response(#[from] serde_json::Error),
    #[error("Native response task failed: {0}")]
    Task(#[from] tokio::task::JoinError),
}

struct Draft {
    document: Document,
    smiles: String,
    inchi: Option<input::Input>,
    figure_only: bool,
}

/// Build an atomic response without changing either snapshot. `prepared` may
/// be absent only for an atom-empty CDXML/CDX export. Empty abbreviation results
/// still require preparation and receive the native empty-molecule analysis.
///
/// Dropping this future kills an active helper. An already running blocking
/// task may finish its bounded detached calculation, but cannot publish a
/// response or mutate the caller's drawing after cancellation.
pub async fn build(
    request: Arc<Request>,
    prepared: Option<Arc<Prepared>>,
    config: Config,
) -> Result<Response, Error> {
    let source = Arc::clone(&request);
    let state = prepared.clone();
    let draft = tokio::task::spawn_blocking(move || begin(&source, state.as_deref())).await??;
    let inchi = if let Some(input) = &draft.inchi {
        // Native warning/error statuses are not transport failures. Like the
        // original MolToInchi wrapper, retain the returned identifier (possibly
        // empty); neither native log nor message becomes an application warning.
        generator::generate_with_limits(&config.helper, input, config.limits)
            .await?
            .inchi
    } else {
        String::new()
    };
    tokio::task::spawn_blocking(move || finish(&request, prepared.as_deref(), draft, inchi)).await?
}

fn begin(request: &Request, prepared: Option<&Prepared>) -> Result<Draft, Error> {
    if request.protocol != 1 {
        return Err(Error::Protocol);
    }
    if !matches!(
        request.operation.as_str(),
        "analyze" | "finish_abbreviation" | "export"
    ) {
        return Err(Error::Operation);
    }
    let document = request.document.as_ref().ok_or(Error::MissingDocument)?;
    if document.atoms.is_empty() && request.operation != "finish_abbreviation" {
        if request.operation == "export"
            && matches!(request.format.as_deref(), Some("cdxml" | "cdx"))
        {
            return Ok(Draft {
                document: document.clone(),
                smiles: String::new(),
                inchi: None,
                figure_only: true,
            });
        }
        return Err(Error::Empty);
    }
    let prepared = prepared.ok_or(Error::MissingPrepared)?;
    molecular::validate_molecule(&prepared.molecule)?;
    molecular::validate_molecule(prepared.drawing.molecule())?;
    if prepared.molecule.ids != prepared.drawing.molecule().ids
        || !prepared
            .molecule
            .ids
            .iter()
            .copied()
            .eq(document.atoms.iter().map(|a| a.id))
        || prepared.molecule.state.graph.bonds.len() != document.bonds.len()
    {
        return Err(Error::Identity);
    }
    // The original to_document pass precedes Analyze. Full CIP operates on
    // the detached drawing, never on the molecule used for identifiers.
    let document = prepared
        .drawing
        .clone()
        .finish(prepared.drawing.labels()?)?;
    let molecule = &prepared.molecule;
    let bonds = &molecule.state.graph.bonds;
    let identifiers = !bonds.iter().any(|b| matches!(b.order, 0 | 7));
    let smiles = if identifiers {
        smiles::write::write(&molecule.state, Default::default())?.text
    } else {
        String::new()
    };
    let inchi = if identifiers && !bonds.iter().any(|b| matches!(b.order, 5 | 6)) {
        match input::prepare(&molecule.state, Some(&molecule.positions)) {
            Ok(input) => Some(input),
            // The pinned adapter returns an empty identifier before calling the
            // kernel when its one-sided adjacency exceeds native storage.
            Err(input::Error::NativeEmpty(_)) => None,
            Err(error) => return Err(error.into()),
        }
    } else {
        None
    };
    Ok(Draft {
        document,
        smiles,
        inchi,
        figure_only: false,
    })
}

fn finish(
    request: &Request,
    prepared: Option<&Prepared>,
    draft: Draft,
    inchi: String,
) -> Result<Response, Error> {
    let analysis = if draft.figure_only {
        None
    } else {
        let molecule = &prepared.ok_or(Error::MissingPrepared)?.molecule;
        let graph = &molecule.state.graph;
        let inchikey = if inchi.is_empty() {
            String::new()
        } else {
            key::from_inchi(&inchi)?
        };
        let ring_atoms = rings::perceive(graph, rings::Options::default())?.atoms;
        let descriptors =
            chemistry::descriptors::calculate(graph, &ring_atoms).map_err(Error::Chemistry)?;
        let properties = chemistry::properties(&graph.atom_facts().map_err(Error::Chemistry)?)
            .map_err(Error::Chemistry)?;
        Some(Analysis {
            smiles: draft.smiles,
            formula: properties.formula,
            mass: properties.mass,
            exact_mass: properties.exact_mass,
            logp: descriptors.logp,
            tpsa: descriptors.tpsa,
            donors: descriptors.donors,
            acceptors: descriptors.acceptors,
            rings: u32::try_from(ring_atoms.len())
                .map_err(|_| Error::Chemistry("Ring count exceeds supported range".into()))?,
            unpaired_electrons: properties.unpaired_electrons,
            inchi,
            inchikey,
        })
    };
    let mut response = Response {
        document: Some(draft.document),
        analysis,
        output: None,
        engine_version: chemistry::RDKIT_VERSION.into(),
        warnings: Vec::new(),
    };
    if request.operation == "export" {
        let document = response.document.as_ref().ok_or(Error::MissingDocument)?;
        if !draft.figure_only && !document.reactions.is_empty() {
            response.warnings.push("This drawing or molecule format does not retain reaction roles. Use RXN/reaction SMILES for reaction data, or .reshiki for the complete scheme.".into());
        }
        let format = request.format.as_deref();
        if document.bonds.iter().any(|bond| match format {
            Some("mol") => matches!(bond.order, 0 | 6 | 7),
            Some("smiles") => matches!(bond.order, 0 | 7),
            Some("inchi") => matches!(bond.order, 0 | 5 | 6 | 7),
            _ => false,
        }) {
            return Err(Error::Exotic);
        }
        response.output = Some(match format {
            Some("smiles") => response
                .analysis
                .as_ref()
                .ok_or(Error::Empty)?
                .smiles
                .clone(),
            Some("inchi") => response
                .analysis
                .as_ref()
                .ok_or(Error::Empty)?
                .inchi
                .clone(),
            Some("mol") => molfile::write(
                &prepared.ok_or(Error::MissingPrepared)?.molecule,
                Default::default(),
            )?,
            Some("cdxml" | "cdx") => {
                let xml = exchange::drawing::write(document, request.into())?;
                if format == Some("cdx") {
                    STANDARD.encode(exchange::to_cdx(&xml).map_err(Error::Chemistry)?)
                } else {
                    xml
                }
            }
            None => return Err(Error::MissingFormat),
            _ => return Err(Error::Export),
        });
    }
    // Match the real engine boundary: JSON values carry f64, then Response
    // decodes Document f32. Direct JSON-to-f32 decimal parsing is not equivalent.
    let response: Response = serde_json::from_value(serde_json::to_value(response)?)?;
    if let Some(document) = &response.document {
        document.validate().map_err(Error::Chemistry)?;
    }
    Ok(response)
}
