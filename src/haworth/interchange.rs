//! Recognize the tested five/six-member Haworth convention at file boundaries.
//! A bold front edge, two wedges pointing toward it, a matching affine ring,
//! and unambiguous up/down substituents are required. Ordinary wedges alone
//! never authorize an inferred carbohydrate configuration.
use crate::document::{AtomStereo, Document, Point};
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub(crate) struct Recognized {
    pub bonds: HashSet<usize>,
    fronts: HashSet<usize>,
    pub atoms: HashSet<u64>,
    pub stereo: HashMap<u64, AtomStereo>,
}

fn pair(a: u64, b: u64) -> (u64, u64) {
    (a.min(b), a.max(b))
}
fn sub(a: Point, b: Point) -> Point {
    Point::new(a.x - b.x, a.y - b.y)
}
fn mul(a: Point, f: f32) -> Point {
    Point::new(a.x * f, a.y * f)
}
fn cross(a: Point, b: Point) -> f32 {
    a.x * b.y - a.y * b.x
}
fn length(a: Point) -> f32 {
    a.x.hypot(a.y)
}

pub(crate) fn recognize(doc: &Document) -> Recognized {
    recognize_with(doc, false)
}

fn recognize_with(doc: &Document, mol: bool) -> Recognized {
    let positions: HashMap<_, _> = doc.atoms.iter().map(|a| (a.id, a)).collect();
    let mut adjacent: HashMap<u64, Vec<(u64, usize)>> = HashMap::new();
    let mut edges = HashMap::new();
    for (index, b) in doc.bonds.iter().enumerate() {
        adjacent.entry(b.a).or_default().push((b.b, index));
        adjacent.entry(b.b).or_default().push((b.a, index));
        edges.insert(pair(b.a, b.b), index);
    }
    let mut result = Recognized::default();
    let mut work = 200_000usize;
    for (front_index, front) in doc
        .bonds
        .iter()
        .enumerate()
        .filter(|(_, b)| b.order == 1 && (b.display == "bold" || mol && b.display == "wedge"))
    {
        if [front.a, front.b]
            .iter()
            .any(|id| adjacent.get(id).is_none_or(|n| n.len() > 4))
        {
            continue;
        }
        let tips = |id| -> Vec<u64> {
            adjacent
                .get(&id)
                .into_iter()
                .flatten()
                .filter_map(|&(other, i)| {
                    let b = doc.bonds.get(i)?;
                    (i != front_index && b.order == 1 && b.display == "wedge" && b.b == id)
                        .then_some(other)
                })
                .collect()
        };
        let (right, left) = (tips(front.a), tips(front.b));
        let ([right], [left]) = (right.as_slice(), left.as_slice()) else {
            continue;
        };
        let prefix = vec![*right, front.a, front.b, *left];
        let mut stack = vec![prefix];
        while let Some(path) = stack.pop() {
            let Some(remaining) = work.checked_sub(1) else {
                return Recognized::default();
            };
            work = remaining;
            let Some(&last) = path.last() else { continue };
            let Some(neighbors) = adjacent.get(&last).filter(|v| v.len() <= 4) else {
                continue;
            };
            for &(next, index) in neighbors {
                let Some(bond) = doc.bonds.get(index) else {
                    continue;
                };
                if bond.order != 1 || bond.display != "plain" {
                    continue;
                }
                if next == *right && matches!(path.len(), 5 | 6) {
                    if let Some(stereo) = assignments(&path, &positions, &adjacent, doc) {
                        // Fused/overlapping perspective rings need a separate
                        // convention; do not mix independent inferred planes.
                        if path.iter().any(|id| result.atoms.contains(id)) {
                            continue;
                        }
                        let indices: Option<Vec<_>> = path
                            .iter()
                            .zip(path.iter().cycle().skip(1))
                            .take(path.len())
                            .map(|(&a, &b)| edges.get(&pair(a, b)).copied())
                            .collect();
                        if let Some(indices) = indices {
                            result.bonds.extend(indices);
                            result.fronts.insert(front_index);
                            result.atoms.extend(path.iter().copied());
                            result.stereo.extend(stereo);
                        }
                    }
                } else if path.len() < 6 && !path.contains(&next) {
                    let mut extended = path.clone();
                    extended.push(next);
                    stack.push(extended);
                }
            }
        }
    }
    result
}

