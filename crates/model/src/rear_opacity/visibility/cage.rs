use super::budget::{MAX_FACE, MAX_LINE_QUERIES, MAX_TRIANGLE_PAIRS, MAX_TRIANGLES, MAX_VERTICES};
use super::triangle::Triangle;
use super::vector::{V, cross, cross_xy, dot, mix_v, sub, unit, xyz};
use super::{Document, face_key};
use crate::polygon::closed_edges;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn improper_intersection(a: [V; 3], b: [V; 3], epsilon: f64) -> bool {
    for (a, b) in [
        (a.map(|p| p[0]), b.map(|p| p[0])),
        (a.map(|p| p[1]), b.map(|p| p[1])),
        (a.map(|p| p[2]), b.map(|p| p[2])),
    ] {
        let bounds = |t: [f64; 3]| {
            (
                t.into_iter().fold(f64::INFINITY, f64::min),
                t.into_iter().fold(f64::NEG_INFINITY, f64::max),
            )
        };
        let (lo, hi) = bounds(a);
        let (other_lo, other_hi) = bounds(b);
        if hi + epsilon < other_lo || other_hi + epsilon < lo {
            return false;
        }
    }
    let shared: Vec<_> = a
        .iter()
        .copied()
        .filter(|p| {
            b.iter()
                .any(|q| dot(sub(*p, *q), sub(*p, *q)).sqrt() <= epsilon)
        })
        .collect();
    let allowed = |p: V| {
        if shared
            .iter()
            .any(|q| dot(sub(p, *q), sub(p, *q)).sqrt() <= epsilon)
        {
            return true;
        }
        if let [a, b] = shared.as_slice() {
            let edge = sub(*b, *a);
            let t = dot(sub(p, *a), edge) / dot(edge, edge);
            let closest = mix_v(*a, *b, t);
            return (-epsilon..=1. + epsilon).contains(&t)
                && dot(sub(p, closest), sub(p, closest)).sqrt() <= epsilon;
        }
        false
    };
    for (target, source) in [(a, b), (b, a)] {
        let [ta, tb, tc] = target;
        let Some(normal) = unit(cross(sub(tb, ta), sub(tc, ta))) else {
            return true;
        };
        let [nx, ny, nz] = normal.map(f64::abs);
        let axis = if nx >= ny && nx >= nz {
            0
        } else if ny >= nz {
            1
        } else {
            2
        };
        let project = |p: V| match axis {
            0 => [p[1], p[2], 0.],
            1 => [p[0], p[2], 0.],
            _ => [p[0], p[1], 0.],
        };
        let [pa, pb, pc] = target.map(project);
        let edges = [(pa, pb, ta, tb), (pb, pc, tb, tc), (pc, pa, tc, ta)];
        let sign = cross_xy(pa, pb, pc).signum();
        let inside = |p: V| {
            edges.iter().all(|(a, b, x, y)| {
                sign * cross_xy(*a, *b, project(p))
                    >= -epsilon * dot(sub(*y, *x), sub(*y, *x)).sqrt()
            })
        };
        let [sa, sb, sc] = source;
        for (p, q) in [(sa, sb), (sb, sc), (sc, sa)] {
            let (dp, dq) = (dot(sub(p, ta), normal), dot(sub(q, ta), normal));
            if dp.abs() <= epsilon && dq.abs() <= epsilon {
                for end in [p, q] {
                    if inside(end) && !allowed(end) {
                        return true;
                    }
                }
                let (x, y) = (project(p), project(q));
                for (u, v, _, _) in edges {
                    let d = cross_xy([0.; 3], sub(y, x), sub(v, u));
                    if d.abs() < epsilon * epsilon {
                        continue;
                    }
                    let t = cross_xy([0.; 3], sub(u, x), sub(v, u)) / d;
                    let s = cross_xy([0.; 3], sub(u, x), sub(y, x)) / d;
                    if (0. ..=1.).contains(&t)
                        && (0. ..=1.).contains(&s)
                        && !allowed(mix_v(p, q, t))
                    {
                        return true;
                    }
                }
            } else if (dp >= -epsilon && dq <= epsilon) || (dq >= -epsilon && dp <= epsilon) {
                let d = dp - dq;
                if d.abs() <= epsilon {
                    continue;
                }
                let hit = mix_v(p, q, (dp / d).clamp(0., 1.));
                if inside(hit) && !allowed(hit) {
                    return true;
                }
            }
        }
    }
    false
}
/// Infer a shell only from a complete closed trivalent covalent component. Ordinary rings,
/// open branches, ambiguous radial order and invalid meshes remain real ink.
pub(super) fn cages(doc: &Document, queries: usize) -> Vec<Triangle> {
    let real: BTreeSet<_> = doc
        .atoms
        .iter()
        .filter(|a| a.centroid.is_empty() && doc.atom_visible(a.id))
        .map(|a| a.id)
        .collect();
    let mut adjacent: BTreeMap<u64, BTreeSet<u64>> = BTreeMap::new();
    let mut duplicate = BTreeSet::new();
    for b in &doc.bonds {
        if b.is_covalent()
            && doc.bond_visible(b.a, b.b)
            && real.contains(&b.a)
            && real.contains(&b.b)
        {
            if b.a == b.b || !adjacent.entry(b.a).or_default().insert(b.b) {
                duplicate.insert(b.a);
                duplicate.insert(b.b);
            }
            adjacent.entry(b.b).or_default().insert(b.a);
        }
    }
    let mut remaining: BTreeSet<_> = adjacent.keys().copied().collect();
    let mut out = vec![];
    while let Some(start) = remaining.first().copied() {
        let mut pending = vec![start];
        let mut ids = vec![];
        while let Some(id) = pending.pop() {
            if remaining.remove(&id) {
                ids.push(id);
                pending.extend(adjacent.get(&id).into_iter().flatten().copied());
            }
        }
        ids.sort_unstable();
        if ids.iter().any(|id| duplicate.contains(id)) {
            continue;
        }
        if let Some(shell) = shell(doc, &ids, &adjacent, queries) {
            out.extend(shell);
        }
    }
    out
}
/// Each stage returns None for an input that must stay real ink, so any
/// rejection falls back to no shell for this component.
fn shell(
    doc: &Document,
    ids: &[u64],
    adjacent: &BTreeMap<u64, BTreeSet<u64>>,
    queries: usize,
) -> Option<Vec<Triangle>> {
    let (points, origin, epsilon) = centered_points(doc, ids, adjacent)?;
    let center = [0.; 3];
    let order = radial_order(ids, adjacent, &points, center)?;
    let (faces, seen_len) = trace_faces(ids, adjacent, &order)?;
    let edges = ids.len() * 3 / 2;
    if seen_len != edges * 2 || ids.len() + faces.len() != edges + 2 {
        return None;
    }
    let keys: BTreeSet<_> = faces.iter().map(|f| face_key(f)).collect();
    if keys.len() != faces.len()
        || edges * 2 > MAX_TRIANGLES
        || (edges * 2).saturating_mul(queries) > MAX_LINE_QUERIES
    {
        return None;
    }
    let (mut triangles, counts, volume, mean) =
        face_triangles(doc, ids, &faces, &points, center, epsilon)?;
    validate_mesh(&triangles, &counts, edges, volume, mean, epsilon)?;
    for triangle in &mut triangles {
        for p in &mut triangle.vertices {
            for (v, o) in p.iter_mut().zip(origin) {
                *v += o;
            }
        }
    }
    Some(triangles)
}

