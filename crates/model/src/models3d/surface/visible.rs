//! Screen-affine hidden-surface clipping and merged, nested shading regions.
//!
//! Full references: docs/references/xtb.md.
// Numeric loops index the mesh and parallel arrays under Input's contract.
#![allow(clippy::indexing_slicing)]

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};

use i_overlay::core::{fill_rule::FillRule, overlay::ShapeType, overlay_rule::OverlayRule};
use i_overlay::float::overlay::FloatOverlay;
use i_overlay::i_float::adapter::FloatPointAdapter;

mod coverage;
mod runs;

type Polygon = Vec<[f64; 2]>;
type Segment = ([f64; 2], [f64; 2]);

const AREA: f64 = 1e-9;
// Shallow folds should not dim a continuous silhouette rim.
const FOLD_DEPTH: f64 = 0.5;
// A fine common grid avoids slivers at T-junctions. Leave five f64 mantissa
// bits for roundoff when the same intersection is reached by different cuts.
const UNION_BITS: u32 = f64::MANTISSA_DIGITS - 5;
const SEGMENTS: usize = 64;
const RINGS: usize = 12;

/// Shadows of the ball discs and half-bond quads actually painted. Radii and
/// half widths are positive; cap and roof rises are nonnegative.
#[derive(Clone, Copy, Debug)]
pub enum Occluder {
    /// A front cap rising `depth` above `center[2]` over the painted disc.
    Ball {
        center: [f32; 3],
        radius: f32,
        depth: f32,
    },
    /// Axis endpoints, painted half widths, and roof rises above endpoint Z.
    /// The roof is the convex hull of the two endpoint half-hexagons.
    Stick {
        a: [f32; 3],
        b: [f32; 3],
        half: [f32; 2],
        depth: [f32; 2],
    },
}

/// Mesh indices must address `positions` and `lightness`; `positive` and
/// `front` must have one entry per triangle. Coordinates and shading are finite.
pub struct Input<'a> {
    pub positions: &'a [[f32; 3]],
    pub triangles: &'a [[u32; 3]],
    pub positive: &'a [bool],
    pub front: &'a [bool],
    /// Per-vertex shading coordinate, from darkest (0) to lightest (1).
    pub lightness: &'a [f32],
    pub occluders: &'a [Occluder],
    /// Tone counts, nearest group first. At least one group is required.
    /// A zero-tone group still has rims and mesh lines.
    pub tones: &'a [usize],
    pub lines: &'a [Line],
    pub tolerance: f32,
}

/// A straight segment on a mesh triangle, with endpoints in screen-depth space.
#[derive(Clone, Copy, Debug)]
pub struct Line {
    pub triangle: u32,
    pub ends: [[f32; 3]; 2],
}

#[derive(Clone, Debug)]
pub struct Layer {
    pub positive: bool,
    pub depth: usize,
    /// The region includes this tone and all lighter tones of this sign and depth.
    pub tone: usize,
    /// Implicitly closed contours, with holes wound opposite to their outer contour.
    /// Fill with the nonzero winding rule.
    pub region: Vec<Vec<[f32; 2]>>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub front: usize,
    pub pieces: usize,
    pub overlaps: usize,
    /// Convex fragments retained after counting coverage, before shading.
    pub fragments: usize,
    /// Pieces whose unresolved fragments were dropped at the work limit.
    pub fragment_limit_hits: usize,
    pub millis: f32,
}

#[derive(Clone, Debug, Default)]
pub struct Visible {
    /// Nearest depth first, then positive sign first and ascending occupied tone.
    pub layers: Vec<Layer>,
    /// Silhouette runs at each resolved depth.
    pub rims: Vec<Vec<Vec<[f32; 2]>>>,
    /// Mesh lines at each resolved depth, positive then negative.
    pub lines: Vec<[Vec<Vec<[f32; 2]>>; 2]>,
    pub stats: Stats,
}

#[derive(Clone, Copy)]
pub(super) struct Vertex {
    p: [f32; 3],
    // The incoming edge is a surviving mesh silhouette, rather than a cut.
    rim: bool,
    cut: usize,
}

struct Piece {
    triangle: usize,
    rims: Vec<usize>,
    polygon: Polygon,
    bounds: [f32; 4],
    z: Affine,
    light: Affine,
}

#[derive(Clone, Copy)]
struct Plane([f64; 4]);

impl Plane {
    fn at(self, p: [f32; 3]) -> f64 {
        self.0[0] * p[0] as f64 + self.0[1] * p[1] as f64 + self.0[2] * p[2] as f64 + self.0[3]
    }
}

struct Volume {
    planes: Vec<Plane>,
    bounds: [f32; 4],
    top: f32,
    ball: Option<([f32; 3], f32, f32)>,
}

