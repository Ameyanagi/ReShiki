//! Drawing depth at crossings; never changes chemical connectivity.
use crate::{
    document::{Document, Point},
    scene::Primitive,
    style::DEFAULT,
};
mod depth;
pub use depth::bond_over;
mod ring_strokes;
pub(crate) use ring_strokes::{ring_stroke, ring_stroke_with};
#[derive(Debug, Clone, Copy)]
pub struct Gap {
    origin: Point,
    axis: Point,
    low: f32,
    high: f32,
    /// For a multi-rail bond, the one rail (and offset tolerance) this gap
    /// cuts: its rails pass under the front ink at different places.
    rail: Option<(Rail, f32)>,
    /// Pieces the gap leaves shorter than this (a stroke width) are dropped,
    /// so no dots or stubs remain.
    min: f32,
}
fn cross(a: Point, b: Point) -> f32 {
    a.x * b.y - a.y * b.x
}
fn delta(a: Point, b: Point) -> Point {
    Point::new(b.x - a.x, b.y - a.y)
}
/// One stroke of a bond as the scene draws it: the part of the bond it
/// covers (distance from the bond's start), its centerline offsets across
/// the bond's scene normal (-uy, ux) at the two ends of that part, its
/// width, and whether it is a crossed double's diagonal.
#[derive(Debug, Clone, Copy)]
struct Rail {
    span: (f32, f32),
    offset: (f32, f32),
    width: f32,
    crossed: bool,
}
impl Rail {
    /// The centerline offset at distance `s` from the bond's start.
    fn at(&self, s: f32) -> f32 {
        let length = self.span.1 - self.span.0;
        if length.abs() < 1e-6 {
            return self.offset.0;
        }
        self.offset.0 + (self.offset.1 - self.offset.0) * (s - self.span.0) / length
    }
    /// The centerline of this rail of the bond from `from` along unit `axis`:
    /// its point at distance 0 along the bond and its step per unit distance.
    fn line(&self, from: Point, axis: Point) -> (Point, Point) {
        let (offset, slope) = (self.at(0.), self.at(1.) - self.at(0.));
        (
            from.offset(-axis.y * offset, axis.x * offset),
            Point::new(axis.x - axis.y * slope, axis.y + axis.x * slope),
        )
    }
}
pub(crate) struct Rails {
    strokes: Vec<Rail>,
    /// Strokes a gap can cut one at a time. Hashes and hollow wedges draw
    /// their ink across the bond, so every gap cuts all of it.
    separate: bool,
}
impl Rails {
    /// The band of offsets the strokes' ink covers, each by its own width.
    fn band(&self) -> (f32, f32) {
        self.strokes.iter().fold((0., 0.), |(lo, hi), r| {
            let half = r.width * 0.5;
            (
                lo.min(r.offset.0.min(r.offset.1) - half),
                hi.max(r.offset.0.max(r.offset.1) + half),
            )
        })
    }
    /// Each stroke with its gap filter; a single rail needs none.
    fn filters(&self) -> Vec<(Rail, Option<(Rail, f32)>)> {
        let mut nearest = f32::INFINITY;
        for (i, a) in self.strokes.iter().enumerate() {
            for b in self.strokes.iter().skip(i + 1) {
                let apart = (a.offset.0 - b.offset.0).abs() + (a.offset.1 - b.offset.1).abs();
                nearest = nearest.min(apart * 0.5);
            }
        }
        let filter = self.separate && self.strokes.len() > 1;
        self.strokes
            .iter()
            .map(|r| (*r, filter.then_some((*r, nearest * 0.5))))
            .collect()
    }
}
/// Resolves where the scene puts each bond's rails, lazily: most crossings
/// involve only single bonds.
pub(crate) struct Sides<'d> {
    doc: &'d Document,
    faces: std::cell::OnceCell<crate::scene::projected_bonds::FaceIndex<'d>>,
    circles: std::cell::OnceCell<Vec<crate::aromatic::Circle>>,
}
impl<'d> Sides<'d> {
    pub(crate) fn new(doc: &'d Document) -> Self {
        Self {
            doc,
            faces: Default::default(),
            circles: Default::default(),
        }
    }
    fn faces(&self) -> &crate::scene::projected_bonds::FaceIndex<'d> {
        self.faces
            .get_or_init(|| crate::scene::projected_bonds::FaceIndex::new(self.doc))
    }
    /// Mirrors the scene's rails: a positioned or face-resolved double keeps
    /// its trimmed second rail, in that rail's own width, on one side
    /// (foreshortened within a projected face), a crossed double runs its
    /// strokes diagonally, and an aromatic bond without a ring circle draws
    /// its inner line toward the ring.
    fn rails(&self, bond: &crate::document::Bond) -> Rails {
        use crate::bonds::DoublePosition::{Auto, Left, Right};
        let style = &self.doc.drawing_style;
        let display = bond.base_display();
        let width_of = |display: &str| {
            if ["bold", "wedge", "hollow_wedge", "hashed", "hash"].contains(&display) {
                style.world(style.bold_width_pt)
            } else {
                style.line_width()
            }
        };
        let width = width_of(display);
        let second = bond.secondary_display.as_deref().map_or(width, width_of);
        let length = self
            .doc
            .atom(bond.a)
            .zip(self.doc.atom(bond.b))
            .map_or(0., |(a, b)| a.position.distance(b.position));
        let rail = |offset: f32, width: f32| Rail {
            span: (0., length),
            offset: (offset, offset),
            width,
            crossed: false,
        };
        let spacing = style.bond_length_world * style.bond_spacing_ratio;
        let strokes = match bond.order {
            2 if display == "wavy" => {
                // The diagonals run between the label-trimmed ends.
                let span = self
                    .doc
                    .atom(bond.a)
                    .zip(crate::scene::bond_segment(self.doc, bond, None))
                    .map_or((0., length), |(a, (start, end))| {
                        (a.position.distance(start), a.position.distance(end))
                    });
                [-spacing / 2., spacing / 2.]
                    .map(|from| Rail {
                        span,
                        offset: (from, -from),
                        width,
                        crossed: true,
                    })
                    .to_vec()
            }
            2 | 7 => match crate::scene::double_position(self.faces(), bond) {
                side @ (Left | Right) => {
                    let offset = if side == Left { -spacing } else { spacing };
                    let trim = spacing * 0.75;
                    let projected = if bond.double_position == Auto {
                        self.projected(bond, offset, trim, second)
                    } else {
                        None
                    };
                    let inner = projected.unwrap_or_else(|| {
                        (length - 2. * trim > 0.1).then_some(Rail {
                            span: (trim, length - trim),
                            ..rail(offset, second)
                        })
                    });
                    std::iter::once(rail(0., width)).chain(inner).collect()
                }
                _ => vec![rail(-spacing / 2., width), rail(spacing / 2., second)],
            },
            4 if self
                .circles
                .get_or_init(|| crate::aromatic::circles(self.doc))
                .iter()
                .any(|c| c.contains_bond(bond.a, bond.b)) =>
            {
                vec![rail(0., width)]
            }
            4 => {
                let side = crate::scene::automatic_double_side(self.faces(), bond).unwrap_or(1.);
                vec![rail(0., width), rail(side * spacing, width)]
            }
            3 => vec![rail(-spacing, width), rail(0., width), rail(spacing, width)],
            6 => [-1.5, -0.5, 0.5, 1.5]
                .map(|k| rail(k * spacing, width))
                .to_vec(),
            _ => vec![rail(0., width)],
        };
        Rails {
            strokes,
            separate: !matches!(display, "hash" | "hashed" | "hollow_wedge"),
        }
    }
    /// An automatic double bond's inner rail within its projected face:
    /// `None` keeps the 2D rail, `Some(None)` omits it.
    fn projected(
        &self,
        bond: &crate::document::Bond,
        offset: f32,
        trim: f32,
        width: f32,
    ) -> Option<Option<Rail>> {
        let a = self.doc.atom(bond.a)?.position;
        let b = self.doc.atom(bond.b)?.position;
        let length = a.distance(b).max(0.001);
        let axis = Point::new((b.x - a.x) / length, (b.y - a.y) / length);
        let rail = self
            .faces()
            .rail_depth(bond, a, b, offset, trim, width / 2.)?;
        Some(rail.map(|rail| {
            let (first, last) = (delta(a, rail.points.0), delta(a, rail.points.1));
            let along = |p: Point| p.x * axis.x + p.y * axis.y;
            Rail {
                span: (along(first), along(last)),
                offset: (cross(axis, first), cross(axis, last)),
                width,
                crossed: false,
            }
        }))
    }
}
/// Ink of a straight stroke: the segment `from`→`to`, padded by `ends` along
/// it, covering offsets `band` across its normal (-uy, ux).
#[derive(Clone, Copy)]
struct Stroke {
    from: Point,
    to: Point,
    ends: f32,
    band: (f32, f32),
}
/// Parameter interval along an under rail (`origin` + s·`direction`, see
/// [`Rail::line`]) that lies inside the over stroke's finite ink. Each rail
/// meets the over ink at its own place, and the finite length keeps a shallow
/// cut off ink that is not there.
fn clearance(over: Stroke, (origin, direction): (Point, Point)) -> Option<(f32, f32)> {
    let length = over.from.distance(over.to);
    if length < 0.001 {
        return None;
    }
    let along = Point::new(
        (over.to.x - over.from.x) / length,
        (over.to.y - over.from.y) / length,
    );
    let normal = Point::new(-along.y, along.x);
    let dot = |a: Point, b: Point| a.x * b.x + a.y * b.y;
    // Solve lo <= start + s·rate <= hi for s.
    let range = |start: f32, rate: f32, (lo, hi): (f32, f32)| {
        if rate.abs() < 1e-6 {
            (lo..=hi)
                .contains(&start)
                .then_some((f32::NEG_INFINITY, f32::INFINITY))
        } else {
            let (a, b) = ((lo - start) / rate, (hi - start) / rate);
            Some((a.min(b), a.max(b)))
        }
    };
    let start = delta(over.from, origin);
    let a = range(
        dot(start, along),
        dot(direction, along),
        (-over.ends, length + over.ends),
    )?;
    let b = range(dot(start, normal), dot(direction, normal), over.band)?;
    let (low, high) = (a.0.max(b.0), a.1.min(b.1));
    (low <= high && low.is_finite() && high.is_finite()).then_some((low, high))
}
/// The convex hull of a front bond's drawn rail centerlines. Its occlusion
/// envelope, every point within a radius of it, covers the space between its
/// rails but not where a trimmed or omitted rail is not drawn.
struct Envelope {
    /// Counterclockwise.
    hull: Vec<Point>,
}
impl Envelope {
    fn new(mut points: Vec<Point>) -> Self {
        points.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
        points.dedup_by(|a, b| a.distance(*b) < 1e-6);
        if points.len() < 3 {
            return Self { hull: points };
        }
        // Monotone chain: the lower hull, then the upper.
        let mut hull: Vec<Point> = vec![];
        for pass in [points.clone(), points.into_iter().rev().collect()] {
            let start = hull.len();
            for p in pass {
                while hull.len() >= start + 2 {
                    let [.., a, b] = hull.as_slice() else { break };
                    if cross(delta(*a, *b), delta(*a, p)) > 0. {
                        break;
                    }
                    hull.pop();
                }
                hull.push(p);
            }
            hull.pop();
        }
        Self { hull }
    }
    fn distance(&self, p: Point) -> f32 {
        let edges = || self.hull.iter().zip(self.hull.iter().cycle().skip(1));
        if self.hull.len() >= 3 && edges().all(|(a, b)| cross(delta(*a, *b), delta(*a, p)) >= 0.) {
            return 0.;
        }
        edges()
            .map(|(a, b)| crate::graphics::segment_distance(p, *a, *b))
            .fold(f32::INFINITY, f32::min)
    }
    /// The interval of `origin + s·axis`, s in `range`, within `radius`,
    /// and its closest approach. Distance to a convex set is convex along a
    /// line, so a ternary search finds the closest point and bisection the
    /// two boundaries.
    fn span(
        &self,
        origin: Point,
        axis: Point,
        (lo, hi): (f32, f32),
        radius: f32,
    ) -> Option<(f32, f32, f32)> {
        let at = |s: f32| self.distance(origin.offset(axis.x * s, axis.y * s));
        let (mut a, mut b) = (lo, hi);
        for _ in 0..48 {
            let (m1, m2) = (a + (b - a) / 3., b - (b - a) / 3.);
            if at(m1) < at(m2) {
                b = m2;
            } else {
                a = m1;
            }
        }
        let best = (a + b) * 0.5;
        if at(best) > radius {
            return None;
        }
        let edge = |mut inside: f32, mut outside: f32| {
            if at(outside) <= radius {
                return outside;
            }
            for _ in 0..24 {
                let middle = (inside + outside) * 0.5;
                if at(middle) <= radius {
                    inside = middle;
                } else {
                    outside = middle;
                }
            }
            inside
        };
        Some((edge(best, lo), edge(best, hi), best))
    }
}
/// Sorted sweep avoids checking every pair of well-separated molecules.
pub fn gaps(doc: &Document) -> Vec<Vec<Gap>> {
    gaps_with_opacity(doc, &crate::rear_opacity::Paint::new(doc))
}

