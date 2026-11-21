//! Shape effects beyond a flat fill and stroke: ChemDraw's shading, drop
//! shadows and wavy strokes, and gradient and see-through fills.
use super::{Graphic, GraphicKind, PathCommand, flattened};
use crate::document::Point;
use serde::{Deserialize, Serialize};

/// The light grey of ChemDraw's drop shadows.
pub const SHADOW_RGB: [u8; 3] = [192, 192, 192];
/// Flat bands per shading, fine enough to read as a gradient when zoomed.
const MIN_BANDS: usize = 32;
const MAX_BANDS: usize = 256;
// ChemDraw's shading, fitted to its own renders of ellipses, rectangles and
// closed curves (within 2–5 of 255 per pixel for the first two, 14 for curves).
/// The highlight of an ellipse's shading, in its frame from the top left.
const OVAL_HIGHLIGHT: [f32; 2] = [0.225, 0.225];
/// The highlight of another shape's shading, in its bounding box.
const CURVE_HIGHLIGHT: [f32; 2] = [0.3, 0.25];
/// A rectangle shades in boxes that grow from this far in at the top left.
const BOX_HIGHLIGHT: f32 = 0.05;
/// How fast the color darkens toward the edge: white × (1 − t^2.5).
const FALLOFF: f32 = 2.5;
/// Band vertices one shading may produce.
const MAX_BAND_POINTS: usize = 400_000;
/// Band vertices all the shadings of one drawing share, so a ChemDraw library
/// with hundreds of shaded shapes stays within the canvas's GPU buffers.
const DRAWING_BAND_POINTS: usize = 1_000_000;
/// Half-waves one wavy path may produce; longer paths get longer waves.
const MAX_WAVES: f32 = 10_000.;
/// Half-waves around the smallest closed wavy outline.
const MIN_CLOSED_WAVES: usize = 8;

/// Effects on a shape. ChemDraw keeps Shaded and Shadow, and draws a
/// see-through fill as its faded fill (FadePercent).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Effects {
    /// Shades the fill from white at the upper left to its color at the far edge.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub shaded: bool,
    /// A grey copy of the shape this many points down and to the right.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shadow_pt: Option<f32>,
    /// How opaque the fill is, 0–1; a shape without a fill fades its line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opacity: Option<f32>,
    /// A fill that blends from another color into the fill color.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gradient: Option<Gradient>,
}
/// A gradient fill: from `from` at the start to the shape's fill color.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Gradient {
    pub from: crate::palette::Color,
    pub kind: GradientKind,
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GradientKind {
    /// Across the shape, `angle` degrees clockwise from left to right.
    Linear { angle: f32 },
    /// Out from the middle of the shape to its edge.
    Radial,
}
impl Gradient {
    /// The paper into the fill color, as the plan's gradient tiles show, so
    /// a gradient keeps its range on both canvases.
    pub const PAPER: crate::palette::Color = crate::palette::Color::Paper;
    /// The start and end of the gradient in the unit square of the shape's
    /// frame: a linear one spans the square's extent along its angle; a
    /// radial one reaches the circle inscribed in the square.
    pub fn ends(self) -> ([f32; 2], [f32; 2]) {
        match self.kind {
            GradientKind::Linear { angle } => {
                let (sin, cos) = angle.to_radians().sin_cos();
                let half = (sin.abs() + cos.abs()) / 2.;
                (
                    [0.5 - cos * half, 0.5 - sin * half],
                    [0.5 + cos * half, 0.5 + sin * half],
                )
            }
            GradientKind::Radial => ([0.5, 0.5], [1., 0.5]),
        }
    }
}
impl Effects {
    /// ChemDraw's default ShadowSize of 400, in points.
    pub const SHADOW_PT: f32 = 4.;
    /// The opacity a new see-through fill starts at.
    pub const SEE_THROUGH: f32 = 0.5;
    pub fn is_plain(&self) -> bool {
        *self == Self::default()
    }
    pub fn validate(&self) -> Result<(), String> {
        if self
            .shadow_pt
            .is_some_and(|d| !d.is_finite() || !(0.1..=36.).contains(&d))
        {
            return Err("Shadow offset must be 0.1–36 pt".into());
        }
        if self
            .opacity
            .is_some_and(|a| !a.is_finite() || !(0.01..=0.99).contains(&a))
        {
            return Err("Opacity must be 1–99%".into());
        }
        if self.gradient.is_some_and(|g| match g.kind {
            GradientKind::Linear { angle } => !angle.is_finite() || angle.abs() > 360.,
            GradientKind::Radial => false,
        }) {
            return Err("Gradient angle must be within ±360°".into());
        }
        // Shading draws nested bands, which would darken where they overlap.
        if self.shaded && (self.gradient.is_some() || self.opacity.is_some()) {
            return Err("A shaded fill cannot also be a gradient or see-through".into());
        }
        Ok(())
    }
}

