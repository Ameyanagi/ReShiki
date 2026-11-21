//! Cycle-local perimeter geometry. Stored shapes use the graphic's existing
//! affine frame and corner radius; the legacy circle keeps its mean radius.
use crate::{
    document::Point,
    graphics::{Graphic, GraphicKind, PathCommand},
};
use kurbo::{CubicBez, ParamCurve, ParamCurveArclen};
use serde::{Deserialize, Serialize};
use std::f64::consts::{FRAC_PI_2, TAU};

mod shaft;
pub(super) use shaft::Shaft;
mod profile;
pub(super) use profile::Profile;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Shape {
    #[default]
    Circle,
    Ellipse,
    RoundedRectangle,
    Rectangle,
}

impl Shape {
    pub(super) fn is_circle(&self) -> bool {
        *self == Self::Circle
    }

    pub(crate) fn valid_frame(self, graphic: &Graphic) -> bool {
        graphic.origin.is_finite()
            && graphic.axis_x.is_finite()
            && graphic.axis_y.is_finite()
            && determinant(graphic) != 0.
    }

    fn kind(self) -> GraphicKind {
        match self {
            Self::Circle | Self::Ellipse => GraphicKind::Ellipse,
            Self::RoundedRectangle => GraphicKind::RoundedRectangle,
            Self::Rectangle => GraphicKind::Rectangle,
        }
    }
}

fn determinant(g: &Graphic) -> f64 {
    f64::from(g.axis_x.x) * f64::from(g.axis_y.y) - f64::from(g.axis_x.y) * f64::from(g.axis_y.x)
}

/// One-sided unit tangents in canonical clockwise order. At a sharp corner
/// these differ; a consumer must retain both rather than average them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tangents {
    pub incoming: Point,
    pub outgoing: Point,
}

/// The closest point on a cycle's perimeter. Equal-distance solutions choose
/// the lowest canonical distance `s` from its seam.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Projection {
    pub s: f64,
    pub point: Point,
    pub distance: f64,
}

/// A closed clockwise perimeter measured in world units. Noncircle shapes
/// retain the first Move of Graphic::commands as their seam. Reflection
/// reverses their segments once, preserving that seam. The legacy circle's
/// seam is its rightmost point, independent of its affine frame's direction.
#[derive(Debug, Clone)]
pub struct Outline {
    geometry: Geometry,
    length: f64,
}

#[derive(Debug, Clone)]
enum Geometry {
    Circle { center: V, radius: f64 },
    Path(Vec<Piece>),
}

/// A native cubic with its end on the original trimmed step's arclength
/// scale. Editor-only geometry, never a stored cycle field.
#[derive(Debug, Clone)]
pub(super) struct CubicSection {
    pub points: [[f64; 2]; 4],
    pub end_t: f64,
    pub corner_after: bool,
}

struct Section {
    curve: Segment,
    from: f64,
    to: f64,
    start_corner: bool,
    end_corner: bool,
}

impl Outline {
    pub(super) fn new(g: &Graphic, shape: Shape) -> Option<Self> {
        if shape == Shape::Circle {
            let (center, radius) = g.cycle_circle();
            return (center.is_finite() && radius.is_finite() && radius > 1e-3).then(|| Self {
                geometry: Geometry::Circle {
                    center: center.into(),
                    radius: f64::from(radius),
                },
                length: TAU * f64::from(radius),
            });
        }
        if !shape.valid_frame(g) {
            return None;
        }
        let mut frame = g.clone();
        frame.kind = shape.kind();
        let mut segments = segments(&frame.commands())?;
        if determinant(g) < 0. {
            segments = segments.into_iter().rev().map(Segment::reversed).collect();
        }
        let mut length = 0.;
        let mut pieces = vec![];
        for curve in segments {
            if curve.polygon_length() == 0. {
                continue;
            }
            let piece = Piece::new(curve, length)?;
            let end = length + piece.length;
            // A distance scalar cannot address a short edge swallowed by a
            // much larger prefix. Refuse it instead of skipping that corner.
            if end <= length {
                return None;
            }
            length = end;
            pieces.push(piece);
        }
        (length.is_finite() && length > 0.).then_some(Self {
            geometry: Geometry::Path(pieces),
            length,
        })
    }

