use super::super::{mesh::*, visible::*};

mod budget;
mod checks;
mod evidence;

type V3 = [f64; 3];

// Deliberately independent of the visibility implementation: the original
// G=1 oracle hits both sides of the actual 3D triangles. The G=3 oracle keeps
// front-facing hits in depth/index order, ahead of the analytic painted caps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hit {
    Surface(bool),
    Ball,
    Stick,
    Nothing,
}

#[derive(Clone, Copy)]
enum Molecule {
    Ball { center: V3, radius: f64 },
    Stick { a: V3, b: V3, half: f64 },
}

const PIERCING_BALL: Molecule = Molecule::Ball {
    center: [1.65, 0.1, 1.65],
    radius: 0.72,
};
const CROSSING_STICK: Molecule = Molecule::Stick {
    a: [-2.5, -0.15, 1.1],
    b: [2.5, 0.15, -1.1],
    half: 0.27,
};

#[derive(Clone, Copy)]
enum Paint {
    Ball {
        center: V3,
        radius: f64,
        depth: f64,
    },
    Stick {
        a: V3,
        b: V3,
        half: [f64; 2],
        depth: [f64; 2],
    },
}

struct Random(u64);

impl Random {
    fn unit(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / ((1_u64 << 53) as f64)
    }
}

fn add(a: V3, b: V3) -> V3 {
    std::array::from_fn(|i| a[i] + b[i])
}
fn sub(a: V3, b: V3) -> V3 {
    std::array::from_fn(|i| a[i] - b[i])
}
fn mul(a: V3, t: f64) -> V3 {
    a.map(|x| x * t)
}
fn dot(a: V3, b: V3) -> f64 {
    (0..3).map(|i| a[i] * b[i]).sum()
}
fn norm(a: V3) -> f64 {
    dot(a, a).sqrt()
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn f32s(p: V3) -> [f32; 3] {
    p.map(|v| v as f32)
}
fn f64s(p: [f32; 3]) -> V3 {
    p.map(f64::from)
}

fn rotations() -> Vec<[V3; 3]> {
    let mut result = vec![[[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]]];
    let mut random = Random(0xa54f_f53a_5f1d_36f1);
    for _ in 0..6 {
        let axis = std::array::from_fn(|_| 2. * random.unit() - 1.);
        let axis = mul(axis, 1. / norm(axis));
        let angle = random.unit() * std::f64::consts::TAU;
        let (s, c) = angle.sin_cos();
        let [x, y, z] = axis;
        result.push([
            [
                c + x * x * (1. - c),
                x * y * (1. - c) - z * s,
                x * z * (1. - c) + y * s,
            ],
            [
                y * x * (1. - c) + z * s,
                c + y * y * (1. - c),
                y * z * (1. - c) - x * s,
            ],
            [
                z * x * (1. - c) - y * s,
                z * y * (1. - c) + x * s,
                c + z * z * (1. - c),
            ],
        ]);
    }
    result
}

fn rotate(p: V3, rotation: [V3; 3]) -> V3 {
    rotation.map(|row| dot(row, p))
}

struct Camera {
    perspective: bool,
    c: f64,
    scale: f64,
}

impl Camera {
    fn project(&self, p: V3) -> V3 {
        let f = if self.perspective {
            self.c / (self.c - p[2])
        } else {
            1.
        };
        [
            self.scale * f * p[0],
            -self.scale * f * p[1],
            self.scale * f * p[2],
        ]
    }

    fn ray(&self, p: [f64; 2]) -> (V3, V3) {
        if self.perspective {
            (
                [0., 0., self.c],
                [p[0] / self.scale, -p[1] / self.scale, -self.c],
            )
        } else {
            (
                [p[0] / self.scale, -p[1] / self.scale, self.c],
                [0., 0., -1.],
            )
        }
    }
}

// A 2D bucket lookup only rejects rays outside a triangle's projected bounds;
// it never makes a depth or facing decision for the ray oracle.
struct Oracle<'a> {
    view: Vec<V3>,
    mesh: &'a Mesh,
    camera: Camera,
    paint: Vec<Paint>,
    bounds: [f64; 4],
    buckets: Vec<Vec<usize>>,
}

struct Frame {
    positions: Vec<[f32; 3]>,
    front: Vec<bool>,
    lightness: Vec<f32>,
    occluders: Vec<Occluder>,
}

impl Frame {
    fn input<'a>(&'a self, mesh: &'a Mesh, tones: &'a [usize]) -> Input<'a> {
        Input {
            positions: &self.positions,
            triangles: &mesh.triangles,
            positive: &mesh.positive,
            front: &self.front,
            lightness: &self.lightness,
            occluders: &self.occluders,
            tones,
            lines: &[],
            tolerance: 0.025,
        }
    }
}