/// What a ChemDraw export loses of gradient and see-through fills.
pub const EXPORT_NOTICE: &str = "ChemDraw has no gradient or see-through fills. Gradients become flat colors, and see-through fills become faded colors that cover what lies behind them. Save a .rsk file to keep them.";

/// What a ChemDraw export loses of glows and discs.
pub const GLOW_NOTICE: &str = "ChemDraw has no glows or discs, so highlights and ring fills are written flat. Save a .rsk file to keep them.";

/// The notices for a ChemDraw export of `doc`, for the effects it has.
pub fn export_notice(doc: &crate::document::Document) -> Vec<String> {
    use crate::ring_fills::RingEffect;
    let fills = doc
        .graphics
        .iter()
        .any(|g| g.style.effects.gradient.is_some() || g.style.effects.opacity.is_some())
        || doc
            .ring_fills
            .iter()
            .any(|f| matches!(f.effect, RingEffect::Gradient | RingEffect::SeeThrough));
    let glows = doc
        .ring_fills
        .iter()
        .any(|f| matches!(f.effect, RingEffect::Glow | RingEffect::Disc))
        || !doc.highlight_style.is_flat() && crate::highlights::any(doc);
    [(fills, EXPORT_NOTICE), (glows, GLOW_NOTICE)]
        .into_iter()
        .filter(|(lost, _)| *lost)
        .map(|(_, notice)| notice.into())
        .collect()
}

/// Whether `doc` has paint the canvas draws from image tiles, as it does
/// 3D models: gradients, gradient arrow bodies and glows.
pub fn tiled(doc: &crate::document::Document) -> bool {
    use crate::ring_fills::RingEffect;
    doc.graphics
        .iter()
        .any(|g| g.style.effects.gradient.is_some())
        || doc
            .arrows
            .iter()
            .any(|a| a.drawn_body() == crate::arrows::Body::Gradient)
        || doc
            .ring_fills
            .iter()
            .any(|f| matches!(f.effect, RingEffect::Gradient | RingEffect::Glow))
        || doc.highlight_style == crate::highlights::HighlightStyle::Glow
            && crate::highlights::any(doc)
}

