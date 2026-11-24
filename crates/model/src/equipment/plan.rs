//! The paint plan: placed pieces become one ordered list of plain paths, each
//! tagged with its owner, pass and material. Painting, hit testing and
//! ChemDraw export all read this list, so they agree on what is on top.
//! Nothing is clipped at render time: seated cones, hatching and liquid are
//! cut here, geometrically.
use super::eval::{Evaluated, Values, evaluate, resolve, tube_outline};
use super::format::*;
use super::geometry::{Affine, Seg, add, area, dot, mul, normalize, perp, polygon, sub};
use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::overlay::FloatOverlay;
use std::collections::BTreeSet;

#[cfg(test)]
mod liquid_tests;

/// A joint detail's material, colour, ink, style, path and flat-cap policy.
type Detail = (
    Material,
    Option<Rgb>,
    Ink,
    Option<StyleKind>,
    Vec<Seg>,
    bool,
);
/// A piece's glass fill and outline, colour, style and whether the outline was cut.
type GlassPath = (Vec<Seg>, Vec<Seg>, Option<Rgb>, Option<StyleKind>, bool);

/// One placed piece.
#[derive(Debug, Clone)]
pub struct Instance<'a> {
    pub definition: &'a Definition,
    pub values: Values,
    /// Millimetres to world units: a similarity.
    pub transform: Affine,
    /// Joints with a partner seated in them.
    pub seated: BTreeSet<String>,
    /// A tube piece's centre line, in the piece's millimetres.
    pub centre_line: Vec<Xy>,
    /// An edited rigid tube's rounded route in physical millimetres.
    pub tube_route: Option<super::tube_bend::Route>,
}

impl<'a> Instance<'a> {
    pub fn new(definition: &'a Definition, transform: Affine) -> Self {
        Self {
            definition,
            values: Values::new(),
            transform,
            seated: BTreeSet::new(),
            centre_line: Vec::new(),
            tube_route: None,
        }
    }
}

/// Paint order inside one compositing group (an assembly or a free piece).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Pass {
    Back,
    /// Glass outlines at twice the stroke width, piece by piece …
    Outline,
    /// … each followed by its opaque glass fills, so a piece is one silhouette.
    Glass,
    Liquid,
    Inner,
    Body,
    Front,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Weight {
    /// The outline stroke (0.6 pt by default; twice that in the outline pass).
    Outline,
    Detail,
    Hatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Ink {
    Fill,
    Stroke(Weight),
    FillStroke(Weight),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PlanPath {
    /// Index of the owning instance.
    pub owner: usize,
    pub pass: Pass,
    pub material: Material,
    pub color: Option<Rgb>,
    pub ink: Ink,
    /// Only in this style; `None` in both.
    pub style: Option<StyleKind>,
    /// The shape's depth within its layer, across an assembly.
    #[serde(skip)]
    pub depth: i8,
    /// Open strokes clipped at a physical boundary end at their endpoints.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub flat_caps: bool,
    /// World units.
    #[serde(serialize_with = "segs")]
    pub path: Vec<Seg>,
}

fn segs<S: serde::Serializer>(path: &[Seg], s: S) -> Result<S::Ok, S::Error> {
    let r = |v: f64| (v * 1000.).round() / 1000.;
    let mut text = String::new();
    for seg in path {
        let part = match *seg {
            Seg::Move(p) => format!("M{} {}", r(p[0]), r(p[1])),
            Seg::Line(p) => format!("L{} {}", r(p[0]), r(p[1])),
            Seg::Cubic(a, b, p) => format!(
                "C{} {} {} {} {} {}",
                r(a[0]),
                r(a[1]),
                r(b[0]),
                r(b[1]),
                r(p[0]),
                r(p[1])
            ),
            Seg::Close => "Z".into(),
        };
        text.push_str(&part);
    }
    s.serialize_str(&text)
}

/// Appearance-independent settings the geometry depends on.
#[derive(Debug, Clone, Copy)]
pub struct Settings {
    /// Page points per world unit (14.4/42 by default).
    pub points_per_world: f64,
    /// Ground-glass hatch spacing in points.
    pub hatch_pt: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            points_per_world: 14.4 / 42.,
            hatch_pt: 1.5,
        }
    }
}

/// How far a hose pushed onto a barb reaches past the barb's tip, over its ribs.
const HOSE_CUFF_MM: f64 = 12.;
pub(super) const GLASS_OUTLINE_PT: f32 = 0.6;
pub(super) const JOINT_HATCH_PT: f32 = 0.25;
const SOCKET_GROUND_START_MM: f64 = 1.2;