impl Volume {
    fn new(occluder: Occluder) -> Option<Self> {
        match occluder {
            Occluder::Ball {
                center,
                radius,
                depth,
            } => {
                let mut planes = Vec::with_capacity(SEGMENTS * (RINGS + 1) + 1);
                let step = std::f64::consts::TAU / SEGMENTS as f64;
                let apothem = radius as f64 * (step * 0.5).cos();
                for i in 0..SEGMENTS {
                    let (s, c) = ((i as f64 + 0.5) * step).sin_cos();
                    planes.push(Plane([
                        c,
                        s,
                        0.,
                        -c * center[0] as f64 - s * center[1] as f64 - apothem,
                    ]));
                }
                // Twelve mid-latitude tangent rings circumscribe the cap. The
                // apex tangent also bounds it at the analytic maximum depth,
                // making the inexpensive max-Z rejection exact.
                planes.push(Plane([0., 0., 1., -center[2] as f64 - depth as f64]));
                if depth > 0. {
                    for ring in 0..RINGS {
                        let (r, z) = ((ring as f64 + 0.5) * std::f64::consts::FRAC_PI_2
                            / RINGS as f64)
                            .sin_cos();
                        for i in 0..SEGMENTS {
                            let (s, c) = ((i as f64 + 0.5) * step).sin_cos();
                            let nx = r * c / radius as f64;
                            let ny = r * s / radius as f64;
                            let nz = z / depth as f64;
                            planes.push(Plane([
                                nx,
                                ny,
                                nz,
                                -1. - nx * center[0] as f64
                                    - ny * center[1] as f64
                                    - nz * center[2] as f64,
                            ]));
                        }
                    }
                }
                Some(Self {
                    planes,
                    bounds: [
                        center[0] - radius,
                        center[1] - radius,
                        center[0] + radius,
                        center[1] + radius,
                    ],
                    top: center[2] + depth,
                    ball: Some((center, radius, depth)),
                })
            }
            Occluder::Stick { a, b, half, depth } => {
                let dx = b[0] as f64 - a[0] as f64;
                let dy = b[1] as f64 - a[1] as f64;
                let length = dx.hypot(dy);
                if length <= 1e-12 {
                    return None;
                }
                let side = [-dy / length, dx / length];
                let profile: [[f64; 2]; 7] = std::array::from_fn(|j| {
                    let (s, c) = (j as f64 * std::f64::consts::PI / 6.).sin_cos();
                    [c, s]
                });
                let mut points = Vec::with_capacity(4);
                for (p, h) in [(a, half[0]), (b, half[1])] {
                    for sign in [1., -1.] {
                        points.push([
                            p[0] as f64 + sign * side[0] * h as f64,
                            p[1] as f64 + sign * side[1] * h as f64,
                        ]);
                    }
                }
                let quad = [points[0], points[1], points[3], points[2]];
                let mut planes = Vec::with_capacity(16);
                for i in 0..4 {
                    let p = quad[i];
                    let q = quad[(i + 1) % 4];
                    let nx = q[1] - p[1];
                    let ny = p[0] - q[0];
                    planes.push(Plane([nx, ny, 0., -nx * p[0] - ny * p[1]]));
                }
                // Cross-sections of the hull are Minkowski sums of the two
                // endpoint profiles: their chord slopes give every upper face.
                // Proportional taper has six slopes; otherwise there are twelve.
                let mut slopes = Vec::with_capacity(12);
                for (h, d) in half.into_iter().zip(depth) {
                    for edge in profile.windows(2) {
                        let m = d as f64 / h as f64 * (edge[1][1] - edge[0][1])
                            / (edge[1][0] - edge[0][0]);
                        if slopes.contains(&m) {
                            continue;
                        }
                        slopes.push(m);
                        let support: [f64; 2] = std::array::from_fn(|i| {
                            profile
                                .iter()
                                .map(|p| depth[i] as f64 * p[1] - m * half[i] as f64 * p[0])
                                .fold(f64::NEG_INFINITY, f64::max)
                        });
                        let along = (b[2] as f64 - a[2] as f64 + support[1] - support[0])
                            / (length * length);
                        let nx = -along * dx - m * side[0];
                        let ny = -along * dy - m * side[1];
                        planes.push(Plane([
                            nx,
                            ny,
                            1.,
                            -a[2] as f64 - support[0] - nx * a[0] as f64 - ny * a[1] as f64,
                        ]));
                    }
                }
                let bounds = points.iter().fold(
                    [
                        f32::INFINITY,
                        f32::INFINITY,
                        f32::NEG_INFINITY,
                        f32::NEG_INFINITY,
                    ],
                    |mut b, p| {
                        for c in 0..2 {
                            b[c] = b[c].min(p[c] as f32);
                            b[c + 2] = b[c + 2].max(p[c] as f32);
                        }
                        b
                    },
                );
                Some(Self {
                    planes,
                    bounds,
                    top: (a[2] + depth[0]).max(b[2] + depth[1]),
                    ball: None,
                })
            }
        }
    }

    fn clip(&self, vertices: Vec<Vertex>, out: &mut Vec<Vec<Vertex>>) {
        let b = vertex_bounds(&vertices);
        if !boxes_overlap(b, self.bounds) || vertices.iter().all(|v| v.p[2] > self.top) {
            out.push(vertices);
            return;
        }
        // Most atom-covered triangles are wholly inside the true cap. Its
        // convexity proves that none of the hundreds of proxy planes can cut it.
        if let Some((c, radius, depth)) = self.ball {
            let inner = radius as f64 * (std::f64::consts::PI / SEGMENTS as f64).cos();
            if vertices.iter().all(|v| {
                let x = (v.p[0] - c[0]) as f64;
                let y = (v.p[1] - c[1]) as f64;
                let rr = x * x + y * y;
                rr <= inner * inner
                    && (v.p[2] - c[2]) as f64
                        <= depth as f64 * (1. - rr / (radius as f64 * radius as f64)).max(0.).sqrt()
            }) {
                return;
            }
        }
        let original = vertices;
        let mut remaining = original.clone();
        for v in &mut remaining {
            v.cut = usize::MAX;
        }
        // Only the nearest outline azimuth can constrain a ring at a given XY.
        // A circle enclosing the XY box conservatively bounds those directions.
        let azimuths = self.ball.map(|(c, _, _)| {
            let x = (b[0] as f64 + b[2] as f64) * 0.5 - c[0] as f64;
            let y = (b[1] as f64 + b[3] as f64) * 0.5 - c[1] as f64;
            let radius =
                ((b[2] as f64 - b[0] as f64) * 0.5).hypot((b[3] as f64 - b[1] as f64) * 0.5);
            let distance = x.hypot(y);
            let angle = y.atan2(x);
            let half = if distance > radius {
                (radius / distance).asin() + std::f64::consts::PI / SEGMENTS as f64 + 1e-12
            } else {
                std::f64::consts::PI
            };
            std::array::from_fn::<_, SEGMENTS, _>(|i| {
                let phi = (i as f64 + 0.5) * std::f64::consts::TAU / SEGMENTS as f64;
                ((phi - angle + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU)
                    - std::f64::consts::PI)
                    .abs()
                    <= half
            })
        });
        // Start at the cap tangent facing this piece. This removes the large
        // outside portion at once, rather than emitting it as many thin wedges
        // while traversing unrelated rings from the pole to the equator.
        let first = self.ball.filter(|(_, _, d)| *d > 0.).map(|(c, r, d)| {
            let center = remaining
                .iter()
                .fold([0.; 3], |mut p, v| {
                    for (sum, coordinate) in p.iter_mut().zip(v.p) {
                        *sum += coordinate as f64;
                    }
                    p
                })
                .map(|p| p / remaining.len() as f64);
            let x = (center[0] - c[0] as f64) / r as f64;
            let y = (center[1] - c[1] as f64) / r as f64;
            let z = (center[2] - c[2] as f64) / d as f64;
            let ring = ((x.hypot(y).atan2(z) * RINGS as f64 / std::f64::consts::FRAC_PI_2 - 0.5)
                .round() as isize)
                .clamp(0, RINGS as isize - 1) as usize;
            let azimuth = ((y.atan2(x) * SEGMENTS as f64 / std::f64::consts::TAU - 0.5).round()
                as isize)
                .rem_euclid(SEGMENTS as isize) as usize;
            SEGMENTS + 1 + ring * SEGMENTS + azimuth
        });
        for index in first.into_iter().chain((0..self.planes.len()).filter(|&i| {
            Some(i) != first
                && azimuths.as_ref().is_none_or(|selected| {
                    i == SEGMENTS || selected[if i < SEGMENTS { i } else { (i - 1) % SEGMENTS }]
                })
        })) {
            let plane = self.planes[index];
            let (inside, _) = split::<false>(remaining, plane, index);
            if inside.len() < 3 {
                out.push(original);
                return;
            }
            remaining = inside;
        }
        if !valid_vertices(&remaining) {
            out.push(original);
            return;
        }
        // Only planes supporting the actual hidden polygon are necessary to
        // subtract it. Visiting inactive tangents when emitting outsides would
        // partition a large visible face into hundreds of redundant wedges.
        let mut active = Vec::new();
        for v in remaining {
            if v.cut != usize::MAX && !active.contains(&v.cut) {
                active.push(v.cut);
            }
        }
        let mut remaining = original;
        for index in active {
            let (inside, outside) = split::<true>(remaining, self.planes[index], index);
            if valid_vertices(&outside) {
                out.push(outside);
            }
            if !valid_vertices(&inside) {
                return;
            }
            remaining = inside;
        }
    }
}