impl Graphic {
    /// Whether the shape encloses an area that effects can paint.
    pub fn closed_shape(&self) -> bool {
        !matches!(
            self.kind,
            GraphicKind::Picture
                | GraphicKind::TlcPlate
                | GraphicKind::Symbol(_)
                | GraphicKind::Orbital(_)
                | GraphicKind::Icon
        ) && (self.kind.closed()
            || self
                .commands()
                .iter()
                .any(|c| matches!(c, PathCommand::Close)))
    }
    /// The color a reader sees inside a filled shape lying on `paper`, with
    /// `shown` giving each color as the canvas shows it: a see-through fill
    /// mixed with the paper, a gradient at its middle. None without a fill.
    pub fn backing(
        &self,
        shown: impl Fn(crate::palette::Color) -> [u8; 3],
        paper: [u8; 3],
    ) -> Option<[u8; 3]> {
        let fill = self.style.fill.filter(|_| {
            self.closed_shape() && self.cycle.is_none() && self.kind != GraphicKind::Piece
        })?;
        let mix = |mut a: [u8; 3], b: [u8; 3], t: f32| {
            for (a, b) in a.iter_mut().zip(b) {
                *a = (f32::from(*a) * (1. - t) + f32::from(b) * t).round() as u8;
            }
            a
        };
        let effects = self.style.effects;
        let rgb = match effects.gradient {
            Some(gradient) => mix(shown(gradient.from), shown(fill), 0.5),
            None => shown(fill),
        };
        Some(mix(paper, rgb, effects.opacity.unwrap_or(1.)))
    }
    /// The outline as drawn, waves included.
    fn outline(&self) -> Vec<PathCommand> {
        let commands = self.commands();
        if self.style.pattern == super::LinePattern::Wavy {
            wavy(&commands, wave_radius(self.style.width_pt), [0.; 2])
        } else {
            commands
        }
    }
    /// The frame of a rectangle or ellipse, axes turned to point right and
    /// down so a reflected shape keeps its light at the upper left.
    fn upright_frame(&self) -> Option<(Point, Point, Point)> {
        if !matches!(
            self.kind,
            GraphicKind::Rectangle | GraphicKind::RoundedRectangle | GraphicKind::Ellipse
        ) {
            return None;
        }
        let (mut origin, mut x, mut y) = (self.origin, self.axis_x, self.axis_y);
        if x.x < 0. || x.x == 0. && x.y < 0. {
            (origin, x) = (origin.offset(x.x, x.y), Point::new(-x.x, -x.y));
        }
        if y.y < 0. || y.y == 0. && y.x < 0. {
            (origin, y) = (origin.offset(y.x, y.y), Point::new(-y.x, -y.y));
        }
        Some((origin, x, y))
    }
    /// The shape's outline moved by its shadow offset: down and right in its
    /// own frame, so a tilted ellipse casts its shadow as ChemDraw's does.
    pub fn shadow_commands(&self) -> Option<Vec<PathCommand>> {
        let d = crate::style::DEFAULT.world(self.style.effects.shadow_pt?);
        if !self.closed_shape() {
            return None;
        }
        let unit = |p: Point| {
            let length = p.distance(Point::default());
            if length > 1e-6 {
                Point::new(p.x / length, p.y / length)
            } else {
                Point::default()
            }
        };
        let (dx, dy) = self.upright_frame().map_or((d, d), |(_, x, y)| {
            let (x, y) = (unit(x), unit(y));
            (d * (x.x + y.x), d * (x.y + y.y))
        });
        Some(
            self.outline()
                .iter()
                .map(|c| c.map(|p| p.offset(dx, dy)))
                .collect(),
        )
    }
    /// The outline a gradient fills and the frame it is laid out in: a
    /// rectangle's or ellipse's own axes, so it turns with the shape, else
    /// the outline's bounding box.
    pub fn gradient_area(&self) -> Option<(Vec<PathCommand>, [Point; 3])> {
        if self.style.effects.gradient.is_none()
            || self.style.fill.is_none()
            || !self.closed_shape()
        {
            return None;
        }
        let commands = self.outline();
        // The stored axes, so the gradient turns and reflects with the shape.
        let (origin, x, y) = if self.upright_frame().is_some() {
            (self.origin, self.axis_x, self.axis_y)
        } else {
            bounding_frame(&commands)?
        };
        Some((commands, [origin, x, y]))
    }
    /// ChemDraw-style shading as nested flat bands, painted in order, each
    /// with its color's distance from the highlight (0) to the far edge (1).
    /// Rectangles shade in boxes anchored at their top-left corner. Other
    /// shapes shade from a highlight out to their own edge, so every band is
    /// the shape shrunk toward the highlight. A shape that cannot be seen
    /// whole from its highlight shades out to the circle inscribed in its
    /// frame instead, its corners outside that circle in the edge color.
    /// `shaded` shapes in the drawing share its band budget.
    pub fn shading_bands(&self, shaded: usize) -> Vec<(Vec<Point>, f32)> {
        if !self.style.effects.shaded || self.style.fill.is_none() || !self.closed_shape() {
            return vec![];
        }
        let commands = self.outline();
        let Some(frame) = self.upright_frame().or_else(|| bounding_frame(&commands)) else {
            return vec![];
        };
        let highlight = if self.kind == GraphicKind::Ellipse {
            OVAL_HIGHLIGHT
        } else {
            CURVE_HIGHLIGHT
        };
        let boxes = matches!(
            self.kind,
            GraphicKind::Rectangle | GraphicKind::RoundedRectangle
        );
        bands(&commands, frame, highlight, boxes, shaded)
    }
}

/// The shading of one lobe of a shaded orbital phase, as
/// [`Graphic::shading_bands`] shades a closed curve, or an ellipse when `oval`.
pub fn lobe_shading(commands: &[PathCommand], oval: bool, shaded: usize) -> Vec<(Vec<Point>, f32)> {
    let Some(frame) = bounding_frame(commands) else {
        return vec![];
    };
    let highlight = if oval {
        OVAL_HIGHLIGHT
    } else {
        CURVE_HIGHLIGHT
    };
    bands(commands, frame, highlight, false, shaded)
}

