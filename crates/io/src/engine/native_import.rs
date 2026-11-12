//! Finish molecular, drawing and reaction imports without a Python worker.
//!
//! Retained file coordinates and drawing scenes keep their chemical state for
//! analysis. Fresh atropisomer layouts use the stereo their wedges show.
//! Reactions prepare their finished combined drawing, matching the original
//! reaction import path. Layout and drawing construction stay detached; parse,
//! chemistry and helper errors never request a fallback.
use super::{Request, Response, native_response};
use crate::{
    chemistry::{
        self, cdxml, depict, document as molecular,
        inchi::{helper, output},
        kekulize::Direction,
        layout_refinement::{self, ComponentRow},
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
#[cfg(test)]
mod molfile_atrop_tests;
#[cfg(test)]
mod partial_tests;
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
const CONFIGURATION_NOTICE_END: &str = "; it imported that atom without a configuration";
const RADICAL_NOTICE: &str = " radical electrons; ReShiki keeps at most 2 and drew it as a triplet";
const ALLENE_NOTICE_END: &str = " is invalid; ReShiki imported that atom without a configuration";
const QUERY_NOTICE: &str = "The input's query constraints (such as CXSMILES u, rb or s) were ignored; ReShiki imported the plain structure";
const ANY_BOND_NOTICE_END: &str = " has any order (~); bond order assumed single";
const ZERO_BOND_NOTICE_END: &str =
    " has zero order (CXSMILES Z); ReShiki imported the atoms without that bond";

/// Recognize the feature-loss notices emitted only by partial SMILES import.
pub fn is_partial_import_warning(warning: &str) -> bool {
    (warning.starts_with("ReShiki cannot draw the ") && warning.ends_with(CONFIGURATION_NOTICE_END))
        || (warning.starts_with("Atom ") && warning.contains(RADICAL_NOTICE))
        || (warning.starts_with("The allene stereo tag (@AL) at ")
            && warning.ends_with(ALLENE_NOTICE_END))
        || warning == QUERY_NOTICE
        || (warning.starts_with("Bond ")
            && (warning.ends_with(ANY_BOND_NOTICE_END) || warning.ends_with(ZERO_BOND_NOTICE_END)))
}

fn dropped_notices(dropped: impl IntoIterator<Item = smiles::Dropped>) -> Vec<String> {
    use smiles::{Configuration, Dropped};
    let name = |atom: smiles::InputAtom| format!("{}{}", atom.element, atom.position + 1);
    let mut queries = false;
    let mut notices = Vec::new();
    for item in dropped {
        let notice = match item {
            Dropped::Configuration { atom, class } => {
                let class = match class {
                    Configuration::SquarePlanar => "square-planar configuration (@SP)",
                    Configuration::TrigonalBipyramidal => {
                        "trigonal-bipyramidal configuration (@TB)"
                    }
                    Configuration::Octahedral => "octahedral configuration (@OH)",
                };
                format!(
                    "ReShiki cannot draw the {class} at {}{CONFIGURATION_NOTICE_END}",
                    name(atom)
                )
            }
            Dropped::Radical {
                atom,
                electrons,
                hydrogens: (before, after),
            } => {
                let mut notice = format!("Atom {} had {electrons}{RADICAL_NOTICE}", name(atom));
                if before != after {
                    notice.push_str(&format!(
                        "; its hydrogen count changed from {before} to {after}"
                    ));
                }
                notice
            }
            Dropped::Allene(atom) => format!(
                "The allene stereo tag (@AL) at {}{ALLENE_NOTICE_END}",
                name(atom)
            ),
            Dropped::Queries => {
                if queries {
                    continue;
                }
                queries = true;
                QUERY_NOTICE.into()
            }
            Dropped::AnyBond(a, b) => format!("Bond {}–{}{ANY_BOND_NOTICE_END}", name(a), name(b)),
            Dropped::ZeroBond(a, b) => {
                format!("Bond {}–{}{ZERO_BOND_NOTICE_END}", name(a), name(b))
            }
        };
        notices.push(notice);
    }
    notices
}

struct Prepared {
    molecule: molecular::Molecule,
    document: Document,
    warnings: Vec<String>,
    /// The molecule keeps a 3D MOL file's own XYZ, which state its stereo as
    /// RDKit reads them, even where the drawing is a projection of them.
    file_xyz: bool,
}

struct Layout {
    positions: Vec<Point3>,
    warnings: Vec<String>,
    row: ComponentRow,
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
    if !matches!(format, "rxn" | "rsmi" | "prepared") && text.trim().is_empty() {
        return Err(Error::Empty);
    }
    let prepared = match format {
        "prepared" => {
            let imported = request.prepared.ok_or(Error::Parse)?;
            let mut molecule = molecule(imported.state)?;
            let (warnings, row) = layout(&mut molecule, &[])?;
            let mut prepared =
                finish_molecule(molecule, &imported.dummy_labels, drawing_options, &row)?;
            prepared.warnings.extend(warnings);
            prepared
        }
        "inchi" => return Ok(Preparation::Inchi(text)),
        format if crate::crystal::is_crystal_format(format) => {
            let (document, warnings) = crate::crystal::read(format, &text)?;
            return Ok(Preparation::Drawing(Box::new(document), warnings));
        }
        "smiles" | "naming_smiles" => {
            let strict = prepare_smiles(&text, format == "naming_smiles", drawing_options, false);
            if format == "smiles" {
                strict.or_else(|error| {
                    prepare_smiles(&text, false, drawing_options, true).map_err(|_| error)
                })?
            } else {
                strict?
            }
        }
        "mol" => {
            let imported = molfile::read(&text)?;
            let expected = axes(&imported.molecule)?;
            // A 3D file is never Haworth-restored, so the molecule keeps its XYZ.
            let file_xyz = imported.annotations.is_3d;
            let (document, molecule, mut warnings) = imported.display_document()?;
            // Projection bonds cannot carry axis wedges; other file wedges
            // can lose or add axes in the finished drawing too (#1057).
            // If the drawing cannot be prepared, keep the existing import.
            if !expected.is_empty()
                && let Ok(drawn) = molecular::prepare(&document)
                && axes(&drawn)? != expected
            {
                warnings.push("The drawing cannot keep the imported atropisomer axis; editing, Clean Up or export of this drawing will not include it".into());
            }
            Prepared {
                molecule,
                document,
                warnings,
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
                warnings.extend(layout(&mut scene.molecule, &[])?.0);
                scene.conformer_3d = Some(false);
            }
            let imported = scene.into_document()?;
            warnings.extend(imported.notices);
            Prepared {
                molecule: imported.molecule,
                document: imported.document,
                warnings,
                file_xyz: false,
            }
        }
        "rxn" => finish_reaction(reaction::read_rxn(&text)?.drawing()?)?,
        "rsmi" => prepare_reaction_smiles(&text, false)
            .or_else(|error| prepare_reaction_smiles(&text, true).map_err(|_| error))?,
        _ => return Err(Error::Format),
    };
    prepared.document.validate().map_err(Error::Document)?;
    Ok(Preparation::Complete(Box::new(prepared)))
}

fn prepare_smiles(
    text: &str,
    naming: bool,
    drawing_options: molecular::DrawingOptions,
    partial: bool,
) -> Result<Prepared, Error> {
    let mut imported = if partial {
        smiles::read_partial(text)?
    } else {
        smiles::read(text)?
    };
    if naming {
        imported.prepared.state =
            crate::molecule_names::apply_axis_markers(&imported.prepared.state)
                .map_err(|_| Error::AxisMarkers)?;
    }
    // Wedges were read at the 2D input coordinates, which the fresh
    // layout replaces (#1037). Without coordinates the @/@@ tags state
    // the configuration (#1044); smiles::read has already taken any
    // axial configuration from the flags. Either way, draw wedges again
    // from the stereo, keeping wavy bonds' unknown stereo (#1049).
    let directions = &mut imported.prepared.state.directions;
    // Parity keeps the wavy exclusion from #1044.
    let tag_wins = imported.conformers.is_empty()
        && (molecular::refine() || !directions.contains(&Direction::Unknown));
    if tag_wins || imported.conformers.iter().any(|c| !c.is_3d) {
        for direction in directions {
            if matches!(direction, Direction::Wedge | Direction::Hash) {
                *direction = Direction::None;
            }
        }
    }
    let mut molecule = molecule(imported.prepared.state)?;
    let (mut warnings, row) = layout(&mut molecule, &imported.reaction_properties)?;
    // Import perception does not keep cumulene E/Z; say so.
    if imported.dropped_cumulene_ez {
        warnings.push(CUMULENE_EZ_NOTICE.into());
    }
    warnings.extend(dropped_notices(imported.dropped));
    let mut prepared = finish_molecule(
        molecule,
        &imported.prepared.dummy_labels,
        drawing_options,
        &row,
    )?;
    prepared.warnings.extend(warnings);
    Ok(prepared)
}

fn prepare_reaction_smiles(text: &str, partial: bool) -> Result<Prepared, Error> {
    let mut reaction = if partial {
        reaction::read_smiles_partial(text)?
    } else {
        reaction::read_smiles(text)?
    };
    if partial {
        reaction.drop_undrawable_wedges();
    }
    let dropped = dropped_notices(
        reaction
            .reactants
            .iter_mut()
            .chain(&mut reaction.agents)
            .chain(&mut reaction.products)
            .flat_map(|part| std::mem::take(&mut part.dropped)),
    );
    let layout = reaction.layout()?;
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
    prepared.warnings.extend(dropped);
    Ok(prepared)
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
    let (document, warnings) = drawing.finish_with_notices(labels)?;
    let molecule = molecular::prepare(&document)?;
    Ok(Prepared {
        molecule,
        document,
        warnings,
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

fn coordinates(state: &State, properties: &[Vec<(Vec<u8>, Vec<u8>)>]) -> Result<Layout, Error> {
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
            preserve_spiro_rings: molecular::refine(),
            ..depict::Options::default()
        },
    )?;
    let positions = template_retry::improve(state, &ranks, conformer.positions, 1.5);
    let mut layout = chain_layout::improve(state, &ranks, positions);
    let row = if molecular::refine() {
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
        layout_refinement::arrange_components(state, &mut layout.positions)
    } else {
        ComponentRow::default()
    };
    Ok(Layout {
        positions: layout.positions,
        warnings: layout.warnings,
        row,
    })
}

fn layout(
    molecule: &mut molecular::Molecule,
    properties: &[Vec<(Vec<u8>, Vec<u8>)>],
) -> Result<(Vec<String>, ComponentRow), Error> {
    molecular::validate_molecule(molecule)?;
    let layout = coordinates(&molecule.state, properties)?;
    molecule.positions = layout.positions;
    Ok((layout.warnings, layout.row))
}

fn finish_molecule(
    mut molecule: molecular::Molecule,
    labels: &[Option<String>],
    options: molecular::DrawingOptions,
    row: &ComponentRow,
) -> Result<Prepared, Error> {
    let drawing = molecular::for_generated(&molecule, labels, options)?;
    let labels = drawing.labels()?;
    let mut document = drawing.finish(labels)?;
    layout_refinement::space_component_labels(row, &mut molecule, &mut document);
    document.validate().map_err(Error::Document)?;
    let mut warnings = Vec::new();
    let expected = axes(&molecule)?;
    let mut readback = None;
    if !expected.is_empty() {
        let mut visible = document.clone();
        // Stored source windings must not conceal centres the wedges lost.
        for atom in &mut visible.atoms {
            atom.stereo = None;
        }
        // Named dummy atoms can prevent preparation; keep their import path.
        if let Ok(mut read) = molecular::prepare(&visible) {
            let source_centres = centres(&molecule);
            // A failed repair retains the reference wedges, which can reverse
            // an axis. Remove only its own carriers, not a neighbour's wedge.
            let mut changed = false;
            loop {
                let unwanted: Vec<_> = axes(&read)?
                    .into_iter()
                    .filter(|axis| !expected.contains(axis))
                    .flat_map(|(a, b, _)| [a, b])
                    .collect();
                let mut cleared = false;
                for bond in &mut visible.bonds {
                    if !bond.projection
                        && bond.order == 1
                        && unwanted.contains(&bond.a)
                        && matches!(
                            bond.display.as_str(),
                            "wedge" | "hollow_wedge" | "bold" | "hash" | "hashed"
                        )
                    {
                        bond.display = "plain".into();
                        cleared = true;
                    }
                }
                if !cleared {
                    break;
                }
                changed = true;
                read = molecular::prepare(&visible)?;
            }
            let actual = axes(&read)?;
            for (a, b, stereo) in expected {
                if !actual.contains(&(a, b, stereo)) {
                    let a = chemistry::stereo_lint::atom_name(&visible, a);
                    let b = chemistry::stereo_lint::atom_name(&visible, b);
                    warnings.push(format!(
                        "The wedges in this layout cannot show the atropisomer axis {a}–{b}; ReShiki drew the structure without that axis"
                    ));
                }
            }
            let read_centres = centres(&read);
            for id in source_centres {
                if !read_centres.contains(&id) {
                    let atom = chemistry::stereo_lint::atom_name(&visible, id);
                    warnings.push(format!(
                        "The wedges in this layout cannot show the stereocentre {atom}; ReShiki drew the structure without it"
                    ));
                }
            }
            // Labels and the first analysis must describe the same wedges as
            // later preparation/export. Preserve the finished drawing's paint.
            // A drawing that shows every source element stays as finished.
            if changed || !warnings.is_empty() {
                let drawing = molecular::for_drawing(&read, &visible)?;
                let labels = drawing.labels()?;
                document = drawing.finish_preserving(labels, &visible)?;
                readback = Some(read);
            }
        }
    }
    warnings.extend(chemistry::drop_added_axis_wedges(&mut document, &molecule));
    if molecule
        .state
        .metadata
        .groups
        .iter()
        .any(|group| !group.bonds.is_empty())
    {
        warnings.push(chemistry::STEREO_GROUP_BONDS_NOTICE.into());
    }
    if let Some(read) = readback {
        molecule = read;
    }
    Ok(Prepared {
        molecule,
        document,
        warnings,
        file_xyz: false,
    })
}

/// Tetrahedral centres the molecule specifies, by stable atom IDs. Wedges
/// either show a generated centre's configuration or leave it unspecified.
fn centres(molecule: &molecular::Molecule) -> std::collections::BTreeSet<u64> {
    molecule
        .ids
        .iter()
        .zip(&molecule.state.metadata.atoms)
        .filter(|(_, atom)| matches!(atom.chiral_tag, 1 | 2))
        .map(|(&id, _)| id)
        .collect()
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
    let (layout_warnings, row) = layout(&mut molecule, &[])?;
    warnings.extend(layout_warnings);
    let labels = vec![None; molecule.ids.len()];
    let mut prepared = finish_molecule(molecule, &labels, options, &row)?;
    prepared.warnings.extend(warnings);
    Ok(prepared)
}
