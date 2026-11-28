//! Metric 2D drawings of supported haptic dimetal hydride geometries.
use super::Model3d;
use crate::{
    attachments::Kind,
    document::{Document, Point},
    palette::Color,
};
use std::collections::{HashMap, HashSet};

#[cfg(test)]
mod tests;

struct Ligand {
    anchor: u64,
    ring: Vec<u64>,
    methyls: Vec<u64>,
    hydrogens: Vec<(u64, u64)>,
}

struct Diagram {
    metals: [u64; 2],
    ligands: [Ligand; 2],
    bridges: Vec<u64>,
    terminals: [Vec<u64>; 2],
    ids: HashSet<u64>,
}

fn ligand(
    doc: &Document,
    anchor: u64,
    metal: u64,
    adjacent: &HashMap<u64, Vec<u64>>,
) -> Option<Ligand> {
    let point = doc.atom(anchor)?;
    if point.attachment != Some(Kind::MultiCenter) || point.centroid.len() != 5 {
        return None;
    }
    if adjacent.get(&anchor)?.as_slice() != [metal] {
        return None;
    }
    let members: HashSet<_> = point.centroid.iter().copied().collect();
    if members.len() != 5
        || members
            .iter()
            .any(|id| doc.atom(*id).is_none_or(|a| a.element != "C"))
    {
        return None;
    }
    if members.iter().any(|id| {
        adjacent
            .get(id)
            .into_iter()
            .flatten()
            .filter(|id| members.contains(id))
            .count()
            != 2
    }) {
        return None;
    }
    // Stable cycle order follows bonds rather than arbitrary attachment-member order.
    let first = *members.iter().min()?;
    let mut ring = vec![first];
    while ring.len() < 5 {
        let current = *ring.last()?;
        let next = adjacent
            .get(&current)?
            .iter()
            .copied()
            .filter(|id| members.contains(id) && !ring.contains(id))
            .min()?;
        ring.push(next);
    }
    let mut methyls = Vec::new();
    let mut hydrogens = Vec::new();
    for &carbon in &ring {
        let mut methyl = None;
        for &other in adjacent.get(&carbon)? {
            if members.contains(&other) {
                continue;
            }
            let atom = doc.atom(other)?;
            match atom.element.as_str() {
                "H" if adjacent.get(&other)?.as_slice() == [carbon] => {
                    hydrogens.push((other, carbon))
                }
                "C" if methyl.is_none() => {
                    let neighbors = adjacent.get(&other)?;
                    if neighbors
                        .iter()
                        .any(|id| *id != carbon && doc.atom(*id).is_none_or(|a| a.element != "H"))
                    {
                        return None;
                    }
                    for &hydrogen in neighbors.iter().filter(|id| **id != carbon) {
                        if adjacent.get(&hydrogen)?.as_slice() != [other] {
                            return None;
                        }
                        hydrogens.push((hydrogen, other));
                    }
                    methyl = Some(other);
                }
                _ => return None,
            }
        }
        if let Some(methyl) = methyl {
            methyls.push(methyl);
        }
    }
    if !matches!(methyls.len(), 0 | 5) {
        return None;
    }
    Some(Ligand {
        anchor,
        ring,
        methyls,
        hydrogens,
    })
}

