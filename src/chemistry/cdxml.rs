//! Bounded molecular CDXML reader for the pinned ChemDraw-enabled RDKit reader.
//!
//! Adapted from RDKit External/ChemDraw/{node,bond,fragment,chemdraw,utils}.cpp
//! (2026.03.6), Copyright (C) 2024 Glysade Inc. BSD-3-Clause; see
//! licenses/cdxml-native/ for original licenses and attribution. Coordinate conversion follows
//! ChemDraw 1.0.14's signed 16.16 representation.
//!
//! The molecular reader accepts chemical XML after abbreviation expansion and
//! bond display normalization. Detached preprocessing expands explicit
//! abbreviation definitions and normalizes bond depictions. These boundaries
//! preserve raw unsanitized chemistry. `prepare_cdxml` then combines fragments,
//! restores chemical orders and performs the original sanitization and legacy
//! stereo sequence. Scene assembly, final CIP labels and identifiers remain
//! separate. Unsupported queries and malformed fragments fail atomically.
mod abbreviations;
mod arrows;
pub(crate) use arrows::native_hypot;
mod association;
mod attachments;
pub mod bonds;
pub mod graphics;
mod groups;
mod labels;
mod marks;
mod normalize;
mod numeric;
mod parse;
mod preparation;
pub mod presentation;
mod read_abbreviations;
mod scene;
mod stereo;
mod tree;
mod xml_guard;

pub use abbreviations::{Abbreviation, Flattened, flatten_abbreviations};
pub use arrows::{ArrowError, ArrowReader, NativeArrow, NativeArrowStyle};
pub use association::{ImportPoint, ObjectMapEntry, PreparedAtoms};
pub use groups::read_groups;
pub use labels::{
    AtomLabel, BondIndicator, Labels, LabelsError, NativeAtomDisplay, NativeNumber, NativeStereo,
    read_labels,
};
pub use marks::{AtomMarks, Marks, NativeMark, read_marks};
pub use normalize::chemistry_xml;
pub use preparation::{
    PreparationCause, PreparationError, PreparationStage, PreparedCdxml, prepare_cdxml,
};
pub use read_abbreviations::read_abbreviations;
pub use scene::{
    CdxmlScene, ImportedCdxml, NativeCaption, NativeScene, SceneAtom, SceneBond, SceneError,
    assemble_cdxml, import_cdxml,
};

use super::{graph::Graph, kekulize::Direction, ranking::Metadata, stereo::Point3};
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid molecular CDXML: {0}")]
    Invalid(String),
    #[error("CDXML contains unsupported chemistry: {0}")]
    Unsupported(&'static str),
    #[error("CDXML exceeds the size, nesting, or molecular work limit")]
    Limit,
    #[error("CDXML stereochemistry: {0}")]
    Stereo(String),
}
type Result<T> = std::result::Result<T, Error>;

/// Detached unsanitized chemistry. No property-cache or hybridization values
/// are invented: subsequent preparation computes them after restoring drawing
/// bond orders. Fragment and dense atom/bond ordering match the native reader.
#[derive(Clone, Debug, Serialize)]
pub struct Parsed {
    pub fragments: Vec<Fragment>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Fragment {
    /// Native CDX_FRAG_ID, including its signed representation.
    pub id: i32,
    /// Source identifiers retained separately; the native reader erases these.
    pub atom_ids: Vec<u32>,
    pub bond_ids: Vec<u32>,
    /// Native CDX_NODE_ID on standalone external connection points.
    pub fuse_labels: Vec<Option<u32>>,
    pub graph: Graph,
    pub metadata: Metadata,
    pub directions: Vec<Direction>,
    /// Y-up normalized positions; an empty fragment has no conformer.
    pub positions: Vec<Point3>,
    pub is_3d: bool,
    /// Native _MolFileBondCfg values retained after direction clearing.
    pub bond_cfg: Vec<Option<u8>>,
    /// Native non-explicit 3D chirality annotations, when assigned.
    pub non_explicit_3d_chirality: Vec<Option<i32>>,
    /// Native CDX_BOND_CIP retained on double bonds with BS=E/Z.
    pub bond_cip: Vec<Option<u8>>,
    /// Legacy priorities produced only if BS=E/Z needs its native fallback.
    pub atom_cip_ranks: Vec<Option<u32>>,
}

/// Read preflattened molecular CDXML without changing the input.
pub fn read(text: &str) -> Result<Parsed> {
    parse::read(text)
}
