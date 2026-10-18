//! Drawing depth at crossings; never changes chemical connectivity.
use crate::{
    document::{Document, Point},
    scene::Primitive,
    style::{DEFAULT, DrawingStyle},
};
mod depth;
pub use depth::bond_over;
mod ring_strokes;
pub(crate) use ring_strokes::ring_stroke;
#[derive(Debug, Clone, Copy)]
pub struct Gap {
    origin: Point,
    axis: Point,
    low: f32,
    high: f32,
}
fn cross(a: Point, b: Point) -> f32 {
    a.x * b.y - a.y * b.x
}
fn delta(a: Point, b: Point) -> Point {
    Point::new(b.x - a.x, b.y - a.y)
}
/// Outermost rail centerlines across a bond's scene normal (-uy, ux), and
/// the stroke width.
#[derive(Clone, Copy)]
struct Rails {
    lo: f32,
    hi: f32,
    width: f32,
}
/// Where the scene places a bond's second rail: on one side (`Some(±1.)`),
/// centered (`None`), or absent (`Some(0.)`, an aromatic bond drawn as one
/// line beside its ring circle).
type Side = Option<f32>;
fn rails(bond: &crate::document::Bond, style: &DrawingStyle, side: Side) -> Rails {
    let display = bond.base_display();
    let width = if ["bold", "wedge", "hollow_wedge", "hashed", "hash"].contains(&display) {
        style.world(style.bold_width_pt)
    } else {
        style.line_width()
    };
    let spacing = style.bond_length_world * style.bond_spacing_ratio;
    let (lo, hi) = match (bond.order, side) {
        (2 | 4 | 7, Some(side)) => ((side * spacing).min(0.), (side * spacing).max(0.)),
        (2 | 4 | 7, None) => (-spacing / 2., spacing / 2.),
        (3, _) => (-spacing, spacing),
        (6, _) => (-spacing * 1.5, spacing * 1.5),
        _ => (0., 0.),
    };
    Rails { lo, hi, width }
}
/// Resolve rail sides lazily: most crossings involve only single bonds.
struct Sides<'d> {
    doc: &'d Document,
    faces: std::cell::OnceCell<crate::scene::projected_bonds::FaceIndex<'d>>,
    /// `None` keeps aromatic bonds centered instead of finding ring circles.
    circles: Option<std::cell::OnceCell<Vec<crate::aromatic::Circle>>>,
}
impl<'d> Sides<'d> {
    fn new(doc: &'d Document) -> Self {
        Self {
            doc,
            faces: Default::default(),
            circles: Some(Default::default()),
        }
    }
    /// Per-circle callers skip the whole-document circle search.
    fn doubles(doc: &'d Document) -> Self {
        Self {
            circles: None,
            ..Self::new(doc)
        }
    }
    fn faces(&self) -> &crate::scene::projected_bonds::FaceIndex<'d> {
        self.faces
            .get_or_init(|| crate::scene::projected_bonds::FaceIndex::new(self.doc))
    }
    /// Mirrors the scene's rail placement: a positioned or face-resolved
    /// double keeps its second rail on one side, and an aromatic bond
    /// without a ring circle draws its inner line toward the ring.
    fn rails(&self, bond: &crate::document::Bond) -> Rails {
        use crate::bonds::DoublePosition::{Left, Right};
        let side = match bond.order {
            2 | 7 => match crate::scene::double_position(self.faces(), bond) {
                Left => Some(-1.),
                Right => Some(1.),
                _ => None,
            },
            4 => self.circles.as_ref().map(|circles| {
                if circles
                    .get_or_init(|| crate::aromatic::circles(self.doc))
                    .iter()
                    .any(|c| c.contains_bond(bond.a, bond.b))
                {
                    0.
                } else {
                    crate::scene::automatic_double_side(self.faces(), bond).unwrap_or(1.)
                }
            }),
            _ => None,
        };
        rails(bond, &self.doc.drawing_style, side)
    }
}
/// Interval along the under bond's unit axis, relative to where the two
/// centerlines cross, that clears every over-ink offset in `over_band`
/// (across the over bond's normal) for every under rail in `under_rails`.
/// Offset rails cross the over ink away from the centerline crossing, so
/// a symmetric cut leaves side rails of double bonds touching.
fn clearance(
    over: Point,
    under: Point,
    over_band: (f32, f32),
    under_rails: (f32, f32),
) -> (f32, f32) {
    let normal = Point::new(-over.y, over.x);
    let sine = normal.x * under.x + normal.y * under.y;
    // Nearly parallel bonds keep the established bounded cut.
    let sine = sine.signum() * sine.abs().max(0.15);
    let cosine = -normal.x * under.y + normal.y * under.x;
    let mut low = f32::INFINITY;
    let mut high = f32::NEG_INFINITY;
    for o in [over_band.0, over_band.1] {
        for q in [under_rails.0, under_rails.1] {
            let s = (o - q * cosine) / sine;
            low = low.min(s);
            high = high.max(s);
        }
    }
    (low, high)
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
            let (under, from, to, parameter, over) = if first_over {
                (*j, *c, *d, u, first)
            } else {
                (i, a, b, t, second)
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
            let over_length = over_from.distance(over_to);
            if length < 0.001 || over_length < 0.001 {
                continue;
            }
            let axis = Point::new((to.x - from.x) / length, (to.y - from.y) / length);
            let over_axis = Point::new(
                (over_to.x - over_from.x) / over_length,
                (over_to.y - over_from.y) / over_length,
            );
            let Some(under_bond) = doc.bonds.get(under) else {
                continue;
            };
            // Clear the whole front ink, including a double bond's side rail
            // or an aromatic inner line, for every rail of the rear bond.
            let ink = sides.rails(over);
            let margin = ink.width * 0.5 + DEFAULT.world(1.1);
            let rear = sides.rails(under_bond);
            let (low, high) = clearance(
                over_axis,
                axis,
                (ink.lo - margin, ink.hi + margin),
                (rear.lo, rear.hi),
            );
            if let Some(list) = gaps.get_mut(under) {
                list.push(Gap {
                    origin: from,
                    axis,
                    low: parameter * length + low,
                    high: parameter * length + high,
                });
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
                ) {
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
