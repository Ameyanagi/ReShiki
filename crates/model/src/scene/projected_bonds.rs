//! Secondary rails in a retained ring face, projected without rescaling XY.
use super::depth_field::DepthField;
use crate::{
    document::{Bond, Document, Point},
    polygon::closed_edges,
    projection::vector::{V, cross, dot, sub, unit, xyz},
};
use std::{
    cell::{OnceCell, RefCell},
    collections::{BTreeMap, HashMap, VecDeque},
    rc::Rc,
};

pub(super) const MAX_RING: usize = 24;
const SEARCH_BUDGET: usize = 4096;

fn xy([x, y, _]: V) -> Point {
    Point::new(x, y)
}
fn minus(a: Point, b: Point) -> Point {
    Point::new(a.x - b.x, a.y - b.y)
}
fn cross_xy(a: Point, b: Point) -> f32 {
    a.x * b.y - a.y * b.x
}

#[cfg_attr(test, derive(Clone))]
struct Face {
    atoms: Vec<u64>,
    points: Vec<Point>,
    tangent: Point,
    inward: Point,
    side: f32,
    origin: V,
    end: V,
    tangent_xyz: V,
    inward_xyz: V,
}

type FaceMemo = HashMap<(u64, u64), Option<Rc<Face>>>;

/// Built once per immutable document. Adjacency is shared, but each face search
/// gets its own budget. Stored endpoint order keys the direction-dependent face.
/// The depth flag and adjacency are computed on first use, so a single query
/// costs no more than before.
pub(crate) struct FaceIndex<'d> {
    doc: &'d Document,
    depth: OnceCell<bool>,
    neighbors: OnceCell<BTreeMap<u64, Vec<u64>>>,
    graph: OnceCell<super::graph::Graph<'d>>,
    memo: RefCell<FaceMemo>,
    ring_centers: RefCell<HashMap<(u64, u64), Option<Point>>>,
}

fn adjacency(doc: &Document) -> BTreeMap<u64, Vec<u64>> {
    let mut neighbors: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
    for b in &doc.bonds {
        if !matches!(b.order, 1 | 2 | 4 | 7) || !doc.bond_visible(b.a, b.b) {
            continue;
        }
        neighbors.entry(b.a).or_default().push(b.b);
        neighbors.entry(b.b).or_default().push(b.a);
    }
    for adjacent in neighbors.values_mut() {
        adjacent.sort_unstable();
        adjacent.dedup();
    }
    neighbors
}

/// Unmemoized single query over fresh adjacency; tests only.
#[cfg(test)]
fn paths(doc: &Document, bond: &Bond) -> Vec<Vec<u64>> {
    paths_in(&adjacency(doc), bond)
}

/// Enumerate shortest alternate paths with sorted neighbors. The canonical
/// endpoint order makes equal-sized incident faces stable under bond reversal
/// and insertion-order changes; camera angle never changes the chosen face.
/// The shared adjacency contains the queried bond: both walks skip its
/// endpoint pair before charging the budget, as if it had never been added.
fn paths_in(neighbors: &BTreeMap<u64, Vec<u64>>, bond: &Bond) -> Vec<Vec<u64>> {
    let (from, to) = (bond.a.min(bond.b), bond.a.max(bond.b));
    let mut budget = SEARCH_BUDGET;
    let mut distances = HashMap::from([(to, 0)]);
    let mut queue = VecDeque::from([to]);
    while let Some(id) = queue.pop_front() {
        let Some(depth) = distances.get(&id).copied() else {
            continue;
        };
        if depth >= MAX_RING - 1 {
            continue;
        }
        if distances
            .get(&from)
            .is_some_and(|shortest| depth >= *shortest)
        {
            continue;
        }
        for next in neighbors
            .get(&id)
            .into_iter()
            .flatten()
            .copied()
            .filter(|next| (id.min(*next), id.max(*next)) != (from, to))
        {
            if budget == 0 {
                return vec![];
            }
            budget -= 1;
            if let std::collections::hash_map::Entry::Vacant(entry) = distances.entry(next) {
                entry.insert(depth + 1);
                queue.push_back(next);
            }
        }
    }
    if !distances.contains_key(&from) {
        return vec![];
    }
    let mut pending = vec![vec![from]];
    let mut result = vec![];
    while let Some(path) = pending.pop() {
        if budget == 0 {
            // Avoid choosing an insertion-dependent partial set of candidates.
            return vec![];
        }
        budget -= 1;
        let Some(id) = path.last().copied() else {
            continue;
        };
        if id == to {
            result.push(path);
            continue;
        }
        let Some(depth) = distances.get(&id).copied() else {
            continue;
        };
        for next in neighbors
            .get(&id)
            .into_iter()
            .flatten()
            .rev()
            .copied()
            .filter(|next| (id.min(*next), id.max(*next)) != (from, to))
        {
            if distances.get(&next).is_some_and(|d| *d + 1 == depth) {
                let mut path = path.clone();
                path.push(next);
                pending.push(path);
            }
        }
    }
    result
}

