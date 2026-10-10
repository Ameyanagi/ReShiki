//! View visibility for conservative closed cages and real projected ink.
//! Geometry is recomputed from retained XYZ; RGB/frozen paint is not a camera.
use super::{Field, PathCommand, Point, Primitive, flatten, ink};
use crate::document::{Bond, Document};
use std::collections::{BTreeMap, BTreeSet};

type V = [f64; 3];
const MAX_VERTICES: usize = 256;
const MAX_FACE: usize = 24;
const MAX_INK_EDGES: usize = 100_000;
const MAX_TRIANGLES: usize = 768;
const MAX_TRIANGLE_PAIRS: usize = 500_000;
const MAX_LINE_QUERIES: usize = 1_000_000;
const MAX_INK_WORK: usize = 100_000_000;
const MAX_CROSSING_WORK: usize = 200_000;

fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: V, b: V) -> f64 {
    a.into_iter().zip(b).map(|(a, b)| a * b).sum()
}
fn cross(a: V, b: V) -> V {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn unit(v: V) -> Option<V> {
    let n = dot(v, v).sqrt();
    (n.is_finite() && n > 1e-9).then(|| v.map(|x| x / n))
}
fn xy(v: V) -> Point {
    Point::new(v[0] as f32, v[1] as f32)
}
fn xyz(doc: &Document, id: u64) -> Option<V> {
    let a = doc.atom(id)?;
    Some([a.position.x.into(), a.position.y.into(), a.depth.into()])
}
fn cross2(a: Point, b: Point, c: Point) -> f64 {
    (f64::from(b.x) - f64::from(a.x)) * (f64::from(c.y) - f64::from(a.y))
        - (f64::from(b.y) - f64::from(a.y)) * (f64::from(c.x) - f64::from(a.x))
}
fn cross_xy(a: V, b: V, c: V) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

#[derive(Clone)]
struct Triangle {
    vertices: [V; 3],
    face: Vec<u64>,
    shell: u64,
    layer: i16,
}
impl Triangle {
    fn depth(&self, p: Point) -> Option<f64> {
        let [a, b, c] = self.vertices;
        let p = [f64::from(p.x), f64::from(p.y), 0.];
        let det = cross_xy(a, b, c);
        if det.abs() < 1e-10 {
            return None;
        }
        let u = cross_xy(a, p, c) / det;
        let v = cross_xy(a, b, p) / det;
        Some(a[2] + u * (b[2] - a[2]) + v * (c[2] - a[2]))
    }
    #[cfg(test)]
    fn hidden(&self, a: Point, b: Point, za: f64, zb: f64, epsilon: f64) -> Option<(f32, f32)> {
        self.interval(a, b, Some((za, zb)), epsilon)
    }
    fn interval(
        &self,
        a: Point,
        b: Point,
        depth: Option<(f64, f64)>,
        epsilon: f64,
    ) -> Option<(f32, f32)> {
        let mut range = (0., 1.);
        let det = cross_xy(self.vertices[0], self.vertices[1], self.vertices[2]);
        if det.abs() < 1e-10 {
            return None;
        }
        let sign = det.signum();
        let [x, y, z] = self.vertices;
        for (p, q) in [(x, y), (y, z), (z, x)] {
            clip(
                &mut range,
                sign * cross_xy(p, q, [a.x.into(), a.y.into(), 0.]),
                sign * cross_xy(p, q, [b.x.into(), b.y.into(), 0.]),
            )?;
        }
        if let Some((za, zb)) = depth {
            clip(
                &mut range,
                self.depth(a)? - za - epsilon,
                self.depth(b)? - zb - epsilon,
            )?;
        }
        (range.1 - range.0 > 1e-7).then_some((range.0 as f32, range.1 as f32))
    }
}
/// Intersect an interval with a linear inequality f(t)>=0.
fn clip(range: &mut (f64, f64), a: f64, b: f64) -> Option<()> {
    if !a.is_finite() || !b.is_finite() {
        return None;
    }
    let d = b - a;
    if d.abs() < 1e-14 {
        if a < 0. {
            return None;
        }
    } else if d > 0. {
        range.0 = range.0.max(-a / d);
    } else {
        range.1 = range.1.min(-a / d);
    }
    (range.0 <= range.1 && range.1 >= 0. && range.0 <= 1.).then_some(())
}

struct Ink {
    owners: Vec<u64>,
    layer: i16,
    contours: Vec<Vec<Point>>,
    field: Field,
}
impl Ink {
    fn winding(&self, p: V) -> i32 {
        let mut winding = 0_i32;
        for contour in &self.contours {
            for (&a, &b) in contour
                .iter()
                .zip(contour.iter().cycle().skip(1))
                .take(contour.len())
            {
                if f64::from(a.y) <= p[1]
                    && f64::from(b.y) > p[1]
                    && cross_xy(
                        [a.x.into(), a.y.into(), 0.],
                        [b.x.into(), b.y.into(), 0.],
                        p,
                    ) > 0.
                {
                    winding += 1;
                }
                if f64::from(a.y) > p[1]
                    && f64::from(b.y) <= p[1]
                    && cross_xy(
                        [a.x.into(), a.y.into(), 0.],
                        [b.x.into(), b.y.into(), 0.],
                        p,
                    ) < 0.
                {
                    winding -= 1;
                }
            }
        }
        winding
    }
    fn intervals(
        &self,
        a: Point,
        b: Point,
        za: f64,
        zb: f64,
        layer: i16,
        epsilon: f64,
    ) -> Vec<(f32, f32)> {
        if self.layer < layer {
            return vec![];
        }
        let dx = f64::from(b.x) - f64::from(a.x);
        let dy = f64::from(b.y) - f64::from(a.y);
        let mut events = vec![];
        for contour in &self.contours {
            for (&p, &q) in contour
                .iter()
                .zip(contour.iter().cycle().skip(1))
                .take(contour.len())
            {
                let ex = f64::from(q.x) - f64::from(p.x);
                let ey = f64::from(q.y) - f64::from(p.y);
                let det = dx * ey - dy * ex;
                let px = f64::from(p.x) - f64::from(a.x);
                let py = f64::from(p.y) - f64::from(a.y);
                let sy = dx * py - dy * px;
                let ty = sy + det;
                // Half-open transverse crossings group vertex/tangent events
                // without repeatedly scanning every contour for each interval.
                if !((sy <= 0. && ty > 0.) || (ty <= 0. && sy > 0.)) {
                    continue;
                }
                let t = (px * ey - py * ex) / det;
                if t > 0. && t < 1. {
                    events.push((t, if det > 0. { -1 } else { 1 }));
                }
            }
        }
        events.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut groups: Vec<(f64, i32)> = vec![];
        for (t, change) in events {
            if let Some(last) = groups.last_mut()
                && (last.0 - t).abs() < 1e-10
            {
                last.1 += change;
            } else {
                groups.push((t, change));
            }
        }
        let first = groups.first().map_or(1., |e| e.0);
        let at = |t: f64| [f64::from(a.x) + t * dx, f64::from(a.y) + t * dy, 0.];
        let mut winding = self.winding(at(first / 2.));
        let mut low = 0.;
        let mut out = vec![];
        groups.push((1., 0));
        for (high, change) in groups {
            if winding != 0 {
                let mut range = (low, high);
                if (self.layer != layer
                    || clip(
                        &mut range,
                        f64::from(self.field.at(a)) - za - epsilon,
                        f64::from(self.field.at(b)) - zb - epsilon,
                    )
                    .is_some())
                    && range.1 - range.0 > 1e-7
                {
                    out.push((range.0 as f32, range.1 as f32));
                }
            }
            winding += change;
            low = high;
        }
        out
    }
}

#[derive(Default)]
pub(super) struct Visibility {
    triangles: Vec<Triangle>,
    ink: Vec<Ink>,
    epsilon: f64,
    rim_atoms: BTreeMap<u64, (u64, Point)>,
    rim_bonds: BTreeMap<(u64, u64), (u64, Point, Point)>,
    atom_shells: BTreeMap<u64, u64>,
}
impl Visibility {
    pub(super) fn new(doc: &Document) -> Self {
        // Flat 2D drawings retain their exact paint order and opaque ink.
        let lo = doc
            .atoms
            .iter()
            .map(|a| a.depth)
            .fold(f32::INFINITY, f32::min);
        let hi = doc
            .atoms
            .iter()
            .map(|a| a.depth)
            .fold(f32::NEG_INFINITY, f32::max);
        if hi - lo < 1e-6 {
            return Self::default();
        }
        let epsilon = f64::from(doc.drawing_style.bond_length_world.max(1.)) * 0.0001;
        let rear = rear_ids(doc);
        let mut ink = vec![];
        let mut budget = MAX_INK_EDGES;
        let sources = crate::scene::occlusion_ink(doc);
        // Conservative preflight estimates, not a formal operation counter.
        // Include actual generated dash/rail pieces: one long dashed bond is
        // not merely one query. Crossing cuts can proliferate runtime pieces.
        let source_queries = sources
            .iter()
            .filter(|(owners, _, _, _)| owners.iter().any(|id| rear.contains(id)))
            .map(|(_, _, _, parts)| parts.len())
            .sum::<usize>()
            .saturating_mul(4)
            .saturating_add(rear.len());
        let queries = estimated_queries(doc, &rear)
            .max(source_queries)
            .saturating_add(crossing_queries(doc, &rear));
        let triangles = cages(doc, queries);
        let (rim_atoms, rim_bonds, atom_shells) = rims(doc, &triangles, epsilon);
        for (owners, layer, field, parts) in sources {
            let mut contours = vec![];
            for part in parts {
                let part = match part {
                    Primitive::Line(a, b, w) => Primitive::Polygon(super::capsule(a, b, w)),
                    p => ink::single_paint(p),
                };
                let paths = match part {
                    Primitive::Polygon(p) => vec![p],
                    Primitive::Path {
                        commands,
                        filled: true,
                        ..
                    } => flatten(&commands),
                    _ => vec![],
                };
                for path in paths {
                    if path.len() > budget {
                        return Self::default();
                    }
                    budget -= path.len();
                    contours.push(path);
                }
            }
            if !contours.is_empty() {
                ink.push(Ink {
                    owners,
                    layer,
                    field,
                    contours,
                });
            }
        }
        let work = ink
            .iter()
            .map(|i| {
                let edges = i.contours.iter().map(Vec::len).sum::<usize>();
                edges.saturating_mul((usize::BITS - edges.max(1).leading_zeros()) as usize + 3)
            })
            .sum::<usize>();
        if triangles.len().saturating_mul(queries) > MAX_LINE_QUERIES
            || work.saturating_mul(queries) > MAX_INK_WORK
        {
            return Self::default();
        }
        Self {
            triangles,
            ink,
            epsilon,
            rim_atoms,
            rim_bonds,
            atom_shells,
        }
    }
    pub(super) fn is_empty(&self) -> bool {
        self.triangles.is_empty() && self.ink.is_empty()
    }
    pub(super) fn rim_anchor(&self, id: u64) -> Option<Point> {
        self.rim_atoms.get(&id).map(|(_, p)| *p)
    }
    pub(super) fn hidden(
        &self,
        a: Point,
        b: Point,
        field: Field,
        owners: &[u64],
        layer: i16,
    ) -> Vec<(f32, f32)> {
        self.hidden_with_support(a, b, field, owners, layer, None)
    }
    pub(super) fn hidden_with_support(
        &self,
        a: Point,
        b: Point,
        field: Field,
        owners: &[u64],
        layer: i16,
        support: Option<&[u64]>,
    ) -> Vec<(f32, f32)> {
        self.hidden_with_policy(a, b, field, owners, layer, (support, None))
    }
    /// An exposed anchor owns its original shared corner, but unrelated
    /// foreground ink must still occlude that corner's restored opaque paint.
    pub(super) fn external_cap_hidden(
        &self,
        a: Point,
        b: Point,
        field: Field,
        bond: &Bond,
        anchor: u64,
    ) -> Vec<(f32, f32)> {
        let shell = self.rim_atoms.get(&anchor).map(|(shell, _)| *shell);
        self.hidden_with_policy(a, b, field, &[bond.a, bond.b], bond.z_order, (None, shell))
    }
    fn hidden_with_policy(
        &self,
        a: Point,
        b: Point,
        field: Field,
        owners: &[u64],
        layer: i16,
        (support, exempt_shell): (Option<&[u64]>, Option<u64>),
    ) -> Vec<(f32, f32)> {
        let za = f64::from(field.at(a));
        let zb = f64::from(field.at(b));
        let mut ranges = vec![];
        let support = support.map(face_key);
        // A genuine geometric rim is exposed to its own shell. Its graphical
        // stroke thickness has no separate XYZ and must stay solid in full.
        let rim_shell = if exempt_shell.is_some() {
            exempt_shell
        } else if let [first, last] = owners
            && support.is_none()
        {
            self.rim_bonds
                .get(&((*first).min(*last), (*first).max(*last)))
                .filter(|(_, x, y)| {
                    on_segment(a, *x, *y, self.epsilon) && on_segment(b, *x, *y, self.epsilon)
                })
                .map(|(shell, _, _)| *shell)
        } else if let [id] = owners
            && a == b
        {
            self.rim_atoms
                .get(id)
                .filter(|(_, p)| p.distance(a) <= self.epsilon as f32)
                .map(|(shell, _)| *shell)
        } else {
            None
        };
        for triangle in &self.triangles {
            if rim_shell == Some(triangle.shell) {
                continue;
            }
            if support.as_ref().is_some_and(|key| *key == triangle.face) {
                continue;
            }
            let same_shell = owners
                .iter()
                .all(|id| self.atom_shells.get(id) == Some(&triangle.shell));
            if triangle.layer < layer {
                continue;
            }
            let depth = (same_shell || triangle.layer == layer).then_some((za, zb));
            if let Some(r) = triangle.interval(a, b, depth, self.epsilon) {
                ranges.push(r);
            }
        }
        for ink in &self.ink {
            if rim_shell.is_some_and(|shell| {
                ink.owners
                    .iter()
                    .all(|id| self.atom_shells.get(id) == Some(&shell))
            }) {
                continue;
            }
            if !ink.owners.iter().any(|id| owners.contains(id)) {
                ranges.extend(ink.intervals(a, b, za, zb, layer, self.epsilon));
            }
        }
        merge(ranges)
    }
    pub(super) fn occluded(&self, p: Point, z: f32, owners: &[u64], layer: i16) -> bool {
        !self
            .hidden(p, p, Field::Constant(z), owners, layer)
            .is_empty()
    }
    pub(super) fn masks(&self, field: Field, owners: &[u64]) -> Vec<Vec<Point>> {
        let support = face_key(owners);
        self.triangles
            .iter()
            .filter(|triangle| {
                triangle.face != support
                    && (triangle.layer >= 0
                        || owners
                            .iter()
                            .all(|id| self.atom_shells.get(id) == Some(&triangle.shell)))
            })
            .filter_map(|triangle| {
                let p: Vec<_> = triangle.vertices.iter().copied().map(xy).collect();
                let clipped = super::clip_values(&p, |p| {
                    triangle.depth(p).unwrap_or(f64::NEG_INFINITY)
                        - f64::from(field.at(p))
                        - self.epsilon
                });
                (clipped.len() >= 3).then_some(clipped)
            })
            .collect()
    }
    #[cfg(test)]
    pub(super) fn support_ranges(
        &self,
        a: Point,
        b: Point,
        field: Field,
        support: &[u64],
    ) -> Vec<(f32, f32)> {
        let key = face_key(support);
        merge(
            self.triangles
                .iter()
                .filter(|t| t.face == key)
                .filter_map(|t| {
                    t.hidden(a, b, field.at(a).into(), field.at(b).into(), self.epsilon)
                })
                .collect(),
        )
    }
    #[cfg(test)]
    pub(super) fn face_count(&self) -> usize {
        self.triangles
            .iter()
            .map(|t| (&t.face, t.shell))
            .collect::<BTreeSet<_>>()
            .len()
    }
    #[cfg(test)]
    pub(super) fn raw_atom_hits(
        &self,
        p: Point,
        z: f32,
        owners: &[u64],
    ) -> (Vec<Vec<u64>>, Vec<Vec<u64>>) {
        let triangles = self
            .triangles
            .iter()
            .filter(|t| t.hidden(p, p, z.into(), z.into(), self.epsilon).is_some())
            .map(|t| t.face.clone())
            .collect();
        let ink = self
            .ink
            .iter()
            .filter(|i| {
                !i.owners.iter().any(|id| owners.contains(id))
                    && !i
                        .intervals(p, p, z.into(), z.into(), 0, self.epsilon)
                        .is_empty()
            })
            .map(|i| i.owners.clone())
            .collect();
        (triangles, ink)
    }
}
fn on_segment(p: Point, a: Point, b: Point, epsilon: f64) -> bool {
    let (x, y) = (
        f64::from(b.x) - f64::from(a.x),
        f64::from(b.y) - f64::from(a.y),
    );
    let length = x.hypot(y);
    if length < epsilon {
        return p.distance(a) <= epsilon as f32;
    }
    if cross2(a, b, p).abs() > epsilon * length {
        return false;
    }
    let along = (f64::from(p.x) - f64::from(a.x)) * x + (f64::from(p.y) - f64::from(a.y)) * y;
    along >= -epsilon * length && along <= length * length + epsilon * length
}
type RimAtoms = BTreeMap<u64, (u64, Point)>;
type RimBonds = BTreeMap<(u64, u64), (u64, Point, Point)>;
fn rims(
    doc: &Document,
    triangles: &[Triangle],
    epsilon: f64,
) -> (RimAtoms, RimBonds, BTreeMap<u64, u64>) {
    let mut groups: BTreeMap<u64, BTreeSet<u64>> = BTreeMap::new();
    for triangle in triangles {
        groups
            .entry(triangle.shell)
            .or_default()
            .extend(&triangle.face);
    }
    let mut atoms = BTreeMap::new();
    let mut bonds = BTreeMap::new();
    let mut memberships = BTreeMap::new();
    for (shell, ids) in groups {
        let mut points: Vec<_> = ids
            .iter()
            .filter_map(|id| doc.atom(*id).map(|a| (a.position, *id)))
            .collect();
        points.sort_by(|a, b| a.0.x.total_cmp(&b.0.x).then(a.0.y.total_cmp(&b.0.y)));
        let mut low: Vec<(Point, u64)> = vec![];
        let mut high: Vec<(Point, u64)> = vec![];
        for &p in &points {
            while last_turn(&low, p.0).is_some_and(|turn| turn <= 0.) {
                low.pop();
            }
            low.push(p);
        }
        for &p in points.iter().rev() {
            while last_turn(&high, p.0).is_some_and(|turn| turn <= 0.) {
                high.pop();
            }
            high.push(p);
        }
        low.pop();
        high.pop();
        low.extend(high);
        if low.len() < 3 {
            continue;
        }
        for (p, id) in &points {
            memberships.insert(*id, shell);
            if low
                .iter()
                .zip(low.iter().cycle().skip(1))
                .take(low.len())
                .any(|(a, b)| on_segment(*p, a.0, b.0, epsilon))
            {
                atoms.insert(*id, (shell, *p));
            }
        }
        for bond in doc
            .bonds
            .iter()
            .filter(|b| ids.contains(&b.a) && ids.contains(&b.b))
        {
            let (Some(a), Some(b)) = (doc.atom(bond.a), doc.atom(bond.b)) else {
                continue;
            };
            let (a, b) = (a.position, b.position);
            if low
                .iter()
                .zip(low.iter().cycle().skip(1))
                .take(low.len())
                .any(|(x, y)| on_segment(a, x.0, y.0, epsilon) && on_segment(b, x.0, y.0, epsilon))
            {
                bonds.insert((bond.a.min(bond.b), bond.a.max(bond.b)), (shell, a, b));
            }
        }
    }
    (atoms, bonds, memberships)
}
fn last_turn(stack: &[(Point, u64)], p: Point) -> Option<f64> {
    let mut tail = stack.iter().rev();
    let b = tail.next()?.0;
    let a = tail.next()?.0;
    Some(cross2(a, b, p))
}
fn face_key(ids: &[u64]) -> Vec<u64> {
    let reverse: Vec<_> = ids.iter().rev().copied().collect();
    [ids, reverse.as_slice()]
        .into_iter()
        .flat_map(|p| {
            (0..p.len()).map(move |i| {
                p.iter()
                    .cycle()
                    .skip(i)
                    .take(p.len())
                    .copied()
                    .collect::<Vec<_>>()
            })
        })
        .min()
        .unwrap_or_default()
}
pub(super) fn merge(mut ranges: Vec<(f32, f32)>) -> Vec<(f32, f32)> {
    ranges.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    let mut out: Vec<(f32, f32)> = vec![];
    for (a, b) in ranges {
        let a = a.clamp(0., 1.);
        let b = b.clamp(0., 1.);
        if b - a <= 1e-7 {
            continue;
        }
        if let Some(last) = out.last_mut()
            && a <= last.1 + 1e-6
        {
            last.1 = last.1.max(b);
        } else {
            out.push((a, b));
        }
    }
    out
}

fn rear_ids(doc: &Document) -> BTreeSet<u64> {
    let mut ids: BTreeSet<_> = doc
        .depth_appearance
        .iter()
        .flat_map(|s| {
            s.atoms
                .iter()
                .filter(|id| s.rear_opacity_for(**id) < 1.)
                .copied()
        })
        .collect();
    // Match Paint's ownership for nonchemical centroids, without charging
    // opaque RGB-owner members as visibility queries.
    for atom in doc.atoms.iter().filter(|a| !a.centroid.is_empty()) {
        if atom.centroid.iter().any(|id| ids.contains(id)) {
            ids.insert(atom.id);
        } else {
            ids.remove(&atom.id);
        }
    }
    ids
}
fn estimated_queries(doc: &Document, rear: &BTreeSet<u64>) -> usize {
    let active = |ids: &[u64]| ids.iter().any(|id| rear.contains(id));
    let bonds = doc.bonds.iter().filter(|b| active(&[b.a, b.b])).count();
    let fills = doc.ring_fills.iter().filter(|f| active(&f.atoms)).count();
    // Use original pre-knockout curves. Query ownership includes a managed
    // foreground bond crossing an otherwise opaque ring, not just ring IDs.
    // The uncut helper never constructs crossing Paint, avoiding recursion.
    let arcs = crate::ring_arcs::uncut(doc);
    let mut work = MAX_CROSSING_WORK;
    let mut curves = 0_usize;
    for (part, ids) in arcs.primitives.iter().zip(&arcs.owners) {
        let Some(cost) = ring_queries(doc, rear, part, ids, &mut work) else {
            return MAX_LINE_QUERIES + 1;
        };
        curves = curves.saturating_add(cost);
    }
    for circle in crate::aromatic::circles(doc)
        .into_iter()
        .filter(|c| !arcs.intersects(c))
    {
        for part in circle.graphic().parts() {
            let part = Primitive::Path {
                commands: part.commands,
                style: part.style,
                filled: part.filled,
            };
            let Some(cost) = ring_queries(doc, rear, &part, &circle.atoms, &mut work) else {
                return MAX_LINE_QUERIES + 1;
            };
            curves = curves.saturating_add(cost);
        }
    }
    rear.len()
        .saturating_add(bonds.saturating_mul(4))
        .saturating_add(fills.saturating_mul(128))
        .saturating_add(curves)
}
fn ring_queries(
    doc: &Document,
    rear: &BTreeSet<u64>,
    part: &Primitive,
    ids: &[u64],
    work: &mut usize,
) -> Option<usize> {
    let count = curve_queries(part);
    let owned = ids.iter().any(|id| rear.contains(id));
    let Primitive::Path { commands, .. } = part else {
        return Some(MAX_LINE_QUERIES + 1);
    };
    let Some((lo, hi)) = commands.iter().flat_map(PathCommand::iter_points).fold(
        None::<(Point, Point)>,
        |bounds, p| {
            Some(match bounds {
                None => (p, p),
                Some((lo, hi)) => (
                    Point::new(lo.x.min(p.x), lo.y.min(p.y)),
                    Point::new(hi.x.max(p.x), hi.y.max(p.y)),
                ),
            })
        },
    ) else {
        return Some(0);
    };
    let mut crossings = 0_usize;
    for bond in &doc.bonds {
        if *work == 0 {
            return None;
        }
        *work -= 1;
        if !owned && !rear.contains(&bond.a) && !rear.contains(&bond.b) {
            continue;
        }
        let (Some(a), Some(b)) = (doc.atom(bond.a), doc.atom(bond.b)) else {
            continue;
        };
        let (a, b) = (a.position, b.position);
        if doc.bond_visible(bond.a, bond.b)
            && a.x.min(b.x) <= hi.x
            && a.x.max(b.x) >= lo.x
            && a.y.min(b.y) <= hi.y
            && a.y.max(b.y) >= lo.y
        {
            crossings += 1;
        }
    }
    Some(count.saturating_mul(crossings.saturating_add(if owned { 2 } else { 0 })))
}
fn curve_queries(part: &Primitive) -> usize {
    match part {
        Primitive::Path {
            commands,
            filled: false,
            ..
        } => flatten(commands)
            .iter()
            .map(|p| p.len().saturating_sub(1))
            .sum::<usize>(),
        _ => 1,
    }
}
fn crossing_queries(doc: &Document, rear: &BTreeSet<u64>) -> usize {
    let mut segments: Vec<_> = doc
        .bonds
        .iter()
        .filter_map(|b| {
            doc.bond_visible(b.a, b.b).then_some((
                b,
                doc.atom(b.a)?.position,
                doc.atom(b.b)?.position,
            ))
        })
        .collect();
    segments.sort_by(|(_, a, b), (_, c, d)| a.x.min(b.x).total_cmp(&c.x.min(d.x)));
    let mut active: Vec<(&Bond, Point, Point)> = vec![];
    let mut work = MAX_CROSSING_WORK;
    let mut queries = 0_usize;
    for (first, a, b) in segments {
        active.retain(|(_, c, d)| c.x.max(d.x) + 0.001 >= a.x.min(b.x));
        for (second, c, d) in &active {
            if work == 0 {
                return MAX_LINE_QUERIES + 1;
            }
            work -= 1;
            if ![first.a, first.b, second.a, second.b]
                .iter()
                .any(|id| rear.contains(id))
                || [first.a, first.b]
                    .iter()
                    .any(|id| *id == second.a || *id == second.b)
                || a.y.min(b.y) > c.y.max(d.y) + 0.001
                || a.y.max(b.y) < c.y.min(d.y) - 0.001
            {
                continue;
            }
            let ab = [
                f64::from(b.x) - f64::from(a.x),
                f64::from(b.y) - f64::from(a.y),
            ];
            let cd = [
                f64::from(d.x) - f64::from(c.x),
                f64::from(d.y) - f64::from(c.y),
            ];
            let ac = [
                f64::from(c.x) - f64::from(a.x),
                f64::from(c.y) - f64::from(a.y),
            ];
            let cross = |[x, y]: [f64; 2], [u, v]: [f64; 2]| x * v - y * u;
            let det = cross(ab, cd);
            if det.abs() >= 0.0009
                && (-0.001..=1.001).contains(&(cross(ac, cd) / det))
                && (-0.001..=1.001).contains(&(cross(ac, ab) / det))
            {
                queries = queries.saturating_add(2);
            }
        }
        active.push((first, a, b));
    }
    queries
}
fn mix_v(a: V, b: V, t: f64) -> V {
    [
        a[0] + t * (b[0] - a[0]),
        a[1] + t * (b[1] - a[1]),
        a[2] + t * (b[2] - a[2]),
    ]
}
fn improper_intersection(a: [V; 3], b: [V; 3], epsilon: f64) -> bool {
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
fn cages(doc: &Document, queries: usize) -> Vec<Triangle> {
    let real: BTreeSet<_> = doc
        .atoms
        .iter()
        .filter(|a| a.centroid.is_empty() && doc.atom_visible(a.id))
        .map(|a| a.id)
        .collect();
    let mut adjacent: BTreeMap<u64, BTreeSet<u64>> = BTreeMap::new();
    let mut duplicate = BTreeSet::new();
    for b in &doc.bonds {
        if matches!(b.order, 1..=4 | 6 | 7)
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
fn shell(
    doc: &Document,
    ids: &[u64],
    adjacent: &BTreeMap<u64, BTreeSet<u64>>,
    queries: usize,
) -> Option<Vec<Triangle>> {
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
    let center = [0.; 3];
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
        for (a, b) in angles
            .iter()
            .zip(angles.iter().cycle().skip(1))
            .take(angles.len())
        {
            let d = (b.0 - a.0).rem_euclid(std::f64::consts::TAU);
            if d < 1e-5 {
                return None;
            }
        }
        order.insert(*id, angles.into_iter().map(|(_, id)| id).collect());
    }
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
    let edges = ids.len() * 3 / 2;
    if seen.len() != edges * 2 || ids.len() + faces.len() != edges + 2 {
        return None;
    }
    let keys: BTreeSet<_> = faces.iter().map(|f| face_key(f)).collect();
    if keys.len() != faces.len()
        || edges * 2 > MAX_TRIANGLES
        || (edges * 2).saturating_mul(queries) > MAX_LINE_QUERIES
    {
        return None;
    }
    // Match the existing ring-paint owner rule: each inferred face carries
    // its highest boundary-bond layer, never an invented neutral layer.
    let layers: BTreeMap<_, _> = doc
        .bonds
        .iter()
        .filter(|b| ids.contains(&b.a) && ids.contains(&b.b) && matches!(b.order, 1..=4 | 6 | 7))
        .map(|b| ((b.a.min(b.b), b.a.max(b.b)), b.z_order))
        .collect();
    let mut counts: BTreeMap<(u64, u64), usize> = BTreeMap::new();
    let mut triangles = vec![];
    let mut volume = 0.;
    let mut mean = 0.;
    let mut orientation = 0.;
    for face in &faces {
        let layer = face
            .iter()
            .zip(face.iter().cycle().skip(1))
            .take(face.len())
            .filter_map(|(a, b)| layers.get(&((*a).min(*b), (*a).max(*b))))
            .copied()
            .max()?;
        for (&a, &b) in face
            .iter()
            .zip(face.iter().cycle().skip(1))
            .take(face.len())
        {
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
        for (&a, &b) in face
            .iter()
            .zip(face.iter().cycle().skip(1))
            .take(face.len())
        {
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
        for (&a, &b) in face
            .iter()
            .zip(face.iter().cycle().skip(1))
            .take(face.len())
        {
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
    for triangle in &mut triangles {
        for p in &mut triangle.vertices {
            for (v, o) in p.iter_mut().zip(origin) {
                *v += o;
            }
        }
    }
    Some(triangles)
}

pub(crate) fn bond_field(doc: &Document, bond: &Bond) -> Field {
    let (Some(a), Some(b)) = (doc.atom(bond.a), doc.atom(bond.b)) else {
        return Field::Constant(0.);
    };
    let linear = Field::Bond {
        a: a.position,
        b: b.position,
        wa: a.depth,
        wb: b.depth,
    };
    if matches!(bond.order, 2 | 7) && bond.double_position == crate::bonds::DoublePosition::Auto {
        crate::scene::projected_bonds::face_atoms(doc, bond)
            .and_then(|ids| {
                let vertices: Vec<_> = ids.iter().filter_map(|id| doc.atom(*id)).collect();
                let n = vertices.len().max(1) as f32;
                let center = vertices.iter().fold(Point::default(), |p, a| {
                    p.offset(a.position.x / n, a.position.y / n)
                });
                let depth = vertices.iter().map(|a| a.depth / n).sum();
                Field::plane(&[
                    (a.position, a.depth),
                    (b.position, b.depth),
                    (center, depth),
                ])
            })
            .unwrap_or(linear)
    } else {
        linear
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn opaque_ring_query_estimate_includes_managed_foreground_without_backbone_crossing() {
        let mut doc = Document::default();
        crate::editing::ring(&mut doc, Point::default(), 6, true, 0.);
        let circle = crate::aromatic::circles(&doc).remove(0);
        let foreground: Vec<_> = [-1.1, 1.1]
            .into_iter()
            .map(|x| {
                let id = doc.add_atom("C", circle.center.offset(circle.radius * x, 0.));
                doc.atom_mut(id).unwrap().depth = 100.;
                id
            })
            .collect();
        doc.add_bond(foreground[0], foreground[1], 1, "plain");
        crate::depth_appearance::set_rear_opacity(&mut doc, &foreground, 0.25).unwrap();
        let rear = rear_ids(&doc);
        assert!(
            circle.atoms.iter().all(|id| !rear.contains(id)),
            "The ring is fully opaque"
        );
        assert_eq!(
            crossing_queries(&doc, &rear),
            0,
            "Short foreground does not cross any graph backbone"
        );
        let source = circle.graphic().parts().remove(0);
        let original = Primitive::Path {
            commands: source.commands,
            style: source.style,
            filled: source.filled,
        };
        let (paint, _) = crate::crossings::ring_stroke(&doc, &circle, original.clone());
        let (
            Primitive::Path {
                commands: painted, ..
            },
            Primitive::Path {
                commands: source, ..
            },
        ) = (paint, original)
        else {
            panic!("Expected an actual cubic circle stroke");
        };
        assert_ne!(
            painted, source,
            "The foreground must actually cross the aromatic circle"
        );
        let estimate = estimated_queries(&doc, &rear);
        assert!(
            estimate > foreground.len() + 4,
            "Managed foreground ring queries were ignored"
        );
    }
    #[test]
    fn query_estimate_retains_pre_knockout_owned_arc_geometry() {
        let mut doc = Document::default();
        let ids = crate::editing::ring(&mut doc, Point::default(), 6, false, 0.);
        crate::ring_arcs::toggle(&mut doc, &ids[..3]).unwrap();
        crate::depth_appearance::set_rear_opacity(&mut doc, &ids, 0.25).unwrap();
        let original = crate::ring_arcs::render(&doc);
        let raw_cost = original.primitives.iter().map(curve_queries).sum::<usize>();
        assert!(raw_cost > 20, "The fixture must exercise a real curved arc");
        doc.drawing_style.bold_width_pt = 100.;
        let a = doc.add_atom("C", Point::new(0., -200.));
        let b = doc.add_atom("C", Point::new(0., 200.));
        for id in [a, b] {
            doc.atom_mut(id).unwrap().depth = 100.;
        }
        doc.add_bond(a, b, 1, "bold");
        doc.bonds.last_mut().unwrap().z_order = 1;
        let mut opaque = doc.clone();
        opaque.depth_appearance.clear();
        assert_eq!(
            crate::ring_arcs::render(&opaque)
                .primitives
                .iter()
                .map(curve_queries)
                .sum::<usize>(),
            0,
            "The opaque knockout must remove the original arc"
        );
        let estimate = estimated_queries(&doc, &rear_ids(&doc));
        println!("Pre-knockout curved segments={raw_cost}, retained query estimate={estimate}");
        assert!(
            estimate >= raw_cost * 2,
            "Owned curve quota was inferred from already-knocked-out paint"
        );
    }
    #[test]
    fn exact_triangle_intervals_find_hidden_middle_and_keep_depth_ties_visible() {
        let triangle = Triangle {
            vertices: [[0., -1., 2.], [1., 1., 2.], [-1., 1., 2.]],
            face: vec![],
            shell: 0,
            layer: 0,
        };
        let range = triangle
            .hidden(Point::new(-2., 0.), Point::new(2., 0.), 0., 0., 0.001)
            .unwrap();
        assert_eq!(range, (0.375, 0.625));
        assert!(
            triangle
                .hidden(Point::new(-2., 0.), Point::new(2., 0.), 2., 2., 0.001)
                .is_none()
        );
        assert!(
            triangle
                .hidden(Point::new(-2., 0.), Point::new(2., 0.), 3., 3., 0.001)
                .is_none()
        );
    }
    #[test]
    fn triangle_validation_rejects_crossings_and_overlap_but_allows_shared_edges() {
        let a = [[0., 0., 0.], [2., 0., 0.], [0., 2., 0.]];
        assert!(improper_intersection(
            a,
            [[0.5, 0.5, -1.], [0.5, 0.5, 1.], [1.5, 0.5, 0.]],
            1e-6
        ));
        assert!(improper_intersection(
            a,
            [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]],
            1e-6
        ));
        assert!(!improper_intersection(
            a,
            [[2., 0., 0.], [0., 0., 0.], [1., -1., 0.]],
            1e-6
        ));
        assert!(!improper_intersection(
            a,
            [[0., 0., 0.], [-1., 0., 1.], [0., -1., 1.]],
            1e-6
        ));
    }
    fn rectangle(x: f32, y: f32, w: f32, h: f32) -> Vec<Point> {
        vec![
            Point::new(x, y),
            Point::new(x + w, y),
            Point::new(x + w, y + h),
            Point::new(x, y + h),
        ]
    }
    #[test]
    fn ink_sweep_preserves_holes_tangencies_vertex_crossings_and_many_disjoint_contours() {
        let outer = rectangle(0., 0., 10., 10.);
        let mut hole = rectangle(3., 3., 4., 4.);
        hole.reverse();
        let ink = Ink {
            owners: vec![],
            layer: 0,
            contours: vec![outer, hole],
            field: Field::Constant(10.),
        };
        assert_eq!(
            ink.intervals(Point::new(-5., 5.), Point::new(15., 5.), 0., 0., 0, 0.001),
            vec![(0.25, 0.4), (0.6, 0.75)]
        );
        assert_eq!(
            ink.intervals(Point::new(5., 5.), Point::new(15., 5.), 0., 0., 0, 0.001),
            vec![(0.2, 0.5)]
        );
        assert_eq!(
            ink.intervals(Point::new(1., 1.), Point::new(2., 2.), 0., 0., 0, 0.001),
            vec![(0., 1.)]
        );
        assert!(
            ink.intervals(Point::new(-1., 1.), Point::new(1., -1.), 0., 0., 0, 0.001)
                .is_empty()
        );
        assert_eq!(
            ink.intervals(Point::new(-1., -1.), Point::new(11., 11.), 0., 0., 0, 0.001),
            vec![(1. / 12., 1. / 3.), (2. / 3., 11. / 12.)]
        );
        assert_eq!(
            ink.intervals(Point::new(1., 1.), Point::new(1., 1.), 0., 0., 0, 0.001),
            vec![(0., 1.)]
        );
        let many = Ink {
            owners: vec![],
            layer: 0,
            contours: (0..1000)
                .map(|i| rectangle(i as f32 * 2., -1., 1., 2.))
                .collect(),
            field: Field::Constant(10.),
        };
        let intervals =
            many.intervals(Point::new(-1., 0.), Point::new(2000., 0.), 0., 0., 0, 0.001);
        assert_eq!(intervals.len(), 1000);
        assert_eq!(intervals[0], (1. / 2001., 2. / 2001.));
        assert_eq!(intervals[999], (1999. / 2001., 2000. / 2001.));
    }
}
