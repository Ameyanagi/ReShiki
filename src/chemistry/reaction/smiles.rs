//! Reaction framing adapted from RDKit DaylightParser.cpp (2026.03.6).
//! Copyright (C) 2007-2021 Novartis Institutes for BioMedical Research Inc.
//! and other RDKit contributors. BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
use crate::chemistry::{
    cx, graph::Graph, kekulize::Direction, ranking::Metadata, sanitize, smiles as molecule,
};
use serde::Serialize;

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
}
impl Part {
    fn read(text: &str, count: &mut usize) -> Result<Self> {
        let parsed = molecule::parse(text)?;
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
        })
    }
    fn finish(
        self,
        suffix: &str,
        atoms: &mut usize,
        bonds: &mut usize,
    ) -> Result<molecule::Imported> {
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
            cx::read_at(suffix, &topology, *atoms, *bonds)
                .map_err(molecule::Error::from)?
                .events
        };
        *atoms = atoms.checked_add(topology.atoms).ok_or(ReadError::Limit)?;
        *bonds = bonds
            .checked_add(topology.bonds.len())
            .ok_or(ReadError::Limit)?;
        let parsed = molecule::Parsed {
            query_bonds: vec![false; self.graph.bonds.len()],
            graph: self.graph,
            metadata: self.metadata,
            directions: self.directions,
            dummy_labels: self.labels,
            bond_indices: self.indices,
            ring_bonds: self.rings,
        };
        molecule::reaction_part(parsed, events).map_err(|error| match error {
            molecule::Error::Sanitization(error) => ReadError::Sanitization(error),
            error => ReadError::Molecule(error),
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
        }
        Ok(parts)
    }
}

/// Read reaction SMILES participants, including global CX annotations.
/// Drawing layout and application import integration remain separate.
pub fn read(text: &str) -> Result<SmilesReaction> {
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
    let mut row = |parts: Vec<&str>| -> Result<Vec<Part>> {
        parts
            .into_iter()
            .map(|p| Part::read(p, &mut count))
            .collect()
    };
    let reactants = row(reactants)?;
    let products = row(products)?;
    let agents = text.get(left + 1..right).ok_or(ReadError::Limit)?;
    let agents = if agents.is_empty() {
        Vec::new()
    } else {
        Part::read(agents, &mut count)?.fragments()?
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
    let (mut atoms, mut bonds) = (0, 0);
    let mut finish = |parts: Vec<Part>| -> Result<Vec<molecule::Imported>> {
        parts
            .into_iter()
            .map(|p| p.finish(suffix, &mut atoms, &mut bonds))
            .collect()
    };
    let reactants = finish(reactants)?;
    let agents = finish(agents)?;
    let products = finish(products)?;
    Ok(SmilesReaction {
        reactants,
        products,
        agents,
    })
}