/// The paint plan of one compositing group, in paint order.
pub fn paint_plan(group: &[Instance], settings: Settings) -> Result<Vec<PlanPath>, String> {
    let mut items = Vec::new();
    for (owner, instance) in group.iter().enumerate() {
        let values = resolve(instance.definition, &instance.values)?;
        if let Some(route) = &instance.tube_route {
            items.extend(routed_tube(owner, instance, &values, route)?);
            continue;
        }
        let evaluated = evaluate(instance.definition, &values)?;
        piece(owner, instance, &evaluated, settings, &mut items)?;
    }
    order(&mut items);
    Ok(items)
}

/// A rigid tube's physical route replaces its stock shapes. Circular offsets
/// keep the same wall thickness through each bend in Exterior and Section.
fn routed_tube(
    owner: usize,
    instance: &Instance,
    values: &Values,
    route: &super::tube_bend::Route,
) -> Result<Vec<PlanPath>, String> {
    let spec = super::parts_list::spec(instance.definition)
        .ok_or("A routed tube needs its physical tube specification.")?;
    let centerline = route.centerline()?;
    let sides = |width: f64| routed_sides(&centerline.segments, width);
    let (outer_left, outer_right) = sides(spec.od_mm);
    let mut out = Vec::new();
    let mut push = |depth, material, color, ink, path: Vec<Seg>| {
        if !path.is_empty() {
            out.push(PlanPath {
                owner,
                pass: Pass::Body,
                material,
                color,
                ink,
                style: None,
                depth,
                flat_caps: false,
                path: instance.transform.map(&path),
            });
        }
    };
    let color = instance
        .definition
        .shapes
        .iter()
        .find(|shape| shape.material == Material::Metal && shape.role.filled())
        .and_then(|shape| shape.color);
    push(
        0,
        Material::Metal,
        color,
        Ink::Fill,
        routed_band(&outer_left, &outer_right),
    );
    let section =
        matches!(values.get("view"), Some(super::eval::Value::State(view)) if view == "section");
    let inner = spec.id_mm.map(sides);
    if section && let Some((left, right)) = &inner {
        push(
            1,
            Material::Paper,
            None,
            Ink::Fill,
            routed_band(left, right),
        );
    }
    push(
        2,
        Material::Ink,
        None,
        Ink::Stroke(Weight::Outline),
        [outer_left.clone(), outer_right.clone()].concat(),
    );
    if let Some((inner_left, inner_right)) = &inner {
        if section {
            push(
                2,
                Material::Ink,
                None,
                Ink::Stroke(Weight::Detail),
                [inner_left.clone(), inner_right.clone()].concat(),
            );
        }
        let mut rims = Vec::new();
        for (outer, inner) in [(&outer_left, inner_left), (&outer_right, inner_right)] {
            for (a, b) in [
                (routed_endpoint(outer, false), routed_endpoint(inner, false)),
                (routed_endpoint(outer, true), routed_endpoint(inner, true)),
            ] {
                if let (Some(a), Some(b)) = (a, b) {
                    rims.extend([Seg::Move(a), Seg::Line(b)]);
                }
            }
        }
        push(2, Material::Ink, None, Ink::Stroke(Weight::Detail), rims);
    }
    Ok(out)
}

fn routed_endpoint(path: &[Seg], last: bool) -> Option<Xy> {
    match if last { path.last() } else { path.first() } {
        Some(Seg::Move(p) | Seg::Line(p) | Seg::Cubic(_, _, p)) => Some(*p),
        _ => None,
    }
}

pub(super) fn routed_band(left: &[Seg], right: &[Seg]) -> Vec<Seg> {
    let Some(at) = routed_endpoint(right, false) else {
        return Vec::new();
    };
    let mut out = left.to_vec();
    out.push(Seg::Line(at));
    out.extend(right.iter().skip(1).copied());
    out.push(Seg::Close);
    out
}

pub(super) fn routed_sides(
    segments: &[super::tube_bend::Segment],
    width: f64,
) -> (Vec<Seg>, Vec<Seg>) {
    (
        routed_side(segments, width / 2., false),
        routed_side(segments, width / 2., true),
    )
}