fn prepare<'a>(
    mesh: &'a Mesh,
    molecule: &[Molecule],
    rotation: [V3; 3],
    perspective: bool,
) -> (Frame, Oracle<'a>) {
    let view: Vec<_> = mesh
        .positions
        .iter()
        .map(|&p| rotate(f64s(p), rotation))
        .collect();
    let mut radius = view.iter().copied().map(norm).fold(0., f64::max);
    for &item in molecule {
        radius = radius.max(match item {
            Molecule::Ball { center, radius } => norm(center) + radius,
            Molecule::Stick { a, b, half } => norm(a).max(norm(b)) + half,
        });
    }
    let camera = Camera {
        perspective,
        c: 3. * radius,
        scale: 60.,
    };
    let projected: Vec<_> = view.iter().map(|&p| camera.project(p)).collect();
    let front = mesh
        .triangles
        .iter()
        .map(|tri| {
            let [a, b, c] = tri.map(|i| view[i as usize]);
            let toward = if perspective {
                sub([0., 0., camera.c], a)
            } else {
                [0., 0., 1.]
            };
            dot(cross(sub(b, a), sub(c, a)), toward) > 0.
        })
        .collect();
    let paint: Vec<_> = molecule
        .iter()
        .map(|&item| match item {
            Molecule::Ball { center, radius } => {
                let center = rotate(center, rotation);
                let screen = camera.project(center);
                let f = if perspective {
                    camera.c / (camera.c - center[2])
                } else {
                    1.
                };
                Paint::Ball {
                    center: screen,
                    radius: radius * camera.scale * f,
                    depth: camera.project(add(center, [0., 0., radius]))[2] - screen[2],
                }
            }
            Molecule::Stick { a, b, half } => {
                let a = rotate(a, rotation);
                let b = rotate(b, rotation);
                let f = |z| {
                    if perspective {
                        camera.c / (camera.c - z)
                    } else {
                        1.
                    }
                };
                let depth =
                    |p: V3| camera.project(add(p, [0., 0., half]))[2] - camera.project(p)[2];
                Paint::Stick {
                    a: camera.project(a),
                    b: camera.project(b),
                    half: [half * camera.scale * f(a[2]), half * camera.scale * f(b[2])],
                    depth: [depth(a), depth(b)],
                }
            }
        })
        .collect();
    let occluders = paint
        .iter()
        .map(|&item| match item {
            Paint::Ball {
                center,
                radius,
                depth,
            } => Occluder::Ball {
                center: f32s(center),
                radius: radius as f32,
                depth: depth as f32,
            },
            Paint::Stick { a, b, half, depth } => Occluder::Stick {
                a: f32s(a),
                b: f32s(b),
                half: half.map(|x| x as f32),
                depth: depth.map(|x| x as f32),
            },
        })
        .collect();
    let mut bounds = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for p in &projected {
        for i in 0..2 {
            bounds[i] = bounds[i].min(p[i]);
            bounds[i + 2] = bounds[i + 2].max(p[i]);
        }
    }
    let padding = 0.08 * (bounds[2] - bounds[0]).max(bounds[3] - bounds[1]);
    for i in 0..2 {
        bounds[i] -= padding;
        bounds[i + 2] += padding;
    }
    let mut oracle = Oracle {
        view,
        mesh,
        camera,
        paint,
        bounds,
        buckets: vec![Vec::new(); 64 * 64],
    };
    for (index, triangle) in mesh.triangles.iter().enumerate() {
        let points = triangle.map(|i| projected[i as usize]);
        let low = [0, 1].map(|i| points.iter().map(|p| p[i]).fold(f64::INFINITY, f64::min));
        let high = [0, 1].map(|i| {
            points
                .iter()
                .map(|p| p[i])
                .fold(f64::NEG_INFINITY, f64::max)
        });
        let low = oracle.bucket(low);
        let high = oracle.bucket(high);
        for y in low[1]..=high[1] {
            for x in low[0]..=high[0] {
                oracle.buckets[y * 64 + x].push(index);
            }
        }
    }
    let frame = Frame {
        positions: projected.into_iter().map(f32s).collect(),
        front,
        lightness: vec![0.7; mesh.positions.len()],
        occluders,
    };
    (frame, oracle)
}

