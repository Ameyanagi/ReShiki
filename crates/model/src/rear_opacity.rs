//! Transparency for view-occluded cage/atom/bond ink, independent of RGB.
//! Positive Z is toward the viewer. Exposed ink, including the complete cage
//! rim, remains opaque regardless of its distance from the depth midplane.
use crate::{
    document::{Bond, Document, Point},
    graphics::PathCommand,
    scene::Primitive,
};
use std::collections::BTreeMap;
mod ink;
mod visibility;
pub(crate) use visibility::bond_field;

pub const EXPORT_NOTICE: &str = "This chemical format does not retain rear opacity. Chemical structure and XYZ are unchanged; use native ReShiki for editable appearance or SVG/PDF/PNG for the figure.";

pub fn present(doc: &Document) -> bool {
    doc.depth_appearance
        .iter()
        .any(crate::depth_appearance::Scope::has_rear_opacity)
}
#[derive(Clone, Copy)]
struct Node {
    position: Point,
    depth: f32,
    opacity: f32,
}
/// Built from original current XYZ, before detached RGB/style normalization.
pub struct Paint {
    nodes: BTreeMap<u64, Node>,
    atoms: BTreeMap<u64, f32>,
    visibility: Option<visibility::Visibility>,
    corners: BTreeMap<(u64, u64, u64), Vec<Point>>,
}
impl Paint {
    pub fn new(doc: &Document) -> Self {
        let mut nodes: BTreeMap<_, _> = doc
            .atoms
            .iter()
            .map(|a| {
                (
                    a.id,
                    Node {
                        position: a.position,
                        depth: a.depth,
                        opacity: 1.,
                    },
                )
            })
            .collect();
        for scope in doc.depth_appearance.iter().filter(|s| s.has_rear_opacity()) {
            for id in &scope.atoms {
                if let Some(node) = nodes.get_mut(id) {
                    node.opacity = scope.rear_opacity_for(*id).clamp(0., 1.);
                }
            }
        }
        for atom in doc.atoms.iter().filter(|a| !a.centroid.is_empty()) {
            let opacity = atom
                .centroid
                .iter()
                .filter_map(|id| nodes.get(id).map(|n| n.opacity))
                .fold(1., f32::min);
            if let Some(node) = nodes.get_mut(&atom.id) {
                node.opacity = opacity;
            }
        }
        // Preserve the exact legacy 100% path, including draw order and curves.
        let visibility = nodes
            .values()
            .any(|n| n.opacity < 1.)
            .then(|| visibility::Visibility::new(doc))
            .filter(|v| !v.is_empty());
        let atoms: BTreeMap<_, _> = nodes
            .iter()
            .map(|(id, node)| {
                (
                    *id,
                    if node.opacity < 1.
                        && visibility
                            .as_ref()
                            .is_some_and(|v| v.occluded(node.position, node.depth, &[*id], 0))
                    {
                        node.opacity
                    } else {
                        1.
                    },
                )
            })
            .collect();
        let mut corners = BTreeMap::new();
        if let Some(v) = &visibility {
            let joins = crate::bond_joins::Joins::new(doc);
            for bond in &doc.bonds {
                if !joins.needed(bond) {
                    continue;
                }
                for id in [bond.a, bond.b] {
                    if atoms.get(&id) == Some(&1.)
                        && v.rim_anchor(id).is_some()
                        && let Some(corner) = joins.corner(bond, id)
                    {
                        corners.insert((bond.a.min(bond.b), bond.a.max(bond.b), id), corner);
                    }
                }
            }
        }
        Self {
            nodes,
            atoms,
            visibility,
            corners,
        }
    }
    fn node(&self, id: u64) -> Node {
        self.nodes.get(&id).copied().unwrap_or(Node {
            position: Point::default(),
            depth: 0.,
            opacity: 1.,
        })
    }
    pub fn is_empty(&self) -> bool {
        self.visibility.is_none()
    }
    pub fn atom(&self, id: u64) -> f32 {
        self.atoms.get(&id).copied().unwrap_or(1.)
    }
    pub(crate) fn manages_bond(&self, bond: &Bond) -> bool {
        self.visibility.is_some() && self.node(bond.a).opacity.min(self.node(bond.b).opacity) < 1.
    }
    pub fn bond(&self, bond: &Bond, t: f32) -> f32 {
        let (a, b) = (self.node(bond.a), self.node(bond.b));
        let alpha = a.opacity.min(b.opacity);
        if alpha >= 1. {
            return 1.;
        }
        let t = t.clamp(0., 1.);
        if self.visibility.as_ref().is_some_and(|v| {
            v.occluded(
                mix(a.position, b.position, t),
                lerp(a.depth, b.depth, t),
                &[bond.a, bond.b],
                bond.z_order,
            )
        }) {
            alpha
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
        let alpha = self.node(bond.a).opacity.min(self.node(bond.b).opacity);
        let (Some(v), Some(a), Some(b)) = (&self.visibility, doc.atom(bond.a), doc.atom(bond.b))
        else {
            return parts;
        };
        if alpha >= 1. {
            return parts;
        }
        let field = visibility::bond_field(doc, bond);
        let mut out = vec![];
        for part in parts {
            // Every projected secondary rail uses its own actual centerline.
            // Wedges/junctions inherit the backbone intervals across full width.
            let (first, last) = match &part {
                Primitive::Line(first, last, _)
                    if parallel(*first, *last, a.position, b.position) =>
                {
                    (*first, *last)
                }
                _ => (a.position, b.position),
            };
            let hidden = v.hidden(first, last, field, &[bond.a, bond.b], bond.z_order);
            if !hidden.is_empty() {
                out.extend(self.exposed_caps(doc, bond, &part));
            }
            out.extend(split_intervals(part, first, last, &hidden, alpha));
        }
        out
    }
    pub(crate) fn atom_parts(&self, id: u64, parts: Vec<Primitive>) -> Vec<Primitive> {
        with_opacity(parts, self.atom(id))
    }
    pub(crate) fn rail_parts(
        &self,
        doc: &Document,
        bond: &Bond,
        rail: &crate::scene::projected_bonds::RailDepth,
        parts: Vec<Primitive>,
    ) -> Vec<Primitive> {
        let alpha = self.node(bond.a).opacity.min(self.node(bond.b).opacity);
        let Some(v) = &self.visibility else {
            return parts;
        };
        if alpha >= 1. {
            return parts;
        }
        let (a, b) = rail.points;
        let support = (!rail.support.is_empty()).then_some(rail.support.as_slice());
        let hidden =
            v.hidden_with_support(a, b, rail.field, &[bond.a, bond.b], bond.z_order, support);
        let mut out = vec![];
        for part in parts {
            if support.is_none() && !hidden.is_empty() {
                out.extend(self.exposed_caps(doc, bond, &part));
            }
            out.extend(split_intervals(part, a, b, &hidden, alpha));
        }
        out
    }
    fn exposed_caps(&self, doc: &Document, bond: &Bond, part: &Primitive) -> Vec<Primitive> {
        let Some(v) = &self.visibility else {
            return vec![];
        };
        let original = match part {
            Primitive::Polygon(p) => p.clone(),
            Primitive::Line(a, b, w) => capsule(*a, *b, *w),
            _ => return vec![],
        };
        let width = doc.drawing_style.world(
            if matches!(bond.display.as_str(), "bold" | "wedge" | "hollow_wedge") {
                doc.drawing_style.bold_width_pt
            } else {
                doc.drawing_style.line_width_pt
            },
        );
        [bond.a, bond.b]
            .into_iter()
            .filter(|id| self.atom(*id) == 1.)
            .filter_map(|id| v.rim_anchor(id).map(|p| (id, p)))
            .flat_map(|(id, anchor)| {
                // The shared corner belongs to the exposed atom anchor.
                // Restore only its original ink, never an added disc/halo.
                let round = capsule(anchor, anchor, width);
                let corner = self
                    .corners
                    .get(&(bond.a.min(bond.b), bond.a.max(bond.b), id));
                let mask = if matches!(part, Primitive::Polygon(_)) {
                    corner.unwrap_or(&round)
                } else {
                    &round
                };
                let pieces = mask_clip(&original, mask, false);
                let other = self
                    .node(if id == bond.a { bond.b } else { bond.a })
                    .position;
                let length = anchor.distance(other);
                if length < 1e-6 {
                    return vec![];
                }
                let axis = Point::new((other.x - anchor.x) / length, (other.y - anchor.y) / length);
                // Include the complete original cap extent, including any
                // shared miter projecting outside the backbone endpoints.
                let (low, high) = pieces
                    .iter()
                    .flatten()
                    .map(|p| (p.x - anchor.x) * axis.x + (p.y - anchor.y) * axis.y)
                    .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), t| {
                        (lo.min(t), hi.max(t))
                    });
                if high - low < 1e-6 {
                    return vec![];
                }
                let a = anchor.offset(axis.x * low, axis.y * low);
                let b = anchor.offset(axis.x * high, axis.y * high);
                let hidden =
                    v.external_cap_hidden(a, b, Field::Constant(self.node(id).depth), bond, id);
                pieces
                    .into_iter()
                    .flat_map(|p| split_intervals(Primitive::Polygon(p), a, b, &hidden, 0.))
                    .collect()
            })
            .collect()
    }
    fn owned_field(doc: &Document, ids: &[u64]) -> Field {
        let vertices: Vec<_> = ids
            .iter()
            .filter_map(|id| doc.atom(*id).map(|a| (a.position, a.depth)))
            .collect();
        Field::plane(&vertices).unwrap_or_else(|| {
            Field::Constant(
                vertices.iter().map(|(_, z)| z).sum::<f32>() / vertices.len().max(1) as f32,
            )
        })
    }
    pub(crate) fn owned_at(&self, doc: &Document, ids: &[u64], point: Point) -> f32 {
        let alpha = ids
            .iter()
            .map(|id| self.node(*id).opacity)
            .fold(1., f32::min);
        if self.is_empty() || ids.is_empty() || alpha >= 1. {
            return 1.;
        }
        let field = Self::owned_field(doc, ids);
        if self
            .visibility
            .as_ref()
            .is_some_and(|v| v.occluded(point, field.at(point), ids, 0))
        {
            alpha
        } else {
            1.
        }
    }
    pub(crate) fn owned_parts(
        &self,
        doc: &Document,
        ids: &[u64],
        parts: Vec<Primitive>,
    ) -> Vec<Primitive> {
        if let [id] = ids {
            return self.atom_parts(*id, parts);
        }
        let Some(v) = &self.visibility else {
            return parts;
        };
        let alpha = ids
            .iter()
            .map(|id| self.node(*id).opacity)
            .fold(1., f32::min);
        if alpha >= 1. || ids.is_empty() {
            return parts;
        }
        let field = Self::owned_field(doc, ids);
        let mut out = vec![];
        for part in parts {
            match &part {
                Primitive::Line(a, b, _) => out.extend(split_intervals(
                    part.clone(),
                    *a,
                    *b,
                    &v.hidden(*a, *b, field, ids, 0),
                    alpha,
                )),
                Primitive::Path {
                    commands,
                    filled: false,
                    ..
                } => {
                    let paths = flatten(commands);
                    let samples: Vec<_> = paths.iter().flat_map(|p| p.windows(2)).collect();
                    if samples
                        .iter()
                        .all(|p| matches!(p,[a,b] if v.hidden(*a,*b,field,ids,0).is_empty()))
                    {
                        out.push(part);
                    } else {
                        out.extend(split_curve(part, paths, field, v, ids, alpha));
                    }
                }
                _ => out.extend(split_fill(part, field, v, ids, alpha)),
            }
        }
        out
    }
}
fn parallel(a: Point, b: Point, c: Point, d: Point) -> bool {
    let x = b.x - a.x;
    let y = b.y - a.y;
    let u = d.x - c.x;
    let v = d.y - c.y;
    (x * u + y * v).abs() >= 0.7 * (x * x + y * y).sqrt() * (u * u + v * v).sqrt()
}
fn band(part: &Primitive, a: Point, b: Point, low: f32, high: f32) -> Vec<Primitive> {
    let (x, y) = (
        f64::from(b.x) - f64::from(a.x),
        f64::from(b.y) - f64::from(a.y),
    );
    let length = x * x + y * y;
    if length < 1e-12 {
        return vec![part.clone()];
    }
    let field = |threshold: f32| Field::Plane {
        origin: a,
        weight: 0.5 - f64::from(threshold),
        dx: x / length,
        dy: y / length,
    };
    let below = if low <= 0. {
        vec![part.clone()]
    } else {
        half(part, field(low), true)
    };
    if high >= 1. {
        below
    } else {
        below
            .iter()
            .flat_map(|p| half(p, field(high), false))
            .collect()
    }
}
fn split_intervals(
    part: Primitive,
    a: Point,
    b: Point,
    hidden: &[(f32, f32)],
    alpha: f32,
) -> Vec<Primitive> {
    if hidden.is_empty() || alpha >= 1. {
        return vec![part];
    }
    if matches!(hidden,[(low,high)] if *low<=0.&&*high>=1.) {
        return with_opacity(vec![part], alpha);
    }
    let part = match part {
        Primitive::Line(a, b, w) => Primitive::Polygon(capsule(a, b, w)),
        p => ink::single_paint(p),
    };
    let mut out = with_opacity(vec![part.clone()], alpha);
    let mut cursor = 0.;
    for &(low, high) in hidden {
        if low > cursor {
            out.extend(band(&part, a, b, cursor, low));
        }
        cursor = cursor.max(high);
    }
    if cursor < 1. {
        out.extend(band(&part, a, b, cursor, 1.));
    }
    out
}
/// Polygon clipping in drawing coordinates; f64 evaluations keep equality visible.
fn clip_values(points: &[Point], value: impl Fn(Point) -> f64) -> Vec<Point> {
    let mut out = vec![];
    for (&a, &b) in points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
    {
        let (va, vb) = (value(a), value(b));
        let (ia, ib) = (va >= 0., vb >= 0.);
        if ia {
            out.push(a);
        }
        if ia != ib && va.is_finite() && vb.is_finite() {
            out.push(mix(a, b, (va / (va - vb)).clamp(0., 1.) as f32));
        }
    }
    out
}
fn mask_clip(points: &[Point], mask: &[Point], outside: bool) -> Vec<Vec<Point>> {
    if mask.len() < 3 {
        return if outside {
            vec![points.to_vec()]
        } else {
            vec![]
        };
    }
    let area: f64 = mask
        .iter()
        .zip(mask.iter().cycle().skip(1))
        .take(mask.len())
        .map(|(a, b)| f64::from(a.x) * f64::from(b.y) - f64::from(a.y) * f64::from(b.x))
        .sum();
    if area.abs() < 1e-10 {
        return if outside {
            vec![points.to_vec()]
        } else {
            vec![]
        };
    }
    let sign = area.signum();
    let mut inside = points.to_vec();
    let mut out = vec![];
    for (&a, &b) in mask
        .iter()
        .zip(mask.iter().cycle().skip(1))
        .take(mask.len())
    {
        let edge = |p: Point| {
            sign * ((f64::from(b.x) - f64::from(a.x)) * (f64::from(p.y) - f64::from(a.y))
                - (f64::from(b.y) - f64::from(a.y)) * (f64::from(p.x) - f64::from(a.x)))
        };
        if outside {
            let piece = clip_values(&inside, |p| -edge(p));
            if piece.len() >= 3 {
                out.push(piece);
            }
        }
        inside = clip_values(&inside, edge);
        if inside.len() < 3 {
            break;
        }
    }
    if !outside && inside.len() >= 3 {
        out.push(inside);
    }
    out
}
fn contours(part: &Primitive) -> Vec<Vec<Point>> {
    match part {
        Primitive::Polygon(p) => vec![p.clone()],
        Primitive::Path {
            commands,
            filled: true,
            ..
        } => flatten(commands),
        _ => vec![],
    }
}
fn contour_part(part: &Primitive, contours: Vec<Vec<Point>>) -> Vec<Primitive> {
    let mut commands = vec![];
    for path in contours {
        if let Some(first) = path.first().filter(|_| path.len() >= 3) {
            commands.push(PathCommand::Move(*first));
            commands.extend(path.iter().skip(1).copied().map(PathCommand::Line));
            commands.push(PathCommand::Close);
        }
    }
    if commands.is_empty() {
        return vec![];
    }
    let style = match part {
        Primitive::Path { style, .. } => style.clone(),
        _ => crate::graphics::GraphicStyle {
            fill: Some(crate::palette::Color::Ink),
            width_pt: 0.,
            ..Default::default()
        },
    };
    vec![Primitive::Path {
        commands,
        style,
        filled: true,
    }]
}
fn split_fill(
    part: Primitive,
    field: Field,
    v: &visibility::Visibility,
    ids: &[u64],
    alpha: f32,
) -> Vec<Primitive> {
    let single = ink::single_paint(part.clone());
    let original = contours(&single);
    if original.is_empty() {
        return vec![part];
    }
    let mut visible = original.clone();
    for mask in v.masks(field, ids) {
        visible = visible
            .iter()
            .flat_map(|p| mask_clip(p, &mask, true))
            .collect();
        // A conservative whole-shape fallback avoids insertion-order partial
        // rendering if a highly subdivided custom filled shape exceeds budget.
        if visible.len() > 8192 {
            return vec![part];
        }
    }
    if visible == original {
        return vec![part];
    }
    let mut out = with_opacity(vec![single.clone()], alpha);
    out.extend(contour_part(&single, visible));
    out
}
fn split_curve(
    part: Primitive,
    paths: Vec<Vec<Point>>,
    field: Field,
    v: &visibility::Visibility,
    ids: &[u64],
    alpha: f32,
) -> Vec<Primitive> {
    let width = match &part {
        Primitive::Path { style, .. } => style.width(),
        _ => return vec![part],
    };
    let single = ink::single_paint(part);
    let original = contours(&single);
    let mut visible = vec![];
    for path in paths {
        for pair in path.windows(2) {
            let &[a, b] = pair else {
                continue;
            };
            let hidden = v.hidden(a, b, field, ids, 0);
            let mut intervals = vec![];
            let mut low = 0.;
            for (s, e) in hidden {
                if s > low {
                    intervals.push((low, s));
                }
                low = e;
            }
            if low < 1. {
                intervals.push((low, 1.));
            }
            for (s, e) in intervals {
                let mask = Primitive::Polygon(capsule(mix(a, b, s), mix(a, b, e), width + 0.04));
                let clipped = band(&mask, a, b, s, e);
                let mask = clipped.first().map(contours).unwrap_or_default();
                for contour in &original {
                    for mask in &mask {
                        visible.extend(mask_clip(contour, mask, false));
                    }
                }
            }
            if visible.len() > 8192 {
                return vec![single];
            }
        }
    }
    let mut out = with_opacity(vec![single.clone()], alpha);
    out.extend(contour_part(&single, visible));
    out
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
fn mix(a: Point, b: Point, t: f32) -> Point {
    Point::new(lerp(a.x, b.x, t), lerp(a.y, b.y, t))
}

#[derive(Clone, Copy)]
pub(crate) enum Field {
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
            } => {
                (weight
                    + (f64::from(p.x) - f64::from(origin.x)) * dx
                    + (f64::from(p.y) - f64::from(origin.y)) * dy) as f32
            }
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
#[cfg(test)]
mod visibility_tests;
