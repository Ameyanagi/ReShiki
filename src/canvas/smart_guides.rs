//! Smart guides: while dragging, an object's edges, center and gaps snap to
//! those of the other objects on screen, as in PowerPoint or Keynote. Objects
//! are the Align menu's units (`editing::groups`) with their visible bounds.
use super::{Camera, guides::Unit, layered::Frame};
use iced::widget::canvas::{self, Path, Stroke};
use iced::{Color, Point, Rectangle, Size, Vector};
use reshiki::document::{Document, Point as World};
use std::collections::HashSet;

/// Snapping reach in screen pixels, so it feels the same at any zoom.
const THRESHOLD: f32 = 6.;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Axis {
    X,
    Y,
}
impl Axis {
    pub const BOTH: [Self; 2] = [Self::X, Self::Y];
    fn of(self, p: World) -> f32 {
        match self {
            Self::X => p.x,
            Self::Y => p.y,
        }
    }
    fn cross(self) -> Self {
        match self {
            Self::X => Self::Y,
            Self::Y => Self::X,
        }
    }
    /// The world point at `along` on this axis and `across` on the other.
    fn point(self, along: f32, across: f32) -> World {
        match self {
            Self::X => World::new(along, across),
            Self::Y => World::new(across, along),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Bounds {
    pub lo: World,
    pub hi: World,
}
impl Bounds {
    fn lo(self, axis: Axis) -> f32 {
        axis.of(self.lo)
    }
    fn hi(self, axis: Axis) -> f32 {
        axis.of(self.hi)
    }
    /// Low edge, center and high edge.
    fn lines(self, axis: Axis) -> [f32; 3] {
        let (lo, hi) = (self.lo(axis), self.hi(axis));
        [lo, (lo + hi) / 2., hi]
    }
    pub fn translated(self, d: World) -> Self {
        Self {
            lo: self.lo.offset(d.x, d.y),
            hi: self.hi.offset(d.x, d.y),
        }
    }
    /// Shares a row along `axis` (or a column): their extents across it overlap.
    fn shares_row(self, other: Self, axis: Axis) -> bool {
        let cross = axis.cross();
        self.lo(cross) < other.hi(cross) && self.hi(cross) > other.lo(cross)
    }
    fn intersects(self, other: Self) -> bool {
        Axis::BOTH
            .into_iter()
            .all(|a| self.lo(a) <= other.hi(a) && self.hi(a) >= other.lo(a))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Guide {
    /// Objects share the line `at` on `axis`; the guide spans `from..to` across it.
    Align {
        axis: Axis,
        at: f32,
        from: f32,
        to: f32,
    },
    /// An equal gap `from..to` along `axis`, marked at `at` across it.
    Gap {
        axis: Axis,
        from: f32,
        to: f32,
        at: f32,
    },
}

/// Alignment units split into those a drag moves and the rest.
#[derive(Debug)]
pub(super) struct Layout {
    /// Visible bounds of the dragged objects before the drag.
    pub moving: Bounds,
    /// The drag moves whole units only, never part of a molecule.
    pub whole: bool,
    units: Vec<(Bounds, bool)>,
}
impl Layout {
    pub fn new(doc: &Document, ids: &[u64]) -> Option<Self> {
        let (lo, hi) = reshiki::scene::selection_bounds(doc, ids)?;
        let dragged: HashSet<_> = doc.expand_abbreviation_selection(ids).into_iter().collect();
        let groups = reshiki::editing::groups(doc, &doc.all_ids());
        let mut whole = true;
        let mut units = Vec::new();
        for (unit, bounds) in groups
            .iter()
            .zip(reshiki::scene::selections_bounds(doc, &groups))
        {
            let inside = unit.iter().filter(|id| dragged.contains(id)).count();
            whole &= inside == 0 || inside == unit.len();
            if let Some((lo, hi)) = bounds {
                units.push((Bounds { lo, hi }, inside > 0));
            }
        }
        Some(Self {
            moving: Bounds { lo, hi },
            whole,
            units,
        })
    }

    /// Units within `view` a drag can snap to. A copy leaves its originals in place.
    pub fn targets(&self, view: Bounds, copy: bool) -> Vec<Bounds> {
        self.units
            .iter()
            .filter(|(b, moved)| (copy || !moved) && b.intersects(view))
            .map(|(b, _)| *b)
            .collect()
    }
}

/// The nearest object before (or after) `q` in its row along `axis`, allowing
/// `slack` of overlap. `skip` is `q` itself when it is one of `others`.
fn neighbour(
    q: Bounds,
    others: &[Bounds],
    skip: Option<usize>,
    axis: Axis,
    after: bool,
    slack: f32,
) -> Option<(usize, Bounds)> {
    let row = others
        .iter()
        .copied()
        .enumerate()
        .filter(|&(i, r)| Some(i) != skip && r.shares_row(q, axis));
    if after {
        row.filter(|(_, r)| r.lo(axis) >= q.hi(axis) - slack)
            .min_by(|a, b| a.1.lo(axis).total_cmp(&b.1.lo(axis)))
    } else {
        row.filter(|(_, r)| r.hi(axis) <= q.lo(axis) + slack)
            .max_by(|a, b| a.1.hi(axis).total_cmp(&b.1.hi(axis)))
    }
}

/// The drag offset `requested` of `moving`, snapped along `axes` to the nearest
/// matching line or equal gap within reach. `pixel` is one screen pixel in world units.
pub(super) fn snap(
    moving: Bounds,
    requested: World,
    others: &[Bounds],
    axes: &[Axis],
    pixel: f32,
) -> World {
    let raw = moving.translated(requested);
    let reach = THRESHOLD * pixel;
    let mut delta = requested;
    for &axis in axes {
        let [lo, center, hi] = raw.lines(axis);
        let size = hi - lo;
        // Edges match edges and centers match centers, never an edge and a center.
        let mut offsets: Vec<f32> = others
            .iter()
            .flat_map(|q| {
                let [a, c, b] = q.lines(axis);
                [a - lo, b - lo, a - hi, b - hi, c - center]
            })
            .collect();
        let before = neighbour(raw, others, None, axis, false, reach);
        let after = neighbour(raw, others, None, axis, true, reach);
        if let (Some((_, b)), Some((_, a))) = (before, after) {
            offsets.push((b.hi(axis) + a.lo(axis) - size) / 2. - lo);
        }
        if let Some((i, b)) = before
            && let Some((_, b2)) = neighbour(b, others, Some(i), axis, false, pixel / 2.)
        {
            offsets.push(b.hi(axis) + (b.lo(axis) - b2.hi(axis)) - lo);
        }
        if let Some((i, a)) = after
            && let Some((_, a2)) = neighbour(a, others, Some(i), axis, true, pixel / 2.)
        {
            offsets.push(a.lo(axis) - (a2.lo(axis) - a.hi(axis)) - size - lo);
        }
        if let Some(best) = offsets
            .into_iter()
            .filter(|d| d.abs() < reach)
            .min_by(|a, b| a.abs().total_cmp(&b.abs()))
        {
            delta = match axis {
                Axis::X => delta.offset(best, 0.),
                Axis::Y => delta.offset(0., best),
            };
        }
    }
    delta
}

/// Guides for the exact matches of `moved` at its final position.
pub(super) fn guides(moved: Bounds, others: &[Bounds], pixel: f32) -> Vec<Guide> {
    let exact = pixel / 2.;
    let mut guides = Vec::new();
    for axis in Axis::BOTH {
        let cross = axis.cross();
        let lines = moved.lines(axis);
        // Edges match either edge; the center matches the center.
        let matches = |q: &Bounds, line: usize, v: f32| {
            let [a, c, b] = q.lines(axis);
            let same = |t: f32| (t - v).abs() < exact;
            if line == 1 {
                same(c)
            } else {
                same(a) || same(b)
            }
        };
        let [_, center, _] = lines;
        let centred: Vec<usize> = (0..others.len())
            .zip(others)
            .filter(|(_, q)| matches(q, 1, center))
            .map(|(k, _)| k)
            .collect();
        for (line, at) in lines.into_iter().enumerate() {
            // A same-size neighbour matches on all three lines; its center guide says enough.
            let group: Vec<_> = (0..others.len())
                .zip(others)
                .filter(|(k, q)| matches(q, line, at) && (line == 1 || !centred.contains(k)))
                .map(|(_, q)| *q)
                .collect();
            if !group.is_empty() {
                guides.push(Guide::Align {
                    axis,
                    at,
                    from: group
                        .iter()
                        .fold(moved.lo(cross), |m, q| m.min(q.lo(cross))),
                    to: group
                        .iter()
                        .fold(moved.hi(cross), |m, q| m.max(q.hi(cross))),
                });
            }
        }
        // The dragged object's gaps first, then its neighbours' outer gaps.
        let before = neighbour(moved, others, None, axis, false, exact);
        let after = neighbour(moved, others, None, axis, true, exact);
        let mut gaps: Vec<_> = before
            .map(|(_, b)| (b, moved))
            .into_iter()
            .chain(after.map(|(_, a)| (moved, a)))
            .collect();
        let mine = gaps.len();
        if let Some((i, b)) = before
            && let Some((_, b2)) = neighbour(b, others, Some(i), axis, false, exact)
        {
            gaps.push((b2, b));
        }
        if let Some((i, a)) = after
            && let Some((_, a2)) = neighbour(a, others, Some(i), axis, true, exact)
        {
            gaps.push((a, a2));
        }
        let size = |(p, q): &(Bounds, Bounds)| q.lo(axis) - p.hi(axis);
        // A gap shows when it equals another and one of the two is the dragged object's.
        let own = |k: usize, gap| k < mine && size(gap) > pixel;
        for (k, gap) in gaps.iter().enumerate() {
            let shown = gaps.iter().enumerate().any(|(j, other)| {
                k != j && (size(gap) - size(other)).abs() < exact && (own(k, gap) || own(j, other))
            });
            if shown {
                let (p, q) = gap;
                let (lo, hi) = (p.lo(cross).max(q.lo(cross)), p.hi(cross).min(q.hi(cross)));
                guides.push(Guide::Gap {
                    axis,
                    from: p.hi(axis),
                    to: q.lo(axis),
                    at: (lo + hi) / 2.,
                });
            }
        }
    }
    guides
}

/// A dragged point, such as an arrow end, snapped to other objects' edges and
/// centers. From `origin` (fixed angles) it only slides along its direction.
pub(super) fn snap_point(p: World, others: &[Bounds], origin: Option<World>, pixel: f32) -> World {
    let reach = THRESHOLD * pixel;
    let lines = |axis: Axis| others.iter().flat_map(move |q| q.lines(axis));
    let Some(origin) = origin else {
        let nearest = |axis: Axis| {
            let v = axis.of(p);
            lines(axis)
                .filter(|t| (t - v).abs() < reach)
                .min_by(|a, b| (a - v).abs().total_cmp(&(b - v).abs()))
                .unwrap_or(v)
        };
        return World::new(nearest(Axis::X), nearest(Axis::Y));
    };
    let length = origin.distance(p);
    if !length.is_finite() || length <= 1e-6 {
        return p;
    }
    let u = World::new((p.x - origin.x) / length, (p.y - origin.y) / length);
    Axis::BOTH
        .into_iter()
        .filter(|&axis| axis.of(u).abs() > 1e-3)
        .flat_map(|axis| lines(axis).map(move |t| (t - axis.of(origin)) / axis.of(u)))
        .filter(|s| *s > 0.)
        .map(|s| origin.offset(u.x * s, u.y * s))
        .filter(|q| q.distance(p) < reach)
        .min_by(|a, b| a.distance(p).total_cmp(&b.distance(p)))
        .unwrap_or(p)
}

/// Guides through a dragged point at its final position.
pub(super) fn point_guides(p: World, others: &[Bounds], pixel: f32) -> Vec<Guide> {
    Axis::BOTH
        .into_iter()
        .filter_map(|axis| {
            let (cross, at) = (axis.cross(), axis.of(p));
            let hits: Vec<_> = others
                .iter()
                .filter(|q| q.lines(axis).iter().any(|t| (t - at).abs() < pixel / 2.))
                .collect();
            (!hits.is_empty()).then(|| Guide::Align {
                axis,
                at,
                from: hits.iter().fold(cross.of(p), |m, q| m.min(q.lo(cross))),
                to: hits.iter().fold(cross.of(p), |m, q| m.max(q.hi(cross))),
            })
        })
        .collect()
}

/// Magenta guides, distinct from the teal selection and readable on both canvases.
pub(super) fn draw(
    frame: &mut Frame<'_>,
    guides: &[Guide],
    camera: Camera,
    bounds: Rectangle,
    unit: Unit,
    dark: bool,
) {
    let color = guide_color(dark);
    let stroke = Stroke::default().with_width(1.).with_color(color);
    let screen =
        |axis: Axis, along: f32, across: f32| camera.screen(axis.point(along, across), bounds);
    for guide in guides {
        match *guide {
            Guide::Align { axis, at, from, to } => {
                // Past the objects' extent a little, as in the mockup.
                let out = match axis {
                    Axis::X => Vector::new(0., 8.),
                    Axis::Y => Vector::new(8., 0.),
                };
                frame.stroke(
                    &Path::line(
                        screen(axis.cross(), from, at) - out,
                        screen(axis.cross(), to, at) + out,
                    ),
                    stroke,
                );
            }
            Guide::Gap { axis, from, to, at } => {
                let (a, b) = (screen(axis, from, at), screen(axis, to, at));
                let tick = match axis {
                    Axis::X => Vector::new(0., 5.),
                    Axis::Y => Vector::new(5., 0.),
                };
                draw_dimension(frame, a, b, tick, color);
                let label = unit.format(to - from);
                let size = badge_size(&label);
                let middle = Point::new((a.x + b.x) / 2., (a.y + b.y) / 2.);
                let corner = match axis {
                    Axis::X => Point::new(middle.x - size.width / 2., middle.y - 22.),
                    Axis::Y => Point::new(middle.x + 8., middle.y - size.height / 2.),
                };
                draw_badge(frame, label, corner, size, color);
            }
        }
    }
}

/// The selected label's actual clearance, drawn only as an editing overlay.
/// `doc` is the final reconciled gesture preview, not the pointer's raw target.
pub(super) fn draw_arrow_label_gaps(
    frame: &mut Frame<'_>,
    doc: &Document,
    selected: &[u64],
    camera: Camera,
    bounds: Rectangle,
    unit: Unit,
    dark: bool,
) {
    let color = guide_color(dark);
    let stroke = Stroke::default().with_width(1.).with_color(color);
    for dimension in label_gap_dimensions(doc, selected, camera, bounds) {
        draw_dimension(
            frame,
            dimension.from,
            dimension.to,
            dimension.tangent * 5.,
            color,
        );
        // Short witness marks reach the projected label edge without crossing its ink.
        for end in [dimension.from, dimension.to] {
            frame.stroke(
                &Path::line(end - dimension.tangent * 12., end - dimension.tangent * 5.),
                stroke,
            );
        }
        let label = unit.format(dimension.world);
        let size = badge_size(&label);
        draw_badge(frame, label, dimension.badge_corner(size), size, color);
    }
}

#[derive(Debug, Clone, Copy)]
struct LabelGapDimension {
    from: Point,
    to: Point,
    tangent: Vector,
    normal: Vector,
    world: f32,
}
impl LabelGapDimension {
    fn new(
        gap: reshiki::arrow_labels::GapGeometry,
        side: reshiki::arrow_labels::Side,
        camera: Camera,
        bounds: Rectangle,
    ) -> Self {
        // Put dimensions on the same screen side when an arrow reverses.
        let mut tangent = Vector::new(gap.tangent.x, gap.tangent.y);
        if tangent.x < -1e-6 || (tangent.x.abs() <= 1e-6 && tangent.y < 0.) {
            tangent = -tangent;
        }
        // Above is visually up (left for vertical arrows), including a zero gap
        // where the dimension endpoints cannot supply a direction themselves.
        let mut normal = Vector::new(tangent.y, -tangent.x);
        if normal.y > 1e-6 || (normal.y.abs() <= 1e-6 && normal.x > 0.) {
            normal = -normal;
        }
        if side == reshiki::arrow_labels::Side::Below {
            normal = -normal;
        }
        let offset = tangent * (gap.half_span * camera.zoom + 12.);
        Self {
            from: camera.screen(gap.from, bounds) + offset,
            to: camera.screen(gap.to, bounds) + offset,
            tangent,
            normal,
            world: gap.from.distance(gap.to),
        }
    }
    fn badge_corner(self, size: Size) -> Point {
        let extent = |axis: Vector| (axis.x.abs() * size.width + axis.y.abs() * size.height) / 2.;
        let middle = Point::new(
            (self.from.x + self.to.x) / 2.,
            (self.from.y + self.to.y) / 2.,
        );
        let center = middle
            + self.tangent * (extent(self.tangent) + 8.)
            + self.normal * (extent(self.normal) + 4.);
        center - Vector::new(size.width / 2., size.height / 2.)
    }
}

fn label_gap_dimensions<'a>(
    doc: &'a Document,
    selected: &'a [u64],
    camera: Camera,
    bounds: Rectangle,
) -> impl Iterator<Item = LabelGapDimension> + 'a {
    // Only attachment metadata is visited; no document clone, scene generation
    // or workspace bounds scan is needed for this selection-only decoration.
    doc.arrow_labels
        .iter()
        .filter(move |label| selected.contains(&label.annotation))
        .filter_map(move |label| {
            reshiki::arrow_labels::gap_geometry(doc, label.annotation)
                .map(|gap| LabelGapDimension::new(gap, label.side, camera, bounds))
        })
}

fn guide_color(dark: bool) -> Color {
    // The frame inverts lightness on the dark canvas; pre-invert its brighter pink.
    if dark {
        crate::appearance::color(true, Color::from_rgb8(255, 111, 163))
    } else {
        Color::from_rgb8(210, 58, 116)
    }
}

fn draw_dimension(frame: &mut Frame<'_>, from: Point, to: Point, tick: Vector, color: Color) {
    let stroke = Stroke::default().with_width(1.).with_color(color);
    frame.stroke(&Path::line(from, to), stroke);
    for end in [from, to] {
        frame.stroke(&Path::line(end - tick, end + tick), stroke);
    }
}

fn badge_size(label: &str) -> Size {
    Size::new(crate::app::text_width(label, 11.) + 10., 16.)
}

fn draw_badge(frame: &mut Frame<'_>, label: String, corner: Point, size: Size, color: Color) {
    frame.fill(&Path::rounded_rectangle(corner, size, 4.into()), color);
    frame.fill_text(canvas::Text {
        content: label,
        position: corner + Vector::new(size.width / 2., size.height / 2.),
        color: Color::WHITE,
        size: 11.into(),
        font: iced::Font::with_name(reshiki::style::ui_font_family()),
        align_x: iced::alignment::Horizontal::Center.into(),
        align_y: iced::alignment::Vertical::Center,
        ..Default::default()
    });
}

#[cfg(test)]
mod tests;
