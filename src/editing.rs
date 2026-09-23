use crate::document::{Document, Point};
use std::collections::{HashMap, HashSet};

pub const CLIPBOARD_PREFIX: &str = "RESHIKI_DRAWING_V1\n";
pub fn clipboard_json(contents: &str) -> Option<&str> {
    contents
        .strip_prefix(CLIPBOARD_PREFIX)
        .or_else(|| contents.strip_prefix("MORUNO_DRAWING_V1\n"))
}
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
    TiltX(f32),
    TiltY(f32),
    FlipHorizontal,
    FlipVertical,
}
#[derive(Debug, Clone, Copy)]
pub enum Arrange {
    AlignLeft,
    AlignRight,
    AlignTop,
    AlignBottom,
    AlignHorizontal,
    AlignVertical,
    DistributeHorizontal,
    DistributeVertical,
}

pub fn selection(doc: &Document, ids: &[u64]) -> Document {
    let ids = crate::attachments::selection(doc, ids);
    let ids = doc.expand_abbreviation_selection(&ids);
    let ids = ids.as_slice();
    let mut part = doc.clone();
    part.reactions
        .retain(|r| r.ids().iter().all(|id| ids.contains(id)));
    part.groups
        .retain(|g| g.members.iter().all(|id| ids.contains(id)));
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
    if doc.validate().is_err()
        || source.validate().is_err()
        || !offset.x.is_finite()
        || !offset.y.is_finite()
    {
        return vec![];
    }
    let first = doc.next_id();
    let mapping: Option<HashMap<_, _>> = source
        .all_ids()
        .into_iter()
        .chain(source.groups.iter().map(|g| g.id))
        .enumerate()
        .map(|(i, id)| {
            first
                .checked_add(i as u64)
                .filter(|next| *next < u64::MAX)
                .map(|next| (id, next))
        })
        .collect();
    let Some(mapping) = mapping else {
        return vec![];
    };
    let mut part = source.clone();
    for a in &mut part.atoms {
        // A pasted fragment keeps its source appearance when document defaults differ.
        if source.atom_labels != doc.atom_labels {
            a.display.carbons.get_or_insert(source.atom_labels.carbons);
            a.display
                .hydrogens
                .get_or_insert(source.atom_labels.hydrogens);
            a.display
                .stereo
                .show
                .get_or_insert(source.atom_labels.stereo);
        }
        let Some(mapped) = mapping.get(&a.id).copied() else {
            return vec![];
        };
        a.id = mapped;
        for id in &mut a.centroid {
            let Some(mapped) = mapping.get(id).copied() else {
                return vec![];
            };
            *id = mapped;
        }
        a.position = a.position.offset(offset.x, offset.y);
        if let Some(s) = &mut a.stereo {
            for id in &mut s.neighbors {
                let Some(mapped) = mapping.get(id).copied() else {
                    return vec![];
                };
                *id = mapped;
            }
        }
    }
    for b in &mut part.bonds {
        if source.atom_labels.stereo != doc.atom_labels.stereo {
            b.indicator.show.get_or_insert(source.atom_labels.stereo);
        }
        let Some(mapped) = mapping.get(&b.a).copied() else {
            return vec![];
        };
        b.a = mapped;
        let Some(mapped) = mapping.get(&b.b).copied() else {
            return vec![];
        };
        b.b = mapped;
        for id in &mut b.stereo_atoms {
            let Some(mapped) = mapping.get(id).copied() else {
                return vec![];
            };
            *id = mapped;
        }
    }
    for a in &mut part.annotations {
        let Some(mapped) = mapping.get(&a.id).copied() else {
            return vec![];
        };
        a.id = mapped;
        a.position = a.position.offset(offset.x, offset.y);
    }
    for a in &mut part.arrows {
        let Some(mapped) = mapping.get(&a.id).copied() else {
            return vec![];
        };
        a.id = mapped;
        a.map_points(|p| p.offset(offset.x, offset.y));
    }
    for g in &mut part.graphics {
        let Some(mapped) = mapping.get(&g.id).copied() else {
            return vec![];
        };
        g.id = mapped;
        g.origin = g.origin.offset(offset.x, offset.y);
    }
    for g in &mut part.groups {
        let Some(mapped) = mapping.get(&g.id).copied() else {
            return vec![];
        };
        g.id = mapped;
        for id in &mut g.members {
            let Some(mapped) = mapping.get(id).copied() else {
                return vec![];
            };
            *id = mapped;
        }
    }
    for group in &mut part.abbreviations {
        let Some(anchor) = mapping.get(&group.anchor).copied() else {
            return vec![];
        };
        group.anchor = anchor;
        for id in &mut group.members {
            let Some(mapped) = mapping.get(id).copied() else {
                return vec![];
            };
            *id = mapped;
        }
    }
    for reaction in &mut part.reactions {
        if reaction.remap(&mapping).is_none() {
            return vec![];
        }
    }
    let ids = part.all_ids();
    doc.atoms.extend(part.atoms);
    doc.bonds.extend(part.bonds);
    doc.annotations.extend(part.annotations);
    doc.arrows.extend(part.arrows);
    doc.graphics.extend(part.graphics);
    doc.groups.extend(part.groups);
    doc.abbreviations.extend(part.abbreviations);
    doc.reactions.extend(part.reactions);
    if !doc.reactions.is_empty() {
        doc.version = 15;
    }
    if !doc.abbreviations.is_empty() {
        doc.version = doc.version.max(11);
    }
    ids
}

