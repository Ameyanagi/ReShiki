//! MOL parsing adapted from RDKit MolFileParser.cpp (2026.03.6).
//! Copyright (C) 2002-2021 Greg Landrum and other RDKit contributors.
//! BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
use crate::chemistry::{
    ELEMENTS, RDKIT_VERSION,
    document::Molecule,
    electronic,
    graph::{Atom, Bond, Graph},
    kekulize::Direction,
    ranking::{AtomMetadata, BondMetadata, Metadata},
    sanitize,
    stereo::{self, Point3, perception},
};
use std::str::Lines;
mod v2000;
mod v3000;

#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    #[error("Invalid MOL input at line {line}: {message}")]
    Invalid { line: usize, message: String },
    #[error("MOL input contains unsupported chemistry: {0}")]
    Unsupported(&'static str),
    /// An explicit migration boundary, never a claim that the file is invalid.
    #[error("MOL input still requires the reference reader: {0}")]
    Pending(&'static str),
    #[error("MOL input exceeds the size or work limit")]
    Limit,
    #[error(transparent)]
    Sanitization(#[from] sanitize::Error),
    #[error("MOL chemistry: {0}")]
    Chemistry(String),
}
type Result<T> = std::result::Result<T, ReadError>;

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
struct FileAtom {
    valence: i32,
    hyd_override: bool,
    attachment: bool,
}
struct Parsed {
    graph: Graph,
    metadata: Metadata,
    directions: Vec<Direction>,
    positions: Vec<Point3>,
    atoms: Vec<FileAtom>,
    chirality: bool,
    marked_3d: bool,
}
impl Parsed {
    fn new(marked_3d: bool) -> Self {
        Self {
            graph: Graph {
                atoms: Vec::new(),
                bonds: Vec::new(),
            },
            metadata: Metadata::default(),
            directions: Vec::new(),
            positions: Vec::new(),
            atoms: Vec::new(),
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
    fn bond(&mut self, a: usize, b: usize, order: u8, dir: Direction) {
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
    }
    fn finish(mut self) -> Result<Molecule> {
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
        let is_3d = self.positions.iter().any(|p| p.z.abs() > 1e-3)
            || self.marked_3d && !self.chirality && !self.graph.atoms.is_empty();
        if is_3d {
            return Err(ReadError::Pending("3D atom and atropisomer perception"));
        }
        if self.chirality {
            let drawn = stereo::from_directions(
                &self.graph,
                &self.metadata,
                &self.directions,
                Some(&self.positions),
                true,
            )
            .map_err(ReadError::Chemistry)?;
            self.graph = drawn.graph;
            self.metadata = drawn.metadata;
        }
        self.check_atropisomer_boundary()?;
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
        let cleaned = sanitize::sanitize(&self.graph, &self.metadata, &self.directions)?;
        let geometry = stereo::detect_bond_stereo(
            &cleaned.graph,
            &cleaned.metadata,
            &cleaned.directions,
            Some(&self.positions),
            &cleaned.rings,
        )
        .map_err(ReadError::Chemistry)?;
        let properties = perception::Properties::unspecified(&cleaned.graph);
        let state = perception::perceive(
            &perception::State {
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
            },
            perception::Options {
                clean: true,
                force: true,
                flag_possible: true,
            },
        )
        .map_err(ReadError::Chemistry)?;
        if state.graph.atoms.iter().any(|a| a.radical_electrons > 2)
            || state.metadata.atoms.iter().any(|a| a.chiral_tag > 2)
            || state.metadata.bonds.iter().any(|b| b.stereo > 5)
            || !state.metadata.groups.is_empty()
        {
            return Err(ReadError::Unsupported(
                "radical count or stereochemistry class",
            ));
        }
        Ok(Molecule {
            rdkit_version: RDKIT_VERSION,
            ids: (1..=state.graph.atoms.len() as u64).collect(),
            positions: self.positions,
            state,
        })
    }

    fn check_atropisomer_boundary(&self) -> Result<()> {
        let mut wedged = vec![None; self.graph.atoms.len()];
        for (i, (bond, dir)) in self.graph.bonds.iter().zip(&self.directions).enumerate() {
            if matches!(dir, Direction::Wedge | Direction::Hash) && matches!(bond.order, 1 | 4) {
                *wedged
                    .get_mut(bond.a)
                    .ok_or_else(|| ReadError::Chemistry("Missing wedge atom".into()))? = Some(i);
            }
        }
        if wedged.iter().all(Option::is_none) {
            return Ok(());
        }
        let cache = self
            .graph
            .provisional_valences()
            .map_err(ReadError::Chemistry)?;
        let conjugated = electronic::conjugation_cached(&self.graph, Some(&cache))
            .map_err(ReadError::Chemistry)?;
        let tags = self
            .metadata
            .atoms
            .iter()
            .map(|a| a.chiral_tag)
            .collect::<Vec<_>>();
        let hybrids =
            electronic::hybridization_cached(&self.graph, &tags, &conjugated, Some(&cache))
                .map_err(ReadError::Chemistry)?;
        for (i, bond) in self.graph.bonds.iter().enumerate() {
            if bond.order == 1
                && [bond.a, bond.b]
                    .iter()
                    .any(|&a| wedged.get(a).is_some_and(|w| w.is_some_and(|w| w != i)))
                && [bond.a, bond.b]
                    .iter()
                    .all(|&a| hybrids.get(a) == Some(&electronic::Hybridization::Sp2))
            {
                return Err(ReadError::Pending("2D atropisomer perception"));
            }
        }
        Ok(())
    }
}

fn order(code: i32, v3000: bool) -> Result<u8> {
    match code {
        1..=4 => Ok(code as u8),
        9 => Ok(5),
        10 if v3000 => Ok(0),
        _ => Err(ReadError::Unsupported("query or unspecified bond order")),
    }
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
/// This staged reader is not enabled in the application until pending file
/// extensions and 3D perception have independent native-reference coverage.
pub fn read(text: &str) -> Result<Molecule> {
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
    reader.next()?;
    let info = reader.next()?;
    let marked = info
        .get(20..22)
        .is_some_and(|s| s.eq_ignore_ascii_case("3D"));
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
    match version {
        "V2000" => v2000::read(&mut reader, &mut parsed, n, e)?,
        "V3000" if n == 0 && e == 0 => v3000::read(&mut reader, &mut parsed)?,
        _ => return Err(reader.invalid("Invalid version or V3000 header counts")),
    }
    parsed.finish()
}
