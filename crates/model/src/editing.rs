use crate::document::{Document, Point};
use std::collections::{HashMap, HashSet};

#[cfg(test)]
mod arrange_tests;
mod rotation;
pub use rotation::center as rotation_center;

pub const CLIPBOARD_PREFIX: &str = "RESHIKI_DRAWING_V1\n";

/// Add the chosen element from an existing atom, or join an existing endpoint.
/// Click-to-replace is separate; dragging never relabels either existing atom.
pub fn add_bonded_atom(
    source: &Document,
    start: u64,
    end: Point,
    target: Option<u64>,
    element: &str,
) -> Result<(Document, u64), String> {
    source.validate()?;
    let origin = source
        .atom(start)
        .ok_or("The starting atom is no longer available")?;
    if !end.x.is_finite() || !end.y.is_finite() || origin.position.distance(end) < 0.001 {
        return Err("Drag away from the starting atom to add a bond".into());
    }
    if let Some(id) = target {
        if id == start || source.atom(id).is_none() {
            return Err("Choose a different existing endpoint".into());
        }
    } else if element != "*" && !ELEMENTS.contains(&element) {
        return Err("Choose an element from Atoms first".into());
    }
    let mut doc = source.clone();
    let id = target.unwrap_or_else(|| doc.add_atom(element, end));
    if !doc
        .bonds
        .iter()
        .any(|b| (b.a == start && b.b == id) || (b.a == id && b.b == start))
    {
        doc.invalidate_chemistry(&[start, id]);
        doc.add_bond(start, id, 1, "plain");
    }
    doc.validate()?;
    Ok((doc, id))
}
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

/// The selected part as its own document: [`crate::attachments::selection`],
/// then [`Document::expand_abbreviation_selection`].
pub fn selection(doc: &Document, ids: &[u64]) -> Document {
    let ids = crate::attachments::selection(doc, ids);
    let ids: HashSet<_> = doc
        .expand_abbreviation_selection(&ids)
        .into_iter()
        .collect();
    let mut part = doc.clone();
    part.reactions
        .retain(|r| r.ids().iter().all(|id| ids.contains(id)));
    part.groups
        .retain(|g| g.members.iter().all(|id| ids.contains(id)));
    let removed: Vec<_> = doc.object_ids().filter(|id| !ids.contains(id)).collect();
    if !removed.is_empty() {
        part.delete(&removed);
    }
    part.depth_appearance =
        crate::depth_appearance::selection(doc, &ids.iter().copied().collect::<Vec<_>>());
    part
}

/// The atoms a chemistry analysis of `selected` covers: any selected
/// abbreviation member pulls in its whole abbreviation, in document order.
pub fn analysis_atoms(doc: &Document, selected: &[u64]) -> Vec<u64> {
    let ids: HashSet<_> = doc
        .expand_abbreviation_selection(selected)
        .into_iter()
        .collect();
    doc.atoms
        .iter()
        .filter(|a| ids.contains(&a.id))
        .map(|a| a.id)
        .collect()
}

/// The fragment analyzed for `atoms`, without cached computed labels.
pub fn analysis_document(doc: &Document, atoms: &[u64]) -> Document {
    let mut part = selection(doc, atoms);
    crate::atom_labels::clear_computed(&mut part);
    part
}

