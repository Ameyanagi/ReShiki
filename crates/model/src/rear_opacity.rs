//! Rear-half transparency, using the same component-local depth as RGB paint.
//! Positive Z is toward the viewer. Normalized rear distance > 0.5 is behind
//! the component's midplane; points on that plane remain front/opaque.
use crate::{
    document::{Bond, Document, Point},
    graphics::PathCommand,
    scene::Primitive,
};
use std::collections::BTreeMap;
mod ink;

pub const EXPORT_NOTICE: &str = "This chemical format does not retain rear opacity. Chemical structure and XYZ are unchanged; use native ReShiki for editable appearance or SVG/PDF/PNG for the figure.";

pub fn present(doc: &Document) -> bool {
    doc.depth_appearance
        .iter()
        .any(|scope| scope.rear_opacity < 1.)
}

#[derive(Clone, Copy)]
struct Node {
    weight: f32,
    opacity: f32,
}

/// Built from the original drawing, before detached RGB/style normalization.
pub struct Paint {
    nodes: BTreeMap<u64, Node>,
}
impl Paint {
    pub fn new(doc: &Document) -> Self {
        let mut nodes = BTreeMap::new();
        for scope in doc.depth_appearance.iter().filter(|s| s.rear_opacity < 1.) {
            let weights = if scope.automatic {
                crate::depth_appearance::automatic_weights(doc, &scope.atoms)
            } else {
                scope.weights.clone()
            };
            for (id, weight) in weights {
                nodes.insert(
                    id,
                    Node {
                        weight: weight.clamp(0., 1.),
                        opacity: scope.rear_opacity.clamp(0., 1.),
                    },
                );
            }
        }
        for atom in doc.atoms.iter().filter(|a| !a.centroid.is_empty()) {
            if atom.centroid.iter().any(|id| nodes.contains_key(id)) {
                let weight = atom
                    .centroid
                    .iter()
                    .map(|id| nodes.get(id).map_or(0., |n| n.weight))
                    .sum::<f32>()
                    / atom.centroid.len() as f32;
                let opacity = atom
                    .centroid
                    .iter()
                    .filter_map(|id| nodes.get(id).map(|n| n.opacity))
                    .fold(1., f32::min);
                nodes.insert(atom.id, Node { weight, opacity });
            }
        }
        Self { nodes }
    }
    fn node(&self, id: u64) -> Node {
        self.nodes.get(&id).copied().unwrap_or(Node {
            weight: 0.,
            opacity: 1.,
        })
    }
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
    pub fn atom(&self, id: u64) -> f32 {
        let node = self.node(id);
        if node.weight > 0.5 { node.opacity } else { 1. }
    }
    pub fn bond(&self, bond: &Bond, t: f32) -> f32 {
        let a = self.node(bond.a);
        let b = self.node(bond.b);
        if lerp(a.weight, b.weight, t.clamp(0., 1.)) > 0.5 {
            a.opacity.min(b.opacity)
        } else {
            1.
        }
    }
    pub(crate) fn bond_parts(
        &self,
        doc: &Document,
        bond: &Bond,
        parts: Vec<Primitive>,
    ) -> Vec<Primitive> {
        if self.is_empty() {
            return parts;
        }
        let (Some(a), Some(b)) = (doc.atom(bond.a), doc.atom(bond.b)) else {
            return parts;
        };
        let first = self.node(a.id);
        let last = self.node(b.id);
        let opacity = first.opacity.min(last.opacity);
        let field = Field::Bond {
            a: a.position,
            b: b.position,
            wa: first.weight,
            wb: last.weight,
        };
        // A projected inward rail has its face's depth, including its XYZ
        // offset. An edge/centroid plane matches the renderer's local tangent
        // even for slightly warped optimized fullerene faces.
        let field = if matches!(bond.order, 2 | 7)
            && bond.double_position == crate::bonds::DoublePosition::Auto
        {
            crate::scene::projected_bonds::face_atoms(doc, bond)
                .and_then(|ids| {
                    let vertices: Vec<_> = ids
                        .iter()
                        .filter_map(|id| doc.atom(*id).map(|a| (a.position, self.node(*id).weight)))
                        .collect();
                    let n = vertices.len().max(1) as f32;
                    let center = vertices
                        .iter()
                        .fold(Point::default(), |p, (a, _)| p.offset(a.x / n, a.y / n));
                    let weight = vertices.iter().map(|(_, w)| w / n).sum();
                    Field::plane(&[
                        (a.position, first.weight),
                        (b.position, last.weight),
                        (center, weight),
                    ])
                })
                .unwrap_or(field)
        } else {
            field
        };
        split(parts, field, opacity)
    }
    pub(crate) fn atom_parts(&self, id: u64, parts: Vec<Primitive>) -> Vec<Primitive> {
        with_opacity(parts, self.atom(id))
    }
    pub(crate) fn owned_at(&self, doc: &Document, ids: &[u64], point: Point) -> f32 {
        if self.is_empty() || ids.is_empty() {
            return 1.;
        }
        let vertices: Vec<_> = ids
            .iter()
            .filter_map(|id| doc.atom(*id).map(|a| (a.position, self.node(*id).weight)))
            .collect();
        let field = Field::plane(&vertices).unwrap_or_else(|| {
            Field::Constant(
                vertices.iter().map(|(_, w)| w).sum::<f32>() / vertices.len().max(1) as f32,
            )
        });
        if field.at(point) > 0.5 {
            ids.iter()
                .map(|id| self.node(*id).opacity)
                .fold(1., f32::min)
        } else {
            1.
        }
    }
    /// Planar rings use their affine depth plane. Degenerate/nonplanar paint
    /// retains a bounded mean-owner classification instead of an unstable fit.
    pub(crate) fn owned_parts(
        &self,
        doc: &Document,
        ids: &[u64],
        parts: Vec<Primitive>,
    ) -> Vec<Primitive> {
        if let [id] = ids {
            return self.atom_parts(*id, parts);
        }
        if self.is_empty() || ids.is_empty() {
            return parts;
        }
        let opacity = ids
            .iter()
            .map(|id| self.node(*id).opacity)
            .fold(1., f32::min);
        let vertices: Vec<_> = ids
            .iter()
            .filter_map(|id| doc.atom(*id).map(|a| (a.position, self.node(*id).weight)))
            .collect();
        let field = Field::plane(&vertices).unwrap_or_else(|| {
            Field::Constant(
                vertices.iter().map(|(_, w)| w).sum::<f32>() / vertices.len().max(1) as f32,
            )
        });
        split(parts, field, opacity)
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
fn mix(a: Point, b: Point, t: f32) -> Point {
    Point::new(lerp(a.x, b.x, t), lerp(a.y, b.y, t))
}

#[derive(Clone, Copy)]
enum Field {
    Constant(f32),
    Bond {
        a: Point,
        b: Point,
        wa: f32,
        wb: f32,
    },
    Plane {
        origin: Point,
        weight: f64,
        dx: f64,
        dy: f64,
    },
}
impl Field {
    fn at(self, p: Point) -> f32 {
        match self {
            Self::Constant(w) => w,
            Self::Bond { a, b, wa, wb } => {
                let (x, y) = (
                    f64::from(b.x) - f64::from(a.x),
                    f64::from(b.y) - f64::from(a.y),
                );
                let length = x * x + y * y;
                let t = if length > 1e-12 {
                    ((f64::from(p.x) - f64::from(a.x)) * x + (f64::from(p.y) - f64::from(a.y)) * y)
                        / length
                } else {
                    0.5
                };
                lerp(wa, wb, t.clamp(0., 1.) as f32)
            }
            Self::Plane {
                origin,
                weight,
                dx,
                dy,
            } => (weight
                + (f64::from(p.x) - f64::from(origin.x)) * dx
                + (f64::from(p.y) - f64::from(origin.y)) * dy)
                .clamp(0., 1.) as f32,
        }
    }
    fn plane(vertices: &[(Point, f32)]) -> Option<Self> {
        let &(origin, weight) = vertices.first()?;
        let mut best = None;
        let mut area = 1e-6_f64;
        for (i, (a, wa)) in vertices.iter().enumerate().skip(1) {
            for (b, wb) in vertices.iter().skip(i + 1) {
                let (ax, ay) = (
                    f64::from(a.x) - f64::from(origin.x),
                    f64::from(a.y) - f64::from(origin.y),
                );
                let (bx, by) = (
                    f64::from(b.x) - f64::from(origin.x),
                    f64::from(b.y) - f64::from(origin.y),
                );
                let det = ax * by - ay * bx;
                if det.abs() > area {
                    area = det.abs();
                    let aw = f64::from(*wa - weight);
                    let bw = f64::from(*wb - weight);
                    best = Some(Self::Plane {
                        origin,
                        weight: f64::from(weight),
                        dx: (aw * by - ay * bw) / det,
                        dy: (ax * bw - aw * bx) / det,
                    });
                }
            }
        }
        let field = best?;
        vertices
            .iter()
            .all(|(p, w)| (field.at(*p) - w).abs() < 0.001)
            .then_some(field)
    }
}

pub(crate) fn with_opacity(parts: Vec<Primitive>, alpha: f32) -> Vec<Primitive> {
    if alpha <= 0. {
        vec![]
    } else if alpha >= 1. {
        parts
    } else {
        parts
            .into_iter()
            .map(|part| Primitive::Opacity {
                alpha,
                primitive: Box::new(ink::single_paint(part)),
            })
            .collect()
    }
}

fn points(part: &Primitive) -> Vec<Point> {
    match part {
        Primitive::Line(a, b, _) => vec![*a, *b],
        Primitive::Polygon(p) => p.clone(),
        Primitive::Path { commands, .. } => {
            commands.iter().flat_map(PathCommand::iter_points).collect()
        }
        _ => vec![],
    }
}
fn split(parts: Vec<Primitive>, field: Field, opacity: f32) -> Vec<Primitive> {
    if opacity >= 1. {
        return parts;
    }
    let mut result = Vec::new();
    for part in parts {
        let vertices = points(&part);
        if vertices.is_empty() || vertices.iter().all(|p| field.at(*p) <= 0.5) {
            result.push(part);
        } else if vertices.iter().all(|p| field.at(*p) > 0.5) {
            result.extend(with_opacity(vec![part], opacity));
        } else {
            // A faded full silhouette under the opaque front gives exact
            // coverage at their common antialiased edge (no pale seam).
            let part = match part {
                Primitive::Line(a, b, width) => Primitive::Polygon(capsule(a, b, width)),
                part => ink::single_paint(part),
            };
            result.extend(with_opacity(vec![part.clone()], opacity));
            result.extend(half(&part, field, false));
        }
    }
    result
}
fn inside(field: Field, p: Point, rear: bool) -> bool {
    if rear {
        field.at(p) >= 0.5
    } else {
        field.at(p) <= 0.5
    }
}
fn cut(a: Point, b: Point, field: Field) -> Point {
    // The classification field is bounded, so use bisection at a crossing
    // rather than extrapolating two clamped endpoint samples.
    let mut low = 0.;
    let mut high = 1.;
    let side = field.at(a) > 0.5;
    for _ in 0..24 {
        let t = (low + high) / 2.;
        if (field.at(mix(a, b, t)) > 0.5) == side {
            low = t;
        } else {
            high = t;
        }
    }
    mix(a, b, (low + high) / 2.)
}
fn polygon(points: &[Point], field: Field, rear: bool) -> Vec<Point> {
    let mut clipped = vec![];
    for (&a, &b) in points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
    {
        let ia = inside(field, a, rear);
        let ib = inside(field, b, rear);
        if ia {
            clipped.push(a);
        }
        if ia != ib {
            clipped.push(cut(a, b, field));
        }
    }
    clipped
}
fn capsule(a: Point, b: Point, width: f32) -> Vec<Point> {
    let angle = (b.y - a.y).atan2(b.x - a.x);
    let radius = width / 2.;
    [b, a]
        .into_iter()
        .enumerate()
        .flat_map(|(side, p)| {
            (0..=16).map(move |i| {
                let theta = angle - std::f32::consts::FRAC_PI_2
                    + side as f32 * std::f32::consts::PI
                    + i as f32 * std::f32::consts::PI / 16.;
                p.offset(radius * theta.cos(), radius * theta.sin())
            })
        })
        .collect()
}
fn half(part: &Primitive, field: Field, rear: bool) -> Vec<Primitive> {
    match part {
        Primitive::Polygon(p) => {
            let p = polygon(p, field, rear);
            if p.len() >= 3 {
                vec![Primitive::Polygon(p)]
            } else {
                vec![]
            }
        }
        Primitive::Line(a, b, width) => {
            half(&Primitive::Polygon(capsule(*a, *b, *width)), field, rear)
        }
        Primitive::Path {
            commands,
            style,
            filled,
        } => {
            let paths = flatten(commands);
            if *filled {
                let mut clipped = vec![];
                for path in &paths {
                    let p = polygon(path, field, rear);
                    if let Some(first) = p.first().filter(|_| p.len() >= 3) {
                        clipped.push(PathCommand::Move(*first));
                        clipped.extend(p.iter().skip(1).copied().map(PathCommand::Line));
                        clipped.push(PathCommand::Close);
                    }
                }
                let mut parts = vec![];
                if !clipped.is_empty() {
                    let mut fill = style.clone();
                    fill.width_pt = 0.;
                    parts.push(Primitive::Path {
                        commands: clipped,
                        style: fill,
                        filled: true,
                    });
                }
                if style.width_pt > 0. {
                    parts.extend(half(
                        &Primitive::Path {
                            commands: commands.clone(),
                            style: style.clone(),
                            filled: false,
                        },
                        field,
                        rear,
                    ));
                }
                parts
            } else {
                let mut clipped = vec![];
                for path in paths {
                    let mut pen = None;
                    for pair in path.windows(2) {
                        let &[a, b] = pair else { continue };
                        let (ia, ib) = (inside(field, a, rear), inside(field, b, rear));
                        let segment = match (ia, ib) {
                            (true, true) => Some((a, b)),
                            (true, false) => Some((a, cut(a, b, field))),
                            (false, true) => Some((cut(a, b, field), b)),
                            _ => None,
                        };
                        if let Some((a, b)) = segment {
                            if pen != Some(a) {
                                clipped.push(PathCommand::Move(a));
                            }
                            clipped.push(PathCommand::Line(b));
                            pen = Some(b);
                        } else {
                            pen = None;
                        }
                    }
                }
                if clipped.is_empty() {
                    vec![]
                } else {
                    vec![Primitive::Path {
                        commands: clipped,
                        style: style.clone(),
                        filled: false,
                    }]
                }
            }
        }
        _ => vec![part.clone()],
    }
}
fn flatten(commands: &[PathCommand]) -> Vec<Vec<Point>> {
    fn cubic(out: &mut Vec<Point>, p: Point, a: Point, b: Point, z: Point, depth: u8) {
        let distance = |v: Point| {
            let (x, y) = (z.x - p.x, z.y - p.y);
            let length = (x * x + y * y).sqrt();
            if length < 1e-6 {
                p.distance(v)
            } else {
                ((v.x - p.x) * y - (v.y - p.y) * x).abs() / length
            }
        };
        if depth >= 12 || distance(a).max(distance(b)) <= 0.02 {
            out.push(z);
            return;
        }
        let (pa, ab, bz) = (mix(p, a, 0.5), mix(a, b, 0.5), mix(b, z, 0.5));
        let (left, right) = (mix(pa, ab, 0.5), mix(ab, bz, 0.5));
        let middle = mix(left, right, 0.5);
        cubic(out, p, pa, left, middle, depth + 1);
        cubic(out, middle, right, bz, z, depth + 1);
    }
    let mut paths = vec![];
    let mut path = vec![];
    for command in commands {
        match *command {
            PathCommand::Move(p) => {
                if !path.is_empty() {
                    paths.push(std::mem::take(&mut path));
                }
                path.push(p);
            }
            PathCommand::Line(p) => path.push(p),
            PathCommand::Cubic(a, b, z) => {
                if let Some(&p) = path.last() {
                    cubic(&mut path, p, a, b, z, 0);
                } else {
                    path.push(z);
                }
            }
            PathCommand::Close => {
                if let Some(&p) = path.first() {
                    path.push(p);
                }
            }
        }
    }
    if !path.is_empty() {
        paths.push(path);
    }
    paths
}

#[cfg(test)]
mod tests;