impl Oracle<'_> {
    fn bucket(&self, point: [f64; 2]) -> [usize; 2] {
        [0, 1].map(|i| {
            (((point[i] - self.bounds[i]) / (self.bounds[i + 2] - self.bounds[i]) * 64.).floor()
                as i64)
                .clamp(0, 63) as usize
        })
    }

    fn triangle_hit(&self, index: usize, point: [f64; 2]) -> Option<(f64, bool)> {
        let (origin, direction) = self.camera.ray(point);
        let [a, b, c] = self.mesh.triangles[index].map(|i| self.view[i as usize]);
        let ab = sub(b, a);
        let ac = sub(c, a);
        let h = cross(direction, ac);
        let determinant = dot(ab, h);
        if determinant.abs() < 1e-12 {
            return None;
        }
        let inverse = 1. / determinant;
        let s = sub(origin, a);
        let u = inverse * dot(s, h);
        if !(0. ..=1.).contains(&u) {
            return None;
        }
        let q = cross(s, ab);
        let v = inverse * dot(direction, q);
        if v < 0. || u + v > 1. {
            return None;
        }
        let t = inverse * dot(ac, q);
        (t > 0.).then(|| {
            (
                self.camera.project(add(origin, mul(direction, t)))[2],
                determinant > 0.,
            )
        })
    }

    fn triangle_depth(&self, index: usize, point: [f64; 2]) -> Option<f64> {
        self.triangle_hit(index, point).map(|(depth, _)| depth)
    }

    fn front_signs(&self, point: [f64; 2], depths: usize) -> Vec<bool> {
        let [x, y] = self.bucket(point);
        let molecule = self.molecule(point).map(|(depth, _)| depth);
        let mut hits: Vec<_> = self.buckets[y * 64 + x]
            .iter()
            .filter_map(|&index| {
                let (depth, front) = self.triangle_hit(index, point)?;
                (front && molecule.is_none_or(|molecule| depth > molecule)).then_some((
                    depth,
                    index,
                    self.mesh.positive[index],
                ))
            })
            .collect();
        hits.sort_by(|a, b| {
            // Exact depth ties have the lower triangle index in front.
            if a.0 == b.0 {
                a.1.cmp(&b.1)
            } else {
                b.0.total_cmp(&a.0)
            }
        });
        hits.into_iter().take(depths).map(|hit| hit.2).collect()
    }

    fn surface(&self, point: [f64; 2]) -> (Option<(f64, bool)>, u8) {
        let [x, y] = self.bucket(point);
        let mut nearest: Option<(f64, bool)> = None;
        let mut signs = 0;
        for &index in &self.buckets[y * 64 + x] {
            let Some(z) = self.triangle_depth(index, point) else {
                continue;
            };
            let positive = self.mesh.positive[index];
            signs |= if positive { 1 } else { 2 };
            if nearest.is_none_or(|(depth, _)| z > depth) {
                nearest = Some((z, positive));
            }
        }
        (nearest, signs)
    }

    fn molecule(&self, point: [f64; 2]) -> Option<(f64, Hit)> {
        self.paint
            .iter()
            .filter_map(|&item| match item {
                Paint::Ball {
                    center,
                    radius,
                    depth,
                } => {
                    let r2 = ((point[0] - center[0]).powi(2) + (point[1] - center[1]).powi(2))
                        / radius.powi(2);
                    (r2 <= 1.).then(|| (center[2] + depth * (1. - r2).sqrt(), Hit::Ball))
                }
                Paint::Stick { a, b, half, depth } => {
                    let axis = [b[0] - a[0], b[1] - a[1]];
                    let length2 = axis[0].powi(2) + axis[1].powi(2);
                    if length2 < 1e-16 {
                        return None;
                    }
                    let t = ((point[0] - a[0]) * axis[0] + (point[1] - a[1]) * axis[1]) / length2;
                    if !(0. ..=1.).contains(&t) {
                        return None;
                    }
                    let u = ((point[0] - a[0]) * axis[1] - (point[1] - a[1]) * axis[0]).abs()
                        / length2.sqrt();
                    let width = half[0] * (1. - t) + half[1] * t;
                    if u > width {
                        return None;
                    }
                    Some((
                        a[2] * (1. - t)
                            + b[2] * t
                            + (depth[0] * (1. - t) + depth[1] * t)
                                * (1. - (u / width).powi(2)).sqrt(),
                        Hit::Stick,
                    ))
                }
            })
            .max_by(|a, b| a.0.total_cmp(&b.0))
    }

    fn first(&self, point: [f64; 2]) -> Hit {
        match (self.surface(point).0, self.molecule(point)) {
            (Some((surface, sign)), Some((molecule, _))) if surface > molecule => {
                Hit::Surface(sign)
            }
            (_, Some((_, hit))) => hit,
            (Some((_, sign)), None) => Hit::Surface(sign),
            (None, None) => Hit::Nothing,
        }
    }
}

fn contains(region: &[Vec<[f32; 2]>], point: [f64; 2]) -> bool {
    let mut inside = false;
    for contour in region {
        for (a, b) in contour
            .iter()
            .zip(contour.iter().cycle().skip(1))
            .take(contour.len())
        {
            let a = a.map(f64::from);
            let b = b.map(f64::from);
            if (a[1] > point[1]) != (b[1] > point[1])
                && point[0] < (b[0] - a[0]) * (point[1] - a[1]) / (b[1] - a[1]) + a[0]
            {
                inside = !inside;
            }
        }
    }
    inside
}

fn distance_to_segment(point: [f64; 2], a: [f32; 2], b: [f32; 2]) -> f64 {
    let a = a.map(f64::from);
    let b = b.map(f64::from);
    let d = [b[0] - a[0], b[1] - a[1]];
    let length2 = d[0].powi(2) + d[1].powi(2);
    let t = if length2 == 0. {
        0.
    } else {
        (((point[0] - a[0]) * d[0] + (point[1] - a[1]) * d[1]) / length2).clamp(0., 1.)
    };
    ((point[0] - a[0] - t * d[0]).powi(2) + (point[1] - a[1] - t * d[1]).powi(2)).sqrt()
}

/// The layers as drawn: fitted with the drawing's error and sliver limits,
/// then flattened finely enough that flattening adds no visible error.
fn fitted(mut result: Visible) -> Visible {
    use super::super::draw::{FIT_ERROR, MIN_AREA, MIN_WIDTH};
    use super::super::fit;
    use crate::graphics::PathCommand;
    for layer in &mut result.layers {
        let mut contours: Vec<Vec<[f32; 2]>> = Vec::new();
        let mut at = [0.; 2];
        for command in fit::region(&layer.region, FIT_ERROR, MIN_AREA, MIN_WIDTH) {
            match command {
                PathCommand::Move(p) => {
                    at = [p.x, p.y];
                    contours.push(vec![at]);
                }
                PathCommand::Line(p) => {
                    at = [p.x, p.y];
                    contours.last_mut().unwrap().push(at);
                }
                PathCommand::Cubic(b, c, d) => {
                    let [a, b, c, d] = [at, [b.x, b.y], [c.x, c.y], [d.x, d.y]];
                    let contour = contours.last_mut().unwrap();
                    for i in 1..=32 {
                        let t = i as f32 / 32.;
                        let s = 1. - t;
                        contour.push(std::array::from_fn(|k| {
                            s * s * s * a[k]
                                + 3. * s * s * t * b[k]
                                + 3. * s * t * t * c[k]
                                + t * t * t * d[k]
                        }));
                    }
                    at = d;
                }
                PathCommand::Close => {}
            }
        }
        layer.region = contours;
    }
    result
}

