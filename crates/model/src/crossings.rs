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
    /// For a multi-rail bond, the one rail (normal offset, tolerance) this
    /// gap cuts: its rails pass under the front ink at different places.
    rail: Option<(f32, f32)>,
}
fn cross(a: Point, b: Point) -> f32 {
    a.x * b.y - a.y * b.x
}
fn delta(a: Point, b: Point) -> Point {
    Point::new(b.x - a.x, b.y - a.y)
}
/// Rail centerline offsets across a bond's scene normal (-uy, ux), as the
/// scene draws them, and the stroke width.
pub(crate) struct Rails {
    offsets: Vec<f32>,
    width: f32,
}
impl Rails {
    fn lo(&self) -> f32 {
        self.offsets.iter().copied().fold(0., f32::min)
    }
    fn hi(&self) -> f32 {
        self.offsets.iter().copied().fold(0., f32::max)
    }
    /// Each rail offset with its gap filter; a single rail needs none.
    fn filters(&self) -> Vec<(f32, Option<(f32, f32)>)> {
        let mut nearest = f32::INFINITY;
        for (i, a) in self.offsets.iter().enumerate() {
            for b in self.offsets.iter().skip(i + 1) {
                nearest = nearest.min((a - b).abs());
            }
        }
        self.offsets
            .iter()
            .map(|q| (*q, (self.offsets.len() > 1).then_some((*q, nearest * 0.5))))
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
    /// its second rail on one side (foreshortened within a projected face),
    /// and an aromatic bond without a ring circle draws its inner line
    /// toward the ring.
    fn rails(&self, bond: &crate::document::Bond) -> Rails {
        use crate::bonds::DoublePosition::{Auto, Left, Right};
        let style = &self.doc.drawing_style;
        let display = bond.base_display();
        let width = if ["bold", "wedge", "hollow_wedge", "hashed", "hash"].contains(&display) {
            style.world(style.bold_width_pt)
        } else {
            style.line_width()
        };
        let spacing = style.bond_length_world * style.bond_spacing_ratio;
        let offsets = match bond.order {
            2 | 7 => match crate::scene::double_position(self.faces(), bond) {
                side @ (Left | Right) => {
                    let offset = if side == Left { -spacing } else { spacing };
                    match self.projected(bond, offset, width, bond.double_position == Auto) {
                        Some(Some(projected)) => vec![0., projected],
                        Some(None) => vec![0.],
                        None => vec![0., offset],
                    }
                }
                _ => vec![-spacing / 2., spacing / 2.],
            },
            4 if self
                .circles
                .get_or_init(|| crate::aromatic::circles(self.doc))
                .iter()
                .any(|c| c.contains_bond(bond.a, bond.b)) =>
            {
                vec![0.]
            }
            4 => {
                let side = crate::scene::automatic_double_side(self.faces(), bond).unwrap_or(1.);
                vec![0., side * spacing]
            }
            3 => vec![-spacing, 0., spacing],
            6 => vec![-spacing * 1.5, -spacing * 0.5, spacing * 0.5, spacing * 1.5],
            _ => vec![0.],
        };
        Rails { offsets, width }
    }
    /// The screen offset of an automatic double bond's rail within its
    /// projected face: `None` keeps the 2D offset, `Some(None)` omits it.
    fn projected(
        &self,
        bond: &crate::document::Bond,
        offset: f32,
        width: f32,
        automatic: bool,
    ) -> Option<Option<f32>> {
        if !automatic {
            return None;
        }
        let a = self.doc.atom(bond.a)?.position;
        let b = self.doc.atom(bond.b)?.position;
        let length = a.distance(b).max(0.001);
        let rail = self
            .faces()
            .rail_depth(bond, a, b, offset, 0., width / 2.)?;
        Some(rail.map(|rail| cross(delta(a, b), delta(a, rail.points.0)) / length))
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
/// Parameter interval along the rail at normal offset `rail` of the under
/// bond (`origin` + s·`axis`) that lies inside the over stroke's finite ink.
/// Each rail meets the over ink at its own place, and the finite length keeps
/// a shallow cut off ink that is not there.
fn clearance(over: Stroke, origin: Point, axis: Point, rail: f32) -> Option<(f32, f32)> {
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
    let start = delta(over.from, origin.offset(-axis.y * rail, axis.x * rail));
    let a = range(
        dot(start, along),
        dot(axis, along),
        (-over.ends, length + over.ends),
    )?;
    let b = range(dot(start, normal), dot(axis, normal), over.band)?;
    let (low, high) = (a.0.max(b.0), a.1.min(b.1));
    (low <= high && low.is_finite() && high.is_finite()).then_some((low, high))
}
/// Sorted sweep avoids checking every pair of well-separated molecules.
pub fn gaps(doc: &Document) -> Vec<Vec<Gap>> {
    gaps_with_opacity(doc, &crate::rear_opacity::Paint::new(doc))
}

pub(crate) fn gaps_with_opacity(
    doc: &Document,
    opacity: &crate::rear_opacity::Paint,
) -> Vec<Vec<Gap>> {
    let mut gaps = vec![vec![]; doc.bonds.len()];
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
    let sides = Sides::new(doc);
    let mut budget = 200_000_usize;
    for (i, a, b) in segments {
        // Rigid rotations can put the same ring vertex a few ulps outside a
        // segment bound. Keep endpoint crossings through the broad phase.
        active.retain(|(_, c, d)| c.x.max(d.x) + 0.001 >= a.x.min(b.x));
        for (j, c, d) in &active {
            if budget == 0 {
                return gaps;
            }
            budget -= 1;
            if a.y.min(b.y) > c.y.max(d.y) + 0.001 || a.y.max(b.y) < c.y.min(d.y) - 0.001 {
                continue;
            }
            let (Some(first), Some(second)) = (doc.bonds.get(i), doc.bonds.get(*j)) else {
                continue;
            };
            if [first.a, first.b]
                .iter()
                .any(|id| *id == second.a || *id == second.b)
            {
                continue;
            }
            let ab = delta(a, b);
            let cd = delta(*c, *d);
            let ac = delta(a, *c);
            let determinant = cross(ab, cd);
            if determinant.abs() < 0.001 {
                continue;
            }
            let t = cross(ac, cd) / determinant;
            let u = cross(ac, ab) / determinant;
            let crosses = |t: f32, projected: bool| {
                (0.04..0.96).contains(&t) || projected && (-0.0001..=1.0001).contains(&t)
            };
            if !crosses(t, first.projection) || !crosses(u, second.projection) {
                continue;
            }
            let first_over = depth::bond_over(doc, i, t, *j, u);
            let (under, from, to, over) = if first_over {
                (*j, *c, *d, first)
            } else {
                (i, a, b, second)
            };
            // Transparent ink cannot knock out the ink beneath it.
            if opacity.bond(over, if first_over { t } else { u }) < 1. {
                continue;
            }
            // Opacity-managed under-ink already carries its exact visibility
            // interval. A conventional knockout would erase its faint base or
            // make the transparent case wider than the physical occlusion.
            if doc
                .bonds
                .get(under)
                .is_some_and(|bond| opacity.manages_bond(bond))
            {
                continue;
            }
            let length = from.distance(to);
            let (over_from, over_to) = if first_over { (a, b) } else { (*c, *d) };
            let Some(under_bond) = doc.bonds.get(under) else {
                continue;
            };
            if length < 0.001 {
                continue;
            }
            let axis = Point::new((to.x - from.x) / length, (to.y - from.y) / length);
            // Clear the whole front ink, including a double bond's side rail
            // or an aromatic inner line, for every rail of the rear bond.
            let ink = sides.rails(over);
            let margin = ink.width * 0.5 + DEFAULT.world(1.1);
            let front = Stroke {
                from: over_from,
                to: over_to,
                ends: margin,
                band: (ink.lo() - margin, ink.hi() + margin),
            };
            for (offset, rail) in sides.rails(under_bond).filters() {
                if let Some((low, high)) = clearance(front, from, axis, offset)
                    && let Some(list) = gaps.get_mut(under)
                {
                    list.push(Gap {
                        origin: from,
                        axis,
                        low,
                        high,
                        rail,
                    });
                }
            }
        }
        active.push((i, a, b));
    }
    gaps
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
    let Some((offset, tolerance)) = gap.rail else {
        return true;
    };
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
    let mean = points
        .iter()
        .map(|p| cross(gap.axis, delta(gap.origin, *p)))
        .sum::<f32>()
        / points.len() as f32;
    (mean - offset).abs() <= tolerance
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
                    [half(p, *gap, gap.low, true), half(p, *gap, gap.high, false)]
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