/// The side on the left of a route, or on its right when traced backwards.
fn routed_side(segments: &[super::tube_bend::Segment], offset: f64, reverse: bool) -> Vec<Seg> {
    use super::tube_bend::Segment;
    let rotate = |p: Xy, angle: f64| {
        let (sin, cos) = angle.sin_cos();
        [p[0] * cos - p[1] * sin, p[0] * sin + p[1] * cos]
    };
    let mut out = Vec::new();
    let mut append = |segment: &Segment| match *segment {
        Segment::Straight { from, to } => {
            let (from, to) = if reverse { (to, from) } else { (from, to) };
            let normal = mul(perp(normalize(sub(to, from))), offset);
            if out.is_empty() {
                out.push(Seg::Move(add(from, normal)));
            }
            out.push(Seg::Line(add(to, normal)));
        }
        Segment::Arc {
            center,
            from,
            sweep_rad,
        } => {
            let radial = sub(from, center);
            let (radial, sweep) = if reverse {
                (rotate(radial, sweep_rad), -sweep_rad)
            } else {
                (radial, sweep_rad)
            };
            let radius = super::geometry::len(radial);
            let radial = mul(radial, (radius - sweep.signum() * offset) / radius);
            if out.is_empty() {
                out.push(Seg::Move(add(center, radial)));
            }
            // Circle offsets remain concentric; only the final paint paths
            // use the same cubic approximation as authored SVG arc commands.
            let steps = (sweep.abs() / std::f64::consts::FRAC_PI_2).ceil().max(1.) as usize;
            let step = sweep / steps as f64;
            let control = 4. / 3. * (step / 4.).tan();
            for k in 0..steps {
                let a = rotate(radial, k as f64 * step);
                let b = rotate(radial, (k + 1) as f64 * step);
                out.push(Seg::Cubic(
                    add(center, add(a, mul(perp(a), control))),
                    add(center, sub(b, mul(perp(b), control))),
                    add(center, b),
                ));
            }
        }
    };
    if reverse {
        for segment in segments.iter().rev() {
            append(segment);
        }
    } else {
        for segment in segments {
            append(segment);
        }
    }
    out
}

/// Put a group's paths in paint order: every piece's back layer, then each
/// piece's glass as one silhouette with its liquid, then the inner, body and
/// front layers of all pieces, each by depth, then by owner. A stable sort
/// keeps each piece's own order inside a pass, so the plans of single
/// pieces merge into the plan of their assembly.
pub fn order(paths: &mut [PlanPath]) {
    paths.sort_by_key(|p| {
        let per_piece = matches!(p.pass, Pass::Outline | Pass::Glass | Pass::Liquid);
        (
            if per_piece { Pass::Outline } else { p.pass },
            if per_piece { 0 } else { p.depth },
            p.owner,
            if per_piece { p.pass } else { Pass::Back },
        )
    });
}

pub(super) fn layer_pass(layer: Layer) -> Pass {
    match layer {
        Layer::Back => Pass::Back,
        Layer::Body => Pass::Body,
        Layer::Inner => Pass::Inner,
        Layer::Front => Pass::Front,
    }
}