fn recognize(doc: &Document, model: &Model3d) -> Option<Diagram> {
    if model.cell.is_some()
        || model.is_stale(doc)
        || model.geometry_sources().iter().any(Option::is_none)
    {
        return None;
    }
    let ids: HashSet<_> = model.linked_ids(doc).into_iter().collect();
    let mut metals: Vec<_> = doc
        .atoms
        .iter()
        .filter(|a| ids.contains(&a.id) && crate::joining::coordination_metal(a))
        .map(|a| a.id)
        .collect();
    metals.sort_unstable();
    let metals: [u64; 2] = metals.try_into().ok()?;
    let mut adjacent = HashMap::<u64, Vec<u64>>::new();
    for bond in &doc.bonds {
        if ids.contains(&bond.a) != ids.contains(&bond.b) {
            return None;
        }
        if !ids.contains(&bond.a) {
            continue;
        }
        if !(matches!(bond.order, 1 | 2 | 4 | 5)
            || metals.contains(&bond.a) && metals.contains(&bond.b))
            || bond.stereo.is_some()
            || !bond.projection && crate::bonds::is_stereo_marker(&bond.display)
        {
            return None;
        }
        adjacent.entry(bond.a).or_default().push(bond.b);
        adjacent.entry(bond.b).or_default().push(bond.a);
    }
    if doc
        .atoms
        .iter()
        .filter(|a| ids.contains(&a.id))
        .any(|a| a.stereo.is_some() || a.stereo_group.is_some())
    {
        return None;
    }
    for neighbors in adjacent.values_mut() {
        neighbors.sort_unstable();
    }
    let mut ligands = Vec::new();
    let mut covered: HashSet<_> = metals.into_iter().collect();
    for metal in metals {
        let anchors: Vec<_> = adjacent
            .get(&metal)?
            .iter()
            .copied()
            .filter(|id| doc.atom(*id).is_some_and(|a| a.attachment.is_some()))
            .collect();
        let [anchor] = anchors.as_slice() else {
            return None;
        };
        let ligand = ligand(doc, *anchor, metal, &adjacent)?;
        for id in std::iter::once(ligand.anchor)
            .chain(ligand.ring.iter().copied())
            .chain(ligand.methyls.iter().copied())
            .chain(ligand.hydrogens.iter().map(|&(id, _)| id))
        {
            if !ids.contains(&id) || !covered.insert(id) {
                return None;
            }
        }
        ligands.push(ligand);
    }
    let mut bridges = Vec::new();
    let mut terminals: [Vec<u64>; 2] = Default::default();
    for atom in doc
        .atoms
        .iter()
        .filter(|a| ids.contains(&a.id) && !covered.contains(&a.id))
    {
        if atom.element != "H" || atom.attachment.is_some() {
            return None;
        }
        match adjacent.get(&atom.id)?.as_slice() {
            [a, b] if [*a, *b] == metals => bridges.push(atom.id),
            [a] if *a == metals[0] => terminals[0].push(atom.id),
            [a] if *a == metals[1] => terminals[1].push(atom.id),
            _ => return None,
        }
    }
    // Keep this optional layout bounded to small hydride cores. Stoichiometry
    // and terminal/bridging assignments come from the graph, not a named complex.
    if bridges.is_empty() || bridges.len() + terminals.iter().map(Vec::len).sum::<usize>() > 8 {
        return None;
    }
    for bond in doc
        .bonds
        .iter()
        .filter(|b| ids.contains(&b.a) && ids.contains(&b.b))
    {
        if bond.order == 1 || metals.contains(&bond.a) && metals.contains(&bond.b) {
            continue;
        }
        let ring_bond = ligands
            .iter()
            .any(|l| l.ring.contains(&bond.a) && l.ring.contains(&bond.b));
        let contact =
            ligands.iter().any(|l| [bond.a, bond.b].contains(&l.anchor)) && bond.order == 5;
        if !(ring_bond && matches!(bond.order, 2 | 4) || contact) {
            return None;
        }
    }
    bridges.sort_unstable();
    Some(Diagram {
        metals,
        ligands: ligands.try_into().ok()?,
        bridges,
        terminals,
        ids,
    })
}

/// Two Cp/Cp* faces with a small bridging/terminal hydride core. Other
/// ligand/connectivity/stereo cases are left alone.
pub fn can_draw_coordination_diagram(doc: &Document, model: &Model3d) -> bool {
    recognize(doc, model).is_some()
}

type Vector = [f32; 3];

fn dot(a: Vector, b: Vector) -> f32 {
    a.into_iter().zip(b).map(|(a, b)| a * b).sum()
}
fn sub([a, b, c]: Vector, [d, e, f]: Vector) -> Vector {
    [a - d, b - e, c - f]
}
fn add([a, b, c]: Vector, [d, e, f]: Vector) -> Vector {
    [a + d, b + e, c + f]
}
fn scale(a: Vector, value: f32) -> Vector {
    a.map(|a| a * value)
}
fn unit(value: Vector) -> Option<Vector> {
    let length = dot(value, value).sqrt();
    (length.is_finite() && length > 1e-5).then(|| scale(value, 1. / length))
}
fn cross([a, b, c]: Vector, [d, e, f]: Vector) -> Vector {
    [b * f - c * e, c * d - a * f, a * e - b * d]
}

