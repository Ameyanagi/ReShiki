//! MOL parsing adapted from RDKit MolFileParser.cpp (2026.03.6).
//! Copyright (C) 2002-2021 Greg Landrum and other RDKit contributors.
//! BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
use crate::chemistry::{
    ELEMENTS, RDKIT_VERSION,
    document::Molecule,
    graph::{Atom, Bond, Graph},
    kekulize::Direction,
    ranking::{AtomMetadata, BondMetadata, Metadata},
    sanitize,
    stereo::{self, Point3, perception},
};
use serde::Serialize;
use std::str::Lines;
mod groups;
mod reaction;
mod v2000;
mod v3000;
pub(crate) use reaction::read as read_reaction;

#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    #[error("Invalid MOL input at line {line}: {message}")]
    Invalid { line: usize, message: String },
    #[error("MOL input contains unsupported chemistry: {0}")]
    Unsupported(&'static str),
    #[error("MOL input exceeds the size or work limit")]
    Limit,
    #[error(transparent)]
    Sanitization(#[from] sanitize::Error),
    #[error(transparent)]
    Spatial(#[from] stereo::SpatialError),
    #[error(transparent)]
    Atropisomer(#[from] stereo::AtropError),
    #[error("MOL chemistry: {0}")]
    Chemistry(String),
}
type Result<T> = std::result::Result<T, ReadError>;

/// Chemical state plus file annotations needed by the drawing and native bridge.
#[derive(Clone, Debug, Serialize)]
pub struct Imported {
    pub molecule: Molecule,
    pub annotations: FileAnnotations,
}

#[derive(Clone, Debug, Serialize)]
pub struct FileAnnotations {
    /// Original ChemDraw directions for the application's Haworth recognition.
    /// Excluded from the RDKit-compatible serialized parser snapshot.
    #[serde(skip)]
    pub chemdraw_directions: Vec<Direction>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<crate::attachments::Attachment>,
    pub is_3d: bool,
    pub attachment_points: Vec<Option<i32>>,
    pub dummy_labels: Vec<Option<String>>,
}

impl Imported {
    pub(crate) fn restore_haworth(&self, document: &mut crate::document::Document) -> bool {
        if self.annotations.is_3d || self.annotations.chemdraw_directions.is_empty() {
            return false;
        }
        let mut source = document.clone();
        let mut originals = std::collections::HashMap::new();
        for (bond, direction) in self
            .molecule
            .state
            .graph
            .bonds
            .iter()
            .zip(&self.annotations.chemdraw_directions)
        {
            let (Some(&a), Some(&b)) =
                (self.molecule.ids.get(bond.a), self.molecule.ids.get(bond.b))
            else {
                return false;
            };
            originals.insert((a.min(b), a.max(b)), (a, b, direction));
        }
        for bond in &mut source.bonds {
            let Some(&(a, b, direction)) = originals.get(&(bond.a.min(bond.b), bond.a.max(bond.b)))
            else {
                return false;
            };
            if bond.a != a {
                bond.stereo_atoms.reverse();
            }
            bond.a = a;
            bond.b = b;
            if bond.order == 1 {
                bond.display = match direction {
                    Direction::Wedge => "wedge",
                    Direction::Hash => "hash",
                    Direction::Unknown => "wavy",
                    _ => "plain",
                }
                .into();
            }
        }
        if crate::haworth::interchange::restore_mol(&mut source) {
            *document = source;
            true
        } else {
            false
        }
    }

    pub fn drawing(
        &self,
    ) -> std::result::Result<crate::chemistry::document::Drawing, crate::chemistry::document::Error>
    {
        crate::chemistry::document::for_import(
            &self.molecule,
            self.annotations.is_3d,
            &self.annotations.dummy_labels,
        )?
        .with_attachments(&self.annotations.attachments)
    }
}

struct Reader<'a> {
    lines: Lines<'a>,
    line: usize,
}
impl<'a> Reader<'a> {
    fn invalid(&self, message: impl Into<String>) -> ReadError {
        ReadError::Invalid {
            line: self.line,
            message: message.into(),
        }
    }
    fn next(&mut self) -> Result<&'a str> {
        self.line += 1;
        self.lines
            .next()
            .ok_or_else(|| self.invalid("Unexpected end of file"))
    }
    fn field<'s>(&self, text: &'s str, start: usize, end: usize) -> Result<&'s str> {
        text.get(start..end)
            .ok_or_else(|| self.invalid("Truncated or non-ASCII field"))
    }
    fn integer(&self, text: &str) -> Result<i32> {
        let value = text.trim_matches(' ');
        if value.is_empty() {
            return Ok(0);
        }
        value
            .parse()
            .map_err(|_| self.invalid(format!("Invalid integer {text:?}")))
    }
    fn field_integer(&self, text: &str, start: usize, end: usize) -> Result<i32> {
        self.integer(self.field(text, start, end)?)
    }
    fn optional_integer(&self, text: &str, start: usize, end: usize) -> Result<i32> {
        if text.len() < end {
            Ok(0)
        } else {
            self.field_integer(text, start, end)
        }
    }
    fn count(&self, value: i32, limit: usize) -> Result<usize> {
        let n = usize::try_from(value).map_err(|_| self.invalid("Negative count"))?;
        if n > limit {
            Err(ReadError::Limit)
        } else {
            Ok(n)
        }
    }
    fn coordinate(&self, text: &str, fixed: bool) -> Result<f64> {
        let text = text.trim();
        if text.is_empty() {
            return Ok(0.);
        }
        if fixed
            && text
                .bytes()
                .any(|b| !b.is_ascii_digit() && !b" +-,.".contains(&b))
        {
            return Err(self.invalid("Invalid fixed-width coordinate"));
        }
        let n: f64 = text
            .parse()
            .map_err(|_| self.invalid("Invalid coordinate"))?;
        if !n.is_finite() || n.abs() > 1e100 {
            return Err(self.invalid("Nonfinite or excessive coordinate"));
        }
        Ok(n)
    }
    fn index(&self, value: i32, count: usize) -> Result<usize> {
        let i = usize::try_from(value).ok().and_then(|n| n.checked_sub(1));
        i.filter(|&i| i < count)
            .ok_or_else(|| self.invalid("Missing atom or bond reference"))
    }
    fn symbol(&self, symbol: &str, v3000: bool) -> Result<Atom> {
        let mut atom = Atom::default();
        match symbol {
            "D" | "T" => {
                atom.atomic_number = 1;
                atom.isotope = if symbol == "D" { 2 } else { 3 };
            }
            "L" | "LP" if !v3000 => (),
            "Pol" | "Mod" | "R#" => (),
            "R" if v3000 => (),
            _ if symbol.starts_with('R') && ("R0"..="R99").contains(&symbol) => {
                // The reference compares labels lexically, so R123 is also
                // a dummy label; malformed numeric suffixes keep isotope zero.
                atom.isotope = symbol
                    .get(1..)
                    .and_then(|n| n.parse::<i32>().ok())
                    .filter(|&n| n >= 0)
                    .map(|n| n as u16)
                    .unwrap_or(0);
            }
            _ => {
                let normalized = if symbol.len() == 2 && symbol.is_ascii() {
                    let mut chars = symbol.chars();
                    match (chars.next(), chars.next()) {
                        (Some(a), Some(b)) => format!("{a}{}", b.to_ascii_lowercase()),
                        _ => return Err(self.invalid("Empty element")),
                    }
                } else {
                    symbol.to_owned()
                };
                atom.atomic_number = ELEMENTS
                    .iter()
                    .position(|e| e.symbol == normalized && e.symbol != "*")
                    .and_then(|n| u8::try_from(n).ok())
                    .ok_or(ReadError::Unsupported("query or unknown atom symbol"))?;
            }
        }
        Ok(atom)
    }
}

