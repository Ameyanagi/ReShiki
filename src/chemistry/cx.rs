//! Shared CX extension syntax and annotation events, adapted from CXSmilesOps.cpp.
//! Copyright (C) 2016-2021 Greg Landrum and contributors.
//! BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
use std::collections::{HashMap, HashSet};
mod number;
pub use number::parse as coordinate;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid CX extension at byte {0}")]
    Syntax(usize),
    #[error("CX extension exceeds parser resource limits")]
    Limit,
}
type Result<T> = std::result::Result<T, Error>;

pub(crate) fn trim(text: &str) -> &str {
    // The native C-locale whitespace class includes vertical tab; Rust's
    // is_ascii_whitespace follows a smaller class that excludes it.
    text.trim_matches(|c| matches!(c, ' ' | '\t'..='\r'))
}

#[derive(Default)]
pub struct Topology {
    pub atoms: usize,
    pub bonds: Vec<ParseBond>,
}
pub struct ParseBond {
    pub a: usize,
    pub b: usize,
    pub index: usize,
}

#[derive(Clone, Copy, Debug)]
pub enum BondKind {
    Hydrogen,
    Coordinate,
    Zero,
}
/// Events remain in file order because repeated annotations can overwrite state.
/// Text fields contain the decoded native bytes, including numeric escapes.
#[derive(Debug)]
pub enum Event {
    Coordinates(Vec<[Vec<u8>; 3]>),
    AtomProperty {
        atom: usize,
        name: Vec<u8>,
        value: Vec<u8>,
    },
    BondKind {
        bond: usize,
        begin: Option<usize>,
        kind: BondKind,
    },
    Radical {
        atom: usize,
        electrons: u8,
    },
    StereoGroup {
        kind: u8,
        id: u32,
        atoms: Vec<usize>,
    },
    QueryAtom(usize),
    SubstanceGroup {
        atoms: Vec<usize>,
        bonds: Vec<usize>,
    },
    Wedge {
        bond: usize,
        begin: usize,
        kind: u8,
    },
    BondStereo {
        bond: usize,
        stereo: u8,
    },
    ProcessLabels,
}
#[derive(Debug)]
pub struct Read {
    pub events: Vec<Event>,
    /// Byte offset immediately after the closing delimiter; a name can follow.
    pub end: usize,
}