/// Insert `source` at `offset` under new IDs and return the inserted IDs.
///
/// IDs are assigned sequentially from `doc.next_id()`: first to
/// `source.all_ids()`, then to the source group IDs. The returned IDs are the
/// inserted part's [`Document::all_ids`], the source `all_ids()` order
/// remapped, so the remap is `zip(source.all_ids(), returned)`. On failure
/// nothing is inserted and the result is empty.
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
    part.depth_appearance = crate::depth_appearance::remap(&source.depth_appearance, &mapping);
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
        crate::arrow_anchors::remap(a, &mapping);
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
    for fill in &mut part.ring_fills {
        for id in &mut fill.atoms {
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
    doc.ring_fills.extend(part.ring_fills);
    doc.depth_appearance.extend(part.depth_appearance);
    if !doc.reactions.is_empty() {
        doc.version = doc.version.max(15);
    }
    if !doc.abbreviations.is_empty() {
        doc.version = doc.version.max(11);
    }
    crate::arrow_anchors::reconcile(doc);
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
    let center = if matches!(transform, Transform::Rotate(_)) {
        let Some(center) = rotation_center(doc, ids) else {
            return;
        };
        center
    } else {
        center(doc, ids)
    };
    let vector = |p: Point| {
        let (x, y) = (p.x, p.y);
        let (x, y) = match transform {
            Transform::Rotate(degrees) => {
                let (s, c) = degrees.to_radians().sin_cos();
                (x * c - y * s, x * s + y * c)
            }
            Transform::FlipHorizontal => (-x, y),
            Transform::FlipVertical => (x, -y),
            Transform::TiltX(_) | Transform::TiltY(_) => (x, y),
        };
        Point::new(x, y)
    };
    let convert = |p: Point| {
        let p = vector(Point::new(p.x - center.x, p.y - center.y));
        center.offset(p.x, p.y)
    };
    map_positions(doc, ids, convert, vector);
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
    let vector = |p: Point| {
        let (x, y) = (p.x * scale, p.y * scale);
        Point::new(x * c - y * s, x * s + y * c)
    };
    map_positions(
        doc,
        ids,
        |p| {
            let p = vector(Point::new(p.x - pivot.x, p.y - pivot.y));
            pivot.offset(p.x, p.y)
        },
        vector,
    );
}

/// Stretch the drawing in its plane without reflecting atoms or resizing text.
/// Projection depth stays unchanged: this changes X/Y, not the Z axis.
pub fn scale_axes_about(doc: &mut Document, ids: &[u64], pivot: Point, x: f32, y: f32) {
    if !x.is_finite()
        || !y.is_finite()
        || x <= 0.
        || y <= 0.
        || !pivot.x.is_finite()
        || !pivot.y.is_finite()
        || (x == 1. && y == 1.)
    {
        return;
    }
    let vector = |p: Point| Point::new(p.x * x, p.y * y);
    map_positions(
        doc,
        ids,
        |p| {
            let p = vector(Point::new(p.x - pivot.x, p.y - pivot.y));
            pivot.offset(p.x, p.y)
        },
        vector,
    );
}

fn map_positions(
    doc: &mut Document,
    ids: &[u64],
    convert: impl Fn(Point) -> Point,
    vector: impl Fn(Point) -> Point,
) {
    let ids: HashSet<_> = doc.expand_abbreviation_selection(ids).into_iter().collect();
    for graphic in &mut doc.graphics {
        if ids.contains(&graphic.id) {
            // Axes are displacements, not translated points. Subtracting two
            // transformed world endpoints loses precision when imported paths
            // have small axes and large local control coordinates.
            graphic.origin = convert(graphic.origin);
            graphic.axis_x = vector(graphic.axis_x);
            graphic.axis_y = vector(graphic.axis_y);
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
    // Derived markers must reach their final sites before attachment offsets
    // are rebased, in both the preview and the committed document.
    crate::projection::sync_centroids(doc);
    crate::arrow_anchors::transformed(doc, &ids.into_iter().collect::<Vec<_>>(), vector);
}

/// Connected selected atoms move as one object during alignment/distribution.
pub fn groups(doc: &Document, ids: &[u64]) -> Vec<Vec<u64>> {
    let mut remaining: HashSet<_> = ids.iter().copied().collect();
    if remaining.is_empty() {
        return vec![];
    }
    let attachments: Vec<Vec<_>> = doc
        .atoms
        .iter()
        .filter(|a| a.attachment.is_some())
        .map(|a| {
            std::iter::once(a.id)
                .chain(a.centroid.iter().copied())
                .collect()
        })
        .collect();
    let memberships: Vec<&[u64]> = attachments
        .iter()
        .map(Vec::as_slice)
        .chain(doc.groups.iter().map(|g| g.members.as_slice()))
        .collect();
    let mut groups_by_id = std::collections::HashMap::<u64, Vec<usize>>::new();
    for (index, members) in memberships.iter().enumerate() {
        for id in *members {
            groups_by_id.entry(*id).or_default().push(index);
        }
    }
    let mut visited_groups = HashSet::new();
    let mut adjacent = std::collections::HashMap::<u64, Vec<u64>>::new();
    for bond in &doc.bonds {
        adjacent.entry(bond.a).or_default().push(bond.b);
        adjacent.entry(bond.b).or_default().push(bond.a);
    }
    let mut result = vec![];
    for id in ids {
        if !remaining.remove(id) {
            continue;
        }
        let mut group = vec![*id];
        let mut i = 0;
        while let Some(current) = group.get(i).copied() {
            for index in groups_by_id.get(&current).into_iter().flatten() {
                if visited_groups.insert(*index) {
                    for id in memberships
                        .get(*index)
                        .into_iter()
                        .flat_map(|ids| ids.iter())
                    {
                        if remaining.remove(id) {
                            group.push(*id);
                        }
                    }
                }
            }
            for neighbor in adjacent.get(&current).into_iter().flatten() {
                if remaining.remove(neighbor) {
                    group.push(*neighbor);
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
    let groups = groups(doc, ids);
    if groups.is_empty() {
        return;
    }
    let bounds = crate::scene::selections_bounds(doc, &groups);
    let mut groups: Vec<_> = groups
        .into_iter()
        .zip(bounds)
        .filter_map(|(g, bounds)| bounds.map(|b| (g, b)))
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
            let d = crate::graphics::segment_distance(p, a, z);
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
    ring_oriented(doc, p, size, aromatic, radius, None).unwrap_or_default()
}

fn regular_ring_valence_fits(doc: &Document, target: &crate::document::Atom) -> bool {
    if target.element == "P" && target.charge == 0 {
        use crate::chemistry::graph::{Atom, Bond, Graph};
        // This is a valence-only local star, never a molecular representation.
        // Native per-atom valence depends on the target and incident bonds;
        // wildcard neighbors avoid interpreting attachment nodes or validating
        // unrelated chemistry. The protected-state guard runs before this.
        let mut graph = Graph {
            atoms: vec![Atom {
                atomic_number: 15,
                aromatic: target.aromatic,
                ..Atom::default()
            }],
            bonds: vec![],
        };
        let mut indices = HashMap::from([(target.id, 0)]);
        for bond in doc
            .bonds
            .iter()
            .filter(|b| b.a == target.id || b.b == target.id)
        {
            let other = if bond.a == target.id { bond.b } else { bond.a };
            if doc.atom(other).is_none() || bond.validate_appearance().is_err() {
                return false;
            }
            let index = *indices.entry(other).or_insert_with(|| {
                let index = graph.atoms.len();
                graph.atoms.push(Atom::default());
                index
            });
            let (a, b) = if bond.a == target.id {
                (0, index)
            } else {
                (index, 0)
            };
            graph.bonds.push(Bond {
                a,
                b,
                order: bond.order,
                aromatic: bond.order == 4,
            });
        }
        return graph.valences().is_ok();
    }
    crate::templates::valence(doc, target.id) <= crate::templates::capacity(target)
}

/// Why a regular ring atom cannot be shared, as a short label.
fn shared_atom_label(
    doc: &Document,
    result: &Document,
    target: &crate::document::Atom,
) -> Option<String> {
    let id = target.id;
    let stereo = target.stereo.is_some()
        || doc.atoms.iter().any(|a| {
            a.stereo
                .as_ref()
                .is_some_and(|stereo| stereo.neighbors.contains(&id))
        })
        || doc.bonds.iter().any(|b| {
            b.stereo_atoms.contains(&id)
                || (b.a == id || b.b == id)
                    && (b.stereo.is_some()
                        || !b.stereo_atoms.is_empty()
                        || (!b.projection
                            && ((b.order == 1
                                && matches!(
                                    b.display.as_str(),
                                    "wedge" | "hollow_wedge" | "bold" | "hash" | "hashed" | "wavy"
                                ))
                                || (b.order == 2 && b.display == "wavy"))))
        });
    let reason = if stereo {
        "has stereochemistry"
    } else if target.radical_electrons != 0 {
        "has a radical"
    } else if !target.marks.is_empty() {
        "has charge or electron marks"
    } else if target.explicit_h != 0 || target.no_implicit {
        "has fixed hydrogens"
    } else if target.isotope != 0 {
        "has an isotope label"
    } else if target.map_num != 0 {
        "has a map number"
    } else if target.attachment.is_some() || !target.centroid.is_empty() {
        "is an attachment point"
    } else if doc.abbreviation(id).is_some() {
        return Some("Expand the abbreviation first".into());
    } else if !regular_ring_valence_fits(result, target) {
        // Neutral phosphorus follows its allowed valences, not a capacity.
        let capacity = if target.element == "P" && target.charge == 0 {
            u32::MAX
        } else {
            crate::templates::capacity(target)
        };
        return Some(crate::templates::valence_label(
            &target.element,
            capacity,
            crate::templates::valence(result, id),
        ));
    } else {
        return None;
    };
    Some(format!("{} {reason}", target.element))
}

/// A short label, shown beside the pointer, for why a ring or template
/// cannot attach at `p`. The full reason is the placement's error message.
pub fn attachment_label(doc: &Document, p: Point, radius: f32) -> String {
    if radius > 0. {
        if let Some(atom) = doc.nearest(p, radius).and_then(|id| doc.atom(id)) {
            return crate::templates::blocked(doc, atom)
                .unwrap_or_else(|| format!("Can't attach to this {}", atom.element));
        }
        if nearest_bond(doc, p, radius).is_some() {
            return "Can't fuse to this bond".into();
        }
    }
    "Can't place the ring here".into()
}

/// Why a ring cannot be placed: the full message for the status bar, a
/// short label for beside the pointer, and the rejected ring's vertices and
/// blocking atom when they are known.
#[derive(Debug, Clone, PartialEq)]
pub struct RingRejection {
    pub message: &'static str,
    pub label: String,
    pub outline: Vec<Point>,
    pub atom: Option<u64>,
}
impl RingRejection {
    fn new(message: &'static str, label: impl Into<String>) -> Self {
        Self {
            message,
            label: label.into(),
            outline: vec![],
            atom: None,
        }
    }
}

/// Attach at an atom/bond, optionally using a drag to choose the ring's side.
pub fn ring_oriented(
    doc: &mut Document,
    p: Point,
    size: u8,
    aromatic: bool,
    radius: f32,
    direction: Option<Point>,
) -> Result<Vec<u64>, &'static str> {
    let (result, ids) =
        ring_placement(doc, p, size, aromatic, radius, direction).map_err(|r| r.message)?;
    *doc = result;
    Ok(ids)
}

const INVALID_RING_GEOMETRY: &str = "Invalid ring attachment geometry.";

/// The clicked point, clamped ring size, hit radius and optional drag that
/// every ring_placement stage reads.
#[derive(Clone, Copy)]
struct RingRequest {
    p: Point,
    n: usize,
    radius: f32,
    direction: Option<Point>,
}

/// The drawing `ring_oriented` would produce. The live preview and commit
/// both build it, so a rejected preview matches the rejected click.
pub fn ring_placement(
    doc: &Document,
    p: Point,
    size: u8,
    aromatic: bool,
    radius: f32,
    direction: Option<Point>,
) -> Result<(Document, Vec<u64>), RingRejection> {
    check_ring_request(p, radius, direction)?;
    let n = size.clamp(3, 8) as usize;
    let request = RingRequest {
        p,
        n,
        radius,
        direction,
    };
    if aromatic && n == 6 {
        return aromatic_hexagon(doc, p, radius, direction);
    }
    let atom = doc.nearest(p, radius);
    let bond = if atom.is_none() {
        nearest_bond(doc, p, radius)
    } else {
        None
    };
    // Both the live preview and commit build the same candidate. Nothing is
    // written back until its attachment chemistry and vertices are checked.
    let mut result = doc.clone();
    let mut ids = vec![];
    if let Some(index) = bond {
        fused_ring_vertices(doc, request, index, &mut result, &mut ids)?;
    } else {
        attached_ring_vertices(doc, request, atom, &mut result, &mut ids);
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
        result.add_bond(a, b, if aromatic { 4 } else { 1 }, "plain");
    }
    if aromatic {
        for id in &ids {
            if let Some(atom) = result.atom_mut(*id) {
                atom.aromatic = true;
            }
        }
    }
    check_ring_candidate(doc, result, ids)
}

/// Reject non-finite or negative input before any geometry is built.
fn check_ring_request(
    p: Point,
    radius: f32,
    direction: Option<Point>,
) -> Result<(), RingRejection> {
    if !p.x.is_finite()
        || !p.y.is_finite()
        || !radius.is_finite()
        || radius < 0.
        || direction.is_some_and(|p| !p.x.is_finite() || !p.y.is_finite())
    {
        return Err(RingRejection::new(
            INVALID_RING_GEOMETRY,
            "Invalid ring geometry",
        ));
    }
    Ok(())
}

/// The aromatic six-ring, built as a benzene template with its circle.
fn aromatic_hexagon(
    doc: &Document,
    p: Point,
    radius: f32,
    direction: Option<Point>,
) -> Result<(Document, Vec<u64>), RingRejection> {
    let drawing = crate::rings::Drawing {
        preset: crate::rings::Preset::Benzene,
        length: crate::style::DEFAULT.bond_length_world,
        alternate: false,
        connect: false,
    };
    // Ring construction historically accepts zero to disable snapping.
    // Keep that convention here without relaxing the template API's
    // positive-radius contract or accidentally hitting an existing label.
    let empty = Document::default();
    let base = if radius == 0. { &empty } else { doc };
    let (mut result, ids) = drawing
        .place(base, p, direction, if radius == 0. { 1. } else { radius })
        .map_err(|message| {
            let mut rejection = RingRejection::new(message, attachment_label(doc, p, radius));
            rejection.atom = doc.nearest(p, radius).filter(|_| radius > 0.);
            rejection
        })?;
    let mut circles: Vec<_> = crate::aromatic::circles(base)
        .into_iter()
        .map(|c| c.atoms)
        .collect();
    circles.push(ids.clone());
    crate::templates::show_circles(&mut result, &circles);
    Ok(if radius == 0. {
        let mut doc = doc.clone();
        let ids = append(&mut doc, &result, Point::default());
        (doc, ids)
    } else {
        (result, ids)
    })
}

/// Add the new vertices of a ring fused to the bond at `index`, on the
/// dragged side or the less crowded one.
fn fused_ring_vertices(
    doc: &Document,
    request: RingRequest,
    index: usize,
    result: &mut Document,
    ids: &mut Vec<u64>,
) -> Result<(), RingRejection> {
    let RingRequest {
        n,
        radius,
        direction,
        ..
    } = request;
    let Some(b) = doc.bonds.get(index).cloned() else {
        return Err(RingRejection::new(
            "The attachment bond is no longer available.",
            "The bond is unavailable",
        ));
    };
    let (Some(a), Some(z)) = (doc.atom(b.a), doc.atom(b.b)) else {
        return Err(RingRejection::new(
            "The attachment bond has missing atoms.",
            "The bond is unavailable",
        ));
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
    let appearance = b.validate_appearance().is_ok();
    if !appearance || b.stereo.is_some() || !b.stereo_atoms.is_empty() {
        return Err(RingRejection {
            outline: points,
            ..RingRejection::new(
                "Choose a supported bond appearance without assigned stereochemistry.",
                if appearance {
                    "The bond has stereochemistry"
                } else {
                    "Unsupported bond style"
                },
            )
        });
    }
    ids.extend([b.a, b.b]);
    for p in points.iter().skip(2) {
        ids.push(result.add_atom("C", *p));
    }
    Ok(())
}

/// Add the vertices of a ring attached at `atom`, or free-standing at `p`.
fn attached_ring_vertices(
    doc: &Document,
    request: RingRequest,
    atom: Option<u64>,
    result: &mut Document,
    ids: &mut Vec<u64>,
) {
    let RingRequest {
        p,
        n,
        radius,
        direction,
    } = request;
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
            result.add_atom("C", center.offset(angle.cos() * r, angle.sin() * r))
        });
    }
}

/// Check the candidate's shared atoms, edge length, overlaps, reactions
/// and validity, rejecting with the ring's outline.
fn check_ring_candidate(
    doc: &Document,
    mut result: Document,
    ids: Vec<u64>,
) -> Result<(Document, Vec<u64>), RingRejection> {
    let outline: Vec<_> = ids
        .iter()
        .filter_map(|id| result.atom(*id).map(|a| a.position))
        .collect();
    let reject = |message, label: &str, atom| RingRejection {
        message,
        label: label.into(),
        outline: outline.clone(),
        atom,
    };
    for id in &ids {
        if let Some(target) = doc.atom(*id)
            && let Some(label) = shared_atom_label(doc, &result, target)
        {
            return Err(reject(
                "This atom has no available valence, or has protected hydrogens, labels or stereochemistry.",
                &label,
                Some(*id),
            ));
        }
    }
    let missing = || {
        reject(
            "The ring has a missing atom.",
            "Invalid ring geometry",
            None,
        )
    };
    let [first, second, ..] = ids.as_slice() else {
        return Err(reject(
            "The ring has a missing edge.",
            "Invalid ring geometry",
            None,
        ));
    };
    let a = result.atom(*first).ok_or_else(missing)?;
    let b = result.atom(*second).ok_or_else(missing)?;
    let length = a.position.distance(b.position);
    if !length.is_finite() || length < 0.001 {
        return Err(reject(
            "Choose an attachment with nonzero bond lengths.",
            "The bond has no length",
            None,
        ));
    }
    // Use the aromatic fusion planner's scale-relative proximity threshold,
    // independent of zoom/hit radius. Regular rings conservatively reject an
    // unshared coincident vertex instead of merging arbitrary existing atoms.
    let tolerance = (length * (5. / 75.)).max(0.01);
    for added in result.atoms.iter().skip(doc.atoms.len()) {
        if let Some(old) = doc
            .atoms
            .iter()
            .find(|old| added.position.distance(old.position) <= tolerance)
        {
            return Err(reject(
                "The ring would overlap an existing atom. Choose another position or side.",
                "Overlaps an existing atom",
                Some(old.id),
            ));
        }
    }
    result.reconcile_molecule_groups();
    crate::reactions::reconcile(&mut result).map_err(|_| {
        reject(
            "The ring would create incompatible reaction participants.",
            "Would mix reaction participants",
            None,
        )
    })?;
    result
        .validate()
        .map_err(|_| reject(INVALID_RING_GEOMETRY, "Invalid ring geometry", None))?;
    Ok((result, ids))
}

pub fn open_angle(anchor: Point, neighbors: &[Point]) -> f32 {
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
    let mut targets = None;
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
        let targets = targets.get_or_insert_with(|| {
            doc.bonds
                .iter()
                .filter_map(|bond| {
                    if ids.contains(&bond.a)
                        || ids.contains(&bond.b)
                        || bond.order != 1
                        || bond.display != "plain"
                    {
                        return None;
                    }
                    let (ta, tb) = doc.atom(bond.a).zip(doc.atom(bond.b))?;
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
                        return None;
                    }
                    Some((ta, tb))
                })
                .collect::<Vec<_>>()
        });
        for &(ta, tb) in targets.iter() {
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
#[path = "editing/core_tests.rs"]
mod core_tests;

#[cfg(test)]
mod ring_placement_parity_tests;

#[cfg(test)]
mod tests;
