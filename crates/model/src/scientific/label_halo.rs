//! Find the atom labels an orbital's complete lobes run into. Those labels are
//! drawn above the orbital inside a paper-colored halo (see `scene::Primitive::Halo`);
//! the saved orbital frame and its lobes stay untouched.
use crate::{
    document::Point,
    graphics::{Graphic, PathCommand},
    polygon::closed_edges,
    style::DEFAULT,
};
type Box2 = (Point, Point);
const HALO_FLATTEN_TOLERANCE: f32 = 0.03;
const HALO_FLATTEN_DEPTH: u8 = 14;
/// Halo radius around label glyphs, in points.
pub const LABEL_HALO_PT: f32 = 1.0;

/// The halo radius in world units.
pub fn label_halo_radius() -> f32 {
    DEFAULT.world(LABEL_HALO_PT)
}

/// The atom labels orbitals' complete lobes reach. Each label is drawn in a
/// halo right above the topmost front orbital reaching it, or at its usual
/// place above the bonds when only orbitals behind the molecule reach it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OrbitalHalos {
    /// Each reached atom with the topmost front orbital reaching it.
    pub atoms: Vec<(u64, Option<u64>)>,
    /// Every front orbital reaching a label, bottom to top.
    pub front: Vec<u64>,
}
impl OrbitalHalos {
    /// The atoms drawn right above `orbital` (`None`: above the bonds).
    pub fn above(&self, orbital: Option<u64>) -> Vec<u64> {
        self.atoms
            .iter()
            .filter(|(_, above)| *above == orbital)
            .map(|(atom, _)| *atom)
            .collect()
    }
    pub fn contains(&self, atom: u64) -> bool {
        self.atoms.iter().any(|(id, _)| *id == atom)
    }
}

pub(crate) fn orbital_halos<'a>(
    doc: &crate::document::Document,
    graphics: impl IntoIterator<Item = &'a Graphic>,
    label_ink: impl Fn(&crate::document::Atom) -> Vec<Box2>,
) -> OrbitalHalos {
    let mut orbitals: Vec<_> = graphics
        .into_iter()
        .filter(|graphic| matches!(graphic.kind, crate::graphics::GraphicKind::Orbital(_)))
        .collect();
    // Paint order: stable by layer, as the scene and exchange stack graphics.
    orbitals.sort_by_key(|graphic| graphic.layer);
    let mut halos = OrbitalHalos::default();
    if orbitals.is_empty() {
        return halos;
    }
    let lobes: Vec<_> = orbitals.iter().map(|g| OrbitalInk::new(g)).collect();
    let mut front = std::collections::HashSet::new();
    for atom in &doc.atoms {
        let ink = label_ink(atom);
        let mut reached = false;
        let mut above = None;
        for (graphic, lobes) in orbitals.iter().zip(&lobes) {
            if lobes.reaches(&ink) {
                reached = true;
                if graphic.layer >= 0 {
                    above = Some(graphic.id);
                    front.insert(graphic.id);
                }
            }
        }
        if reached {
            halos.atoms.push((atom.id, above));
        }
    }
    halos.front = orbitals
        .iter()
        .map(|graphic| graphic.id)
        .filter(|id| front.contains(id))
        .collect();
    halos
}