#[derive(Default)]
enum FileQuery {
    #[default]
    None,
    Number(u32),
    Other,
}
#[derive(Default)]
struct FileAtom {
    valence: i32,
    hyd_override: bool,
    attachment: Option<i32>,
    dummy_label: Option<String>,
    query: FileQuery,
}

fn dummy_label(symbol: &str) -> Option<String> {
    (matches!(symbol, "R" | "R#" | "Pol" | "Mod") || ("R0"..="R99").contains(&symbol))
        .then(|| symbol.to_owned())
}
#[derive(Default)]
struct FileBond {
    attachment: Option<crate::attachments::Attachment>,
    unspecified: bool,
    query: bool,
}
struct Parsed {
    chemdraw: bool,
    graph: Graph,
    metadata: Metadata,
    directions: Vec<Direction>,
    positions: Vec<Point3>,
    atoms: Vec<FileAtom>,
    bonds: Vec<FileBond>,
    groups: groups::Groups,
    chirality: bool,
    marked_3d: bool,
}
#[derive(Clone, Copy)]
enum Context {
    Molecule,
    Reaction { agent: bool, v3000: bool },
}
impl Parsed {
    fn new(marked_3d: bool) -> Self {
        Self {
            chemdraw: false,
            graph: Graph {
                atoms: Vec::new(),
                bonds: Vec::new(),
            },
            metadata: Metadata::default(),
            directions: Vec::new(),
            positions: Vec::new(),
            atoms: Vec::new(),
            bonds: Vec::new(),
            groups: groups::Groups::default(),
            chirality: false,
            marked_3d,
        }
    }
    fn atom(&mut self, atom: Atom, position: Point3, meta: AtomMetadata, props: FileAtom) {
        self.graph.atoms.push(atom);
        self.positions.push(position);
        self.metadata.atoms.push(meta);
        self.atoms.push(props);
    }
    fn bond(&mut self, a: usize, b: usize, order: u8, dir: Direction, props: FileBond) {
        // Graph validation also rejects self-bonds and duplicate endpoints.
        // Unlike the drawing bridge, file parsing marks only aromatic bonds.
        // Sanitization later determines atom aromaticity, including the native
        // behavior for aromatic-order bonds outside rings.
        self.graph.bonds.push(Bond {
            a,
            b,
            order,
            aromatic: order == 4,
        });
        self.metadata.bonds.push(BondMetadata {
            stereo: u8::from(dir == Direction::EitherDouble),
            ..BondMetadata::default()
        });
        self.directions.push(dir);
        self.bonds.push(props);
    }
    fn finish(self) -> Result<Imported> {
        self.finish_in(Context::Molecule)
    }