/// Occlusion: every opaque rear rail loses the part inside a front stroke's
/// envelope (each drawn front rail, padded by both half-widths and the
/// 1.1 pt clearance). Front and rear follow XYZ depth along the overlap, so one rule
/// covers crossings at any angle, near-parallel overlaps and overlaps beside
/// atoms. Bonds sharing an atom meet there and never cut each other.
pub(crate) fn gaps_with_opacity(
    doc: &Document,
    opacity: &crate::rear_opacity::Paint,
) -> Vec<Vec<Gap>> {
    let mut gaps = vec![vec![]; doc.bonds.len()];
    let sides = Sides::new(doc);
    let style = &doc.drawing_style;
    // The farthest an envelope reaches past a centerline: outer quadruple
    // rails, two bold half-widths and the clearance.
    let pad = style.bond_length_world * style.bond_spacing_ratio * 1.5
        + style.world(style.bold_width_pt)
        + DEFAULT.world(1.1)
        + 0.001;
    let mut segments: Vec<_> = doc
        .bonds
        .iter()
        .enumerate()
        .filter_map(|(i, b)| {
            if !doc.bond_visible(b.a, b.b) {
                return None;
            }
            Some((i, doc.atom(b.a)?.position, doc.atom(b.b)?.position))
        })
        .collect();
    segments.sort_by(|(_, a, b), (_, c, d)| a.x.min(b.x).total_cmp(&c.x.min(d.x)));
    let mut active: Vec<(usize, Point, Point)> = vec![];
    let rails: Vec<std::cell::OnceCell<Rails>> =
        (0..doc.bonds.len()).map(|_| Default::default()).collect();
    let rail = |index: usize| {
        let bond = doc.bonds.get(index)?;
        Some(rails.get(index)?.get_or_init(|| sides.rails(bond)))
    };
    let mut budget = 200_000_usize;
    for (i, a, b) in segments {
        active.retain(|(_, c, d)| c.x.max(d.x) + 2. * pad >= a.x.min(b.x));
        for &(j, c, d) in &active {
            if a.y.min(b.y) > c.y.max(d.y) + 2. * pad || a.y.max(b.y) < c.y.min(d.y) - 2. * pad {
                continue;
            }
            if budget == 0 {
                return gaps;
            }
            budget -= 1;
            let (Some(first), Some(second)) = (doc.bonds.get(i), doc.bonds.get(j)) else {
                continue;
            };
            if [first.a, first.b]
                .iter()
                .any(|id| *id == second.a || *id == second.b)
            {
                continue;
            }
            for (rear, front) in [((i, a, b), (j, c, d)), ((j, c, d), (i, a, b))] {
                let (Some(under), Some(over)) = (rail(rear.0), rail(front.0)) else {
                    continue;
                };
                let found = occlusion(doc, opacity, rear, under, front, over);
                if let Some(list) = gaps.get_mut(rear.0) {
                    list.extend(found);
                }
            }
        }
        active.push((i, a, b));
    }
    gaps
}

