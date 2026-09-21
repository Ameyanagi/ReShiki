//! Bounded SMARTS syntax validation for imported substance-group queries.
//! Grammar adapted from RDKit smarts.ll/smarts.yy and SmilesParseOps.cpp.
//! Copyright (C) 2001-2025 Greg Landrum, Rational Discovery LLC and contributors.
//! BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
//!
//! This validates syntax and graph construction, not chemical satisfiability.
//! Query matching is separate.
use std::collections::{HashMap, HashSet};
mod cx;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid SMARTS at byte {0}")]
    Syntax(usize),
    #[error("SMARTS exceeds parser resource limits")]
    Limit,
}
type Result<T> = std::result::Result<T, Error>;

/// Return the number of top-level query atoms. The empty string has zero atoms.
/// Invalid queries return `Syntax`; resource bounds never become valid queries.
pub fn validate(text: &str) -> Result<usize> {
    if text.len() > 1024 * 1024 {
        return Err(Error::Limit);
    }
    if text.is_empty() {
        return Ok(0);
    }
    // Native name/CX splitting precedes scanner whitespace trimming.
    let split = text.bytes().position(|b| b == b' ' || b == b'\t');
    let (body, suffix) = match split.filter(|&i| i != 0) {
        Some(i) => (
            text.get(..i).ok_or(Error::Limit)?,
            text.get(i..)
                .ok_or(Error::Limit)?
                .trim_matches(|c: char| c.is_ascii_whitespace()),
        ),
        None => (text, ""),
    };
    // Match the reference lexer's native-char trimming at both boundaries.
    // Work on bytes; a boundary inside malformed/non-ASCII text is never sliced
    // as UTF-8 and cannot panic.
    let bytes = body.as_bytes();
    let start = bytes
        .iter()
        .position(|&b| (b as std::os::raw::c_char) > 32)
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|&b| (b as std::os::raw::c_char) > 32)
        .map_or(start, |i| i + 1);
    let mut parser = Parser {
        bytes: bytes.get(start..end).ok_or(Error::Limit)?,
        pos: 0,
        atoms: 0,
        bond_index: 0,
        topology: Topology::default(),
    };
    let count = parser.molecule(0)?;
    if !matches!(parser.peek(), None | Some(b'\n')) {
        return Err(parser.invalid());
    }
    if suffix.starts_with('|') {
        cx::validate(suffix, &parser.topology)?;
    }
    Ok(count)
}

#[derive(Clone, Copy, Default)]
struct Chirality {
    limit: Option<u32>,
    permutation: Option<u32>,
}
impl Chirality {
    fn and(mut self, other: Self, copy_permutation: bool) -> Self {
        if self.limit.is_none() {
            self.limit = other.limit;
            if copy_permutation && other.permutation.is_some() {
                self.permutation = other.permutation;
            }
        }
        self
    }
    fn valid(self) -> bool {
        self.limit
            .zip(self.permutation)
            .is_none_or(|(limit, n)| n <= limit)
    }
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
    atoms: usize,
    bond_index: usize,
    topology: Topology,
}

