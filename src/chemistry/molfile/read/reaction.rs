//! RXN framing adapted from RDKit MDLParser.cpp (2026.03.6).
//! Copyright (C) 2007-2026 Novartis Institutes for BioMedical Research Inc.
//! and other RDKit contributors. BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
use super::*;
use crate::chemistry::reaction::Imported as Reaction;

fn count(reader: &Reader<'_>, text: &str) -> Result<usize> {
    let text = text.trim_matches(' ');
    // Native counts are unsigned 32-bit lexical casts, including defined
    // unsigned negation. Apply the application limit after that conversion.
    let parsed = if let Some(magnitude) = text.strip_prefix('-') {
        if magnitude.is_empty() || !magnitude.bytes().all(|b| b.is_ascii_digit()) {
            return Err(reader.invalid("Invalid reaction participant count"));
        }
        magnitude.parse::<u32>().map(u32::wrapping_neg)
    } else {
        text.parse::<u32>()
    };
    let value = parsed.map_err(|_| reader.invalid("Invalid reaction participant count"))?;
    if value > 10_000 {
        return Err(ReadError::Limit);
    }
    Ok(value as usize)
}

pub(crate) fn read(text: &str) -> Result<Reaction> {
    if text.len() > 16 * 1024 * 1024 {
        return Err(ReadError::Limit);
    }
    if text.contains('\0') {
        return Err(ReadError::Invalid {
            line: 0,
            message: "NUL in reaction file".into(),
        });
    }
    let mut reader = Reader {
        lines: text.lines(),
        line: 0,
    };
    let header = reader.next()?;
    if !header.starts_with("$RXN") {
        return Err(reader.invalid("Missing RXN header"));
    }
    let v3 = header.get(5..10) == Some("V3000");
    for _ in 0..3 {
        reader.next()?;
    }
    let counts = if v3 {
        let line = reader.v3()?;
        let tokens: Vec<_> = line
            .trim_matches(|c: char| c.is_ascii_whitespace())
            .split([' ', '\t'])
            .filter(|s| !s.is_empty())
            .collect();
        match tokens.as_slice() {
            [keyword, reactants, products, rest @ ..] if keyword.eq_ignore_ascii_case("COUNTS") => {
                [
                    count(&reader, reactants)?,
                    count(&reader, products)?,
                    rest.first()
                        .map(|value| count(&reader, value))
                        .transpose()?
                        .unwrap_or(0),
                ]
            }
            _ => return Err(reader.invalid("Missing reaction counts")),
        }
    } else {
        let line = reader.next()?;
        let agents = if line.len() > 6 {
            reader.field(line, 6, line.len().min(9))?
        } else {
            ""
        };
        [
            count(&reader, reader.field(line, 0, 3)?)?,
            count(&reader, reader.field(line, 3, 6)?)?,
            if agents
                .trim_matches(|c: char| c.is_ascii_whitespace())
                .is_empty()
            {
                0
            } else {
                count(&reader, agents)?
            },
        ]
    };
    if counts[0] == 0 || counts[1] == 0 {
        return Err(reader.invalid("A reaction needs at least one reactant and one product"));
    }
    let mut total = 0usize;
    let mut row = |n: usize, role: &str, agent: bool| -> Result<Vec<Imported>> {
        if v3 && (n > 0 || !agent) {
            reader.expect(&format!("BEGIN {role}"))?;
        }
        let mut parts = Vec::new();
        for _ in 0..n {
            let parsed = if v3 {
                let mut parsed = Parsed::new(false);
                v3000::read(&mut reader, &mut parsed, false)?;
                parsed
            } else {
                if !reader.next()?.starts_with("$MOL") {
                    return Err(reader.invalid("Missing MOL participant header"));
                }
                read_molecule(&mut reader)?
            };
            if parsed.graph.atoms.is_empty() {
                return Err(reader.invalid("Empty reaction participant"));
            }
            total = total
                .checked_add(parsed.graph.atoms.len())
                .ok_or(ReadError::Limit)?;
            if total > 10_000 {
                return Err(ReadError::Limit);
            }
            let sanitize_file = v3 || !agent;
            let mut imported = parsed.finish_in(Context::Reaction { agent, v3000: v3 })?;
            // Import sanitizes the unwrapped graph once more, invalidating native
            // computed caches while retaining user and legacy CIP annotations.
            if sanitize_file {
                let state = &mut imported.molecule.state;
                let cleaned = sanitize::sanitize(&state.graph, &state.metadata, &state.directions)?;
                state.graph = cleaned.graph;
                state.metadata = cleaned.metadata;
                state.directions = cleaned.directions;
                state.valences = cleaned.valences;
                state.conjugated = cleaned.conjugated;
                state.hybridizations = cleaned.hybridizations;
                state.rings = perception::RingCache {
                    kind: perception::RingKind::Symmetric,
                    atoms: cleaned.rings,
                };
                state.properties.done = None;
                for atom in &mut state.properties.atoms {
                    atom.cip_rank = None;
                    atom.ring_candidate = None;
                    atom.ring_members = None;
                }
                for atom in &mut state.metadata.atoms {
                    atom.ring_stereo = false;
                }
            }
            parts.push(imported);
        }
        if v3 && (n > 0 || !agent) {
            reader.expect(&format!("END {role}"))?;
        }
        Ok(parts)
    };
    Ok(Reaction {
        reactants: row(counts[0], "REACTANT", false)?,
        products: row(counts[1], "PRODUCT", false)?,
        agents: row(counts[2], "AGENT", true)?,
    })
}
