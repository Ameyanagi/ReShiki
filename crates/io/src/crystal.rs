//! Crystal and periodic structures (#417): CIF, PDB, VASP POSCAR/CONTCAR and
//! plain or extended XYZ, read into a 2D drawing in its unit cell, or a 3D
//! model when there is no cell.
//!
//! Each reader keeps only what the structure needs: the lattice, each atom's
//! element, position, charge and label, and from CIF its displacement
//! parameters. Everything else in the file is read past and dropped, so a
//! saved drawing never holds the source text.
use crate::document::Document;
use crate::models3d::Adp;

mod build;
mod cif;
mod elements;
mod pdb;
mod poscar;
mod xyz;

/// The most atoms a structure may have in its cell.
pub const MAX_ATOMS: usize = 2_000;

/// Why a structure file could not be read, with its line where that helps.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct Error(String);

impl Error {
    fn new(message: &str) -> Self {
        Self(message.to_owned())
    }
    fn at(line: usize, message: &str) -> Self {
        Self(format!("Line {line}: {message}"))
    }
}

type V3 = [f64; 3];

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn scale(a: V3, s: f64) -> V3 {
    a.map(|v| v * s)
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn length(a: V3) -> f64 {
    dot(a, a).sqrt()
}

/// One atom as a reader keeps it, at a Cartesian position in ångström.
#[derive(Debug, Clone, PartialEq)]
struct Atom {
    element: String,
    charge: i32,
    label: String,
    xyz: V3,
    adp: Option<Adp>,
    /// A listed CIF input site, before symmetry expansion.
    asymmetric: bool,
}

/// What a reader keeps of a file.
struct Structure {
    lattice: Option<build::Lattice>,
    atoms: Vec<Atom>,
    warnings: Vec<String>,
}

/// The crystal import format of a file name: `cif`, `pdb`, `poscar` (also
/// `*.vasp` and files named POSCAR… or CONTCAR…) or `xyz` (also `*.extxyz`).
pub fn format_of(name: &str) -> Option<&'static str> {
    let lower = name.to_ascii_lowercase();
    let extension = lower.rsplit_once('.').map(|(_, e)| e).unwrap_or_default();
    extension_format(extension).or_else(|| {
        (lower.starts_with("poscar") || lower.starts_with("contcar")).then_some("poscar")
    })
}

/// The crystal import format of a file extension, ignoring ASCII case.
pub fn extension_format(extension: &str) -> Option<&'static str> {
    match extension.to_ascii_lowercase().as_str() {
        "cif" => Some("cif"),
        "pdb" | "ent" => Some("pdb"),
        "vasp" | "poscar" | "contcar" => Some("poscar"),
        "xyz" | "extxyz" => Some("xyz"),
        _ => None,
    }
}

/// Whether `format` is read by [`read`].
pub fn is_crystal_format(format: &str) -> bool {
    matches!(format, "cif" | "pdb" | "poscar" | "xyz")
}

/// Read `text` as `format` (see [`is_crystal_format`]) into a 2D drawing in its
/// unit cell, or a 3D model when there is no cell, with import warnings.
pub fn read(format: &str, text: &str) -> Result<(Document, Vec<String>), Error> {
    let mut structure = match format {
        "cif" => cif::read(text)?,
        "pdb" => pdb::read(text)?,
        "poscar" => poscar::read(text)?,
        "xyz" => xyz::read(text)?,
        _ => return Err(Error::new("Unsupported crystal format")),
    };
    if structure.atoms.is_empty() {
        return Err(Error::new("The file has no atoms"));
    }
    if let Some(lattice) = &structure.lattice {
        for atom in &mut structure.atoms {
            let f = lattice.fractional(atom.xyz).map(build::wrap);
            atom.xyz = lattice.cartesian(f);
        }
    }
    if structure.atoms.len() > MAX_ATOMS {
        return Err(Error(format!(
            "The structure has {} atoms; at most {MAX_ATOMS} can be shown",
            structure.atoms.len()
        )));
    }
    let removed = build::deduplicate(&mut structure.atoms, structure.lattice.as_ref());
    if removed > 0 {
        structure.warnings.push(format!(
            "{removed} overlapping atom{} left out",
            if removed == 1 { "" } else { "s" }
        ));
    }
    let doc = build::document(structure)?;
    let warnings = doc
        .models
        .iter()
        .flat_map(|m| m.notes.iter().cloned())
        .collect();
    Ok((doc, warnings))
}

/// A number as structure files write it, without a standard uncertainty in
/// parentheses, such as `5.6402(3)`.
fn number(text: &str) -> Option<f64> {
    let text = text.split('(').next().unwrap_or(text).trim();
    text.parse::<f64>().ok().filter(|v| v.is_finite())
}

#[cfg(test)]
mod tests;
