use crate::document::{Document, Point};
use std::collections::{HashMap, HashSet};

pub const CLIPBOARD_PREFIX: &str = "MORUNO_DRAWING_V1\n";
pub const ELEMENTS: &[&str] = &[
    "H", "He", "Li", "Be", "B", "C", "N", "O", "F", "Ne", "Na", "Mg", "Al", "Si", "P", "S", "Cl",
    "Ar", "K", "Ca", "Sc", "Ti", "V", "Cr", "Mn", "Fe", "Co", "Ni", "Cu", "Zn", "Ga", "Ge", "As",
    "Se", "Br", "Kr", "Rb", "Sr", "Y", "Zr", "Nb", "Mo", "Tc", "Ru", "Rh", "Pd", "Ag", "Cd", "In",
    "Sn", "Sb", "Te", "I", "Xe", "Cs", "Ba", "La", "Ce", "Pr", "Nd", "Pm", "Sm", "Eu", "Gd", "Tb",
    "Dy", "Ho", "Er", "Tm", "Yb", "Lu", "Hf", "Ta", "W", "Re", "Os", "Ir", "Pt", "Au", "Hg", "Tl",
    "Pb", "Bi", "Po", "At", "Rn", "Fr", "Ra", "Ac", "Th", "Pa", "U", "Np", "Pu", "Am", "Cm", "Bk",
    "Cf", "Es", "Fm", "Md", "No", "Lr", "Rf", "Db", "Sg", "Bh", "Hs", "Mt", "Ds", "Rg", "Cn", "Nh",
    "Fl", "Mc", "Lv", "Ts", "Og",
];

#[derive(Debug, Clone, Copy)]
pub enum Transform {
    Rotate(f32),
    FlipHorizontal,
    FlipVertical,
}
#[derive(Debug, Clone, Copy)]
pub enum Arrange {
    AlignHorizontal,
    AlignVertical,
    DistributeHorizontal,
    DistributeVertical,
}

pub fn selection(doc: &Document, ids: &[u64]) -> Document {
    let mut part = doc.clone();
    let removed: Vec<_> = doc
        .all_ids()
        .into_iter()
        .filter(|id| !ids.contains(id))
        .collect();
    if !removed.is_empty() {
        part.delete(&removed);
    }
    part
}

pub fn append(doc: &mut Document, source: &Document, offset: Point) -> Vec<u64> {
    let first = doc.next_id();
    let mapping: HashMap<_, _> = source
        .all_ids()
        .into_iter()
        .enumerate()
        .map(|(i, id)| (id, first + i as u64))
        .collect();
    let mut part = source.clone();
    for a in &mut part.atoms {
        a.id = mapping[&a.id];
        a.position = a.position.offset(offset.x, offset.y);
        if let Some(s) = &mut a.stereo {
            for id in &mut s.neighbors {
                *id = mapping[id];
            }
        }
    }
    for b in &mut part.bonds {
        b.a = mapping[&b.a];
        b.b = mapping[&b.b];
        for id in &mut b.stereo_atoms {
            *id = mapping[id];
        }
    }
    for a in &mut part.annotations {
        a.id = mapping[&a.id];
        a.position = a.position.offset(offset.x, offset.y);
    }
    for a in &mut part.arrows {
        a.id = mapping[&a.id];
        a.start = a.start.offset(offset.x, offset.y);
        a.end = a.end.offset(offset.x, offset.y);
    }
    let ids = part.all_ids();
    doc.atoms.extend(part.atoms);
    doc.bonds.extend(part.bonds);
    doc.annotations.extend(part.annotations);
    doc.arrows.extend(part.arrows);
    ids
}

fn point_bounds(doc: &Document, ids: &[u64]) -> Option<(Point, Point)> {
    let points = doc
        .atoms
        .iter()
        .filter(|a| ids.contains(&a.id))
        .map(|a| a.position)
        .chain(
            doc.annotations
                .iter()
                .filter(|a| ids.contains(&a.id))
                .flat_map(|a| [a.position, a.position.offset(a.size().0, a.size().1)]),
        )
        .chain(
            doc.arrows
                .iter()
                .filter(|a| ids.contains(&a.id))
                .flat_map(|a| [a.start, a.end]),
        );
    points.fold(None, |bounds, p| {
        Some(match bounds {
            None => (p, p),
            Some((lo, hi)) => (
                Point::new(lo.x.min(p.x), lo.y.min(p.y)),
                Point::new(hi.x.max(p.x), hi.y.max(p.y)),
            ),
        })
    })
}
pub fn center(doc: &Document, ids: &[u64]) -> Point {
    point_bounds(doc, ids)
        .map(|(lo, hi)| Point::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0))
        .unwrap_or_default()
}

