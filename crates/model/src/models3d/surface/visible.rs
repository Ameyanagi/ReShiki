//! Screen-affine hidden-surface clipping and merged, nested shading regions.
// Numeric loops index the mesh and parallel arrays under Input's contract.
#![allow(clippy::indexing_slicing)]

use std::collections::{HashMap, HashSet};

use i_overlay::core::{fill_rule::FillRule, overlay::ShapeType, overlay_rule::OverlayRule};
use i_overlay::float::overlay::FloatOverlay;
use i_overlay::i_float::adapter::FloatPointAdapter;

const AREA: f64 = 1e-9;
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
    pub lightness: &'a [f32],
    pub occluders: &'a [Occluder],
    pub bands: usize,
    pub tolerance: f32,
}

/// The visible region of one sign in this band and all lighter bands.
#[derive(Clone, Debug)]
pub struct Layer {
    pub positive: bool,
    pub band: usize,
    /// Implicitly closed contours, including holes; fill with even-odd parity.
    pub region: Vec<Vec<[f32; 2]>>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub front: usize,
    pub pieces: usize,
    pub overlaps: usize,
    pub millis: f32,
}

#[derive(Clone, Debug, Default)]
pub struct Visible {
    /// Layers for each sign are in ascending band order, starting with its
    /// whole visible region. Empty bands are omitted.
    pub layers: Vec<Layer>,
    pub rims: Vec<Vec<[f32; 2]>>,
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
    vertices: Vec<Vertex>,
    bounds: [f32; 4],
    z: Affine,
    light: Affine,
    positive: bool,
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

fn half_plane(contour: &[[f64; 2]], distance: impl Fn([f64; 2]) -> f64) -> Vec<[f64; 2]> {
    let Some(&last) = contour.last() else {
        return Vec::new();
    };
    let mut result = Vec::with_capacity(contour.len() + 1);
    let mut a = last;
    let mut da = distance(a);
    for &b in contour {
        let db = distance(b);
        if (da < 0. && db > 0.) || (da > 0. && db < 0.) {
            let t = da / (da - db);
            result.push([a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])]);
        }
        if db <= 0. {
            result.push(b);
        }
        a = b;
        da = db;
    }
    result
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
            if other
                .iter()
                .all(|v| (q[0] - p[0]) * (v[1] - p[1]) - (q[1] - p[1]) * (v[0] - p[0]) <= 0.)
            {
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
        let distance = |v: [f64; 2]| (q[1] - p[1]) * (v[0] - p[0]) - (q[0] - p[0]) * (v[1] - p[1]);
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
fn batches<T: Send>(
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
            scope.spawn(move || *slot = Some(make(i * size..((i + 1) * size).min(len))));
        }
    });
    output.into_iter().flatten().collect()
}

