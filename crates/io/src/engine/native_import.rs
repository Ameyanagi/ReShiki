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
        inchi::{helper, output},
        kekulize::Direction,
        molfile, reaction, smiles,
        stereo::{Point3, perception::State},
    },
    document::Document,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use std::sync::Arc;
#[cfg(test)]
mod allene_tests;
#[cfg(test)]
mod atrop_tests;
pub(super) mod chain_layout;
pub(super) mod template_retry;
#[cfg(test)]
mod tests;

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
    #[error("Invalid axial stereo identity markers")]
    AxisMarkers,
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
    #[error(transparent)]
    Crystal(#[from] crate::crystal::Error),
    #[error("Invalid imported drawing: {0}")]
    Document(String),
    #[error("Native import task failed: {0}")]
    Task(#[from] tokio::task::JoinError),
}

const CUMULENE_EZ_NOTICE: &str = "Extended double-bond stereochemistry (e.g. C=C=C=C) ignored";

struct Prepared {
    molecule: molecular::Molecule,
    document: Document,
    warnings: Vec<String>,
    /// The molecule keeps a 3D MOL file's own XYZ, which state its stereo as
    /// RDKit reads them, even where the drawing is a projection of them.
    file_xyz: bool,
}
enum Preparation {
    Complete(Box<Prepared>),
    Drawing(Box<Document>, Vec<String>),
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
    let drawing_options = request.drawing_options.unwrap_or_default();
    // Retain the caller snapshot. Large text/document clones and chemical work
    // belong on the blocking executor; only the bounded InChI exchange is async.
    let prepared =
        crate::task::spawn_blocking(move || prepare(Arc::unwrap_or_clone(request))).await??;
    let prepared = match prepared {
        Preparation::Complete(prepared) => prepared,
        Preparation::Drawing(document, warnings) => {
            return Ok(Outcome::Complete(Box::new(Response {
                document: Some(*document),
                analysis: None,
                output: None,
                engine_version: chemistry::RDKIT_VERSION.into(),
                warnings,
            })));
        }
        Preparation::Inchi(text) => {
            let reader = match config.clone() {
                Some(config) => config,
                None => native_response::Config::new(
                    crate::task::spawn_blocking(helper::discover)
                        .await?
                        .map_err(native_response::Error::Discovery)?,
                ),
            };
            let output = reader
                .read(&text)
                .await
                .map_err(native_response::Error::Helper)?;
            config = Some(reader);
            Box::new(
                crate::task::spawn_blocking(move || prepare_inchi(output, drawing_options))
                    .await??,
            )
        }
        Preparation::Deferred(reason) => return Ok(Outcome::Deferred(reason)),
    };
    let Prepared {
        molecule,
        document,
        mut warnings,
        file_xyz,
    } = *prepared;
    let attachments = crate::attachments::present(&document);
    let generic = crate::generic::present(&document);
    let analysis = if molecule.state.graph.atoms.is_empty() || attachments || generic {
        None
    } else {
        // Generated or projected XY must not define stereo the input left
        // open (#909); a file's own XYZ may.
        let projected = !file_xyz && molfile::has_projected_coordinates(&document);
        warnings.extend(native_response::identifier_notices(&molecule, projected));
        Some(native_response::analyze_prepared(Arc::new(molecule), projected, config).await?)
    };
    if generic {
        warnings.push(crate::generic::NOTICE.into());
    }
    if attachments {
        warnings.push(crate::attachments::ANALYSIS_NOTICE.into());
    }
    Ok(Outcome::Complete(Box::new(Response {
        document: Some(document),
        analysis,
        output: None,
        engine_version: chemistry::RDKIT_VERSION.into(),
        warnings,
    })))
}

fn prepare(request: Request) -> Result<Preparation, Error> {
    if request.protocol != crate::engine::PROTOCOL {
        return Err(Error::Protocol);
    }
    if let Some(document) = &request.document {
        document.validate().map_err(Error::Document)?;
    }
    if request.operation != "import" {
        return Ok(Preparation::Deferred(Deferred::OtherOperation));
    }
    let format = request.format.as_deref().unwrap_or("smiles");
    let drawing_options = request.drawing_options.unwrap_or_default();
    let text = request.text.unwrap_or_default();
    if !matches!(format, "rxn" | "rsmi") && text.trim().is_empty() {
        return Err(Error::Empty);
    }
    let prepared = match format {
        "inchi" => return Ok(Preparation::Inchi(text)),
        format if crate::crystal::is_crystal_format(format) => {
            let (document, warnings) = crate::crystal::read(format, &text)?;
            return Ok(Preparation::Drawing(Box::new(document), warnings));
        }
        "smiles" | "naming_smiles" => {
            let mut imported = smiles::read(&text)?;
            if format == "naming_smiles" {
                imported.prepared.state =
                    crate::molecule_names::apply_axis_markers(&imported.prepared.state)
                        .map_err(|_| Error::AxisMarkers)?;
            }
            // Wedges were read at the 2D input coordinates, which the fresh
            // layout replaces (#1037). Without coordinates the @/@@ tags state
            // the configuration (#1044); smiles::read has already taken any
            // axial configuration from the flags. Either way, draw wedges again
            // from the stereo. A wavy bond left after preparation would stop
            // that redraw at a tagged centre, so such inputs still read the
            // flags on the layout (#1049).
            let directions = &mut imported.prepared.state.directions;
            let tag_wins =
                imported.conformers.is_empty() && !directions.contains(&Direction::Unknown);
            if tag_wins || imported.conformers.iter().any(|c| !c.is_3d) {
                for direction in directions {
                    if matches!(direction, Direction::Wedge | Direction::Hash) {
                        *direction = Direction::None;
                    }
                }
            }
            let mut molecule = molecule(imported.prepared.state)?;
            let mut warnings = layout(&mut molecule, &imported.reaction_properties)?;
            // Import perception does not keep cumulene E/Z; say so.
            if imported.dropped_cumulene_ez {
                warnings.push(CUMULENE_EZ_NOTICE.into());
            }
            let mut prepared =
                finish_molecule(molecule, &imported.prepared.dummy_labels, drawing_options)?;
            prepared.warnings.extend(warnings);
            prepared
        }
        "mol" => {
            let imported = molfile::read(&text)?;
            // A 3D file is never Haworth-restored, so the molecule keeps its XYZ.
            let file_xyz = imported.annotations.is_3d;
            let (document, molecule) = imported.display_document()?;
            Prepared {
                molecule,
                document,
                warnings: Vec::new(),
                file_xyz,
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
            let (xml, variables) =
                cdxml::drawing_variables(&xml).map_err(|e| Error::Binary(e.to_string()))?;
            let mut warnings = Vec::new();
            if cdxml::schemes::validate(&xml).map_err(|e| Error::Binary(e.to_string()))? {
                warnings.push(cdxml::schemes::NOTICE.into());
            }
            if !variables.0.is_empty() {
                let prepared = cdxml::prepare_drawing(&xml)?;
                let mut document = cdxml::assemble_cdxml(&prepared)?.into_unchecked_drawing()?;
                variables
                    .restore(&prepared, &mut document)
                    .map_err(|e| Error::Binary(e.to_string()))?;
                warnings.push("Drawing imported with variable labels as uninterpreted atom text; query semantics and molecular properties are unavailable".into());
                return Ok(Preparation::Drawing(Box::new(document), warnings));
            }
            let prepared = match cdxml::prepare_cdxml(&xml) {
                Ok(prepared) => prepared,
                Err(error) if matches!(error.cause, cdxml::PreparationCause::Sanitization(_)) => {
                    let scene = cdxml::assemble_cdxml(&cdxml::prepare_drawing(&xml)?)?;
                    let document = scene.into_unchecked_drawing()?;
                    warnings.push(format!(
                        "Drawing imported; chemical assignments need review: {}",
                        error.cause
                    ));
                    return Ok(Preparation::Drawing(Box::new(document), warnings));
                }
                Err(error) => return Err(error.into()),
            };
            if styled_coordination_changed(&prepared) {
                let scene = cdxml::assemble_cdxml(&cdxml::prepare_drawing(&xml)?)?;
                warnings.push("Drawing imported; styled metal contacts retained as drawn. Coordination assignments need review; molecular properties are unavailable".into());
                return Ok(Preparation::Drawing(
                    Box::new(scene.into_unchecked_drawing()?),
                    warnings,
                ));
            }
            let mut scene = cdxml::assemble_cdxml(&prepared)?;
            if scene.conformer_3d.is_none() {
                warnings.extend(layout(&mut scene.molecule, &[])?);
                scene.conformer_3d = Some(false);
            }
            let imported = scene.into_document()?;
            Prepared {
                molecule: imported.molecule,
                document: imported.document,
                warnings,
                file_xyz: false,
            }
        }
        "rxn" => finish_reaction(reaction::read_rxn(&text)?.drawing()?)?,
        "rsmi" => {
            let layout = reaction::read_smiles(&text)?.layout()?;
            let layouts = layout
                .requests()
                .map(|request| {
                    molecular::validate_molecule(request.molecule())?;
                    coordinates(&request.molecule().state, request.atom_properties())
                })
                .collect::<Result<Vec<_>, Error>>()?;
            let (positions, warnings): (Vec<_>, Vec<_>) = layouts
                .into_iter()
                .map(|layout| (layout.positions, layout.warnings))
                .unzip();
            let mut prepared = finish_reaction(layout.finish(positions)?)?;
            prepared.warnings.extend(warnings.into_iter().flatten());
            prepared
        }
        _ => return Err(Error::Format),
    };
    prepared.document.validate().map_err(Error::Document)?;
    Ok(Preparation::Complete(Box::new(prepared)))
}

// Sanitization can normalize an overvalent metal contact to a dative bond.
// A source single bond is different chemical input from an explicit dative
// projection, even when both can now retain wedge/hash paint.
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
    Ok(Prepared {
        molecule,
        document,
        warnings: Vec::new(),
        file_xyz: false,
    })
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
) -> Result<chain_layout::Layout, Error> {
    let ranks = if properties.is_empty() {
        vec![depict::ranks::AtomProperties::default(); state.graph.atoms.len()]
    } else {
        properties
            .iter()
            .map(|pairs| depict::ranks::AtomProperties::from_pairs(pairs))
            .collect()
    };
    let conformer = depict::compute_with_rank_properties(
        state,
        &ranks,
        None,
        depict::Options {
            // Native public Compute2DCoords defaults. Cleanup explicitly opts in
            // to templates, while original import calls supply no such option.
            use_ring_templates: false,
            // RDKit 2026.09.1's path angle expansion (#735) is a refinement.
            path_angle_expansion: molecular::refine(),
            ..depict::Options::default()
        },
    )?;
    let positions = template_retry::improve(state, &ranks, conformer.positions, 1.5);
    let mut layout = chain_layout::improve(state, &ranks, positions);
    if molecular::refine() {
        chemistry::layout_refinement::straighten(state, &mut layout.positions, None);
        // Input wedge flags are read against these coordinates, so a reflection
        // would invert them (#1037). Rotation and translation keep their meaning.
        if !state
            .directions
            .iter()
            .any(|d| matches!(d, Direction::Wedge | Direction::Hash))
        {
            chemistry::layout_refinement::conventional_orientation(state, &mut layout.positions);
        }
        chemistry::layout_refinement::arrange_components(state, &mut layout.positions);
    }
    Ok(layout)
}

fn layout(
    molecule: &mut molecular::Molecule,
    properties: &[Vec<(Vec<u8>, Vec<u8>)>],
) -> Result<Vec<String>, Error> {
    molecular::validate_molecule(molecule)?;
    let layout = coordinates(&molecule.state, properties)?;
    molecule.positions = layout.positions;
    Ok(layout.warnings)
}

fn finish_molecule(
    molecule: molecular::Molecule,
    labels: &[Option<String>],
    options: molecular::DrawingOptions,
) -> Result<Prepared, Error> {
    let drawing = molecular::for_generated(&molecule, labels, options)?;
    let labels = drawing.labels()?;
    let document = drawing.finish(labels)?;
    document.validate().map_err(Error::Document)?;
    // An axis end reads a wedge from a neighbouring centre as its own, as
    // RDKit does, so some fresh layouts' wedges hide an axis (#1056). Refuse
    // such a drawing, as the reference refuses atropisomer bond stereo,
    // rather than silently lose the axis.
    // A drawing that cannot be prepared again (named dummy atoms) keeps
    // main's behaviour.
    let expected = axes(&molecule)?;
    if !expected.is_empty()
        && let Ok(read) = molecular::prepare(&document)
        && expected
            .iter()
            .any(|axis| !axes(&read).is_ok_and(|read| read.contains(axis)))
    {
        return Err(molecular::Error::WedgeAssignment(
            "The wedges in this layout cannot show the atropisomer axis next to a wedged stereocentre"
                .into(),
        )
        .into());
    }
    Ok(Prepared {
        molecule,
        document,
        warnings: Vec::new(),
        file_xyz: false,
    })
}

/// Atropisomer axes by stable atom IDs.
fn axes(molecule: &molecular::Molecule) -> Result<Vec<(u64, u64, u8)>, Error> {
    let id = |atom: usize| {
        molecule
            .ids
            .get(atom)
            .copied()
            .ok_or_else(|| Error::Document("Missing atom ID".into()))
    };
    let mut axes = Vec::new();
    for (bond, meta) in molecule
        .state
        .graph
        .bonds
        .iter()
        .zip(&molecule.state.metadata.bonds)
    {
        if matches!(meta.stereo, 6 | 7) {
            let (a, b) = (id(bond.a)?, id(bond.b)?);
            axes.push((a.min(b), a.max(b), meta.stereo));
        }
    }
    axes.sort_unstable();
    Ok(axes)
}

fn prepare_inchi(
    imported: crate::chemistry::inchi::kernel::Imported,
    options: molecular::DrawingOptions,
) -> Result<Prepared, Error> {
    let state = imported.state.ok_or(Error::Parse)?;
    if imported.unspecified_bonds.iter().any(|&value| value) {
        return Err(Error::BondType);
    }
    let mut molecule = molecule(state)?;
    let mut warnings = imported.diagnostics;
    warnings.extend(layout(&mut molecule, &[])?);
    let labels = vec![None; molecule.ids.len()];
    let mut prepared = finish_molecule(molecule, &labels, options)?;
    prepared.warnings.extend(warnings);
    Ok(prepared)
}