impl<'d> FaceIndex<'d> {
    pub(crate) fn new(doc: &'d Document) -> Self {
        Self {
            doc,
            depth: OnceCell::new(),
            neighbors: OnceCell::new(),
            graph: OnceCell::new(),
            memo: RefCell::new(HashMap::new()),
            ring_centers: RefCell::new(HashMap::new()),
        }
    }

    pub(crate) fn doc(&self) -> &'d Document {
        self.doc
    }

    /// Lookups by atom ID for this document, built on first use.
    pub(crate) fn graph(&self) -> &super::graph::Graph<'d> {
        self.graph
            .get_or_init(|| super::graph::Graph::new(self.doc))
    }

    /// The first atom with this ID, like [`Document::atom`].
    pub(crate) fn atom(&self, id: u64) -> Option<&'d crate::document::Atom> {
        self.graph().atom(id)
    }

    pub(super) fn ring_center(&self, from: u64, to: u64) -> Option<Point> {
        let key = (from, to);
        if let Some(center) = self.ring_centers.borrow().get(&key) {
            return *center;
        }
        let center = super::find_ring_center(self, from, to);
        self.ring_centers.borrow_mut().insert(key, center);
        center
    }

    fn paths(&self, bond: &Bond) -> Vec<Vec<u64>> {
        paths_in(self.neighbors.get_or_init(|| adjacency(self.doc)), bond)
    }

    fn face(&self, bond: &Bond) -> Option<Rc<Face>> {
        // Ordinary 2D diagrams, including components translated in depth, keep
        // their established screen-space positioning and spacing exactly.
        if !*self
            .depth
            .get_or_init(|| self.doc.atoms.iter().any(|a| a.depth.abs() > 0.001))
        {
            return None;
        }
        let key = (bond.a, bond.b);
        if let Some(face) = self.memo.borrow().get(&key) {
            return face.clone();
        }
        let face = find_face(self, bond).map(Rc::new);
        self.memo.borrow_mut().insert(key, face.clone());
        face
    }

    pub(crate) fn face_atoms(&self, bond: &Bond) -> Option<Vec<u64>> {
        Some(self.face(bond)?.atoms.clone())
    }

    pub(super) fn automatic_side(&self, bond: &Bond) -> Option<f32> {
        Some(self.face(bond)?.side)
    }
}