fn piece(
    owner: usize,
    instance: &Instance,
    e: &Evaluated,
    settings: Settings,
    items: &mut Vec<PlanPath>,
) -> Result<(), String> {
    let t = instance.transform;
    let mm = t.scale_factor();
    let push = |items: &mut Vec<PlanPath>,
                pass: Pass,
                depth: i8,
                material: Material,
                color: Option<Rgb>,
                ink: Ink,
                style: Option<StyleKind>,
                path: &[Seg],
                flat_caps: bool| {
        items.push(PlanPath {
            owner,
            pass,
            material,
            color,
            ink,
            style,
            depth,
            flat_caps,
            path: t.map(path),
        })
    };
    // Glass on the body layer joins one silhouette: outlines, then fills.
    let mut glass: Vec<GlassPath> = Vec::new();
    // A source edge within half the outline width can show through at the
    // mouth. Stop at the ground zone so deeper glass details stay intact.
    let cap_depth = (f64::from(GLASS_OUTLINE_PT) / (mm * settings.points_per_world))
        .clamp(RIM_TOLERANCE_MM, SOCKET_GROUND_START_MM);
    let socket_mouths: Vec<_> = e
        .joints
        .iter()
        .filter_map(|j| {
            let t = j
                .joint
                .standard()
                .filter(|_| j.joint.gender() == Gender::F)?;
            Some(SocketMouth {
                at: j.at,
                axis: normalize(j.dir),
                bore: t.d / 2.,
                lip: t.bead_radius(),
                cap_depth,
            })
        })
        .collect();
    for d in &e.shapes {
        let s = d.shape;
        if s.role == Role::Glass && s.layer == Layer::Body {
            let outline = open_socket_mouths(&d.path, &socket_mouths);
            let cut = outline != d.path;
            glass.push((d.path.clone(), outline, s.color, s.style, cut));
            continue;
        }
        let ink = match (s.role, s.material) {
            (Role::Detail, _) => Ink::Stroke(Weight::Detail),
            (Role::Outline, _) => Ink::Stroke(Weight::Outline),
            (Role::Fill, _) | (_, Material::Ink) => Ink::Fill,
            _ => Ink::FillStroke(Weight::Outline),
        };
        push(
            items,
            layer_pass(s.layer),
            s.depth,
            s.material,
            s.color,
            ink,
            s.style,
            &d.path,
            false,
        );
    }
    let points_per_mm = mm * settings.points_per_world;
    let hatch_mm = settings.hatch_pt / points_per_mm;
    let hatch_radius_mm = f64::from(JOINT_HATCH_PT) / (2. * points_per_mm);
    let mut details: Vec<Detail> = Vec::new();
    for j in &e.joints {
        let Some(size) = j.joint.standard() else {
            continue;
        };
        let seated = instance.seated.contains(j.joint.id());
        standard_joint(
            j.joint.gender(),
            size,
            j.at,
            j.dir,
            seated,
            hatch_mm,
            hatch_radius_mm,
            &mut glass,
            &mut details,
        );
    }
    glass.sort_by(|a, b| fill_area(&a.0).total_cmp(&fill_area(&b.0)));
    for (_, outline, _, style, cut) in &glass {
        push(
            items,
            Pass::Outline,
            0,
            Material::Glass,
            None,
            Ink::Stroke(Weight::Outline),
            *style,
            outline,
            *cut,
        );
    }
    for (fill, _, color, style, _) in &glass {
        push(
            items,
            Pass::Glass,
            0,
            Material::Glass,
            *color,
            Ink::Fill,
            *style,
            fill,
            false,
        );
    }
    for (material, color, ink, style, path, flat_caps) in details {
        push(
            items,
            Pass::Body,
            0,
            material,
            color,
            ink,
            style,
            &path,
            flat_caps,
        );
    }
    if let Some(tube) = &instance.definition.tube
        && instance.centre_line.len() >= 2
    {
        // A seated end is pushed over its barb's ribs.
        let mut line = instance.centre_line.clone();
        for (k, end) in tube.ends.iter().enumerate() {
            if instance.seated.contains(end) {
                if k == 1 {
                    line.reverse();
                }
                let a = line.first().copied().unwrap_or_default();
                // The first point apart from the end gives its direction.
                if let Some(b) = line
                    .iter()
                    .find(|p| sub(**p, a).iter().any(|v| v.abs() > 1e-9))
                {
                    line.insert(0, add(a, mul(normalize(sub(a, *b)), HOSE_CUFF_MM)));
                }
                if k == 1 {
                    line.reverse();
                }
            }
        }
        let path = tube_outline(&line, tube.width_mm);
        push(
            items,
            Pass::Front,
            0,
            tube.material,
            tube.color,
            Ink::FillStroke(Weight::Outline),
            None,
            &path,
            false,
        );
    }
    for l in &e.liquids {
        // The surface stays level on the page, so liquid is cut in world space.
        let region: Vec<Vec<Xy>> = l
            .region
            .iter()
            .map(|c| c.iter().map(|p| t.apply(*p)).collect())
            .collect();
        for path in liquid(&region, l.fill, l.liquid.meniscus, mm) {
            items.push(PlanPath {
                owner,
                pass: Pass::Liquid,
                material: Material::Liquid,
                color: Some(l.color),
                ink: path.0,
                style: None,
                depth: 0,
                flat_caps: false,
                path: path.1,
            });
        }
    }
    Ok(())
}

pub(super) fn fill_area(path: &[Seg]) -> f64 {
    super::geometry::flatten(path)
        .iter()
        .map(|(p, _)| area(p).abs())
        .sum()
}

struct SocketMouth {
    at: Xy,
    axis: Xy,
    bore: f64,
    lip: f64,
    cap_depth: f64,
}

const RIM_TOLERANCE_MM: f64 = 0.5;

impl SocketMouth {
    fn at_lip(&self, point: Xy) -> bool {
        // Rim marks can sit slightly outside the authored edge.
        let local = sub(point, self.at);
        (-self.cap_depth..=RIM_TOLERANCE_MM).contains(&dot(local, self.axis))
            && dot(local, perp(self.axis)).abs() <= self.lip + RIM_TOLERANCE_MM
    }

    fn open_outline(&self, edges: &mut [OutlineEdge]) {
        let at_lip = |edge: &OutlineEdge| {
            matches!(edge.command, Seg::Line(_)) && self.at_lip(edge.from) && self.at_lip(edge.to)
        };
        let Some(seam) = edges.iter().position(|edge| !at_lip(edge)) else {
            return;
        };
        let (before, after) = edges.split_at_mut(seam + 1);
        let across = perp(self.axis);
        let mut lip: Vec<&mut OutlineEdge> = Vec::new();
        for edge in after.iter_mut().chain(before) {
            if at_lip(edge) {
                lip.push(edge);
                continue;
            }
            if let Some((first, last)) = lip.first().zip(lip.last()) {
                let a = dot(sub(first.from, self.at), across);
                let b = dot(sub(last.to, self.at), across);
                if a.min(b) <= -self.bore + RIM_TOLERANCE_MM
                    && a.max(b) >= self.bore - RIM_TOLERANCE_MM
                {
                    for edge in &mut lip {
                        edge.open = true;
                    }
                }
            }
            lip.clear();
        }
    }
}

struct OutlineEdge {
    command: Seg,
    from: Xy,
    to: Xy,
    open: bool,
}