// The hidden-polygon probe needs only the inside, without outside allocation.
fn split<const BOTH: bool>(
    vertices: Vec<Vertex>,
    plane: Plane,
    cut_id: usize,
) -> (Vec<Vertex>, Vec<Vertex>) {
    let mut below = false;
    let mut above = false;
    for v in &vertices {
        let value = plane.at(v.p);
        below |= value < 0.;
        above |= value > 0.;
    }
    if !above {
        return (vertices, Vec::new());
    }
    if !below {
        return (Vec::new(), if BOTH { vertices } else { Vec::new() });
    }
    let mut inside = Vec::with_capacity(vertices.len() + 1);
    let mut outside = Vec::with_capacity(if BOTH { vertices.len() + 1 } else { 0 });
    let mut previous = vertices.len() - 1;
    for current in 0..vertices.len() {
        let a = vertices[previous];
        let b = vertices[current];
        let da = plane.at(a.p);
        let db = plane.at(b.p);
        if (da < 0. && db > 0.) || (da > 0. && db < 0.) {
            let t = da / (da - db);
            let cut = Vertex {
                p: std::array::from_fn(|i| {
                    (a.p[i] as f64 + t * (b.p[i] as f64 - a.p[i] as f64)) as f32
                }),
                rim: b.rim,
                cut: b.cut,
            };
            let boundary = Vertex {
                rim: false,
                cut: cut_id,
                ..cut
            };
            inside.push(if da < 0. { cut } else { boundary });
            if BOTH {
                outside.push(if da < 0. { boundary } else { cut });
            }
        }
        if db <= 0. {
            inside.push(Vertex {
                rim: b.rim && !(db == 0. && da > 0.),
                cut: if db == 0. && da > 0. { cut_id } else { b.cut },
                ..b
            });
        }
        if BOTH && db >= 0. {
            outside.push(Vertex {
                rim: b.rim && !(db == 0. && da < 0.),
                cut: if db == 0. && da < 0. { cut_id } else { b.cut },
                ..b
            });
        }
        previous = current;
    }
    dedup_vertices(&mut inside);
    if BOTH {
        dedup_vertices(&mut outside);
    }
    (inside, outside)
}

fn dedup_vertices(vertices: &mut Vec<Vertex>) {
    // An intersection can round onto its neighbour in f32. Keep the first
    // vertex: its incoming edge precedes the collapsed, zero-length edge.
    vertices.dedup_by(|a, b| a.p[..2] == b.p[..2]);
    if vertices.len() > 1 && vertices[0].p[..2] == vertices[vertices.len() - 1].p[..2] {
        let last = vertices[vertices.len() - 1];
        vertices[0].rim = last.rim;
        vertices[0].cut = last.cut;
        vertices.pop();
    }
}

fn vertex_bounds(v: &[Vertex]) -> [f32; 4] {
    v.iter().fold(
        [
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ],
        |mut b, v| {
            for c in 0..2 {
                b[c] = b[c].min(v.p[c]);
                b[c + 2] = b[c + 2].max(v.p[c]);
            }
            b
        },
    )
}

fn boxes_overlap(a: [f32; 4], b: [f32; 4]) -> bool {
    a[0] <= b[2] && b[0] <= a[2] && a[1] <= b[3] && b[1] <= a[3]
}

fn valid_vertices(v: &[Vertex]) -> bool {
    if v.len() < 3 {
        return false;
    }
    let origin = v[0].p;
    (0..v.len())
        .map(|i| {
            let a = v[i].p;
            let b = v[(i + 1) % v.len()].p;
            cross(
                [
                    a[0] as f64 - origin[0] as f64,
                    a[1] as f64 - origin[1] as f64,
                ],
                [
                    b[0] as f64 - origin[0] as f64,
                    b[1] as f64 - origin[1] as f64,
                ],
            )
        })
        .sum::<f64>()
        .abs()
        > 2. * AREA
}

#[derive(Clone, Copy)]
struct Affine {
    anchor: [f64; 3],
    gradient: [f64; 2],
}

