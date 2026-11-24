//! Per-instance circular bends in local millimetres. One centre line supplies
//! the artwork, terminal tangents and physical cut length.
use super::eval::{joints, resolve};
use super::format::{Definition, Gender, Joint, PlugFamily, TubeMaterial, Xy};
use super::geometry::{Affine, add, dot, len, mul, normalize, sub};
use super::parts_list;
use super::solve::{self, JointRef};
use crate::document::{Document, Point};
use crate::graphics::Graphic;
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

const MAX_BENDS: usize = 32;
const EPS: f64 = 1e-6;
const MAX_COORDINATE: f64 = 1e5;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Route {
    pub start: Xy,
    pub bends: Vec<Bend>,
    pub end: Xy,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bend {
    /// Intersection of the two tangent lines, before trimming the corner.
    pub corner: Xy,
    pub radius_mm: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Segment {
    Straight {
        from: Xy,
        to: Xy,
    },
    Arc {
        center: Xy,
        from: Xy,
        sweep_rad: f64,
    },
}

#[derive(Debug)]
pub struct Centerline {
    pub segments: Vec<Segment>,
    pub length_mm: f64,
}

struct Fillet {
    start: Xy,
    end: Xy,
    center: Xy,
    sweep: f64,
}

impl Route {
    fn vertices(&self) -> Vec<Xy> {
        std::iter::once(self.start)
            .chain(self.bends.iter().map(|bend| bend.corner))
            .chain(std::iter::once(self.end))
            .collect()
    }

    fn reverse(&mut self) {
        std::mem::swap(&mut self.start, &mut self.end);
        self.bends.reverse();
    }

    pub fn end_poses(&self) -> Option<[(Xy, Xy); 2]> {
        let first = self.bends.first().map_or(self.end, |bend| bend.corner);
        let last = self.bends.last().map_or(self.start, |bend| bend.corner);
        let (a, b) = (sub(self.start, first), sub(self.end, last));
        (len(a) > EPS && len(b) > EPS)
            .then(|| [(self.start, normalize(a)), (self.end, normalize(b))])
    }

    pub fn validate(&self, od_mm: f64) -> Result<(), String> {
        self.checked_centerline(od_mm).map(|_| ())
    }

    fn checked_centerline(&self, od_mm: f64) -> Result<Centerline, String> {
        if self.bends.is_empty() {
            return Err("A custom tube route needs a bend.".into());
        }
        if self.bends.iter().any(|bend| bend.radius_mm <= od_mm / 2.) {
            return Err(format!(
                "Bend radius must be greater than half the tube diameter ({:.3} mm).",
                od_mm / 2.
            ));
        }
        let centerline = self.centerline()?;
        if crosses_itself(&centerline.segments, od_mm) {
            return Err("This bend overlaps another part of the tube.".into());
        }
        Ok(centerline)
    }

    pub fn centerline(&self) -> Result<Centerline, String> {
        if self.bends.len() > MAX_BENDS {
            return Err(format!("A tube can have at most {MAX_BENDS} bends."));
        }
        let vertices = self.vertices();
        if vertices
            .iter()
            .flatten()
            .any(|x| x.abs() > MAX_COORDINATE || !x.is_finite())
        {
            return Err("The tube route lies outside the drawing limits.".into());
        }
        let fillets = (vertices.windows(3).zip(&self.bends))
            .map(|(points, bend)| {
                let [a, corner, b] = points else {
                    return Err("The bend needs two straight legs.".into());
                };
                fillet(*a, *corner, *b, bend.radius_mm)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut start = self.start;
        let mut segments = Vec::with_capacity(2 * self.bends.len() + 1);
        let mut length_mm = 0.;
        for ((from, to), next) in vertices
            .windows(2)
            .filter_map(|pair| Some((*pair.first()?, *pair.last()?)))
            .zip(fillets.iter().map(Some).chain(std::iter::once(None)))
        {
            let end = next.map_or(self.end, |f| f.start);
            let straight = dot(sub(end, start), normalize(sub(to, from)));
            if !straight.is_finite() || straight <= EPS {
                return Err("The bend does not fit. Reduce the radius or angle, or choose a point farther from the tube end or adjacent bend.".into());
            }
            segments.push(Segment::Straight {
                from: start,
                to: end,
            });
            length_mm += straight;
            if let Some(f) = next {
                segments.push(Segment::Arc {
                    center: f.center,
                    from: f.start,
                    sweep_rad: f.sweep,
                });
                length_mm += len(sub(f.start, f.center)) * f.sweep.abs();
                start = f.end;
            }
        }
        Ok(Centerline {
            segments,
            length_mm,
        })
    }
}

fn cross(a: Xy, b: Xy) -> f64 {
    let ([ax, ay], [bx, by]) = (a, b);
    ax * by - ay * bx
}

fn turn(a: Xy, b: Xy) -> f64 {
    cross(a, b).atan2(dot(a, b))
}

fn rotate([x, y]: Xy, angle: f64) -> Xy {
    let (s, c) = angle.sin_cos();
    [x * c - y * s, x * s + y * c]
}

fn fillet(a: Xy, corner: Xy, b: Xy, radius: f64) -> Result<Fillet, String> {
    let (incoming, outgoing) = (sub(corner, a), sub(b, corner));
    if len(incoming) <= EPS || len(outgoing) <= EPS {
        return Err("The bend needs two straight legs.".into());
    }
    let (u, v) = (normalize(incoming), normalize(outgoing));
    let sweep = turn(u, v);
    if !(EPS..PI - EPS).contains(&sweep.abs()) {
        return Err("Enter a bend angle greater than 0° and less than 180°.".into());
    }
    if !radius.is_finite() || radius <= EPS || radius > MAX_COORDINATE {
        return Err("Enter a positive bend radius within the drawing limits.".into());
    }
    let trim = radius * (sweep.abs() / 2.).tan();
    let start = sub(corner, mul(u, trim));
    let end = add(corner, mul(v, trim));
    let center = add(start, mul([-u[1], u[0]], radius * sweep.signum()));
    Ok(Fillet {
        start,
        end,
        center,
        sweep,
    })
}

fn frame(g: &Graphic) -> Affine {
    let f = f64::from;
    Affine([
        f(g.axis_x.x),
        f(g.axis_x.y),
        f(g.axis_y.x),
        f(g.axis_y.y),
        f(g.origin.x),
        f(g.origin.y),
    ])
}

fn xy(point: Point) -> Xy {
    [f64::from(point.x), f64::from(point.y)]
}

fn point([x, y]: Xy) -> Point {
    Point::new(x as f32, y as f32)
}

fn supports_definition(d: &Definition) -> bool {
    d.family == "compression-tube"
        && d.tube.is_none()
        && parts_list::spec(d).is_some_and(|spec| {
            spec.material == TubeMaterial::StainlessSteel
                && spec.ends.iter().all(|id| {
                    d.joints.iter().any(|joint| {
                        joint.id() == id
                            && matches!(joint, Joint::Plug {
                        family: PlugFamily::Compression,
                        gender: Gender::M,
                        size_mm, ..
                    } if *size_mm == spec.od_mm)
                    })
                })
        })
}

pub fn supports(g: &Graphic) -> bool {
    g.piece
        .as_ref()
        .and_then(|p| p.linked.as_deref())
        .is_some_and(supports_definition)
}

pub(super) fn validate_piece(g: &Graphic, d: &Definition) -> Result<(), String> {
    let Some(route) = g.piece.as_ref().and_then(|piece| piece.tube_route.as_ref()) else {
        return Ok(());
    };
    if !supports_definition(d) {
        return Err(
            "Only stainless-steel compression tubing can store a custom tube route.".into(),
        );
    }
    route.validate(
        parts_list::spec(d)
            .ok_or("Tube dimensions are missing.")?
            .od_mm,
    )
}

fn initial_route(g: &Graphic, d: &Definition) -> Option<Route> {
    let piece = g.piece.as_ref()?;
    if let Some(route) = &piece.tube_route {
        return Some(route.clone());
    }
    let spec = parts_list::spec(d)?;
    let values = resolve(d, &piece.params).ok()?;
    let evaluated = joints(d, &values);
    let [a, b] = spec
        .ends
        .each_ref()
        .map(|id| evaluated.iter().find(|j| j.joint.id() == id));
    let (a, b) = (a?, b?);
    let u = mul(a.dir, -1.);
    let v = b.dir;
    let sweep = turn(u, v);
    let mut route = Route {
        start: a.at,
        bends: Vec::new(),
        end: b.at,
    };
    if sweep.abs() > EPS {
        // The preset bend has its centre-line arc length in TubeSpec.
        let corner = add(a.at, mul(u, cross(sub(b.at, a.at), v) / cross(u, v)));
        route.bends.push(Bend {
            corner,
            radius_mm: spec.length.fixed_mm / sweep.abs(),
        });
    }
    Some(route)
}

/// A draft never mutates the document. Applying it replaces one graphic and
/// retains the original seat records.
#[derive(Debug, Clone)]
pub struct Draft {
    original: Graphic,
    route: Route,
    reversed: bool,
    origin: Xy,
    direction: Xy,
    starting_leg: Option<[Point; 2]>,
    handedness: f64,
    target: Target,
    od_mm: f64,
    minimum_lead: [f64; 2],
    fixed_ends: usize,
    seated_hold: bool,
}

#[derive(Debug, Clone, Copy)]
enum Target {
    Insert(usize),
    Existing {
        index: usize,
        angle: f64,
        radius_mm: f64,
    },
}

impl Draft {
    pub fn new(doc: &Document, id: u64, world: Point) -> Result<Self, String> {
        let g = doc
            .graphics
            .iter()
            .find(|g| g.id == id)
            .ok_or("The tube is missing.")?;
        let d = g
            .piece
            .as_ref()
            .and_then(|p| p.linked.as_deref())
            .filter(|d| supports_definition(d))
            .ok_or("Select stainless-steel compression tubing.")?;
        let spec = parts_list::spec(d).ok_or("Tube dimensions are missing.")?;
        let mut route = initial_route(g, d).ok_or("The tube's current route could not be read.")?;
        let mut minimum_lead = [0.; 2];
        let mut fixed = [false; 2];
        let mut seated_hold = false;
        let scale = frame(g).scale_factor();
        for seat in &doc.seats {
            let (mine, other) = if seat.a.piece == id {
                (&seat.a, &seat.b)
            } else if seat.b.piece == id {
                (&seat.b, &seat.a)
            } else {
                continue;
            };
            if let Some(index) = spec.ends.iter().position(|joint| *joint == mine.joint) {
                if let (Some(locked), Some(lead)) =
                    (fixed.get_mut(index), minimum_lead.get_mut(index))
                {
                    *locked = true;
                    *lead = insertion_world(doc, other) / scale;
                }
            } else {
                seated_hold = true;
            }
        }
        let reversed = fixed[1] && !fixed[0];
        if reversed {
            route.reverse();
            minimum_lead.reverse();
        }
        let frame = frame(g);
        let local = frame
            .inverse()
            .ok_or("The tube has an invalid drawing scale.")?
            .apply(xy(world));
        let path = route.centerline()?;
        let (index, nearest) = path
            .segments
            .iter()
            .enumerate()
            .map(|(i, &s)| (i, closest(s, local)))
            .min_by(|(_, a), (_, b)| len(sub(*a, local)).total_cmp(&len(sub(*b, local))))
            .ok_or("The tube has no straight legs.")?;
        let vertices = route.vertices();
        let leg = index / 2;
        let before = *vertices.get(leg).ok_or("The bend location is missing.")?;
        let after = *vertices
            .get(leg + 1)
            .ok_or("The bend location is missing.")?;
        let (origin, target) = if index.is_multiple_of(2) {
            (nearest, Target::Insert(leg))
        } else {
            let bend = route.bends.get(leg).ok_or("The bend is missing.")?;
            let next = *vertices
                .get(leg + 2)
                .ok_or("The outgoing leg is missing.")?;
            (
                bend.corner,
                Target::Existing {
                    index: leg,
                    angle: turn(sub(after, before), sub(next, after)),
                    radius_mm: bend.radius_mm,
                },
            )
        };
        let starting_leg = match target {
            Target::Insert(_) => match path.segments.get(index) {
                Some(Segment::Straight { to, .. }) => Some([origin, *to]),
                _ => None,
            },
            Target::Existing { .. } => match path.segments.get(index + 1) {
                Some(Segment::Straight { from, to }) => Some([*from, *to]),
                _ => None,
            },
        }
        .map(|leg| leg.map(|at| point(frame.apply(at))));
        Ok(Self {
            original: g.clone(),
            route,
            reversed,
            origin,
            direction: normalize(sub(after, before)),
            starting_leg,
            target,
            od_mm: spec.od_mm,
            minimum_lead,
            handedness: if frame.mirrored() { -1. } else { 1. },
            fixed_ends: usize::from(fixed[0]) + usize::from(fixed[1]),
            seated_hold,
        })
    }

    pub fn is_existing_bend(&self) -> bool {
        matches!(self.target, Target::Existing { .. })
    }

    pub fn default_angle_deg(&self) -> f64 {
        match self.target {
            Target::Insert(_) => 0.,
            Target::Existing { angle, .. } => angle.to_degrees() * self.handedness,
        }
    }

    pub fn default_radius_mm(&self) -> f64 {
        match self.target {
            Target::Insert(_) => self.od_mm * 2.,
            Target::Existing { radius_mm, .. } => radius_mm,
        }
    }

    pub fn pivot(&self) -> Point {
        point(frame(&self.original).apply(self.origin))
    }

    /// The original outgoing leg, before the user rotates the new bend point.
    pub fn starting_leg(&self) -> Option<[Point; 2]> {
        self.starting_leg
    }

    pub fn blocked_edit(&self) -> Option<&'static str> {
        if self.seated_hold {
            Some("Release the tube from its clamp or through fitting before bending it.")
        } else if self.fixed_ends == 2 && !self.is_existing_bend() {
            Some("Both tube ends are seated. Detach one end to add a bend point.")
        } else {
            None
        }
    }

    pub fn can_rotate(&self) -> bool {
        self.fixed_ends < 2 && !self.seated_hold
    }

    /// The first straight beyond this bend, in canvas units. A drag anywhere
    /// on this leg adjusts the bend angle around its theoretical corner.
    pub fn outgoing_leg(&self, candidate: &Graphic) -> Option<[Point; 2]> {
        let definition = candidate.piece.as_ref()?.linked.as_deref()?;
        let mut route = initial_route(candidate, definition)?;
        if self.reversed {
            route.reverse();
        }
        let index = match self.target {
            Target::Insert(index) | Target::Existing { index, .. } => index,
        };
        let line = route.centerline().ok()?;
        let Segment::Straight { from, to } = *line.segments.get(2 * (index + 1))? else {
            return None;
        };
        let frame = frame(candidate);
        Some([point(frame.apply(from)), point(frame.apply(to))])
    }

    pub fn angle_to(&self, world: Point) -> Option<f64> {
        let local = frame(&self.original).inverse()?.apply(xy(world));
        let direction = sub(local, self.origin);
        (len(direction) > EPS)
            .then(|| turn(self.direction, direction).to_degrees() * self.handedness)
    }

    pub fn preview(&self, angle_deg: f64, radius_mm: f64) -> Result<Graphic, String> {
        if let Some(reason) = self.blocked_edit() {
            return Err(reason.into());
        }
        let angle = angle_deg.to_radians() * self.handedness;
        if !(EPS..PI - EPS).contains(&angle.abs()) {
            return Err("Enter a bend angle greater than 0° and less than 180°.".into());
        }
        let mut route = self.route.clone();
        let (index, delta) = match self.target {
            Target::Insert(index) => {
                if route.bends.len() >= MAX_BENDS {
                    return Err(format!("A tube can have at most {MAX_BENDS} bends."));
                }
                route.bends.insert(
                    index,
                    Bend {
                        corner: self.origin,
                        radius_mm,
                    },
                );
                (index, angle)
            }
            Target::Existing {
                index,
                angle: old,
                radius_mm: old_radius,
            } => {
                if (old - angle).abs() <= EPS && (old_radius - radius_mm).abs() <= EPS {
                    return Ok(self.original.clone());
                }
                if self.fixed_ends == 2 && (old - angle).abs() > EPS {
                    return Err("Both tube ends are seated. The radius can change while the angle stays fixed.".into());
                }
                route
                    .bends
                    .get_mut(index)
                    .ok_or("The bend is missing.")?
                    .radius_mm = radius_mm;
                (
                    index,
                    if (old - angle).abs() <= EPS {
                        0.
                    } else {
                        angle - old
                    },
                )
            }
        };
        let move_point = |p| add(self.origin, rotate(sub(p, self.origin), delta));
        if delta != 0. {
            for bend in route.bends.iter_mut().skip(index + 1) {
                bend.corner = move_point(bend.corner);
            }
            route.end = move_point(route.end);
        }
        let centerline = route.checked_centerline(self.od_mm)?;
        for (segment, lead) in [centerline.segments.first(), centerline.segments.last()]
            .into_iter()
            .zip(self.minimum_lead)
        {
            let Some(Segment::Straight { from, to }) = segment else {
                return Err("The bend needs a straight lead at both tube ends.".into());
            };
            if len(sub(*to, *from)) + EPS < lead {
                return Err("The bend needs a longer straight lead at the seated fitting.".into());
            }
        }
        if self.reversed {
            route.reverse();
        }
        let mut graphic = self.original.clone();
        graphic
            .piece
            .as_mut()
            .ok_or("The tube is missing.")?
            .tube_route = Some(route);
        Ok(graphic)
    }
}

fn insertion_world(doc: &Document, joint: &JointRef) -> f64 {
    doc.graphics
        .iter()
        .find(|g| g.id == joint.piece)
        .and_then(|g| solve::ports(g).into_iter().find(|p| p.at == *joint))
        .map_or(0., |p| p.insertion)
}

fn closest(segment: Segment, at: Xy) -> Xy {
    match segment {
        Segment::Straight { from, to } => {
            let v = sub(to, from);
            add(
                from,
                mul(v, (dot(v, sub(at, from)) / dot(v, v)).clamp(0., 1.)),
            )
        }
        Segment::Arc {
            center,
            from,
            sweep_rad,
        } => {
            let a = sub(from, center);
            let delta = turn(a, sub(at, center));
            if delta.signum() == sweep_rad.signum() && delta.abs() <= sweep_rad.abs() {
                return add(center, rotate(a, delta));
            }
            let to = add(center, rotate(a, sweep_rad));
            if len(sub(at, from)) < len(sub(at, to)) {
                from
            } else {
                to
            }
        }
    }
}

/// Check the same outer wall and cut rims that are drawn. Sorting the
/// flattened edges by their minimum coordinate limits each comparison to
/// potentially intersecting neighbors in the drawing, even for many bends.
fn crosses_itself(segments: &[Segment], diameter: f64) -> bool {
    if segments.len() < 5 {
        return false;
    }
    let (left, right) = super::plan::routed_sides(segments, diameter);
    let wall = super::plan::routed_band(&left, &right);
    let outline = super::geometry::flatten(&wall);
    for (points, closed) in outline {
        let mut edges: Vec<_> = super::geometry::edges(&points, closed)
            .enumerate()
            .filter(|(_, (a, b))| len(sub(*a, *b)) > EPS)
            .map(|(i, ([ax, ay], [bx, by]))| {
                (
                    (ax.min(bx), ax.max(bx), ay.min(by), ay.max(by)),
                    i,
                    [ax, ay],
                    [bx, by],
                )
            })
            .collect();
        edges.sort_unstable_by(|a, b| a.0.0.total_cmp(&b.0.0));
        for (k, &((_, xmax, ymin, ymax), i, a, b)) in edges.iter().enumerate() {
            for &((xmin, _, low, high), j, c, d) in edges.iter().skip(k + 1) {
                if xmin > xmax + EPS {
                    break;
                }
                let distance = i.abs_diff(j);
                if distance != 1
                    && distance + 1 != points.len()
                    && low <= ymax + EPS
                    && high + EPS >= ymin
                    && segments_cross(a, b, c, d)
                {
                    return true;
                }
            }
        }
    }
    false
}

fn segments_cross(a: Xy, b: Xy, c: Xy, d: Xy) -> bool {
    let (u, v, w) = (sub(b, a), sub(d, c), sub(c, a));
    let normal = cross(u, v);
    let tolerance = EPS * len(u) * len(v);
    if normal.abs() > tolerance {
        let on_segment = |t| (-EPS..=1. + EPS).contains(&t);
        return on_segment(cross(w, v) / normal) && on_segment(cross(w, u) / normal);
    }
    if cross(w, u).abs() > EPS * len(u) {
        return false;
    }
    let (p, q) = (dot(w, u), dot(sub(d, a), u));
    p.min(q) <= dot(u, u) && p.max(q) >= 0.
}
