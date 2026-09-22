//! Atom and bond text from RDKit SmilesWrite.cpp (2026.03.6).
//! Copyright (C) 2002-2025 Greg Landrum and other RDKit contributors.
//! BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
//!
//! These are lexical pieces, not a molecular serializer. A traversal must first
//! establish atom winding, bond directions and ring closure order.
use crate::chemistry::{
    ELEMENTS,
    graph::{Graph, Valence},
    kekulize::Direction,
    ranking::Metadata,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid SMILES writer input: {0}")]
    Invalid(String),
    #[error("Invalid SMILES chirality permutation on atom {0}")]
    Chirality(usize),
    #[error("SMILES writer resource limit exceeded")]
    Limit,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Options {
    pub isomeric: bool,
    pub kekule: bool,
    pub all_hydrogens: bool,
    pub all_bonds: bool,
    pub non_tetrahedral: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            isomeric: true,
            kekule: false,
            all_hydrogens: false,
            all_bonds: false,
            non_tetrahedral: true,
        }
    }
}

#[derive(Clone, Copy, Default)]
pub struct AtomProperties<'a> {
    pub custom_symbol: Option<&'a str>,
    pub supplement: Option<&'a str>,
    /// Presence of the native marker suppresses winding regardless of its value.
    pub broken_chirality: bool,
}

pub struct Writer<'a> {
    graph: &'a Graph,
    metadata: &'a Metadata,
    valences: Vec<Valence>,
    metal_neighbors: Vec<bool>,
}
impl<'a> Writer<'a> {
    pub fn new(
        graph: &'a Graph,
        metadata: &'a Metadata,
        cache: Option<&[Valence]>,
    ) -> Result<Self, Error> {
        let valences = graph.cached_valences(cache).map_err(Error::Invalid)?;
        metadata.validate(graph).map_err(Error::Invalid)?;
        let mut metal_neighbors = vec![false; graph.atoms.len()];
        for bond in &graph.bonds {
            for (a, b) in [(bond.a, bond.b), (bond.b, bond.a)] {
                // QueryOps::isMetal, including the native dummy-atom exclusion.
                if !matches!(at(&graph.atoms, b)?.atomic_number,
                    0..=2 | 5..=10 | 14..=18 | 33..=36 | 52..=54 | 85..=86)
                {
                    *metal_neighbors.get_mut(a).ok_or_else(invalid_index)? = true;
                }
            }
        }
        Ok(Self {
            graph,
            metadata,
            valences,
            metal_neighbors,
        })
    }