fn first_layer_at_depth(result: &Visible, positive: bool, depth: usize) -> Option<&Layer> {
    result
        .layers
        .iter()
        .find(|layer| layer.depth == depth && layer.positive == positive)
}

fn first_layer(result: &Visible, positive: bool) -> Option<&Layer> {
    first_layer_at_depth(result, positive, 0)
}

#[derive(Default, Debug)]
struct Counts {
    checked: usize,
    skipped: usize,
    surface: usize,
    ball_hides: usize,
    stick_hides: usize,
    front_of_ball: usize,
    front_of_stick: usize,
    phases_overlap: usize,
}

const ORACLE_SAMPLES: usize = 3_000;

#[derive(Default, Debug)]
struct DepthCounts {
    checked: usize,
    skipped: usize,
    populated: [usize; 3],
}

fn classify_depth_samples(
    result: &Visible,
    oracle: &Oracle<'_>,
    seed: u64,
) -> (DepthCounts, Vec<[f64; 2]>) {
    assert_eq!(
        result.stats.fragment_limit_hits, 0,
        "oracle scene reached the fragment cap"
    );
    let mut random = Random(seed);
    let mut counts = DepthCounts::default();
    let mut disagreements = Vec::new();
    let delta =
        0.0075 * (oracle.bounds[2] - oracle.bounds[0]).max(oracle.bounds[3] - oracle.bounds[1]);
    for _ in 0..ORACLE_SAMPLES {
        let point = [0, 1]
            .map(|i| oracle.bounds[i] + random.unit() * (oracle.bounds[i + 2] - oracle.bounds[i]));
        let expected = oracle.front_signs(point, 3);
        // Compare the first G signs and their existence, including absent
        // entries. Deeper hits and triangle changes within one sign cannot
        // create an exemption around an otherwise stable first-G sequence.
        if [[delta, 0.], [-delta, 0.], [0., delta], [0., -delta]]
            .iter()
            .any(|offset| {
                oracle.front_signs([point[0] + offset[0], point[1] + offset[1]], 3) != expected
            })
        {
            counts.skipped += 1;
            continue;
        }
        counts.checked += 1;
        for (depth, populated) in counts.populated.iter_mut().enumerate() {
            *populated += usize::from(expected.get(depth).is_some());
        }
        if (0..3).any(|depth| {
            [false, true].into_iter().any(|positive| {
                let drawn = first_layer_at_depth(result, positive, depth)
                    .is_some_and(|layer| contains(&layer.region, point));
                drawn != (expected.get(depth) == Some(&positive))
            })
        }) {
            disagreements.push(point);
        }
    }
    (counts, disagreements)
}

fn classify_samples(result: &Visible, oracle: &Oracle<'_>, seed: u64) -> (Counts, Vec<[f64; 2]>) {
    assert_eq!(
        result.stats.fragment_limit_hits, 0,
        "oracle scene reached the fragment cap"
    );
    let mut random = Random(seed);
    let mut counts = Counts::default();
    let mut disagreements = Vec::new();
    let phases = [false, true];
    let layers = phases.map(|sign| first_layer(result, sign));
    let delta =
        0.0075 * (oracle.bounds[2] - oracle.bounds[0]).max(oracle.bounds[3] - oracle.bounds[1]);
    for _ in 0..ORACLE_SAMPLES {
        let point = [0, 1]
            .map(|i| oracle.bounds[i] + random.unit() * (oracle.bounds[i + 2] - oracle.bounds[i]));
        let first = oracle.first(point);
        // Only ground-truth transitions define the approximation band. A
        // returned contour must never excuse its own false hole or island.
        if [[delta, 0.], [-delta, 0.], [0., delta], [0., -delta]]
            .iter()
            .any(|offset| oracle.first([point[0] + offset[0], point[1] + offset[1]]) != first)
        {
            counts.skipped += 1;
            continue;
        }
        counts.checked += 1;
        let (surface, signs) = oracle.surface(point);
        counts.phases_overlap += usize::from(signs == 3);
        match first {
            Hit::Surface(_) => {
                counts.surface += 1;
                match oracle.molecule(point) {
                    Some((_, Hit::Ball)) => counts.front_of_ball += 1,
                    Some((_, Hit::Stick)) => counts.front_of_stick += 1,
                    _ => {}
                }
            }
            Hit::Ball => counts.ball_hides += usize::from(surface.is_some()),
            Hit::Stick => counts.stick_hides += usize::from(surface.is_some()),
            Hit::Nothing => {}
        }
        if phases.into_iter().zip(layers).any(|(sign, layer)| {
            let drawn = layer.is_some_and(|layer| contains(&layer.region, point));
            drawn != (first == Hit::Surface(sign))
        }) {
            disagreements.push(point);
        }
    }
    (counts, disagreements)
}

