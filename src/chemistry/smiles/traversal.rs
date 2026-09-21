//! Ranked depth-first traversal from RDKit Canon.cpp and SmilesWrite.cpp.
//! Copyright (C) 2001-2025 Greg Landrum and other RDKit contributors.
//! BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
//!
//! This stage orders one connected component. Chemical preparation, ranking,
//! and traversal-dependent stereochemistry are separate writer stages.
use super::symbols::{self, AtomProperties, Options, Writer};
use crate::chemistry::{graph::Graph, kekulize::Direction};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
#[cfg(target_os = "macos")]
mod apple_sort;
#[cfg(target_os = "linux")]
mod linux_sort;
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod sort_common;
#[cfg(all(
    test,
    any(target_os = "linux", target_os = "macos", target_os = "windows")
))]
mod sort_tests;
#[cfg(target_os = "windows")]
mod windows_sort;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid SMILES traversal: {0}")]
    Invalid(String),
    #[error("Too many simultaneously open SMILES rings")]
    OpenRings,
    #[error("SMILES traversal resource limit exceeded")]
    Limit,
    #[error(transparent)]
    Symbols(#[from] symbols::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Token {
    Atom(usize),
    Bond { index: usize, left: usize },
    Ring(usize),
    Open,
    Close,
}

#[derive(Debug, Serialize)]
pub struct Traversal {
    tokens: Vec<Token>,
    atom_bond_order: Vec<Vec<usize>>,
    ring_closures: Vec<Vec<usize>>,
    bond_count: usize,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Output {
    pub text: String,
    pub atom_order: Vec<usize>,
    pub bond_order: Vec<usize>,
}

pub fn build(
    graph: &Graph,
    ring_bonds: &[bool],
    ranks: &[u32],
    start: usize,
) -> Result<Traversal, Error> {
    let top = Topology::new(graph, ring_bonds, ranks)?;
    if start >= graph.atoms.len() {
        return Err(invalid("Missing start atom"));
    }
    let (ring_closures, closures) = top.find_cycles(start)?;
    let mut state = Builder {
        top: &top,
        closures,
        colors: vec![Color::White; graph.atoms.len()],
        ring_numbers: vec![None; graph.bonds.len()],
        available: (1..=1024).collect(),
        result: Traversal {
            tokens: Vec::new(),
            atom_bond_order: vec![Vec::new(); graph.atoms.len()],
            ring_closures,
            bond_count: graph.bonds.len(),
        },
    };
    let mut pending = vec![state.enter(start, None, false)?];
    while let Some(frame) = pending.last_mut() {
        if let Some(&edge) = frame.edges.get(frame.next) {
            let branch = frame.next + 1 != frame.edges.len();
            frame.next += 1;
            if *at(&state.colors, edge.atom)? != Color::White {
                continue;
            }
            let parent = frame.atom;
            at_mut(&mut state.result.atom_bond_order, parent)?.push(edge.bond);
            if branch {
                state.result.tokens.push(Token::Open);
            }
            state.result.tokens.push(Token::Bond {
                index: edge.bond,
                left: parent,
            });
            pending.push(state.enter(edge.atom, Some(edge.bond), branch)?);
        } else {
            *at_mut(&mut state.colors, frame.atom)? = Color::Black;
            if frame.close_branch {
                state.result.tokens.push(Token::Close);
            }
            pending.pop();
        }
    }
    Ok(state.result)
}

fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}
fn at<T>(values: &[T], index: usize) -> Result<&T, Error> {
    values
        .get(index)
        .ok_or_else(|| invalid("Missing traversal item"))
}
fn at_mut<T>(values: &mut [T], index: usize) -> Result<&mut T, Error> {
    values
        .get_mut(index)
        .ok_or_else(|| invalid("Missing traversal item"))
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Color {
    White,
    Gray,
    Black,
}
#[derive(Clone, Copy)]
struct Edge {
    atom: usize,
    bond: usize,
}
struct Frame {
    atom: usize,
    edges: Vec<Edge>,
    next: usize,
    close_branch: bool,
}
struct Topology<'a> {
    adjacent: Vec<Vec<Edge>>,
    kinds: Vec<i32>,
    ranks: &'a [u32],
    ring_bonds: &'a [bool],
}
impl<'a> Topology<'a> {
    fn new(graph: &Graph, ring_bonds: &'a [bool], ranks: &'a [u32]) -> Result<Self, Error> {
        graph.validate().map_err(Error::Invalid)?;
        if ranks.len() != graph.atoms.len() || ring_bonds.len() != graph.bonds.len() {
            return Err(invalid("Rank or ring membership count differs from graph"));
        }
        let mut seen = vec![false; ranks.len()];
        for &rank in ranks {
            let used = at_mut(&mut seen, usize::try_from(rank).map_err(|_| Error::Limit)?)?;
            if *used {
                return Err(invalid("Traversal requires unique atom ranks"));
            }
            *used = true;
        }
        let mut adjacent = vec![Vec::new(); graph.atoms.len()];
        let mut kinds = Vec::with_capacity(graph.bonds.len());
        for (bond, b) in graph.bonds.iter().enumerate() {
            at_mut(&mut adjacent, b.a)?.push(Edge { atom: b.b, bond });
            at_mut(&mut adjacent, b.b)?.push(Edge { atom: b.a, bond });
            kinds.push(match b.order {
                0 => 14,
                4 => 12,
                5 => 17,
                6 => 4,
                n => i32::from(n),
            });
        }
        Ok(Self {
            adjacent,
            kinds,
            ranks,
            ring_bonds,
        })
    }

