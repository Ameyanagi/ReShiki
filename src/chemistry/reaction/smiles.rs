//! Reaction framing adapted from RDKit DaylightParser.cpp (2026.03.6).
//! Copyright (C) 2007-2021 Novartis Institutes for BioMedical Research Inc.
//! and other RDKit contributors. BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
use crate::chemistry::{
    graph::Graph,
    kekulize::Direction,
    ranking::Metadata,
    sanitize, smiles as molecule,
    stereo::perception::{Properties, RingCache, RingKind, State},
};
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    #[error("Invalid reaction SMILES: {0}")]
    Invalid(&'static str),
    #[error("Reaction SMILES exceeds the 10,000 atom or 16 MB limit")]
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
        })
    }
    fn finish(self) -> Result<molecule::Imported> {
        let clean = sanitize::sanitize(&self.graph, &self.metadata, &self.directions)?;
        if clean.graph.atoms.iter().any(|a| a.radical_electrons > 2)
            || clean.metadata.atoms.iter().any(|a| a.chiral_tag > 2)
            || clean.metadata.bonds.iter().any(|b| b.stereo > 5)
            || !clean.metadata.groups.is_empty()
        {
            return Err(invalid(
                "Unsupported participant stereochemistry or radical count",
            ));
        }
        let properties = Properties::unspecified(&clean.graph);
        Ok(molecule::Imported {
            prepared: molecule::Prepared {
                state: State {
                    graph: clean.graph,
                    metadata: clean.metadata,
                    directions: clean.directions,
                    valences: clean.valences,
                    conjugated: clean.conjugated,
                    hybridizations: clean.hybridizations,
                    rings: RingCache {
                        kind: RingKind::Symmetric,
                        atoms: clean.rings,
                    },
                    properties,
                },
                dummy_labels: self.labels,
            },
            conformers: Vec::new(),
            name: None,
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
        }
        Ok(parts)
    }
}

/// Read bare reaction SMILES without layout or CX extensions. This preparatory
/// API is not the application's import path until extended syntax is migrated.
pub fn read(text: &str) -> Result<SmilesReaction> {
    if text.len() > 16 * 1024 * 1024 {
        return Err(ReadError::Limit);
    }
    // The Python entry point strips Unicode whitespace before passing a C string.
    let text = text.trim_matches(|c: char| c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}'));
    let text = text.split('\0').next().unwrap_or_default();
    if text.find('|').is_some_and(|i| i > 0) {
        return Err(invalid("CX reaction extensions are not migrated yet"));
    }
    let text = text.trim_matches(|c: char| c.is_ascii_whitespace());
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
    let mut row = |parts: Vec<&str>| -> Result<Vec<molecule::Imported>> {
        parts
            .into_iter()
            .map(|p| Part::read(p, &mut count)?.finish())
            .collect()
    };
    let reactants = row(reactants)?;
    let products = row(products)?;
    let agents = text.get(left + 1..right).ok_or(ReadError::Limit)?;
    let agents = if agents.is_empty() {
        Vec::new()
    } else {
        Part::read(agents, &mut count)?
            .fragments()?
            .into_iter()
            .map(Part::finish)
            .collect::<Result<_>>()?
    };
    Ok(SmilesReaction {
        reactants,
        products,
        agents,
    })
}