fn affine(p: [[f32; 3]; 3], value: [f32; 3]) -> Option<Affine> {
    let x = p[1][0] as f64 - p[0][0] as f64;
    let y = p[1][1] as f64 - p[0][1] as f64;
    let u = p[2][0] as f64 - p[0][0] as f64;
    let v = p[2][1] as f64 - p[0][1] as f64;
    let det = x * v - y * u;
    if det == 0. {
        return None;
    }
    let a =
        ((value[1] as f64 - value[0] as f64) * v - (value[2] as f64 - value[0] as f64) * y) / det;
    let b =
        (x * (value[2] as f64 - value[0] as f64) - u * (value[1] as f64 - value[0] as f64)) / det;
    Some(Affine {
        anchor: [p[0][0], p[0][1], value[0]].map(f64::from),
        gradient: [a, b],
    })
}

impl Affine {
    fn at(self, p: [f64; 2]) -> f64 {
        let [x, y, value] = self.anchor;
        self.gradient[0] * (p[0] - x) + self.gradient[1] * (p[1] - y) + value
    }
}

fn area(contour: &[[f64; 2]]) -> f64 {
    let Some(&origin) = contour.first() else {
        return 0.;
    };
    (0..contour.len())
        .map(|i| {
            let a = contour[i];
            let b = contour[(i + 1) % contour.len()];
            cross(
                [a[0] - origin[0], a[1] - origin[1]],
                [b[0] - origin[0], b[1] - origin[1]],
            )
        })
        .sum::<f64>()
        * 0.5
}