    fn choices(
        &self,
        atom: usize,
        incoming: Option<usize>,
        colors: &[Color],
        closures: Option<&[bool]>,
    ) -> Result<Vec<Edge>, Error> {
        let mut choices = Vec::new();
        for &edge in at(&self.adjacent, atom)? {
            if incoming == Some(edge.bond) {
                continue;
            }
            let color = *at(colors, edge.atom)?;
            if let Some(closures) = closures
                && (color != Color::White || *at(closures, edge.bond)?)
            {
                continue;
            }
            let kind = *at(&self.kinds, edge.bond)?;
            let mut key = i32::try_from(*at(self.ranks, edge.atom)?).map_err(|_| Error::Limit)?;
            // These offsets and their order match the reference's signed
            // conversion of its unsigned rank arithmetic. Graph bounds keep
            // every key inside i32, including negative ring-closure ranks.
            if closures.is_none() && color == Color::Gray {
                key -= 33 * 5000 * 5000;
                key += (32 - kind) * 5000;
            } else if *at(self.ring_bonds, edge.bond)? {
                key += (32 - kind) * 5000 * 5000;
            }
            choices.push((key, edge));
        }
        #[cfg(target_os = "linux")]
        if self.ranks.len() > 5000 {
            // Native rank offsets can collide beyond MAX_NATOMS. The Linux
            // reference uses an unstable median-of-three introsort for ties.
            linux_sort::sort(&mut choices)?;
        } else {
            choices.sort_by_key(|(rank, _)| *rank);
        }
        #[cfg(target_os = "macos")]
        if self.ranks.len() > 5000 {
            apple_sort::sort(&mut choices)?;
        } else {
            choices.sort_by_key(|(rank, _)| *rank);
        }
        #[cfg(target_os = "windows")]
        if self.ranks.len() > 5000 {
            windows_sort::sort(&mut choices)?;
        } else {
            choices.sort_by_key(|(rank, _)| *rank);
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        choices.sort_by_key(|(rank, _)| *rank);
        Ok(choices.into_iter().map(|(_, edge)| edge).collect())
    }

    fn find_cycles(&self, start: usize) -> Result<(Vec<Vec<usize>>, Vec<bool>), Error> {
        let mut colors = vec![Color::White; self.adjacent.len()];
        let mut rings = vec![Vec::new(); self.adjacent.len()];
        let mut closures = vec![false; self.kinds.len()];
        *at_mut(&mut colors, start)? = Color::Gray;
        let mut pending = vec![Frame {
            atom: start,
            edges: self.choices(start, None, &colors, None)?,
            next: 0,
            close_branch: false,
        }];
        while let Some(frame) = pending.last_mut() {
            if let Some(&edge) = frame.edges.get(frame.next) {
                frame.next += 1;
                match *at(&colors, edge.atom)? {
                    Color::White => {
                        *at_mut(&mut colors, edge.atom)? = Color::Gray;
                        pending.push(Frame {
                            atom: edge.atom,
                            edges: self.choices(edge.atom, Some(edge.bond), &colors, None)?,
                            next: 0,
                            close_branch: false,
                        });
                    }
                    Color::Gray => {
                        at_mut(&mut rings, edge.atom)?.push(edge.bond);
                        at_mut(&mut rings, frame.atom)?.push(edge.bond);
                        *at_mut(&mut closures, edge.bond)? = true;
                    }
                    Color::Black => (),
                }
            } else {
                *at_mut(&mut colors, frame.atom)? = Color::Black;
                pending.pop();
            }
        }
        if colors.iter().any(|&c| c != Color::Black) {
            return Err(invalid("Traversal requires one connected component"));
        }
        Ok((rings, closures))
    }
}

struct Builder<'a, 'g> {
    top: &'a Topology<'g>,
    closures: Vec<bool>,
    colors: Vec<Color>,
    ring_numbers: Vec<Option<usize>>,
    available: BTreeSet<usize>,
    result: Traversal,
}
impl Builder<'_, '_> {
    fn enter(
        &mut self,
        atom: usize,
        incoming: Option<usize>,
        close_branch: bool,
    ) -> Result<Frame, Error> {
        self.result.tokens.push(Token::Atom(atom));
        *at_mut(&mut self.colors, atom)? = Color::Gray;
        let order = at_mut(&mut self.result.atom_bond_order, atom)?;
        order.extend(incoming);
        let mut closed = Vec::new();
        for &bond in at(&self.result.ring_closures, atom)? {
            order.push(bond);
            if let Some(number) = *at(&self.ring_numbers, bond)? {
                self.result.tokens.push(Token::Bond {
                    index: bond,
                    left: atom,
                });
                self.result.tokens.push(Token::Ring(number));
                closed.push(number);
            } else {
                let number = self.available.pop_first().ok_or(Error::OpenRings)?;
                *at_mut(&mut self.ring_numbers, bond)? = Some(number);
                self.result.tokens.push(Token::Ring(number));
            }
        }
        // A label closed here becomes available only after all this atom's
        // closures, preventing an opening from reusing it on the same atom.
        self.available.extend(closed);
        Ok(Frame {
            atom,
            edges: self
                .top
                .choices(atom, incoming, &self.colors, Some(&self.closures))?,
            next: 0,
            close_branch,
        })
    }
}

impl Traversal {
    pub fn tokens(&self) -> &[Token] {
        &self.tokens
    }
    pub fn atom_bond_order(&self) -> &[Vec<usize>] {
        &self.atom_bond_order
    }
    pub fn ring_closures(&self) -> &[Vec<usize>] {
        &self.ring_closures
    }
    /// Render only after the caller adjusts winding and directions for this
    /// traversal. This function does not perceive or canonicalize stereo.
    pub fn render(
        &self,
        writer: &Writer<'_>,
        directions: &[Direction],
        properties: &[AtomProperties<'_>],
        options: Options,
    ) -> Result<Output, Error> {
        if directions.len() != self.bond_count || properties.len() != self.atom_bond_order.len() {
            return Err(invalid("Symbol context differs from traversal"));
        }
        let mut result = Output {
            text: String::new(),
            atom_order: Vec::new(),
            bond_order: Vec::new(),
        };
        let mut ring_map = BTreeMap::new();
        let mut available: BTreeSet<usize> = (1..=1024).collect();
        let mut closed = Vec::new();
        for token in &self.tokens {
            match *token {
                Token::Atom(atom) => {
                    for number in closed.drain(..) {
                        let label = ring_map
                            .remove(&number)
                            .ok_or_else(|| invalid("Missing closed ring"))?;
                        available.insert(label);
                    }
                    result
                        .text
                        .push_str(&writer.atom(atom, options, *at(properties, atom)?)?);
                    result.atom_order.push(atom);
                }
                Token::Bond { index, left } => {
                    result.text.push_str(writer.bond(
                        index,
                        *at(directions, index)?,
                        left,
                        options,
                    )?);
                    result.bond_order.push(index);
                }
                Token::Ring(number) => {
                    let label = if let Some(&label) = ring_map.get(&number) {
                        closed.push(number);
                        label
                    } else {
                        let label = available.pop_first().ok_or(Error::OpenRings)?;
                        ring_map.insert(number, label);
                        label
                    };
                    match label {
                        1..=9 => result.text.push(char::from(b'0' + label as u8)),
                        10..=99 => {
                            result.text.push('%');
                            result.text.push_str(&label.to_string());
                        }
                        _ => {
                            result.text.push_str("%(");
                            result.text.push_str(&label.to_string());
                            result.text.push(')');
                        }
                    }
                }
                Token::Open => result.text.push('('),
                Token::Close => result.text.push(')'),
            }
            if result.text.len() > 16 * 1024 * 1024 {
                return Err(Error::Limit);
            }
        }
        if ring_map.len() != closed.len() {
            return Err(invalid("Unclosed output ring"));
        }
        Ok(result)
    }
}