    fn finish_in(mut self, context: Context) -> Result<Imported> {
        let chemdraw_directions = if self.chemdraw && matches!(context, Context::Molecule) {
            self.directions.clone()
        } else {
            Vec::new()
        };
        if !matches!(context, Context::Molecule)
            && self.bonds.iter().any(|b| b.attachment.is_some())
        {
            return Err(ReadError::Unsupported(
                "RXN attachment targets; use CDXML or native ReShiki",
            ));
        }
        let sanitize_file = !matches!(
            context,
            Context::Reaction {
                agent: true,
                v3000: false
            }
        );
        self.graph.validate().map_err(ReadError::Chemistry)?;
        let cache = self
            .graph
            .provisional_valences()
            .map_err(ReadError::Chemistry)?;
        for ((atom, props), valence) in self.graph.atoms.iter_mut().zip(&self.atoms).zip(cache) {
            if props.valence != 0 && !props.hyd_override {
                atom.no_implicit = true;
                let hs = if matches!(props.valence, -1 | 15) {
                    0
                } else {
                    props
                        .valence
                        .saturating_sub(valence.explicit_valence as i32)
                        .max(0)
                };
                atom.explicit_hydrogens = u8::try_from(hs)
                    .map_err(|_| ReadError::Unsupported("excessive explicit hydrogens"))?;
            }
        }
        let groups = std::mem::take(&mut self.groups);
        groups.apply(&mut self)?;
        for (atom, props) in self.graph.atoms.iter().zip(&self.atoms) {
            match props.query {
                FileQuery::None => (),
                FileQuery::Number(n)
                    if matches!(context, Context::Reaction { .. })
                        && n == u32::from(atom.atomic_number) => {}
                _ => return Err(ReadError::Unsupported("substance-group query")),
            }
        }
        if self.bonds.iter().any(|b| b.unspecified || b.query) {
            return Err(ReadError::Unsupported("query or unspecified bond order"));
        }
        let is_3d =
            self.positions.iter().any(|p| p.z.abs() > 1e-3) || self.marked_3d && !self.chirality;
        let conformer = stereo::wedging::Conformer {
            positions: self.positions,
            is_3d,
        };
        if is_3d {
            self.metadata = stereo::from_3d(
                &self.graph,
                &self.metadata,
                &self.directions,
                Some(&conformer),
                &stereo::SpatialAnnotations {
                    non_explicit: vec![None; self.graph.atoms.len()],
                    done: None,
                },
                stereo::SpatialOptions::default(),
            )?
            .metadata;
        } else if self.chirality {
            let drawn = stereo::from_directions(
                &self.graph,
                &self.metadata,
                &self.directions,
                Some(&conformer.positions),
                true,
            )
            .map_err(ReadError::Chemistry)?;
            self.graph = drawn.graph;
            self.metadata = drawn.metadata;
        }
        self.metadata = stereo::detect_atropisomers(
            &self.graph,
            &self.metadata,
            &self.directions,
            Some(&conformer),
        )?;
        for ((bond, metadata), direction) in self
            .graph
            .bonds
            .iter()
            .zip(&mut self.metadata.bonds)
            .zip(&mut self.directions)
        {
            if bond.order == 1 {
                metadata.unknown_stereo = *direction == Direction::Unknown;
                *direction = Direction::None;
            }
        }
        // V2000 reaction agents perceive bond directions before the application's
        // sanitization pass, without the file reader's legacy stereo assignment.
        if !sanitize_file {
            let rings = crate::chemistry::rings::perceive(&self.graph, Default::default())
                .map_err(|error| sanitize::Error {
                    stage: sanitize::Stage::Rings,
                    cause: sanitize::Cause::Rings(error),
                })?;
            let geometry = stereo::detect_bond_stereo(
                &self.graph,
                &self.metadata,
                &self.directions,
                Some(&conformer.positions),
                &rings.atoms,
            )
            .map_err(ReadError::Chemistry)?;
            self.metadata = geometry.metadata;
            self.directions = geometry.directions;
        }
        let cleaned = sanitize::sanitize(&self.graph, &self.metadata, &self.directions)?;
        let geometry = if sanitize_file {
            stereo::detect_bond_stereo(
                &cleaned.graph,
                &cleaned.metadata,
                &cleaned.directions,
                Some(&conformer.positions),
                &cleaned.rings,
            )
            .map_err(ReadError::Chemistry)?
        } else {
            stereo::BondGeometry {
                metadata: cleaned.metadata.clone(),
                directions: cleaned.directions.clone(),
            }
        };
        let mut properties = perception::Properties::unspecified(&cleaned.graph);
        if !sanitize_file
            && geometry
                .metadata
                .bonds
                .iter()
                .any(|b| !b.stereo_atoms.is_empty())
        {
            properties.needs_detection = Some(true);
        }
        let mut state = perception::State {
            graph: cleaned.graph,
            metadata: geometry.metadata,
            directions: geometry.directions,
            valences: cleaned.valences,
            conjugated: cleaned.conjugated,
            hybridizations: cleaned.hybridizations,
            rings: perception::RingCache {
                kind: perception::RingKind::Symmetric,
                atoms: cleaned.rings,
            },
            properties,
        };
        if sanitize_file {
            let queries: Vec<_> = self
                .atoms
                .iter()
                .map(|a| !matches!(a.query, FileQuery::None))
                .collect();
            state = perception::perceive_file_queries(
                &state,
                perception::Options {
                    clean: true,
                    force: true,
                    flag_possible: true,
                },
                &queries,
            )
            .map_err(ReadError::Chemistry)?;
        }
        if state.graph.atoms.iter().any(|a| a.radical_electrons > 2)
            || state.metadata.atoms.iter().any(|a| a.chiral_tag > 2)
            || state.metadata.bonds.iter().any(|b| b.stereo > 5)
            || !state.metadata.groups.is_empty()
        {
            return Err(ReadError::Unsupported(
                "radical count or stereochemistry class",
            ));
        }
        // Reaction parsing wraps ordinary reactant/product atoms as queries.
        // Existing explicit number queries replace that generated wrapper.
        if matches!(context, Context::Reaction { agent: false, .. })
            && state.graph.atoms.iter().zip(&self.atoms).any(|(a, props)| {
                matches!(props.query, FileQuery::None)
                    && (a.isotope != 0 || a.charge != 0 || a.radical_electrons != 0)
            })
        {
            return Err(ReadError::Unsupported("query reaction atom"));
        }
        Ok(Imported {
            molecule: Molecule {
                rdkit_version: RDKIT_VERSION,
                ids: (1..=state.graph.atoms.len() as u64).collect(),
                positions: conformer.positions,
                state,
            },
            annotations: FileAnnotations {
                chemdraw_directions,
                attachments: self
                    .bonds
                    .iter()
                    .filter_map(|b| b.attachment.clone())
                    .collect(),
                is_3d,
                attachment_points: self.atoms.iter().map(|a| a.attachment).collect(),
                dummy_labels: self.atoms.into_iter().map(|a| a.dummy_label).collect(),
            },
        })
    }
}