/// Open the ink of a straight source rim, possibly split into several edges,
/// while preserving the glass fill.
/// Leave authored curves, open outlines and the rest of each contour alone.
fn open_socket_mouths(path: &[Seg], mouths: &[SocketMouth]) -> Vec<Seg> {
    if mouths.is_empty() {
        return path.to_vec();
    }
    let mut outline = Vec::with_capacity(path.len());
    for contour in path.chunk_by(|_, next| !matches!(next, Seg::Move(_))) {
        let [Seg::Move(start), .., Seg::Close] = contour else {
            outline.extend_from_slice(contour);
            continue;
        };
        let mut previous = *start;
        let mut edges: Vec<_> = contour
            .iter()
            .skip(1)
            .map(|&command| {
                let (command, end) = match command {
                    Seg::Move(p) | Seg::Line(p) | Seg::Cubic(_, _, p) => (command, p),
                    Seg::Close => (Seg::Line(*start), *start),
                };
                let edge = OutlineEdge {
                    command,
                    from: previous,
                    to: end,
                    open: false,
                };
                previous = end;
                edge
            })
            .collect();
        for mouth in mouths {
            mouth.open_outline(&mut edges);
        }
        let Some(gap) = edges.iter().position(|edge| edge.open) else {
            outline.extend_from_slice(contour);
            continue;
        };
        let mut resume_at = None;
        for edge in edges.iter().cycle().skip(gap).take(edges.len()) {
            if edge.open {
                resume_at = Some(edge.to);
            } else {
                if let Some(p) = resume_at.take() {
                    outline.push(Seg::Move(p));
                }
                outline.push(edge.command);
            }
        }
    }
    outline
}