/// Cage atoms moved so their centroid is the origin, plus that centroid and the
/// mesh tolerance. Rejects out-of-range sizes and non-trivalent, non-finite or
/// coincident atoms.
fn centered_points(
    doc: &Document,
    ids: &[u64],
    adjacent: &BTreeMap<u64, BTreeSet<u64>>,
) -> Option<(BTreeMap<u64, V>, V, f64)> {
    if !(4..=MAX_VERTICES).contains(&ids.len())
        || ids
            .iter()
            .any(|id| adjacent.get(id).is_none_or(|n| n.len() != 3))
    {
        return None;
    }
    let mut points: BTreeMap<_, _> = ids
        .iter()
        .map(|id| Some((*id, xyz(doc, *id)?)))
        .collect::<Option<_>>()?;
    if points.values().flatten().any(|p| !p.is_finite()) {
        return None;
    }
    let mut center = [0.; 3];
    for p in points.values() {
        for (c, p) in center.iter_mut().zip(p) {
            *c += p / ids.len() as f64;
        }
    }
    for p in points.values_mut() {
        *p = sub(*p, center);
    }
    let origin = center;
    let diameter = points
        .values()
        .map(|p| dot(*p, *p).sqrt() * 2.)
        .fold(0., f64::max);
    let epsilon = (diameter * 1e-5).max(1e-6);
    for (i, a) in ids.iter().enumerate() {
        for b in ids.iter().skip(i + 1) {
            if dot(
                sub(*points.get(a)?, *points.get(b)?),
                sub(*points.get(a)?, *points.get(b)?),
            )
            .sqrt()
                <= epsilon
            {
                return None;
            }
        }
    }
    Some((points, origin, epsilon))
}

