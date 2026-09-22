//! Stable drawing IDs from engine/abbreviations_exchange.py::read.
use super::{
    Abbreviation, Error, Fragment, Result,
    abbreviations::{position, python_number},
    tree::Tree,
};
use crate::{abbreviations::Abbreviation as DrawingAbbreviation, chemistry::stereo::Point3};
use std::collections::HashMap;

const TOLERANCE: f64 = 0.02;
const ASSOCIATION: &str = "Could not safely associate abbreviation atoms";

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
    // At magnitudes beyond this integer grid, adjacent floating values are
    // farther apart than the matching tolerance. Only exact values can match.
    if bin >= i64::MIN as f64 + 4096. && bin <= i64::MAX as f64 - 4096. {
        Some(Cell::Grid(bin as i64))
    } else {
        Some(Cell::Exact(value.to_bits()))
    }
}
fn neighbors(cell: Cell) -> Vec<Cell> {
    match cell {
        // A second cell covers rounding of division at a cell boundary. Exact
        // distance comparisons below retain the original strict inequality.
        Cell::Grid(bin) => (-2..=2)
            .filter_map(|delta| bin.checked_add(delta).map(Cell::Grid))
            .collect(),
        Cell::Exact(_) => vec![cell],
    }
}
struct Candidate {
    id: u64,
    x: f64,
    y: f64,
}
type Index = HashMap<(u8, Cell, Cell), Vec<Candidate>>;

/// Associate expanded source records with the combined molecule's drawing IDs.
///
/// `positions` must follow dense atom order across `fragments`, AFTER the
/// importer's conformer scaling. `source_scale` converts source XML coordinates
/// to drawing units. This function applies the original separate factor of 28,
/// Y inversion, element test, and strict 0.02-unit uniqueness test. It never
/// trusts source XML IDs as molecular IDs or prepares/sanitizes the molecule.
///
/// Repeated source IDs are cached. A bounded spatial lookup handles distinct
/// IDs without a quadratic atom scan. Fragment/position dimensions, finite
/// position/scale values, record storage, and XML/work limits are checked.
pub fn read_abbreviations(
    records: &[Abbreviation],
    expanded_xml: &str,
    fragments: &[Fragment],
    positions: &[Point3],
    source_scale: f64,
) -> Result<Vec<DrawingAbbreviation>> {
    let tree = Tree::parse(expanded_xml)?;
    if records.len() > 100_000 || fragments.len() > 100_000 {
        return Err(Error::Limit);
    }
    let atoms = fragments.iter().try_fold(0usize, |count, fragment| {
        count
            .checked_add(fragment.graph.atoms.len())
            .ok_or(Error::Limit)
    })?;
    if atoms > 100_000 {
        return Err(Error::Limit);
    }
    if atoms != positions.len() {
        return Err(Error::Invalid(
            "Abbreviation molecule/position dimensions differ".into(),
        ));
    }
    if !source_scale.is_finite()
        || positions
            .iter()
            .any(|p| !p.x.is_finite() || !p.y.is_finite() || !p.z.is_finite())
    {
        return Err(Error::Invalid(
            "Nonfinite abbreviation association coordinates".into(),
        ));
    }
    let mut members = 0usize;
    let mut bytes = 0usize;
    for record in records {
        members = members
            .checked_add(record.members.len())
            .ok_or(Error::Limit)?;
        bytes = bytes
            .checked_add(record.label.len())
            .and_then(|n| n.checked_add(record.reverse_label.len()))
            .ok_or(Error::Limit)?;
        if members > 1_000_000 || bytes > 16 * 1024 * 1024 {
            return Err(Error::Limit);
        }
        for id in std::iter::once(&record.anchor).chain(&record.members) {
            bytes = bytes
                .checked_add(id.as_ref().map_or(0, String::len))
                .ok_or(Error::Limit)?;
            if bytes > 16 * 1024 * 1024 {
                return Err(Error::Limit);
            }
        }
    }
    let mut nodes = HashMap::new();
    for index in tree.descendants(0)? {
        let node = tree.node(index)?;
        if node.tag == "n" {
            // The original dictionary deliberately uses the last occurrence,
            // including an absent identifier, across the complete source tree.
            nodes.insert(node.attr("id").map(str::to_owned), index);
        }
    }
    let mut index = Index::new();
    for (i, (atom, point)) in fragments
        .iter()
        .flat_map(|part| &part.graph.atoms)
        .zip(positions)
        .enumerate()
    {
        if atom.atomic_number > 118 {
            return Err(Error::Invalid("Invalid abbreviation atom element".into()));
        }
        let (x, y) = (point.x * 28., -point.y * 28.);
        if let Some((cx, cy)) = cell(x).zip(cell(y)) {
            let id = u64::try_from(i)
                .map_err(|_| Error::Limit)?
                .checked_add(1)
                .ok_or(Error::Limit)?;
            index
                .entry((atom.atomic_number, cx, cy))
                .or_default()
                .push(Candidate { id, x, y });
        }
    }
    let mut cache = HashMap::new();
    let mut identify = |id: &Option<String>| -> Result<u64> {
        if let Some(&id) = cache.get(id) {
            return Ok(id);
        }
        let node = *nodes
            .get(id)
            .ok_or_else(|| Error::Invalid("Missing abbreviation source atom".into()))?;
        let [x, y] = position(&tree, node, "Invalid abbreviation atom position")?;
        if atoms == 0 {
            return Err(Error::Invalid(ASSOCIATION.into()));
        }
        let element = element(tree.node(node)?.attr("Element").unwrap_or("6"))?
            .ok_or_else(|| Error::Invalid(ASSOCIATION.into()))?;
        let (x, y) = (x * source_scale, y * source_scale);
        let (cx, cy) = cell(x)
            .zip(cell(y))
            .ok_or_else(|| Error::Invalid(ASSOCIATION.into()))?;
        let mut matched = None;
        for cx in neighbors(cx) {
            for cy in neighbors(cy) {
                tree.spend(1)?;
                for candidate in index.get(&(element, cx, cy)).into_iter().flatten() {
                    tree.spend(1)?;
                    if (candidate.x - x).abs() < TOLERANCE
                        && (candidate.y - y).abs() < TOLERANCE
                        && matched.replace(candidate.id).is_some()
                    {
                        return Err(Error::Invalid(ASSOCIATION.into()));
                    }
                }
            }
        }
        let matched = matched.ok_or_else(|| Error::Invalid(ASSOCIATION.into()))?;
        cache.insert(id.clone(), matched);
        Ok(matched)
    };
    let mut result = Vec::with_capacity(records.len());
    for record in records {
        let anchor = identify(&record.anchor)?;
        let members = record
            .members
            .iter()
            .map(&mut identify)
            .collect::<Result<Vec<_>>>()?;
        result.push(DrawingAbbreviation {
            label: record.label.clone(),
            reverse_label: record.reverse_label.clone(),
            anchor,
            members,
        });
    }
    Ok(result)
}

fn element(text: &str) -> Result<Option<u8>> {
    let number = python_number(text.trim())
        .ok_or_else(|| Error::Invalid("Invalid abbreviation atom element".into()))?;
    let digits = number.strip_prefix(['+', '-']).unwrap_or(&number);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(Error::Invalid("Invalid abbreviation atom element".into()));
    }
    Ok(number
        .parse::<i64>()
        .ok()
        .and_then(|n| u8::try_from(n).ok())
        .filter(|&n| n <= 118))
}