fn point_bounds(doc: &Document, ids: &[u64]) -> Option<(Point, Point)> {
    let points = doc
        .atoms
        .iter()
        .filter(|a| ids.contains(&a.id) && doc.atom_visible(a.id))
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
        )
        .chain(
            doc.graphics
                .iter()
                .filter(|g| ids.contains(&g.id))
                .flat_map(|g| {
                    let (lo, hi) = g.bounds();
                    [lo, hi]
                }),
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
    if let Transform::TiltX(degrees) | Transform::TiltY(degrees) = transform {
        crate::projection::tilt(doc, ids, degrees, matches!(transform, Transform::TiltX(_)));
        return;
    }
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
            Transform::TiltX(_) | Transform::TiltY(_) => (x, y),
        };
        center.offset(x, y)
    };
    map_positions(doc, ids, convert);
    crate::projection::sync_centroids(doc);
    if matches!(
        transform,
        Transform::FlipHorizontal | Transform::FlipVertical
    ) {
        // Reflect the projection while preserving the molecule's stereochemistry.
        for b in &mut doc.bonds {
            if ids.contains(&b.a) && ids.contains(&b.b) {
                b.double_position = b.double_position.reversed();
                if b.projection {
                    continue;
                }
                b.display = match (b.order, b.display.as_str()) {
                    (1, "wedge") => "hash",
                    (1, "hash") => "wedge",
                    (1, "hollow_wedge" | "bold") => "hashed",
                    (1, "hashed") => "hollow_wedge",
                    (_, other) => other,
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
    for atom in &mut doc.atoms {
        if ids.contains(&atom.id) {
            atom.depth *= scale;
        }
    }
    for graphic in &mut doc.graphics {
        if ids.contains(&graphic.id) {
            graphic.depth = graphic.depth.map(|z| z * scale);
        }
    }
    let (s, c) = degrees.to_radians().sin_cos();
    map_positions(doc, ids, |p| {
        let x = (p.x - pivot.x) * scale;
        let y = (p.y - pivot.y) * scale;
        pivot.offset(x * c - y * s, x * s + y * c)
    });
}

fn map_positions(doc: &mut Document, ids: &[u64], convert: impl Fn(Point) -> Point) {
    let ids = doc.expand_abbreviation_selection(ids);
    let ids = ids.as_slice();
    for graphic in &mut doc.graphics {
        if ids.contains(&graphic.id) {
            graphic.map_positions(&convert);
        }
    }
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
            let position = convert(a.position);
            for offset in [
                a.display.number.as_mut().and_then(|n| n.offset.as_mut()),
                a.display.stereo.offset.as_mut(),
            ]
            .into_iter()
            .flatten()
            {
                let moved = convert(a.position.offset(offset.x, offset.y));
                *offset = Point::new(moved.x - position.x, moved.y - position.y);
            }
            for mark in &mut a.marks {
                let moved = convert(a.position.offset(mark.offset.x, mark.offset.y));
                mark.offset = Point::new(moved.x - position.x, moved.y - position.y);
            }
            a.position = position;
        }
    }
    for b in &mut doc.bonds {
        if ids.contains(&b.a)
            && ids.contains(&b.b)
            && let Some(offset) = &mut b.indicator.offset
        {
            let zero = convert(Point::default());
            let moved = convert(*offset);
            *offset = Point::new(moved.x - zero.x, moved.y - zero.y);
        }
    }
    for a in &mut doc.annotations {
        if ids.contains(&a.id) {
            a.position = convert(a.position);
        }
    }
    for a in &mut doc.arrows {
        if ids.contains(&a.id) {
            a.map_points(&convert);
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
        while let Some(current) = group.get(i).copied() {
            for atom in doc.atoms.iter().filter(|a| a.attachment.is_some()) {
                if atom.id == current || atom.centroid.contains(&current) {
                    for id in std::iter::once(&atom.id).chain(&atom.centroid) {
                        if remaining.remove(id) {
                            group.push(*id);
                        }
                    }
                }
            }
            for persistent in &doc.groups {
                if persistent.members.contains(&current) {
                    for id in &persistent.members {
                        if remaining.remove(id) {
                            group.push(*id);
                        }
                    }
                }
            }
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
        Arrange::AlignHorizontal
            | Arrange::DistributeHorizontal
            | Arrange::AlignLeft
            | Arrange::AlignRight
    );
    let distribute = matches!(
        action,
        Arrange::DistributeHorizontal | Arrange::DistributeVertical
    );
    let mut groups: Vec<_> = groups(doc, ids)
        .into_iter()
        .filter_map(|g| crate::scene::selection_bounds(doc, &g).map(|b| (g, b)))
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
        } else if matches!(action, Arrange::AlignLeft | Arrange::AlignTop) {
            lo - coordinate(min)
        } else if matches!(action, Arrange::AlignRight | Arrange::AlignBottom) {
            hi - coordinate(max)
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
        .filter(|(_, b)| doc.bond_visible(b.a, b.b))
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
            if [3, 6].contains(&order)
                || [3, 6].contains(&previous_order)
                || (order == 2 && previous_order == 2)
            {
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
            angles
                .iter()
                .zip(angles.iter().cycle().skip(1))
                .take(angles.len())
                .enumerate()
                .map(|(i, (&start, &next))| {
                    let gap = next + if i + 1 == angles.len() { TAU } else { 0. } - start;
                    (start + gap / 2.0, gap)
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
    let Some(mut best) = candidates.first().copied() else {
        return point(preferred);
    };
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
        let Some(b) = doc.bonds.get(index).cloned() else {
            return vec![];
        };
        let (Some(a), Some(z)) = (doc.atom(b.a), doc.atom(b.b)) else {
            return vec![];
        };
        let (a, z) = (a.position, z.position);
        let positions = |sign: f32| {
            let mut points = vec![a, z];
            let mut vector = Point::new(z.x - a.x, z.y - a.y);
            let (s, c) = (sign * std::f32::consts::TAU / n as f32).sin_cos();
            for _ in 2..n {
                vector = Point::new(vector.x * c - vector.y * s, vector.x * s + vector.y * c);
                if let Some(last) = points.last().copied() {
                    points.push(last.offset(vector.x, vector.y));
                }
            }
            points
        };
        let score = |points: &[Point]| {
            points
                .iter()
                .skip(2)
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
        for p in points.iter().skip(2) {
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
    for (i, (&a, &b)) in ids
        .iter()
        .zip(ids.iter().cycle().skip(1))
        .take(n)
        .enumerate()
    {
        if i == 0 && bond.is_some() && !aromatic {
            continue;
        }
        doc.add_bond(a, b, if aromatic { 4 } else { 1 }, "plain");
    }
    if aromatic {
        for id in &ids {
            if let Some(atom) = doc.atom_mut(*id) {
                atom.aromatic = true;
            }
        }
    }
    ids
}

pub(crate) fn open_angle(anchor: Point, neighbors: &[Point]) -> f32 {
    use std::f32::consts::{PI, TAU};
    if neighbors.is_empty() {
        return PI;
    }
    let mut angles: Vec<_> = neighbors
        .iter()
        .map(|p| (p.y - anchor.y).atan2(p.x - anchor.x).rem_euclid(TAU))
        .collect();
    angles.sort_by(f32::total_cmp);
    let (start, gap) = angles
        .iter()
        .zip(angles.iter().cycle().skip(1))
        .take(angles.len())
        .enumerate()
        .map(|(i, (&start, &end))| {
            (
                start,
                if i + 1 == angles.len() {
                    end + TAU - start
                } else {
                    end - start
                },
            )
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .unwrap_or((0., TAU));
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
    let first = *ids.first()?;
    let mut ordered = vec![first];
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
        if next == first {
            return (ordered.len() == ids.len()).then_some(ordered);
        }
        if ordered.contains(&next) {
            return None;
        }
        ordered.push(next);
        previous = current;
    }
}

/// Hit the interior of a visible ring, including aromatic and substituted rings.
/// The separate fusion operation still requires an isolated saturated ring.
pub fn ring_at(doc: &Document, p: Point) -> Option<Vec<u64>> {
    let mut rings = crate::aromatic::ring_circles(doc, false);
    rings.sort_by(|a, b| a.radius.total_cmp(&b.radius));
    rings.into_iter().find_map(|ring| {
        let ring = ring.atoms;
        let mut inside = false;
        for (a, b) in ring
            .iter()
            .zip(ring.iter().cycle().skip(1))
            .take(ring.len())
        {
            let a = doc.atom(*a)?.position;
            let b = doc.atom(*b)?.position;
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
    if doc
        .groups
        .iter()
        .any(|g| g.members.iter().any(|id| ids.contains(id)))
    {
        return None;
    }
    struct Candidate {
        score: f32,
        source: [u64; 2],
        target: [u64; 2],
        points: Vec<Point>,
    }
    let ring = isolated_ring(doc, ids)?;
    let mut best: Option<Candidate> = None;
    for (a, b) in ring
        .iter()
        .zip(ring.iter().cycle().skip(1))
        .take(ring.len())
    {
        let source = [*a, *b];
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
                        let p = doc.atom(*id)?.position;
                        let x = p.x - a.x;
                        let y = p.y - a.y;
                        Some(target[0].position.offset(
                            cosine_scale * x - sine_scale * y,
                            sine_scale * x + cosine_scale * y,
                        ))
                    })
                    .collect::<Option<Vec<_>>>()?;
                let mut score = 0.0;
                for (id, p) in ring.iter().zip(&points) {
                    let moved = doc.atom(*id)?.position.offset(delta.x, delta.y);
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
    fn ring_interior_hit_includes_aromatic_hetero_and_substituted_rings() -> Result<(), String> {
        let mut doc = Document::default();
        let ids = ring(&mut doc, Point::default(), 6, true, 5.);
        let nitrogen = *ids.first().ok_or("ring atom")?;
        doc.atom_mut(nitrogen).ok_or("nitrogen")?.element = "N".into();
        let attach = *ids.get(2).ok_or("substituted atom")?;
        let position = doc.atom(attach).ok_or("atom")?.position;
        let methyl = doc.add_atom("C", position.offset(60., 0.));
        doc.add_bond(attach, methyl, 1, "plain");
        let before = doc.clone();
        let mut hit = ring_at(&doc, center(&doc, &ids)).ok_or("ring interior")?;
        let mut expected = ids.clone();
        hit.sort_unstable();
        expected.sort_unstable();
        assert_eq!(hit, expected);
        assert_eq!(ring_at(&doc, Point::new(500., 500.)), None);
        assert!(snap_ring(&mut doc, &ids, Point::new(40., 0.), 15.).is_none());
        assert_eq!(
            doc, before,
            "Selecting an aromatic ring must not enable fusion"
        );
        crate::projection::tilt(&mut doc, &ids, 60., true);
        let mut tilted = ring_at(&doc, center(&doc, &ids)).ok_or("tilted interior")?;
        tilted.sort_unstable();
        assert_eq!(tilted, expected);
        Ok(())
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
            serde_json::from_str(include_str!("../tests/fixtures/ui-drawn-ethanol.reshiki"))
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
