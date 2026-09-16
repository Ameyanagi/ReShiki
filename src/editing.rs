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
    map_positions(doc, ids, convert);
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

/// Uniform, orientation-preserving transform used by the selection handles.
pub fn transform_about(doc: &mut Document, ids: &[u64], pivot: Point, scale: f32, degrees: f32) {
    if !scale.is_finite()
        || scale <= 0.0
        || !degrees.is_finite()
        || (scale == 1.0 && degrees == 0.0)
    {
        return;
    }
    let (s, c) = degrees.to_radians().sin_cos();
    map_positions(doc, ids, |p| {
        let x = (p.x - pivot.x) * scale;
        let y = (p.y - pivot.y) * scale;
        pivot.offset(x * c - y * s, x * s + y * c)
    });
}

fn map_positions(doc: &mut Document, ids: &[u64], convert: impl Fn(Point) -> Point) {
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

/// Place a clicked bond using the local graph, keeping terminal chains zigzagged.
/// Explicit drags still choose their own direction.
pub fn bond_extension(doc: &Document, start: Point, atom: Option<u64>, order: u8) -> Point {
    use std::f32::consts::{FRAC_PI_3, PI, TAU};
    let length = crate::style::DEFAULT.bond_length_world;
    let neighbors: Vec<_> = doc
        .bonds
        .iter()
        .filter_map(|b| {
            let id = if Some(b.a) == atom {
                b.b
            } else if Some(b.b) == atom {
                b.a
            } else {
                return None;
            };
            let p = doc.atom(id)?.position;
            (start.distance(p) > 0.001).then_some((id, p, b.order))
        })
        .collect();
    let direction = |a: Point, b: Point| (b.y - a.y).atan2(b.x - a.x);
    let mut preferred = -PI / 6.0;
    let candidates = match neighbors.as_slice() {
        [] => vec![(preferred, TAU)],
        &[(id, neighbor, previous_order)] => {
            let incoming = direction(neighbor, start);
            // Triple bonds and two consecutive double bonds have a linear junction.
            if order == 3 || previous_order == 3 || (order == 2 && previous_order == 2) {
                vec![(incoming, PI)]
            } else {
                let previous: Vec<_> = doc
                    .bonds
                    .iter()
                    .filter_map(|b| {
                        let other = if b.a == id {
                            b.b
                        } else if b.b == id {
                            b.a
                        } else {
                            return None;
                        };
                        (Some(other) != atom).then(|| doc.atom(other)).flatten()
                    })
                    .collect();
                // Reuse the direction of the preceding segment so repeated clicks
                // alternate turns instead of curling into a ring.
                preferred = if let [previous] = previous.as_slice() {
                    direction(previous.position, neighbor)
                } else {
                    (incoming / PI).round() * PI
                };
                vec![
                    (incoming + FRAC_PI_3, 2.0 * FRAC_PI_3),
                    (incoming - FRAC_PI_3, 2.0 * FRAC_PI_3),
                ]
            }
        }
        _ => {
            let mut angles: Vec<_> = neighbors
                .iter()
                .map(|(_, p, _)| direction(start, *p).rem_euclid(TAU))
                .collect();
            angles.sort_by(f32::total_cmp);
            (0..angles.len())
                .map(|i| {
                    let next = if i + 1 == angles.len() {
                        angles[0] + TAU
                    } else {
                        angles[i + 1]
                    };
                    let gap = next - angles[i];
                    (angles[i] + gap / 2.0, gap)
                })
                .collect()
        }
    };
    let point = |angle: f32| start.offset(length * angle.cos(), length * angle.sin());
    let score = |angle: f32, gap: f32| {
        let end = point(angle);
        let mut collisions = 0.0;
        // Avoid placing a new atom on an existing atom or routing through a bond.
        for a in &doc.atoms {
            if Some(a.id) != atom {
                let distance = segment_distance(a.position, start, end) / length;
                collisions += (0.45 - distance).max(0.0).powi(2);
            }
        }
        for b in &doc.bonds {
            if Some(b.a) == atom || Some(b.b) == atom {
                continue;
            }
            if let Some((a, z)) = doc.atom(b.a).zip(doc.atom(b.b)) {
                for fraction in [0.25, 0.5, 0.75, 1.0] {
                    let p =
                        start.offset((end.x - start.x) * fraction, (end.y - start.y) * fraction);
                    let distance = segment_distance(p, a.position, z.position) / length;
                    collisions += (0.2 - distance).max(0.0).powi(2);
                }
            }
        }
        collisions * 1000.0 + (TAU - gap) + (1.0 - (angle - preferred).cos()) * 0.01
    };
    let mut best = candidates[0];
    let mut best_score = score(best.0, best.1);
    for candidate in candidates.into_iter().skip(1) {
        let value = score(candidate.0, candidate.1);
        if value < best_score - 0.00001 {
            best = candidate;
            best_score = value;
        }
    }
    point(best.0)
}

fn segment_distance(p: Point, a: Point, b: Point) -> f32 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let length_sq = dx * dx + dy * dy;
    if length_sq < 0.00001 {
        return p.distance(a);
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / length_sq).clamp(0.0, 1.0);
    p.distance(a.offset(dx * t, dy * t))
}