fn cross(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

fn edge_distance(p: [f64; 2], q: [f64; 2], v: [f64; 2]) -> f64 {
    (q[1] - p[1]) * (v[0] - p[0]) - (q[0] - p[0]) * (v[1] - p[1])
}

// Sutherland–Hodgman half-plane clipping [Sutherland1974Clipping], keeping both halves.
// Both halves use the very same intersection vertex.
fn split_polygon(polygon: Polygon, distance: impl Fn([f64; 2]) -> f64) -> (Polygon, Polygon) {
    if polygon.iter().all(|&v| distance(v) <= 0.) {
        return (polygon, Vec::new());
    }
    if polygon.iter().all(|&v| distance(v) >= 0.) {
        return (Vec::new(), polygon);
    }
    let mut inside = Vec::with_capacity(polygon.len() + 1);
    let mut outside = Vec::with_capacity(polygon.len() + 1);
    let mut a = polygon[polygon.len() - 1];
    let mut da = distance(a);
    for b in polygon {
        let db = distance(b);
        if (da < 0. && db > 0.) || (da > 0. && db < 0.) {
            let v = along(a, b, da / (da - db));
            inside.push(v);
            outside.push(v);
        }
        if db <= 0. {
            inside.push(b);
        }
        if db >= 0. {
            outside.push(b);
        }
        a = b;
        da = db;
    }
    (inside, outside)
}

pub(super) fn intersection_centroid(
    a: &[[f64; 2]],
    b: &[[f64; 2]],
    scratch: &mut [Vec<[f64; 2]>; 2],
) -> Option<([f64; 2], f64)> {
    for (contour, other) in [(a, b), (b, a)] {
        for i in 0..contour.len() {
            let p = contour[i];
            let q = contour[(i + 1) % contour.len()];
            if p == q {
                continue;
            }
            if other.iter().all(|&v| edge_distance(p, q, v) >= 0.) {
                return None;
            }
        }
    }
    let [overlap, buffer] = scratch;
    overlap.clear();
    overlap.extend_from_slice(a);
    for i in 0..b.len() {
        let p = b[i];
        let q = b[(i + 1) % b.len()];
        let distance = |v| edge_distance(p, q, v);
        buffer.clear();
        let mut previous = overlap[overlap.len() - 1];
        let mut previous_distance = distance(previous);
        for &current in overlap.iter() {
            let distance = distance(current);
            if (previous_distance < 0. && distance > 0.)
                || (previous_distance > 0. && distance < 0.)
            {
                buffer.push(along(
                    previous,
                    current,
                    previous_distance / (previous_distance - distance),
                ));
            }
            if distance <= 0. {
                buffer.push(current);
            }
            previous = current;
            previous_distance = distance;
        }
        std::mem::swap(overlap, buffer);
        if overlap.len() < 3 {
            return None;
        }
    }
    let mut twice_area = 0.;
    let mut center = [0.; 2];
    let origin = overlap[0];
    for i in 0..overlap.len() {
        let p = [overlap[i][0] - origin[0], overlap[i][1] - origin[1]];
        let next = overlap[(i + 1) % overlap.len()];
        let q = [next[0] - origin[0], next[1] - origin[1]];
        let w = cross(p, q);
        twice_area += w;
        for c in 0..2 {
            center[c] += (p[c] + q[c]) * w;
        }
    }
    (twice_area > 2. * AREA).then(|| {
        (
            [
                origin[0] + center[0] / (3. * twice_area),
                origin[1] + center[1] / (3. * twice_area),
            ],
            twice_area * 0.5,
        )
    })
}

struct Buckets {
    bounds: [f32; 4],
    width: usize,
    height: usize,
    cells: Vec<Vec<usize>>,
}

impl Buckets {
    fn new(pieces: &[Piece]) -> Self {
        let bounds = pieces.iter().fold(
            [
                f32::INFINITY,
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
            ],
            |mut b, p| {
                for c in 0..2 {
                    b[c] = b[c].min(p.bounds[c]);
                    b[c + 2] = b[c + 2].max(p.bounds[c + 2]);
                }
                b
            },
        );
        let aspect =
            ((bounds[2] - bounds[0]) as f64 / (bounds[3] - bounds[1]) as f64).clamp(1e-3, 1e3);
        let count = (pieces.len() / 4).max(1) as f64;
        let width = (count * aspect).sqrt().ceil().clamp(1., 512.) as usize;
        let height = (count / width as f64).ceil().clamp(1., 512.) as usize;
        let mut grid = Self {
            bounds,
            width,
            height,
            cells: vec![Vec::new(); width * height],
        };
        for (i, p) in pieces.iter().enumerate() {
            let [x0, y0, x1, y1] = grid.range(p.bounds);
            for y in y0..=y1 {
                for x in x0..=x1 {
                    grid.cells[y * width + x].push(i);
                }
            }
        }
        grid
    }

    fn range(&self, b: [f32; 4]) -> [usize; 4] {
        let x = |v: f32| {
            (((v - self.bounds[0]) as f64 / (self.bounds[2] - self.bounds[0]).max(1e-12) as f64
                * self.width as f64)
                .floor()
                .max(0.) as usize)
                .min(self.width - 1)
        };
        let y = |v: f32| {
            (((v - self.bounds[1]) as f64 / (self.bounds[3] - self.bounds[1]).max(1e-12) as f64
                * self.height as f64)
                .floor()
                .max(0.) as usize)
                .min(self.height - 1)
        };
        [x(b[0]), y(b[1]), x(b[2]), y(b[3])]
    }

    fn candidates(&self, b: [f32; 4], seen: &mut [usize], stamp: usize, out: &mut Vec<usize>) {
        out.clear();
        let [x0, y0, x1, y1] = self.range(b);
        for y in y0..=y1 {
            for x in x0..=x1 {
                for &i in &self.cells[y * self.width + x] {
                    if seen[i] != stamp {
                        seen[i] = stamp;
                        out.push(i);
                    }
                }
            }
        }
    }
}

// Each batch owns its scratch buffers; the browser path stays sequential.
pub(super) fn batches<T: Send>(
    len: usize,
    minimum: usize,
    make: impl Fn(std::ops::Range<usize>) -> T + Sync,
) -> Vec<T> {
    let workers = if cfg!(target_arch = "wasm32") {
        1
    } else {
        std::thread::available_parallelism()
            .map_or(1, usize::from)
            .min((len / minimum).max(1))
    };
    if workers == 1 {
        return vec![make(0..len)];
    }
    let size = len.div_ceil(workers);
    let mut output: Vec<Option<T>> = (0..workers).map(|_| None).collect();
    std::thread::scope(|scope| {
        for (i, slot) in output.iter_mut().enumerate() {
            let make = &make;
            // Rounding the size up can leave the last workers nothing to do.
            let range = (i * size).min(len)..((i + 1) * size).min(len);
            scope.spawn(move || *slot = Some(make(range)));
        }
    });
    output.into_iter().flatten().collect()
}

/// Keep workers busy when independent contours vary widely in size.
pub(super) fn ordered_jobs<T: Send, Scratch>(
    len: usize,
    minimum: usize,
    scratch: impl Fn() -> Scratch + Sync,
    run: impl Fn(&mut Scratch, usize) -> T + Sync,
) -> Vec<T> {
    let next = AtomicUsize::new(0);
    let mut output: Vec<_> = batches(len, minimum, |range| {
        let mut scratch = scratch();
        let mut completed = Vec::with_capacity(range.len());
        loop {
            let i = next.fetch_add(1, Ordering::Relaxed);
            if i >= len {
                break;
            }
            completed.push((i, run(&mut scratch, i)));
        }
        completed
    })
    .into_iter()
    .flatten()
    .collect();
    output.sort_unstable_by_key(|&(i, _)| i);
    output.into_iter().map(|(_, value)| value).collect()
}

fn mesh_pieces(input: &Input<'_>, lines_by_triangle: &HashMap<usize, Vec<usize>>) -> Vec<Piece> {
    let mut back_edges = HashSet::new();
    for (i, triangle) in input.triangles.iter().enumerate() {
        if input.front[i] {
            continue;
        }
        for e in 0..3 {
            let a = triangle[e];
            let b = triangle[(e + 1) % 3];
            back_edges.insert((a.min(b), a.max(b)));
        }
    }
    let volumes: Vec<_> = input
        .occluders
        .iter()
        .copied()
        .filter_map(Volume::new)
        .collect();
    let mut pieces = Vec::new();
    let mut work = Vec::new();
    let mut next = Vec::new();
    for (i, triangle) in input.triangles.iter().enumerate() {
        if !input.front[i] && !lines_by_triangle.contains_key(&i) {
            continue;
        }
        let mut indices = *triangle;
        let p = triangle.map(|j| input.positions[j as usize]);
        let Some(z) = affine(p, p.map(|p| p[2])) else {
            continue;
        };
        let lights = triangle.map(|j| input.lightness[j as usize]);
        let Some(light) = affine(p, lights) else {
            continue;
        };
        if cross(
            [
                p[1][0] as f64 - p[0][0] as f64,
                p[1][1] as f64 - p[0][1] as f64,
            ],
            [
                p[2][0] as f64 - p[0][0] as f64,
                p[2][1] as f64 - p[0][1] as f64,
            ],
        ) < 0.
        {
            indices.swap(1, 2);
        }
        let vertices = (0..3)
            .map(|j| {
                let a = indices[(j + 2) % 3];
                let b = indices[j];
                Vertex {
                    p: input.positions[b as usize],
                    rim: input.front[i] && back_edges.contains(&(a.min(b), a.max(b))),
                    cut: usize::MAX,
                }
            })
            .collect();
        work.clear();
        work.push(vertices);
        for volume in &volumes {
            next.clear();
            for vertices in work.drain(..) {
                volume.clip(vertices, &mut next);
            }
            std::mem::swap(&mut work, &mut next);
            if work.is_empty() {
                break;
            }
        }
        for vertices in work.drain(..) {
            if valid_vertices(&vertices) {
                pieces.push(Piece {
                    triangle: i,
                    polygon: vertices
                        .iter()
                        .map(|v| [v.p[0] as f64, v.p[1] as f64])
                        .collect(),
                    bounds: vertex_bounds(&vertices),
                    rims: vertices
                        .iter()
                        .enumerate()
                        .filter_map(|(e, v)| v.rim.then_some(e))
                        .collect(),
                    z,
                    light,
                });
            }
        }
    }
    pieces
}

fn shade(contour: Polygon, light: Affine, tones: &mut [Vec<Polygon>]) {
    if tones.is_empty() {
        return;
    }
    let (min, max) = contour
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(min, max), &p| {
            let value = light.at(p);
            (min.min(value), max.max(value))
        });
    let count = tones.len();
    let tone_at = |v: f64| ((v * count as f64).floor() as usize).min(count - 1);
    let (first, last) = (tone_at(min), tone_at(max));
    if first == last {
        tones[first].push(contour);
        return;
    }
    let mut remaining = contour;
    for (tone, polygons) in tones.iter_mut().enumerate().take(last).skip(first) {
        let threshold = (tone + 1) as f64 / count as f64;
        let (cut, lighter) = split_polygon(remaining, |p| light.at(p) - threshold);
        if area(&cut) > AREA {
            polygons.push(cut);
        }
        remaining = lighter;
    }
    if area(&remaining) > AREA {
        tones[last].push(remaining);
    }
}

