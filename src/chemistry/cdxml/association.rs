//! Shared bounded association of source nodes with prepared, scaled atoms.
use super::{Error, Result, abbreviations::python_number, tree::Tree};
use crate::chemistry::{graph::Atom, stereo::Point3};
use serde::Serialize;
use std::collections::HashMap;

const TOLERANCE: f64 = 0.02;

/// Drawing-space coordinates kept at the original reader's f64 precision.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct ImportPoint {
    pub x: f64,
    pub y: f64,
}

/// An element's identity is its zero-based ordinal among XML elements in
/// document order, including the CDXML root. IDs may be absent or duplicated.
/// Apply updates only to the same, unchanged XML document used by the reader.
#[derive(Clone, Debug, Serialize)]
pub struct ObjectMapEntry {
    pub source: usize,
    pub atoms: Vec<u64>,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
enum Cell {
    Grid(i64),
    Exact(u64),
}
fn cell(value: f64) -> Option<Cell> {
    if !value.is_finite() {
        return None;
    }
    let bin = (value / TOLERANCE).floor();
    if bin >= i64::MIN as f64 + 4096. && bin <= i64::MAX as f64 - 4096. {
        Some(Cell::Grid(bin as i64))
    } else {
        Some(Cell::Exact(value.to_bits()))
    }
}
fn neighbors(cell: Cell) -> Vec<Cell> {
    match cell {
        Cell::Grid(bin) => (-2..=2)
            .filter_map(|d| bin.checked_add(d).map(Cell::Grid))
            .collect(),
        Cell::Exact(_) => vec![cell],
    }
}
struct Candidate {
    index: usize,
    x: f64,
    y: f64,
}

/// One reusable association index for label and mark readers. Atoms must come
/// from the prepared combined molecule, including its final charge/radical
/// facts. Positions must follow the same dense order AFTER conformer scaling.
/// Stable drawing IDs are dense indices plus one. This does not prepare atoms.
pub struct PreparedAtoms<'a> {
    atoms: &'a [Atom],
    index: HashMap<(u8, Cell, Cell), Vec<Candidate>>,
}
impl<'a> PreparedAtoms<'a> {
    pub fn new(atoms: &'a [Atom], positions: &[Point3]) -> Result<Self> {
        if atoms.len() > 100_000 || positions.len() > 100_000 {
            return Err(Error::Limit);
        }
        if atoms.len() != positions.len() {
            return Err(invalid("Prepared atom/position dimensions differ"));
        }
        let mut index: HashMap<_, Vec<_>> = HashMap::new();
        for (i, (atom, point)) in atoms.iter().zip(positions).enumerate() {
            if atom.atomic_number > 118 {
                return Err(invalid("Invalid prepared atom element"));
            }
            if !point.x.is_finite() || !point.y.is_finite() || !point.z.is_finite() {
                return Err(invalid("Nonfinite prepared atom coordinates"));
            }
            let (x, y) = (point.x * 28., -point.y * 28.);
            if let Some((cx, cy)) = cell(x).zip(cell(y)) {
                index
                    .entry((atom.atomic_number, cx, cy))
                    .or_default()
                    .push(Candidate { index: i, x, y });
            }
        }
        Ok(Self { atoms, index })
    }
    pub(super) fn atom(&self, id: u64) -> Result<&Atom> {
        id.checked_sub(1)
            .and_then(|n| usize::try_from(n).ok())
            .and_then(|i| self.atoms.get(i))
            .ok_or_else(|| invalid("Missing prepared atom"))
    }
    pub(super) fn identify(
        &self,
        tree: &Tree,
        node: usize,
        point: ImportPoint,
        failure: &str,
    ) -> Result<u64> {
        // Native loops do not parse Element when the molecule has no atoms.
        if self.atoms.is_empty() {
            return Err(invalid(failure));
        }
        let element = element(tree.node(node)?.attr("Element").unwrap_or("6"))?
            .ok_or_else(|| invalid(failure))?;
        let (cx, cy) = cell(point.x)
            .zip(cell(point.y))
            .ok_or_else(|| invalid(failure))?;
        let mut matched = None;
        for cx in neighbors(cx) {
            for cy in neighbors(cy) {
                tree.spend(1)?;
                for candidate in self.index.get(&(element, cx, cy)).into_iter().flatten() {
                    tree.spend(1)?;
                    if (candidate.x - point.x).abs() < TOLERANCE
                        && (candidate.y - point.y).abs() < TOLERANCE
                        && matched.replace(candidate.index).is_some()
                    {
                        return Err(invalid(failure));
                    }
                }
            }
        }
        u64::try_from(matched.ok_or_else(|| invalid(failure))?)
            .map_err(|_| Error::Limit)?
            .checked_add(1)
            .ok_or(Error::Limit)
    }
}
pub(super) fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}
pub(super) fn scale(scale: f64) -> Result<()> {
    if !scale.is_finite() {
        return Err(invalid("Nonfinite source coordinate scale"));
    }
    Ok(())
}
pub(super) fn numbers(text: &str, failure: &str) -> Result<Vec<f64>> {
    text.split_whitespace()
        .map(|v| {
            python_number(v)
                .and_then(|v| v.parse().ok())
                .ok_or_else(|| invalid(failure))
        })
        .collect()
}
fn element(text: &str) -> Result<Option<u8>> {
    let number =
        python_number(text.trim()).ok_or_else(|| invalid("Invalid source atom element"))?;
    let digits = number.strip_prefix(['+', '-']).unwrap_or(&number);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(invalid("Invalid source atom element"));
    }
    Ok(number
        .parse::<i64>()
        .ok()
        .and_then(|n| u8::try_from(n).ok())
        .filter(|&n| n <= 118))
}