/// An orbital's flattened lobes, padded by the halo and half its stroke.
struct OrbitalInk {
    parts: Vec<(Vec<Vec<Point>>, bool)>,
    bounds: Box2,
    padding: f32,
}
impl OrbitalInk {
    fn new(graphic: &Graphic) -> Self {
        let parts: Vec<_> = graphic
            .parts()
            .into_iter()
            .map(|part| (flatten(&part.commands), part.filled))
            .collect();
        let padding = label_halo_radius() + graphic.style.width() * 0.5;
        let mut points = parts.iter().flat_map(|(paths, _)| paths.iter().flatten());
        let first = points.next().copied().unwrap_or_default();
        let (lo, hi) = points.fold((first, first), |(lo, hi), p| {
            (
                Point::new(lo.x.min(p.x), lo.y.min(p.y)),
                Point::new(hi.x.max(p.x), hi.y.max(p.y)),
            )
        });
        Self {
            parts,
            bounds: (lo.offset(-padding, -padding), hi.offset(padding, padding)),
            padding,
        }
    }
    /// Whether a lobe (fill or outline) comes within the halo of label ink.
    fn reaches(&self, labels: &[Box2]) -> bool {
        let padding = self.padding;
        labels.iter().any(|(lo, hi)| {
            let (outer_lo, outer_hi) = self.bounds;
            if hi.x < outer_lo.x || lo.x > outer_hi.x || hi.y < outer_lo.y || lo.y > outer_hi.y {
                return false;
            }
            let bounds = (lo.offset(-padding, -padding), hi.offset(padding, padding));
            let middle = bounds.0.offset(
                (bounds.1.x - bounds.0.x) * 0.5,
                (bounds.1.y - bounds.0.y) * 0.5,
            );
            self.parts.iter().any(|(paths, filled)| {
                paths.iter().flatten().any(|p| inside(*p, bounds))
                    || paths.iter().any(|points| {
                        points
                            .windows(2)
                            .any(|edge| matches!(edge, [a, b] if crosses(*a, *b, bounds)))
                    })
                    || (*filled && paths.iter().any(|points| contains(points, middle)))
            })
        })
    }
}

fn inside(p: Point, (lo, hi): Box2) -> bool {
    p.x >= lo.x && p.x <= hi.x && p.y >= lo.y && p.y <= hi.y
}

// Closes the contour (last vertex to first), unlike graphics::inside_polygon's windows(2).
fn contains(points: &[Point], p: Point) -> bool {
    let mut inside = false;
    for (a, b) in closed_edges(points) {
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
    }
    inside
}

/// Liang-Barsky: whether segment a-b passes through the box.
fn crosses(a: Point, b: Point, (lo, hi): Box2) -> bool {
    let mut enter: f32 = 0.;
    let mut leave: f32 = 1.;
    for (start, delta, min, max) in [(a.x, b.x - a.x, lo.x, hi.x), (a.y, b.y - a.y, lo.y, hi.y)] {
        if delta.abs() < 1e-8 {
            if start < min || start > max {
                return false;
            }
        } else {
            let t0 = (min - start) / delta;
            let t1 = (max - start) / delta;
            enter = enter.max(t0.min(t1));
            leave = leave.min(t0.max(t1));
        }
        if enter >= leave {
            return false;
        }
    }
    true
}

// Deliberate sibling of rear_opacity::clip::flatten (sqrt, 1e-6 cutoff, 0.02/depth 12, pushes a
// cubic end without a preceding Move); do not merge them.
fn flatten(commands: &[PathCommand]) -> Vec<Vec<Point>> {
    let mut paths: Vec<Vec<Point>> = Vec::new();
    for command in commands {
        match *command {
            PathCommand::Move(p) => paths.push(vec![p]),
            PathCommand::Line(p) => {
                if let Some(points) = paths.last_mut() {
                    points.push(p);
                }
            }
            PathCommand::Close => {
                if let Some(points) = paths.last_mut()
                    && let Some(first) = points.first().copied()
                {
                    points.push(first);
                }
            }
            PathCommand::Cubic(a, b, end) => {
                if let Some(points) = paths.last_mut()
                    && let Some(start) = points.last().copied()
                {
                    flatten_cubic(points, start, a, b, end, 0);
                }
            }
        }
    }
    paths
}
fn flatten_cubic(out: &mut Vec<Point>, start: Point, a: Point, b: Point, end: Point, depth: u8) {
    let deviation = |p: Point| {
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let length = dx.hypot(dy);
        if length < 1e-8 {
            p.distance(start)
        } else {
            ((p.x - start.x) * dy - (p.y - start.y) * dx).abs() / length
        }
    };
    if depth == HALO_FLATTEN_DEPTH || deviation(a).max(deviation(b)) <= HALO_FLATTEN_TOLERANCE {
        out.push(end);
        return;
    }
    let a0 = start.lerp(a, 0.5);
    let a1 = a.lerp(b, 0.5);
    let a2 = b.lerp(end, 0.5);
    let b0 = a0.lerp(a1, 0.5);
    let b1 = a1.lerp(a2, 0.5);
    let middle = b0.lerp(b1, 0.5);
    flatten_cubic(out, start, a0, b0, middle, depth + 1);
    flatten_cubic(out, middle, b1, a2, end, depth + 1);
}
