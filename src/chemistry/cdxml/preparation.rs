//! Chemical prefix of engine/worker.py::import_cdxml, through legacy stereo.
//! Fragment concatenation follows RDKit ChemTransforms.cpp and RWMol.cpp
//! (2026.03.6), Copyright (C) 2006-2021 Greg Landrum (ChemTransforms), and
//! Copyright (C) 2003-2024 Greg Landrum and other contributors (RWMol); BSD-3-Clause,
//! see licenses/rdkit/LICENSE and NOTICE. No scene objects are assembled here.
use super::{
    Abbreviation, Fragment, ObjectMapEntry,
    bonds::{self, NativeBond},
    presentation::{self, NativeDrawingStyle, Palette},
    tree::Tree,
};
use crate::chemistry::{
    RDKIT_VERSION,
    document::Molecule,
    graph::Graph,
    kekulize::Direction,
    ranking::{Metadata, StereoGroup},
    sanitize,
    stereo::{self, Point3, perception},
};
use serde::Serialize;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PreparationStage {
    Validation,
    Expansion,
    Normalization,
    Parser,
    Bindings,
    Combination,
    Style,
    Scaling,
    Bonds,
    Restoration,
    Sanitization,
    Chirality,
    Detection,
    Legacy,
}
#[derive(Debug, thiserror::Error)]
#[error("CDXML {stage:?}: {cause}")]
pub struct PreparationError {
    pub stage: PreparationStage,
    #[source]
    pub cause: PreparationCause,
}
#[derive(Debug, thiserror::Error)]
pub enum PreparationCause {
    #[error(transparent)]
    Xml(#[from] super::Error),
    #[error(transparent)]
    Presentation(#[from] presentation::Error),
    #[error(transparent)]
    Bonds(#[from] bonds::Error),
    #[error(transparent)]
    Sanitization(#[from] sanitize::Error),
    #[error("Unsupported query or reaction predicate: {0}")]
    Predicate(&'static str),
    #[error("CDXML contains unsupported drawing objects or multiple pages.")]
    Objects,
    #[error("{0}")]
    Chemistry(String),
    #[error("{0}")]
    Invariant(&'static str),
    #[error("CDXML preparation exceeds its atom, bond, or storage limit")]
    Limit,
}
type Result<T> = std::result::Result<T, PreparationError>;
fn at<T, E: Into<PreparationCause>>(
    stage: PreparationStage,
    result: std::result::Result<T, E>,
) -> Result<T> {
    result.map_err(|error| PreparationError {
        stage,
        cause: error.into(),
    })
}
fn error(stage: PreparationStage, cause: PreparationCause) -> PreparationError {
    PreparationError { stage, cause }
}

/// Detached chemical state plus original-precision inputs for later readers.
/// `expanded_xml` is the single immutable source of document-order ordinals.
/// `fragments` snapshot the parts at the bond-reader call: only a single part
/// shares the combined molecule's scaled positions; chemistry is still raw.
#[derive(Clone, Debug, Serialize)]
pub struct PreparedCdxml {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<crate::attachments::Attachment>,
    pub molecule: Molecule,
    pub expanded_xml: String,
    pub abbreviations: Vec<Abbreviation>,
    pub fragments: Vec<Fragment>,
    pub fragment_bindings: Vec<ObjectMapEntry>,
    pub drawing_style: NativeDrawingStyle,
    pub bonds: Vec<NativeBond>,
    pub palette: Palette,
    pub source_scale: f64,
    pub conformer_scale: f64,
    /// None means the native combined molecule has no conformer.
    pub conformer_3d: Option<bool>,
}

fn validate(text: &str) -> Result<Tree> {
    use PreparationStage::Validation;
    let tree = at(Validation, Tree::parse_import(text))?;
    let nodes = at(Validation, tree.descendants(0))?;
    // Preserve the original two scans: predicates on any n/b are checked before
    // unsupported object tags, including chemical-looking children in pictures.
    const PREDICATES: &[(&str, &[&str])] = &[
        ("RingBondCount", &["Unspecified", "-1"]),
        ("UnsaturatedBonds", &["Unspecified", "0"]),
        ("SubstituentsUpTo", &[]),
        ("SubstituentsExactly", &[]),
        ("FreeSites", &["0"]),
        ("LinkCountLow", &[]),
        ("LinkCountHigh", &[]),
        ("IsotopicAbundance", &["Unspecified", "0"]),
        ("Topology", &["Unspecified", "0"]),
        ("RxnChange", &["no", "0"]),
        ("RxnStereo", &["Unspecified", "0"]),
        ("RxnParticipation", &["Unspecified", "0"]),
    ];
    for &index in &nodes {
        let node = at(Validation, tree.node(index))?;
        if matches!(node.tag.as_str(), "n" | "b") {
            for &(name, allowed) in PREDICATES {
                if node
                    .attr(name)
                    .is_some_and(|value| !allowed.contains(&value))
                {
                    return Err(error(Validation, PreparationCause::Predicate(name)));
                }
            }
        }
    }
    let mut pages = 0;
    for index in nodes {
        let node = at(Validation, tree.node(index))?;
        if node.tag == "page" {
            pages += 1;
        }
        if !matches!(
            node.tag.as_str(),
            "CDXML"
                | "page"
                | "fragment"
                | "n"
                | "b"
                | "t"
                | "s"
                | "fonttable"
                | "font"
                | "colortable"
                | "color"
                | "arrow"
                | "graphic"
                | "curve"
                | "group"
                | "represent"
                | "objecttag"
                | "embeddedobject"
        ) {
            return Err(error(Validation, PreparationCause::Objects));
        }
    }
    if pages != 1 {
        return Err(error(Validation, PreparationCause::Objects));
    }
    Ok(tree)
}

fn bindings(tree: &Tree, fragments: &[Fragment]) -> Result<Vec<ObjectMapEntry>> {
    use PreparationStage::Bindings;
    let mut sources = HashMap::new();
    for (ordinal, index) in at(Bindings, tree.descendants(0))?.into_iter().enumerate() {
        let node = at(Bindings, tree.node(index))?;
        if node.tag == "fragment" {
            sources.insert(node.attr("id"), ordinal);
        }
    }
    let mut patches: Vec<ObjectMapEntry> = Vec::new();
    let mut slots: HashMap<usize, usize> = HashMap::new();
    let mut offset = 0usize;
    for part in fragments {
        let next = offset
            .checked_add(part.graph.atoms.len())
            .filter(|n| *n <= 100_000)
            .ok_or_else(|| error(Bindings, PreparationCause::Limit))?;
        if let Some(&source) = sources.get(&Some(part.id.to_string().as_str())) {
            let atoms = (offset..next)
                .map(|i| u64::try_from(i + 1).map_err(|_| error(Bindings, PreparationCause::Limit)))
                .collect::<Result<Vec<_>>>()?;
            if let Some(&slot) = slots.get(&source) {
                let patch = patches.get_mut(slot).ok_or_else(|| {
                    error(
                        Bindings,
                        PreparationCause::Invariant("Missing fragment binding"),
                    )
                })?;
                patch.atoms = atoms;
            } else {
                slots.insert(source, patches.len());
                patches.push(ObjectMapEntry { source, atoms });
            }
        }
        offset = next;
    }
    Ok(patches)
}

struct Combined {
    graph: Graph,
    metadata: Metadata,
    directions: Vec<Direction>,
    positions: Vec<Point3>,
    conformer_3d: Option<bool>,
}
fn combine(parts: &[Fragment]) -> Result<Combined> {
    use PreparationStage::Combination;
    let mut result = Combined {
        graph: Graph {
            atoms: Vec::new(),
            bonds: Vec::new(),
        },
        metadata: Metadata::default(),
        directions: Vec::new(),
        positions: Vec::new(),
        conformer_3d: None,
    };
    // insertStereoGroups merges ABS groups and moves the combined ABS to the
    // end only if a subsequent part contributes any group. A linear collection
    // produces the same order without rescanning all prior groups per part.
    let merge_groups = parts.iter().skip(1).any(|p| !p.metadata.groups.is_empty());
    let mut absolute = StereoGroup::default();
    for (part_index, part) in parts.iter().enumerate() {
        let atom_offset = result.graph.atoms.len();
        let bond_offset = result.graph.bonds.len();
        let n = atom_offset
            .checked_add(part.graph.atoms.len())
            .filter(|n| *n <= 100_000)
            .ok_or_else(|| error(Combination, PreparationCause::Limit))?;
        bond_offset
            .checked_add(part.graph.bonds.len())
            .filter(|n| *n <= 300_000)
            .ok_or_else(|| error(Combination, PreparationCause::Limit))?;
        if part.positions.len() != part.graph.atoms.len()
            || part.metadata.atoms.len() != part.graph.atoms.len()
            || part.metadata.bonds.len() != part.graph.bonds.len()
            || part.directions.len() != part.graph.bonds.len()
        {
            return Err(error(
                Combination,
                PreparationCause::Invariant("Fragment dimensions changed"),
            ));
        }
        result.graph.atoms.extend_from_slice(&part.graph.atoms);
        result
            .metadata
            .atoms
            .extend_from_slice(&part.metadata.atoms);
        result.directions.extend_from_slice(&part.directions);
        if result.conformer_3d.is_some() && !part.positions.is_empty() {
            // CombineMols adds its default zero offset to incoming conformers.
            // Retain the native signed-zero result of those additions.
            result
                .positions
                .extend(part.positions.iter().map(|p| Point3 {
                    x: p.x + 0.0,
                    y: p.y + 0.0,
                    z: p.z + 0.0,
                }));
        } else {
            result.positions.extend_from_slice(&part.positions);
        }
        if !part.positions.is_empty() && result.conformer_3d.is_none() {
            result.conformer_3d = Some(part.is_3d);
        }
        let shifted_atom = |index: usize| -> Result<usize> {
            index
                .checked_add(atom_offset)
                .filter(|i| *i < n)
                .ok_or_else(|| {
                    error(
                        Combination,
                        PreparationCause::Invariant("Missing combined atom"),
                    )
                })
        };
        for bond in &part.graph.bonds {
            let mut bond = bond.clone();
            bond.a = shifted_atom(bond.a)?;
            bond.b = shifted_atom(bond.b)?;
            result.graph.bonds.push(bond);
        }
        for metadata in &part.metadata.bonds {
            let mut metadata = metadata.clone();
            metadata.stereo_atoms = metadata
                .stereo_atoms
                .into_iter()
                .map(shifted_atom)
                .collect::<Result<_>>()?;
            result.metadata.bonds.push(metadata);
        }
        for group in &part.metadata.groups {
            let mut group = group.clone();
            group.atoms = group
                .atoms
                .into_iter()
                .map(shifted_atom)
                .collect::<Result<_>>()?;
            group.bonds = group
                .bonds
                .into_iter()
                .map(|i| {
                    i.checked_add(bond_offset)
                        .filter(|i| *i < result.graph.bonds.len())
                        .ok_or_else(|| {
                            error(
                                Combination,
                                PreparationCause::Invariant("Missing combined bond"),
                            )
                        })
                })
                .collect::<Result<_>>()?;
            if merge_groups && group.kind == 0 {
                absolute.atoms.extend(group.atoms);
                absolute.bonds.extend(group.bonds);
            } else {
                if part_index != 0 {
                    group.write_id = 0;
                }
                result.metadata.groups.push(group);
            }
        }
    }
    if merge_groups && (!absolute.atoms.is_empty() || !absolute.bonds.is_empty()) {
        result.metadata.groups.push(absolute);
    }
    Ok(result)
}

/// Execute only the original chemical import prefix. Inputs are immutable and
/// no result is published until all preparation stages succeed. Raw f64 data
/// remains unrounded; scene assembly and final document validation are separate.
pub fn prepare_cdxml(text: &str) -> Result<PreparedCdxml> {
    prepare(text, true)
}

/// Preserve structurally valid drawing input when chemical validation fails.
/// This state is for scene reconstruction only, never molecular properties.
pub(crate) fn prepare_drawing(text: &str) -> Result<PreparedCdxml> {
    prepare(text, false)
}

fn prepare(text: &str, check_chemistry: bool) -> Result<PreparedCdxml> {
    use PreparationStage::*;
    let mut source = validate(text)?;
    at(Validation, super::attachments::normalize(&mut source))?;
    let molecular_root = at(Validation, source.node(0))?.tag == "CDXML";
    let flattened = at(Expansion, super::abbreviations::flatten_tree(source))?;
    let chemical = at(
        Normalization,
        super::normalize::normalize_tree(at(Normalization, Tree::parse_import(&flattened.xml))?),
    )?;
    // The native ChemDraw reader recognizes a CDXML root only. Other permitted
    // roots can contain supported captions/graphics; read_bonds still rejects
    // any molecular fragment the native parser did not read.
    let mut fragments = if molecular_root {
        at(Parser, super::read(&chemical))?.fragments
    } else {
        Vec::new()
    };
    let tree = at(Bindings, Tree::parse_import(&flattened.xml))?;
    let attachments = at(Bindings, super::attachments::read(&tree, &fragments))?;
    let fragment_bindings = bindings(&tree, &fragments)?;
    let mut combined = combine(&fragments)?;
    let document = at(Style, presentation::parse(&flattened.xml))?;
    let drawing_style = at(Style, presentation::drawing_style(document.root_element()))?;
    let source_scale = 1.0 / (14.4 / 42.0);
    let conformer_scale =
        drawing_style.bond_length_pt / at(Scaling, NativeDrawingStyle::defaults())?.bond_length_pt;
    for p in &mut combined.positions {
        p.x *= conformer_scale;
        p.y *= conformer_scale;
        p.z *= conformer_scale;
    }
    if let [part] = fragments.as_mut_slice() {
        part.positions.clone_from(&combined.positions);
    }
    let palette = at(Bonds, presentation::palette(document.root_element()))?;
    let bonds = at(
        Bonds,
        bonds::read_native(&flattened.xml, &fragments, source_scale, &palette.colors),
    )?;
    let mut pairs = HashMap::new();
    for (index, bond) in combined.graph.bonds.iter().enumerate() {
        pairs.insert((bond.a.min(bond.b), bond.a.max(bond.b)), index);
    }
    for bond in &bonds {
        let index = |id: u64| {
            id.checked_sub(1)
                .and_then(|id| usize::try_from(id).ok())
                .ok_or_else(|| {
                    error(
                        Restoration,
                        PreparationCause::Invariant("Invalid restored atom ID"),
                    )
                })
        };
        let (a, b) = (index(bond.a)?, index(bond.b)?);
        let index = pairs.get(&(a.min(b), a.max(b))).ok_or_else(|| {
            error(
                Restoration,
                PreparationCause::Invariant("Missing chemical bond during restoration"),
            )
        })?;
        let chemical = combined.graph.bonds.get_mut(*index).ok_or_else(|| {
            error(
                Restoration,
                PreparationCause::Invariant("Missing restored chemical bond"),
            )
        })?;
        chemical.order = bond.order;
        if bond.order == 4 {
            chemical.aromatic = true;
            for index in [chemical.a, chemical.b] {
                combined
                    .graph
                    .atoms
                    .get_mut(index)
                    .ok_or_else(|| {
                        error(
                            Restoration,
                            PreparationCause::Invariant("Missing aromatic atom"),
                        )
                    })?
                    .aromatic = true;
            }
        }
    }
    let state = if check_chemistry {
        chemical_state(&combined)?
    } else {
        let graph = &combined.graph;
        at(
            Sanitization,
            graph.validate().map_err(PreparationCause::Chemistry),
        )?;
        at(
            Sanitization,
            combined
                .metadata
                .validate(graph)
                .map_err(PreparationCause::Chemistry),
        )?;
        perception::State {
            graph: graph.clone(),
            metadata: combined.metadata.clone(),
            directions: combined.directions.clone(),
            valences: at(
                Sanitization,
                graph
                    .provisional_valences()
                    .map_err(PreparationCause::Chemistry),
            )?,
            conjugated: vec![false; graph.bonds.len()],
            hybridizations: vec![
                crate::chemistry::electronic::Hybridization::Unspecified;
                graph.atoms.len()
            ],
            rings: perception::RingCache {
                kind: perception::RingKind::Symmetric,
                atoms: at(
                    Sanitization,
                    crate::chemistry::rings::perceive(graph, Default::default())
                        .map_err(|e| PreparationCause::Chemistry(e.to_string())),
                )?
                .atoms,
            },
            properties: perception::Properties::unspecified(graph),
        }
    };
    let ids = (0..state.graph.atoms.len())
        .map(|i| u64::try_from(i + 1).map_err(|_| error(Combination, PreparationCause::Limit)))
        .collect::<Result<_>>()?;
    Ok(PreparedCdxml {
        attachments,
        molecule: Molecule {
            rdkit_version: RDKIT_VERSION,
            ids,
            positions: combined.positions,
            state,
        },
        expanded_xml: flattened.xml,
        abbreviations: flattened.abbreviations,
        fragments,
        fragment_bindings,
        drawing_style,
        bonds,
        palette,
        source_scale,
        conformer_scale,
        conformer_3d: combined.conformer_3d,
    })
}

fn chemical_state(combined: &Combined) -> Result<perception::State> {
    use PreparationStage::*;
    let sanitized = at(
        Sanitization,
        sanitize::sanitize(&combined.graph, &combined.metadata, &combined.directions),
    )?;
    let positions = combined.conformer_3d.map(|_| combined.positions.as_slice());
    let drawn = at(
        Chirality,
        stereo::from_directions(
            &sanitized.graph,
            &sanitized.metadata,
            &sanitized.directions,
            positions,
            false,
        )
        .map_err(PreparationCause::Chemistry),
    )?;
    let geometry = at(
        Detection,
        stereo::detect_bond_stereo(
            &drawn.graph,
            &drawn.metadata,
            &sanitized.directions,
            positions,
            &sanitized.rings,
        )
        .map_err(PreparationCause::Chemistry),
    )?;
    // SanitizeMol clears computed properties, including raw fallback CIP ranks.
    // Noncomputed CDXML annotations remain in the separate source fragments.
    let properties = perception::Properties::unspecified(&drawn.graph);
    at(
        Legacy,
        perception::perceive(
            &perception::State {
                graph: drawn.graph,
                metadata: geometry.metadata,
                directions: geometry.directions,
                valences: drawn.valences,
                conjugated: sanitized.conjugated,
                hybridizations: sanitized.hybridizations,
                rings: perception::RingCache {
                    kind: perception::RingKind::Symmetric,
                    atoms: sanitized.rings,
                },
                properties,
            },
            perception::Options {
                clean: false,
                force: true,
                flag_possible: false,
            },
        )
        .map_err(PreparationCause::Chemistry),
    )
}