/// ChemDraw-style shading of `commands` in `frame` from `highlight`, or in
/// boxes for a rectangle, as [`Graphic::shading_bands`] describes.
fn bands(
    commands: &[PathCommand],
    (origin, x, y): (Point, Point, Point),
    [hx, hy]: [f32; 2],
    boxes: bool,
    shaded: usize,
) -> Vec<(Vec<Point>, f32)> {
    let world = |u: f32, v: f32| origin.offset(x.x * u + y.x * v, x.y * u + y.y * v);
    let determinant = x.x * y.y - x.y * y.x;
    if determinant.abs() < 1e-6 {
        return vec![];
    }
    let shapes = flattened(commands);
    // About three bands per point of size, so steps stay invisible when
    // zoomed in, within a bounded number of vertices for huge paths.
    let size = crate::style::DEFAULT.points_per_world()
        * x.distance(Point::default())
            .max(y.distance(Point::default()));
    let points: usize = shapes.iter().map(Vec::len).sum();
    let count = ((size * 3.).ceil() as usize)
        .clamp(MIN_BANDS, MAX_BANDS)
        .min(MAX_BAND_POINTS.min(DRAWING_BAND_POINTS / shaded.max(1)) / points.max(1));
    // A path too detailed to shade within the budget keeps a flat fill.
    if count < 2 {
        return vec![];
    }
    let highlight = world(hx, hy);
    // The highlight sees every edge from inside, so shrunk copies nest.
    let star = !boxes
        && shapes.iter().all(|shape| {
            let edges = || shape.iter().zip(shape.iter().skip(1));
            let area: f32 = edges().map(|(a, b)| a.x * b.y - b.x * a.y).sum();
            // A self-crossing outline such as a bow tie has no area.
            area.abs() > 1e-6
                && edges().all(|(a, b)| {
                    ((b.x - a.x) * (highlight.y - a.y) - (b.y - a.y) * (highlight.x - a.x)) * area
                        >= 0.
                })
        });
    let mut bands = Vec::with_capacity(count * shapes.len());
    for k in (1..=count).rev() {
        let t = k as f32 / count as f32;
        let region: Vec<Point> = if k == count || star {
            // The outermost band is the whole shape.
            vec![]
        } else if boxes {
            let b = BOX_HIGHLIGHT + t * (1. - BOX_HIGHLIGHT);
            vec![world(-1., -1.), world(b, -1.), world(b, b), world(-1., b)]
        } else {
            let (cx, cy) = (hx + t * (0.5 - hx), hy + t * (0.5 - hy));
            // Circumscribe the circle so the band reaches its radius.
            let r = t * 0.5 / (std::f32::consts::PI / 64.).cos();
            (0..64)
                .map(|i| {
                    let a = i as f32 * std::f32::consts::TAU / 64.;
                    world(cx + r * a.cos(), cy + r * a.sin())
                })
                .collect()
        };
        let color = (k as f32 - 0.5) / count as f32;
        for shape in &shapes {
            let band = if star {
                shape.iter().map(|p| highlight.lerp(*p, t)).collect()
            } else if region.is_empty() {
                shape.clone()
            } else {
                clip(shape, &region)
            };
            if band.len() >= 3 {
                bands.push((band, color));
            }
        }
    }
    bands
}

/// The bounding box of `commands` as a frame: its corner and two sides.
pub(crate) fn bounding_frame(commands: &[PathCommand]) -> Option<(Point, Point, Point)> {
    let mut points = commands.iter().flat_map(PathCommand::iter_points);
    let first = points.next()?;
    let (lo, hi) = points.fold((first, first), |(lo, hi), p| {
        (
            Point::new(lo.x.min(p.x), lo.y.min(p.y)),
            Point::new(hi.x.max(p.x), hi.y.max(p.y)),
        )
    });
    Some((lo, Point::new(hi.x - lo.x, 0.), Point::new(0., hi.y - lo.y)))
}

/// The shading color `t` of the way from white to `rgb`, darkening faster
/// toward the edge as ChemDraw's does.
pub fn shade(rgb: [u8; 3], t: f32) -> [u8; 3] {
    let w = t.powf(FALLOFF);
    rgb.map(|c| (255. + (f32::from(c) - 255.) * w).round() as u8)
}

