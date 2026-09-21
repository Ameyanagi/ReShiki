//! SMILES graph construction adapted from RDKit smiles.ll/smiles.yy,
//! SmilesParseOps.cpp and Canon.cpp (2026.03.6).
//! Copyright (C) 2001-2022 Randal Henne, Greg Landrum, Rational Discovery LLC
//! and other RDKit contributors. BSD-3-Clause; see licenses/rdkit/LICENSE.
//!
//! `parse` reads the raw graph; `prepare` removes eligible hydrogens,
//! sanitizes and perceives stereo. `read` also handles CX extensions and names.
mod atom;
mod chirality;
mod prepare;
pub mod stereo;
pub mod symbols;
pub mod traversal;
pub mod write;
pub use prepare::{Prepared, prepare};
mod read;
pub(crate) use read::reaction_part;
pub use read::{Imported, read};

use super::{
    graph::{Atom, Bond, Graph},
    kekulize::Direction,
    ranking::{AtomMetadata, BondMetadata, Metadata},
};
use serde::Serialize;
use std::collections::{BTreeMap, HashSet};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid SMILES at byte {0}")]
    Syntax(usize),
    #[error("SMILES contains unsupported chemistry: {0}")]
    Unsupported(&'static str),
    #[error("SMILES exceeds parser resource limits")]
    Limit,
    #[error(transparent)]
    Hydrogens(#[from] super::hydrogens::Error),
    #[error(transparent)]
    Sanitization(#[from] super::sanitize::Error),
    #[error("SMILES stereochemistry: {0}")]
    Stereo(String),
    #[error(transparent)]
    Cx(#[from] super::cx::Error),
    #[error(transparent)]
    Spatial(#[from] super::stereo::SpatialError),
    #[error(transparent)]
    Atropisomer(#[from] super::stereo::AtropError),
}
type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Serialize)]
pub struct Parsed {
    pub graph: Graph,
    pub metadata: Metadata,
    pub directions: Vec<Direction>,
    /// Textual bond order, including ring closures, for CX bond references.
    pub bond_indices: Vec<usize>,
    #[serde(skip)]
    pub(crate) ring_bonds: Vec<bool>,
    pub dummy_labels: Vec<Option<String>>,
    // Import can remove a query bond with its explicit hydrogen. Preserve the
    // marker until that stage; surviving queries cannot become editable bonds.
    #[serde(skip)]
    pub(crate) query_bonds: Vec<bool>,
}

#[derive(Clone, Copy)]
struct BondSpec {
    order: Option<u8>,
    direction: Direction,
    reverse: bool,
    query: bool,
}
impl Default for BondSpec {
    fn default() -> Self {
        Self {
            order: None,
            direction: Direction::None,
            reverse: false,
            query: false,
        }
    }
}
struct RingEnd {
    atom: usize,
    spec: BondSpec,
    slot: usize,
    index: usize,
}
struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
    parsed: Parsed,
    adjacent: Vec<Vec<(usize, usize)>>,
    closures: Vec<Vec<Option<usize>>>,
    starts: Vec<bool>,
    rings: BTreeMap<u32, Vec<RingEnd>>,
    pairs: HashSet<(usize, usize)>,
    bond_count: usize,
}

/// Read a bare SMILES graph without modifying hydrogen or chemical state.
/// Query bonds are rejected; stereo classes are retained for later cleanup.
/// Do not use this result as a sanitized molecule or skip later import stages.
pub fn parse(text: &str) -> Result<Parsed> {
    let parsed = parse_inner(text)?;
    if parsed.query_bonds.iter().any(|&query| query) {
        return Err(Error::Unsupported("query bond"));
    }
    Ok(parsed)
}

fn parse_inner(text: &str) -> Result<Parsed> {
    if text.len() > 1024 * 1024 {
        return Err(Error::Limit);
    }
    let bytes = text.as_bytes();
    let start = bytes
        .iter()
        .position(|&b| (b as std::os::raw::c_char) > 32)
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|&b| (b as std::os::raw::c_char) > 32)
        .map_or(start, |i| i + 1);
    let mut reader = Reader {
        bytes: bytes.get(start..end).ok_or(Error::Limit)?,
        pos: 0,
        parsed: Parsed {
            graph: Graph {
                atoms: Vec::new(),
                bonds: Vec::new(),
            },
            metadata: Metadata::default(),
            directions: Vec::new(),
            bond_indices: Vec::new(),
            ring_bonds: Vec::new(),
            dummy_labels: Vec::new(),
            query_bonds: Vec::new(),
        },
        adjacent: Vec::new(),
        closures: Vec::new(),
        starts: Vec::new(),
        rings: BTreeMap::new(),
        pairs: HashSet::new(),
        bond_count: 0,
    };
    if reader.bytes.is_empty() {
        return if text.is_empty() {
            Ok(reader.parsed)
        } else {
            Err(Error::Syntax(start))
        };
    }
    reader.read()?;
    reader.close_rings()?;
    reader.chirality()?;
    Ok(reader.parsed)
}

impl Reader<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }
    fn starts_with(&self, bytes: &[u8]) -> bool {
        self.bytes
            .get(self.pos..)
            .is_some_and(|s| s.starts_with(bytes))
    }
    fn invalid(&self) -> Error {
        Error::Syntax(self.pos)
    }
    fn eat(&mut self, byte: u8) -> bool {
        if self.peek() == Some(byte) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn require(&mut self, byte: u8) -> Result<()> {
        if self.eat(byte) {
            Ok(())
        } else {
            Err(self.invalid())
        }
    }
    fn number(&mut self) -> Result<u32> {
        let first = self
            .peek()
            .filter(u8::is_ascii_digit)
            .ok_or_else(|| self.invalid())?;
        self.pos += 1;
        let mut n = u32::from(first - b'0');
        if n == 0 {
            return Ok(0);
        }
        while let Some(digit) = self.peek().filter(u8::is_ascii_digit) {
            if n >= i32::MAX as u32 / 10 {
                return Err(self.invalid());
            }
            n = n * 10 + u32::from(digit - b'0');
            self.pos += 1;
        }
        Ok(n)
    }
    fn spec(&mut self) -> Result<BondSpec> {
        let mut spec = BondSpec::default();
        if self.starts_with(b"->") || self.starts_with(b"<-") {
            spec.order = Some(5);
            spec.reverse = self.peek() == Some(b'<');
            self.pos += 2;
        } else {
            match self.peek() {
                Some(b'-') => spec.order = Some(1),
                Some(b'=') => spec.order = Some(2),
                Some(b'#') => spec.order = Some(3),
                Some(b':') => spec.order = Some(4),
                Some(b'$') => spec.order = Some(6),
                Some(b'/') => spec.direction = Direction::Up,
                Some(b'\\') => {
                    spec.direction = Direction::Down;
                    self.eat(b'\\');
                    if self.peek() != Some(b'\\') {
                        return Ok(spec);
                    }
                }
                Some(b'~') => {
                    spec.order = Some(0);
                    spec.query = true;
                }
                _ => return Ok(spec),
            }
            self.pos += 1;
        }
        Ok(spec)
    }
    fn ring_number(&mut self) -> Result<u32> {
        if self.eat(b'%') {
            if self.eat(b'(') {
                let mut value = 0;
                let mut count = 0;
                while let Some(d) = self.peek().filter(u8::is_ascii_digit) {
                    count += 1;
                    if count > 5 {
                        return Err(self.invalid());
                    }
                    value = value * 10 + u32::from(d - b'0');
                    self.pos += 1;
                }
                if count == 0 {
                    return Err(self.invalid());
                }
                self.require(b')')?;
                Ok(value)
            } else {
                let tens = self
                    .peek()
                    .filter(|b| (b'1'..=b'9').contains(b))
                    .ok_or_else(|| self.invalid())?;
                self.pos += 1;
                let units = self
                    .peek()
                    .filter(u8::is_ascii_digit)
                    .ok_or_else(|| self.invalid())?;
                self.pos += 1;
                Ok(u32::from(tens - b'0') * 10 + u32::from(units - b'0'))
            }
        } else {
            let n = self
                .peek()
                .filter(u8::is_ascii_digit)
                .ok_or_else(|| self.invalid())?;
            self.pos += 1;
            Ok(u32::from(n - b'0'))
        }
    }
    fn add_atom(&mut self, start: bool) -> Result<usize> {
        if self.parsed.graph.atoms.len() >= 100_000 {
            return Err(Error::Limit);
        }
        let (atom, meta, label) = self.atom()?;
        let id = self.parsed.graph.atoms.len();
        self.parsed.graph.atoms.push(atom);
        self.parsed.metadata.atoms.push(meta);
        self.parsed.dummy_labels.push(label);
        self.adjacent.push(Vec::new());
        self.closures.push(Vec::new());
        self.starts.push(start);
        Ok(id)
    }
    fn add_bond(
        &mut self,
        mut a: usize,
        mut b: usize,
        spec: BondSpec,
        index: usize,
    ) -> Result<usize> {
        if self.parsed.graph.bonds.len() >= 300_000 {
            return Err(Error::Limit);
        }
        if a == b || !self.pairs.insert((a.min(b), a.max(b))) {
            return Err(self.invalid());
        }
        let aromatic = self
            .parsed
            .graph
            .atoms
            .get(a)
            .zip(self.parsed.graph.atoms.get(b))
            .ok_or(Error::Limit)?;
        let order = spec
            .order
            .unwrap_or(if aromatic.0.aromatic && aromatic.1.aromatic {
                4
            } else {
                1
            });
        if spec.reverse {
            std::mem::swap(&mut a, &mut b);
        }
        let id = self.parsed.graph.bonds.len();
        self.parsed.graph.bonds.push(Bond {
            a,
            b,
            order,
            aromatic: order == 4,
        });
        self.parsed.metadata.bonds.push(BondMetadata::default());
        self.parsed.directions.push(spec.direction);
        self.parsed.bond_indices.push(index);
        self.parsed.ring_bonds.push(false);
        self.parsed.query_bonds.push(spec.query);
        self.adjacent.get_mut(a).ok_or(Error::Limit)?.push((b, id));
        self.adjacent.get_mut(b).ok_or(Error::Limit)?.push((a, id));
        Ok(id)
    }
    fn read(&mut self) -> Result<()> {
        let mut active = self.add_atom(true)?;
        let mut branches = Vec::new();
        while !matches!(self.peek(), None | Some(b'\n')) {
            if self.eat(b')') {
                active = branches.pop().ok_or_else(|| self.invalid())?;
            } else if self.eat(b'.') {
                active = self.add_atom(true)?;
            } else {
                let branch = self.eat(b'(');
                if branch {
                    if branches.len() >= 4096 {
                        return Err(Error::Limit);
                    }
                    branches.push(active);
                }
                let spec = self.spec()?;
                if !branch && self.peek().is_some_and(|b| b.is_ascii_digit() || b == b'%') {
                    self.ring(active, spec)?;
                } else {
                    let next = self.add_atom(false)?;
                    self.add_bond(active, next, spec, self.bond_count)?;
                    self.bond_count += 1;
                    active = next;
                }
            }
        }
        if branches.is_empty() {
            Ok(())
        } else {
            Err(self.invalid())
        }
    }
    fn ring(&mut self, atom: usize, spec: BondSpec) -> Result<()> {
        let label = self.ring_number()?;
        if self.bond_count >= 300_000 {
            return Err(Error::Limit);
        }
        let degree = self.adjacent.get(atom).ok_or(Error::Limit)?.len();
        let meta = self
            .parsed
            .metadata
            .atoms
            .get_mut(atom)
            .ok_or(Error::Limit)?;
        if atom + 1 != self.parsed.graph.atoms.len()
            && (degree == 1 || degree == 2 && atom != 0 || degree == 3 && atom == 0)
            && matches!(meta.chiral_tag, 1 | 2)
        {
            meta.chiral_tag = 3 - meta.chiral_tag;
        }
        let slots = self.closures.get_mut(atom).ok_or(Error::Limit)?;
        let slot = slots.len();
        slots.push(None);
        let ends = self.rings.entry(label).or_default();
        ends.push(RingEnd {
            atom,
            spec,
            slot,
            index: self.bond_count,
        });
        if ends.len().is_multiple_of(2) {
            self.bond_count += 1;
        }
        Ok(())
    }
    fn close_rings(&mut self) -> Result<()> {
        for ends in std::mem::take(&mut self.rings).into_values() {
            if ends.len() % 2 != 0 {
                return Err(self.invalid());
            }
            for pair in ends.chunks_exact(2) {
                let first = pair.first().ok_or(Error::Limit)?;
                let second = pair.get(1).ok_or(Error::Limit)?;
                let (chosen, other) = if first.spec.order.is_some() {
                    (first, second)
                } else {
                    (second, first)
                };
                let mut spec = chosen.spec;
                // Native ring grammar creates a plain partial bond from the
                // token's type. An unspecified query token loses its predicate
                // here, then receives the endpoints' default bond order.
                if spec.query {
                    spec.query = false;
                    spec.order = None;
                }
                // A direction copied from the opposite end changes orientation.
                if spec.direction == Direction::None {
                    spec.direction = if spec.reverse {
                        other.spec.direction
                    } else {
                        match other.spec.direction {
                            Direction::Up => Direction::Down,
                            Direction::Down => Direction::Up,
                            d => d,
                        }
                    };
                }
                let id = self.add_bond(chosen.atom, other.atom, spec, second.index)?;
                *self.parsed.ring_bonds.get_mut(id).ok_or(Error::Limit)? = true;
                for end in pair {
                    *self
                        .closures
                        .get_mut(end.atom)
                        .and_then(|v| v.get_mut(end.slot))
                        .ok_or(Error::Limit)? = Some(id);
                }
            }
        }
        Ok(())
    }
    fn chirality(&mut self) -> Result<()> {
        for (id, meta) in self.parsed.metadata.atoms.iter_mut().enumerate() {
            if !matches!(meta.chiral_tag, 1 | 2 | 6..=8) {
                continue;
            }
            let adjacent = self.adjacent.get(id).ok_or(Error::Limit)?;
            // Invalid extreme degrees must not create quadratic parser work.
            if adjacent.len() > 256 {
                return Err(Error::Limit);
            }
            let closures = self
                .closures
                .get(id)
                .ok_or(Error::Limit)?
                .iter()
                .copied()
                .collect::<Option<Vec<_>>>()
                .ok_or(Error::Limit)?;
            let mut neighbors = adjacent
                .iter()
                .copied()
                .filter(|(_, bond)| !closures.contains(bond))
                .collect::<Vec<_>>();
            neighbors.sort_unstable();
            let split = neighbors.partition_point(|(other, _)| *other < id);
            let ordered = neighbors
                .iter()
                .take(split)
                .map(|(_, b)| *b)
                .chain(closures.iter().copied())
                .chain(neighbors.iter().skip(split).map(|(_, b)| *b))
                .collect::<Vec<_>>();
            if matches!(meta.chiral_tag, 6..=8) {
                meta.chiral_permutation = Some(chirality::permutation(
                    meta.chiral_tag,
                    meta.chiral_permutation.unwrap_or(0),
                    &ordered,
                    adjacent,
                    *self.starts.get(id).ok_or(Error::Limit)?,
                )?);
                continue;
            }
            let mut odd = false;
            for (i, a) in ordered.iter().enumerate() {
                for b in ordered.iter().skip(i + 1) {
                    odd ^= a > b;
                }
            }
            let atom = self.parsed.graph.atoms.get(id).ok_or(Error::Limit)?;
            let unsaturated = adjacent.iter().any(|(_, b)| {
                self.parsed
                    .graph
                    .bonds
                    .get(*b)
                    .is_some_and(|b| matches!(b.order, 2..=4 | 6 | 7))
            });
            if adjacent.len() == 3
                && ((*self.starts.get(id).ok_or(Error::Limit)? && atom.explicit_hydrogens == 1)
                    || (atom.explicit_hydrogens != 1 && closures.len() == 1 && !unsaturated))
            {
                odd = !odd;
            }
            if odd {
                meta.chiral_tag = 3 - meta.chiral_tag;
            }
        }
        Ok(())
    }
}