pub fn ring(doc: &mut Document, p: Point, size: u8, aromatic: bool, radius: f32) -> Vec<u64> {
    ring_oriented(doc, p, size, aromatic, radius, None)
}

/// Attach at an atom/bond, optionally using a drag to choose the ring's side.
pub fn ring_oriented(
    doc: &mut Document,
    p: Point,
    size: u8,
    aromatic: bool,
    radius: f32,
    direction: Option<Point>,
) -> Vec<u64> {
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
        let side = direction.map(|p| (z.x - a.x) * (p.y - a.y) - (z.y - a.y) * (p.x - a.x));
        let first_side = side
            .filter(|side| side.abs() > radius * a.distance(z))
            .map(|side| side > 0.0)
            .unwrap_or_else(|| score(&first) <= score(&second));
        let points = if first_side { first } else { second };
        ids.extend([b.a, b.b]);
        for p in &points[2..] {
            ids.push(doc.add_atom("C", *p));
        }
    } else {
        let neighbors: Vec<_> = doc
            .bonds
            .iter()
            .filter_map(|b| {
                let other = if Some(b.a) == atom {
                    b.b
                } else if Some(b.b) == atom {
                    b.a
                } else {
                    return None;
                };
                doc.atom(other).map(|a| a.position)
            })
            .collect();
        let anchor = atom
            .and_then(|id| doc.atom(id))
            .map(|a| a.position)
            .unwrap_or(p);
        let length = if neighbors.is_empty() {
            crate::style::DEFAULT.bond_length_world
        } else {
            neighbors.iter().map(|p| p.distance(anchor)).sum::<f32>() / neighbors.len() as f32
        };
        let r = length / (2.0 * (std::f32::consts::PI / n as f32).sin());
        // The center belongs in the open angular gap, opposite the substituent
        // at a terminal atom. Never assume that the ring lies to its left.
        let angle = direction
            .filter(|p| p.distance(anchor) > radius)
            .map(|p| (p.y - anchor.y).atan2(p.x - anchor.x))
            .unwrap_or_else(|| open_angle(anchor, &neighbors));
        let center = if atom.is_some() {
            anchor.offset(r * angle.cos(), r * angle.sin())
        } else {
            p
        };
        let phase = if atom.is_some() {
            angle + std::f32::consts::PI
        } else {
            0.0
        };
        for i in 0..n {
            let angle = phase + i as f32 * std::f32::consts::TAU / n as f32;
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

fn open_angle(anchor: Point, neighbors: &[Point]) -> f32 {
    use std::f32::consts::{PI, TAU};
    if neighbors.is_empty() {
        return PI;
    }
    let mut angles: Vec<_> = neighbors
        .iter()
        .map(|p| (p.y - anchor.y).atan2(p.x - anchor.x).rem_euclid(TAU))
        .collect();
    angles.sort_by(f32::total_cmp);
    let (start, gap) = (0..angles.len())
        .map(|i| {
            let end = if i + 1 < angles.len() {
                angles[i + 1]
            } else {
                angles[0] + TAU
            };
            (angles[i], end - angles[i])
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .unwrap();
    start + gap / 2.0
}

/// A standalone, unlabelled saturated ring can be grabbed by its interior.
fn isolated_ring(doc: &Document, ids: &[u64]) -> Option<Vec<u64>> {
    if !(3..=8).contains(&ids.len()) {
        return None;
    }
    for id in ids {
        let atom = doc.atom(*id)?;
        if atom.element != "C"
            || atom.charge != 0
            || atom.isotope != 0
            || atom.aromatic
            || atom.stereo.is_some()
            || atom.explicit_h != 0
            || atom.no_implicit
            || atom.map_num != 0
        {
            return None;
        }
        let bonds: Vec<_> = doc
            .bonds
            .iter()
            .filter(|b| b.a == *id || b.b == *id)
            .collect();
        if bonds.len() != 2
            || bonds.iter().any(|b| {
                !ids.contains(&b.a)
                    || !ids.contains(&b.b)
                    || b.order != 1
                    || b.display != "plain"
                    || b.stereo.is_some()
            })
        {
            return None;
        }
    }
    let mut ordered = vec![ids[0]];
    let mut previous = 0;
    loop {
        let current = *ordered.last()?;
        let next = doc
            .bonds
            .iter()
            .filter_map(|b| {
                if b.a == current {
                    Some(b.b)
                } else if b.b == current {
                    Some(b.a)
                } else {
                    None
                }
            })
            .find(|id| *id != previous)?;
        if next == ordered[0] {
            return (ordered.len() == ids.len()).then_some(ordered);
        }
        if ordered.contains(&next) {
            return None;
        }
        ordered.push(next);
        previous = current;
    }
}

pub fn ring_at(doc: &Document, p: Point) -> Option<Vec<u64>> {
    groups(doc, &doc.all_ids()).into_iter().find_map(|ids| {
        let ring = isolated_ring(doc, &ids)?;
        let mut inside = false;
        for i in 0..ring.len() {
            let a = doc.atom(ring[i])?.position;
            let b = doc.atom(ring[(i + 1) % ring.len()])?.position;
            if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
                inside = !inside;
            }
        }
        inside.then_some(ring)
    })
}

/// Fuse a dragged standalone saturated carbon ring onto a nearby single bond.
/// The two shared atoms are reused; the remaining ring atoms rotate and scale
/// together to match that edge. Failed snaps leave the document untouched.
pub fn snap_ring(doc: &mut Document, ids: &[u64], delta: Point, radius: f32) -> Option<Vec<u64>> {
    struct Candidate {
        score: f32,
        source: [u64; 2],
        target: [u64; 2],
        points: Vec<Point>,
    }
    let ring = isolated_ring(doc, ids)?;
    let mut best: Option<Candidate> = None;
    for i in 0..ring.len() {
        let source = [ring[i], ring[(i + 1) % ring.len()]];
        let a = doc.atom(source[0])?.position;
        let b = doc.atom(source[1])?.position;
        let sx = b.x - a.x;
        let sy = b.y - a.y;
        let source_length_sq = sx * sx + sy * sy;
        if source_length_sq < 0.001 {
            continue;
        }
        let midpoint = Point::new((a.x + b.x) / 2.0 + delta.x, (a.y + b.y) / 2.0 + delta.y);
        for bond in &doc.bonds {
            if ids.contains(&bond.a)
                || ids.contains(&bond.b)
                || bond.order != 1
                || bond.display != "plain"
            {
                continue;
            }
            let Some((ta, tb)) = doc.atom(bond.a).zip(doc.atom(bond.b)) else {
                continue;
            };
            if [ta, tb].iter().any(|a| {
                a.element != "C"
                    || a.charge != 0
                    || a.isotope != 0
                    || a.aromatic
                    || doc
                        .bonds
                        .iter()
                        .filter(|b| b.a == a.id || b.b == a.id)
                        .map(|b| b.order as u32)
                        .sum::<u32>()
                        > 3
            }) {
                continue;
            }
            let target_midpoint = Point::new(
                (ta.position.x + tb.position.x) / 2.0,
                (ta.position.y + tb.position.y) / 2.0,
            );
            if midpoint.distance(target_midpoint) > radius {
                continue;
            }
            let length = ta.position.distance(tb.position);
            if length < 0.001 {
                continue;
            }
            for target in [[ta, tb], [tb, ta]] {
                let tx = target[1].position.x - target[0].position.x;
                let ty = target[1].position.y - target[0].position.y;
                let cosine_scale = (tx * sx + ty * sy) / source_length_sq;
                let sine_scale = (ty * sx - tx * sy) / source_length_sq;
                let points: Vec<_> = ring
                    .iter()
                    .map(|id| {
                        let p = doc.atom(*id).unwrap().position;
                        let x = p.x - a.x;
                        let y = p.y - a.y;
                        target[0].position.offset(
                            cosine_scale * x - sine_scale * y,
                            sine_scale * x + cosine_scale * y,
                        )
                    })
                    .collect();
                let mut score = 0.0;
                for (id, p) in ring.iter().zip(&points) {
                    let moved = doc.atom(*id).unwrap().position.offset(delta.x, delta.y);
                    score += (p.distance(moved) / length).powi(2) * 0.1;
                    if !source.contains(id) {
                        for other in &doc.atoms {
                            if !ids.contains(&other.id) {
                                score +=
                                    (0.6 - p.distance(other.position) / length).max(0.0).powi(2)
                                        * 1000.0;
                            }
                        }
                    }
                }
                if best.as_ref().is_none_or(|best| score < best.score) {
                    best = Some(Candidate {
                        score,
                        source,
                        target: [target[0].id, target[1].id],
                        points,
                    });
                }
            }
        }
    }
    let Candidate {
        source,
        target,
        points,
        ..
    } = best?;
    let mapped = |id| {
        if id == source[0] {
            target[0]
        } else if id == source[1] {
            target[1]
        } else {
            id
        }
    };
    doc.invalidate_chemistry(&[source[0], source[1], target[0], target[1]]);
    for (id, point) in ring.iter().zip(points) {
        if !source.contains(id) {
            doc.atom_mut(*id)?.position = point;
        }
    }
    doc.atoms.retain(|a| !source.contains(&a.id));
    doc.bonds
        .retain(|b| !(source.contains(&b.a) && source.contains(&b.b)));
    for bond in &mut doc.bonds {
        bond.a = mapped(bond.a);
        bond.b = mapped(bond.b);
    }
    Some(ring.into_iter().map(mapped).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertex_attachment_keeps_substituents_outside_at_any_orientation_and_scale() {
        for degrees in (0..360).step_by(30) {
            for length in [21.0, 42.0, 63.0] {
                let angle = (degrees as f32).to_radians();
                let mut doc = Document::default();
                let anchor = Point::new(100.0, 50.0);
                let substituent = anchor.offset(length * angle.cos(), length * angle.sin());
                let a = doc.add_atom("C", anchor);
                let b = doc.add_atom("C", substituent);
                doc.add_bond(a, b, 1, "plain");
                let ids = ring(&mut doc, anchor, 6, false, 5.0);
                let center = center(&doc, &ids);
                let dot = (center.x - anchor.x) * (substituent.x - anchor.x)
                    + (center.y - anchor.y) * (substituent.y - anchor.y);
                assert!(
                    dot < 0.0,
                    "substituent must point away from the ring center"
                );
                assert_eq!(doc.atom(b).unwrap().position, substituent);
                assert_eq!(doc.atom(a).unwrap().position, anchor);
                assert_eq!((doc.atoms.len(), doc.bonds.len()), (7, 7));
                for bond in &doc.bonds {
                    assert!(
                        (doc.atom(bond.a)
                            .unwrap()
                            .position
                            .distance(doc.atom(bond.b).unwrap().position)
                            - length)
                            .abs()
                            < 0.001
                    );
                }
                doc.validate().unwrap();
            }
        }
    }

    #[test]
    fn ring_drag_chooses_the_requested_side_and_matches_the_target_edge() {
        for side in [-1.0, 1.0] {
            let mut doc = Document::default();
            let a = doc.add_atom("C", Point::default());
            let b = doc.add_atom("C", Point::new(60.0, 0.0));
            doc.add_bond(a, b, 1, "plain");
            let ids = ring_oriented(
                &mut doc,
                Point::new(30.0, 0.0),
                5,
                false,
                5.0,
                Some(Point::new(30.0, side * 50.0)),
            );
            assert_eq!(&ids[..2], &[a, b]);
            assert_eq!((doc.atoms.len(), doc.bonds.len()), (5, 5));
            for id in &ids[2..] {
                assert!(doc.atom(*id).unwrap().position.y * side > 0.0);
            }
            for bond in &doc.bonds {
                assert!(
                    (doc.atom(bond.a)
                        .unwrap()
                        .position
                        .distance(doc.atom(bond.b).unwrap().position)
                        - 60.0)
                        .abs()
                        < 0.001
                );
            }
        }
    }

    #[test]
    fn dragging_an_existing_ring_rotates_scales_and_reuses_the_target_atoms() {
        let mut doc = Document::default();
        let fixed = ring(&mut doc, Point::default(), 6, false, 5.0);
        let fixed_atoms = doc.atoms.clone();
        let moving = ring(&mut doc, Point::new(250.0, 200.0), 5, false, 5.0);
        transform(&mut doc, &moving, Transform::Rotate(37.0));
        for id in &moving {
            let p = &mut doc.atom_mut(*id).unwrap().position;
            *p = Point::new(250.0 + (p.x - 250.0) * 0.6, 200.0 + (p.y - 200.0) * 0.6);
        }
        assert_eq!(ring_at(&doc, center(&doc, &moving)).unwrap().len(), 5);
        let midpoint = |ids: &[u64]| {
            let a = doc.atom(ids[0]).unwrap().position;
            let b = doc.atom(ids[1]).unwrap().position;
            Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0)
        };
        let a = midpoint(&fixed);
        let b = midpoint(&moving);
        let snapped = snap_ring(&mut doc, &moving, Point::new(a.x - b.x, a.y - b.y), 5.0).unwrap();
        assert_eq!((doc.atoms.len(), doc.bonds.len()), (9, 10));
        assert_eq!(snapped.len(), 5);
        assert!(snapped.contains(&fixed[0]) && snapped.contains(&fixed[1]));
        for original in fixed_atoms {
            assert_eq!(doc.atom(original.id).unwrap().position, original.position);
        }
        for bond in &doc.bonds {
            assert!(
                (doc.atom(bond.a)
                    .unwrap()
                    .position
                    .distance(doc.atom(bond.b).unwrap().position)
                    - 42.0)
                    .abs()
                    < 0.001
            );
        }
        for (i, atom) in doc.atoms.iter().enumerate() {
            for other in &doc.atoms[i + 1..] {
                assert!(atom.position.distance(other.position) > 10.0);
            }
        }
        doc.validate().unwrap();
        let before = doc.clone();
        assert!(
            snap_ring(&mut doc, &snapped, Point::default(), 5.0).is_none(),
            "an attached ring must not silently discard external bonds"
        );
        assert_eq!(doc, before);
    }

    #[test]
    fn growth_uses_open_space_at_either_end_and_at_a_branch() {
        let mut doc = Document::default();
        let a = doc.add_atom("C", Point::default());
        let b = doc.add_atom("C", Point::new(36.373066, -21.0));
        let c = doc.add_atom("C", Point::new(72.74613, 0.0));
        doc.add_bond(a, b, 1, "plain");
        doc.add_bond(b, c, 1, "plain");
        let left = bond_extension(&doc, doc.atom(a).unwrap().position, Some(a), 1);
        let right = bond_extension(&doc, doc.atom(c).unwrap().position, Some(c), 1);
        let branch = bond_extension(&doc, doc.atom(b).unwrap().position, Some(b), 1);
        assert!(left.x < -36.0 && (left.y + 21.0).abs() < 0.01);
        assert!(right.x > 109.0 && (right.y + 21.0).abs() < 0.01);
        assert!((branch.x - 36.373066).abs() < 0.01 && (branch.y + 63.0).abs() < 0.01);

        // An occupied preferred endpoint should choose the other 120° turn.
        doc.add_atom("O", right);
        let alternate = bond_extension(&doc, doc.atom(c).unwrap().position, Some(c), 1);
        assert!(alternate.distance(right) > 60.0);
        assert!((alternate.x - 72.74613).abs() < 0.01 && (alternate.y - 42.0).abs() < 0.01);
    }

    #[test]
    fn triple_and_cumulative_double_bonds_extend_linearly() {
        for (previous, next) in [(3, 1), (1, 3), (2, 2)] {
            let mut doc = Document::default();
            let a = doc.add_atom("C", Point::default());
            let b = doc.add_atom("C", Point::new(42.0, 0.0));
            doc.add_bond(a, b, previous, "plain");
            let end = bond_extension(&doc, doc.atom(b).unwrap().position, Some(b), next);
            assert!(end.distance(Point::new(84.0, 0.0)) < 0.001);
        }
    }
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