/// Sutherland–Hodgman: `subject` (any polygon) inside the convex `window`.
fn clip(subject: &[Point], window: &[Point]) -> Vec<Point> {
    let area: f32 = window
        .iter()
        .zip(window.iter().cycle().skip(1))
        .map(|(a, b)| a.x * b.y - b.x * a.y)
        .sum();
    let side = |a: Point, b: Point, p: Point| {
        ((b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x)) * area.signum()
    };
    let mut out = subject.to_vec();
    for (&a, &b) in window.iter().zip(window.iter().cycle().skip(1)) {
        let input = std::mem::take(&mut out);
        let Some(&last) = input.last() else {
            break;
        };
        let mut previous = last;
        for &p in &input {
            let (sp, sq) = (side(a, b, previous), side(a, b, p));
            if sq >= 0. {
                if sp < 0. {
                    out.push(previous.lerp(p, sp / (sp - sq)));
                }
                out.push(p);
            } else if sp >= 0. {
                out.push(previous.lerp(p, sp / (sp - sq)));
            }
            previous = p;
        }
    }
    out
}

/// Half-circle waves along every subpath, alternating sides, each about
/// `radius` across; closed subpaths keep an even count so the waves meet.
/// The `straight` length at each end runs straight, under and into an arrowhead.
pub fn wavy(commands: &[PathCommand], radius: f32, straight: [f32; 2]) -> Vec<PathCommand> {
    const K: f32 = 0.552_284_8;
    let paths = flattened(commands);
    // One budget for every subpath: a very long outline gets longer waves.
    let length: f32 = paths
        .iter()
        .flat_map(|path| path.iter().zip(path.iter().skip(1)))
        .map(|(a, b)| a.distance(*b))
        .sum();
    let radius = radius.max(length / (2. * MAX_WAVES));
    let mut out = vec![];
    for path in paths {
        let closed = path.len() > 2 && path.first() == path.last();
        let mut lengths = vec![0.];
        for pair in path.windows(2) {
            if let [a, b] = pair {
                lengths.push(lengths.last().copied().unwrap_or(0.) + a.distance(*b));
            }
        }
        let total = lengths.last().copied().unwrap_or(0.);
        let (Some(&first), Some(&last), true) = (path.first(), path.last(), total > 1e-3) else {
            continue;
        };
        let [lead0, lead1] = straight;
        if lead0 + lead1 > 0. && lead0 + lead1 + 4. * radius >= total {
            // Too short to wave beside its heads: stay straight under them.
            out.push(PathCommand::Move(first));
            out.extend(path.iter().skip(1).map(|p| PathCommand::Line(*p)));
            continue;
        }
        let span = total - lead0 - lead1;
        let mut n = (span / (2. * radius)).round().clamp(1., MAX_WAVES) as usize;
        if closed {
            // Too few waves would retrace each other and close nothing.
            n = (n + n % 2).max(MIN_CLOSED_WAVES);
        }
        let at = |s: f32| {
            let i = lengths.partition_point(|&l| l < s).clamp(1, path.len() - 1);
            match (lengths.get(i - 1..=i), path.get(i - 1..=i)) {
                (Some(&[l0, l1]), Some(&[p0, p1])) if l1 > l0 => p0.lerp(p1, (s - l0) / (l1 - l0)),
                (_, Some(&[p0, _])) => p0,
                _ => last,
            }
        };
        out.push(PathCommand::Move(first));
        let mut a = first;
        if lead0 > 0. {
            a = at(lead0);
            out.push(PathCommand::Line(a));
        }
        let end = if lead1 > 0. { at(total - lead1) } else { last };
        for i in 0..n {
            let b = if i + 1 == n {
                end
            } else {
                at(lead0 + span * (i + 1) as f32 / n as f32)
            };
            let r = a.distance(b) / 2.;
            if r > 1e-6 {
                let u = Point::new((b.x - a.x) / (2. * r), (b.y - a.y) / (2. * r));
                let side = if i % 2 == 0 { r } else { -r };
                let normal = Point::new(-u.y * side, u.x * side);
                let top = a.lerp(b, 0.5).offset(normal.x, normal.y);
                out.push(PathCommand::Cubic(
                    a.offset(normal.x * K, normal.y * K),
                    top.offset(-u.x * r * K, -u.y * r * K),
                    top,
                ));
                out.push(PathCommand::Cubic(
                    top.offset(u.x * r * K, u.y * r * K),
                    b.offset(normal.x * K, normal.y * K),
                    b,
                ));
            }
            a = b;
        }
        if lead1 > 0. {
            out.push(PathCommand::Line(last));
        }
        if closed {
            out.push(PathCommand::Close);
        }
    }
    out
}

/// The wave radius for a stroke `width_pt` wide: ChemDraw's 1.8 pt for
/// ordinary lines, growing with bolder ones.
pub fn wave_radius(width_pt: f32) -> f32 {
    crate::style::DEFAULT.world(1.8 * width_pt.max(1.))
}

#[cfg(test)]
mod tests;