struct Batch {
    // One entry per (depth, sign), then one list of polygons per tone.
    shades: Vec<Vec<Vec<Polygon>>>,
    regions: Vec<Vec<Polygon>>,
    rims: Vec<Vec<Segment>>,
    lines: Vec<runs::LineInterval>,
    fragments: usize,
    limit_hits: usize,
}

impl Batch {
    fn new(tones: &[usize]) -> Self {
        Self {
            shades: tones
                .iter()
                .flat_map(|&n| [vec![Vec::new(); n], vec![Vec::new(); n]])
                .collect(),
            regions: vec![Vec::new(); 2 * tones.len()],
            rims: vec![Vec::new(); tones.len()],
            lines: Vec::new(),
            fragments: 0,
            limit_hits: 0,
        }
    }

    fn append(&mut self, other: Self) {
        for (tones, more) in self.shades.iter_mut().zip(other.shades) {
            for (target, polygons) in tones.iter_mut().zip(more) {
                target.extend(polygons);
            }
        }
        for (regions, more) in self.regions.iter_mut().zip(other.regions) {
            regions.extend(more);
        }
        for (rims, more) in self.rims.iter_mut().zip(other.rims) {
            rims.extend(more);
        }
        self.lines.extend(other.lines);
        self.fragments += other.fragments;
        self.limit_hits += other.limit_hits;
    }
}

fn merge_layers(
    shades: &[Vec<Vec<Polygon>>],
    regions: &[Vec<Polygon>],
    adapter: &FloatPointAdapter<[f64; 2], i64>,
    tolerance: f64,
) -> Vec<Layer> {
    const TONE_BLOCK_SIZE: usize = 12;

    let union_worker = || {
        // Keep each worker's overlay scratch on the same grid across unions.
        let mut scratch = None;
        move |lighter: &[Polygon], sources: &[Polygon]| {
            let mut overlay = scratch
                .take()
                .unwrap_or_else(|| FloatOverlay::<[f64; 2], i64>::with_adapter(adapter.clone(), 0))
                .unsafe_add_source(lighter, ShapeType::Subject)
                .unsafe_add_source(sources, ShapeType::Subject);
            let shapes = overlay.overlay(OverlayRule::Subject, FillRule::NonZero);
            overlay.clear();
            scratch = Some(overlay);
            shapes
                .into_iter()
                .flatten()
                .filter(|c| area(c).abs() > AREA)
                .collect::<Vec<Polygon>>()
        }
    };
    let mut jobs = Vec::new();
    let mut blocks = Vec::new();
    let mut block_groups = Vec::new();
    let mut block_for_tone = Vec::new();
    for (group, tones) in shades.iter().enumerate() {
        let mut occupied = tones.iter().enumerate().filter(|(_, p)| !p.is_empty());
        let Some((first, _)) = occupied.next() else {
            continue;
        };
        // Shade cuts change contour starts. Merge the whole region directly
        // so an opacity change cannot affect its fitted outline.
        jobs.push((group, first, regions[group].as_slice()));
        block_for_tone.push(None);
        let start = jobs.len();
        jobs.extend(occupied.map(|(tone, polygons)| (group, tone, polygons.as_slice())));
        let first_block = blocks.len();
        for start in (start..jobs.len()).step_by(TONE_BLOCK_SIZE) {
            let end = (start + TONE_BLOCK_SIZE).min(jobs.len());
            block_for_tone.resize(end, Some(blocks.len()));
            blocks.push(start..end);
        }
        if first_block < blocks.len() {
            block_groups.push(first_block..blocks.len());
        }
    }

    // Merge the fragments before nesting, so later unions work on contours.
    let mut nested = ordered_jobs(jobs.len(), 4, union_worker, |union, i| {
        union(&[], jobs[i].2)
    });

    for (i, contours) in ordered_jobs(blocks.len(), 1, union_worker, |union, block| {
        let block = &blocks[block];
        let last = block.end - 1;
        let mut lighter = &nested[last];
        let mut output = Vec::with_capacity(block.len() - 1);
        for i in (block.start..last).rev() {
            let index = output.len();
            output.push((i, union(lighter, &nested[i])));
            lighter = &output[index].1;
        }
        output
    })
    .into_iter()
    .flatten()
    {
        nested[i] = contours;
    }

    // Each suffix includes only the lighter blocks of the same depth and sign.
    let suffixes: Vec<_> = ordered_jobs(block_groups.len(), 1, union_worker, |union, group| {
        let group = &block_groups[group];
        let mut output = Vec::with_capacity(group.len());
        let mut lighter = Vec::new();
        for block in blocks[group.start + 1..group.end].iter().rev() {
            let contours = &nested[block.start];
            let next = if lighter.is_empty() {
                contours.clone()
            } else {
                union(&lighter, contours)
            };
            output.push(lighter);
            lighter = next;
        }
        output.push(lighter);
        output.reverse();
        output
    })
    .into_iter()
    .flatten()
    .collect();

    ordered_jobs(jobs.len(), 4, union_worker, |union, i| {
        let (group, tone, _) = jobs[i];
        let suffix = block_for_tone[i].map_or(&[][..], |block| suffixes[block].as_slice());
        let joined;
        let contours = if suffix.is_empty() {
            &nested[i]
        } else {
            joined = union(suffix, &nested[i]);
            &joined
        };
        // Thinning an outer and its holes independently can erase a thin ring.
        let tolerance = if contours.iter().any(|contour| area(contour) < 0.) {
            0.
        } else {
            tolerance
        };
        (!contours.is_empty()).then(|| Layer {
            positive: group % 2 == 0,
            depth: group / 2,
            tone,
            region: contours
                .iter()
                .map(|c| simplify_closed(c, tolerance))
                .collect(),
        })
    })
    .into_iter()
    .flatten()
    .collect()
}