fn check_oracle(label: &str, result: &Visible, oracle: &Oracle<'_>, seed: u64) -> Counts {
    let (counts, disagreements) = classify_samples(result, oracle, seed);
    eprintln!(
        "surface oracle G=1 {label}: {counts:?}, disagreements={}",
        disagreements.len()
    );
    assert!(
        disagreements.is_empty(),
        "{label}: {} disagreements, first at {:?}",
        disagreements.len(),
        disagreements.first()
    );
    assert!(
        counts.skipped * 100 < ORACLE_SAMPLES * 15,
        "{label}: approximation band skipped >=15%: {counts:?}"
    );
    counts
}

#[test]
fn ray_oracle_rejects_a_false_hole_inside_a_uniformly_visible_square() {
    let square = vec![[10., 10.], [90., 10.], [90., 90.], [10., 90.]];
    let mesh = Mesh {
        positions: square.iter().map(|&[x, y]| [x, -y, 0.]).collect(),
        normals: vec![[0., 0., 1.]; 4],
        triangles: vec![[0, 2, 1], [0, 3, 2]],
        positive: vec![true; 2],
        truncated: false,
    };
    let oracle = Oracle {
        view: mesh.positions.iter().copied().map(f64s).collect(),
        mesh: &mesh,
        camera: Camera {
            perspective: false,
            c: 100.,
            scale: 1.,
        },
        paint: Vec::new(),
        bounds: [0., 0., 100., 100.],
        buckets: vec![vec![0, 1]; 64 * 64],
    };
    let mut result = Visible {
        layers: vec![Layer {
            positive: true,
            depth: 0,
            tone: 0,
            region: vec![square],
        }],
        ..Visible::default()
    };
    let seed = 0xfa15_e401;
    let (counts, disagreements) = classify_samples(&result, &oracle, seed);
    assert!(disagreements.is_empty(), "the true square must agree");
    assert!(counts.surface > 1_500);

    // The entire false hole is narrower than 2δ (δ = 0.75 here). An
    // exemption near returned contours would conceal every missing point.
    result.layers[0]
        .region
        .push(vec![[49.5, 20.], [50.5, 20.], [50.5, 80.], [49.5, 80.]]);
    let (counts, disagreements) = classify_samples(&result, &oracle, seed);
    eprintln!(
        "surface oracle false-hole self-test: {counts:?}, {} disagreements",
        disagreements.len()
    );
    assert!(
        !disagreements.is_empty(),
        "the oracle must reject a false hole even beside returned contours"
    );
    assert!(
        disagreements
            .iter()
            .all(|p| { (49.5..50.5).contains(&p[0]) && (20.0..80.0).contains(&p[1]) })
    );
}

#[test]
fn depth_ray_oracle_orders_front_faces_and_filters_the_painted_molecule() {
    let mut mesh = Mesh::default();
    for (index, depth) in [2., 2., 3., 1.].into_iter().enumerate() {
        let base = mesh.positions.len() as u32;
        mesh.positions
            .extend([[0., 0., depth], [10., 0., depth], [0., -10., depth]]);
        // The nearest two-sided hit is a back face, which G=3 must exclude.
        mesh.triangles.push(if index == 2 {
            [base, base + 1, base + 2]
        } else {
            [base, base + 2, base + 1]
        });
        mesh.positive.push(index == 0);
    }
    let mut oracle = Oracle {
        view: mesh.positions.iter().copied().map(f64s).collect(),
        mesh: &mesh,
        camera: Camera {
            perspective: false,
            c: 100.,
            scale: 1.,
        },
        paint: Vec::new(),
        bounds: [0., 0., 10., 10.],
        buckets: vec![vec![3, 2, 1, 0]; 64 * 64],
    };
    let point = [2., 2.];
    assert_eq!(oracle.surface(point).0, Some((3., false)));
    assert_eq!(oracle.front_signs(point, 3), [true, false, false]);
    oracle.paint.push(Paint::Ball {
        center: [2., 2., 1.],
        radius: 1.,
        depth: 0.,
    });
    // The rear sheet ties the paint depth and is therefore already hidden.
    assert_eq!(oracle.front_signs(point, 3), [true, false]);
}

#[test]
fn depth_ray_oracle_rejects_a_false_hole_in_a_rear_layer() {
    let square = vec![[10., 10.], [90., 10.], [90., 90.], [10., 90.]];
    let mut mesh = Mesh::default();
    let mut result = Visible::default();
    for (depth, positive) in [true, false, true].into_iter().enumerate() {
        let base = mesh.positions.len() as u32;
        mesh.positions
            .extend(square.iter().map(|&[x, y]| [x, -y, 3. - depth as f32]));
        mesh.triangles
            .extend([[base, base + 2, base + 1], [base, base + 3, base + 2]]);
        mesh.positive.extend([positive; 2]);
        result.layers.push(Layer {
            positive,
            depth,
            tone: 0,
            region: vec![square.clone()],
        });
    }
    let oracle = Oracle {
        view: mesh.positions.iter().copied().map(f64s).collect(),
        mesh: &mesh,
        camera: Camera {
            perspective: false,
            c: 100.,
            scale: 1.,
        },
        paint: Vec::new(),
        bounds: [0., 0., 100., 100.],
        buckets: vec![(0..6).collect(); 64 * 64],
    };
    let seed = 0xfa15_e401;
    let (counts, disagreements) = classify_depth_samples(&result, &oracle, seed);
    assert!(
        disagreements.is_empty(),
        "all three true squares must agree"
    );
    assert!(counts.populated.into_iter().all(|count| count > 1_500));
    result.layers[1]
        .region
        .push(vec![[49.5, 20.], [50.5, 20.], [50.5, 80.], [49.5, 80.]]);
    let (_, disagreements) = classify_depth_samples(&result, &oracle, seed);
    assert!(
        !disagreements.is_empty(),
        "the oracle must inspect a rear layer even when the nearest is correct"
    );
    assert!(
        disagreements
            .iter()
            .all(|p| (49.5..50.5).contains(&p[0]) && (20.0..80.0).contains(&p[1]))
    );
}