/// Resolve front pieces independently, then merge their visible isobands.
pub fn visible(input: &Input<'_>) -> Visible {
    #[cfg(not(target_arch = "wasm32"))]
    let started = std::time::Instant::now();
    let mut result = Visible::default();
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
        if !input.front[i] {
            continue;
        }
        result.stats.front += 1;
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
                    rim: back_edges.contains(&(a.min(b), a.max(b))),
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
                    bounds: vertex_bounds(&vertices),
                    vertices,
                    z,
                    light,
                    positive: input.positive[i],
                });
            }
        }
    }
    result.stats.pieces = pieces.len();
    if !pieces.is_empty() {
        let grid = Buckets::new(&pieces);
        let polygons: Vec<Vec<_>> = pieces
            .iter()
            .map(|p| {
                p.vertices
                    .iter()
                    .map(|v| [f64::from(v.p[0]), f64::from(v.p[1])])
                    .collect()
            })
            .collect();
        let areas: Vec<_> = polygons.iter().map(|p| area(p)).collect();
        let in_front = |i: usize, j: usize, point| {
            let dz = pieces[i].z.at(point) - pieces[j].z.at(point);
            dz > 0. || (dz == 0. && i < j)
        };
        let mut nearer = vec![Vec::new(); pieces.len()];
        let mut covered = vec![false; pieces.len()];
        let comparisons = batches(pieces.len(), 512, |range| {
            let mut seen = vec![usize::MAX; pieces.len()];
            let mut candidates = Vec::new();
            let mut scratch = [Vec::with_capacity(16), Vec::with_capacity(16)];
            let mut comparisons = Vec::new();
            for i in range {
                let piece = &pieces[i];
                grid.candidates(piece.bounds, &mut seen, i, &mut candidates);
                for &j in &candidates {
                    if j <= i || !boxes_overlap(piece.bounds, pieces[j].bounds) {
                        continue;
                    }
                    let Some((center, overlap_area)) =
                        intersection_centroid(&polygons[i], &polygons[j], &mut scratch)
                    else {
                        continue;
                    };
                    let (back, front) = if in_front(j, i, center) {
                        (i, j)
                    } else {
                        (j, i)
                    };
                    comparisons.push((back, front, overlap_area));
                }
            }
            comparisons
        });
        for comparisons in comparisons {
            result.stats.overlaps += comparisons.len();
            for (back, front, overlap_area) in comparisons {
                nearer[back].push((front, overlap_area));
                covered[back] |= overlap_area >= areas[back] - AREA;
            }
        }
        // A front sheet usually covers a rear face with several adjacent,
        // disjoint triangles. Their intersection areas add without a Boolean
        // operation; overlapping candidates are conservatively left out.
        for i in 0..pieces.len() {
            if covered[i] {
                continue;
            }
            let mut disjoint: Vec<usize> = Vec::new();
            let mut total = 0.;
            for &(j, overlap_area) in &nearer[i] {
                if disjoint.iter().all(|&k| {
                    !nearer[j].iter().any(|&(n, _)| n == k)
                        && !nearer[k].iter().any(|&(n, _)| n == j)
                }) {
                    total += overlap_area;
                    if total >= areas[i] - AREA {
                        covered[i] = true;
                        break;
                    }
                    disjoint.push(j);
                }
            }
        }
        let bands = input.bands.max(1);
        // All differences and unions share the same integer grid. Recentring
        // and rescaling each small piece would introduce cracks at shared cuts.
        let adapter = FloatPointAdapter::with_iter_conservative(polygons.iter().flatten());
        let partial = batches(pieces.len(), 512, |range| {
            let mut raw = vec![vec![Vec::<Vec<[f64; 2]>>::new(); bands]; 2];
            let mut overlay = FloatOverlay::<[f64; 2]>::with_adapter(adapter.clone(), 0);
            let mut segments = Vec::new();
            let mut seen = vec![usize::MAX; pieces.len()];
            let mut candidates = Vec::new();
            let mut stamp = 0;
            for i in range {
                let piece = &pieces[i];
                let regions = if covered[i] {
                    Vec::new()
                } else if nearer[i].is_empty() {
                    vec![polygons[i].clone()]
                } else {
                    let clips: Vec<_> = nearer[i]
                        .iter()
                        .map(|&(j, _)| polygons[j].clone())
                        .collect();
                    overlay.clear();
                    // These safe builder methods require contours inside the
                    // adapter bounds, which contain every piece and every cut.
                    overlay = overlay
                        .unsafe_add_source(&polygons[i], ShapeType::Subject)
                        .unsafe_add_source(&clips, ShapeType::Clip);
                    overlay
                        .overlay(OverlayRule::Difference, FillRule::NonZero)
                        .into_iter()
                        .flatten()
                        .collect()
                };
                for contour in regions {
                    if contour.len() < 3 || area(&contour).abs() <= AREA {
                        continue;
                    }
                    let (min, max) = contour.iter().fold(
                        (f64::INFINITY, f64::NEG_INFINITY),
                        |(min, max), &p| {
                            let l = piece.light.at(p);
                            (min.min(l), max.max(l))
                        },
                    );
                    let first = ((min.max(0.) * bands as f64).floor() as usize).min(bands - 1);
                    let last = ((max.max(0.) * bands as f64).floor() as usize).min(bands - 1);
                    for (band, target) in raw[usize::from(!piece.positive)]
                        .iter_mut()
                        .enumerate()
                        .take(last + 1)
                        .skip(first)
                    {
                        let mut cut = contour.clone();
                        if band > 0 {
                            let lower = band as f64 / bands as f64;
                            cut = half_plane(&cut, |p| lower - piece.light.at(p));
                        }
                        if band + 1 < bands {
                            let upper = (band + 1) as f64 / bands as f64;
                            cut = half_plane(&cut, |p| piece.light.at(p) - upper);
                        }
                        if cut.len() >= 3 && area(&cut).abs() > AREA {
                            target.push(cut);
                        }
                    }
                }
                for e in 0..piece.vertices.len() {
                    let b = piece.vertices[e];
                    if !b.rim {
                        continue;
                    }
                    let a = piece.vertices[(e + piece.vertices.len() - 1) % piece.vertices.len()];
                    let a = [a.p[0] as f64, a.p[1] as f64];
                    let b = [b.p[0] as f64, b.p[1] as f64];
                    let bounds = [
                        a[0].min(b[0]) as f32,
                        a[1].min(b[1]) as f32,
                        a[0].max(b[0]) as f32,
                        a[1].max(b[1]) as f32,
                    ];
                    grid.candidates(bounds, &mut seen, stamp, &mut candidates);
                    stamp += 1;
                    let mut hidden = Vec::new();
                    for &j in &candidates {
                        if j == i || !boxes_overlap(bounds, pieces[j].bounds) {
                            continue;
                        }
                        if let Some((start, end)) = segment_interval(a, b, &polygons[j]) {
                            let t = (start + end) * 0.5;
                            let midpoint = [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])];
                            if in_front(j, i, midpoint) {
                                hidden.push((start, end));
                            }
                        }
                    }
                    hidden.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
                    let mut cursor: f64 = 0.;
                    for (start, end) in hidden {
                        if start > cursor {
                            segments.push((along(a, b, cursor), along(a, b, start)));
                        }
                        cursor = cursor.max(end);
                    }
                    if cursor < 1. {
                        segments.push((along(a, b, cursor), b));
                    }
                }
            }
            (raw, segments)
        });
        let mut raw = vec![vec![Vec::<Vec<[f64; 2]>>::new(); bands]; 2];
        let mut segments = Vec::new();
        for (mut regions, mut edges) in partial {
            for sign in 0..2 {
                for band in 0..bands {
                    raw[sign][band].append(&mut regions[sign][band]);
                }
            }
            segments.append(&mut edges);
        }
        let merged = batches(2, 1, |range| {
            let mut output = Vec::new();
            let mut overlay = FloatOverlay::<[f64; 2]>::with_adapter(adapter.clone(), 0);
            for sign in range {
                let raw = &raw[sign];
                let mut cumulative = Vec::<Vec<[f64; 2]>>::new();
                let mut layers = Vec::new();
                for band in (0..bands).rev() {
                    if raw[band].is_empty() {
                        continue;
                    }
                    cumulative.extend(raw[band].iter().cloned());
                    if cumulative.is_empty() {
                        continue;
                    }
                    overlay.clear();
                    overlay = overlay.unsafe_add_source(&cumulative, ShapeType::Subject);
                    cumulative = overlay
                        .overlay(OverlayRule::Subject, FillRule::NonZero)
                        .into_iter()
                        .flatten()
                        .filter(|c| area(c).abs() > AREA)
                        .collect();
                    let region: Vec<_> = cumulative
                        .iter()
                        .map(|c| simplify_closed(c, input.tolerance as f64))
                        .collect();
                    if region.is_empty() {
                        continue;
                    }
                    layers.push(Layer {
                        positive: sign == 0,
                        band,
                        region,
                    });
                }
                output.extend(layers.into_iter().rev());
            }
            output
        });
        result.layers = merged.into_iter().flatten().collect();
        result.rims = chain(segments, input.tolerance as f64);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        result.stats.millis = started.elapsed().as_secs_f32() * 1000.;
    }
    result
}

fn along(a: [f64; 2], b: [f64; 2], t: f64) -> [f64; 2] {
    [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])]
}

// Cyrus–Beck clipping of a segment to a counterclockwise convex contour.
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
        let distance = |v: [f64; 2]| (q[1] - p[1]) * (v[0] - p[0]) - (q[0] - p[0]) * (v[1] - p[1]);
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
    let original = || points.iter().map(|p| p.map(|x| x as f32)).collect();
    if points.len() < 3 || tolerance <= 0. {
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
            let key = ((p[0] * 1e4).round() as i64, (p[1] * 1e4).round() as i64);
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
