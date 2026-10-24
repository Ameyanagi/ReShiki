//! PDB: the CRYST1 cell and ATOM/HETATM records of the first model, for small
//! molecules and ligands. Alternate locations keep the first; other records
//! are read past.
use super::build::Lattice;
use super::{Atom, Error, Structure, elements, number};
use crate::models3d::Adp;
use std::collections::HashMap;

/// Columns `from..=to` (1-based, as the PDB format lists them), trimmed.
fn columns(line: &str, from: usize, to: usize) -> &str {
    let start = from.saturating_sub(1).min(line.len());
    let end = to.min(line.len());
    line.get(start..end).unwrap_or_default().trim()
}

const EIGHT_PI_SQUARED: f64 = 8. * std::f64::consts::PI * std::f64::consts::PI;

pub(super) fn read(text: &str) -> Result<Structure, Error> {
    let mut lattice = None;
    let mut atoms = vec![];
    let mut warnings = vec![];
    let mut first_alternative: HashMap<(String, String, String), char> = HashMap::new();
    let mut alternatives = 0;
    for (index, line) in text.lines().enumerate() {
        let line_number = index + 1;
        let record = columns(line, 1, 6);
        match record {
            "CRYST1" => {
                let field = |from, to, name: &str| {
                    number(columns(line, from, to))
                        .ok_or_else(|| Error::at(line_number, &format!("CRYST1 has no {name}")))
                };
                let lengths = [field(7, 15, "a")?, field(16, 24, "b")?, field(25, 33, "c")?];
                let angles = [
                    field(34, 40, "alpha")?,
                    field(41, 47, "beta")?,
                    field(48, 54, "gamma")?,
                ];
                // A 1 Å cube marks a structure without a cell, as from NMR.
                if lengths != [1.; 3] {
                    lattice = Some(
                        Lattice::from_parameters(lengths, angles)
                            .map_err(|e| Error::at(line_number, &e.to_string()))?,
                    );
                }
            }
            "ATOM" | "HETATM" => {
                // The first alternate location of each residue.
                let alternative = line.chars().nth(16).filter(|c| *c != ' ');
                if let Some(alt) = alternative {
                    let residue = (
                        columns(line, 22, 22).to_owned(),
                        columns(line, 23, 26).to_owned(),
                        columns(line, 27, 27).to_owned(),
                    );
                    let first = *first_alternative.entry(residue).or_insert(alt);
                    if alt != first {
                        alternatives += 1;
                        continue;
                    }
                }
                let coordinate = |from, to| {
                    number(columns(line, from, to)).ok_or_else(|| {
                        Error::at(line_number, "The atom's coordinates are not numbers")
                    })
                };
                let xyz = [
                    coordinate(31, 38)?,
                    coordinate(39, 46)?,
                    coordinate(47, 54)?,
                ];
                let name = columns(line, 13, 16);
                let symbol = columns(line, 77, 78);
                let element = if symbol.is_empty() {
                    // Without an element column, the name's first two columns
                    // hold it right-justified: " CA " is carbon, "CA  " calcium.
                    elements::from_symbol(
                        columns(line, 13, 14).trim_start_matches(char::is_numeric),
                    )
                    .map(|(e, _)| e)
                } else {
                    elements::from_symbol(symbol).map(|(e, _)| e)
                }
                .ok_or_else(|| {
                    Error::at(
                        line_number,
                        &format!("Cannot tell the element of atom '{name}'"),
                    )
                })?;
                let charge = charge(columns(line, 79, 80));
                let adp = number(columns(line, 61, 66))
                    .filter(|b| *b > 0.)
                    .map(|b| Adp::Iso((b / EIGHT_PI_SQUARED) as f32));
                atoms.push(Atom {
                    element,
                    charge,
                    label: name.to_owned(),
                    xyz,
                    adp,
                });
            }
            // The first model only.
            "ENDMDL" => break,
            _ => {}
        }
    }
    if atoms.is_empty() {
        return Err(Error::new("The PDB file has no ATOM or HETATM records"));
    }
    if alternatives > 0 {
        warnings.push(format!(
            "Alternate locations: kept the first of each residue, left out {alternatives} atom{}",
            if alternatives == 1 { "" } else { "s" }
        ));
    }
    Ok(Structure {
        lattice,
        atoms,
        warnings,
    })
}

/// A PDB charge such as `2+` or `1-`.
fn charge(text: &str) -> i32 {
    let digits: String = text.chars().filter(char::is_ascii_digit).collect();
    let size = digits.parse::<i32>().unwrap_or(1).min(9);
    if text.contains('-') {
        -size
    } else if text.contains('+') {
        size
    } else {
        0
    }
}