fn p_mesh() -> Mesh {
    let mesh = isosurface(
        |p| p[2] * (-p.iter().map(|x| x * x).sum::<f32>().sqrt()).exp(),
        Grid {
            origin: [-4.5; 3],
            step: 0.3,
            size: [31; 3],
        },
        0.13,
    );
    assert!(!mesh.truncated);
    simplify(&mesh, 2_400)
}

#[test]
fn analytic_rays_agree_with_layers_through_balls_sticks_and_overlapping_lobes() {
    let mesh = p_mesh();
    let molecule = [PIERCING_BALL, CROSSING_STICK];
    let mut hidden_by_ball = 0;
    let mut hidden_by_stick = 0;
    let mut front_of_ball = 0;
    let mut front_of_stick = 0;
    let mut overlap = 0;
    let mut checked = 0;
    let mut skipped = 0;
    for perspective in [false, true] {
        for (index, rotation) in rotations().into_iter().enumerate() {
            for (name, items) in [("pierced", molecule.as_slice()), ("overlap", &[][..])] {
                let (frame, oracle) = prepare(&mesh, items, rotation, perspective);
                let output = visible(&frame.input(&mesh, &[1]));
                let counts = check_oracle(
                    &format!("{name}, perspective={perspective}, rotation={index}"),
                    &output,
                    &oracle,
                    0x7ab3_91a5 + index as u64,
                );
                // The fitted, drawn outlines must agree too.
                let fitted = check_oracle(
                    &format!("fitted {name}, perspective={perspective}, rotation={index}"),
                    &fitted(output),
                    &oracle,
                    0x5eed_f17d + index as u64,
                );
                checked += fitted.checked;
                skipped += fitted.skipped;
                hidden_by_ball += counts.ball_hides;
                hidden_by_stick += counts.stick_hides;
                front_of_ball += counts.front_of_ball;
                front_of_stick += counts.front_of_stick;
                overlap += counts.phases_overlap;
                checked += counts.checked;
                skipped += counts.skipped;
            }
        }
    }
    eprintln!(
        "surface oracle total G=1, polygons and fitted paths: checked={checked}, skipped={skipped}, disagreements=0"
    );
    assert!(hidden_by_ball > 30, "ball must hide mesh: {hidden_by_ball}");
    assert!(
        hidden_by_stick > 30,
        "stick must hide mesh: {hidden_by_stick}"
    );
    assert!(
        front_of_ball > 30,
        "mesh must also be in front of the ball footprint: {front_of_ball}"
    );
    assert!(
        front_of_stick > 30,
        "mesh must also be in front of the stick footprint: {front_of_stick}"
    );
    assert!(
        overlap > 100,
        "both phases must overlap along rays: {overlap}"
    );
}

#[test]
fn front_facing_rays_agree_with_three_depths_through_balls_sticks_and_overlapping_lobes() {
    let mesh = p_mesh();
    let molecule = [PIERCING_BALL, CROSSING_STICK];
    let mut total = DepthCounts::default();
    let mut total_disagreements = 0;
    let mut failures = Vec::new();
    for perspective in [false, true] {
        for (index, rotation) in rotations().into_iter().enumerate() {
            for (name, items) in [("pierced", molecule.as_slice()), ("overlap", &[][..])] {
                let (frame, oracle) = prepare(&mesh, items, rotation, perspective);
                let output = visible(&frame.input(&mesh, &[1; 3]));
                let (counts, disagreements) =
                    classify_depth_samples(&output, &oracle, 0x7ab3_91a5 + index as u64);
                let label = format!("{name}, perspective={perspective}, rotation={index}");
                eprintln!(
                    "surface oracle G=3 {label}: {counts:?}, disagreements={}",
                    disagreements.len()
                );
                if !disagreements.is_empty() {
                    failures.push(format!(
                        "{label}: {} disagreements, first at {:?}",
                        disagreements.len(),
                        disagreements.first()
                    ));
                }
                total.checked += counts.checked;
                total.skipped += counts.skipped;
                for (depth, populated) in total.populated.iter_mut().enumerate() {
                    *populated += counts.populated[depth];
                }
                total_disagreements += disagreements.len();
                assert!(
                    counts.skipped * 100 < ORACLE_SAMPLES * 15,
                    "{label}: approximation band skipped >=15%: {counts:?}"
                );
            }
        }
    }
    eprintln!(
        "surface oracle total G=3: checked={}, skipped={}, disagreements={total_disagreements}, populated={:?}",
        total.checked, total.skipped, total.populated
    );
    assert_eq!(total_disagreements, 0, "{}", failures.join("\n"));
    assert!(
        total.populated[1] > 100,
        "the oracle must compare populated rear depths: {total:?}"
    );
}