struct Projection {
    points: HashMap<u64, (Point, f32)>,
    // Obscured ink, insufficient hydride spans, shortest span, then
    // nonincident clearances and deterministic view preferences.
    score: (usize, usize, f32, usize, f32),
}

fn crosses(a: Point, b: Point, c: Point, d: Point) -> bool {
    let side = |a: Point, b: Point, p: Point| (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
    side(a, b, c) * side(a, b, d) < 0. && side(c, d, a) * side(c, d, b) < 0.
}

fn projection(doc: &Document, model: &Model3d, diagram: &Diagram) -> Option<Projection> {
    // Work in the current physical view. This makes the screen direction a
    // property of XYZ/view, never of file atom order or a fixture identifier.
    let viewed: HashMap<_, _> = super::viewed(doc, model).into_iter().collect();
    let [a, b] = diagram.metals;
    let (&a_xyz, &b_xyz) = (viewed.get(&a)?, viewed.get(&b)?);
    let center = scale(add(a_xyz, b_xyz), 0.5);
    let mut axis = unit(sub(b_xyz, a_xyz))?;
    if axis.into_iter().find(|v| v.abs() > 1e-5)? < 0. {
        axis = scale(axis, -1.);
    }
    let up = unit(sub([0., 1., 0.], scale(axis, dot(axis, [0., 1., 0.]))))
        .or_else(|| unit(sub([1., 0., 0.], scale(axis, dot(axis, [1., 0., 0.])))))?;
    let front = unit(cross(axis, up))?;
    // Ordinary molecular single C--C reference, independent of this geometry.
    // The same factor applies to every coordinate; rings and spokes retain
    // their true projected shortening and calculated length ratios.
    let units = doc.drawing_style.molecular_units_per_angstrom();
    let center_2d = doc.atom(a)?.position.lerp(doc.atom(b)?.position, 0.5);
    let labels: Vec<_> = diagram
        .metals
        .iter()
        .chain(&diagram.bridges)
        .chain(diagram.terminals.iter().flatten())
        .map(|id| {
            let atom = doc.atom(*id)?;
            let (lo, hi) = crate::scene::atom_label_ink_bounds(atom, doc)?;
            Some((
                *id,
                lo.offset(-atom.position.x, -atom.position.y),
                hi.offset(-atom.position.x, -atom.position.y),
            ))
        })
        .collect::<Option<Vec<_>>>()?;
    let ink: HashMap<_, Vec<_>> = labels
        .iter()
        .map(|&(id, _, _)| {
            let atom = doc.atom(id)?;
            Some((
                id,
                crate::scene::atom_label_ink_boxes(atom, doc)
                    .into_iter()
                    .map(|(lo, hi)| {
                        (
                            lo.offset(-atom.position.x, -atom.position.y),
                            hi.offset(-atom.position.x, -atom.position.y),
                        )
                    })
                    .collect(),
            ))
        })
        .collect::<Option<_>>()?;
    let margin = doc.drawing_style.world(doc.drawing_style.margin_width_pt);
    let minimum_visible = 2. * doc.drawing_style.line_width();
    let mut best: Option<Projection> = None;
    // Choose a physical tilt with open ligand faces. Score the resulting
    // fixed-scale drawing against rendered glyph extents; do not enlarge it.
    for degrees in [30_f32, 35., 40., 45., 50., 55., 60., 65., 70., 75., 80.] {
        let (sin, cos) = degrees.to_radians().sin_cos();
        for step in -12_i32..12 {
            let (s, c) = (step as f32 * 15.).to_radians().sin_cos();
            let vertical = add(scale(up, c), scale(front, s));
            let toward = add(scale(front, c), scale(up, -s));
            let horizontal = add(scale(axis, sin), scale(toward, cos));
            let depth = cross(horizontal, vertical);
            let points: HashMap<_, _> = viewed
                .iter()
                .map(|(&id, &xyz)| {
                    let xyz = sub(xyz, center);
                    (
                        id,
                        (
                            Point::new(dot(xyz, horizontal) * units, -dot(xyz, vertical) * units),
                            dot(xyz, depth) * units,
                        ),
                    )
                })
                .collect();
            let mut face = 1_f32;
            for ligand in &diagram.ligands {
                let ring: Vec<_> = ligand
                    .ring
                    .iter()
                    .map(|id| points.get(id).map(|p| p.0))
                    .collect::<Option<_>>()?;
                let n = ring.len() as f32;
                let center = ring
                    .iter()
                    .fold(Point::default(), |sum, p| sum.offset(p.x / n, p.y / n));
                let (xx, xy, yy) = ring.iter().fold((0., 0., 0.), |(xx, xy, yy), p| {
                    let (x, y) = (p.x - center.x, p.y - center.y);
                    (xx + x * x, xy + x * y, yy + y * y)
                });
                let spread = ((xx - yy).powi(2) + 4. * xy * xy).sqrt();
                let aspect = ((xx + yy - spread).max(0.) / (xx + yy + spread)).sqrt();
                face = face.min(aspect);
            }
            if face < 0.45 {
                continue;
            }
            if points
                .values()
                .any(|(p, z)| !p.is_finite() || !z.is_finite())
            {
                continue;
            }
            // Roll the rigid view in the drawing plane as well: upright
            // glyphs have different width and height, so a diagonal/vertical
            // metal axis can expose a bond hidden by a horizontal Ru label.
            for roll in -6_i32..=6 {
                let (sin, cos) = (roll as f32 * 15.).to_radians().sin_cos();
                let mut points = points.clone();
                for (p, _) in points.values_mut() {
                    *p = Point::new(p.x * cos - p.y * sin, p.x * sin + p.y * cos);
                }
                for ligand in &diagram.ligands {
                    let (at, z) =
                        ligand
                            .ring
                            .iter()
                            .try_fold((Point::default(), 0.), |(sum, z), id| {
                                let (p, depth) = points.get(id)?;
                                Some((sum.offset(p.x / 5., p.y / 5.), z + depth / 5.))
                            })?;
                    points.insert(ligand.anchor, (at, z));
                }
                let placed_ink: HashMap<_, Vec<_>> = ink
                    .iter()
                    .map(|(&id, boxes)| {
                        let p = points.get(&id)?.0;
                        Some((
                            id,
                            boxes
                                .iter()
                                .map(|(lo, hi)| (lo.offset(p.x, p.y), hi.offset(p.x, p.y)))
                                .collect(),
                        ))
                    })
                    .collect::<Option<_>>()?;
                let mut label_overlaps = 0;
                let mut collisions = 0;
                for (i, &(id, lo, hi)) in labels.iter().enumerate() {
                    let at = points.get(&id)?.0;
                    for &(other, other_lo, other_hi) in labels.iter().skip(i + 1) {
                        let other = points.get(&other)?.0;
                        let width = (at.x + hi.x).min(other.x + other_hi.x)
                            - (at.x + lo.x).max(other.x + other_lo.x);
                        let height = (at.y + hi.y).min(other.y + other_hi.y)
                            - (at.y + lo.y).max(other.y + other_lo.y);
                        if width > 0. && height > 0. {
                            label_overlaps += 1;
                        }
                    }
                }
                let mut obscured_ink = label_overlaps;
                // Carbon vertices have no glyph to trim or protect them. A
                // visible ring/spoke vertex must not sit inside a core label.
                for boxes in placed_ink.values() {
                    for carbon in diagram
                        .ligands
                        .iter()
                        .flat_map(|ligand| ligand.ring.iter().chain(&ligand.methyls))
                    {
                        let p = points.get(carbon)?.0;
                        if boxes.iter().any(|(lo, hi)| {
                            p.x >= lo.x && p.x <= hi.x && p.y >= lo.y && p.y <= hi.y
                        }) {
                            obscured_ink += 1;
                        }
                    }
                }
                let mut shortest = f32::INFINITY;
                let mut insufficient = 0;
                for bond in doc
                    .bonds
                    .iter()
                    .filter(|b| diagram.ids.contains(&b.a) && diagram.ids.contains(&b.b))
                {
                    let (Some(&(a, _)), Some(&(b, _))) = (points.get(&bond.a), points.get(&bond.b))
                    else {
                        continue;
                    };
                    let hydride = diagram.bridges.contains(&bond.a)
                        || diagram.bridges.contains(&bond.b)
                        || diagram
                            .terminals
                            .iter()
                            .flatten()
                            .any(|id| *id == bond.a || *id == bond.b);
                    let length = a.distance(b);
                    if length < 1e-5 {
                        if hydride {
                            shortest = shortest.min(0.);
                            insufficient += 1;
                        }
                        collisions += 1;
                        continue;
                    }
                    let (ux, uy) = ((b.x - a.x) / length, (b.y - a.y) / length);
                    let boxes = |id| placed_ink.get(&id).map(Vec::as_slice).unwrap_or_default();
                    let start = crate::scene::label_end(a, ux, uy, boxes(bond.a), length, margin);
                    let end = crate::scene::label_end(b, -ux, -uy, boxes(bond.b), length, margin);
                    let visible = (end.x - start.x) * ux + (end.y - start.y) * uy;
                    if hydride {
                        shortest = shortest.min(visible);
                        insufficient += usize::from(visible < minimum_visible);
                    }
                    if visible <= 0. {
                        continue;
                    }
                    let ring_front = bond.order == 1
                        && diagram.ligands.iter().any(|ligand| {
                            ligand.ring.contains(&bond.a)
                                && ligand.ring.contains(&bond.b)
                                && (points.get(&bond.a).map(|p| p.1).unwrap_or_default()
                                    + points.get(&bond.b).map(|p| p.1).unwrap_or_default())
                                    / 2.
                                    > points.get(&ligand.anchor).map(|p| p.1).unwrap_or_default()
                        });
                    let half_stroke = if ring_front {
                        doc.drawing_style.world(doc.drawing_style.bold_width_pt) / 2.
                    } else {
                        doc.drawing_style.line_width() / 2.
                    };
                    for (&id, boxes) in &placed_ink {
                        if id == bond.a || id == bond.b {
                            continue;
                        }
                        // Actual stroke/glyph overlap has priority over edge
                        // visibility. The surrounding label margin is only a
                        // lower-priority preference and cannot erase an edge.
                        if boxes.iter().any(|&(lo, hi)| {
                            crate::atom_labels::segment_hits_rect(
                                start,
                                end,
                                lo.offset(-half_stroke, -half_stroke),
                                hi.offset(half_stroke, half_stroke),
                            )
                        }) {
                            obscured_ink += 1;
                        } else if boxes.iter().any(|&(lo, hi)| {
                            crate::atom_labels::segment_hits_rect(
                                start,
                                end,
                                lo.offset(-margin, -margin),
                                hi.offset(margin, margin),
                            )
                        }) {
                            collisions += 1;
                        }
                    }
                }
                if !shortest.is_finite() {
                    continue;
                }
                let mut preference = 0.01 * (0.6 - face).max(0.)
                    + (degrees - 55.).abs() * 0.00001
                    + step.unsigned_abs() as f32 * 0.000001
                    + roll.unsigned_abs() as f32 * 0.0000001;
                // A terminal H should not disappear into the bridge fan. Choose
                // a physical view where its one bond stays distinct; crossing
                // gaps must not consume that bond's short visible segment.
                for (metal, terminals) in diagram.metals.iter().zip(&diagram.terminals) {
                    let at = points.get(metal)?.0;
                    for hydrogen in terminals {
                        let end = points.get(hydrogen)?.0;
                        for bond in doc.bonds.iter().filter(|b| {
                            diagram.ids.contains(&b.a)
                                && diagram.ids.contains(&b.b)
                                && b.a != *metal
                                && b.b != *metal
                                && b.a != *hydrogen
                                && b.b != *hydrogen
                        }) {
                            if let (Some(&(a, _)), Some(&(b, _))) =
                                (points.get(&bond.a), points.get(&bond.b))
                                && crosses(at, end, a, b)
                            {
                                preference += 0.02;
                            }
                        }
                        if let Some(other) = diagram.metals.iter().find(|id| *id != metal) {
                            let other = points.get(other)?.0;
                            preference += 0.01 * ((end.x - at.x) * (other.x - at.x)).max(0.)
                                / at.distance(other).powi(2);
                        }
                    }
                }
                // A small nonincident-margin improvement must never erase
                // an otherwise visible metal--hydride contact.
                let score = (
                    obscured_ink,
                    insufficient,
                    -shortest,
                    collisions,
                    preference,
                );
                if best.as_ref().is_none_or(|best| score < best.score) {
                    for (point, _) in points.values_mut() {
                        *point = center_2d.offset(point.x, point.y);
                    }
                    best = Some(Projection { points, score });
                }
            }
        }
    }
    best
}

/// Orthographically project a recognized physical geometry at the molecular
/// drawing scale (theme bond length / 1.54 angstrom). A rigid physical tilt
/// selects directions and depth cues; calculation XYZ and bond orders stay
/// exact. The caller owns display choice, drawing snapshot and undo. Returns
/// false without mutation for unsupported topology or unavailable coordinates.
pub fn lay_out_coordination_diagram(doc: &mut Document, model: &Model3d) -> bool {
    let Some(diagram) = recognize(doc, model) else {
        return false;
    };
    let Some(mut projected) = projection(doc, model, &diagram) else {
        return false;
    };
    let mut ring_foreground = HashSet::new();
    for ligand in &diagram.ligands {
        let Some(sum) = ligand
            .ring
            .iter()
            .try_fold((Point::default(), 0.), |(sum, depth), id| {
                let (p, z) = projected.points.get(id)?;
                Some((sum.offset(p.x, p.y), depth + z))
            })
        else {
            return false;
        };
        let n = ligand.ring.len() as f32;
        projected.points.insert(
            ligand.anchor,
            (Point::new(sum.0.x / n, sum.0.y / n), sum.1 / n),
        );
        for (a, b) in ligand.ring.iter().zip(ligand.ring.iter().cycle().skip(1)) {
            let Some((_, a_depth)) = projected.points.get(a) else {
                return false;
            };
            let Some((_, b_depth)) = projected.points.get(b) else {
                return false;
            };
            if (a_depth + b_depth) / 2. > sum.1 / n {
                ring_foreground.insert(((*a).min(*b), (*a).max(*b)));
            }
        }
    }
    let core_depth = diagram
        .metals
        .iter()
        .filter_map(|id| projected.points.get(id).map(|p| p.1))
        .sum::<f32>()
        / 2.;
    let front_hydrogens: HashSet<_> = diagram
        .bridges
        .iter()
        .copied()
        .filter(|id| projected.points.get(id).is_some_and(|p| p.1 > core_depth))
        .collect();
    for atom in doc.atoms.iter_mut().filter(|a| diagram.ids.contains(&a.id)) {
        if let Some(&(position, depth)) = projected.points.get(&atom.id) {
            atom.position = position;
            atom.depth = depth;
        }
        if let Some(style) = &mut atom.text_style {
            style.color = Color::Ink;
        }
    }
    for bond in doc
        .bonds
        .iter_mut()
        .filter(|b| diagram.ids.contains(&b.a) && diagram.ids.contains(&b.b))
    {
        bond.color = Color::Ink;
        let front = ring_foreground.contains(&(bond.a.min(bond.b), bond.a.max(bond.b)))
            || front_hydrogens.contains(&bond.a)
            || front_hydrogens.contains(&bond.b);
        bond.z_order = i16::from(front);
        let ring_edge = diagram
            .ligands
            .iter()
            .any(|ligand| ligand.ring.contains(&bond.a) && ligand.ring.contains(&bond.b));
        if ring_edge || bond.order == 1 {
            // The native tilt refresher needs the complete connected outline,
            // including initially plain rear edges and retained double/aromatic
            // edges. Initial width is independent of this non-stereo marker.
            bond.projection = ring_edge;
        }
        if bond.order == 1 {
            let hydride = diagram.bridges.contains(&bond.a)
                || diagram.bridges.contains(&bond.b)
                || diagram
                    .terminals
                    .iter()
                    .flatten()
                    .any(|id| *id == bond.a || *id == bond.b);
            bond.display = if front && !hydride { "bold" } else { "plain" }.into();
        }
    }
    crate::depth_appearance::clear(doc, &diagram.ids.into_iter().collect::<Vec<_>>());
    true
}
