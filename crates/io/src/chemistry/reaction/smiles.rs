//! Reaction framing adapted from RDKit DaylightParser.cpp (2026.03.6).
//! Copyright (C) 2007-2021 Novartis Institutes for BioMedical Research Inc.
//! and other RDKit contributors. BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
use crate::chemistry::{
    cx, graph::Graph, kekulize::Direction, ranking::Metadata, sanitize, smiles as molecule,
};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    #[error("Invalid reaction SMILES: {0}")]
    Invalid(&'static str),
    #[error("Reaction SMILES exceeds atom, input size or annotation work limits")]
    Limit,
    #[error(transparent)]
    Molecule(#[from] molecule::Error),
    #[error(transparent)]
    Sanitization(#[from] sanitize::Error),
}
type Result<T> = std::result::Result<T, ReadError>;

/// Chemical participants before 2D layout. Unlike ordinary SMILES import, this
/// path retains explicit H and does not assign legacy stereochemistry.
#[derive(Debug, Serialize)]
pub struct SmilesReaction {
    pub reactants: Vec<molecule::Imported>,
    pub products: Vec<molecule::Imported>,
    pub agents: Vec<molecule::Imported>,
}

fn invalid(message: &'static str) -> ReadError {
    ReadError::Invalid(message)
}
fn at<T>(items: &[T], i: usize) -> Result<&T> {
    items
        .get(i)
        .ok_or_else(|| invalid("Missing participant graph item"))
}
fn put<T>(items: &mut [T], i: usize) -> Result<&mut T> {
    items
        .get_mut(i)
        .ok_or_else(|| invalid("Missing participant graph item"))
}
fn separator(bytes: &[u8], i: usize) -> bool {
    bytes.get(i) == Some(&b'>') && (i == 0 || bytes.get(i - 1) != Some(&b'-'))
}

// Remove adjacent spaces and tabs in linear time, preserving other whitespace.
// The native parser processes reaction arrows before dots, then discards names.
fn tighten(text: &str, arrows: bool) -> Result<String> {
    let bytes = text.as_bytes();
    let punctuation = |i| {
        if arrows {
            separator(bytes, i)
        } else {
            bytes.get(i) == Some(&b'.')
        }
    };
    let mut output = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while let Some(&byte) = bytes.get(i) {
        if matches!(byte, b' ' | b'\t') {
            let start = i;
            while bytes.get(i).is_some_and(|b| matches!(b, b' ' | b'\t')) {
                i += 1;
            }
            if !(start > 0 && punctuation(start - 1) || punctuation(i)) {
                output.extend_from_slice(bytes.get(start..i).ok_or(ReadError::Limit)?);
            }
        } else {
            output.push(byte);
            i += 1;
        }
    }
    String::from_utf8(output).map_err(|_| invalid("Invalid UTF-8 reaction text"))
}

fn components(text: &str) -> Result<Vec<&str>> {
    let (mut start, mut level, mut block) = (0, 0u32, 0);
    let mut result = Vec::new();
    let component = |start: usize, end: usize, block| {
        let (start, end) = if block == 2 {
            (
                start + 1,
                end.checked_sub(1)
                    .ok_or_else(|| invalid("Invalid grouped participant"))?,
            )
        } else {
            (start, end)
        };
        text.get(start..end)
            .ok_or_else(|| invalid("Invalid grouped participant"))
    };
    for (i, byte) in text.bytes().enumerate() {
        match byte {
            b'(' => {
                if i == start {
                    block = 1;
                }
                level = level.wrapping_add(1);
            }
            b')' => {
                if level == 1 && block != 0 {
                    block = 2;
                }
                // The native framing counter is unsigned. Let the molecular
                // reader validate each component: a newline may terminate its
                // chemistry before an unmatched parenthesis in the suffix.
                level = level.wrapping_sub(1);
            }
            b'.' if level == 0 => {
                if result.len() >= 10000 {
                    return Err(ReadError::Limit);
                }
                result.push(component(start, i, block)?);
                start = i + 1;
                block = 0;
            }
            _ => (),
        }
    }
    if start < text.len() {
        if result.len() >= 10000 {
            return Err(ReadError::Limit);
        }
        result.push(component(start, text.len(), block)?);
    }
    Ok(result)
}

struct Part {
    graph: Graph,
    metadata: Metadata,
    directions: Vec<Direction>,
    labels: Vec<Option<String>>,
    indices: Vec<usize>,
    rings: Vec<bool>,
    queries: Vec<bool>,
    positions: Vec<usize>,
}
struct Finished {
    imported: molecule::Imported,
    relative_group: Option<usize>,
}

// File group numbers are reaction-global. Reserve them before assigning the
// relative flag or an explicit group without a number, even when their centres
// belong to different participants.
fn stereo_groups(parts: &mut [&mut Finished]) -> Result<()> {
    let mut taken = HashSet::new();
    let mut missing = HashSet::new();
    for part in parts.iter() {
        for (index, group) in part
            .imported
            .prepared
            .state
            .metadata
            .groups
            .iter()
            .enumerate()
        {
            if group.kind == 0 || part.relative_group == Some(index) {
                continue;
            }
            if group.read_id == 0 {
                missing.insert(group.kind);
            } else {
                taken.insert((group.kind, group.read_id));
            }
        }
    }
    let mut allocate = |kind| -> Result<u32> {
        let number = (1..=u32::MAX)
            .find(|&number| !taken.contains(&(kind, number)))
            .ok_or(ReadError::Limit)?;
        taken.insert((kind, number));
        Ok(number)
    };
    let mut replacements = HashMap::new();
    for kind in [1, 2] {
        if missing.contains(&kind) {
            replacements.insert(kind, allocate(kind)?);
        }
    }
    let relative = if parts.iter().any(|part| part.relative_group.is_some()) {
        Some(allocate(2)?)
    } else {
        None
    };
    for part in parts.iter_mut() {
        for (index, group) in part
            .imported
            .prepared
            .state
            .metadata
            .groups
            .iter_mut()
            .enumerate()
        {
            let number = if part.relative_group == Some(index) {
                relative
            } else if group.kind != 0 && group.read_id == 0 {
                replacements.get(&group.kind).copied()
            } else {
                None
            };
            if let Some(number) = number {
                group.read_id = number;
                group.write_id = number;
            }
        }
    }
    Ok(())
}

impl Part {
    fn read(text: &str, count: &mut usize, partial: bool) -> Result<Self> {
        let parsed = if partial {
            molecule::parse_for_import(text)?
        } else {
            molecule::parse(text)?
        };
        let start = *count;
        if parsed.graph.atoms.is_empty() {
            return Err(invalid("Empty participant"));
        }
        *count = count
            .checked_add(parsed.graph.atoms.len())
            .ok_or(ReadError::Limit)?;
        if *count > 10000 {
            return Err(ReadError::Limit);
        }
        Ok(Self {
            graph: parsed.graph,
            metadata: parsed.metadata,
            directions: parsed.directions,
            labels: parsed.dummy_labels,
            indices: parsed.bond_indices,
            rings: parsed.ring_bonds,
            queries: parsed.query_bonds,
            positions: (start..*count).collect(),
        })
    }
    fn finish(
        self,
        suffix: &str,
        atoms: &mut usize,
        bonds: &mut usize,
        fragments: &mut usize,
        partial: bool,
    ) -> Result<Finished> {
        let topology = cx::Topology {
            atoms: self.graph.atoms.len(),
            bonds: self
                .graph
                .bonds
                .iter()
                .zip(&self.indices)
                .zip(&self.rings)
                .map(|((b, &index), &ring)| cx::ParseBond {
                    a: b.a,
                    b: b.b,
                    index: ring.then_some(index),
                })
                .collect(),
        };
        let events = if suffix.is_empty() {
            Vec::new()
        } else {
            let extension = cx::read_at(suffix, &topology, *atoms, *bonds, *fragments)
                .map_err(molecule::Error::from)?;
            *fragments = fragments
                .checked_add(extension.fragments)
                .ok_or(ReadError::Limit)?;
            extension.events
        };
        *atoms = atoms.checked_add(topology.atoms).ok_or(ReadError::Limit)?;
        *bonds = bonds
            .checked_add(topology.bonds.len())
            .ok_or(ReadError::Limit)?;
        let parsed = molecule::Parsed {
            query_bonds: self.queries,
            graph: self.graph,
            metadata: self.metadata,
            directions: self.directions,
            dummy_labels: self.labels,
            bond_indices: self.indices,
            ring_bonds: self.rings,
        };
        let explicit_groups = events
            .iter()
            .filter_map(|event| match event {
                cx::Event::StereoGroup { kind, id, .. } => Some((*kind, *id)),
                _ => None,
            })
            .collect::<HashSet<_>>()
            .len();
        let imported = if partial {
            molecule::reaction_part_partial(parsed, events, &self.positions)
        } else {
            molecule::reaction_part(parsed, events)
        };
        let imported = imported.map_err(|error| match error {
            molecule::Error::Sanitization(error) => ReadError::Sanitization(error),
            error => ReadError::Molecule(error),
        })?;
        // The chemistry reader retains explicit groups and appends at most one
        // normalized relative group after them.
        let relative_group = (imported.prepared.state.metadata.groups.len() > explicit_groups)
            .then_some(explicit_groups);
        Ok(Finished {
            imported,
            relative_group,
        })
    }

    /// Agents split by connectivity before sanitization, including bonds whose
    /// ring closures connect across textual dots. Preserve native array order.
    fn fragments(self) -> Result<Vec<Self>> {
        let n = self.graph.atoms.len();
        let mut adjacent = vec![Vec::new(); n];
        for b in &self.graph.bonds {
            put(&mut adjacent, b.a)?.push(b.b);
            put(&mut adjacent, b.b)?.push(b.a);
        }
        let mut groups = vec![None; n];
        let mut parts = Vec::new();
        for start in 0..n {
            if at(&groups, start)?.is_some() {
                continue;
            }
            let id = parts.len();
            let mut pending = vec![start];
            *put(&mut groups, start)? = Some(id);
            while let Some(atom) = pending.pop() {
                for &neighbor in at(&adjacent, atom)? {
                    if at(&groups, neighbor)?.is_none() {
                        *put(&mut groups, neighbor)? = Some(id);
                        pending.push(neighbor);
                    }
                }
            }
            parts.push(Self {
                graph: Graph {
                    atoms: Vec::new(),
                    bonds: Vec::new(),
                },
                metadata: Metadata::default(),
                directions: Vec::new(),
                labels: Vec::new(),
                indices: Vec::new(),
                rings: Vec::new(),
                queries: Vec::new(),
                positions: Vec::new(),
            });
        }
        let mut indices = vec![0; n];
        for (i, atom) in self.graph.atoms.into_iter().enumerate() {
            let id = at(&groups, i)?.ok_or_else(|| invalid("Missing agent component"))?;
            let part = put(&mut parts, id)?;
            *put(&mut indices, i)? = part.graph.atoms.len();
            part.graph.atoms.push(atom);
            part.metadata
                .atoms
                .push(at(&self.metadata.atoms, i)?.clone());
            part.labels.push(at(&self.labels, i)?.clone());
            part.positions.push(*at(&self.positions, i)?);
        }
        for (i, mut bond) in self.graph.bonds.into_iter().enumerate() {
            let id = at(&groups, bond.a)?.ok_or_else(|| invalid("Missing agent component"))?;
            if *at(&groups, bond.b)? != Some(id) {
                return Err(invalid("Bond crosses agent components"));
            }
            let part = put(&mut parts, id)?;
            bond.a = *at(&indices, bond.a)?;
            bond.b = *at(&indices, bond.b)?;
            part.graph.bonds.push(bond);
            let mut meta = at(&self.metadata.bonds, i)?.clone();
            meta.stereo_atoms = meta
                .stereo_atoms
                .iter()
                .map(|&a| at(&indices, a).copied())
                .collect::<Result<_>>()?;
            part.metadata.bonds.push(meta);
            part.directions.push(*at(&self.directions, i)?);
            part.indices.push(*at(&self.indices, i)?);
            part.rings.push(*at(&self.rings, i)?);
            part.queries.push(*at(&self.queries, i)?);
        }
        Ok(parts)
    }
}

/// Read reaction SMILES participants, including global CX annotations.
/// Drawing layout and application import integration remain separate.
pub fn read(text: &str) -> Result<SmilesReaction> {
    read_with(text, false)
}

pub(crate) fn read_partial(text: &str) -> Result<SmilesReaction> {
    read_with(text, true)
}

fn read_with(text: &str, partial: bool) -> Result<SmilesReaction> {
    if text.len() > 16 * 1024 * 1024 {
        return Err(ReadError::Limit);
    }
    // The Python entry point strips Unicode whitespace before passing a C string.
    let text = text.trim_matches(|c: char| c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}'));
    let text = text.split('\0').next().unwrap_or_default();
    let (text, suffix) = if let Some(i) = text.find('|').filter(|&i| i > 0) {
        (
            text.get(..i).ok_or(ReadError::Limit)?,
            cx::trim(text.get(i..).ok_or(ReadError::Limit)?),
        )
    } else {
        (text, "")
    };
    let text = cx::trim(text);
    if text
        .bytes()
        .enumerate()
        .filter(|(i, _)| separator(text.as_bytes(), *i))
        .count()
        < 2
    {
        return Err(invalid("A reaction requires two separators"));
    }
    let text = tighten(&tighten(text, true)?, false)?;
    let text = text.split([' ', '\t']).next().unwrap_or_default();
    let positions: Vec<_> = text
        .bytes()
        .enumerate()
        .filter_map(|(i, _)| separator(text.as_bytes(), i).then_some(i))
        .take(3)
        .collect();
    let &[left, right] = positions.as_slice() else {
        return Err(invalid("A reaction requires exactly two separators"));
    };
    let reactants = components(text.get(..left).ok_or(ReadError::Limit)?)?;
    let products = components(text.get(right + 1..).ok_or(ReadError::Limit)?)?;
    if reactants.is_empty() || products.is_empty() {
        return Err(invalid("A reaction needs reactants and products"));
    }
    let mut count = 0;
    let row = |parts: Vec<&str>, count: &mut usize| -> Result<Vec<Part>> {
        parts
            .into_iter()
            .map(|p| Part::read(p, count, partial))
            .collect()
    };
    let reactants = row(reactants, &mut count)?;
    let agents = text.get(left + 1..right).ok_or(ReadError::Limit)?;
    let agent_row = |count: &mut usize| -> Result<Vec<Part>> {
        if agents.is_empty() {
            Ok(Vec::new())
        } else {
            Part::read(agents, count, partial)?.fragments()
        }
    };
    // Partial notices use textual atom positions; retain strict parse/error order.
    let (products, agents) = if partial {
        let agents = agent_row(&mut count)?;
        (row(products, &mut count)?, agents)
    } else {
        let products = row(products, &mut count)?;
        (products, agent_row(&mut count)?)
    };
    // Native CX indices traverse reactants, then agents, then products.
    // Bound repeated syntax scans when many tiny participants share a suffix.
    if suffix
        .len()
        .checked_mul(reactants.len() + agents.len() + products.len())
        .is_none_or(|n| n > 32 * 1024 * 1024)
    {
        return Err(ReadError::Limit);
    }
    let (mut atoms, mut bonds, mut fragments) = (0, 0, 0);
    let mut finish = |parts: Vec<Part>| -> Result<Vec<Finished>> {
        parts
            .into_iter()
            .map(|p| p.finish(suffix, &mut atoms, &mut bonds, &mut fragments, partial))
            .collect()
    };
    let mut reactants = finish(reactants)?;
    let mut agents = finish(agents)?;
    let mut products = finish(products)?;
    stereo_groups(
        &mut reactants
            .iter_mut()
            .chain(&mut agents)
            .chain(&mut products)
            .collect::<Vec<_>>(),
    )?;
    Ok(SmilesReaction {
        reactants: reactants.into_iter().map(|part| part.imported).collect(),
        products: products.into_iter().map(|part| part.imported).collect(),
        agents: agents.into_iter().map(|part| part.imported).collect(),
    })
}