    pub fn length(&self) -> f64 {
        self.length
    }

    /// Strictly inside the actual outline; its boundary counts as outside.
    pub(super) fn contains(&self, point: Point) -> Option<bool> {
        Some(self.locate(point)?.0)
    }

    /// Classify and project once. All cycle outlines are convex, including
    /// their affine images. Both incident half-planes retain sharp corners.
    /// Nearest-point tests avoid cubic ray roots rounding past an endpoint
    /// when the query lies exactly on a native quarter's horizontal axis.
    pub(super) fn locate(&self, point: Point) -> Option<(bool, Projection)> {
        let (s, on) = self.project_raw(point)?;
        let (incoming, outgoing) = self.tangent_vectors(s)?;
        let delta = V::from(point).sub(on);
        let inside = |t: V| t.x * delta.y - t.y * delta.x > 0.;
        Some((
            inside(incoming) && inside(outgoing),
            Projection {
                s,
                point: on.point()?,
                distance: delta.length(),
            },
        ))
    }

    /// Point at a wrapped clockwise world distance. Nonfinite distances fail.
    pub fn point(&self, s: f64) -> Option<Point> {
        let s = self.wrapped(s)?;
        match &self.geometry {
            Geometry::Circle { center, radius } => {
                circle_point(*center, *radius, s / radius).point()
            }
            Geometry::Path(pieces) => {
                let piece = at(pieces, s)?;
                piece.curve.at(piece.parameter(s - piece.start)).point()
            }
        }
    }

    pub fn tangents(&self, s: f64) -> Option<Tangents> {
        let (incoming, outgoing) = self.tangent_vectors(s)?;
        Some(Tangents {
            incoming: incoming.point()?,
            outgoing: outgoing.point()?,
        })
    }

    fn tangent_vectors(&self, s: f64) -> Option<(V, V)> {
        let s = self.wrapped(s)?;
        Some(match &self.geometry {
            Geometry::Circle { radius, .. } => {
                let (sin, cos) = (s / radius).sin_cos();
                (V::new(-sin, cos), V::new(-sin, cos))
            }
            Geometry::Path(pieces) => {
                let piece = at(pieces, s)?;
                let t = piece.parameter(s - piece.start);
                let outgoing = piece.curve.tangent(t);
                let incoming = if s == piece.start {
                    pieces
                        .iter()
                        .rev()
                        .find(|p| p.start < piece.start)
                        .or_else(|| pieces.last())?
                        .curve
                        .tangent(1.)
                } else {
                    outgoing
                };
                (incoming, outgoing)
            }
        })
    }

    pub fn project(&self, point: Point) -> Option<Projection> {
        let (s, on) = self.project_raw(point)?;
        Some(Projection {
            s,
            point: on.point()?,
            distance: V::from(point).sub(on).length(),
        })
    }

    fn project_raw(&self, point: Point) -> Option<(f64, V)> {
        if !point.is_finite() {
            return None;
        }
        let p = V::from(point);
        match &self.geometry {
            Geometry::Circle { center, radius } => {
                let delta = p.sub(*center);
                let angle = delta.y.atan2(delta.x).rem_euclid(TAU);
                let on = circle_point(*center, *radius, angle);
                Some((angle * radius, on))
            }
            Geometry::Path(pieces) => {
                let mut best: Option<(f64, V)> = None;
                let tolerance = self.length * 1e-8;
                // Find a nearby candidate before searching remote cubics.
                // Their control boxes are conservative bounds, including for
                // skewed frames; ties still reach the exact comparison below.
                let mut candidates: Vec<_> = pieces
                    .iter()
                    .map(|piece| (piece, piece.curve.bounds_distance(p)))
                    .collect();
                candidates.sort_by(|a, b| a.1.total_cmp(&b.1));
                for (piece, lower) in candidates {
                    if let Some((_, old)) = best {
                        let rounding =
                            32. * f64::EPSILON * (p.length() + old.length() + self.length);
                        if lower > p.sub(old).length() + tolerance + rounding {
                            continue;
                        }
                    }
                    let (t, on) = piece.curve.closest(p, tolerance);
                    let s = (piece.start + piece.distance(t)).rem_euclid(self.length);
                    if best.is_none_or(|(old_s, old)| nearer(p, on, s, old, old_s)) {
                        best = Some((s, on));
                    }
                }
                best
            }
        }
    }

