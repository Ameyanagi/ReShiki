//! ChemDraw's shape effects: shading, drop shadows and wavy strokes.
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
/// Half-waves one wavy path may produce; longer paths get longer waves.
const MAX_WAVES: f32 = 10_000.;

/// Effects on a closed shape, written to ChemDraw as Shaded and Shadow.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Effects {
    /// Shades the fill from white at the upper left to its color at the far edge.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub shaded: bool,
    /// A grey copy of the shape this many points down and to the right.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shadow_pt: Option<f32>,
}
impl Effects {
    /// ChemDraw's default ShadowSize of 400, in points.
    pub const SHADOW_PT: f32 = 4.;
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
        Ok(())
    }
}

impl Graphic {
    /// Whether the shape encloses an area that effects can paint.
    pub fn closed_shape(&self) -> bool {
        !matches!(
            self.kind,
            GraphicKind::Picture | GraphicKind::Symbol(_) | GraphicKind::Orbital(_)
        ) && (self.kind.closed()
            || self
                .commands()
                .iter()
                .any(|c| matches!(c, PathCommand::Close)))
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
    /// ChemDraw-style shading as nested flat bands, painted in order, each
    /// with its color's distance from the highlight (0) to the far edge (1).
    /// Rectangles shade in boxes anchored at their top-left corner. Other
    /// shapes shade from a highlight out to their own edge, so every band is
    /// the shape shrunk toward the highlight. A shape that cannot be seen
    /// whole from its highlight shades out to the circle inscribed in its
    /// frame instead, its corners outside that circle in the edge color.
    pub fn shading_bands(&self) -> Vec<(Vec<Point>, f32)> {
        if !self.style.effects.shaded || self.style.fill.is_none() || !self.closed_shape() {
            return vec![];
        }
        let commands = self.outline();
        let boxes = matches!(
            self.kind,
            GraphicKind::Rectangle | GraphicKind::RoundedRectangle
        );
        let (origin, x, y) = if let Some(frame) = self.upright_frame() {
            frame
        } else {
            let mut points = commands.iter().flat_map(PathCommand::iter_points);
            let Some(first) = points.next() else {
                return vec![];
            };
            let (lo, hi) = points.fold((first, first), |(lo, hi), p| {
                (
                    Point::new(lo.x.min(p.x), lo.y.min(p.y)),
                    Point::new(hi.x.max(p.x), hi.y.max(p.y)),
                )
            });
            (lo, Point::new(hi.x - lo.x, 0.), Point::new(0., hi.y - lo.y))
        };
        let world = |u: f32, v: f32| origin.offset(x.x * u + y.x * v, x.y * u + y.y * v);
        let determinant = x.x * y.y - x.y * y.x;
        if determinant.abs() < 1e-6 {
            return vec![];
        }
        let shapes = flattened(&commands);
        // About three bands per point of size, so steps stay invisible when
        // zoomed in, within a bounded number of vertices for huge paths.
        let size = crate::style::DEFAULT.points_per_world()
            * x.distance(Point::default())
                .max(y.distance(Point::default()));
        let points: usize = shapes.iter().map(Vec::len).sum();
        let count = ((size * 3.).ceil() as usize)
            .clamp(MIN_BANDS, MAX_BANDS)
            .min(MAX_BAND_POINTS / points.max(1));
        // A path too detailed to shade within the budget keeps a flat fill.
        if count < 2 {
            return vec![];
        }
        let [hx, hy] = if self.kind == GraphicKind::Ellipse {
            OVAL_HIGHLIGHT
        } else {
            CURVE_HIGHLIGHT
        };
        let highlight = world(hx, hy);
        // The highlight sees every edge from inside, so shrunk copies nest.
        let star = !boxes
            && shapes.iter().all(|shape| {
                let edges = || shape.iter().zip(shape.iter().skip(1));
                let area: f32 = edges().map(|(a, b)| a.x * b.y - b.x * a.y).sum();
                // A self-crossing outline such as a bow tie has no area.
                area.abs() > 1e-6
                    && edges().all(|(a, b)| {
                        ((b.x - a.x) * (highlight.y - a.y) - (b.y - a.y) * (highlight.x - a.x))
                            * area
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
        if closed && n % 2 == 1 {
            n += 1;
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