fn find_face(faces: &FaceIndex<'_>, bond: &Bond) -> Option<Face> {
    let a = xyz(faces.atom(bond.a)?);
    let b = xyz(faces.atom(bond.b)?);
    let t = unit(sub(b, a))?;
    let paths = faces.paths(bond);
    let (low, high) = paths
        .iter()
        .flatten()
        .filter_map(|id| faces.atom(*id))
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), a| {
            (lo.min(a.depth), hi.max(a.depth))
        });
    if high - low <= 0.001 {
        return None;
    }
    // Test depth across incident candidates before choosing a face. A face
    // turning parallel to the screen must not switch to its tilted neighbor.
    for path in paths {
        let atoms: Option<Vec<_>> = path.iter().map(|id| faces.atom(*id)).collect();
        let atoms = atoms?;
        if atoms
            .iter()
            .any(|a| a.element == "*" || !a.centroid.is_empty())
        {
            continue;
        }
        let n = atoms.len() as f32;
        let mut center = [0.; 3];
        for atom in &atoms {
            for (value, p) in center.iter_mut().zip(xyz(atom)) {
                *value += p / n;
            }
        }
        let [ax, ay, az] = a;
        let [bx, by, bz] = b;
        let w = sub(center, [(ax + bx) / 2., (ay + by) / 2., (az + bz) / 2.]);
        let along = dot(w, t);
        let [wx, wy, wz] = w;
        let [tx, ty, tz] = t;
        let Some(inward) = unit([wx - along * tx, wy - along * ty, wz - along * tz]) else {
            continue;
        };
        let normal = cross(t, inward);
        let mean_length = closed_edges(&atoms)
            .map(|(a, b)| dot(sub(xyz(a), xyz(b)), sub(xyz(a), xyz(b))).sqrt())
            .sum::<f32>()
            / n;
        // Optimized fullerene faces are slightly warped, especially C70.
        // Use the edge/centroid tangent only within a conservative local
        // envelope; this is not a global plane fit or a coordinate edit.
        if !mean_length.is_finite()
            || mean_length < 0.001
            || atoms
                .iter()
                .any(|atom| dot(sub(xyz(atom), a), normal).abs() > mean_length * 0.2)
        {
            continue;
        }
        let local: Vec<_> = atoms
            .iter()
            .map(|atom| {
                let p = sub(xyz(atom), a);
                Point::new(dot(p, t), dot(p, inward))
            })
            .collect();
        if !convex(&local, mean_length * mean_length * 0.00001) {
            continue;
        }
        let tangent = xy(t);
        let inward = xy(inward);
        let side = if cross_xy(tangent, inward) < 0. {
            -1.
        } else {
            1.
        };
        return Some(Face {
            atoms: path,
            points: atoms.iter().map(|a| a.position).collect(),
            tangent,
            inward,
            side,
            origin: a,
            end: b,
            tangent_xyz: t,
            inward_xyz: unit([wx - along * tx, wy - along * ty, wz - along * tz])?,
        });
    }
    None
}

#[cfg(test)]
fn face(doc: &Document, bond: &Bond) -> Option<Face> {
    FaceIndex::new(doc).face(bond).map(|face| (*face).clone())
}

fn convex(points: &[Point], tolerance: f32) -> bool {
    let mut orientation = 0.;
    for ((a, b), c) in points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .zip(points.iter().cycle().skip(2))
    {
        let turn = cross_xy(minus(*b, *a), minus(*c, *b));
        if !turn.is_finite() || turn.abs() <= tolerance {
            return false;
        }
        if orientation == 0. {
            orientation = turn.signum();
        } else if turn * orientation <= tolerance {
            return false;
        }
    }
    true
}

/// Unmemoized single query; tests check the index against it.
#[cfg(test)]
pub(crate) fn face_atoms(doc: &Document, bond: &Bond) -> Option<Vec<u64>> {
    FaceIndex::new(doc).face_atoms(bond)
}

/// Unmemoized single query; tests check the index against it.
#[cfg(test)]
pub(super) fn automatic_side(doc: &Document, bond: &Bond) -> Option<f32> {
    FaceIndex::new(doc).automatic_side(bond)
}

/// `None` keeps the established 2D fallback; `Some(None)` means a valid face
/// whose rail is too collapsed to draw. Never restore a fixed screen gap in
/// the latter case, and never divide by a face's projection cosine.
#[cfg(test)]
pub(super) fn rail(
    doc: &Document,
    bond: &Bond,
    start: Point,
    end: Point,
    offset: f32,
    trim: (f32, f32),
    half_width: f32,
) -> Option<Option<(Point, Point)>> {
    rail_depth(doc, bond, start, end, offset, trim, half_width)
        .map(|rail| rail.map(|rail| rail.points))
}

/// The existing XY rail plus its own XYZ centerline and one support face.
#[derive(Clone)]
pub(crate) struct RailDepth {
    pub points: (Point, Point),
    pub field: DepthField,
    pub support: Vec<u64>,
}
/// Unmemoized single query; tests check the index against it.
#[cfg(test)]
pub(crate) fn rail_depth(
    doc: &Document,
    bond: &Bond,
    start: Point,
    end: Point,
    offset: f32,
    trim: (f32, f32),
    half_width: f32,
) -> Option<Option<RailDepth>> {
    FaceIndex::new(doc).rail_depth(bond, start, end, offset, trim, half_width)
}