pub fn read(text: &str, graph: &Topology) -> Result<Read> {
    if text.len() > 1024 * 1024 || graph.atoms > 100_000 || graph.bonds.len() > 300_000 {
        return Err(Error::Limit);
    }
    let mut degrees = vec![0usize; graph.atoms];
    let mut neighbors = vec![Vec::new(); graph.atoms];
    let mut indices = HashMap::new();
    let mut earlier = vec![0usize; graph.bonds.len() + 1];
    for (i, bond) in graph.bonds.iter().enumerate() {
        for atom in [bond.a, bond.b] {
            *degrees.get_mut(atom).ok_or(Error::Limit)? += 1;
            neighbors
                .get_mut(atom)
                .ok_or(Error::Limit)?
                .push((i, if atom == bond.a { bond.b } else { bond.a }));
        }
        indices.entry(bond.index).or_insert(i);
        if let Some(n) = earlier.get_mut(bond.index.checked_add(1).ok_or(Error::Limit)?) {
            *n += 1;
        }
    }
    // Match get_bond_with_smiles_idx, including fallback slots when recursive
    // patterns have consumed parse-order indices before the outer graph.
    let mut lower = 0;
    let mapping = earlier
        .iter()
        .take(graph.bonds.len())
        .enumerate()
        .map(|(i, &n)| {
            lower += n;
            indices
                .get(&i)
                .copied()
                .or_else(|| i.checked_sub(lower).filter(|&b| b < graph.bonds.len()))
        })
        .collect();
    let mut p = Extension {
        input: text.as_bytes(),
        pos: 0,
        graph,
        degrees,
        neighbors,
        mapping,
        wedged: HashSet::new(),
        groups: HashSet::new(),
        next_group: 0,
        stereo_atoms: HashMap::new(),
        events: Vec::new(),
        work: 2_000_000,
    };
    p.require(b'|')?;
    while p.peek().is_some_and(|b| b != b'|') {
        match p.peek() {
            Some(b'(') => p.coords()?,
            Some(b'$') => {
                let name = if p.starts(b"$_AV:") {
                    p.pos += 5;
                    b"molFileValue".as_slice()
                } else {
                    p.pos += 1;
                    b"atomLabel".as_slice()
                };
                let mut atom = 0;
                while p.peek() != Some(b'$') {
                    let value = p.text(b";$")?;
                    if atom < p.graph.atoms && !value.is_empty() {
                        p.events.push(Event::AtomProperty {
                            atom,
                            name: name.to_vec(),
                            value,
                        });
                    }
                    atom += 1;
                    if p.peek() != Some(b'$') {
                        p.advance()?;
                    }
                }
                p.advance()?;
            }
            Some(b'a') if p.starts(b"atomProp:") => p.atom_props()?,
            Some(b'C' | b'H') => p.coordinate_bonds()?,
            Some(b'Z' | b'u') => {
                let zero = p.peek() == Some(b'Z');
                p.advance()?;
                p.require(b':')?;
                for id in p.sequence()? {
                    if zero {
                        if let Some(bond) = p.bond(id)? {
                            p.events.push(Event::BondKind {
                                bond,
                                begin: None,
                                kind: BondKind::Zero,
                            });
                        }
                    } else if p.atom(id) {
                        p.events.push(Event::QueryAtom(id as usize));
                    }
                }
            }
            Some(b'^') => p.radicals()?,
            Some(b'a' | b'o') => p.stereo_group()?,
            Some(b'&') if !p.starts(b"&#") => p.stereo_group()?,
            Some(b'r') if p.starts(b"rb") => p.substitution(true)?,
            Some(b'L') if p.starts(b"LN") => p.link_nodes()?,
            Some(b'S') if p.starts(b"SgD") => p.data_group()?,
            Some(b'S') if p.starts(b"SgH") => p.group_hierarchy()?,
            Some(b'S') if p.starts(b"Sg") => p.polymer_group()?,
            Some(b's') => p.substitution(false)?,
            Some(b'm') => p.variable_attachment()?,
            Some(b'w') => p.wedges()?,
            Some(b'c' | b't') => {
                let stereo = if p.starts(b"ctu") {
                    1
                } else if p.peek() == Some(b'c') {
                    4
                } else {
                    5
                };
                while p.peek() != Some(b':') {
                    p.advance()?;
                }
                p.advance()?;
                for bond in p.sequence()? {
                    if let Some(bond) = p.bond(bond)? {
                        p.events.push(Event::BondStereo { bond, stereo });
                    }
                }
            }
            _ => p.advance()?,
        }
    }
    p.require(b'|')?;
    p.events.push(Event::ProcessLabels);
    Ok(Read {
        events: p.events,
        end: p.pos,
    })
}