/// Each atom's neighbors in angular order around its outward radial axis.
/// Rejects degenerate axes and ambiguous (near-equal) angles.
fn radial_order(
    ids: &[u64],
    adjacent: &BTreeMap<u64, BTreeSet<u64>>,
    points: &BTreeMap<u64, V>,
    center: V,
) -> Option<BTreeMap<u64, Vec<u64>>> {
    let mut order: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
    for id in ids {
        let radial = unit(sub(*points.get(id)?, center))?;
        let neighbors = adjacent.get(id)?;
        let first = *neighbors.first()?;
        let edge = sub(*points.get(&first)?, *points.get(id)?);
        let along = dot(edge, radial);
        let axis = unit(sub(edge, radial.map(|x| x * along)))?;
        let tangent = cross(radial, axis);
        let mut angles = vec![];
        for n in neighbors {
            let edge = sub(*points.get(n)?, *points.get(id)?);
            let x = dot(edge, axis);
            let y = dot(edge, tangent);
            if x.hypot(y) < 1e-8 {
                return None;
            }
            angles.push((y.atan2(x), *n));
        }
        angles.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (a, b) in closed_edges(&angles) {
            let d = (b.0 - a.0).rem_euclid(std::f64::consts::TAU);
            if d < 1e-5 {
                return None;
            }
        }
        order.insert(*id, angles.into_iter().map(|(_, id)| id).collect());
    }
    Some(order)
}

/// Faces traced from the radial order, with the number of directed edges
/// visited. Rejects oversized, self-touching or degenerate faces.
fn trace_faces(
    ids: &[u64],
    adjacent: &BTreeMap<u64, BTreeSet<u64>>,
    order: &BTreeMap<u64, Vec<u64>>,
) -> Option<(Vec<Vec<u64>>, usize)> {
    let mut seen = BTreeSet::new();
    let mut faces = vec![];
    for a in ids {
        for b in adjacent.get(a)? {
            if seen.contains(&(*a, *b)) {
                continue;
            }
            let first = (*a, *b);
            let (mut x, mut y) = first;
            let mut face = vec![];
            let mut vertices = BTreeSet::new();
            loop {
                if face.len() >= MAX_FACE || !vertices.insert(x) || !seen.insert((x, y)) {
                    return None;
                }
                face.push(x);
                let around = order.get(&y)?;
                let i = around.iter().position(|id| *id == x)?;
                let next = *around.get((i + 1) % around.len())?;
                x = y;
                y = next;
                if (x, y) == first {
                    break;
                }
            }
            if face.len() < 3 {
                return None;
            }
            faces.push(face);
        }
    }
    Some((faces, seen.len()))
}

type FaceTriangles = (Vec<Triangle>, BTreeMap<(u64, u64), usize>, f64, f64);