impl FaceIndex<'_> {
    /// `None` keeps the established 2D fallback; `Some(None)` means a valid
    /// face whose rail is too collapsed to draw.
    pub(crate) fn rail_depth(
        &self,
        bond: &Bond,
        start: Point,
        end: Point,
        offset: f32,
        trim: (f32, f32),
        half_width: f32,
    ) -> Option<Option<RailDepth>> {
        let doc = self.doc;
        let face = self.face(bond)?;
        let d = offset * face.side;
        let tangent_length = face.tangent.distance(Point::default());
        let gap = cross_xy(face.tangent, face.inward).abs() * d.abs() / tangent_length.max(0.001);
        if gap < doc.drawing_style.line_width() / 2. {
            return Some(None);
        }
        let first = start.offset(
            face.inward.x * d + face.tangent.x * trim.0,
            face.inward.y * d + face.tangent.y * trim.0,
        );
        let last = end.offset(
            face.inward.x * d - face.tangent.x * trim.1,
            face.inward.y * d - face.tangent.y * trim.1,
        );
        // Inward offset rails are shortened against the actual projected face.
        // Explicit outward or centered rails retain their chosen arrangement.
        let points = if d > 0. && (trim.0 > 0. || trim.1 > 0.) {
            clip(first, last, &face.points, half_width)
        } else {
            Some((first, last))
        };
        let axis = minus(xy(face.end), xy(face.origin));
        let length = dot([axis.x, axis.y, 0.], [axis.x, axis.y, 0.]);
        if length <= 0.000001 || !length.is_finite() {
            return Some(None);
        }
        if let Some((first, last)) = points {
            let (ux, uy) = (axis.x / length.sqrt(), axis.y / length.sqrt());
            if (last.x - first.x) * ux + (last.y - first.y) * uy <= 0.1 {
                return Some(None);
            }
        }
        let lift = |p: Point| {
            let delta = minus(p, xy(face.origin));
            let t = (delta.x * axis.x + delta.y * axis.y) / length;
            face.origin[2] + t * (face.end[2] - face.origin[2])
        };
        let first_z = lift(start) + d * face.inward_xyz[2] + trim.0 * face.tangent_xyz[2];
        let last_z = lift(end) + d * face.inward_xyz[2] - trim.1 * face.tangent_xyz[2];
        if !first_z.is_finite() || !last_z.is_finite() {
            return Some(None);
        }
        Some(points.map(|points| RailDepth {
            points,
            field: DepthField::Bond {
                a: first,
                b: last,
                wa: first_z,
                wb: last_z,
            },
            support: face.atoms.clone(),
        }))
    }
}

fn clip(first: Point, last: Point, points: &[Point], margin: f32) -> Option<(Point, Point)> {
    let origin = points.first().copied()?;
    // Keep f32 arithmetic here; polygon::signed_area evaluates and accumulates in f64.
    let area = closed_edges(points)
        .map(|(a, b)| cross_xy(minus(a, origin), minus(b, origin)))
        .sum::<f32>();
    if !area.is_finite() || area.abs() < 0.001 || !simple(points) {
        return None;
    }
    let orientation = area.signum();
    let direction = minus(last, first);
    let (mut low, mut high) = (0_f32, 1_f32);
    for (a, b) in closed_edges(points) {
        let edge = minus(b, a);
        let alpha = orientation * cross_xy(edge, direction);
        let beta = orientation * cross_xy(edge, minus(first, a)) - margin * a.distance(b);
        if !alpha.is_finite() || !beta.is_finite() {
            return None;
        }
        if alpha.abs() < 0.00001 {
            if beta < 0. {
                return None;
            }
        } else if alpha > 0. {
            low = low.max(-beta / alpha);
        } else {
            high = high.min(-beta / alpha);
        }
        if low > high {
            return None;
        }
    }
    Some((
        first.offset(direction.x * low, direction.y * low),
        first.offset(direction.x * high, direction.y * high),
    ))
}

fn simple(points: &[Point]) -> bool {
    let edges: Vec<_> = closed_edges(points).collect();
    for (i, (a, b)) in edges.iter().enumerate() {
        for (j, (c, d)) in edges.iter().enumerate().skip(i + 2) {
            if i == 0 && j + 1 == points.len() {
                continue;
            }
            let edge = minus(*b, *a);
            let other = minus(*d, *c);
            let denominator = cross_xy(edge, other);
            if denominator.abs() <= 0.00001 {
                continue;
            }
            let between = minus(*c, *a);
            let t = cross_xy(between, other) / denominator;
            let u = cross_xy(between, edge) / denominator;
            if (0. ..=1.).contains(&t) && (0. ..=1.).contains(&u) {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests;
