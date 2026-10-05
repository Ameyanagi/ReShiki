//! Substance groups adapted from RDKit MolSGroupParsing.cpp/MolFileParser.cpp.
//! Copyright (C) 2002-2021 Greg Landrum, T5 Informatics GmbH and contributors.
//! BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
use super::*;
use std::collections::{BTreeMap, HashMap, HashSet};
mod v2000;
mod v3000;
pub(super) use v3000::read as read_v3000;

const TYPES: &[&str] = &[
    "SRU", "MON", "COP", "CRO", "GRA", "MOD", "MER", "ANY", "COM", "MIX", "FOR", "SUP", "MUL",
    "DAT", "GEN",
];

#[derive(Default)]
pub(super) struct Groups {
    entries: BTreeMap<i32, Group>,
    data: String,
    data_group: i32,
    data_lines: usize,
}
struct Group {
    kind: String,
    atoms: Vec<usize>,
    atom_set: HashSet<usize>,
    bonds: Vec<usize>,
    bond_set: HashSet<usize>,
    field: Option<String>,
    query: Option<String>,
    operator: Option<String>,
    data: Vec<String>,
}

// Native FileParserUtils checks the character set, then uses from_chars.
// It accepts a numeric prefix and leaves the initialized value at zero on
// overflow or a leading '+'. Keep those extension-field rules explicitly.
fn numeric_field(text: &str, signed: bool) -> Result<i64> {
    if text
        .bytes()
        .any(|b| !b.is_ascii_digit() && b != b' ' && b != b'+' && !(signed && b == b'-'))
    {
        return Err(ReadError::Chemistry(
            "Invalid substance-group integer".into(),
        ));
    }
    let text = text.trim_start_matches(' ');
    let offset = usize::from(signed && text.starts_with('-'));
    let length = text
        .bytes()
        .skip(offset)
        .take_while(u8::is_ascii_digit)
        .count()
        + offset;
    let prefix = text.get(..length).ok_or(ReadError::Limit)?;
    Ok(if signed {
        i64::from(prefix.parse::<i32>().unwrap_or(0))
    } else {
        i64::from(prefix.parse::<u32>().unwrap_or(0))
    })
}
impl Group {
    fn new(r: &Reader<'_>, kind: &str) -> Result<Self> {
        if !TYPES.contains(&kind) {
            return Err(r.invalid("Unknown substance-group type"));
        }
        Ok(Self {
            kind: kind.into(),
            atoms: Vec::new(),
            atom_set: HashSet::new(),
            bonds: Vec::new(),
            bond_set: HashSet::new(),
            field: None,
            query: None,
            operator: None,
            data: Vec::new(),
        })
    }
    fn atom(&mut self, index: usize) {
        self.atoms.push(index);
        self.atom_set.insert(index);
    }
    fn bond(&mut self, index: usize) {
        self.bonds.push(index);
        self.bond_set.insert(index);
    }
    fn parent(&self, r: &Reader<'_>, index: usize) -> Result<()> {
        if self.atom_set.contains(&index) {
            Ok(())
        } else {
            Err(r.invalid("Parent atom is outside substance group"))
        }
    }
    fn crossing(&self, r: &Reader<'_>, p: &Parsed, index: usize) -> Result<()> {
        let bond = p
            .graph
            .bonds
            .get(index)
            .ok_or_else(|| r.invalid("Missing group bond"))?;
        if self.bond_set.contains(&index)
            && (self.atom_set.contains(&bond.a) ^ self.atom_set.contains(&bond.b))
        {
            Ok(())
        } else {
            Err(r.invalid("Substance-group bond vector is not on a crossing bond"))
        }
    }
}