/// A shape symmetric about an axis: profile of (distance along `u`, radius) pairs.
fn axial(p: Xy, u: Xy, profile: &[(f64, f64)]) -> Vec<Xy> {
    let v = perp(u);
    let at = |s: f64, r: f64| add(add(p, mul(u, s)), mul(v, r));
    profile
        .iter()
        .map(|&(s, r)| at(s, r))
        .chain(profile.iter().rev().map(|&(s, r)| at(s, -r)))
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn standard_joint(
    gender: Gender,
    t: &TaperSize,
    at: Xy,
    dir: Xy,
    seated: bool,
    hatch_mm: f64,
    hatch_radius_mm: f64,
    glass: &mut Vec<GlassPath>,
    details: &mut Vec<Detail>,
) {
    let (big, small) = (t.d / 2., t.d / 2. - t.l / 20.);
    // Draw the hatch within the frosted fill before drawing its boundary.
    let ground = |points: Vec<Xy>, cut: bool, details: &mut Vec<Detail>| {
        let filled = polygon(&points, true);
        details.push((Material::Frosted, None, Ink::Fill, None, filled, false));
        // The second edge is the exposed end of the taper at the socket mouth.
        let open_edge = cut.then_some(1);
        let lines = hatch(&points, hatch_mm, hatch_radius_mm, open_edge);
        if !lines.is_empty() {
            details.push((
                Material::Frosted,
                None,
                Ink::Stroke(Weight::Hatch),
                Some(StyleKind::Line),
                lines,
                true,
            ));
        }
        let mut boundary = points;
        if let Some(edge) = open_edge {
            boundary.rotate_left(edge + 1);
        }
        details.push((
            Material::Frosted,
            None,
            Ink::Stroke(Weight::Hatch),
            None,
            polygon(&boundary, !cut),
            cut,
        ));
    };
    match gender {
        Gender::F => {
            let u = mul(normalize(dir), -1.);
            let v = perp(u);
            let rim = |s: f64, r: f64| add(add(at, mul(u, s)), mul(v, r));
            let ri = |s: f64| big - s / 20.;
            let ro = |s: f64| ri(s) + t.socket_wall;
            let rb = t.bead_radius();
            // The empty socket has a straight mouth in side view. Leave that
            // edge unstroked when a seated cone occupies the bore.
            let body = axial(
                at,
                u,
                &[
                    (0., big),
                    (0., rb),
                    (2.8, rb),
                    (3.6, ro(3.6)),
                    (t.l, ro(t.l)),
                    (t.l + 4., t.neck / 2.),
                ],
            );
            glass.push((
                polygon(&body, true),
                polygon(&body, !seated),
                None,
                None,
                seated,
            ));
            ground(
                axial(
                    at,
                    u,
                    &[
                        (SOCKET_GROUND_START_MM, ri(SOCKET_GROUND_START_MM)),
                        (t.l - 0.6, ri(t.l - 0.6)),
                    ],
                ),
                false,
                details,
            );
            for side in [-1., 1.] {
                details.push((
                    Material::Glass,
                    None,
                    Ink::Stroke(Weight::Detail),
                    None,
                    polygon(&[rim(2.8, side * rb), rim(2.8, side * ri(2.8))], false),
                    true,
                ));
            }
        }
        Gender::M => {
            let u = normalize(dir);
            let r = |s: f64| big - s / 20.;
            let collar = big + 0.8;
            if seated {
                // Cut at the socket mouth; the cut edge is filled but not stroked.
                let e = t.exposure();
                let half: Vec<(f64, f64)> =
                    vec![(-3.6, collar), (-0.5, collar), (0., big), (e, r(e))];
                let body = axial(at, u, &half);
                // Start after the cut edge so the open outline leaves it out.
                let mut outline = body.clone();
                outline.rotate_left(half.len());
                glass.push((
                    polygon(&body, true),
                    polygon(&outline, false),
                    None,
                    None,
                    true,
                ));
                ground(axial(at, u, &[(0., big), (e, r(e))]), true, details);
            } else {
                let body = axial(
                    at,
                    u,
                    &[
                        (-3.6, collar),
                        (-0.5, collar),
                        (0., big),
                        (t.l, small),
                        (t.l + 1.4, small - 0.7),
                    ],
                );
                glass.push((
                    polygon(&body, true),
                    polygon(&body, true),
                    None,
                    None,
                    false,
                ));
                ground(axial(at, u, &[(0., big), (t.l, small)]), false, details);
            }
        }
    }
}

/// 45° flat-ended hatch strokes inside a convex polygon, `h` apart.
fn hatch(points: &[Xy], h: f64, radius: f64, open_edge: Option<usize>) -> Vec<Seg> {
    let n = [
        std::f64::consts::FRAC_1_SQRT_2,
        -std::f64::consts::FRAC_1_SQRT_2,
    ];
    let along = [
        std::f64::consts::FRAC_1_SQRT_2,
        std::f64::consts::FRAC_1_SQRT_2,
    ];
    let ts: Vec<f64> = points.iter().map(|p| dot(*p, n)).collect();
    let (t0, t1) = ts
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &t| {
            (a.min(t), b.max(t))
        });
    let mut out = Vec::new();
    if h.is_nan() || h <= 0. || !radius.is_finite() || (t1 - t0) / h > 10_000. {
        return out;
    }
    let winding = area(points).signum();
    // Allow for the hatch pen's crosswise extent and for any boundary stroke.
    // Their translucent ink should meet without layering at hatch endpoints.
    let planes: Vec<_> = super::geometry::edges(points, true)
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(edge, (a, b))| {
            let inward = mul(normalize(perp(sub(b, a))), winding);
            let boundary_inset = if Some(edge) == open_edge { 0. } else { radius };
            let limit = dot(inward, a) + radius * dot(inward, n).abs() + boundary_inset;
            (inward, limit, dot(inward, along))
        })
        .collect();
    // Center the pattern in the patch so a narrow taper can still show a hatch.
    let center = t0.midpoint(t1);
    let first = ((t0 - center) / h).ceil();
    // A counted loop: t + h may not advance t when the coordinates are huge.
    for k in 0..(((t1 - center) / h - first).ceil().max(0.) as usize) {
        let base = mul(n, center + (first + k as f64) * h);
        let (mut start, mut end) = (f64::NEG_INFINITY, f64::INFINITY);
        for &(normal, limit, slope) in &planes {
            let distance = limit - dot(normal, base);
            if slope > 1e-12 {
                start = start.max(distance / slope);
            } else if slope < -1e-12 {
                end = end.min(distance / slope);
            } else if distance > 0. {
                end = f64::NEG_INFINITY;
                break;
            }
        }
        if start < end && start.is_finite() && end.is_finite() {
            out.push(Seg::Move(add(base, mul(along, start))));
            out.push(Seg::Line(add(base, mul(along, end))));
        }
    }
    out
}

type Pools = Vec<Vec<Vec<Xy>>>;

/// The part of a region (world space, y down) below the level `y`: pools,
/// each an outer contour and its holes.
fn below(region: &[Vec<Xy>], y: f64) -> Pools {
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for p in region.iter().flatten() {
        lo = [lo[0].min(p[0]), lo[1].min(p[1])];
        hi = [hi[0].max(p[0]), hi[1].max(p[1])];
    }
    let clip = vec![
        [lo[0] - 1., y],
        [hi[0] + 1., y],
        [hi[0] + 1., hi[1] + 1.],
        [lo[0] - 1., hi[1] + 1.],
    ];
    FloatOverlay::with_subj_and_clip(&region.to_vec(), &clip)
        .overlay(OverlayRule::Intersect, FillRule::EvenOdd)
}

fn pooled(shapes: &Pools) -> f64 {
    shapes
        .iter()
        .flat_map(|s| s.iter().enumerate())
        .map(|(i, c)| {
            if i == 0 {
                area(c).abs()
            } else {
                -area(c).abs()
            }
        })
        .sum()
}

fn span(region: &[Vec<Xy>]) -> (f64, f64) {
    region
        .iter()
        .flatten()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), p| {
            (a.min(p[1]), b.max(p[1]))
        })
}