fn enclosed_lobe() -> (Mesh, [Molecule; 1]) {
    let radius = 0.9_f32;
    let mesh = isosurface(
        |p| (-p.iter().map(|x| x * x).sum::<f32>()).exp(),
        Grid {
            origin: [-1.4; 3],
            step: 0.1,
            size: [29; 3],
        },
        (-radius * radius).exp(),
    );
    let mesh = simplify(&mesh, 1_200);
    let molecule = [Molecule::Ball {
        center: [0.; 3],
        radius: f64::from(radius) * 1.12,
    }];
    (mesh, molecule)
}

#[test]
fn a_lobe_enclosed_by_a_slightly_larger_painted_ball_is_fully_hidden() {
    let (mesh, molecule) = enclosed_lobe();
    let mut hidden = 0;
    for perspective in [false, true] {
        for (index, rotation) in rotations().into_iter().enumerate() {
            let (frame, oracle) = prepare(&mesh, &molecule, rotation, perspective);
            let output = visible(&frame.input(&mesh, &[1]));
            assert!(
                output.layers.iter().all(|layer| layer.region.is_empty()),
                "enclosed lobe leaked: perspective={perspective}, rotation={index}"
            );
            assert!(
                output.rims.iter().all(Vec::is_empty),
                "enclosed lobe left a visible rim"
            );
            let counts = check_oracle(
                &format!("enclosed, perspective={perspective}, rotation={index}"),
                &output,
                &oracle,
                0xba11_5eed + index as u64,
            );
            assert_eq!(counts.surface, 0);
            hidden += counts.ball_hides;
        }
    }
    assert!(
        hidden > 1_000,
        "the enclosure test must cast rays through the lobe"
    );
}

#[test]
#[ignore = "writes SVG evidence to RESHIKI_SURFACE_EVIDENCE_DIR"]
fn occlusion_evidence_images() -> std::io::Result<()> {
    use std::{fmt::Write, fs, path::PathBuf};

    const BANDS: usize = 10;
    let dir = PathBuf::from(
        std::env::var_os("RESHIKI_SURFACE_EVIDENCE_DIR")
            .expect("set RESHIKI_SURFACE_EVIDENCE_DIR to the output directory"),
    );
    fs::create_dir_all(&dir)?;
    let mesh = p_mesh();
    let (enclosed_mesh, enclosing_ball) = enclosed_lobe();
    let scenes: [(_, _, &Mesh, &[Molecule], _); 4] = [
        (
            "ball",
            "A lobe piercing a ball",
            &mesh,
            &[PIERCING_BALL],
            [5, 6],
        ),
        (
            "stick",
            "A stick running through a lobe",
            &mesh,
            &[CROSSING_STICK],
            [3, 4],
        ),
        (
            "enclosed",
            "A lobe enclosed in a slightly larger ball",
            &enclosed_mesh,
            &enclosing_ball,
            [0, 2],
        ),
        (
            "overlap",
            "Two lobes overlapping on screen",
            &mesh,
            &[],
            [1, 6],
        ),
    ];
    let rotations = rotations();
    let light = [-0.38, 0.42, 0.82];
    let light = mul(light, 1. / norm(light));
    let mut readme = String::new();
    let mut total_disagreements = 0;
    for (name, description, mesh, molecule, views) in scenes {
        for (view, index) in (1..).zip(views) {
            let rotation = rotations[index];
            let (mut frame, oracle) = prepare(mesh, molecule, rotation, true);
            frame.lightness = mesh
                .normals
                .iter()
                .map(|&n| (0.35 + 0.65 * dot(rotate(f64s(n), rotation), light).max(0.)) as f32)
                .collect();
            let output = visible(&frame.input(mesh, &[BANDS]));
            let (counts, disagreements) =
                classify_samples(&output, &oracle, 0x7ab3_91a5 + index as u64);
            let file = format!("occlusion-{name}-{view}.svg");
            fs::write(
                dir.join(&file),
                evidence::svg(&oracle, &output, &[BANDS], &counts, &disagreements),
            )?;
            writeln!(
                readme,
                "{file} — {description}, perspective rotation {index}."
            )
            .unwrap();
            eprintln!("{file}: {counts:?}, {} disagreements", disagreements.len());
            total_disagreements += disagreements.len();
        }
    }
    fs::write(dir.join("README.txt"), readme)?;
    assert_eq!(
        total_disagreements,
        0,
        "see marked points in {}",
        dir.display()
    );
    Ok(())
}