/// Resolve coverage counts on the molecule-clipped pieces and their mesh lines.
pub fn visible(input: &Input<'_>) -> Visible {
    #[cfg(not(target_arch = "wasm32"))]
    let started = std::time::Instant::now();
    let groups = input.tones.len();
    assert!(groups > 0, "at least one depth group is required");
    let mut lines_by_triangle = HashMap::<usize, Vec<usize>>::new();
    for (id, line) in input.lines.iter().enumerate() {
        let triangle = line.triangle as usize;
        if groups > 1 || input.front[triangle] {
            lines_by_triangle.entry(triangle).or_default().push(id);
        }
    }
    let pieces = mesh_pieces(input, &lines_by_triangle);
    let mut result = Visible::default();
    result.stats.front = input.front.iter().filter(|&&front| front).count();
    result.stats.pieces = pieces.len();
    let mut all = Batch::new(input.tones);
    let tolerance = f64::from(input.tolerance);
    if !pieces.is_empty() {
        let grid = Buckets::new(&pieces);
        let nearer = coverage::nearer_pieces(&pieces, &grid, input.front);
        result.stats.overlaps = nearer.iter().map(Vec::len).sum();

        for batch in batches(pieces.len(), 512, |range| {
            let mut batch = Batch::new(input.tones);
            let mut seen = vec![usize::MAX; pieces.len()];
            let mut candidates = Vec::new();
            let mut stamp = 0;
            for i in range {
                let piece = &pieces[i];
                let front = input.front[piece.triangle];
                let sign = usize::from(!input.positive[piece.triangle]);
                let lines = lines_by_triangle.get(&piece.triangle);
                if groups > 1 || lines.is_some() || !coverage::fully_covered(i, &pieces, &nearer) {
                    let (kept, limited) = coverage::resolve(
                        piece,
                        &nearer[i],
                        &pieces,
                        groups,
                        lines.is_some(),
                        |polygon, depth| {
                            if let Some(lines) = lines {
                                for &id in lines {
                                    if let Some(interval) =
                                        runs::clip_line(&input.lines[id], &polygon)
                                    {
                                        batch.lines.push(runs::LineInterval {
                                            source: id,
                                            depth,
                                            interval,
                                        });
                                    }
                                }
                            }
                            if front && depth < groups {
                                if input.tones[depth] > 0 {
                                    batch.regions[2 * depth + sign].push(polygon.clone());
                                }
                                shade(polygon, piece.light, &mut batch.shades[2 * depth + sign]);
                            }
                        },
                    );
                    batch.fragments += kept;
                    batch.limit_hits += usize::from(limited);
                }
                // Area coverage cannot establish visibility on a silhouette edge.
                for &e in &piece.rims {
                    let a = piece.polygon[(e + piece.polygon.len() - 1) % piece.polygon.len()];
                    let b = piece.polygon[e];
                    let bounds = [
                        a[0].min(b[0]) as f32,
                        a[1].min(b[1]) as f32,
                        a[0].max(b[0]) as f32,
                        a[1].max(b[1]) as f32,
                    ];
                    grid.candidates(bounds, &mut seen, stamp, &mut candidates);
                    stamp += 1;
                    let covers = candidates.iter().filter_map(|&j| {
                        let other = &pieces[j];
                        if other.triangle == piece.triangle
                            || !input.front[other.triangle]
                            || !boxes_overlap(bounds, other.bounds)
                        {
                            return None;
                        }
                        let (lo, hi) = segment_interval(a, b, &other.polygon)?;
                        let midpoint = along(a, b, (lo + hi) * 0.5);
                        (other.z.at(midpoint) - piece.z.at(midpoint) > FOLD_DEPTH)
                            .then_some((other.triangle, (lo, hi)))
                    });
                    runs::rim_segments(a, b, covers, &mut batch.rims);
                }
            }
            batch
        }) {
            all.append(batch);
        }

        // A common integer grid keeps cuts on adjacent pieces together.
        let adapter = FloatPointAdapter::with_iter_and_coordinate_bits(
            pieces.iter().flat_map(|p| &p.polygon),
            UNION_BITS,
        );
        result.layers = merge_layers(&all.shades, &all.regions, &adapter, tolerance);
    }
    result.stats.fragments = all.fragments;
    result.stats.fragment_limit_hits = all.limit_hits;
    result.rims = all
        .rims
        .into_iter()
        .map(|rims| chain(rims, tolerance))
        .collect();
    result.lines = runs::chain_lines(all.lines, input);
    #[cfg(not(target_arch = "wasm32"))]
    {
        result.stats.millis = started.elapsed().as_secs_f32() * 1000.;
    }
    result
}

fn along(a: [f64; 2], b: [f64; 2], t: f64) -> [f64; 2] {
    [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])]
}

// Cyrus–Beck clipping of a segment to a counterclockwise convex contour [Cyrus1978Clipping].
pub(super) fn segment_interval(
    a: [f64; 2],
    b: [f64; 2],
    contour: &[[f64; 2]],
) -> Option<(f64, f64)> {
    let mut low: f64 = 0.;
    let mut high: f64 = 1.;
    for i in 0..contour.len() {
        let p = contour[i];
        let q = contour[(i + 1) % contour.len()];
        let distance = |v| edge_distance(p, q, v);
        let da = distance(a);
        let db = distance(b);
        let delta = db - da;
        if delta.abs() <= 1e-14 {
            if da > 1e-10 {
                return None;
            }
        } else if delta > 0. {
            high = high.min(-da / delta);
        } else {
            low = low.max(-da / delta);
        }
        if high <= low {
            return None;
        }
    }
    Some((low.max(0.), high.min(1.)))
}

// Douglas–Peucker simplification [Douglas1973Simplification].
fn simplify_open(points: &[[f64; 2]], tolerance: f64) -> Vec<[f64; 2]> {
    if points.len() <= 2 || tolerance <= 0. {
        return points.to_vec();
    }
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    let mut stack = vec![(0, points.len() - 1)];
    while let Some((first, last)) = stack.pop() {
        let a = points[first];
        let b = points[last];
        let d = [b[0] - a[0], b[1] - a[1]];
        let length = d[0] * d[0] + d[1] * d[1];
        let mut maximum = tolerance.max(0.).powi(2);
        let mut split = None;
        for (i, &p) in points.iter().enumerate().take(last).skip(first + 1) {
            let t = if length > 0. {
                (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / length).clamp(0., 1.)
            } else {
                0.
            };
            let q = along(a, b, t);
            let distance = (p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2);
            if distance > maximum {
                maximum = distance;
                split = Some(i);
            }
        }
        if let Some(i) = split {
            keep[i] = true;
            stack.push((first, i));
            stack.push((i, last));
        }
    }
    points
        .iter()
        .zip(keep)
        .filter_map(|(&p, keep)| keep.then_some(p))
        .collect()
}