/// Gaps in bond `rear` (index, from, to) where it lies behind `front`'s ink.
fn occlusion(
    doc: &Document,
    opacity: &crate::rear_opacity::Paint,
    (rear, from, to): (usize, Point, Point),
    under: &Rails,
    (front, over_from, over_to): (usize, Point, Point),
    over: &Rails,
) -> Vec<Gap> {
    let (Some(rear_bond), Some(front_bond)) = (doc.bonds.get(rear), doc.bonds.get(front)) else {
        return vec![];
    };
    // Opacity-managed under-ink already carries its exact visibility
    // interval. A conventional knockout would erase its faint base or make
    // the transparent case wider than the physical occlusion.
    let (length, front_length) = (from.distance(to), over_from.distance(over_to));
    if length < 0.001 || front_length < 0.001 || opacity.manages_bond(rear_bond) {
        return vec![];
    }
    let axis = Point::new((to.x - from.x) / length, (to.y - from.y) / length);
    let along = Point::new(
        (over_to.x - over_from.x) / front_length,
        (over_to.y - over_from.y) / front_length,
    );
    let front_t = |p: Point| {
        let d = delta(over_from, p);
        ((d.x * along.x + d.y * along.y) / front_length).clamp(0., 1.)
    };
    let ends = |stroke: &Rail| {
        [stroke.span.0, stroke.span.1].map(|u| {
            let offset = stroke.at(u);
            over_from.offset(
                along.x * u - along.y * offset,
                along.y * u + along.x * offset,
            )
        })
    };
    // The hull padded by the thinnest front rail covers the space between
    // rails; a wider rail also clears by its own width.
    let thinnest = over
        .strokes
        .iter()
        .map(|r| r.width)
        .fold(f32::INFINITY, f32::min);
    let mut envelopes = vec![(
        Envelope::new(over.strokes.iter().flat_map(ends).collect()),
        thinnest,
    )];
    envelopes.extend(
        over.strokes
            .iter()
            .filter(|r| over.strokes.len() > 1 && r.width > thinnest)
            .map(|r| (Envelope::new(ends(r).to_vec()), r.width)),
    );
    let mut spans = vec![];
    for (rail, filter) in under.filters() {
        // The rail's centerline, linear in the distance `s` along the bond.
        let (origin, direction) = rail.line(from, axis);
        let point = |s: f32| origin.offset(direction.x * s, direction.y * s);
        // Whether the front ink covers the rear rail at `s`: it is in front
        // by XYZ depth there, and opaque there, since transparent ink cannot
        // knock out the ink beneath it.
        let behind = |s: f32| {
            let (t, front_t) = ((s / length).clamp(0., 1.), front_t(point(s)));
            depth::bond_over(doc, front, front_t, rear, t)
                && opacity.bond(front_bond, front_t) >= 1.
        };
        let range = (rail.span.0 - rail.width, rail.span.1 + rail.width);
        for (envelope, width) in &envelopes {
            let radius = (width + rail.width) * 0.5 + DEFAULT.world(1.1);
            let Some((low, high, _)) = envelope.span(point(0.), direction, range, radius) else {
                continue;
            };
            // Depth is linear in s between the points where either bond's
            // parameter clamps to an end, so the hidden set within each piece is
            // an interval touching a piece end: check those ends, then bisect.
            // A front bond faded where it passes behind other ink changes
            // opacity in between, so sample it more finely.
            let rate = direction.x * along.x + direction.y * along.y;
            let base = delta(over_from, point(0.));
            let base = base.x * along.x + base.y * along.y;
            let mut stops = vec![low, high, 0., length];
            if rate.abs() > 1e-6 {
                stops.extend([-base / rate, (front_length - base) / rate]);
            }
            if opacity.manages_bond(front_bond) {
                stops.extend((1..16).map(|i| low + (high - low) * i as f32 / 16.));
            }
            stops.retain(|s| (low..=high).contains(s));
            stops.sort_by(f32::total_cmp);
            stops.dedup();
            // Flat drawings, layered or not, order bonds without XYZ depth; there
            // only real crossings knock out ink, not bonds that merely pass close.
            let flat = stops.iter().all(|s| {
                let t = (s / length).clamp(0., 1.);
                let z = depth::at(doc, rear_bond, t);
                let front_z = depth::at(doc, front_bond, front_t(point(*s)));
                z.zip(front_z)
                    .is_none_or(|(z, front_z)| (z - front_z).abs() <= 0.001)
            });
            if flat && !centerlines_cross(from, to, over_from, over_to) {
                continue;
            }
            let boundary = |hidden: f32, shown: f32| {
                let (mut hidden, mut shown) = (hidden, shown);
                for _ in 0..16 {
                    let middle = (hidden + shown) * 0.5;
                    if behind(middle) {
                        hidden = middle;
                    } else {
                        shown = middle;
                    }
                }
                shown
            };
            // Pieces shorter than a painted stroke are dropped. Hashes and hollow
            // wedges draw thin strokes across a bold-width envelope.
            let min = if under.separate {
                rail.width
            } else {
                doc.drawing_style.line_width().min(rail.width)
            };
            let mut run: Option<f32> = None;
            let mut previous = low;
            for stop in stops {
                match (run, behind(stop)) {
                    (None, true) => {
                        run = Some(if stop == low {
                            low
                        } else {
                            boundary(stop, previous)
                        })
                    }
                    (Some(start), false) => {
                        spans.push((start, boundary(previous, stop), filter, min));
                        run = None;
                    }
                    _ => {}
                }
                previous = stop;
            }
            if let Some(start) = run {
                spans.push((start, high, filter, min));
            }
        }
    }
    spans
        .into_iter()
        .map(|(low, high, rail, min)| Gap {
            origin: from,
            axis,
            low,
            high,
            rail,
            min,
        })
        .collect()
}
fn centerlines_cross(a: Point, b: Point, c: Point, d: Point) -> bool {
    let (ab, cd, ac) = (delta(a, b), delta(c, d), delta(a, c));
    let determinant = cross(ab, cd);
    if determinant.abs() < 0.001 {
        return false;
    }
    let (t, u) = (cross(ac, cd) / determinant, cross(ac, ab) / determinant);
    (-0.0001..=1.0001).contains(&t) && (-0.0001..=1.0001).contains(&u)
}
fn projection(p: Point, g: Gap) -> f32 {
    (p.x - g.origin.x) * g.axis.x + (p.y - g.origin.y) * g.axis.y
}
fn intersection(a: Point, b: Point, pa: f32, pb: f32, bound: f32) -> Point {
    let t = if (pb - pa).abs() > 0.00001 {
        (bound - pa) / (pb - pa)
    } else {
        0.
    };
    Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
}
fn path_half(
    commands: &[crate::graphics::PathCommand],
    g: Gap,
    bound: f32,
    below: bool,
) -> Vec<crate::graphics::PathCommand> {
    use crate::graphics::PathCommand::{self, Cubic, Line, Move};
    let inside = |p| {
        if below {
            projection(p, g) <= bound
        } else {
            projection(p, g) >= bound
        }
    };
    let lerp = |a: Point, b: Point, t: f32| a.offset((b.x - a.x) * t, (b.y - a.y) * t);
    let mut out = vec![];
    let mut cursor = None;
    let mut pen = None;
    let mut subpath_start = None;
    for command in commands {
        // A closed outline has a real final edge. Clip it like every other
        // segment, otherwise hollow wedges lose their narrow cap at crossings.
        let closing_edge;
        let command = if matches!(command, PathCommand::Close) {
            let Some(start) = subpath_start else {
                continue;
            };
            closing_edge = Line(start);
            &closing_edge
        } else {
            command
        };
        let end = match command {
            Move(p) => {
                subpath_start = Some(*p);
                cursor = Some(*p);
                pen = None;
                continue;
            }
            Line(p) | Cubic(_, _, p) => *p,
            PathCommand::Close => continue,
        };
        let Some(start) = cursor.replace(end) else {
            continue;
        };
        let mut from = start;
        let clipped = match (inside(start), inside(end)) {
            (false, false) => {
                pen = None;
                continue;
            }
            (true, true) => command.clone(),
            (keep_start, _) => {
                // Bond curves advance linearly along their axis; split the cubic
                // itself so a crossing gap does not introduce polygonal edges.
                let pa = projection(start, g);
                let pb = projection(end, g);
                let t = ((bound - pa) / (pb - pa)).clamp(0., 1.);
                match command {
                    Cubic(a, b, _) => {
                        let a0 = lerp(start, *a, t);
                        let a1 = lerp(*a, *b, t);
                        let a2 = lerp(*b, end, t);
                        let b0 = lerp(a0, a1, t);
                        let b1 = lerp(a1, a2, t);
                        let cut = lerp(b0, b1, t);
                        if keep_start {
                            Cubic(a0, b0, cut)
                        } else {
                            from = cut;
                            Cubic(b1, a2, end)
                        }
                    }
                    _ => {
                        let cut = lerp(start, end, t);
                        if keep_start {
                            Line(cut)
                        } else {
                            from = cut;
                            Line(end)
                        }
                    }
                }
            }
        };
        if pen != Some(from) {
            out.push(Move(from));
        }
        pen = match clipped {
            Line(p) | Cubic(_, _, p) => Some(p),
            _ => None,
        };
        out.push(clipped);
    }
    out
}
fn half(primitive: &Primitive, g: Gap, bound: f32, below: bool) -> Option<Primitive> {
    let inside = |value: f32| {
        if below {
            value <= bound
        } else {
            value >= bound
        }
    };
    match primitive {
        Primitive::Path {
            commands,
            style,
            filled: false,
        } => {
            let commands = path_half(commands, g, bound, below);
            (!commands.is_empty()).then(|| Primitive::Path {
                commands,
                style: style.clone(),
                filled: false,
            })
        }
        Primitive::Line(a, b, width) => {
            let pa = projection(*a, g);
            let pb = projection(*b, g);
            match (inside(pa), inside(pb)) {
                (true, true) => Some(primitive.clone()),
                (false, false) => None,
                (true, false) => Some(Primitive::Line(
                    *a,
                    intersection(*a, *b, pa, pb, bound),
                    *width,
                )),
                (false, true) => Some(Primitive::Line(
                    intersection(*a, *b, pa, pb, bound),
                    *b,
                    *width,
                )),
            }
        }
        Primitive::Polygon(points) => {
            let mut clipped = vec![];
            for (a, b) in points
                .iter()
                .zip(points.iter().cycle().skip(1))
                .take(points.len())
            {
                let pa = projection(*a, g);
                let pb = projection(*b, g);
                if inside(pa) {
                    clipped.push(*a);
                }
                if inside(pa) != inside(pb) {
                    clipped.push(intersection(*a, *b, pa, pb, bound));
                }
            }
            (clipped.len() >= 3).then_some(Primitive::Polygon(clipped))
        }
        _ => Some(primitive.clone()),
    }
}
/// Whether a bond piece lies on the rail a gap cuts.
fn on_rail(primitive: &Primitive, gap: &Gap) -> bool {
    let Some((rail, tolerance)) = gap.rail else {
        return true;
    };
    let off = |p: &Point| cross(gap.axis, delta(gap.origin, *p)) - rail.at(projection(*p, *gap));
    // Crossed diagonals lean opposite ways, and cuts and label trimming keep
    // that, even for a short piece near where they cross.
    if let (true, Primitive::Line(a, b, _)) = (rail.crossed, primitive) {
        let rise = cross(gap.axis, delta(*a, *b)) * (projection(*b, *gap) - projection(*a, *gap));
        return rise * (rail.offset.1 - rail.offset.0) > 0.;
    }
    let points: Vec<Point> = match primitive {
        Primitive::Line(a, b, _) => vec![*a, *b],
        Primitive::Polygon(points) => points.clone(),
        Primitive::Path { commands, .. } => commands
            .iter()
            .flat_map(crate::graphics::PathCommand::iter_points)
            .collect(),
        _ => return true,
    };
    if points.is_empty() {
        return true;
    }
    (points.iter().map(off).sum::<f32>() / points.len() as f32).abs() <= tolerance
}
/// Extent of a bond piece along a gap's axis.
fn span(primitive: &Primitive, gap: &Gap) -> f32 {
    let points: Vec<Point> = match primitive {
        Primitive::Line(a, b, _) => vec![*a, *b],
        Primitive::Polygon(points) => points.clone(),
        Primitive::Path { commands, .. } => commands
            .iter()
            .flat_map(crate::graphics::PathCommand::iter_points)
            .collect(),
        _ => return f32::INFINITY,
    };
    let (lo, hi) = points
        .iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), p| {
            let s = projection(*p, *gap);
            (lo.min(s), hi.max(s))
        });
    hi - lo
}
/// Trim real geometry, leaving transparent gaps in vector and raster exports.
pub fn cut(mut primitives: Vec<Primitive>, gaps: &[Gap]) -> Vec<Primitive> {
    for gap in gaps {
        primitives = primitives
            .iter()
            .flat_map(|p| {
                let halves = if matches!(
                    p,
                    Primitive::Line(..)
                        | Primitive::Polygon(_)
                        | Primitive::Path { filled: false, .. }
                ) && on_rail(p, gap)
                {
                    // Drop pieces the gap cuts down to less than a stroke
                    // width, so no dots or stubs remain.
                    let whole = span(p, gap);
                    [half(p, *gap, gap.low, true), half(p, *gap, gap.high, false)].map(|piece| {
                        piece.filter(|piece| {
                            let length = span(piece, gap);
                            length >= gap.min || length >= whole - 0.0001
                        })
                    })
                } else {
                    [Some(p.clone()), None]
                };
                halves.into_iter().flatten()
            })
            .collect();
    }
    primitives
}
#[cfg(test)]
mod tests;
