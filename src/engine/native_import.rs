//! Finish molecular, drawing and reaction imports without a Python worker.
//!
//! Molecular files and drawing scenes retain their original chemical state for
//! analysis. Reactions deliberately prepare their finished combined drawing,
//! matching the original reaction import path. Layout and drawing construction
//! stay detached; parse, chemistry and helper errors never request a fallback.
use super::{Request, Response, native_response};
use crate::{
    chemistry::{
        self, cdxml, depict, document as molecular,
        inchi::{generator, helper, output},
        molfile, reaction, smiles,
        stereo::{Point3, perception::State},
    },
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
    #[error("Could not parse this structure")]
    Parse,
    #[error("Imported molecule exceeds the stable atom ID limit")]
    AtomIds,
    #[error("This bond type is not supported yet.")]
    BondType,
    #[error(transparent)]
    Smiles(#[from] smiles::Error),
    #[error(transparent)]
    Inchi(#[from] output::Error),
    #[error(transparent)]
    Layout(#[from] depict::Error),
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
    Drawing(Box<Document>, String),
    Inchi(String),
    Deferred(Deferred),
}

/// Complete an import using the supplied helper, or discover the installed
/// helper lazily when `config` is absent. Empty figures need no helper. Dropping
/// this future cancels an active helper and cannot publish a partial drawing.
pub async fn execute(
    request: impl Into<Arc<Request>> + Send,
    mut config: Option<native_response::Config>,
) -> Result<Outcome, Error> {
    let request = request.into();
    // Retain the caller snapshot. Large text/document clones and chemical work
    // belong on the blocking executor; only the bounded InChI exchange is async.
    let prepared = tokio::task::spawn_blocking(move || prepare((*request).clone())).await??;
    let prepared = match prepared {
        Preparation::Complete(prepared) => prepared,
        Preparation::Drawing(document, warning) => {
            return Ok(Outcome::Complete(Box::new(Response {
                document: Some(*document),
                analysis: None,
                output: None,
                engine_version: chemistry::RDKIT_VERSION.into(),
                warnings: vec![warning],
            })));
        }
        Preparation::Inchi(text) => {
            let reader = match config.clone() {
                Some(config) => config,
                None => native_response::Config::new(
                    tokio::task::spawn_blocking(helper::discover)
                        .await?
                        .map_err(native_response::Error::Discovery)?,
                ),
            };
            let output = generator::read_with_limits(&reader.helper, &text, reader.limits)
                .await
                .map_err(native_response::Error::Helper)?;
            config = Some(reader);
            Box::new(tokio::task::spawn_blocking(move || prepare_inchi(output)).await??)
        }
        Preparation::Deferred(reason) => return Ok(Outcome::Deferred(reason)),
    };
    let Prepared { molecule, document } = *prepared;
    let attachments = crate::attachments::present(&document);
    let analysis = if molecule.state.graph.atoms.is_empty() || attachments {
        None
    } else {
        Some(native_response::analyze_prepared(Arc::new(molecule), config).await?)
    };
    Ok(Outcome::Complete(Box::new(Response {
        document: Some(document),
        analysis,
        output: None,
        engine_version: chemistry::RDKIT_VERSION.into(),
        warnings: if attachments {
            vec![crate::attachments::ANALYSIS_NOTICE.into()]
        } else {
            Vec::new()
        },
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
    let text = request.text.unwrap_or_default();
    if !matches!(format, "rxn" | "rsmi") && text.trim().is_empty() {
        return Err(Error::Empty);
    }
    let prepared = match format {
        "inchi" => return Ok(Preparation::Inchi(text)),
        "smiles" => {
            let imported = smiles::read(&text)?;
            let mut molecule = molecule(imported.prepared.state)?;
            layout(&mut molecule, &imported.reaction_properties)?;
            finish_molecule(molecule, &imported.prepared.dummy_labels)?
        }
        "mol" => {
            let imported = molfile::read(&text)?;
            let drawing = imported.drawing()?;
            let labels = drawing.labels()?;
            let mut document = drawing.finish(labels)?;
            let restored = imported.restore_haworth(&mut document);
            let molecule = if restored {
                molecular::prepare(&document)?
            } else {
                imported.molecule
            };
            Prepared { molecule, document }
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
            let (xml, variables) =
                cdxml::drawing_variables(&xml).map_err(|e| Error::Binary(e.to_string()))?;
            if !variables.0.is_empty() {
                let prepared = cdxml::prepare_drawing(&xml)?;
                let mut document = cdxml::assemble_cdxml(&prepared)?.into_unchecked_drawing()?;
                variables
                    .restore(&prepared, &mut document)
                    .map_err(|e| Error::Binary(e.to_string()))?;
                return Ok(Preparation::Drawing(Box::new(document),
                    "Drawing imported with R/X labels as uninterpreted atom text; query semantics and molecular properties are unavailable".into()));
            }
            let prepared = match cdxml::prepare_cdxml(&xml) {
                Ok(prepared) => prepared,
                Err(error) if matches!(error.cause, cdxml::PreparationCause::Sanitization(_)) => {
                    let scene = cdxml::assemble_cdxml(&cdxml::prepare_drawing(&xml)?)?;
                    let document = scene.into_unchecked_drawing()?;
                    return Ok(Preparation::Drawing(
                        Box::new(document),
                        format!(
                            "Drawing imported; chemical assignments need review: {}",
                            error.cause
                        ),
                    ));
                }
                Err(error) => return Err(error.into()),
            };
            if styled_coordination_changed(&prepared) {
                let scene = cdxml::assemble_cdxml(&cdxml::prepare_drawing(&xml)?)?;
                return Ok(Preparation::Drawing(
                    Box::new(scene.into_unchecked_drawing()?),
                    "Drawing imported; styled metal contacts retained as drawn. Coordination assignments need review; molecular properties are unavailable".into(),
                ));
            }
            let mut scene = cdxml::assemble_cdxml(&prepared)?;
            if scene.conformer_3d.is_none() {
                layout(&mut scene.molecule, &[])?;
                scene.conformer_3d = Some(false);
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
            let positions = layout
                .requests()
                .map(|request| {
                    molecular::validate_molecule(request.molecule())?;
                    coordinates(&request.molecule().state, request.atom_properties())
                })
                .collect::<Result<Vec<_>, Error>>()?;
            finish_reaction(layout.finish(positions)?)?
        }
        _ => return Err(Error::Format),
    };
    prepared.document.validate().map_err(Error::Document)?;
    Ok(Preparation::Complete(Box::new(prepared)))
}

// Sanitization can normalize an overvalent metal contact to a dative bond.
// A wedge/hash on the source single bond has no equivalent dative appearance.
// Preserve that drawing explicitly instead of either dropping its style or
// publishing properties for a different graph. Ordinary molecular parsing
// and supported plain/dashed coordinate bonds retain their strict behavior.
fn styled_coordination_changed(prepared: &cdxml::PreparedCdxml) -> bool {
    let ids = &prepared.molecule.ids;
    let changed: std::collections::HashSet<_> = prepared
        .molecule
        .state
        .graph
        .bonds
        .iter()
        .filter(|b| b.order == 5)
        .filter_map(|b| ids.get(b.a).zip(ids.get(b.b)))
        .map(|(&a, &b)| (a.min(b), a.max(b)))
        .collect();
    prepared.bonds.iter().any(|b| {
        b.order == 1
            && !matches!(b.display.as_str(), "plain" | "dashed")
            && changed.contains(&(b.a.min(b.b), b.a.max(b.b)))
    })
}

fn finish_reaction(drawing: reaction::Drawing) -> Result<Prepared, Error> {
    let labels = drawing.labels()?;
    let document = drawing.finish(labels)?;
    let molecule = molecular::prepare(&document)?;
    Ok(Prepared { molecule, document })
}

fn molecule(state: State) -> Result<molecular::Molecule, Error> {
    let count = state.graph.atoms.len();
    let last = u64::try_from(count).map_err(|_| Error::AtomIds)?;
    Ok(molecular::Molecule {
        rdkit_version: chemistry::RDKIT_VERSION,
        ids: (1..=last).collect(),
        positions: vec![Point3::default(); count],
        state,
    })
}

fn coordinates(
    state: &State,
    properties: &[Vec<(Vec<u8>, Vec<u8>)>],
) -> Result<Vec<Point3>, Error> {
    let ranks = if properties.is_empty() {
        vec![depict::ranks::AtomProperties::default(); state.graph.atoms.len()]
    } else {
        properties
            .iter()
            .map(|pairs| depict::ranks::AtomProperties::from_pairs(pairs))
            .collect()
    };
    Ok(depict::compute_with_rank_properties(
        state,
        &ranks,
        None,
        depict::Options {
            // Native public Compute2DCoords defaults. Cleanup explicitly opts in
            // to templates, while original import calls supply no such option.
            use_ring_templates: false,
            ..depict::Options::default()
        },
    )?
    .positions)
}

fn layout(
    molecule: &mut molecular::Molecule,
    properties: &[Vec<(Vec<u8>, Vec<u8>)>],
) -> Result<(), Error> {
    molecular::validate_molecule(molecule)?;
    molecule.positions = coordinates(&molecule.state, properties)?;
    Ok(())
}

fn finish_molecule(
    molecule: molecular::Molecule,
    labels: &[Option<String>],
) -> Result<Prepared, Error> {
    let drawing = molecular::for_import(&molecule, false, labels)?;
    let labels = drawing.labels()?;
    let document = drawing.finish(labels)?;
    document.validate().map_err(Error::Document)?;
    Ok(Prepared { molecule, document })
}

fn prepare_inchi(output: output::Output) -> Result<Prepared, Error> {
    let imported = output::reconstruct(
        &output,
        output::Options {
            sanitize: true,
            remove_hydrogens: false,
        },
    )?;
    let state = imported.state.ok_or(Error::Parse)?;
    if imported.unspecified_bonds.iter().any(|&value| value) {
        return Err(Error::BondType);
    }
    let mut molecule = molecule(state)?;
    layout(&mut molecule, &[])?;
    let labels = vec![None; molecule.ids.len()];
    finish_molecule(molecule, &labels)
}