impl Groups {
    // This pass follows the file's explicit valence processing. Native field
    // storage narrows signed charges and H counts to eight bits; retain that
    // defined conversion, including negative and overflowing extension values.
    pub(super) fn apply(&self, p: &mut Parsed) -> Result<()> {
        let mut aromatic = vec![false; p.graph.atoms.len()];
        for bond in &p.graph.bonds {
            if bond.aromatic || bond.order == 4 {
                for atom in [bond.a, bond.b] {
                    *aromatic.get_mut(atom).ok_or(ReadError::Limit)? = true;
                }
            }
        }
        let mut work = 0usize;
        for group in self.entries.values().filter(|g| g.kind == "DAT") {
            work = work.saturating_add(group.data.len().saturating_mul(group.atoms.len().max(1)));
            if work > 10_000_000 {
                return Err(ReadError::Limit);
            }
            match group.field.as_deref() {
                Some("MRV_COORDINATE_BOND_TYPE") => {
                    if let Some(data) = group.data.first() {
                        let index = numeric_field(data, false)?
                            .checked_sub(1)
                            .and_then(|n| usize::try_from(n).ok());
                        if let Some(index) = index
                            && let Some(props) = p.bonds.get_mut(index)
                            && props.unspecified
                        {
                            let bond = p.graph.bonds.get_mut(index).ok_or(ReadError::Limit)?;
                            bond.order = 5;
                            // replaceBond increases the order by one, reducing
                            // explicit H on both ends, and resets bond stereo.
                            for atom in [bond.a, bond.b] {
                                let atom = p.graph.atoms.get_mut(atom).ok_or(ReadError::Limit)?;
                                atom.explicit_hydrogens = atom.explicit_hydrogens.saturating_sub(1);
                            }
                            *p.directions.get_mut(index).ok_or(ReadError::Limit)? = Direction::None;
                            let metadata =
                                p.metadata.bonds.get_mut(index).ok_or(ReadError::Limit)?;
                            metadata.stereo = 0;
                            metadata.stereo_atoms.clear();
                            *props = FileBond::default();
                        }
                    }
                }
                Some("MRV_IMPLICIT_H") => {
                    for data in &group.data {
                        if let Some(value) = data.strip_prefix("IMPL_H") {
                            let hs = numeric_field(value, true)? as u8;
                            for &index in &group.atoms {
                                if aromatic.get(index) == Some(&true) {
                                    p.graph
                                        .atoms
                                        .get_mut(index)
                                        .ok_or(ReadError::Limit)?
                                        .explicit_hydrogens = hs;
                                }
                            }
                        }
                    }
                }
                Some("ZBO") if !group.bonds.is_empty() => {
                    return Err(ReadError::Unsupported("zero-order substance-group bond"));
                }
                Some("ZBO") => (),
                Some(field @ ("ZCH" | "HYD")) => {
                    for data in &group.data {
                        let values = data.trim().split(';').collect::<Vec<_>>();
                        if values.len() < group.atoms.len() {
                            continue;
                        }
                        for (&index, value) in group.atoms.iter().zip(values) {
                            let value = numeric_field(value, true)?;
                            let atom = p.graph.atoms.get_mut(index).ok_or(ReadError::Limit)?;
                            if field == "ZCH" {
                                atom.charge = value as i8;
                            } else {
                                atom.explicit_hydrogens = value as u8;
                            }
                        }
                    }
                }
                _ => {
                    if matches!(group.query.as_deref(), Some("SMARTSQ" | "SQ"))
                        && group.operator.as_deref().is_none_or(|op| op == "=")
                        && group.data.first().is_some_and(|v| !v.is_empty())
                        && !group.atoms.is_empty()
                    {
                        let data = group.data.first().ok_or(ReadError::Limit)?;
                        match crate::chemistry::smarts::validate(data) {
                            Ok(0) | Err(crate::chemistry::smarts::Error::Syntax(_)) => (),
                            Ok(count) => {
                                let number = if count == 1 {
                                    crate::chemistry::smarts::atomic_number_query(data).map_err(
                                        |error| match error {
                                            crate::chemistry::smarts::Error::Limit => {
                                                ReadError::Limit
                                            }
                                            crate::chemistry::smarts::Error::Syntax(_) => {
                                                ReadError::Unsupported("substance-group query")
                                            }
                                        },
                                    )?
                                } else {
                                    None
                                };
                                for &index in &group.atoms {
                                    p.atoms.get_mut(index).ok_or(ReadError::Limit)?.query =
                                        number.map_or(FileQuery::Other, FileQuery::Number);
                                }
                            }
                            Err(crate::chemistry::smarts::Error::Limit) => {
                                return Err(ReadError::Limit);
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

struct Fixed<'text, 'r, 'input> {
    reader: &'r Reader<'input>,
    text: &'text str,
    pos: usize,
}
impl Fixed<'_, '_, '_> {
    fn minimum(&self, count: usize) -> Result<()> {
        if self.text.len() < self.pos + count {
            Err(self.reader.invalid("Truncated substance-group record"))
        } else {
            Ok(())
        }
    }
    fn integer(&mut self, counter: bool) -> Result<i32> {
        self.pos += 1;
        let len = if counter { 2 } else { 3 };
        let end = (self.pos + len).min(self.text.len());
        let text = self.reader.field(self.text, self.pos, end)?;
        self.pos += len;
        numeric_field(text, true).map(|n| n as i32)
    }
    fn count(&mut self) -> Result<usize> {
        let n = self.integer(true)?;
        self.reader.count(n, 100_000)
    }
    fn float(&mut self) -> Result<f64> {
        let end = (self.pos + 10).min(self.text.len());
        let text = self.reader.field(self.text, self.pos, end)?;
        self.pos += 10;
        self.reader.coordinate(text, false)
    }
}