pub fn transform(doc: &mut Document, ids: &[u64], transform: Transform) {
    let center = center(doc, ids);
    let convert = |p: Point| {
        let x = p.x - center.x;
        let y = p.y - center.y;
        let (x, y) = match transform {
            Transform::Rotate(degrees) => {
                let (s, c) = degrees.to_radians().sin_cos();
                (x * c - y * s, x * s + y * c)
            }
            Transform::FlipHorizontal => (-x, y),
            Transform::FlipVertical => (x, -y),
        };
        center.offset(x, y)
    };
    let boundary: Vec<_> = doc
        .bonds
        .iter()
        .filter(|b| ids.contains(&b.a) != ids.contains(&b.b))
        .flat_map(|b| [b.a, b.b])
        .collect();
    if !boundary.is_empty() {
        doc.invalidate_chemistry(&boundary);
    }
    for a in &mut doc.atoms {
        if ids.contains(&a.id) {
            a.position = convert(a.position);
        }
    }
    for a in &mut doc.annotations {
        if ids.contains(&a.id) {
            a.position = convert(a.position);
        }
    }
    for a in &mut doc.arrows {
        if ids.contains(&a.id) {
            a.start = convert(a.start);
            a.end = convert(a.end);
        }
    }
    if !matches!(transform, Transform::Rotate(_)) {
        // Reflect the projection while preserving the molecule's stereochemistry.
        for b in &mut doc.bonds {
            if ids.contains(&b.a) && ids.contains(&b.b) {
                b.display = match b.display.as_str() {
                    "wedge" => "hash",
                    "hash" => "wedge",
                    other => other,
                }
                .into();
            }
        }
    }
}

/// Connected selected atoms move as one object during alignment/distribution.
pub fn groups(doc: &Document, ids: &[u64]) -> Vec<Vec<u64>> {
    let mut remaining: HashSet<_> = ids.iter().copied().collect();
    let mut result = vec![];
    for id in ids {
        if !remaining.remove(id) {
            continue;
        }
        let mut group = vec![*id];
        let mut i = 0;
        while i < group.len() {
            let current = group[i];
            for bond in &doc.bonds {
                let neighbor = if bond.a == current {
                    Some(bond.b)
                } else if bond.b == current {
                    Some(bond.a)
                } else {
                    None
                };
                if let Some(n) = neighbor
                    && remaining.remove(&n)
                {
                    group.push(n);
                }
            }
            i += 1;
        }
        result.push(group);
    }
    result
}
pub fn arrange(doc: &mut Document, ids: &[u64], action: Arrange) {
    let horizontal = matches!(
        action,
        Arrange::AlignHorizontal | Arrange::DistributeHorizontal
    );
    let distribute = matches!(
        action,
        Arrange::DistributeHorizontal | Arrange::DistributeVertical
    );
    let mut groups: Vec<_> = groups(doc, ids)
        .into_iter()
        .filter_map(|g| point_bounds(doc, &g).map(|b| (g, b)))
        .collect();
    if groups.len() < 2 {
        return;
    }
    let coordinate = |p: Point| if horizontal { p.x } else { p.y };
    groups.sort_by(|a, b| coordinate(a.1.0).total_cmp(&coordinate(b.1.0)));
    let lo = groups
        .iter()
        .map(|g| coordinate(g.1.0))
        .fold(f32::INFINITY, f32::min);
    let hi = groups
        .iter()
        .map(|g| coordinate(g.1.1))
        .fold(f32::NEG_INFINITY, f32::max);
    let size: f32 = groups
        .iter()
        .map(|g| coordinate(g.1.1) - coordinate(g.1.0))
        .sum();
    let gap = (hi - lo - size) / (groups.len() - 1) as f32;
    let mut target = lo;
    for (group, (min, max)) in groups {
        let delta = if distribute {
            target - coordinate(min)
        } else {
            (lo + hi - coordinate(min) - coordinate(max)) / 2.0
        };
        doc.translate(
            &group,
            if horizontal { delta } else { 0.0 },
            if horizontal { 0.0 } else { delta },
        );
        target += coordinate(max) - coordinate(min) + gap;
    }
}