struct Extension<'a> {
    input: &'a [u8],
    pos: usize,
    graph: &'a Topology,
    degrees: Vec<usize>,
    neighbors: Vec<Vec<(usize, usize)>>,
    mapping: Vec<Option<usize>>,
    wedged: HashSet<usize>,
    groups: HashSet<usize>,
    next_group: usize,
    stereo_atoms: HashMap<u32, HashSet<u32>>,
    events: Vec<Event>,
    work: usize,
}
impl Extension<'_> {
    fn peek(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }
    fn starts(&self, text: &[u8]) -> bool {
        self.input
            .get(self.pos..)
            .is_some_and(|s| s.starts_with(text))
    }
    fn invalid(&self) -> Error {
        Error::Syntax(self.pos)
    }
    fn advance(&mut self) -> Result<()> {
        if self.peek().is_none() {
            return Err(self.invalid());
        }
        self.pos += 1;
        Ok(())
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
    fn unsigned(&mut self) -> Result<Option<u32>> {
        let mut number = None::<u32>;
        while let Some(b) = self.peek().filter(u8::is_ascii_digit) {
            number = Some(
                number
                    .unwrap_or(0)
                    .checked_mul(10)
                    .and_then(|n| n.checked_add(u32::from(b - b'0')))
                    .ok_or_else(|| self.invalid())?,
            );
            self.pos += 1;
        }
        Ok(number)
    }
    fn number(&mut self) -> Result<u32> {
        self.unsigned()?.ok_or_else(|| self.invalid())
    }
    fn list(&mut self, separator: u8) -> Result<Vec<u32>> {
        let mut values = Vec::new();
        loop {
            if let Some(n) = self.unsigned()? {
                values.push(n);
            }
            if !self.eat(separator) {
                break;
            }
        }
        Ok(values)
    }
    fn sequence(&mut self) -> Result<Vec<u32>> {
        let mut values = Vec::new();
        while self.peek().is_some_and(|b| b.is_ascii_digit()) {
            values.push(self.number()?);
            self.eat(b',');
        }
        Ok(values)
    }
    fn text(&mut self, delimiters: &[u8]) -> Result<Vec<u8>> {
        let mut result = Vec::new();
        while let Some(byte) = self.peek() {
            if delimiters.contains(&byte) {
                return Ok(result);
            }
            if self.starts(b"&#") {
                self.pos += 2;
                let number = self.unsigned()?;
                self.require(b';')?;
                if let Some(number) = number {
                    if number > i32::MAX as u32 {
                        return Err(self.invalid());
                    }
                    result.push(number as u8);
                }
            } else {
                result.push(byte);
                self.pos += 1;
            }
        }
        Err(self.invalid())
    }
    fn atom(&self, id: u32) -> bool {
        (id as usize) < self.graph.atoms
    }
    fn bond(&self, id: u32) -> Result<Option<usize>> {
        match self.mapping.get(id as usize) {
            None => Ok(None),
            Some(Some(id)) => Ok(Some(*id)),
            Some(None) => Err(self.invalid()),
        }
    }
    fn endpoint(&self, atom: u32, bond: u32) -> Result<Option<usize>> {
        if !self.atom(atom) {
            return Ok(None);
        }
        let Some(id) = self.bond(bond)? else {
            return Ok(None);
        };
        let b = self.graph.bonds.get(id).ok_or(Error::Limit)?;
        if atom as usize != b.a && atom as usize != b.b {
            return Err(self.invalid());
        }
        Ok(Some(id))
    }
    fn coords(&mut self) -> Result<()> {
        self.require(b'(')?;
        let mut atom = 0usize;
        let mut points = Vec::new();
        while self.peek() != Some(b')') {
            let text = self.text(b";)")?;
            if atom < self.graph.atoms {
                let mut point = [Vec::new(), Vec::new(), Vec::new()];
                for (slot, field) in point.iter_mut().zip(text.split(|&b| b == b',')) {
                    let text = std::str::from_utf8(field).map_err(|_| self.invalid())?;
                    if !text.is_empty() && !number::valid(text) {
                        return Err(self.invalid());
                    }
                    *slot = field.to_vec();
                }
                points.push(point);
            }
            atom += 1;
            if self.peek() != Some(b')') {
                self.advance()?;
            }
        }
        self.require(b')')?;
        self.events.push(Event::Coordinates(points));
        Ok(())
    }
    fn atom_props(&mut self) -> Result<()> {
        self.pos += 9;
        while !matches!(self.peek(), None | Some(b'|' | b',')) {
            if let Some(atom) = self.unsigned()? {
                self.require(b'.')?;
                let name = self.text(b".")?;
                if !name.is_empty() {
                    self.require(b'.')?;
                    let value = self.text(b":|,")?;
                    if self.atom(atom) && !value.is_empty() {
                        self.events.push(Event::AtomProperty {
                            atom: atom as usize,
                            name,
                            value,
                        });
                    }
                }
            }
            if !matches!(self.peek(), Some(b'|' | b',')) {
                self.advance()?;
            }
        }
        if self.peek().is_none() {
            return Err(self.invalid());
        }
        self.eat(b',');
        Ok(())
    }
    fn coordinate_bonds(&mut self) -> Result<()> {
        let kind = if self.peek() == Some(b'C') {
            BondKind::Coordinate
        } else {
            BondKind::Hydrogen
        };
        self.advance()?;
        self.require(b':')?;
        while self.peek().is_some_and(|b| b.is_ascii_digit()) {
            let atom = self.number()?;
            self.require(b'.')?;
            let bond = self.number()?;
            if let Some(bond) = self.endpoint(atom, bond)? {
                self.events.push(Event::BondKind {
                    bond,
                    begin: Some(atom as usize),
                    kind,
                });
            }
            self.eat(b',');
        }
        Ok(())
    }
    fn radicals(&mut self) -> Result<()> {
        while self.eat(b'^') {
            if !matches!(self.peek(), Some(b'1'..=b'7')) {
                return Err(self.invalid());
            }
            let electrons = match self.peek() {
                Some(b'1') => 1,
                Some(b'2'..=b'4') => 2,
                _ => 3,
            };
            self.advance()?;
            self.require(b':')?;
            let atom = self.number()?;
            if self.atom(atom) {
                self.events.push(Event::Radical {
                    atom: atom as usize,
                    electrons,
                });
            }
            while self.eat(b',') {
                if self.peek().is_some_and(|b| !b.is_ascii_digit()) {
                    break;
                }
                let atom = self.number()?;
                if self.atom(atom) {
                    self.events.push(Event::Radical {
                        atom: atom as usize,
                        electrons,
                    });
                }
            }
            if self.peek().is_none() {
                return Err(self.invalid());
            }
        }
        Ok(())
    }
    fn stereo_group(&mut self) -> Result<()> {
        let kind = match self.peek() {
            Some(b'a') => 0u32,
            Some(b'o') => 1,
            _ => 2,
        };
        let absolute = self.eat(b'a');
        let mut group = 0;
        if !absolute {
            self.advance()?;
            group = self.unsigned()?.unwrap_or(0);
        }
        self.require(b':')?;
        let mut atoms = HashSet::new();
        let mut ordered = Vec::new();
        while self.peek().is_some_and(|b| b.is_ascii_digit()) {
            let atom = self.number()?;
            if self.atom(atom) && !atoms.insert(atom) {
                return Err(self.invalid());
            }
            if self.atom(atom) {
                ordered.push(atom as usize);
            }
            self.eat(b',');
        }
        let hash = group.wrapping_mul(10).wrapping_add(kind);
        let stored = self.stereo_atoms.entry(hash).or_default();
        if atoms.iter().any(|a| stored.contains(a)) {
            return Err(self.invalid());
        }
        stored.extend(atoms);
        if !ordered.is_empty() {
            self.events.push(Event::StereoGroup {
                kind: kind as u8,
                id: group,
                atoms: ordered,
            });
        }
        Ok(())
    }
    fn substitution(&mut self, ring: bool) -> Result<()> {
        self.pos += if ring { 2 } else { 1 };
        self.require(b':')?;
        while self.peek().is_some_and(|b| b.is_ascii_digit()) {
            let atom = self.number()?;
            self.require(b':')?;
            if !self.eat(b'*') {
                let count = self.number()?;
                if ring && !matches!(count, 0 | 2 | 3 | 4) {
                    return Err(self.invalid());
                }
            }
            if self.atom(atom) {
                self.events.push(Event::QueryAtom(atom as usize));
            }
            self.eat(b',');
        }
        Ok(())
    }
    fn link_nodes(&mut self) -> Result<()> {
        self.pos += 2;
        self.require(b':')?;
        while self.peek().is_some_and(|b| b.is_ascii_digit()) {
            let atom = self.number()?;
            self.require(b':')?;
            self.number()?;
            self.require(b'.')?;
            self.number()?;
            if self.eat(b'.') {
                self.number()?;
                // The native implementation skips this separator unchecked.
                self.advance()?;
                self.number()?;
            } else if self.atom(atom) && self.degrees.get(atom as usize) != Some(&2) {
                return Err(self.invalid());
            }
            self.eat(b',');
        }
        Ok(())
    }
    fn data_group(&mut self) -> Result<()> {
        self.pos += 3;
        self.require(b':')?;
        let atoms = self
            .list(b',')?
            .into_iter()
            .filter(|&id| self.atom(id))
            .map(|id| id as usize)
            .collect::<Vec<_>>();
        let keep = !atoms.is_empty();
        self.advance()?;
        for _ in 0..5 {
            if self.peek().is_none() {
                return Err(self.invalid());
            }
            if self.peek() != Some(b'|') {
                self.text(b":")?;
                self.advance()?;
            }
        }
        if self.peek() == Some(b'(') {
            self.text(b")")?;
            self.advance()?;
        }
        if keep {
            self.groups.insert(self.next_group);
            self.events.push(Event::ProcessLabels);
            self.events.push(Event::SubstanceGroup {
                atoms,
                bonds: Vec::new(),
            });
        }
        self.next_group += 1;
        Ok(())
    }
    fn group_hierarchy(&mut self) -> Result<()> {
        self.pos += 3;
        self.require(b':')?;
        loop {
            let parent = self.number()?;
            self.require(b':')?;
            let children = self.list(b'.')?;
            if self.groups.contains(&(parent as usize))
                && children.iter().any(|&n| n as usize >= self.groups.len())
            {
                return Err(self.invalid());
            }
            if !self.eat(b',') {
                break;
            }
        }
        Ok(())
    }
    fn polymer_group(&mut self) -> Result<()> {
        self.pos += 2;
        self.require(b':')?;
        let kind = self.text(b":")?;
        if ![
            b"n".as_slice(),
            b"mon",
            b"mer",
            b"co",
            b"xl",
            b"mod",
            b"mix",
            b"f",
            b"any",
            b"gen",
            b"c",
            b"grf",
            b"alt",
            b"ran",
            b"blk",
        ]
        .contains(&kind.as_slice())
        {
            return Err(self.invalid());
        }
        self.advance()?;
        let atoms = self
            .list(b',')?
            .into_iter()
            .filter(|&id| self.atom(id))
            .map(|id| id as usize)
            .collect::<Vec<_>>();
        let mut keep = !atoms.is_empty();
        let mut crossings = Vec::new();
        if self.eat(b':') {
            self.text(b":|")?;
            if self.eat(b':') {
                self.text(b":|,")?;
                if self.eat(b':') {
                    crossings = self.list(b',')?;
                    if self.eat(b':') {
                        crossings.extend(self.list(b',')?);
                    }
                    keep &= crossings.iter().all(|&id| self.atom(id));
                }
            }
        }
        if keep {
            if crossings
                .iter()
                .any(|&id| id as usize >= self.graph.bonds.len())
            {
                return Err(self.invalid());
            }
            self.groups.insert(self.next_group);
            self.events.push(Event::ProcessLabels);
            let bonds = if crossings.is_empty() {
                self.polymer_crossings(&atoms)?
            } else {
                crossings.into_iter().map(|id| id as usize).collect()
            };
            self.events.push(Event::SubstanceGroup { atoms, bonds });
        }
        self.next_group += 1;
        Ok(())
    }
    fn variable_attachment(&mut self) -> Result<()> {
        self.advance()?;
        self.require(b':')?;
        while self.peek().is_some_and(|b| b.is_ascii_digit()) {
            let atom = self.number()?;
            if self.atom(atom) && self.degrees.get(atom as usize) != Some(&1) {
                return Err(self.invalid());
            }
            self.require(b':')?;
            while self.peek().is_some_and(|b| b.is_ascii_digit()) {
                self.number()?;
                self.eat(b'.');
            }
            self.eat(b',');
        }
        Ok(())
    }
    fn wedges(&mut self) -> Result<()> {
        self.advance()?;
        let kind = match self.peek() {
            Some(b'U') => 1,
            Some(b'D') => 3,
            _ => 2,
        };
        if matches!(self.peek(), Some(b'U' | b'D')) {
            self.advance()?;
        }
        self.require(b':')?;
        while self.peek().is_some_and(|b| b.is_ascii_digit()) {
            let atom = self.number()?;
            self.require(b'.')?;
            let bond = self.number()?;
            if let Some(bond) = self.endpoint(atom, bond)? {
                if !self.wedged.insert(bond) {
                    return Err(self.invalid());
                }
                self.events.push(Event::Wedge {
                    bond,
                    begin: atom as usize,
                    kind,
                });
            }
            self.eat(b',');
        }
        Ok(())
    }

    fn polymer_crossings(&mut self, atoms: &[usize]) -> Result<Vec<usize>> {
        let first = *atoms.first().ok_or(Error::Limit)?;
        let last = *atoms.last().ok_or(Error::Limit)?;
        let scans = atoms
            .len()
            .checked_add(*self.degrees.get(first).ok_or(Error::Limit)?)
            .and_then(|n| n.checked_add(*self.degrees.get(last)?))
            .ok_or(Error::Limit)?;
        self.work = self.work.checked_sub(scans).ok_or(Error::Limit)?;
        let members = atoms.iter().copied().collect::<HashSet<_>>();
        let mut bonds = Vec::new();
        for endpoint in if atoms.len() == 1 {
            vec![first]
        } else {
            vec![first, last]
        } {
            for &(id, other) in self.neighbors.get(endpoint).ok_or(Error::Limit)? {
                if !members.contains(&other) {
                    bonds.push(id);
                    if atoms.len() == 1 && bonds.len() == 2 {
                        break;
                    }
                }
            }
        }
        Ok(bonds)
    }
}