fn order(code: i32, v3000: bool) -> (u8, FileBond) {
    let kind = match code {
        1..=4 => code as u8,
        9 => 5,
        10 if v3000 => 0,
        _ => {
            return (
                0,
                FileBond {
                    unspecified: true,
                    query: code != 0,
                    ..Default::default()
                },
            );
        }
    };
    (kind, FileBond::default())
}

fn radical(code: i32) -> Result<u8> {
    match code {
        0 => Ok(0),
        1 | 3 => Ok(2),
        2 => Ok(1),
        _ => Err(ReadError::Unsupported("radical code")),
    }
}

/// Read one strict MOL block, retaining explicit H atoms and native atom order.
/// File annotations remain separate from the chemical graph and are retained
/// for drawing reconstruction and the remaining native identifier operations.
pub fn read(text: &str) -> Result<Imported> {
    if text.len() > 16 * 1024 * 1024 {
        return Err(ReadError::Limit);
    }
    if text.contains('\0') {
        return Err(ReadError::Invalid {
            line: 0,
            message: "NUL in file".into(),
        });
    }
    let mut reader = Reader {
        lines: text.lines(),
        line: 0,
    };
    read_molecule(&mut reader)?.finish()
}

fn read_molecule(reader: &mut Reader<'_>) -> Result<Parsed> {
    reader.next()?;
    let info = reader.next()?;
    let marked = info
        .get(20..22)
        .is_some_and(|s| s.eq_ignore_ascii_case("3D"));
    let chemdraw = info.get(2..10) == Some("ChemDraw");
    reader.next()?;
    let counts = reader.next()?;
    let n = reader.count(reader.field_integer(counts, 0, 3)?, 100_000)?;
    let e = reader.count(reader.field_integer(counts, 3, 6)?, 300_000)?;
    let version = if counts.len() > 35 {
        reader.field(counts, 34, 39)?
    } else {
        "V2000"
    };
    let mut parsed = Parsed::new(marked);
    parsed.chemdraw = chemdraw;
    match version {
        "V2000" => v2000::read(reader, &mut parsed, n, e)?,
        "V3000" if n == 0 && e == 0 => v3000::read(reader, &mut parsed, true)?,
        _ => return Err(reader.invalid("Invalid version or V3000 header counts")),
    }
    Ok(parsed)
}