pub(super) fn simplify_closed(points: &[[f64; 2]], tolerance: f64) -> Vec<[f32; 2]> {
    const MAX_THINNING_POINTS: usize = 2048;
    let original = || points.iter().map(|p| p.map(|x| x as f32)).collect();
    // Bound the cost of the quadratic intersection check below.
    if !(3..=MAX_THINNING_POINTS).contains(&points.len()) || tolerance <= 0. {
        return original();
    }
    let opposite = (1..points.len())
        .max_by(|&i, &j| {
            let distance =
                |p: [f64; 2]| (p[0] - points[0][0]).powi(2) + (p[1] - points[0][1]).powi(2);
            distance(points[i]).total_cmp(&distance(points[j]))
        })
        .unwrap_or(1);
    let mut thinned = simplify_open(&points[..=opposite], tolerance);
    let mut rest = points[opposite..].to_vec();
    rest.push(points[0]);
    let b = simplify_open(&rest, tolerance);
    thinned.pop();
    thinned.extend_from_slice(&b[..b.len() - 1]);
    if thinned.len() < 3 || thinned.len() == points.len() {
        return original();
    }
    // Check the coordinates that will actually be painted.
    for p in &mut thinned {
        *p = p.map(|v| f64::from(v as f32));
    }
    let original_area = area(points);
    if (area(&thinned) - original_area).abs() > 0.1 * original_area.abs() {
        return original();
    }
    let side = |a: [f64; 2], b: [f64; 2], p: [f64; 2]| {
        cross([b[0] - a[0], b[1] - a[1]], [p[0] - a[0], p[1] - a[1]])
    };
    let n = thinned.len();
    for i in 0..n {
        let a = thinned[i];
        let b = thinned[(i + 1) % n];
        // A contour's adjacent edges meet by construction, including at its start.
        for j in i + 2..n - usize::from(i == 0) {
            let c = thinned[j];
            let d = thinned[(j + 1) % n];
            if (0..2).all(|k| a[k].min(b[k]) <= c[k].max(d[k]) && c[k].min(d[k]) <= a[k].max(b[k]))
                && side(a, b, c) * side(a, b, d) <= 0.
                && side(c, d, a) * side(c, d, b) <= 0.
            {
                return original();
            }
        }
    }
    thinned.iter().map(|p| p.map(|x| x as f32)).collect()
}

fn chain(segments: Vec<([f64; 2], [f64; 2])>, tolerance: f64) -> Vec<Vec<[f32; 2]>> {
    let mut lookup = HashMap::<(i64, i64), usize>::new();
    let mut points = Vec::new();
    let mut edges = Vec::new();
    let mut adjacency = Vec::<Vec<usize>>::new();
    for (a, b) in segments {
        let mut endpoints = [0; 2];
        for (i, p) in [a, b].into_iter().enumerate() {
            let key = runs::endpoint_key(p);
            endpoints[i] = *lookup.entry(key).or_insert_with(|| {
                points.push([key.0 as f64 * 1e-4, key.1 as f64 * 1e-4]);
                adjacency.push(Vec::new());
                points.len() - 1
            });
        }
        if endpoints[0] != endpoints[1] {
            for &v in &endpoints {
                adjacency[v].push(edges.len());
            }
            edges.push(endpoints);
        }
    }
    let mut used = vec![false; edges.len()];
    let mut result = Vec::new();
    // Open runs first, then cycles. At a branch each unused edge starts a run.
    let starts: Vec<_> = (0..points.len())
        .filter(|&i| adjacency[i].len() != 2)
        .chain((0..points.len()).filter(|&i| adjacency[i].len() == 2))
        .collect();
    for start in starts {
        while let Some(&first) = adjacency[start].iter().find(|&&e| !used[e]) {
            let mut run = vec![points[start]];
            let mut current = start;
            let mut edge = first;
            loop {
                used[edge] = true;
                let pair = edges[edge];
                current = if pair[0] == current { pair[1] } else { pair[0] };
                run.push(points[current]);
                if current == start || adjacency[current].len() != 2 {
                    break;
                }
                let Some(&next) = adjacency[current].iter().find(|&&e| !used[e]) else {
                    break;
                };
                edge = next;
            }
            if run.len() > 3 && run.first() == run.last() {
                run.pop();
                let mut simple = simplify_closed(&run, tolerance);
                if simple.len() >= 3 {
                    simple.push(simple[0]);
                    result.push(simple);
                }
            } else {
                result.push(
                    simplify_open(&run, tolerance)
                        .into_iter()
                        .map(|p| p.map(|x| x as f32))
                        .collect(),
                );
            }
        }
    }
    result
}

/// Nearest front-facing mesh triangle containing the point, before clipping.
pub fn front_depth(input: &Input<'_>, point: [f32; 2]) -> Option<f32> {
    let point64 = point.map(|v| v as f64);
    let mut nearest: Option<f64> = None;
    for (i, triangle) in input.triangles.iter().enumerate() {
        if !input.front[i] {
            continue;
        }
        let p = triangle.map(|j| input.positions[j as usize]);
        let mut sign: f64 = 0.;
        let mut inside = true;
        for e in 0..3 {
            let a = p[e];
            let b = p[(e + 1) % 3];
            let side = cross(
                [b[0] as f64 - a[0] as f64, b[1] as f64 - a[1] as f64],
                [point64[0] - a[0] as f64, point64[1] - a[1] as f64],
            );
            if side != 0. {
                if sign != 0. && side.signum() != sign {
                    inside = false;
                    break;
                }
                sign = side.signum();
            }
        }
        if inside && let Some(plane) = affine(p, p.map(|v| v[2])) {
            let z = plane.at(point64);
            nearest = Some(nearest.map_or(z, |old| old.max(z)));
        }
    }
    nearest.map(|z| z as f32)
}