    /// An open line/cubic path from `start` through a signed world distance.
    /// Positive lengths run clockwise, negative lengths run counterclockwise.
    /// The magnitude is capped at one loop; zero returns only a Move command.
    /// Cubics are split, not flattened, so subsequent export remains editable.
    pub fn subpath(&self, start: f64, length: f64) -> Option<Vec<PathCommand>> {
        let start = self.wrapped(start)?;
        if !length.is_finite() {
            return None;
        }
        let amount = length.abs().min(self.length);
        let mut out = vec![PathCommand::Move(self.point(start)?)];
        if amount == 0. {
            return Some(out);
        }
        match &self.geometry {
            Geometry::Circle { center, radius } => {
                let sweep = amount.copysign(length) / radius;
                let count = (sweep.abs() / FRAC_PI_2).ceil() as usize;
                let step = sweep / count as f64;
                for k in 0..count {
                    let a = start / radius + k as f64 * step;
                    let b = a + step;
                    let first = circle_point(*center, *radius, a);
                    let end = circle_point(*center, *radius, b);
                    let controls = (4. / 3.) * (step * 0.25).tan() * radius;
                    out.push(PathCommand::Cubic(
                        first
                            .add(V::new(-a.sin(), a.cos()).scaled(controls))
                            .point()?,
                        end.sub(V::new(-b.sin(), b.cos()).scaled(controls))
                            .point()?,
                        end.point()?,
                    ));
                }
            }
            Geometry::Path(pieces) => {
                let mut s = if length < 0. {
                    (start - amount).rem_euclid(self.length)
                } else {
                    start
                };
                let mut remaining = amount;
                let mut curves = vec![];
                for _ in 0..=pieces.len() {
                    if remaining <= 0. {
                        break;
                    }
                    let piece = at(pieces, s)?;
                    let local = s - piece.start;
                    let take = remaining.min(piece.length - local);
                    curves.push(
                        piece
                            .curve
                            .trim(piece.parameter(local), piece.parameter(local + take)),
                    );
                    remaining -= take;
                    s = (piece.start + piece.length).rem_euclid(self.length);
                }
                if length < 0. {
                    curves = curves.into_iter().rev().map(Segment::reversed).collect();
                }
                for curve in curves {
                    out.push(curve.command()?);
                }
            }
        }
        Some(out)
    }

    /// Retain source boundaries and the trimmed step's arclength midpoint.
    /// Native arrow parameters give equal time to unequal segments, so Bow
    /// consumers must use these original distances instead.
    pub(super) fn cubic_sections(&self, start: f64, length: f64) -> Option<Vec<CubicSection>> {
        let amount = length.abs().min(self.length);
        if amount == 0. {
            return None;
        }
        Some(
            self.sections(start, length)?
                .into_iter()
                .map(|s| CubicSection {
                    points: s.curve.controls().map(|p| [p.x, p.y]),
                    end_t: s.to / amount,
                    corner_after: s.end_corner,
                })
                .collect(),
        )
    }