/// Checks each face for planarity, convexity and a consistent outward
/// orientation, then fans it around its centroid. Returns the triangles,
/// undirected edge counts, enclosed volume and mean edge length.
fn face_triangles(
    doc: &Document,
    ids: &[u64],
    faces: &[Vec<u64>],
    points: &BTreeMap<u64, V>,
    center: V,
    epsilon: f64,
) -> Option<FaceTriangles> {
    // Match the existing ring-paint owner rule: each inferred face carries
    // its highest boundary-bond layer, never an invented neutral layer.
    let layers: BTreeMap<_, _> = doc
        .bonds
        .iter()
        .filter(|b| ids.contains(&b.a) && ids.contains(&b.b) && b.is_covalent())
        .map(|b| ((b.a.min(b.b), b.a.max(b.b)), b.z_order))
        .collect();
    let mut counts: BTreeMap<(u64, u64), usize> = BTreeMap::new();
    let mut triangles = vec![];
    let mut volume = 0.;
    let mut mean = 0.;
    let mut orientation = 0.;
    for face in faces {
        let layer = closed_edges(face)
            .filter_map(|(a, b)| layers.get(&(a.min(b), a.max(b))))
            .copied()
            .max()?;
        for (a, b) in closed_edges(face) {
            *counts.entry((a.min(b), a.max(b))).or_default() += 1;
        }
        let mut fc = [0.; 3];
        for id in face {
            for (c, p) in fc.iter_mut().zip(*points.get(id)?) {
                *c += p / face.len() as f64;
            }
        }
        let mut normal = [0.; 3];
        let mut length = 0.;
        for (a, b) in closed_edges(face) {
            let n = cross(sub(*points.get(&a)?, fc), sub(*points.get(&b)?, fc));
            for (n, x) in normal.iter_mut().zip(n) {
                *n += x;
            }
            length += dot(
                sub(*points.get(&a)?, *points.get(&b)?),
                sub(*points.get(&a)?, *points.get(&b)?),
            )
            .sqrt();
        }
        let mut normal = unit(normal)?;
        let facing = dot(normal, sub(fc, center));
        if facing.abs() <= epsilon {
            return None;
        }
        if orientation == 0. {
            orientation = facing.signum();
        }
        if facing * orientation <= 0. {
            return None;
        }
        if facing < 0. {
            normal = normal.map(|x| -x);
        }
        length /= face.len() as f64;
        if length < 1e-6 {
            return None;
        }
        mean += length / faces.len() as f64;
        if face
            .iter()
            .filter_map(|id| points.get(id))
            .any(|p| dot(sub(*p, fc), normal).abs() > length * 0.1)
            || points
                .iter()
                .filter(|(id, _)| !face.contains(id))
                .any(|(_, p)| dot(sub(*p, fc), normal) > epsilon)
        {
            return None;
        }
        let mut sign = 0.;
        for ((a, b), c) in face
            .iter()
            .zip(face.iter().cycle().skip(1))
            .zip(face.iter().cycle().skip(2))
            .take(face.len())
        {
            let (a, b, c) = (*points.get(a)?, *points.get(b)?, *points.get(c)?);
            let turn = dot(cross(sub(b, a), sub(c, b)), normal);
            if turn.abs() < epsilon * length {
                return None;
            }
            if sign == 0. {
                sign = turn.signum();
            }
            if turn * sign <= 0. {
                return None;
            }
        }
        for (a, b) in closed_edges(face) {
            let (mut a, mut b) = (*points.get(&a)?, *points.get(&b)?);
            if dot(cross(sub(a, fc), sub(b, fc)), normal) < 0. {
                std::mem::swap(&mut a, &mut b);
            }
            let v = dot(sub(fc, center), cross(sub(a, center), sub(b, center))) / 6.;
            if v <= 0. {
                return None;
            }
            volume += v;
            triangles.push(Triangle {
                vertices: [fc, a, b],
                face: face_key(face),
                shell: *ids.first()?,
                layer,
            });
        }
    }
    Some((triangles, counts, volume, mean))
}

/// Rejects meshes that are not closed two-manifolds, enclose too little
/// volume, exceed the pair budget or intersect themselves.
fn validate_mesh(
    triangles: &[Triangle],
    counts: &BTreeMap<(u64, u64), usize>,
    edges: usize,
    volume: f64,
    mean: f64,
    epsilon: f64,
) -> Option<()> {
    if counts.len() != edges || counts.values().any(|n| *n != 2) || volume <= mean.powi(3) * 0.001 {
        return None;
    }
    if triangles
        .len()
        .saturating_mul(triangles.len().saturating_sub(1))
        / 2
        > MAX_TRIANGLE_PAIRS
    {
        return None;
    }
    for (i, a) in triangles.iter().enumerate() {
        for b in triangles.iter().skip(i + 1) {
            if improper_intersection(a.vertices, b.vertices, epsilon) {
                return None;
            }
        }
    }
    Some(())
}