fn assignments(
    ring: &[u64],
    atoms: &HashMap<u64, &crate::document::Atom>,
    adjacent: &HashMap<u64, Vec<(u64, usize)>>,
    doc: &Document,
) -> Option<HashMap<u64, AtomStereo>> {
    let reference = match ring.len() {
        5 => &[(1., 0.), (0.65, 0.5), (-0.65, 0.5), (-1., 0.), (0., -0.6)][..],
        6 => &[
            (1., 0.),
            (0.5, 0.5),
            (-0.5, 0.5),
            (-1., 0.),
            (-0.5, -0.5),
            (0.5, -0.5),
        ][..],
        _ => return None,
    };
    let p = |i: usize| Some(atoms.get(ring.get(i)?)?.position);
    let half = reference.get(1)?.0;
    let x = mul(sub(p(1)?, p(2)?), 1. / (2. * half));
    let y = mul(sub(sub(p(0)?, p(1)?), mul(x, 1. - half)), -2.);
    let origin = sub(sub(p(1)?, mul(x, half)), mul(y, 0.5));
    let determinant = cross(x, y);
    let tolerance = (length(x) + length(y)) * 0.01;
    if !determinant.is_finite() || determinant.abs() <= length(x) * length(y) * 0.05 {
        return None;
    }
    let mut result = HashMap::new();
    for (i, (&id, &(rx, ry))) in ring.iter().zip(reference).enumerate() {
        let atom = atoms.get(&id)?;
        if !matches!(atom.element.as_str(), "C" | "O")
            || atom.aromatic
            || atom.charge != 0
            || atom.radical_electrons != 0
            || atom.attachment.is_some()
        {
            return None;
        }
        let expected = Point::new(
            origin.x + rx * x.x + ry * y.x,
            origin.y + rx * x.y + ry * y.y,
        );
        if length(sub(atom.position, expected)) > tolerance {
            return None;
        }
        let neighbors = adjacent.get(&id)?;
        let branches: Vec<_> = neighbors
            .iter()
            .filter(|(id, _)| !ring.contains(id))
            .collect();
        if branches.is_empty() {
            continue;
        }
        if atom.element != "C" || branches.len() > 2 {
            return None;
        }
        let mut above = Vec::new();
        let mut ordered = vec![
            *ring.get((i + ring.len() - 1) % ring.len())?,
            *ring.get((i + 1) % ring.len())?,
        ];
        for &&(other, index) in &branches {
            let bond = doc.bonds.get(index)?;
            if bond.order != 1 || bond.display != "plain" {
                return None;
            }
            let delta = sub(atoms.get(&other)?.position, atom.position);
            let dx = cross(delta, y) / determinant;
            let dy = cross(x, delta) / determinant;
            if !dx.is_finite() || !dy.is_finite() || dy.abs() < 0.05 || dx.abs() > dy.abs() * 0.05 {
                return None;
            }
            above.push(dy < 0.);
            ordered.push(other);
        }
        if above.len() == 2 && above.first() == above.last() {
            return None;
        }
        let ccw = *above.first()? ^ (determinant < 0.);
        result.insert(
            id,
            AtomStereo {
                winding: if ccw { "ccw" } else { "cw" }.into(),
                neighbors: ordered,
            },
        );
    }
    Some(result)
}

fn equivalent(a: &AtomStereo, b: &AtomStereo) -> bool {
    if a.neighbors.len() != b.neighbors.len() {
        return false;
    }
    let permutation: Option<Vec<_>> = a
        .neighbors
        .iter()
        .map(|n| b.neighbors.iter().position(|m| m == n))
        .collect();
    let Some(permutation) = permutation else {
        return false;
    };
    let odd = permutation
        .iter()
        .enumerate()
        .map(|(i, x)| permutation.iter().skip(i + 1).filter(|y| x > *y).count())
        .sum::<usize>()
        % 2
        != 0;
    (a.winding == b.winding) != odd
}

/// Export is permitted only when the visible Haworth convention agrees with
/// explicitly stored stereochemistry. Never invent configurations on export.
pub(crate) fn export_bonds(doc: &Document) -> Result<HashSet<usize>, &'static str> {
    let recognized = recognize(doc);
    for atom in doc
        .atoms
        .iter()
        .filter(|a| recognized.atoms.contains(&a.id))
    {
        match (atom.stereo.as_ref(), recognized.stereo.get(&atom.id)) {
            (None, None) => (),
            (Some(a), Some(b)) if equivalent(a, b) => (),
            _ => {
                return Err(
                    "Haworth appearance conflicts with stored stereochemistry. Use a matching sugar template or save as ReShiki, SVG, PNG or PDF.",
                );
            }
        }
    }
    Ok(recognized.bonds)
}

/// Apply the established Haworth drawing convention after ordinary CDXML
/// reconstruction. Returns whether chemical state needs to be rebuilt.
pub(crate) fn restore(doc: &mut Document) -> bool {
    let recognized = recognize(doc);
    apply(doc, recognized)
}

/// ChemDraw MOL writes a bold front edge as a wedge. This entry point is used
/// only with retained original directions from a ChemDraw-authored 2D MOL.
pub(crate) fn restore_mol(doc: &mut Document) -> bool {
    let recognized = recognize_with(doc, true);
    for &index in &recognized.fronts {
        if let Some(bond) = doc.bonds.get_mut(index) {
            bond.display = "bold".into();
        }
    }
    apply(doc, recognized)
}

fn apply(doc: &mut Document, recognized: Recognized) -> bool {
    if recognized.bonds.is_empty() {
        return false;
    }
    for index in recognized.bonds {
        if let Some(bond) = doc.bonds.get_mut(index) {
            bond.projection = true;
        }
    }
    for atom in &mut doc.atoms {
        if recognized.atoms.contains(&atom.id) {
            atom.stereo = recognized.stereo.get(&atom.id).cloned();
            atom.cip_label = None;
        }
    }
    true
}