/// The region's whole area, by the same even-odd rule as the cut, so holes
/// and windings agree.
fn whole(region: &[Vec<Xy>]) -> f64 {
    pooled(&below(region, span(region).0 - 1.))
}

/// The share of a region's drawn area below the level `y`, 0–1.
pub fn share_below(region: &[Vec<Xy>], y: f64) -> f64 {
    let total = whole(region);
    if total <= 0. {
        return 0.;
    }
    (pooled(&below(region, y)) / total).clamp(0., 1.)
}

/// The level that leaves `fill` of the region's area below it, by 24 bisection steps.
pub fn level(region: &[Vec<Xy>], fill: f64) -> f64 {
    let (mut lo, mut hi) = span(region);
    let total = whole(region);
    if fill >= 1. || total <= 0. {
        return lo;
    }
    for _ in 0..24 {
        let mid = (lo + hi) / 2.;
        if pooled(&below(region, mid)) / total > fill {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (lo + hi) / 2.
}

/// Stretches of the cut surface at `level`, each from left to right.
fn surfaces(shapes: &Pools, level: f64) -> Vec<(f64, f64)> {
    cut_edges(shapes, level)
        .map(|(a, b)| (a[0].min(b[0]), a[0].max(b[0])))
        .collect()
}

fn is_cut_edge(a: Xy, b: Xy, level: f64) -> bool {
    (a[1] - level).abs() < 1e-6 && (b[1] - level).abs() < 1e-6 && a[0] != b[0]
}

fn cut_edges(shapes: &Pools, level: f64) -> impl Iterator<Item = (Xy, Xy)> + '_ {
    shapes
        .iter()
        .flat_map(|shape| shape.first())
        .flat_map(|outer| super::geometry::edges(outer, true))
        .filter(move |&(a, b)| is_cut_edge(a, b, level))
}

/// Where the liquid's surface is, for its handle: the middle of its widest
/// stretch, or the region's lowest point while it is empty and its highest
/// once it is full.
pub fn surface(region: &[Vec<Xy>], fill: f64) -> Option<Xy> {
    let points = || region.iter().flatten().copied();
    let lowest = points().max_by(|a, b| a[1].total_cmp(&b[1]))?;
    if fill <= 0. {
        return Some(lowest);
    }
    if fill >= 1. {
        return points().min_by(|a, b| a[1].total_cmp(&b[1]));
    }
    let y = level(region, fill);
    let widest = surfaces(&below(region, y), y)
        .into_iter()
        .max_by(|a, b| (a.1 - a.0).total_cmp(&(b.1 - b.0)));
    Some(widest.map_or([lowest[0], y], |(l, r)| [(l + r) / 2., y]))
}

/// Pools below the level that leaves `fill` of the region's area, and a meniscus on each.
fn liquid(region: &[Vec<Xy>], fill: f64, meniscus: Meniscus, scale: f64) -> Vec<(Ink, Vec<Seg>)> {
    if fill <= 0. || region.is_empty() {
        return Vec::new();
    }
    let pools = |shapes: Pools| -> Vec<(Ink, Vec<Seg>)> {
        shapes
            .into_iter()
            .map(|shape| {
                (
                    Ink::Fill,
                    shape.iter().flat_map(|c| polygon(c, true)).collect(),
                )
            })
            .collect()
    };
    if fill >= 1. || whole(region) <= 0. {
        return pools(below(region, span(region).0 - 1.));
    }
    let level = level(region, fill);
    let shapes = below(region, level);
    if meniscus == Meniscus::Flat {
        return pools(shapes);
    }
    let mut surfaces = liquid_surfaces(&shapes, level, meniscus, scale, region).into_iter();
    let (mut out, mut strokes) = (Vec::new(), Vec::new());
    for shape in shapes {
        let mut path = Vec::new();
        for (contour, points) in shape.iter().enumerate() {
            if contour != 0 {
                path.extend(polygon(points, true));
                continue;
            }
            if let Some(start) = points.first() {
                path.push(Seg::Move(*start));
            }
            for (edge, (a, b)) in super::geometry::edges(points, true).enumerate() {
                if is_cut_edge(a, b, level) {
                    let (left, right) = if a[0] < b[0] { (a, b) } else { (b, a) };
                    let segment = surfaces
                        .next()
                        .map_or(Seg::Line(right), |surface| surface.segment());
                    path.push(match segment {
                        Seg::Cubic(c1, c2, _) if a[0] > b[0] => Seg::Cubic(c2, c1, b),
                        Seg::Cubic(..) => segment,
                        _ => Seg::Line(b),
                    });
                    strokes.push((Ink::Stroke(Weight::Detail), vec![Seg::Move(left), segment]));
                } else if edge + 1 < points.len() {
                    path.push(Seg::Line(b));
                }
            }
            path.push(Seg::Close);
        }
        out.push((Ink::Fill, path));
    }
    // Every fill precedes every surface stroke, including disconnected pools.
    out.extend(strokes);
    out
}

/// A left-to-right surface, in the original pool traversal order.
struct LiquidSurface {
    order: usize,
    left: Xy,
    right: Xy,
    controls: Option<[Xy; 2]>,
}

impl LiquidSurface {
    fn segment(&self) -> Seg {
        self.controls.map_or(Seg::Line(self.right), |[c1, c2]| {
            Seg::Cubic(c1, c2, self.right)
        })
    }
}

/// Check all control hulls against the source in one overlay. Disjoint interior
/// X intervals keep the subject's even-odd rule from cancelling overlapping hulls.
fn liquid_surfaces(
    shapes: &Pools,
    level: f64,
    meniscus: Meniscus,
    scale: f64,
    region: &[Vec<Xy>],
) -> Vec<LiquidSurface> {
    let mut surfaces: Vec<_> = cut_edges(shapes, level)
        .enumerate()
        .map(|(order, (a, b))| {
            let (a, b) = if a[0] < b[0] { (a, b) } else { (b, a) };
            let m = (1.6 * scale).min((b[0] - a[0]) / 8.)
                * if meniscus == Meniscus::Convex {
                    -1.
                } else {
                    1.
                };
            // Keep the old centre offset (0.2 m), with the wall contacts fixed.
            let q = [a[0].midpoint(b[0]), level + 0.4 * m];
            let c1 = add(a, mul(sub(q, a), 2. / 3.));
            let c2 = add(b, mul(sub(q, b), 2. / 3.));
            let hull_area = area(&[[0., 0.], sub(c1, a), sub(c2, a), sub(b, a)]);
            let nondegenerate = m != 0.
                && a[0] < c1[0]
                && c1[0] < c2[0]
                && c2[0] < b[0]
                && c1[1].is_finite()
                && c2[1].is_finite()
                && hull_area.is_finite()
                && hull_area != 0.;
            LiquidSurface {
                order,
                left: a,
                right: b,
                controls: nondegenerate.then_some([c1, c2]),
            }
        })
        .collect();
    surfaces.sort_by(|a, b| a.left[0].total_cmp(&b.left[0]));
    // A running rightmost endpoint also catches nested intervals, without
    // comparing each hull to every other hull or retrying rejected candidates.
    let mut previous: Option<&mut LiquidSurface> = None;
    let mut right = f64::NEG_INFINITY;
    for surface in &mut surfaces {
        if surface.left[0] < right {
            surface.controls = None;
            if let Some(previous) = previous {
                previous.controls = None;
            }
        }
        right = right.max(surface.right[0]);
        previous = Some(surface);
    }
    let hulls: Vec<Vec<Xy>> = surfaces
        .iter()
        .filter_map(|s| s.controls.map(|[c1, c2]| vec![s.left, c1, c2, s.right]))
        .collect();
    if !hulls.is_empty() {
        // A Bezier stays inside its full control hull. Difference catches holes
        // and narrow walls that testing only the four control points misses.
        let outside = FloatOverlay::with_subj_and_clip(&hulls, region)
            .overlay(OverlayRule::Difference, FillRule::EvenOdd);
        let mut intervals: Vec<_> = outside
            .iter()
            .filter_map(|shape| shape.first())
            .map(|outer| {
                outer
                    .iter()
                    .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, r), p| {
                        (l.min(p[0]), r.max(p[0]))
                    })
            })
            .collect();
        intervals.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut intervals = intervals.into_iter().peekable();
        for surface in &mut surfaces {
            while intervals.peek().is_some_and(|&(_, r)| r <= surface.left[0]) {
                intervals.next();
            }
            if intervals.peek().is_some_and(|&(l, _)| l < surface.right[0]) {
                surface.controls = None;
            }
        }
    }
    surfaces.sort_by_key(|s| s.order);
    surfaces
}

/// The transform that seats `moving` (a joint of a piece drawn at `scale`
/// world units per mm) on a fixed joint at `at`/`dir` in world space: the
/// axes oppose, and a male shoulder sits 0.06 L outside the female mouth.
pub fn mating_transform(
    at: Xy,
    dir: Xy,
    exposure_world: f64,
    moving_at: Xy,
    moving_dir: Xy,
    scale: f64,
) -> Affine {
    let target = add(at, mul(normalize(dir), exposure_world));
    let want = mul(normalize(dir), -1.);
    let have = normalize(moving_dir);
    let angle = (want[1].atan2(want[0]) - have[1].atan2(have[0])).to_degrees();
    let r = Affine::rotate(angle, [0., 0.]).then(Affine::scale(scale, [0., 0.]));
    let q = r.apply(moving_at);
    Affine::translate(sub(target, q)).then(r)
}