#[test]
fn cap_facets_cannot_leak_either_phase_just_inside_the_analytic_ball() {
    // Halfway between cap rings and outline azimuths, the chord of an
    // inscribed proxy would lie below the true cap by much more than 0.02.
    // Tiny patches only 0.02 units inside it therefore catch that regression.
    let radius = 100_f64;
    let theta = 6.5 * std::f64::consts::FRAC_PI_2 / 12.;
    let low = 6. * std::f64::consts::FRAC_PI_2 / 12.;
    let high = 7. * std::f64::consts::FRAC_PI_2 / 12.;
    let azimuth_half = std::f64::consts::PI / 64.;
    let t = (theta.sin() - low.sin() * azimuth_half.cos())
        / ((high.sin() - low.sin()) * azimuth_half.cos());
    let inscribed_depth = radius * ((1. - t) * low.cos() + t * high.cos());
    let mut mesh = Mesh::default();
    for i in 0..2 {
        let phi = (i as f64 * 32. + 0.5) * std::f64::consts::TAU / 64.;
        let center = [
            radius * theta.sin() * phi.cos(),
            radius * theta.sin() * phi.sin(),
            radius * theta.cos() - 0.02,
        ];
        assert!(
            center[2] > inscribed_depth + 0.1,
            "patch must lie beyond the inscribed facet"
        );
        for [x, y] in [[-0.005, -0.005], [0.005, -0.005], [0., 0.005]] {
            let point = add(center, [x, y, 0.]);
            assert!(
                norm(point) < radius,
                "test patch must be inside the analytic ball"
            );
            mesh.positions.push(f32s(point));
            mesh.normals.push([0., 0., 1.]);
        }
        mesh.triangles.push([3 * i, 3 * i + 2, 3 * i + 1]);
        mesh.positive.push(i == 0);
    }
    let occluders = [Occluder::Ball {
        center: [0.; 3],
        radius: radius as f32,
        depth: radius as f32,
    }];
    let input = Input {
        positions: &mesh.positions,
        triangles: &mesh.triangles,
        positive: &mesh.positive,
        front: &[true, true],
        lightness: &[0.7; 6],
        occluders: &occluders,
        tones: &[1],
        lines: &[],
        tolerance: 0.00001,
    };
    let result = visible(&input);
    assert!(
        result.layers.iter().all(|layer| layer.region.is_empty()),
        "an inscribed cap leaked a near-contact phase patch"
    );
}

#[test]
fn cap_tangent_planes_clip_a_straddling_triangle() {
    // At z = 80, x² + y² + z² = 100² meets y = 0 at x = 60. The
    // triangle crosses that boundary, so whole-triangle containment cannot
    // bypass the cap planes. The witnesses lie on opposite sides of the cap.
    let result = visible(&Input {
        positions: &[[59., -1., 80.], [61., -1., 80.], [60., 1., 80.]],
        triangles: &[[0, 1, 2]],
        positive: &[true],
        front: &[true],
        lightness: &[0.7; 3],
        occluders: &[Occluder::Ball {
            center: [0.; 3],
            radius: 100.,
            depth: 100.,
        }],
        tones: &[1],
        lines: &[],
        tolerance: 0.00001,
    });
    for (point, expected) in [([59.98_f64, 0.], false), ([60.5, -0.4], true)] {
        assert_eq!(
            point[0].powi(2) + point[1].powi(2) + 80_f64.powi(2) > 100_f64.powi(2),
            expected,
            "analytic sphere witness"
        );
        let drawn = first_layer(&result, true).is_some_and(|layer| contains(&layer.region, point));
        assert_eq!(drawn, expected, "cap clipping witness at {point:?}");
    }
}

#[test]
fn cyclic_long_thin_triangles_are_visible_without_a_global_depth_order() {
    let mut mesh = Mesh {
        positions: Vec::new(),
        normals: Vec::new(),
        triangles: Vec::new(),
        positive: Vec::new(),
        truncated: false,
    };
    for i in 0..3 {
        let angle = i as f64 * std::f64::consts::TAU / 3.;
        let (s, c) = angle.sin_cos();
        for [x, y] in [[-3., -1.25], [3., -1.25], [0., -0.75]] {
            // Three tilted triangle strips form A>B, B>C, C>A at the corners.
            mesh.positions
                .push(f32s([c * x - s * y, -(s * x + c * y), 0.4 * x]));
            mesh.normals.push([0., 0., 1.]);
        }
        mesh.triangles.push([3 * i, 3 * i + 2, 3 * i + 1]);
        mesh.positive.push(i != 1);
    }
    for positive_triangle in 0..3 {
        mesh.positive.fill(false);
        mesh.positive[positive_triangle] = true;
        let (frame, oracle) = prepare(&mesh, &[], rotations()[0], false);
        assert!(frame.front.iter().all(|front| *front));
        let result = visible(&frame.input(&mesh, &[1]));
        assert!(
            result.stats.overlaps >= 3,
            "cyclic scene must exercise all three projected overlaps"
        );
        for front in 0..3 {
            let angle = front as f64 * std::f64::consts::TAU / 3.;
            let (s, c) = angle.sin_cos();
            let x = 13. * 3_f64.sqrt() / 12.;
            let y = -13. / 12.;
            let point = [60. * (c * x - s * y), 60. * (s * x + c * y)];
            let back = (front + 1) % 3;
            let front_z = oracle
                .triangle_depth(front, point)
                .expect("front witness triangle");
            let back_z = oracle
                .triangle_depth(back, point)
                .expect("back witness triangle");
            assert!(
                front_z > back_z + 1.,
                "fixture must establish A>B, B>C, C>A"
            );
            assert!(oracle.triangle_depth((front + 2) % 3, point).is_none());
            for sign in [false, true] {
                let drawn =
                    first_layer(&result, sign).is_some_and(|layer| contains(&layer.region, point));
                assert_eq!(
                    drawn,
                    (front == positive_triangle) == sign,
                    "direct cyclic witness: front={front}, positive_triangle={positive_triangle}, point={point:?}"
                );
            }
        }
        let counts = check_oracle(
            &format!("cyclic triangles, positive={positive_triangle}"),
            &result,
            &oracle,
            0x00c1_c1e5,
        );
        assert!(counts.surface > 100);
    }
}