    fn sections(&self, start: f64, length: f64) -> Option<Vec<Section>> {
        let Geometry::Path(pieces) = &self.geometry else {
            return None;
        };
        let start = self.wrapped(start)?;
        if !length.is_finite() {
            return None;
        }
        let amount = length.abs().min(self.length);
        let s = if length < 0. {
            (start - amount).rem_euclid(self.length)
        } else {
            start
        };
        let first = pieces.iter().position(|p| s < p.start + p.length)?;
        let mut local = s - pieces.get(first)?.start;
        let mut done = 0.;
        let mut sections = vec![];
        let corner = |index: usize| {
            let before = pieces.get((index + pieces.len() - 1) % pieces.len())?;
            let after = pieces.get(index)?;
            Some(
                matches!(
                    (before.curve, after.curve),
                    (Segment::Line(..), Segment::Line(..))
                ) || before.curve.tangent(1.).dot(after.curve.tangent(0.)) < 1. - 1e-5,
            )
        };
        // Advance by source piece, independently of the optional midpoint.
        // Subtracting a midpoint from an edge can leave a rounding residual
        // or an unwrapped seam distance and must not lose the next edge.
        for i in 0..=pieces.len() {
            if done >= amount {
                break;
            }
            let index = (first + i) % pieces.len();
            let piece = pieces.get(index)?;
            let boundary = done + (piece.length - local);
            let to = amount.min(boundary);
            let at_end = boundary <= amount;
            let end = if at_end {
                piece.length
            } else {
                local + to - done
            };
            let (t0, t1) = (piece.parameter(local), piece.parameter(end));
            let start_corner = local == 0. && corner(index)?;
            let end_corner = at_end && corner((index + 1) % pieces.len())?;
            let mid = amount * 0.5;
            let tm = (done < mid && mid < to).then(|| piece.parameter(local + mid - done));
            if let Some(tm) = tm.filter(|tm| t0 < *tm && *tm < t1) {
                sections.push(Section {
                    curve: piece.curve.trim(t0, tm),
                    from: done,
                    to: mid,
                    start_corner,
                    end_corner: false,
                });
                sections.push(Section {
                    curve: piece.curve.trim(tm, t1),
                    from: mid,
                    to,
                    start_corner: false,
                    end_corner,
                });
            } else {
                // Arclength inversion can round an adjacent midpoint onto
                // a source boundary. Share that knot, retaining its corner,
                // instead of adding a zero-length cubic with no tangent.
                if tm.is_some_and(|tm| tm <= t0)
                    && let Some(last) = sections.last_mut()
                {
                    last.to = mid;
                }
                sections.push(Section {
                    curve: piece.curve.trim(t0, t1),
                    from: done,
                    to: if tm.is_some_and(|tm| tm >= t1) {
                        mid
                    } else {
                        to
                    },
                    start_corner,
                    end_corner,
                });
            }
            done = to;
            local = 0.;
        }
        if amount - done > amount * 1e-12 {
            return None;
        }
        // A nonzero parameter span can still round to one geometric point.
        // Merge only that coincident knot, never a short nonzero source edge.
        let mut retained: Vec<Section> = vec![];
        for section in sections {
            if section.curve.polygon_length() == 0. {
                if let Some(last) = retained.last_mut() {
                    last.to = section.to;
                    last.end_corner |= section.end_corner;
                }
            } else {
                retained.push(section);
            }
        }
        let mut sections = retained;
        sections.first_mut()?.from = 0.;
        if let Some(last) = sections.last_mut() {
            last.to = amount;
        }
        for i in 1..sections.len() {
            let from = sections.get(i - 1)?.to;
            sections.get_mut(i)?.from = from;
        }
        if length < 0. {
            sections = sections
                .into_iter()
                .rev()
                .map(|s| Section {
                    curve: s.curve.reversed(),
                    from: amount - s.to,
                    to: amount - s.from,
                    start_corner: s.end_corner,
                    end_corner: s.start_corner,
                })
                .collect();
        }
        Some(sections)
    }