#[derive(Default)]
struct Topology {
    atoms: usize,
    bonds: Vec<ParseBond>,
}
struct ParseBond {
    a: usize,
    b: usize,
    index: usize,
}
impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }
    fn starts(&self, text: &[u8]) -> bool {
        self.bytes
            .get(self.pos..)
            .is_some_and(|s| s.starts_with(text))
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
        let mut value = u32::from(first - b'0');
        if value == 0 {
            return Ok(0);
        }
        while let Some(digit) = self.peek().filter(u8::is_ascii_digit) {
            // The native grammar rejects even the final 2147483640..7 range.
            if value >= i32::MAX as u32 / 10 {
                return Err(self.invalid());
            }
            value = value * 10 + u32::from(digit - b'0');
            self.pos += 1;
        }
        Ok(value)
    }
    fn simple(&mut self, bracket: bool) -> bool {
        let n = if self.starts(b"Cl")
            || self.starts(b"Br")
            || (bracket
                && (self.starts(b"si")
                    || self.starts(b"as")
                    || self.starts(b"se")
                    || self.starts(b"te")))
        {
            2
        } else if self
            .peek()
            .is_some_and(|b| b"BCNOFPSIbcnops*Aa".contains(&b))
        {
            1
        } else {
            return false;
        };
        self.pos += n;
        true
    }
    fn element(&mut self) -> bool {
        // The pinned scanner predates the Nh/Mc/Ts/Og names and explicitly
        // recognizes Uut/Uup. Other letters may still form Boolean primitives.
        if self.starts(b"Uut") || self.starts(b"Uup") {
            self.pos += 3;
            return true;
        }
        let count = super::ELEMENTS
            .iter()
            .skip(2)
            .filter(|e| !matches!(e.symbol, "Nh" | "Mc" | "Ts" | "Og"))
            .filter(|e| self.starts(e.symbol.as_bytes()))
            .map(|e| e.symbol.len())
            .max();
        if let Some(n) = count {
            self.pos += n;
            true
        } else {
            false
        }
    }
    fn range(&mut self) -> Result<()> {
        self.require(b'{')?;
        if self.eat(b'-') {
            self.number()?;
        } else {
            self.number()?;
            self.require(b'-')?;
            if self.peek() != Some(b'}') {
                self.number()?;
            }
        }
        self.require(b'}')
    }
    fn class(&self, at: usize) -> Option<(usize, u32)> {
        if self.bytes.get(at) != Some(&b'@') {
            return None;
        }
        let mut end = at + 1;
        while matches!(self.bytes.get(end), Some(b' ' | b'\'')) {
            end += 1;
        }
        let rest = self.bytes.get(end..)?;
        [(b"TH", 2), (b"AL", 2), (b"SP", 3), (b"TB", 20), (b"OH", 30)]
            .into_iter()
            .find(|(name, _)| rest.starts_with(*name))
            .map(|(_, limit)| (end + 2, limit))
    }
    fn point(&mut self, depth: usize) -> Result<Chirality> {
        while self.eat(b'!') {}
        if self.starts(b"$(") {
            self.pos += 2;
            self.molecule(depth + 1)?;
            self.require(b')')?;
            if self.eat(b'_') && self.number()? == 0 {
                return Err(self.invalid());
            }
            return Ok(Chirality::default());
        }
        if let Some((end, limit)) = self.class(self.pos) {
            self.pos = end;
            let n = if self.peek().is_some_and(|b| b.is_ascii_digit()) {
                let n = self.number()?;
                if n == 0 {
                    return Err(self.invalid());
                }
                n
            } else {
                0
            };
            return Ok(Chirality {
                limit: Some(limit),
                permutation: Some(n),
            });
        }
        if self.eat(b'@') {
            // @ followed by @TH is two distinct lexer tokens, not @@ then TH.
            if self.class(self.pos).is_none() {
                self.eat(b'@');
            }
            return Ok(Chirality {
                limit: Some(u32::MAX),
                permutation: None,
            });
        }
        // Lexers choose the longest symbol before single-letter primitives.
        if self.element() || self.simple(true) {
            return Ok(Chirality::default());
        }
        let byte = self.peek().ok_or_else(|| self.invalid())?;
        match byte {
            b'0'..=b'9' => {
                self.number()?;
            }
            b'#' => {
                self.pos += 1;
                self.number()?;
            }
            b'H' | b'D' | b'd' | b'X' | b'v' | b'R' | b'r' | b'k' | b'x' | b'h' | b'z' | b'Z' => {
                self.pos += 1;
                if byte != b'H' && self.peek() == Some(b'{') {
                    self.range()?;
                } else if self.peek().is_some_and(|b| b.is_ascii_digit()) {
                    self.number()?;
                }
            }
            b'+' | b'-' => {
                self.pos += 1;
                if self.peek() == Some(b'{') {
                    self.range()?;
                } else if !self.eat(byte) && self.peek().is_some_and(|b| b.is_ascii_digit()) {
                    self.number()?;
                }
            }
            b'^' => {
                self.pos += 1;
                if !matches!(self.peek(), Some(b'0'..=b'5')) {
                    return Err(self.invalid());
                }
                self.pos += 1;
            }
            _ => return Err(self.invalid()),
        }
        Ok(Chirality::default())
    }
    fn conjunction(&mut self, depth: usize) -> Result<Chirality> {
        let mut result = self.point(depth)?;
        loop {
            if matches!(self.peek(), None | Some(b']' | b':' | b',' | b';')) {
                break;
            }
            self.eat(b'&');
            result = result.and(self.point(depth)?, true);
        }
        Ok(result)
    }
    fn disjunction(&mut self, depth: usize) -> Result<Chirality> {
        let mut result = self.conjunction(depth)?;
        while self.eat(b',') {
            result = result.and(self.conjunction(depth)?, false);
        }
        Ok(result)
    }
    fn atom(&mut self, depth: usize) -> Result<Chirality> {
        if self.eat(b'[') {
            // hydrogen_atom takes precedence for [isotope? H charge map?].
            // Once its charge starts, Boolean continuations are not allowed.
            let start = self.pos;
            if self.peek().is_some_and(|b| b.is_ascii_digit()) {
                self.number()?;
            }
            if self.eat(b'H') && matches!(self.peek(), Some(b'+' | b'-')) {
                let sign = self.peek().ok_or_else(|| self.invalid())?;
                self.pos += 1;
                if !self.eat(sign) && self.peek().is_some_and(|b| b.is_ascii_digit()) {
                    self.number()?;
                }
                if self.eat(b':') {
                    self.number()?;
                }
                self.require(b']')?;
                return Ok(Chirality::default());
            }
            self.pos = start;
            let mut result = self.disjunction(depth)?;
            while self.eat(b';') {
                result = result.and(self.disjunction(depth)?, false);
            }
            if self.eat(b':') {
                self.number()?;
            }
            self.require(b']')?;
            Ok(result)
        } else if self.simple(false) {
            Ok(Chirality::default())
        } else {
            Err(self.invalid())
        }
    }
    fn bond_start(&self) -> bool {
        self.peek().is_some_and(|b| b"-=#:$~\\/@!<".contains(&b))
    }
    fn bond(&mut self) -> Result<()> {
        loop {
            while self.eat(b'!') {}
            if self.class(self.pos).is_some() {
                return Err(self.invalid());
            }
            let byte = self.peek().ok_or_else(|| self.invalid())?;
            self.pos += 1;
            match byte {
                b'-' => {
                    self.eat(b'>');
                }
                b'<' => self.require(b'-')?,
                b'\\' => {
                    self.eat(b'\\');
                }
                b'=' | b'#' | b':' | b'$' | b'~' | b'/' | b'@' => (),
                _ => return Err(self.invalid()),
            }
            if matches!(self.peek(), Some(b'&' | b',' | b';')) {
                self.pos += 1;
            } else if !self.bond_start() {
                return Ok(());
            }
        }
    }
    fn ring(&mut self) -> Result<u32> {
        if self.eat(b'%') {
            if self.eat(b'(') {
                let mut n = 0;
                let mut count = 0;
                while let Some(b) = self.peek().filter(u8::is_ascii_digit) {
                    n = n * 10 + u32::from(b - b'0');
                    self.pos += 1;
                    count += 1;
                    if count > 5 {
                        return Err(self.invalid());
                    }
                }
                if count == 0 {
                    return Err(self.invalid());
                }
                self.require(b')')?;
                Ok(n)
            } else {
                let a = self
                    .peek()
                    .filter(|b| matches!(b, b'1'..=b'9'))
                    .ok_or_else(|| self.invalid())?;
                self.pos += 1;
                let b = self
                    .peek()
                    .filter(u8::is_ascii_digit)
                    .ok_or_else(|| self.invalid())?;
                self.pos += 1;
                Ok(u32::from(a - b'0') * 10 + u32::from(b - b'0'))
            }
        } else {
            let b = self
                .peek()
                .filter(u8::is_ascii_digit)
                .ok_or_else(|| self.invalid())?;
            self.pos += 1;
            Ok(u32::from(b - b'0'))
        }
    }
    fn molecule(&mut self, depth: usize) -> Result<usize> {
        if depth > 64 {
            return Err(Error::Limit);
        }
        let first = self.atom(depth)?;
        let mut chirality_valid = first.valid();
        self.atoms += 1;
        if self.atoms > 100_000 {
            return Err(Error::Limit);
        }
        let (mut count, mut current) = (1, 0);
        let mut branches = Vec::new();
        let mut rings = HashMap::new();
        let mut bonds = HashSet::new();
        let mut ordinary = Vec::new();
        let mut closures = Vec::new();
        while let Some(byte) = self.peek() {
            if byte == b'\n' {
                break;
            }
            if byte == b')' {
                if let Some(parent) = branches.pop() {
                    current = parent;
                    self.pos += 1;
                    continue;
                }
                break;
            }
            let mut connected = true;
            let mut require_atom = false;
            let mut specified = false;
            if self.eat(b'(') {
                branches.push(current);
                require_atom = true;
                if self.bond_start() {
                    specified = true;
                    self.bond()?;
                }
            } else if self.eat(b'.') {
                connected = false;
                require_atom = true;
            } else if self.bond_start() {
                specified = true;
                self.bond()?;
            }
            if !require_atom && self.peek().is_some_and(|b| b.is_ascii_digit() || b == b'%') {
                let ring = self.ring()?;
                if let Some(other) = rings.remove(&ring) {
                    let edge = (current.min(other), current.max(other));
                    if current == other || !bonds.insert(edge) {
                        return Err(self.invalid());
                    }
                    if depth == 0 {
                        closures.push((
                            ring,
                            ParseBond {
                                a: other,
                                b: current,
                                index: self.bond_index,
                            },
                        ));
                    }
                    self.bond_index += 1;
                } else {
                    rings.insert(ring, current);
                    if specified {
                        self.bond_index += 1;
                    }
                }
                continue;
            }
            let atom = self.atom(depth)?;
            chirality_valid &= atom.valid();
            if connected {
                bonds.insert((current.min(count), current.max(count)));
                if depth == 0 {
                    ordinary.push(ParseBond {
                        a: current,
                        b: count,
                        index: self.bond_index,
                    });
                }
                self.bond_index += 1;
            }
            current = count;
            count += 1;
            self.atoms += 1;
            if self.atoms > 100_000 {
                return Err(Error::Limit);
            }
        }
        if !branches.is_empty() || !rings.is_empty() || (depth == 0 && !chirality_valid) {
            return Err(self.invalid());
        }
        if depth == 0 {
            // Native closure bonds are appended by bookmark order; equal
            // bookmarks retain the order in which their pairs were parsed.
            closures.sort_by_key(|(ring, _)| *ring);
            ordinary.extend(closures.into_iter().map(|(_, bond)| bond));
            self.topology = Topology {
                atoms: count,
                bonds: ordinary,
            };
        }
        Ok(count)
    }
}