    pub fn atom(
        &self,
        index: usize,
        options: Options,
        properties: AtomProperties<'_>,
    ) -> Result<String, Error> {
        if properties
            .custom_symbol
            .map_or(0, str::len)
            .checked_add(properties.supplement.map_or(0, str::len))
            .is_none_or(|length| length > 1024 * 1024)
        {
            return Err(Error::Limit);
        }
        let atom = at(&self.graph.atoms, index)?;
        let meta = at(&self.metadata.atoms, index)?;
        let valence = at(&self.valences, index)?;
        let element = at(ELEMENTS, usize::from(atom.atomic_number))?;
        let hydrogens = u32::from(atom.explicit_hydrogens) + valence.implicit_hydrogens;
        let winding = if options.isomeric && !properties.broken_chirality {
            match meta.chiral_tag {
                1 => "@@".to_owned(),
                2 => "@".to_owned(),
                6..=8 if options.non_tetrahedral => {
                    let (prefix, maximum) = match meta.chiral_tag {
                        6 => ("@SP", 3),
                        7 => ("@TB", 20),
                        _ => ("@OH", 30),
                    };
                    let permutation = meta.chiral_permutation.unwrap_or(0);
                    if permutation > maximum {
                        return Err(Error::Chirality(index));
                    }
                    let mut text = prefix.to_owned();
                    if permutation > 0 {
                        text.push_str(&permutation.to_string());
                    }
                    text
                }
                _ => String::new(),
            }
        } else {
            String::new()
        };
        let map_present = meta.map_present || meta.map_number != 0;
        let brackets = properties.custom_symbol.is_some()
            || options.all_hydrogens
            || !matches!(atom.atomic_number, 0 | 5..=9 | 15..=17 | 35 | 53)
            || atom.charge != 0
            || options.isomeric && (atom.isotope != 0 || !winding.is_empty())
            || map_present
            || atom.radical_electrons != 0
            || matches!(atom.atomic_number, 7 | 15)
                && atom.aromatic
                && atom.explicit_hydrogens != 0
            || hydrogens != 0
                && i64::from(valence.explicit_valence + valence.implicit_hydrogens)
                    != i64::from(*element.valences.first().ok_or_else(invalid_index)?)
            || *at(&self.metal_neighbors, index)?;
        let mut output = String::new();
        if brackets {
            output.push('[');
        }
        if options.isomeric && atom.isotope != 0 {
            output.push_str(&atom.isotope.to_string());
        }
        let symbol = properties.custom_symbol.unwrap_or(element.symbol);
        if !options.kekule
            && atom.aromatic
            && matches!(atom.atomic_number, 5..=8 | 14..=16 | 33..=34 | 52)
            && let Some(&first) = symbol.as_bytes().first().filter(|b| b.is_ascii_uppercase())
        {
            output.push(char::from(first.to_ascii_lowercase()));
            output.push_str(symbol.get(1..).ok_or_else(invalid_index)?);
        } else {
            output.push_str(symbol);
        }
        output.push_str(&winding);
        if brackets {
            if hydrogens > 0 {
                output.push('H');
                if hydrogens > 1 {
                    output.push_str(&hydrogens.to_string());
                }
            }
            match atom.charge {
                1 => output.push('+'),
                -1 => output.push('-'),
                2.. => {
                    output.push('+');
                    output.push_str(&atom.charge.to_string());
                }
                ..=-2 => output.push_str(&atom.charge.to_string()),
                _ => (),
            }
            if map_present {
                output.push(':');
                output.push_str(&meta.map_number.to_string());
            }
            output.push(']');
        }
        if let Some(supplement) = properties.supplement {
            output.push_str(supplement);
        }
        Ok(output)
    }

    pub fn bond(
        &self,
        index: usize,
        direction: Direction,
        left: usize,
        options: Options,
    ) -> Result<&'static str, Error> {
        let bond = at(&self.graph.bonds, index)?;
        if left != bond.a && left != bond.b {
            return Err(Error::Invalid(
                "Bond traversal endpoint is not attached".into(),
            ));
        }
        let a = at(&self.graph.atoms, bond.a)?;
        let b = at(&self.graph.atoms, bond.b)?;
        let aromatic = !options.kekule
            && matches!(bond.order, 1 | 2 | 4)
            && a.aromatic
            && b.aromatic
            && (a.atomic_number != 0 || b.atomic_number != 0);
        let marked = !matches!(direction, Direction::None | Direction::Unknown);
        let directional = matches!(direction, Direction::Up | Direction::Down);
        Ok(match bond.order {
            1 | 4 if directional => {
                if options.all_bonds || options.isomeric {
                    if direction == Direction::Up {
                        "/"
                    } else {
                        "\\"
                    }
                } else {
                    ""
                }
            }
            1 if options.all_bonds || !marked && aromatic && !bond.aromatic => "-",
            1 => "",
            2 if !aromatic || !bond.aromatic || options.all_bonds => "=",
            2 => "",
            3 => "#",
            4 if options.all_bonds || !aromatic => ":",
            4 => "",
            5 if left == bond.a => "->",
            5 => "<-",
            6 => "$",
            _ => "~",
        })
    }
}

fn invalid_index() -> Error {
    Error::Invalid("Missing symbol input".into())
}
fn at<T>(values: &[T], index: usize) -> Result<&T, Error> {
    values.get(index).ok_or_else(invalid_index)
}