    fn wrapped(&self, s: f64) -> Option<f64> {
        s.is_finite().then(|| {
            let s = s.rem_euclid(self.length);
            // Floating-point remainder may round a tiny negative distance to
            // the full period. The seam always addresses the first segment.
            if s == 0. || s >= self.length { 0. } else { s }
        })
    }
}

fn circle_point(center: V, radius: f64, angle: f64) -> V {
    center.add(V::new(angle.cos(), angle.sin()).scaled(radius))
}

fn at(pieces: &[Piece], s: f64) -> Option<&Piece> {
    pieces.iter().find(|p| s < p.start + p.length)
}

fn nearer(p: V, point: V, s: f64, old: V, old_s: f64) -> bool {
    // Compare squared distances without subtracting two large, almost equal
    // squares. A fixed world tolerance loses tiny edges; a tolerance relative
    // to distance loses distinct edges when the query is far from the shape.
    let delta = point.sub(old);
    let sum = point.sub(p).add(old.sub(p));
    let (x, y) = (delta.x * sum.x, delta.y * sum.y);
    let difference = x + y;
    let error = 8. * f64::EPSILON * (x.abs() + y.abs());
    difference < -error || difference.abs() <= error && s < old_s
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct V {
    x: f64,
    y: f64,
}
impl From<Point> for V {
    fn from(p: Point) -> Self {
        Self::new(f64::from(p.x), f64::from(p.y))
    }
}
impl V {
    fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
    fn add(self, other: Self) -> Self {
        Self::new(self.x + other.x, self.y + other.y)
    }
    fn sub(self, other: Self) -> Self {
        Self::new(self.x - other.x, self.y - other.y)
    }
    fn scaled(self, by: f64) -> Self {
        Self::new(self.x * by, self.y * by)
    }
    fn lerp(self, other: Self, t: f64) -> Self {
        self.add(other.sub(self).scaled(t))
    }
    fn dot(self, other: Self) -> f64 {
        self.x * other.x + self.y * other.y
    }
    fn length(self) -> f64 {
        self.x.hypot(self.y)
    }
    fn unit(self) -> Self {
        self.scaled(1. / self.length())
    }
    fn point(self) -> Option<Point> {
        let p = Point::new(self.x as f32, self.y as f32);
        p.is_finite().then_some(p)
    }
}

#[derive(Debug, Clone, Copy)]
enum Segment {
    Line(V, V),
    Cubic([V; 4]),
}
impl Segment {
    fn controls(self) -> [V; 4] {
        match self {
            Self::Cubic(points) => points,
            Self::Line(a, b) => [a, a.lerp(b, 1. / 3.), a.lerp(b, 2. / 3.), b],
        }
    }
    fn cubic(self) -> CubicBez {
        let [p, a, b, q] = self.controls().map(|p| kurbo::Point::new(p.x, p.y));
        CubicBez::new(p, a, b, q)
    }
    fn ends(self) -> (V, V) {
        match self {
            Self::Line(a, b) | Self::Cubic([a, _, _, b]) => (a, b),
        }
    }
    fn at(self, t: f64) -> V {
        match self {
            Self::Line(a, b) => a.lerp(b, t),
            Self::Cubic([p, a, b, q]) => {
                let ab = a.lerp(b, t);
                p.lerp(a, t).lerp(ab, t).lerp(ab.lerp(b.lerp(q, t), t), t)
            }
        }
    }
    fn velocity(self, t: f64) -> V {
        match self {
            Self::Line(a, b) => b.sub(a),
            Self::Cubic([p, a, b, q]) => {
                let u = 1. - t;
                a.sub(p)
                    .scaled(3. * u * u)
                    .add(b.sub(a).scaled(6. * u * t))
                    .add(q.sub(b).scaled(3. * t * t))
            }
        }
    }
    fn acceleration(self, t: f64) -> V {
        match self {
            Self::Line(..) => V::new(0., 0.),
            Self::Cubic([p, a, b, q]) => b
                .sub(a.scaled(2.))
                .add(p)
                .scaled(6. * (1. - t))
                .add(q.sub(b.scaled(2.)).add(a).scaled(6. * t)),
        }
    }
    fn tangent(self, t: f64) -> V {
        let v = self.velocity(t);
        if v.length() > 0. {
            return v.unit();
        }
        let (a, b) = self.ends();
        match self {
            Self::Cubic([_, _, q, _]) if t == 0. && q.sub(a).length() > 0. => q.sub(a).unit(),
            Self::Cubic([_, p, _, _]) if t == 1. && b.sub(p).length() > 0. => b.sub(p).unit(),
            _ => b.sub(a).unit(),
        }
    }
    fn reversed(self) -> Self {
        match self {
            Self::Line(a, b) => Self::Line(b, a),
            Self::Cubic([p, a, b, q]) => Self::Cubic([q, b, a, p]),
        }
    }
    fn split(self, t: f64) -> (Self, Self) {
        match self {
            Self::Line(a, b) => {
                let p = a.lerp(b, t);
                (Self::Line(a, p), Self::Line(p, b))
            }
            Self::Cubic([p, a, b, q]) => {
                let (pa, ab, bq) = (p.lerp(a, t), a.lerp(b, t), b.lerp(q, t));
                let (left, right) = (pa.lerp(ab, t), ab.lerp(bq, t));
                let mid = left.lerp(right, t);
                (
                    Self::Cubic([p, pa, left, mid]),
                    Self::Cubic([mid, right, bq, q]),
                )
            }
        }
    }
    fn trim(self, from: f64, to: f64) -> Self {
        if from == 0. && to == 1. {
            return self;
        }
        let (left, _) = self.split(to);
        if from == 0. || to == 0. {
            left
        } else {
            left.split(from / to).1
        }
    }
    fn command(self) -> Option<PathCommand> {
        Some(match self {
            Self::Line(_, b) => PathCommand::Line(b.point()?),
            Self::Cubic([_, a, b, q]) => PathCommand::Cubic(a.point()?, b.point()?, q.point()?),
        })
    }
    fn polygon_length(self) -> f64 {
        match self {
            Self::Line(a, b) => b.sub(a).length(),
            Self::Cubic([p, a, b, q]) => a.sub(p).length() + b.sub(a).length() + q.sub(b).length(),
        }
    }
    fn bounds_distance(self, p: V) -> f64 {
        let (a, b) = self.ends();
        let controls = match self {
            Self::Cubic([_, c, d, _]) => [a, c, d, b],
            _ => [a, a, b, b],
        };
        let (mut lo, mut hi) = (a, a);
        for q in controls {
            lo = V::new(lo.x.min(q.x), lo.y.min(q.y));
            hi = V::new(hi.x.max(q.x), hi.y.max(q.y));
        }
        p.sub(V::new(p.x.clamp(lo.x, hi.x), p.y.clamp(lo.y, hi.y)))
            .length()
    }
    fn closest(self, p: V, tolerance: f64) -> (f64, V) {
        if let Self::Line(a, b) = self {
            let t = on_line(a, b, p);
            return (t, self.at(t));
        }
        let (a, b) = self.ends();
        let mut best = if nearer(p, b, 1., a, 0.) {
            (1., b)
        } else {
            (0., a)
        };
        // Seed with valid points near a stationary distance before walking
        // tiny flat intervals. The search below still checks every bound
        // that can beat these candidates, including other local minima.
        for t in [on_line(a, b, p), 0.5] {
            let t = self.refine(p, (0., 1.), t);
            let q = self.at(t);
            if nearer(p, q, t, best.1, best.0) {
                best = (t, q);
            }
        }
        self.search(p, self, (0., 1.), tolerance, 0, &mut best);
        best
    }
    fn refine(self, p: V, range: (f64, f64), mut t: f64) -> f64 {
        for _ in 0..8 {
            let delta = self.at(t).sub(p);
            let v = self.velocity(t);
            let slope = v.dot(v) + delta.dot(self.acceleration(t));
            if slope == 0. {
                break;
            }
            let next = (t - delta.dot(v) / slope).clamp(range.0, range.1);
            if next == t {
                break;
            }
            t = next;
        }
        t
    }
    fn search(
        self,
        p: V,
        part: Self,
        range: (f64, f64),
        tolerance: f64,
        depth: u8,
        best: &mut (f64, V),
    ) {
        if part.bounds_distance(p) > p.sub(best.1).length() + tolerance {
            return;
        }
        let (a, b) = part.ends();
        let flatness = match part {
            Self::Cubic([_, c, d, _]) => [c, d]
                .into_iter()
                .map(|q| q.sub(a.lerp(b, on_line(a, b, q))).length())
                .fold(0., f64::max),
            _ => 0.,
        };
        if flatness <= tolerance || depth == 24 {
            let initial = range.0 + (range.1 - range.0) * on_line(a, b, p);
            let t = self.refine(p, range, initial);
            for t in [range.0, t, range.1] {
                let q = self.at(t);
                if nearer(p, q, t, best.1, best.0) {
                    *best = (t, q);
                }
            }
            return;
        }
        let (left, right) = part.split(0.5);
        let middle = (range.0 + range.1) * 0.5;
        self.search(p, left, (range.0, middle), tolerance, depth + 1, best);
        self.search(p, right, (middle, range.1), tolerance, depth + 1, best);
    }
}

fn on_line(a: V, b: V, p: V) -> f64 {
    let v = b.sub(a);
    let length = v.dot(v);
    if length > 0. {
        (p.sub(a).dot(v) / length).clamp(0., 1.)
    } else {
        0.
    }
}

fn segments(commands: &[PathCommand]) -> Option<Vec<Segment>> {
    let mut out = vec![];
    let (mut first, mut last) = (None, None);
    for command in commands {
        if command.points().iter().any(|p| !p.is_finite()) {
            return None;
        }
        match *command {
            PathCommand::Move(p) if first.is_none() => {
                first = Some(V::from(p));
                last = first;
            }
            PathCommand::Move(_) => return None,
            PathCommand::Line(p) => {
                out.push(Segment::Line(last?, p.into()));
                last = Some(p.into());
            }
            PathCommand::Cubic(a, b, p) => {
                out.push(Segment::Cubic([last?, a.into(), b.into(), p.into()]));
                last = Some(p.into());
            }
            PathCommand::Close => {
                out.push(Segment::Line(last?, first?));
                last = first;
            }
        }
    }
    Some(out)
}

#[derive(Debug, Clone)]
struct Piece {
    curve: Segment,
    start: f64,
    length: f64,
    tolerance: f64,
}
impl Piece {
    fn new(curve: Segment, start: f64) -> Option<Self> {
        let polygon = curve.polygon_length();
        let tolerance = polygon * 1e-10;
        let length = match curve {
            Segment::Line(..) => polygon,
            Segment::Cubic(_) => curve.cubic().arclen(tolerance),
        };
        (length > 0. && length.is_finite()).then_some(Self {
            curve,
            start,
            length,
            tolerance,
        })
    }
    fn distance(&self, t: f64) -> f64 {
        if t <= 0. {
            return 0.;
        }
        if t >= 1. {
            return self.length;
        }
        match self.curve {
            Segment::Line(..) => t * self.length,
            Segment::Cubic(_) => self.curve.cubic().subsegment(0. ..t).arclen(self.tolerance),
        }
    }
    fn parameter(&self, s: f64) -> f64 {
        if s <= 0. {
            return 0.;
        }
        if s >= self.length {
            return 1.;
        }
        match self.curve {
            Segment::Line(..) => s / self.length,
            Segment::Cubic(_) => self.curve.cubic().inv_arclen(s, self.tolerance),
        }
    }
}