pub fn nearest_bond(doc: &Document, p: Point, r: f32) -> Option<usize> {
    doc.bonds
        .iter()
        .enumerate()
        .filter_map(|(i, b)| {
            let a = doc.atom(b.a)?.position;
            let z = doc.atom(b.b)?.position;
            let dx = z.x - a.x;
            let dy = z.y - a.y;
            let len = dx * dx + dy * dy;
            let t = if len > 0.0 {
                ((p.x - a.x) * dx + (p.y - a.y) * dy) / len
            } else {
                0.0
            };
            let d = p.distance(a.offset(dx * t.clamp(0.0, 1.0), dy * t.clamp(0.0, 1.0)));
            (d < r).then_some((i, d))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|x| x.0)
}

pub fn ring(doc: &mut Document, p: Point, size: u8, aromatic: bool, radius: f32) -> Vec<u64> {
    let n = size.clamp(3, 8) as usize;
    let atom = doc.nearest(p, radius);
    let bond = if atom.is_none() {
        nearest_bond(doc, p, radius)
    } else {
        None
    };
    let mut ids = vec![];
    if let Some(index) = bond {
        let b = doc.bonds[index].clone();
        let a = doc.atom(b.a).unwrap().position;
        let z = doc.atom(b.b).unwrap().position;
        let positions = |sign: f32| {
            let mut points = vec![a, z];
            let mut vector = Point::new(z.x - a.x, z.y - a.y);
            let (s, c) = (sign * std::f32::consts::TAU / n as f32).sin_cos();
            for _ in 2..n {
                vector = Point::new(vector.x * c - vector.y * s, vector.x * s + vector.y * c);
                points.push(points.last().unwrap().offset(vector.x, vector.y));
            }
            points
        };
        let score = |points: &[Point]| {
            points[2..]
                .iter()
                .map(|p| {
                    doc.atoms
                        .iter()
                        .map(|a| 1.0 / (p.distance(a.position) + 1.0).powi(2))
                        .sum::<f32>()
                })
                .sum::<f32>()
        };
        let first = positions(1.0);
        let second = positions(-1.0);
        let points = if score(&first) <= score(&second) {
            first
        } else {
            second
        };
        ids.extend([b.a, b.b]);
        for p in &points[2..] {
            ids.push(doc.add_atom("C", *p));
        }
    } else {
        let r = 42.0 / (2.0 * (std::f32::consts::PI / n as f32).sin());
        let center = atom
            .and_then(|id| doc.atom(id).map(|a| a.position.offset(-r, 0.0)))
            .unwrap_or(p);
        for i in 0..n {
            let angle = i as f32 * std::f32::consts::TAU / n as f32;
            ids.push(if let Some(id) = atom.filter(|_| i == 0) {
                id
            } else {
                doc.add_atom("C", center.offset(angle.cos() * r, angle.sin() * r))
            });
        }
    }
    for i in 0..n {
        if i == 0 && bond.is_some() && !aromatic {
            continue;
        }
        doc.add_bond(
            ids[i],
            ids[(i + 1) % n],
            if aromatic { 4 } else { 1 },
            "plain",
        );
    }
    if aromatic {
        for id in &ids {
            doc.atom_mut(*id).unwrap().aromatic = true;
        }
    }
    ids
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fused_ring_reuses_shared_atoms_and_chooses_free_side() {
        let mut d = Document::default();
        let ids = ring(&mut d, Point::default(), 6, false, 5.0);
        let a = d.atom(ids[0]).unwrap().position;
        let b = d.atom(ids[1]).unwrap().position;
        ring(
            &mut d,
            Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0),
            6,
            false,
            5.0,
        );
        assert_eq!((d.atoms.len(), d.bonds.len()), (10, 11));
        assert!(d.validate().is_ok());
        for (i, a) in d.atoms.iter().enumerate() {
            for b in &d.atoms[i + 1..] {
                assert!(a.position.distance(b.position) > 10.0);
            }
        }
    }
    #[test]
    fn clipboard_remaps_stereo_ids_and_keeps_annotations() {
        let mut d: Document =
            serde_json::from_str(include_str!("../tests/fixtures/ui-drawn-ethanol.moruno"))
                .unwrap();
        let original = d.clone();
        let ids = append(&mut d, &original, Point::new(100.0, 100.0));
        assert_eq!(ids.len(), original.all_ids().len());
        assert!(d.validate().is_ok());
        assert_eq!(selection(&d, &ids).atoms.len(), 3);
        assert_eq!(d.arrows.len(), 2);
    }
    #[test]
    fn alignment_does_not_collapse_bonded_atoms() {
        let mut d = Document::default();
        let a = d.add_atom("C", Point::default());
        let b = d.add_atom("O", Point::new(42.0, 0.0));
        d.add_bond(a, b, 1, "plain");
        d.add_atom("N", Point::new(100.0, 100.0));
        let ids = d.all_ids();
        arrange(&mut d, &ids, Arrange::AlignVertical);
        assert_eq!(
            d.atom(a)
                .unwrap()
                .position
                .distance(d.atom(b).unwrap().position),
            42.0
        );
        assert_eq!(groups(&d, &ids).len(), 2);
    }
}
